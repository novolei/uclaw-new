//! Memory OS L3 §4.12.4 RETAINED — Concept Drift Detection.
//!
//! Tracks how an EntityPage's `compiled_truth` changes over time. If
//! the same page gets rewritten frequently with large content diffs,
//! it's "drifting" — one of:
//! - (a) real evolution (the user / world is changing)
//! - (b) LLM instability (insufficient sources to converge)
//! - (c) unresolved contradiction (matters to the user)
//!
//! Either way, flagging the drift gives the user / next agent a
//! chance to review. gbrain has no equivalent — it tracks per-page
//! version history but doesn't compute or surface drift signals.
//!
//! ## V1 scope (this PR)
//!
//! - V46 migration: `drift_events` table (PR #271-equivalent shape)
//! - Pure-Rust `compute_drift_score` over an ordered slice of content
//!   versions, returning a [0.0, 1.0] magnitude
//! - `record_drift_event` upsert helper
//! - `list_open_events_for_node` read helper for future UI
//!
//! ## V2 scope (future PR)
//!
//! - Scheduler hook: nightly scan over EntityPages with N+ versions
//!   in the last 30 days, compute drift, insert event when above
//!   threshold
//! - LLM triage hook: optionally classify drift as evolution /
//!   instability / contradiction
//! - UI for the review queue (resolve / dismiss / snooze)
//! - `review_queue_items` integration (Cognitive Phase 13 is paused
//!   per ADR, so for now drift events live alone)
//!
//! ## Algorithm
//!
//! Given N content versions in chronological order:
//! 1. Compute pairwise Levenshtein distance between consecutive
//!    versions, normalized by max length.
//! 2. Drift = average normalized distance across pairs.
//! 3. Boost when version frequency is high (more rewrites in less
//!    time = more drift).
//!
//! Result clamped to [0.0, 1.0]. Threshold for triggering an event:
//! 0.40 (tunable via `DRIFT_THRESHOLD` const).

use rusqlite::{params, Connection};

/// Above this score, drift is considered significant enough to log
/// an event. Tunable. Spec §4.12.4 suggests 0.5 raw Levenshtein —
/// 0.40 here accounts for the version-frequency boost.
pub const DRIFT_THRESHOLD: f64 = 0.40;

/// Minimum number of versions required to compute drift. Below this
/// there's no meaningful pairwise comparison.
pub const MIN_VERSIONS_FOR_DRIFT: usize = 2;

/// Result of one drift computation. The `details` captures per-pair
/// data for debugging / future LLM triage.
#[derive(Debug, Clone, PartialEq)]
pub struct DriftResult {
    /// Final score in [0.0, 1.0]. >= [`DRIFT_THRESHOLD`] should trigger
    /// a `drift_events` row.
    pub score: f64,
    /// Per-consecutive-pair normalized Levenshtein distances.
    pub pair_distances: Vec<f64>,
    /// Whether [`DRIFT_THRESHOLD`] was crossed.
    pub above_threshold: bool,
}

/// Compute drift score from a chronologically-ordered slice of
/// content versions. Returns `None` when fewer than
/// [`MIN_VERSIONS_FOR_DRIFT`] versions are provided.
///
/// Pure function; no I/O. Caller is responsible for fetching
/// versions from `memory_versions` in the right order.
pub fn compute_drift_score(versions: &[String]) -> Option<DriftResult> {
    if versions.len() < MIN_VERSIONS_FOR_DRIFT {
        return None;
    }
    let mut distances: Vec<f64> = Vec::with_capacity(versions.len() - 1);
    for w in versions.windows(2) {
        distances.push(normalized_levenshtein(&w[0], &w[1]));
    }
    let avg = distances.iter().sum::<f64>() / distances.len() as f64;
    // Frequency boost: 5+ pair-diffs → multiply by 1.2 (more rewrites
    // = more drift signal). Clamp final to 1.0.
    let freq_boost = if distances.len() >= 5 { 1.2 } else { 1.0 };
    let score = (avg * freq_boost).min(1.0);
    Some(DriftResult {
        score,
        pair_distances: distances,
        above_threshold: score >= DRIFT_THRESHOLD,
    })
}

