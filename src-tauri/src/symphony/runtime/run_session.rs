//! Per-node session lifecycle for Symphony runs.
//!
//! Mirrors `automation::runtime::run_session`: a Symphony node's transcript
//! lives in `agent_messages` keyed by its `agent_sessions.id` (set up here),
//! and the home space defaults to a shared `'symphonies'` row in `spaces`.
//! The `metadata_json` field on `agent_sessions` is tagged with
//! `origin = "symphony:<node_id>"` so:
//!  - the cost dashboard can roll Symphony spend (`runtime/cost.rs`),
//!  - the existing Agent view can open any node's transcript by session id,
//!  - the automation retention pruner won't touch Symphony rows.

use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
use rusqlite::{Connection, OptionalExtension};

/// Fixed id of the auto-created shared "Symphonies" home space.
pub const SYMPHONIES_SPACE_ID: &str = "symphonies";

/// Ensure the shared "Symphonies" space row exists (idempotent).
///
/// Schema V33 also seeds this row, but we keep this helper around because:
/// (a) it lets unit tests skip V33 if they want, and (b) it documents the
/// invariant the executor relies on.
pub fn ensure_symphonies_space(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
         VALUES (?1, 'Symphonies', '🎼', NULL, datetime('now'), datetime('now'))",
        [SYMPHONIES_SPACE_ID],
    )?;
    Ok(())
}

/// Resolve a workflow's home space id: explicit `space_id` on the workflow
/// if set, else the shared `SYMPHONIES_SPACE_ID`.
pub fn resolve_home_space(
    conn: &Connection,
    workflow_id: &str,
) -> rusqlite::Result<String> {
    let space_id: Option<String> = conn
        .query_row(
            "SELECT space_id FROM symphony_workflows WHERE id = ?1",
            [workflow_id],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    Ok(space_id
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| SYMPHONIES_SPACE_ID.to_string()))
}

/// Create the `agent_sessions` row for one node attempt. Returns the
/// generated session id. Metadata is JSON-encoded with origin tag, workflow
/// id, run id, and node id so downstream consumers (cost dashboard, agent
/// UI, recovery) can attribute rows back to a Symphony run without joining
/// extra tables.
pub fn create_node_session(
    conn: &Connection,
    space_id: &str,
    workflow_id: &str,
    run_id: &str,
    node_id: &str,
    attempt: i64,
) -> rusqlite::Result<String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Chain to the most recent prior node-session for the SAME (workflow, node)
    // so retried attempts surface as a thread in the Agent view.
    let prev: Option<String> = conn
        .query_row(
            "SELECT s.id FROM agent_sessions s
             WHERE json_extract(s.metadata_json, '$.workflow_id') = ?1
               AND json_extract(s.metadata_json, '$.node_id')     = ?2
               AND s.id != ?3
             ORDER BY s.created_at DESC LIMIT 1",
            rusqlite::params![workflow_id, node_id, session_id],
            |r| r.get(0),
        )
        .optional()?;

    let metadata = serde_json::json!({
        "origin": format!("symphony:{}", node_id),
        "workflow_id": workflow_id,
        "run_id": run_id,
        "node_id": node_id,
        "attempt": attempt,
        "prev_run_session_id": prev,
    });
    let title = format!("Symphony · {} · {}", workflow_id, node_id);

    conn.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        rusqlite::params![session_id, space_id, title, metadata.to_string(), now_ms],
    )?;
    Ok(session_id)
}

/// Persist a finished node's transcript into `agent_messages`. One call per
/// session (`<session_id>-<idx>` PK matches `automation::run_session`).
pub fn persist_transcript(
    conn: &Connection,
    session_id: &str,
    messages: &[ChatMessage],
) -> rusqlite::Result<()> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    for (idx, msg) in messages.iter().enumerate() {
        let role = match msg.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        };
        // Same per-role serialization as automation: user/system messages
        // collapse text content blocks into a flat string; assistant uses
        // the full ContentBlock JSON so thinking/tool_use/tool_result render.
        let content = match msg.role {
            MessageRole::User | MessageRole::System => msg
                .content
                .iter()
                .filter_map(|b| match b {
                    ContentBlock::Text { text } => Some(text.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n"),
            MessageRole::Assistant => {
                serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".into())
            }
        };
        let id = format!("{}-{}", session_id, idx);
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, session_id, role, content, now_ms + idx as i64],
        )?;
    }
    conn.execute(
        "UPDATE agent_sessions SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![messages.len() as i64, now_ms, session_id],
    )?;
    Ok(())
}

