//! Memory OS L3 §4.12.3 RETAINED — Spaced Repetition (Anki SM-2 ladder).
//!
//! Schedules periodic LLM re-checks of high-importance verified
//! EntityPages so the wiki stays calibrated as the world (or the
//! user's understanding) evolves. Per ADR 2026-05-20 §8 this is one
//! of 4 RETAINED L3 enhancements; gbrain has no equivalent.
//!
//! ## V1 scope (this PR)
//!
//! - Interval ladder: 1, 3, 7, 14, 30, 90 days (6 levels)
//! - Enrollment helper: pick high-importance verified EntityPages
//! - Review pass/fail handlers updating `interval_idx` and counters
//! - Due-list query (returns nodes with `next_review_at <= now`)
//!
//! ## V2 scope (future PR)
//!
//! - Scheduler hook into `proactive::service` (runs daily,
//!   pulls the due list, fires LLM re-check, records pass/fail)
//! - LLM prompt for "is this page still accurate given the
//!   latest timeline?"
//! - Tauri command + UI for manual review trigger
//!
//! This PR ships the state-machine + table I/O. The scheduler
//! integration is a follow-up so each commit stays bisectable.

use rusqlite::{params, Connection, OptionalExtension};

/// SM-2 ladder of review intervals in days. Index 0 = "review in 1 day"
/// after enrollment; on a passing review, advance to next index; on
/// failure, reset to 0.
pub const INTERVAL_LADDER_DAYS: &[u32] = &[1, 3, 7, 14, 30, 90];

/// Importance threshold above which an EntityPage becomes eligible
/// for spaced-repetition enrollment. Below this we skip; the pages
/// aren't worth periodic re-checking. Matches L3 spec §4.12.3
/// `importance >= 0.6`.
pub const ENROLLMENT_IMPORTANCE_THRESHOLD: f64 = 0.6;

/// Snapshot of one node's current SR state.
#[derive(Debug, Clone, PartialEq)]
pub struct SpacedRepetitionState {
    pub node_id: String,
    pub interval_idx: u8,
    pub last_reviewed_at: i64,
    pub next_review_at: i64,
    pub reviews_total: u32,
    pub reviews_passed: u32,
    pub enabled: bool,
}

impl SpacedRepetitionState {
    /// Pass-rate (0.0–1.0) over the lifetime of this enrollment.
    /// Returns 0.0 when no reviews have happened yet (avoid div-by-zero).
    pub fn pass_rate(&self) -> f64 {
        if self.reviews_total == 0 {
            0.0
        } else {
            self.reviews_passed as f64 / self.reviews_total as f64
        }
    }
}

/// Compute the next review timestamp (unix-ms) given current ladder
/// index and `now`.
pub fn next_review_at_ms(interval_idx: u8, now_ms: i64) -> i64 {
    let idx = (interval_idx as usize).min(INTERVAL_LADDER_DAYS.len() - 1);
    let days = INTERVAL_LADDER_DAYS[idx] as i64;
    now_ms + days * 24 * 60 * 60 * 1000
}

/// Enroll a node for spaced repetition. If the node already has a row
/// (e.g. previously unenrolled), this is an idempotent upsert that
/// keeps the existing `reviews_total` / `reviews_passed` counters
/// (a node that toggles enabled off → on shouldn't lose its history).
///
/// Sets `interval_idx = 0`, `last_reviewed_at = now`, `next_review_at = now + 1 day`,
/// `enabled = 1`.
pub fn enroll_node(
    conn: &Connection,
    node_id: &str,
    now_ms: i64,
) -> rusqlite::Result<()> {
    let next = next_review_at_ms(0, now_ms);
    conn.execute(
        "INSERT INTO spaced_repetition_state
             (node_id, interval_idx, last_reviewed_at, next_review_at,
              reviews_total, reviews_passed, enabled)
         VALUES (?1, 0, ?2, ?3, 0, 0, 1)
         ON CONFLICT(node_id) DO UPDATE SET
             interval_idx = 0,
             last_reviewed_at = excluded.last_reviewed_at,
             next_review_at = excluded.next_review_at,
             enabled = 1",
        params![node_id, now_ms, next],
    )?;
    Ok(())
}

/// Mark a review as **passed**: advance to next ladder index (clamped
/// to last), increment counters, reschedule.
pub fn record_pass(conn: &Connection, node_id: &str, now_ms: i64) -> rusqlite::Result<()> {
    let state = get_state(conn, node_id)?;
    let next_idx = ((state.interval_idx as usize) + 1).min(INTERVAL_LADDER_DAYS.len() - 1) as u8;
    let next = next_review_at_ms(next_idx, now_ms);
    conn.execute(
        "UPDATE spaced_repetition_state
         SET interval_idx = ?1,
             last_reviewed_at = ?2,
             next_review_at = ?3,
             reviews_total = reviews_total + 1,
             reviews_passed = reviews_passed + 1
         WHERE node_id = ?4",
        params![next_idx, now_ms, next, node_id],
    )?;
    Ok(())
}

