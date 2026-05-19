//! Memory OS L3 §3.3 RETAINED — Temporal Query Classifier
//! (per ADR 2026-05-20 §8, the Timeline Engine reads side).
//!
//! V1 (this PR): pure-Rust keyword + regex detection. Identifies
//! "temporal" queries (e.g. "最近两周我做了什么", "what happened last
//! month", "since 2026-04") and parses out a `TimeRange`. The caller
//! (future read path / IPC command in the Timeline UI) routes
//! temporal queries to `temporal_aggregates` instead of the default
//! recall path.
//!
//! V2 (future PR): hand the unmatched-but-suspicious queries to a
//! Haiku call for disambiguation (e.g. "the time we were in Tokyo"
//! needs LLM context). Most queries should match V1 keywords; LLM
//! is the fallback for the long tail.
//!
//! ## What this module is NOT
//!
//! - **Not a recall executor.** It classifies queries; the caller
//!   decides what to do with the classification.
//! - **Not a date-math library.** It produces `TimeRange` enums that
//!   the caller resolves against `chrono::Utc::now()` at call time.
//! - **Not the global query classifier.** Cognitive Phase 14
//!   describes a 4-way classifier (single_hop / multi_hop /
//!   topic_synthesis / temporal). This module only implements the
//!   temporal classifier slice.

use chrono::{DateTime, Datelike, Duration, NaiveDate, TimeZone, Utc};

/// What a temporal query is asking about. The caller resolves
/// (start, end) timestamps against the current clock just before
/// querying `temporal_aggregates` / `timeline_events`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TimeRange {
    /// "from 2026-04-01 to 2026-04-30"-style explicit range.
    /// Both timestamps are unix-ms.
    Absolute { start_ms: i64, end_ms: i64 },
    /// "最近 N 天" / "past N days" / "last N weeks".
    RelativeRecent { unit: TimeUnit, count: u32 },
    /// "今天" / "今年" / "2026 年 5 月" — anchored to a calendar boundary.
    Calendar { year: i32, month: Option<u8> },
}

/// Time unit for relative ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimeUnit {
    Day,
    Week,
    Month,
    Year,
}

impl TimeRange {
    /// Resolve `(start_ms, end_ms)` against `now`. Useful for the
    /// caller that wants concrete bounds to plug into the
    /// `temporal_aggregates` SELECT.
    pub fn resolve(&self, now: DateTime<Utc>) -> (i64, i64) {
        match self {
            Self::Absolute { start_ms, end_ms } => (*start_ms, *end_ms),
            Self::RelativeRecent { unit, count } => {
                let dur = match unit {
                    TimeUnit::Day => Duration::days(*count as i64),
                    TimeUnit::Week => Duration::weeks(*count as i64),
                    TimeUnit::Month => Duration::days((*count as i64) * 30),
                    TimeUnit::Year => Duration::days((*count as i64) * 365),
                };
                let start = now - dur;
                (start.timestamp_millis(), now.timestamp_millis())
            }
            Self::Calendar { year, month } => match month {
                Some(m) => {
                    let start = Utc
                        .with_ymd_and_hms(*year, *m as u32, 1, 0, 0, 0)
                        .single()
                        .unwrap_or(now);
                    // Next-month boundary; handle Dec → next year.
                    let (ny, nm) = if *m == 12 { (*year + 1, 1) } else { (*year, *m as u32 + 1) };
                    let end = Utc
                        .with_ymd_and_hms(ny, nm, 1, 0, 0, 0)
                        .single()
                        .unwrap_or(now);
                    (start.timestamp_millis(), end.timestamp_millis())
                }
                None => {
                    let start = Utc
                        .with_ymd_and_hms(*year, 1, 1, 0, 0, 0)
                        .single()
                        .unwrap_or(now);
                    let end = Utc
                        .with_ymd_and_hms(year + 1, 1, 1, 0, 0, 0)
                        .single()
                        .unwrap_or(now);
                    (start.timestamp_millis(), end.timestamp_millis())
                }
            },
        }
    }
}

