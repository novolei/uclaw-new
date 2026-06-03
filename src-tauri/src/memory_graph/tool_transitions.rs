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
}