/// Mark a review as **failed**: reset to interval 0, increment total
/// (but NOT passed), reschedule to 1 day from now.
pub fn record_fail(conn: &Connection, node_id: &str, now_ms: i64) -> rusqlite::Result<()> {
    let next = next_review_at_ms(0, now_ms);
    conn.execute(
        "UPDATE spaced_repetition_state
         SET interval_idx = 0,
             last_reviewed_at = ?1,
             next_review_at = ?2,
             reviews_total = reviews_total + 1
         WHERE node_id = ?3",
        params![now_ms, next, node_id],
    )?;
    Ok(())
}

/// Toggle enabled/disabled. Disabled rows are excluded from the
/// `due_now` query but their counter history is preserved.
pub fn set_enabled(conn: &Connection, node_id: &str, enabled: bool) -> rusqlite::Result<()> {
    crate::memory_graph::enforce_freeze("spaced_repetition::set_enabled");
    let v = if enabled { 1 } else { 0 };
    conn.execute(
        "UPDATE spaced_repetition_state SET enabled = ?1 WHERE node_id = ?2",
        params![v, node_id],
    )?;
    Ok(())
}

/// Fetch one node's state, if enrolled. Returns `Ok(None)` for nodes
/// without a row.
pub fn get_state(conn: &Connection, node_id: &str) -> rusqlite::Result<SpacedRepetitionState> {
    conn.query_row(
        "SELECT node_id, interval_idx, last_reviewed_at, next_review_at,
                reviews_total, reviews_passed, enabled
         FROM spaced_repetition_state WHERE node_id = ?1",
        [node_id],
        |r| {
            Ok(SpacedRepetitionState {
                node_id: r.get(0)?,
                interval_idx: r.get::<_, i64>(1)? as u8,
                last_reviewed_at: r.get(2)?,
                next_review_at: r.get(3)?,
                reviews_total: r.get::<_, i64>(4)? as u32,
                reviews_passed: r.get::<_, i64>(5)? as u32,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        },
    )
}

/// Optional version of `get_state` for callers that want to handle
/// "not enrolled" without an Err.
pub fn try_get_state(
    conn: &Connection,
    node_id: &str,
) -> rusqlite::Result<Option<SpacedRepetitionState>> {
    conn.query_row(
        "SELECT node_id, interval_idx, last_reviewed_at, next_review_at,
                reviews_total, reviews_passed, enabled
         FROM spaced_repetition_state WHERE node_id = ?1",
        [node_id],
        |r| {
            Ok(SpacedRepetitionState {
                node_id: r.get(0)?,
                interval_idx: r.get::<_, i64>(1)? as u8,
                last_reviewed_at: r.get(2)?,
                next_review_at: r.get(3)?,
                reviews_total: r.get::<_, i64>(4)? as u32,
                reviews_passed: r.get::<_, i64>(5)? as u32,
                enabled: r.get::<_, i64>(6)? != 0,
            })
        },
    )
    .optional()
}

/// List node_ids whose `next_review_at <= now_ms` and `enabled = 1`,
/// limited to `limit` rows ordered by oldest-due-first. The V45
/// partial index makes this an indexed lookup.
pub fn due_now(conn: &Connection, now_ms: i64, limit: usize) -> rusqlite::Result<Vec<String>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT node_id FROM spaced_repetition_state
         WHERE enabled = 1 AND next_review_at <= ?1
         ORDER BY next_review_at ASC
         LIMIT ?2",
    )?;
    let rows: Vec<String> = stmt
        .query_map(params![now_ms, limit as i64], |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        // FK CASCADE off — tests don't need a real memory_nodes row,
        // since spaced_repetition_state references it but we exercise
        // the SR state machine in isolation.
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn
    }

    #[test]
    fn next_review_at_ladder_matches_spec() {
        let now = 1_700_000_000_000_i64;
        let day_ms = 86_400_000_i64;
        assert_eq!(next_review_at_ms(0, now), now + 1 * day_ms);
        assert_eq!(next_review_at_ms(1, now), now + 3 * day_ms);
        assert_eq!(next_review_at_ms(2, now), now + 7 * day_ms);
        assert_eq!(next_review_at_ms(3, now), now + 14 * day_ms);
        assert_eq!(next_review_at_ms(4, now), now + 30 * day_ms);
        assert_eq!(next_review_at_ms(5, now), now + 90 * day_ms);
        // Overflow clamps to last.
        assert_eq!(next_review_at_ms(99, now), now + 90 * day_ms);
    }

    #[test]
    fn enroll_creates_row_with_interval_zero() {
        let conn = fresh_conn();
        let now = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", now).unwrap();
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(state.interval_idx, 0);
        assert!(state.enabled);
        assert_eq!(state.reviews_total, 0);
        assert_eq!(state.reviews_passed, 0);
        assert_eq!(state.last_reviewed_at, now);
        assert_eq!(state.next_review_at, now + 86_400_000);
    }

    #[test]
    fn re_enroll_preserves_history_but_resets_interval() {
        // User toggles off → on; counters should NOT zero out.
        let conn = fresh_conn();
        let now = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", now).unwrap();
        // Simulate 3 reviews completed
        conn.execute(
            "UPDATE spaced_repetition_state SET interval_idx = 4, reviews_total = 3, reviews_passed = 2 WHERE node_id = 'n1'",
            [],
        ).unwrap();
        // Re-enroll
        enroll_node(&conn, "n1", now + 1000).unwrap();
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(state.interval_idx, 0, "re-enroll resets ladder");
        assert_eq!(state.reviews_total, 3, "history preserved");
        assert_eq!(state.reviews_passed, 2);
        assert_eq!(state.last_reviewed_at, now + 1000);
    }

    #[test]
    fn record_pass_advances_ladder_and_counters() {
        let conn = fresh_conn();
        let now = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", now).unwrap();
        record_pass(&conn, "n1", now + 86_400_000).unwrap();
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(state.interval_idx, 1);
        assert_eq!(state.reviews_total, 1);
        assert_eq!(state.reviews_passed, 1);
    }

    #[test]
    fn record_pass_clamps_at_last_ladder_index() {
        let conn = fresh_conn();
        let now = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", now).unwrap();
        // Pass through all 6 ladder steps.
        let mut t = now;
        for _ in 0..(INTERVAL_LADDER_DAYS.len() + 5) {
            t += 1000;
            record_pass(&conn, "n1", t).unwrap();
        }
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(
            state.interval_idx,
            (INTERVAL_LADDER_DAYS.len() - 1) as u8,
            "interval should clamp at last index, got {}",
            state.interval_idx
        );
    }

    #[test]
    fn record_fail_resets_interval_but_keeps_total_count() {
        let conn = fresh_conn();
        let now = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", now).unwrap();
        record_pass(&conn, "n1", now + 1000).unwrap();
        record_pass(&conn, "n1", now + 2000).unwrap();
        record_fail(&conn, "n1", now + 3000).unwrap();
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(state.interval_idx, 0, "fail resets ladder to 0");
        assert_eq!(state.reviews_total, 3, "total includes the failed review");
        assert_eq!(state.reviews_passed, 2, "passed count unchanged");
        // Pass-rate sanity.
        assert!((state.pass_rate() - 2.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn due_now_returns_only_enabled_and_overdue_nodes() {
        let conn = fresh_conn();
        let base = 1_700_000_000_000_i64;
        // n-due: enrolled, scheduled for 1 day after `base` → due at base+day.
        enroll_node(&conn, "n-due", base).unwrap();
        // n-future: enrolled at base+10d, scheduled 1 day later.
        enroll_node(&conn, "n-future", base + 10 * 86_400_000).unwrap();
        // n-disabled: enrolled then disabled.
        enroll_node(&conn, "n-disabled", base).unwrap();
        set_enabled(&conn, "n-disabled", false).unwrap();

        // Query at base + 2 days → only n-due qualifies.
        let due = due_now(&conn, base + 2 * 86_400_000, 100).unwrap();
        assert_eq!(due, vec!["n-due"]);
    }

    #[test]
    fn due_now_respects_limit() {
        let conn = fresh_conn();
        let base = 1_700_000_000_000_i64;
        for i in 0..10 {
            enroll_node(&conn, &format!("n-{}", i), base).unwrap();
        }
        let due = due_now(&conn, base + 2 * 86_400_000, 3).unwrap();
        assert_eq!(due.len(), 3);
    }

    #[test]
    fn due_now_zero_limit_returns_empty() {
        let conn = fresh_conn();
        let base = 1_700_000_000_000_i64;
        enroll_node(&conn, "n1", base).unwrap();
        let due = due_now(&conn, base + 2 * 86_400_000, 0).unwrap();
        assert!(due.is_empty());
    }

    #[test]
    fn try_get_state_returns_none_for_unknown_node() {
        let conn = fresh_conn();
        let result = try_get_state(&conn, "ghost-node-id").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn pass_rate_handles_zero_reviews() {
        let conn = fresh_conn();
        enroll_node(&conn, "n1", 1_700_000_000_000).unwrap();
        let state = get_state(&conn, "n1").unwrap();
        assert_eq!(state.pass_rate(), 0.0, "0/0 should yield 0.0, not NaN");
    }

    #[test]
    fn set_enabled_round_trips() {
        let conn = fresh_conn();
        enroll_node(&conn, "n1", 1_700_000_000_000).unwrap();
        set_enabled(&conn, "n1", false).unwrap();
        assert!(!get_state(&conn, "n1").unwrap().enabled);
        set_enabled(&conn, "n1", true).unwrap();
        assert!(get_state(&conn, "n1").unwrap().enabled);
    }
}