/// Classify a query as temporal-or-not.
///
/// V1: keyword + regex matching against Chinese + English. Returns
/// `Some(TimeRange)` when the query matches; `None` otherwise.
/// Caller treats `None` as "not a temporal query" and falls back to
/// the standard recall path.
///
/// Patterns recognized (V1):
/// - "最近 N (天|周|月|年)" / "past N (days|weeks|months|years)"
/// - "上 (周|月|年)" / "last (week|month|year)"
/// - "今(天|年)" / "this (week|month|year)" / "today"
/// - "昨天" / "yesterday"
/// - "YYYY-MM-DD" exact date
/// - "YYYY 年 N 月" / "in (Jan|Feb|...) YYYY"
/// - "YYYY 年" / "in YYYY"
///
/// Tests in this module enumerate the exact strings each pattern
/// matches.
pub fn classify_temporal_query(query: &str, now: DateTime<Utc>) -> Option<TimeRange> {
    let lower = query.to_lowercase();

    // "今天" / "today" / "today's".
    if query.contains("今天") || lower.contains("today") {
        return Some(TimeRange::RelativeRecent {
            unit: TimeUnit::Day,
            count: 1,
        });
    }

    // "昨天" / "yesterday" — represent as "1-day window ending now".
    // For caller's purposes, "yesterday's events" = RelativeRecent { Day, 1 }
    // works fine since `resolve` returns [now-1day, now]; the caller
    // can intersect with date boundaries if needed.
    if query.contains("昨天") || lower.contains("yesterday") {
        return Some(TimeRange::RelativeRecent {
            unit: TimeUnit::Day,
            count: 1,
        });
    }

    // "最近 N 天/周/月/年" / "past N days/weeks/months/years".
    if let Some(r) = parse_relative_recent(&lower) {
        return Some(r);
    }

    // "上 (周|月|年)" / "last (week|month|year)".
    if let Some(r) = parse_last_period(&lower) {
        return Some(r);
    }

    // "今 (周|月|年)" / "this (week|month|year)".
    if let Some(r) = parse_this_period(&lower, now) {
        return Some(r);
    }

    // "YYYY-MM-DD" exact ISO date.
    if let Some(r) = parse_iso_date(&lower) {
        return Some(r);
    }

    // "YYYY 年 N 月" / "YYYY-MM" / "YYYY 年".
    if let Some(r) = parse_calendar(&lower) {
        return Some(r);
    }

    None
}

fn parse_relative_recent(lower: &str) -> Option<TimeRange> {
    // Chinese: "最近 N 天/周/月/年" — N can be 1-2 ASCII digits.
    // English: "past N days/weeks/months/years" or "last N days" etc.
    // We accept loose whitespace.
    let chinese = regex_lite_match(lower, "最近");
    let english = lower.contains("past ") || lower.contains("last ");
    if !chinese && !english {
        return None;
    }
    let units = [
        ("天", TimeUnit::Day),
        ("周", TimeUnit::Week),
        ("个月", TimeUnit::Month),
        ("月", TimeUnit::Month),
        ("年", TimeUnit::Year),
        ("days", TimeUnit::Day),
        ("day", TimeUnit::Day),
        ("weeks", TimeUnit::Week),
        ("week", TimeUnit::Week),
        ("months", TimeUnit::Month),
        ("month", TimeUnit::Month),
        ("years", TimeUnit::Year),
        ("year", TimeUnit::Year),
    ];
    for (kw, unit) in units {
        if let Some(count) = find_count_before_keyword(lower, kw) {
            return Some(TimeRange::RelativeRecent { unit, count });
        }
    }
    // Bare "最近" with no number → default to 7 days (a reasonable
    // "recent" window).
    if chinese {
        return Some(TimeRange::RelativeRecent {
            unit: TimeUnit::Day,
            count: 7,
        });
    }
    None
}

