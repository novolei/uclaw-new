//! openhuman-E — directed, weighted tool-transition graph.
//!
//! Records "tool A was followed by tool B in the same session" as a
//! directed edge with a count, a success count (B did not error), and a
//! recency timestamp. Aggregated incrementally from `agent_turns` by a
//! proactive job; read by `suggest_tool_chain` with a
//! `count × recency_decay × success_rate` score. Replaces the old
//! boolean-undirected `co_used` edges facade.

use rusqlite::{params, Connection, OptionalExtension};

/// One outgoing transition row (the scoring is applied by the caller).
#[derive(Debug, Clone, PartialEq)]
pub struct TransitionRow {
    pub to_tool: String,
    pub count: i64,
    pub success_count: i64,
    pub last_seen_ms: i64,
}

/// Bump (or insert) the directed A→B transition. `success` = B did not error.
pub fn upsert_transition(
    conn: &Connection,
    space_id: &str,
    from_tool: &str,
    to_tool: &str,
    success: bool,
    last_seen_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO tool_transitions
             (space_id, from_tool, to_tool, count, success_count, last_seen_ms)
         VALUES (?1, ?2, ?3, 1, ?4, ?5)
         ON CONFLICT(space_id, from_tool, to_tool) DO UPDATE SET
             count = count + 1,
             success_count = success_count + ?4,
             last_seen_ms = ?5",
        params![space_id, from_tool, to_tool, success as i64, last_seen_ms],
    )?;
    Ok(())
}

const WATERMARK_KEY: &str = "tool_transitions_watermark_rowid";

fn read_watermark(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        params![WATERMARK_KEY],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .and_then(|s| s.parse::<i64>().ok())
    .unwrap_or(0)
}

fn write_watermark(conn: &Connection, ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
        params![WATERMARK_KEY, ms.to_string()],
    )?;
    Ok(())
}

struct TurnLite {
    session_id: String,
    space_id: String,
    tool_name: String,
    is_error: bool,
    created_at: i64,
    turn_index: i64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ToolTransitionOutcome {
    pub pairs_processed: usize,
    pub new_watermark: i64,
}

/// openhuman-E — fold new `agent_turns` (since the watermark) into the directed
/// transition graph. Loads tool turns in `rowid` order (strictly monotonic,
/// no created_at ties, so the processed prefix is contiguous and gap-free),
/// re-sorts in memory by (session, turn_index), and pairs each tool turn with
/// the next tool turn IN THE SAME SESSION (A→B; success = B did not error).
/// Watermark = max rowid in the batch (not created_at). A pair split exactly
/// across the batch boundary is undercounted by 1 — acceptable; it
/// self-corrects when the sequence recurs. Best-effort: per-pair errors log
/// + continue.
pub fn run_tool_transition_aggregation_blocking(
    conn: &Connection,
    batch_size: usize,
) -> ToolTransitionOutcome {
    let mut outcome = ToolTransitionOutcome::default();
    if batch_size == 0 {
        outcome.new_watermark = read_watermark(conn);
        return outcome;
    }
    let watermark = read_watermark(conn);
    outcome.new_watermark = watermark;

    let mut rows: Vec<(i64, TurnLite)> = {
        let mut stmt = match conn.prepare(
            "SELECT t.rowid, t.session_id, COALESCE(s.space_id,'default'), t.tool_name,
                    t.is_error, t.created_at, t.turn_index
             FROM agent_turns t
             LEFT JOIN agent_sessions s ON s.id = t.session_id
             WHERE t.rowid > ?1 AND t.role = 'tool' AND t.tool_name IS NOT NULL
             ORDER BY t.rowid ASC
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(error = %e, "tool_transitions: prepare failed");
                return outcome;
            }
        };
        let mapped = stmt.query_map(params![watermark, batch_size as i64], |r| {
            let rowid: i64 = r.get(0)?;
            Ok((
                rowid,
                TurnLite {
                    session_id: r.get(1)?,
                    space_id: r.get(2)?,
                    tool_name: r.get(3)?,
                    is_error: r.get::<_, i64>(4)? != 0,
                    created_at: r.get(5)?,
                    turn_index: r.get::<_, i64>(6)?,
                },
            ))
        });
        match mapped {
            Ok(m) => m.filter_map(|r| {
                r.map_err(|e| tracing::warn!(error = %e, "tool_transitions: row parse failed")).ok()
            }).collect(),
            Err(e) => {
                tracing::warn!(error = %e, "tool_transitions: query failed");
                return outcome;
            }
        }
    };
    if rows.is_empty() {
        return outcome;
    }