/// Levenshtein edit distance normalized by max length (so result is
/// in [0.0, 1.0]). Uses iterative DP with two rows for memory
/// efficiency — content versions can be 500+ chars; full matrix is
/// avoided.
fn normalized_levenshtein(a: &str, b: &str) -> f64 {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let a_len = a_chars.len();
    let b_len = b_chars.len();
    if a_len == 0 && b_len == 0 {
        return 0.0;
    }
    if a_len == 0 || b_len == 0 {
        return 1.0;
    }
    let mut prev: Vec<usize> = (0..=b_len).collect();
    let mut curr: Vec<usize> = vec![0; b_len + 1];
    for (i, ca) in a_chars.iter().enumerate() {
        curr[0] = i + 1;
        for (j, cb) in b_chars.iter().enumerate() {
            let cost = if ca == cb { 0 } else { 1 };
            curr[j + 1] = (prev[j + 1] + 1)
                .min(curr[j] + 1)
                .min(prev[j] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    let dist = prev[b_len] as f64;
    let max_len = a_len.max(b_len) as f64;
    (dist / max_len).clamp(0.0, 1.0)
}

/// Persist a drift event into `drift_events`. Caller is expected to
/// already have computed the result via `compute_drift_score`.
///
/// Returns the inserted event id (matches input `id`).
pub fn record_drift_event(
    conn: &Connection,
    id: &str,
    node_id: &str,
    score: f64,
    snapshot_version_ids: &[String],
    computed_at_ms: i64,
) -> rusqlite::Result<String> {
    let snapshots_json = serde_json::to_string(snapshot_version_ids)
        .unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "INSERT INTO drift_events
             (id, node_id, score, snapshot_version_ids, computed_at, status)
         VALUES (?1, ?2, ?3, ?4, ?5, 'open')",
        params![id, node_id, score, snapshots_json, computed_at_ms],
    )?;
    Ok(id.to_string())
}

/// Max number of recent version contents to sample per node when
/// computing drift. Caps memory + Levenshtein cost on pages with
/// long histories; the most recent versions carry the most signal.
pub const MAX_VERSIONS_SAMPLED: usize = 6;

/// Outcome of one `scan_and_record_drift` batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriftScanOutcome {
    /// EntityPages examined (had >= MIN_VERSIONS_FOR_DRIFT versions).
    pub scanned: usize,
    /// Drift events recorded (score >= DRIFT_THRESHOLD).
    pub flagged: usize,
}

/// Scan up to `batch_size` EntityPages that have multiple recent
/// versions, compute drift over their content history, and record a
/// `drift_events` row when the score crosses [`DRIFT_THRESHOLD`].
///
/// Zero-LLM: pure content comparison. The scheduler hook (Q1c-style)
/// calls this on a timer. Only considers EntityPages (kind =
/// 'entity_page') with at least [`MIN_VERSIONS_FOR_DRIFT`] versions.
///
/// To avoid re-flagging the same drift every run, a node is skipped
/// if it already has an `open` drift_event whose `computed_at` is
/// newer than its latest version's creation — i.e. we only flag
/// fresh drift since the last open signal. (Simpler heuristic for
/// V1: skip if ANY open event exists for the node. The resolve/
/// dismiss flow clears it so the next drift can re-flag.)
pub fn scan_and_record_drift(
    conn: &Connection,
    batch_size: usize,
    now_ms: i64,
) -> rusqlite::Result<DriftScanOutcome> {
    if batch_size == 0 {
        return Ok(DriftScanOutcome { scanned: 0, flagged: 0 });
    }

    // Candidate nodes: entity_page kind, with >= MIN_VERSIONS_FOR_DRIFT
    // versions, NOT already carrying an open drift event. Ordered by
    // most-recently-updated so active pages are checked first.
    let candidate_ids: Vec<String> = {
        let mut stmt = conn.prepare(
            "SELECT n.id
             FROM memory_nodes n
             WHERE n.kind = 'entity_page'
               AND (SELECT COUNT(*) FROM memory_versions v WHERE v.node_id = n.id) >= ?1
               AND NOT EXISTS (
                   SELECT 1 FROM drift_events d
                   WHERE d.node_id = n.id AND d.status = 'open'
               )
             ORDER BY n.updated_at DESC
             LIMIT ?2",
        )?;
        let ids: Vec<String> = stmt
            .query_map(
                params![MIN_VERSIONS_FOR_DRIFT as i64, batch_size as i64],
                |r| r.get::<_, String>(0),
            )?
            .filter_map(Result::ok)
            .collect();
        ids
    };

    let mut scanned = 0usize;
    let mut flagged = 0usize;
    for node_id in candidate_ids {
        scanned += 1;
        // Pull the most recent N versions, chronological order.
        let versions: Vec<(String, String)> = {
            let mut stmt = match conn.prepare(
                "SELECT id, content FROM memory_versions
                 WHERE node_id = ?1
                 ORDER BY created_at DESC
                 LIMIT ?2",
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(node_id = %node_id, error = %e, "drift scan: prepare failed");
                    continue;
                }
            };
            let mut rows: Vec<(String, String)> = match stmt.query_map(
                params![node_id, MAX_VERSIONS_SAMPLED as i64],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
            ) {
                Ok(it) => it.filter_map(Result::ok).collect(),
                Err(e) => {
                    tracing::warn!(node_id = %node_id, error = %e, "drift scan: query failed");
                    continue;
                }
            };
            // We fetched DESC (newest first); reverse to chronological.
            rows.reverse();
            rows
        };

        if versions.len() < MIN_VERSIONS_FOR_DRIFT {
            continue;
        }
        let contents: Vec<String> = versions.iter().map(|(_, c)| c.clone()).collect();
        let version_ids: Vec<String> = versions.iter().map(|(id, _)| id.clone()).collect();

        if let Some(result) = compute_drift_score(&contents) {
            if result.above_threshold {
                let event_id = format!("drift-{}", uuid::Uuid::new_v4());
                if let Err(e) = record_drift_event(
                    conn,
                    &event_id,
                    &node_id,
                    result.score,
                    &version_ids,
                    now_ms,
                ) {
                    tracing::warn!(node_id = %node_id, error = %e, "drift scan: record failed");
                } else {
                    flagged += 1;
                }
            }
        }
    }

    Ok(DriftScanOutcome { scanned, flagged })
}