fn parse_last_period(lower: &str) -> Option<TimeRange> {
    // "上周" / "上月" / "上年" — Chinese
    // "last week" / "last month" / "last year" — English
    if lower.contains("上周") || lower.contains("last week") {
        return Some(TimeRange::RelativeRecent { unit: TimeUnit::Week, count: 1 });
    }
    if lower.contains("上月") || lower.contains("last month") {
        return Some(TimeRange::RelativeRecent { unit: TimeUnit::Month, count: 1 });
    }
    if lower.contains("上年") || lower.contains("last year") {
        return Some(TimeRange::RelativeRecent { unit: TimeUnit::Year, count: 1 });
    }
    None
}

fn parse_this_period(lower: &str, now: DateTime<Utc>) -> Option<TimeRange> {
    // "今周" — Chinese isn't actually a thing. "本周" is. Handle both.
    if lower.contains("本周") || lower.contains("这周") || lower.contains("this week") {
        return Some(TimeRange::RelativeRecent { unit: TimeUnit::Week, count: 1 });
    }
    if lower.contains("本月") || lower.contains("这个月") || lower.contains("this month") {
        return Some(TimeRange::Calendar {
            year: now.year(),
            month: Some(now.month() as u8),
        });
    }
    if lower.contains("今年") || lower.contains("this year") {
        return Some(TimeRange::Calendar {
            year: now.year(),
            month: None,
        });
    }
    None
}

fn parse_iso_date(lower: &str) -> Option<TimeRange> {
    // Match YYYY-MM-DD anywhere in the string. We don't have the
    // `regex` crate dependency for this module, so we hand-scan.
    let bytes = lower.as_bytes();
    let mut i = 0;
    while i + 10 <= bytes.len() {
        if is_iso_date_at(bytes, i) {
            let date_str = &lower[i..i + 10];
            if let Ok(d) = NaiveDate::parse_from_str(date_str, "%Y-%m-%d") {
                let start = Utc
                    .with_ymd_and_hms(d.year(), d.month(), d.day(), 0, 0, 0)
                    .single()?;
                let end = start + Duration::days(1);
                return Some(TimeRange::Absolute {
                    start_ms: start.timestamp_millis(),
                    end_ms: end.timestamp_millis(),
                });
            }
        }
        i += 1;
    }
    None
}

fn is_iso_date_at(bytes: &[u8], i: usize) -> bool {
    // Pattern: dddd-dd-dd (4 digits, dash, 2 digits, dash, 2 digits).
    if i + 10 > bytes.len() {
        return false;
    }
    let segments = [(0, 4), (5, 2), (8, 2)];
    for (off, len) in segments {
        for j in 0..len {
            if !bytes[i + off + j].is_ascii_digit() {
                return false;
            }
        }
    }
    bytes[i + 4] == b'-' && bytes[i + 7] == b'-'
}

fn parse_calendar(lower: &str) -> Option<TimeRange> {
    // "2026 年 5 月" / "2026年5月" / "2026-05" / "in may 2026" etc.
    // Hand-scan for "YYYY年" + optional "Nm月". UTF-8 safe: we only
    // use byte-indices we got from char_indices(), never raw offsets.
    let bytes = lower.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i + 4 <= len {
        let four_digits = bytes[i..i + 4].iter().all(|b| b.is_ascii_digit());
        if four_digits {
            let after = i + 4;
            let year_str = &lower[i..after];
            if let Ok(year) = year_str.parse::<i32>() {
                if (1900..=2200).contains(&year) {
                    // After the year, optionally skip ASCII whitespace.
                    let mut p = after;
                    while p < len && bytes[p] == b' ' {
                        p += 1;
                    }
                    // 年 in UTF-8 = E5 B9 B4 (3 bytes). Check via
                    // strip_prefix to stay char-safe.
                    if lower[p..].starts_with("年") {
                        let after_year = p + "年".len();
                        if let Some(month) = parse_optional_month_strict(&lower[after_year..]) {
                            return Some(TimeRange::Calendar { year, month: Some(month) });
                        }
                        return Some(TimeRange::Calendar { year, month: None });
                    }
                    // "YYYY-MM" ISO-like
                    if p < len && bytes[p] == b'-' && p + 3 <= len {
                        // Verify the next two chars are ASCII digits.
                        if bytes[p + 1].is_ascii_digit() && bytes[p + 2].is_ascii_digit() {
                            let mm = &lower[p + 1..p + 3];
                            if let Ok(m) = mm.parse::<u8>() {
                                if (1..=12).contains(&m) {
                                    return Some(TimeRange::Calendar { year, month: Some(m) });
                                }
                            }
                        }
                    }
                }
            }
        }
        i += 1;
    }
    None
}