/// Prune old runs for a workflow, keeping the most recent `keep` runs.
/// Deletes the runs + their node-runs + the per-node agent_messages /
/// agent_sessions rows. The workflow itself and its versions are never
/// touched here.
pub fn prune_old_runs(
    conn: &Connection,
    workflow_id: &str,
    keep: u32,
) -> rusqlite::Result<usize> {
    let mut stmt = conn.prepare(
        "SELECT id FROM symphony_runs WHERE workflow_id = ?1 ORDER BY queued_at DESC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([workflow_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    let to_prune = if ids.len() as u32 > keep {
        &ids[keep as usize..]
    } else {
        &[]
    };
    let tx = conn.unchecked_transaction()?;
    for run in to_prune {
        // Collect session ids first so we can delete dependent rows.
        let session_ids: Vec<String> = {
            let mut s = tx.prepare(
                "SELECT session_id FROM symphony_node_runs \
                 WHERE run_id = ?1 AND session_id IS NOT NULL",
            )?;
            let rows = s.query_map([run], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        };
        for sid in &session_ids {
            tx.execute("DELETE FROM agent_messages WHERE session_id = ?1", [sid])?;
            tx.execute("DELETE FROM agent_sessions WHERE id = ?1", [sid])?;
        }
        // FK on symphony_node_runs.run_id cascades to drop node-runs.
        tx.execute("DELETE FROM symphony_runs WHERE id = ?1", [run])?;
    }
    tx.commit()?;
    Ok(to_prune.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::ChatMessage;
    use rusqlite::params;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn
    }

    fn insert_workflow(conn: &Connection, id: &str, space: Option<&str>) {
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO symphony_workflows \
             (id, name, description, space_id, current_version, enabled, created_at, updated_at) \
             VALUES (?1, 'demo', NULL, ?2, 1, 1, ?3, ?3)",
            params![id, space, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_workflow_versions \
             (workflow_id, version, definition_yaml, definition_md, nodes_json, edges_json, created_at) \
             VALUES (?1, 1, 'y', 'm', '[]', '[]', ?2)",
            params![id, now],
        )
        .unwrap();
    }

    fn insert_run(conn: &Connection, run_id: &str, workflow_id: &str) {
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO symphony_runs \
             (id, workflow_id, workflow_version, trigger_kind, status, queued_at) \
             VALUES (?1, ?2, 1, 'manual', 'running', ?3)",
            params![run_id, workflow_id, now],
        )
        .unwrap();
    }

    #[test]
    fn ensure_symphonies_space_is_idempotent() {
        let conn = db();
        ensure_symphonies_space(&conn).unwrap();
        ensure_symphonies_space(&conn).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM spaces WHERE id = ?1",
                [SYMPHONIES_SPACE_ID],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn resolve_home_space_uses_workflow_override() {
        let conn = db();
        conn.execute(
            "INSERT INTO spaces (id, name, created_at, updated_at) \
             VALUES ('proj', 'Project', datetime('now'), datetime('now'))",
            [],
        )
        .unwrap();
        insert_workflow(&conn, "wf-proj", Some("proj"));
        insert_workflow(&conn, "wf-bare", None);
        assert_eq!(resolve_home_space(&conn, "wf-proj").unwrap(), "proj");
        assert_eq!(
            resolve_home_space(&conn, "wf-bare").unwrap(),
            SYMPHONIES_SPACE_ID
        );
    }

    #[test]
    fn create_node_session_chains_prior_attempt() {
        let conn = db();
        insert_workflow(&conn, "wf", None);
        insert_run(&conn, "run-1", "wf");
        let s1 = create_node_session(&conn, SYMPHONIES_SPACE_ID, "wf", "run-1", "node-a", 1).unwrap();
        let s2 = create_node_session(&conn, SYMPHONIES_SPACE_ID, "wf", "run-1", "node-a", 2).unwrap();
        assert_ne!(s1, s2);
        let meta: String = conn
            .query_row(
                "SELECT metadata_json FROM agent_sessions WHERE id = ?1",
                [&s2],
                |r| r.get(0),
            )
            .unwrap();
        assert!(meta.contains(&s1), "second attempt should chain to first");
        assert!(meta.contains("\"origin\":\"symphony:node-a\""));
    }

    #[test]
    fn persist_transcript_inserts_one_message_per_chat_message() {
        let conn = db();
        insert_workflow(&conn, "wf", None);
        insert_run(&conn, "run-1", "wf");
        let sid =
            create_node_session(&conn, SYMPHONIES_SPACE_ID, "wf", "run-1", "node-a", 1).unwrap();
        let msgs = vec![
            ChatMessage::user("hello"),
            ChatMessage::assistant("hi there"),
        ];
        persist_transcript(&conn, &sid, &msgs).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
                [&sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);
        let count: i64 = conn
            .query_row(
                "SELECT message_count FROM agent_sessions WHERE id = ?1",
                [&sid],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn prune_keeps_most_recent_and_drops_their_sessions() {
        let conn = db();
        insert_workflow(&conn, "wf", None);
        // Three runs, each with one node session.
        let mut sessions = Vec::new();
        for i in 0..3 {
            let run = format!("run-{}", i);
            insert_run(&conn, &run, "wf");
            let sid = create_node_session(&conn, SYMPHONIES_SPACE_ID, "wf", &run, "node", 1)
                .unwrap();
            conn.execute(
                "INSERT INTO symphony_node_runs \
                 (id, run_id, node_id, attempt, status, session_id) \
                 VALUES (?1, ?2, 'node', 1, 'succeeded', ?3)",
                params![format!("nr-{}", i), run, sid],
            )
            .unwrap();
            persist_transcript(&conn, &sid, &[ChatMessage::user("x")]).unwrap();
            sessions.push(sid);
            std::thread::sleep(std::time::Duration::from_millis(2));
            // Make queued_at distinct so ORDER BY is deterministic.
            conn.execute(
                "UPDATE symphony_runs SET queued_at = queued_at + ?1 WHERE id = ?2",
                params![i as i64, run],
            )
            .unwrap();
        }
        let pruned = prune_old_runs(&conn, "wf", 2).unwrap();
        assert_eq!(pruned, 1);
        let remaining_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(remaining_runs, 2);
        // The pruned run's session is gone.
        let gone: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_sessions WHERE id = ?1",
                [&sessions[0]],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(gone, 0);
    }
}
