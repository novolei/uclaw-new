//! Write-side persistence for per-turn LLM cost records.
//!
//! Called from the agent dispatcher's `emit_turn_cost` to capture usage
//! synchronously alongside the IPC event. No frontend dependency — events
//! are fire-and-forget from the listener's POV; persistence is the source
//! of truth for the dashboard.

use crate::app::AppState;
use crate::agent::types::calculate_cost;
use rusqlite::params;

/// Insert one cost record without prompt-cache detail. Errors are logged and
/// swallowed — cost capture is best-effort and must never fail the agent loop.
pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32) {
    record_usage(state, session_id, model, input_tokens, output_tokens, 0);
}

/// Insert one cost record including prompt-cache reads
/// (`cost_records.cached_input_tokens`, V49). Best-effort like [`record`].
pub fn record_usage(
    state: &AppState,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
) {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cost_store: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = insert_usage(&conn, session_id, model, input_tokens, output_tokens, cached_input_tokens) {
        tracing::warn!("cost_store: INSERT failed: {}", e);
    }
}

/// Testable core of [`record_usage`]: one `cost_records` row stamped now.
pub(crate) fn insert_usage(
    conn: &rusqlite::Connection,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    insert_usage_at(conn, session_id, model, input_tokens, output_tokens, cached_input_tokens, now)
}

fn insert_usage_at(
    conn: &rusqlite::Connection,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
    created_at: i64,
) -> rusqlite::Result<()> {
    let cost = calculate_cost(model, input_tokens, output_tokens);
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at, cached_input_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            session_id,
            model,
            input_tokens as i64,
            output_tokens as i64,
            cost,
            created_at,
            cached_input_tokens as i64
        ],
    )?;
    Ok(())
}

// ─── Prompt-cache hit rate (context continuity P3, Decision 18) ─────────

/// Anthropic-family models (name starts with `claude`) report `input_tokens`
/// EXCLUDING cache reads; OpenAI-compatible providers (incl. DeepSeek)
/// report `prompt_tokens` already including them.
fn input_excludes_cache(model: &str) -> bool {
    model.trim().to_ascii_lowercase().starts_with("claude")
}

/// Total prompt tokens of one call, cache reads included (Decision 18):
/// Anthropic `input + cached`, others `input`.
pub fn prompt_tokens_total(model: &str, input_tokens: u64, cached_input_tokens: u64) -> u64 {
    if input_excludes_cache(model) {
        input_tokens.saturating_add(cached_input_tokens)
    } else {
        input_tokens
    }
}

/// Prompt-cache hit ratio in `0.0..=1.0` (Decision 18): Anthropic
/// `cached / (input + cached)`, others `cached / input`. Zero denominator → 0.0.
pub fn cache_hit_ratio(model: &str, input_tokens: u64, cached_input_tokens: u64) -> f64 {
    ratio(cached_input_tokens, prompt_tokens_total(model, input_tokens, cached_input_tokens))
}

fn ratio(cached: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (cached as f64 / total as f64).clamp(0.0, 1.0)
    }
}

/// One session's prompt-cache totals: every `cost_records` row of the
/// session, each normalized with [`prompt_tokens_total`].
pub fn session_cache_stats(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<crate::ipc::SessionCacheStats> {
    let mut stmt = conn.prepare(
        "SELECT model, SUM(input_tokens), SUM(cached_input_tokens), COUNT(*)
         FROM cost_records WHERE session_id = ?1 GROUP BY model",
    )?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?, r.get::<_, i64>(2)?, r.get::<_, i64>(3)?))
    })?;
    let (mut calls, mut total, mut cached) = (0u64, 0u64, 0u64);
    for row in rows {
        let (model, input, cache, n) = row?;
        let (input, cache) = (input.max(0) as u64, cache.max(0) as u64);
        calls += n.max(0) as u64;
        total += prompt_tokens_total(&model, input, cache);
        cached += cache;
    }
    Ok(crate::ipc::SessionCacheStats {
        calls: calls.min(u32::MAX as u64) as u32,
        input_tokens: total,
        cached_input_tokens: cached,
        hit_ratio: ratio(cached, total),
    })
}