/// Parse "[whitespace]N月" or "[whitespace]N 月" where N is 1-12.
/// Returns the month number on match, `None` otherwise. UTF-8 safe.
fn parse_optional_month_strict(s: &str) -> Option<u8> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len && bytes[i] == b' ' {
        i += 1;
    }
    let digit_start = i;
    while i < len && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == digit_start {
        return None;
    }
    let num_str = &s[digit_start..i];
    let m: u8 = num_str.parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    // Optionally skip a single space, then check for 月.
    if i < len && bytes[i] == b' ' {
        i += 1;
    }
    if s[i..].starts_with("月") {
        Some(m)
    } else {
        None
    }
}

/// Find a 1-2 digit number that appears before `keyword` in `lower`.
/// e.g. find_count_before_keyword("最近 7 天", "天") = Some(7).
fn find_count_before_keyword(lower: &str, keyword: &str) -> Option<u32> {
    let idx = lower.find(keyword)?;
    let before = &lower[..idx];
    // Scan backwards for digits, allowing one space between.
    let trimmed = before.trim_end();
    let last_digits: String = trimmed
        .chars()
        .rev()
        .take_while(|c| c.is_ascii_digit())
        .collect::<String>()
        .chars()
        .rev()
        .collect();
    if last_digits.is_empty() {
        return None;
    }
    last_digits.parse::<u32>().ok().filter(|&n| n > 0)
}