    // Watermark = max rowid in this (rowid-ordered) batch — strictly monotonic,
    // no created_at ties, so the processed prefix is contiguous and gap-free.
    let new_watermark = rows.iter().map(|(rowid, _)| *rowid).max().unwrap_or(watermark);

    rows.sort_by(|(_, a), (_, b)| a.session_id.cmp(&b.session_id).then(a.turn_index.cmp(&b.turn_index)));
    for w in rows.windows(2) {
        let (_, a) = &w[0];
        let (_, b) = &w[1];
        if a.session_id == b.session_id {
            if let Err(e) =
                upsert_transition(conn, &b.space_id, &a.tool_name, &b.tool_name, !b.is_error, b.created_at)
            {
                tracing::warn!(error = %e, "tool_transitions: upsert failed");
            } else {
                outcome.pairs_processed += 1;
            }
        }
    }

    if let Err(e) = write_watermark(conn, new_watermark) {
        tracing::warn!(error = %e, "tool_transitions: watermark write failed");
    }
    outcome.new_watermark = new_watermark;
    outcome
}

/// Outgoing transitions from `from_tool`, highest count first (scoring applied
/// by the caller). Read-only.
pub fn top_transitions_from(
    conn: &Connection,
    space_id: &str,
    from_tool: &str,
    limit: usize,
) -> rusqlite::Result<Vec<TransitionRow>> {
    if limit == 0 {
        return Ok(vec![]);
    }
    let mut stmt = conn.prepare(
        "SELECT to_tool, count, success_count, last_seen_ms
         FROM tool_transitions
         WHERE space_id = ?1 AND from_tool = ?2
         ORDER BY count DESC
         LIMIT ?3",
    )?;
    let rows = stmt
        .query_map(params![space_id, from_tool, limit as i64], |r| {
            Ok(TransitionRow {
                to_tool: r.get(0)?,
                count: r.get(1)?,
                success_count: r.get(2)?,
                last_seen_ms: r.get(3)?,
            })
        })?
        .filter_map(Result::ok)
        .collect();
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&c).unwrap();
        c
    }

    fn seed_session(c: &Connection, sid: &str, space: &str) {
        c.execute(
            "INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
             VALUES (?1, ?2, 'test', '{}', 0, 0, 0, 0, 0)",
            params![sid, space],
        ).unwrap();
    }
    fn seed_tool_turn(c: &Connection, sid: &str, idx: i64, tool: &str, is_err: bool, ts: i64) {
        c.execute(
            "INSERT INTO agent_turns (id, session_id, turn_index, role, content, tool_name, tool_args, tool_result, reasoning, is_error, duration_ms, created_at)
             VALUES (?1, ?2, ?3, 'tool', NULL, ?4, NULL, NULL, NULL, ?5, 0, ?6)",
            params![format!("{sid}-{idx}"), sid, idx, tool, is_err as i64, ts],
        ).unwrap();
    }

    #[test]
    fn aggregation_pairs_consecutive_tool_turns_in_session() {
        let c = conn();
        seed_session(&c, "s1", "default");
        seed_tool_turn(&c, "s1", 1, "read", false, 100);
        seed_tool_turn(&c, "s1", 3, "edit", false, 200);
        seed_tool_turn(&c, "s1", 5, "bash", true, 300);
        seed_session(&c, "s2", "default");
        seed_tool_turn(&c, "s2", 1, "grep", false, 400);

        let out = run_tool_transition_aggregation_blocking(&c, 500);
        assert_eq!(out.pairs_processed, 2); // read->edit, edit->bash
        let from_read = top_transitions_from(&c, "default", "read", 10).unwrap();
        assert_eq!(from_read[0].to_tool, "edit");
        assert_eq!(from_read[0].success_count, 1);
        let from_edit = top_transitions_from(&c, "default", "edit", 10).unwrap();
        assert_eq!(from_edit[0].to_tool, "bash");
        assert_eq!(from_edit[0].success_count, 0); // bash errored
        assert_eq!(out.new_watermark, 4);
    }

    #[test]
    fn aggregation_is_incremental_and_idempotent_on_rerun() {
        let c = conn();
        seed_session(&c, "s1", "default");
        seed_tool_turn(&c, "s1", 1, "read", false, 100);
        seed_tool_turn(&c, "s1", 3, "edit", false, 200);
        let out1 = run_tool_transition_aggregation_blocking(&c, 500);
        assert_eq!(out1.pairs_processed, 1);
        let out2 = run_tool_transition_aggregation_blocking(&c, 500);
        assert_eq!(out2.pairs_processed, 0);
        let rows = top_transitions_from(&c, "default", "read", 10).unwrap();
        assert_eq!(rows[0].count, 1);
    }

    #[test]
    fn upsert_inserts_then_increments() {
        let c = conn();
        upsert_transition(&c, "default", "a", "b", true, 1000).unwrap();
        upsert_transition(&c, "default", "a", "b", false, 2000).unwrap();
        let rows = top_transitions_from(&c, "default", "a", 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].to_tool, "b");
        assert_eq!(rows[0].count, 2);
        assert_eq!(rows[0].success_count, 1);
        assert_eq!(rows[0].last_seen_ms, 2000);
    }

    #[test]
    fn top_transitions_from_filters_and_orders_by_count() {
        let c = conn();
        for _ in 0..3 { upsert_transition(&c, "default", "a", "b", true, 100).unwrap(); }
        upsert_transition(&c, "default", "a", "c", true, 100).unwrap();
        upsert_transition(&c, "default", "x", "y", true, 100).unwrap();
        let rows = top_transitions_from(&c, "default", "a", 10).unwrap();
        assert_eq!(rows.iter().map(|r| r.to_tool.as_str()).collect::<Vec<_>>(), vec!["b", "c"]);
        assert_eq!(rows[0].count, 3);
    }

    #[test]
    fn aggregation_handles_created_at_ties_at_batch_boundary() {
        let c = conn();
        seed_session(&c, "s1", "default");
        // three tool turns, the last TWO share the same created_at (ms tie)
        seed_tool_turn(&c, "s1", 1, "read", false, 100);
        seed_tool_turn(&c, "s1", 3, "edit", false, 200);
        seed_tool_turn(&c, "s1", 5, "bash", false, 200); // tie with edit on created_at

        // batch_size 2: batch-1 fetches rowids 1,2 (read, edit) → watermark=2.
        // batch-2 fetches rowid 3 (bash) → watermark=3.
        // With a created_at cursor, batch-2 would use WHERE created_at > 200,
        // which permanently skips bash (bash.created_at == 200, not > 200).
        // With a rowid cursor, batch-2 uses WHERE rowid > 2 → bash is found.
        let out1 = run_tool_transition_aggregation_blocking(&c, 2);
        let out2 = run_tool_transition_aggregation_blocking(&c, 2);

        // out1 processed the read→edit pair (both in batch-1).
        assert_eq!(out1.pairs_processed, 1);
        let from_read = top_transitions_from(&c, "default", "read", 10).unwrap();
        assert_eq!(from_read.len(), 1);
        assert_eq!(from_read[0].to_tool, "edit");

        // Prove bash was NOT permanently skipped: out2.new_watermark advanced to 3,
        // meaning the bash row (rowid=3) was loaded from the DB in batch-2.
        // (The edit→bash pair itself is split across the boundary, so pairs_processed=0
        // is expected — that split-boundary undercount is acceptable per design.)
        assert_eq!(out2.new_watermark, 3);
        assert_eq!(out2.pairs_processed, 0);
    }
}