/// List `open` drift events for one node, newest first. Useful for
/// a future UI panel that shows "drift signals on this entity".
pub fn list_open_events_for_node(
    conn: &Connection,
    node_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<(String, f64, i64)>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT id, score, computed_at
         FROM drift_events
         WHERE node_id = ?1 AND status = 'open'
         ORDER BY computed_at DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![node_id, limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, i64>(2)?))
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// 全局漂移事件行（join memory_nodes 取标题）。供 Health 面板列表用。
#[derive(Debug, Clone)]
pub struct DriftEventRow {
    pub id: String,
    pub node_id: String,
    pub title: String,
    pub score: f64,
    pub computed_at: i64,
}

/// 列出某 space 下所有 open 漂移事件，按 score 降序。
pub fn list_open_drift_events(
    conn: &Connection,
    space_id: &str,
    limit: usize,
) -> rusqlite::Result<Vec<DriftEventRow>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT d.id, d.node_id, n.title, d.score, d.computed_at
         FROM drift_events d
         JOIN memory_nodes n ON n.id = d.node_id
         WHERE d.status = 'open' AND n.space_id = ?1
         ORDER BY d.score DESC
         LIMIT ?2",
    )?;
    let rows = stmt
        .query_map(params![space_id, limit as i64], |r| {
            Ok(DriftEventRow {
                id: r.get(0)?,
                node_id: r.get(1)?,
                title: r.get(2)?,
                score: r.get(3)?,
                computed_at: r.get(4)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

/// 把一条漂移事件标记为已处理（open → resolved）。
pub fn resolve_drift_event(
    conn: &Connection,
    id: &str,
    note: Option<&str>,
    now_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE drift_events
         SET status = 'resolved', resolved_at = ?2, resolution_note = ?3
         WHERE id = ?1",
        params![id, now_ms, note],
    )?;
    Ok(())
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn
    }

    #[test]
    fn compute_returns_none_with_fewer_than_two_versions() {
        assert!(compute_drift_score(&[]).is_none());
        assert!(compute_drift_score(&["x".into()]).is_none());
    }

    #[test]
    fn identical_versions_yield_zero_drift() {
        let result = compute_drift_score(&[
            "Alice is a software engineer at OpenAI.".into(),
            "Alice is a software engineer at OpenAI.".into(),
            "Alice is a software engineer at OpenAI.".into(),
        ])
        .unwrap();
        assert!((result.score - 0.0).abs() < 1e-9);
        assert!(!result.above_threshold);
    }

    #[test]
    fn completely_different_versions_yield_high_drift() {
        let result = compute_drift_score(&[
            "Alice is a software engineer.".into(),
            "Bob is a graphic designer.".into(),
        ])
        .unwrap();
        assert!(result.score > 0.4);
        assert!(result.above_threshold);
    }

    #[test]
    fn moderate_change_yields_moderate_score() {
        let result = compute_drift_score(&[
            "Alice works at OpenAI as a software engineer.".into(),
            "Alice works at Anthropic as a software engineer.".into(),
        ])
        .unwrap();
        // Just one word changed; normalized distance should be small.
        assert!(result.score < 0.30);
        assert!(!result.above_threshold);
    }

    #[test]
    fn frequency_boost_kicks_in_at_five_pairs() {
        // 6 versions = 5 pairs → frequency boost applies.
        let versions: Vec<String> = (0..6)
            .map(|i| format!("version {} of the entity content", i))
            .collect();
        let result = compute_drift_score(&versions).unwrap();
        // The diffs are small (just one digit changes), but with 5
        // pairs the boost multiplies by 1.2. Mostly a "doesn't crash
        // when freq_boost branch is taken" test.
        assert!(result.score > 0.0);
        assert_eq!(result.pair_distances.len(), 5);
    }

    #[test]
    fn empty_strings_handled_gracefully() {
        let result = compute_drift_score(&["".into(), "".into()]).unwrap();
        assert_eq!(result.score, 0.0);
    }

    #[test]
    fn one_empty_one_full_yields_max_distance_for_that_pair() {
        let result = compute_drift_score(&["".into(), "hello world".into()]).unwrap();
        assert!((result.pair_distances[0] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn record_drift_event_persists_row_with_open_status() {
        let conn = fresh_conn();
        let id = record_drift_event(
            &conn,
            "drift-1",
            "node-x",
            0.65,
            &["v1".into(), "v2".into(), "v3".into()],
            1_700_000_000_000,
        )
        .unwrap();
        assert_eq!(id, "drift-1");
        let (score, status, snapshots): (f64, String, String) = conn
            .query_row(
                "SELECT score, status, snapshot_version_ids
                 FROM drift_events WHERE id = ?1",
                ["drift-1"],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert!((score - 0.65).abs() < 1e-9);
        assert_eq!(status, "open");
        let arr: Vec<String> = serde_json::from_str(&snapshots).unwrap();
        assert_eq!(arr, vec!["v1", "v2", "v3"]);
    }

    #[test]
    fn list_open_events_returns_newest_first() {
        let conn = fresh_conn();
        record_drift_event(&conn, "d1", "n1", 0.4, &[], 100).unwrap();
        record_drift_event(&conn, "d2", "n1", 0.5, &[], 200).unwrap();
        record_drift_event(&conn, "d3", "n1", 0.6, &[], 300).unwrap();
        // Different node — shouldn't appear.
        record_drift_event(&conn, "d-other", "n2", 0.7, &[], 250).unwrap();

        let result = list_open_events_for_node(&conn, "n1", 10).unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0].0, "d3", "newest first");
        assert_eq!(result[1].0, "d2");
        assert_eq!(result[2].0, "d1");
    }

    #[test]
    fn list_open_events_respects_limit() {
        let conn = fresh_conn();
        for i in 0..5 {
            record_drift_event(&conn, &format!("d{}", i), "n1", 0.5, &[], 100 + i).unwrap();
        }
        let result = list_open_events_for_node(&conn, "n1", 2).unwrap();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn list_open_events_zero_limit_returns_empty() {
        let conn = fresh_conn();
        record_drift_event(&conn, "d1", "n1", 0.5, &[], 100).unwrap();
        let result = list_open_events_for_node(&conn, "n1", 0).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn list_open_events_filters_by_status() {
        let conn = fresh_conn();
        record_drift_event(&conn, "d1", "n1", 0.5, &[], 100).unwrap();
        conn.execute(
            "UPDATE drift_events SET status = 'resolved' WHERE id = 'd1'",
            [],
        )
        .unwrap();
        let result = list_open_events_for_node(&conn, "n1", 10).unwrap();
        assert!(result.is_empty(), "resolved events excluded");
    }

    // ─── R1 — scan_and_record_drift driver ────────────────────────

    fn seed_entity_node(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) \
             VALUES (?1, 'default', 'entity_page', ?2)",
            params![id, format!("title-{}", id)],
        )
        .unwrap();
    }

    fn seed_node_version(conn: &Connection, ver_id: &str, node_id: &str, content: &str, created_at: &str) {
        conn.execute(
            "INSERT INTO memory_versions (id, node_id, status, content, created_at) \
             VALUES (?1, ?2, 'active', ?3, ?4)",
            params![ver_id, node_id, content, created_at],
        )
        .unwrap();
    }

    #[test]
    fn scan_zero_batch_is_noop() {
        let conn = fresh_conn();
        let out = scan_and_record_drift(&conn, 0, 1_700_000_000_000).unwrap();
        assert_eq!(out, DriftScanOutcome { scanned: 0, flagged: 0 });
    }

    #[test]
    fn scan_flags_node_with_high_drift() {
        let conn = fresh_conn();
        seed_entity_node(&conn, "n-drift");
        // 3 wildly different versions → high drift.
        seed_node_version(&conn, "v1", "n-drift", "Alice is a software engineer at OpenAI.", "2026-05-01 00:00:00");
        seed_node_version(&conn, "v2", "n-drift", "Bob runs a bakery in Paris.", "2026-05-02 00:00:00");
        seed_node_version(&conn, "v3", "n-drift", "The quarterly revenue was 4 million dollars.", "2026-05-03 00:00:00");

        let out = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 1);
        assert_eq!(out.flagged, 1, "wildly-different versions should flag");

        let events = list_open_events_for_node(&conn, "n-drift", 10).unwrap();
        assert_eq!(events.len(), 1);
        assert!(events[0].1 >= DRIFT_THRESHOLD);
    }

    #[test]
    fn scan_skips_node_with_stable_versions() {
        let conn = fresh_conn();
        seed_entity_node(&conn, "n-stable");
        seed_node_version(&conn, "v1", "n-stable", "Alice is a software engineer at OpenAI.", "2026-05-01 00:00:00");
        seed_node_version(&conn, "v2", "n-stable", "Alice is a software engineer at OpenAI.", "2026-05-02 00:00:00");
        let out = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 1, "node is scanned");
        assert_eq!(out.flagged, 0, "identical versions don't flag");
    }

    #[test]
    fn scan_skips_single_version_nodes() {
        let conn = fresh_conn();
        seed_entity_node(&conn, "n-single");
        seed_node_version(&conn, "v1", "n-single", "only one version", "2026-05-01 00:00:00");
        let out = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 0, "single-version node below MIN_VERSIONS_FOR_DRIFT");
    }

    #[test]
    fn scan_skips_node_with_existing_open_event() {
        // Don't re-flag a node that already has an open drift signal.
        let conn = fresh_conn();
        seed_entity_node(&conn, "n-flagged");
        seed_node_version(&conn, "v1", "n-flagged", "Alice the engineer.", "2026-05-01 00:00:00");
        seed_node_version(&conn, "v2", "n-flagged", "Bob the baker entirely different.", "2026-05-02 00:00:00");
        // Pre-existing open event.
        record_drift_event(&conn, "pre-existing", "n-flagged", 0.9, &["v1".into()], 1).unwrap();

        let out = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 0, "node with open event is excluded from scan");
        // After resolving the open event, it becomes eligible again.
        conn.execute("UPDATE drift_events SET status = 'resolved' WHERE id = 'pre-existing'", []).unwrap();
        let out2 = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out2.scanned, 1, "resolved event re-opens eligibility");
        assert_eq!(out2.flagged, 1);
    }

    #[test]
    fn scan_only_considers_entity_page_kind() {
        let conn = fresh_conn();
        // A 'reference' node with drifting versions — should be ignored.
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n-ref', 'default', 'reference', 't')",
            [],
        ).unwrap();
        seed_node_version(&conn, "v1", "n-ref", "Alice engineer.", "2026-05-01 00:00:00");
        seed_node_version(&conn, "v2", "n-ref", "Bob baker different.", "2026-05-02 00:00:00");
        let out = scan_and_record_drift(&conn, 100, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 0, "non-entity_page kinds are not scanned");
    }

    #[test]
    fn scan_respects_batch_size() {
        let conn = fresh_conn();
        for i in 0..5 {
            let nid = format!("n-{}", i);
            seed_entity_node(&conn, &nid);
            seed_node_version(&conn, &format!("{}-v1", nid), &nid, "Alice the engineer here.", "2026-05-01 00:00:00");
            seed_node_version(&conn, &format!("{}-v2", nid), &nid, "Bob the baker totally different.", "2026-05-02 00:00:00");
        }
        let out = scan_and_record_drift(&conn, 2, 1_700_000_000_000).unwrap();
        assert_eq!(out.scanned, 2, "batch size caps scanned nodes");
    }

    #[test]
    fn list_open_drift_events_orders_by_score_and_filters_space() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n1','default','reference','Node One')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n2','default','reference','Node Two')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n3','other','reference','Other Space')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e1','n1',0.9,'[]',100,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e2','n2',0.4,'[]',101,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e3','n3',0.99,'[]',102,'open')", []).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e4','n1',0.5,'[]',103,'resolved')", []).unwrap();

        let rows = list_open_drift_events(&conn, "default", 100).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "e1");
        assert_eq!(rows[0].title, "Node One");
        assert_eq!(rows[1].id, "e2");
        assert!(list_open_drift_events(&conn, "default", 0).unwrap().is_empty());
    }

    #[test]
    fn resolve_drift_event_flips_status_and_removes_from_list() {
        let conn = fresh_conn();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title) VALUES ('n1','default','reference','N')",
            [],
        ).unwrap();
        conn.execute("INSERT INTO drift_events (id,node_id,score,snapshot_version_ids,computed_at,status) VALUES ('e1','n1',0.9,'[]',100,'open')", []).unwrap();
        resolve_drift_event(&conn, "e1", Some("looked fine"), 999).unwrap();
        assert!(list_open_drift_events(&conn, "default", 100).unwrap().is_empty());
        let (status, note, resolved): (String, Option<String>, Option<i64>) = conn
            .query_row(
                "SELECT status, resolution_note, resolved_at FROM drift_events WHERE id='e1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(status, "resolved");
        assert_eq!(note.as_deref(), Some("looked fine"));
        assert_eq!(resolved, Some(999));
    }
}