/// Per-day (UTC `YYYY-MM-DD`) cost rollup since `cutoff_ms`, ascending.
/// `input_tokens` stays the raw column sum; `hit_ratio` normalizes each
/// model's rows with [`prompt_tokens_total`] before dividing.
pub fn daily_rollups(
    conn: &rusqlite::Connection,
    cutoff_ms: i64,
) -> rusqlite::Result<Vec<crate::ipc::DailyCostRollup>> {
    let mut stmt = conn.prepare(
        "SELECT strftime('%Y-%m-%d', created_at / 1000, 'unixepoch') AS day, model,
                SUM(input_tokens), SUM(output_tokens), COALESCE(SUM(cost_usd), 0),
                COUNT(*), SUM(cached_input_tokens)
         FROM cost_records
         WHERE created_at >= ?1
         GROUP BY day, model
         ORDER BY day ASC",
    )?;
    let rows = stmt.query_map(params![cutoff_ms], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, f64>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, i64>(6)?,
        ))
    })?;
    // day → (rollup, normalized prompt total)
    let mut days: std::collections::BTreeMap<String, (crate::ipc::DailyCostRollup, u64)> = Default::default();
    for row in rows {
        let (day, model, input, output, cost, turns, cached) = row?;
        let entry = days.entry(day.clone()).or_insert_with(|| {
            (
                crate::ipc::DailyCostRollup {
                    day,
                    input_tokens: 0,
                    output_tokens: 0,
                    cost_usd: 0.0,
                    turn_count: 0,
                    cached_input_tokens: 0,
                    hit_ratio: 0.0,
                },
                0,
            )
        });
        entry.0.input_tokens += input;
        entry.0.output_tokens += output;
        entry.0.cost_usd += cost;
        entry.0.turn_count += turns;
        entry.0.cached_input_tokens += cached;
        entry.1 += prompt_tokens_total(&model, input.max(0) as u64, cached.max(0) as u64);
    }
    Ok(days
        .into_values()
        .map(|(mut d, total)| {
            d.hit_ratio = ratio(d.cached_input_tokens.max(0) as u64, total);
            d
        })
        .collect())
}

/// Per-model cost rollup since `cutoff_ms`, most expensive first.
pub fn model_rollups(
    conn: &rusqlite::Connection,
    cutoff_ms: i64,
) -> rusqlite::Result<Vec<crate::ipc::ModelCostRollup>> {
    let mut stmt = conn.prepare(
        "SELECT model, SUM(input_tokens), SUM(output_tokens),
                COALESCE(SUM(cost_usd), 0) AS cost, COUNT(*), SUM(cached_input_tokens)
         FROM cost_records
         WHERE created_at >= ?1
         GROUP BY model
         ORDER BY cost DESC",
    )?;
    let rows = stmt.query_map(params![cutoff_ms], |r| {
        let model: String = r.get(0)?;
        let input: i64 = r.get(1)?;
        let cached: i64 = r.get(5)?;
        let hit_ratio = cache_hit_ratio(&model, input.max(0) as u64, cached.max(0) as u64);
        Ok(crate::ipc::ModelCostRollup {
            model,
            input_tokens: input,
            output_tokens: r.get(2)?,
            cost_usd: r.get(3)?,
            turn_count: r.get(4)?,
            cached_input_tokens: cached,
            hit_ratio,
        })
    })?;
    rows.collect()
}

/// SUM(cost_usd) for cost_records with created_at >= since_ms.
/// Returns 0.0 on any error (best-effort, matches the rest of this module).
pub fn monthly_total(state: &AppState, since_ms: i64) -> f64 {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return 0.0,
    };
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
        params![since_ms],
        |row| row.get::<_, f64>(0),
    ).unwrap_or(0.0)
}