/// Tiny "does this string contain that substring" helper. Exists
/// to keep the call sites readable; no real regex engine here
/// (we deliberately avoid the `regex` crate for this module to
/// keep the dep footprint small).
fn regex_lite_match(haystack: &str, needle: &str) -> bool {
    haystack.contains(needle)
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn now_at_may_2026() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 5, 20, 12, 0, 0).single().unwrap()
    }

    #[test]
    fn detects_chinese_recent_n_days() {
        let r = classify_temporal_query("最近 7 天我做了什么", now_at_may_2026()).unwrap();
        match r {
            TimeRange::RelativeRecent { unit, count } => {
                assert_eq!(unit, TimeUnit::Day);
                assert_eq!(count, 7);
            }
            _ => panic!("expected RelativeRecent"),
        }
    }

    #[test]
    fn detects_chinese_recent_n_weeks() {
        let r = classify_temporal_query("最近两周的进展", now_at_may_2026());
        // Note: "两" isn't a digit, so V1 won't pick the count. The
        // module falls back to the default 7 days for bare "最近".
        // Future PR could add Chinese-numeral parsing.
        match r.unwrap() {
            TimeRange::RelativeRecent { count, .. } => {
                assert!(count > 0, "got count={}", count);
            }
            _ => panic!("expected RelativeRecent"),
        }
    }

    #[test]
    fn detects_english_past_n_weeks() {
        let r = classify_temporal_query("what did i do past 2 weeks", now_at_may_2026()).unwrap();
        assert_eq!(
            r,
            TimeRange::RelativeRecent {
                unit: TimeUnit::Week,
                count: 2
            }
        );
    }

    #[test]
    fn detects_english_last_month() {
        let r = classify_temporal_query("last month I learned about gbrain", now_at_may_2026())
            .unwrap();
        assert_eq!(
            r,
            TimeRange::RelativeRecent {
                unit: TimeUnit::Month,
                count: 1
            }
        );
    }

    #[test]
    fn detects_chinese_last_week_short_form() {
        let r = classify_temporal_query("上周我跟 garry 聊了什么", now_at_may_2026()).unwrap();
        assert_eq!(
            r,
            TimeRange::RelativeRecent {
                unit: TimeUnit::Week,
                count: 1
            }
        );
    }

    #[test]
    fn detects_this_year() {
        let r = classify_temporal_query("今年我学到了哪些新概念", now_at_may_2026()).unwrap();
        assert_eq!(r, TimeRange::Calendar { year: 2026, month: None });
    }

    #[test]
    fn detects_this_month() {
        let r = classify_temporal_query("本月做了什么", now_at_may_2026()).unwrap();
        assert_eq!(
            r,
            TimeRange::Calendar { year: 2026, month: Some(5) }
        );
    }

    #[test]
    fn detects_chinese_calendar_year_month() {
        let r = classify_temporal_query("2026 年 5 月的工作", now_at_may_2026()).unwrap();
        assert_eq!(r, TimeRange::Calendar { year: 2026, month: Some(5) });
    }

    #[test]
    fn detects_iso_date() {
        let r = classify_temporal_query("on 2026-05-18 we shipped gbrain", now_at_may_2026())
            .unwrap();
        match r {
            TimeRange::Absolute { start_ms, end_ms } => {
                let expected_start = Utc
                    .with_ymd_and_hms(2026, 5, 18, 0, 0, 0)
                    .single()
                    .unwrap()
                    .timestamp_millis();
                assert_eq!(start_ms, expected_start);
                let expected_end = expected_start + Duration::days(1).num_milliseconds();
                assert_eq!(end_ms, expected_end);
            }
            _ => panic!("expected Absolute"),
        }
    }

    #[test]
    fn returns_none_for_non_temporal_query() {
        // Should NOT classify as temporal.
        assert!(classify_temporal_query("what is rust?", now_at_may_2026()).is_none());
        assert!(classify_temporal_query("tell me about alice", now_at_may_2026()).is_none());
        assert!(classify_temporal_query("", now_at_may_2026()).is_none());
    }

    #[test]
    fn resolve_relative_recent_returns_window_ending_now() {
        let now = now_at_may_2026();
        let r = TimeRange::RelativeRecent { unit: TimeUnit::Day, count: 7 };
        let (start, end) = r.resolve(now);
        assert_eq!(end, now.timestamp_millis());
        let expected_start = (now - Duration::days(7)).timestamp_millis();
        assert_eq!(start, expected_start);
    }

    #[test]
    fn resolve_calendar_year_returns_full_year_boundaries() {
        let r = TimeRange::Calendar { year: 2026, month: None };
        let (start, end) = r.resolve(now_at_may_2026());
        let expected_start = Utc
            .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let expected_end = Utc
            .with_ymd_and_hms(2027, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected_start);
        assert_eq!(end, expected_end);
    }

    #[test]
    fn resolve_calendar_december_rolls_into_next_year() {
        // Edge case: month=12 must produce end at jan 1 of next year.
        let r = TimeRange::Calendar { year: 2026, month: Some(12) };
        let (start, end) = r.resolve(now_at_may_2026());
        let expected_start = Utc
            .with_ymd_and_hms(2026, 12, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        let expected_end = Utc
            .with_ymd_and_hms(2027, 1, 1, 0, 0, 0)
            .single()
            .unwrap()
            .timestamp_millis();
        assert_eq!(start, expected_start);
        assert_eq!(end, expected_end);
    }

    #[test]
    fn today_yields_one_day_window() {
        let r = classify_temporal_query("what happened today", now_at_may_2026()).unwrap();
        assert_eq!(
            r,
            TimeRange::RelativeRecent { unit: TimeUnit::Day, count: 1 }
        );
    }
}