/// Compute the start of the current month (UTC) in epoch ms.
pub fn current_month_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Compute the start of the current day (UTC) in epoch ms.
/// Used by per-day spend caps (Sprint 2.1b learning, Phase 5 lint).
pub fn current_day_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Memory OS Sprint 2.1b — SUM(input_tokens + output_tokens) for today's
/// cost_records where `model LIKE 'memory_learning%'`. Used by the chat-turn
/// extractor's daily-budget gate before invoking the LLM layer. Returns 0
/// on any error (best-effort; falling back to 0 lets the producer continue
/// running rather than blocking on transient DB issues).
///
/// Standalone function (not on AppState) so the agent dispatcher can call
/// it given only the raw `Arc<Mutex<Connection>>` — `set_learning_pipeline`
/// hands it the same db handle AppState owns.
pub fn today_learning_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'memory_learning%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// T4 memory growth — today's tokens spent by growth passes (cost rows with
/// `model LIKE 'memory_growth%'`). Returns 0 on any error.
pub fn today_growth_tokens(db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'memory_growth%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Sprint 2.4b — sibling of [`today_learning_tokens`] for the gbrain
/// chat-turn extractor's daily-budget gate. Sums today's `cost_records`
/// where `model LIKE 'gbrain_extract%'` (the prefix
/// `crate::memory_graph::chat_extractor` writes via the `"gbrain_extract"`
/// cost_tag passed to `MemoryOsLlm::complete_text`).
///
/// Standalone fn (not on AppState) so the agent dispatcher can call it
/// given only the raw `Arc<Mutex<Connection>>` — matches the shape
/// `set_gbrain_extractor_pipeline` hands the dispatcher.
pub fn today_gbrain_extract_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'gbrain_extract%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Pure helper: which threshold (if any) was crossed on this turn?
/// Fires 100 OR 80 (preferring 100 when both crossed in one turn), or None.
/// Never fires when budget <= 0.0.
pub fn fired_threshold(total_before: f64, total_after: f64, budget: f64) -> Option<u8> {
    if budget <= 0.0 { return None; }
    let crossed_100 = total_before / budget < 1.00 && total_after / budget >= 1.00;
    let crossed_80 = total_before / budget < 0.80 && total_after / budget >= 0.80;
    if crossed_100 { Some(100) }
    else if crossed_80 { Some(80) }
    else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_threshold_when_under_80() {
        assert_eq!(fired_threshold(10.0, 50.0, 100.0), None);
    }
    #[test]
    fn fires_80_when_crossing_upward() {
        assert_eq!(fired_threshold(75.0, 85.0, 100.0), Some(80));
    }
    #[test]
    fn does_not_refire_80_when_already_above() {
        assert_eq!(fired_threshold(85.0, 90.0, 100.0), None);
    }
    #[test]
    fn fires_100_when_crossing_upward() {
        assert_eq!(fired_threshold(95.0, 105.0, 100.0), Some(100));
    }
    #[test]
    fn fires_100_not_80_when_crossing_both_at_once() {
        assert_eq!(fired_threshold(50.0, 150.0, 100.0), Some(100));
    }
    #[test]
    fn no_fire_when_budget_zero_or_negative() {
        assert_eq!(fired_threshold(50.0, 150.0, 0.0), None);
        assert_eq!(fired_threshold(50.0, 150.0, -10.0), None);
    }

    // ─── Sprint 2.4b — gbrain extractor budget query ─────────────

    fn open_test_db_with_cost_records() -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        use std::sync::{Arc, Mutex};
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Minimal V13 cost_records schema — just enough to exercise the
        // SUM-by-prefix queries. Real schema has more columns but they
        // don't affect this test.
        conn.execute_batch(
            "CREATE TABLE cost_records (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 input_tokens INTEGER NOT NULL,
                 output_tokens INTEGER NOT NULL,
                 cost_usd REAL,
                 created_at INTEGER NOT NULL,
                 cached_input_tokens INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn insert_cost(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        model: &str,
        input: i64,
        output: i64,
        at_ms: i64,
    ) {
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, 'test', ?2, ?3, ?4, 0.0, ?5)",
            params![uuid::Uuid::new_v4().to_string(), model, input, output, at_ms],
        )
        .unwrap();
    }

    #[test]
    fn today_gbrain_extract_tokens_sums_only_gbrain_prefix() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        // Two gbrain rows today → should sum to 350
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 100, 50, now);
        insert_cost(&db, "gbrain_extract:other-model", 150, 50, now);
        // Memory_learning row today — must NOT count toward gbrain total
        insert_cost(&db, "memory_learning:claude-haiku-4-5", 999, 999, now);
        // Gbrain row from yesterday — must NOT count
        let yesterday = now - 26 * 60 * 60 * 1000;
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 500, 500, yesterday);

        assert_eq!(today_gbrain_extract_tokens(&db), 350);
    }

    #[test]
    fn today_gbrain_extract_tokens_returns_zero_when_no_rows() {
        let db = open_test_db_with_cost_records();
        assert_eq!(today_gbrain_extract_tokens(&db), 0);
    }

    // ─── Context continuity P3 Task 11 — prompt-cache observability ─────

    fn insert_cached(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        session: &str,
        model: &str,
        input: u32,
        cached: u32,
        at_ms: i64,
    ) {
        let conn = db.lock().unwrap();
        insert_usage_at(&conn, session, model, input, 10, cached, at_ms).unwrap();
    }

    #[test]
    fn record_usage_writes_cached_input_tokens() {
        let db = open_test_db_with_cost_records();
        let conn = db.lock().unwrap();
        insert_usage(&conn, "s1", "claude-sonnet-4-5", 600, 50, 400).unwrap();
        let (input, output, cached): (i64, i64, i64) = conn
            .query_row(
                "SELECT input_tokens, output_tokens, cached_input_tokens FROM cost_records WHERE session_id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!((input, output, cached), (600, 50, 400));
    }

    #[test]
    fn cache_hit_ratio_anthropic_vs_openai_semantics() {
        // Anthropic: input excludes cache reads.
        assert_eq!(prompt_tokens_total("claude-sonnet-4-5", 600, 400), 1000);
        assert!((cache_hit_ratio("claude-sonnet-4-5", 600, 400) - 0.4).abs() < 1e-9);
        // OpenAI-compatible (DeepSeek): prompt_tokens already include cache.
        assert_eq!(prompt_tokens_total("deepseek-chat", 1000, 400), 1000);
        assert!((cache_hit_ratio("deepseek-chat", 1000, 400) - 0.4).abs() < 1e-9);
        // Zero denominator → 0.0; clamped to 1.0.
        assert_eq!(cache_hit_ratio("deepseek-chat", 0, 0), 0.0);
        assert_eq!(cache_hit_ratio("claude-x", 0, 0), 0.0);
        assert_eq!(cache_hit_ratio("gpt-4o", 100, 500), 1.0);
    }

    #[test]
    fn session_cache_stats_aggregates_per_session() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        insert_cached(&db, "s1", "claude-sonnet-4-5", 600, 400, now);
        insert_cached(&db, "s1", "deepseek-chat", 1000, 400, now);
        insert_cached(&db, "s2", "deepseek-chat", 5000, 0, now);
        let conn = db.lock().unwrap();
        let stats = session_cache_stats(&conn, "s1").unwrap();
        assert_eq!(stats.calls, 2);
        assert_eq!(stats.input_tokens, 2000);
        assert_eq!(stats.cached_input_tokens, 800);
        assert!((stats.hit_ratio - 0.4).abs() < 1e-9);
        let json = serde_json::to_value(&stats).unwrap();
        assert!(json.get("cachedInputTokens").is_some() && json.get("hitRatio").is_some(), "{json}");

        let empty = session_cache_stats(&conn, "nope").unwrap();
        assert_eq!((empty.calls, empty.input_tokens, empty.hit_ratio), (0, 0, 0.0));
    }

    #[test]
    fn daily_rollup_includes_cached_and_ratio() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        insert_cached(&db, "s1", "claude-sonnet-4-5", 600, 400, now);
        insert_cached(&db, "s1", "deepseek-chat", 1000, 400, now);
        let conn = db.lock().unwrap();
        let days = daily_rollups(&conn, now - 86_400_000).unwrap();
        assert_eq!(days.len(), 1);
        let d = &days[0];
        assert_eq!(d.input_tokens, 1600);
        assert_eq!(d.cached_input_tokens, 800);
        assert_eq!(d.turn_count, 2);
        assert!((d.hit_ratio - 0.4).abs() < 1e-9, "{}", d.hit_ratio);

        let models = model_rollups(&conn, now - 86_400_000).unwrap();
        let claude = models.iter().find(|m| m.model == "claude-sonnet-4-5").unwrap();
        assert_eq!(claude.cached_input_tokens, 400);
        assert!((claude.hit_ratio - 0.4).abs() < 1e-9);
        let ds = models.iter().find(|m| m.model == "deepseek-chat").unwrap();
        assert!((ds.hit_ratio - 0.4).abs() < 1e-9);
    }
}
