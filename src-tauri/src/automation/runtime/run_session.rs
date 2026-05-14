//! Run-session lifecycle for automation runs (Phase 2a, design §0).
//!
//! A run IS an agent_session. This module owns: ensuring the shared
//! "Automations" home space exists, creating the per-run agent_session row
//! (with origin + prev_run chain metadata), persisting the loop transcript
//! into agent_messages, and pruning old run-sessions per spec.

use crate::agent::types::{ChatMessage, MessageRole};
use rusqlite::{Connection, OptionalExtension};

/// The fixed id of the auto-created shared "Automations" home space.
pub const AUTOMATIONS_SPACE_ID: &str = "automations";

/// Ensure the shared "Automations" space row exists (idempotent).
pub fn ensure_automations_space(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
         VALUES (?1, 'Automations', '🤖', NULL, datetime('now'), datetime('now'))",
        [AUTOMATIONS_SPACE_ID],
    )?;
    Ok(())
}

/// Resolve a spec's home space id: the spec's space_id if set, else the
/// shared "Automations" space.
pub fn resolve_home_space(conn: &Connection, spec_id: &str) -> rusqlite::Result<String> {
    let space_id: Option<String> = conn.query_row(
        "SELECT space_id FROM automation_specs WHERE id = ?1",
        [spec_id],
        |r| r.get(0),
    )?;
    Ok(space_id.filter(|s| !s.is_empty()).unwrap_or_else(|| AUTOMATIONS_SPACE_ID.to_string()))
}

/// Create the agent_session row for a run. `prev_run_session_id` chains
/// run history. Returns the new session id.
pub fn create_run_session(
    conn: &Connection,
    spec_id: &str,
    space_id: &str,
    trigger_tag: &str,
    activity_id: &str,
) -> rusqlite::Result<String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Find this spec's most recent prior run-session to chain from.
    // Query by metadata_json spec_id so chaining works even before the
    // caller updates automation_activities.session_id.
    let prev: Option<String> = conn.query_row(
        "SELECT s.id FROM agent_sessions s
         WHERE json_extract(s.metadata_json, '$.spec_id') = ?1
           AND s.id != ?2
         ORDER BY s.created_at DESC LIMIT 1",
        rusqlite::params![spec_id, session_id],
        |r| r.get(0),
    ).optional()?;

    let metadata = serde_json::json!({
        "origin": format!("automation:{}", trigger_tag),
        "spec_id": spec_id,
        "activity_id": activity_id,
        "prev_run_session_id": prev,
    });
    let title = format!("Automation run ({})", trigger_tag);

    conn.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        rusqlite::params![session_id, space_id, title, metadata.to_string(), now_ms],
    )?;
    Ok(session_id)
}

/// Persist a finished run's transcript into agent_messages (bulk, post-loop).
/// Must be called at most once per `session_id`; message ids are keyed as
/// `<session_id>-<idx>` so a second call would PK-collide (the D4 design
/// calls this exactly once, after the loop completes).
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
        let content = serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".into());
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

/// Prune old run-sessions for a spec, keeping the most recent `keep` runs.
/// Deletes the agent_messages + agent_session rows of older runs and NULLs
/// their automation_activities.session_id. The ledger row is never deleted.
/// Returns the number of run-sessions pruned.
pub fn prune_old_run_sessions(
    conn: &Connection,
    spec_id: &str,
    keep: u32,
) -> rusqlite::Result<usize> {
    // NOTE: run-sessions not linked via automation_activities.session_id are
    // not pruned (consistent with D4's link-after-create flow).
    let mut stmt = conn.prepare(
        "SELECT s.id FROM agent_sessions s
         JOIN automation_activities a ON a.session_id = s.id
         WHERE a.spec_id = ?1
         ORDER BY s.created_at DESC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([spec_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let to_prune = if ids.len() as u32 > keep {
        &ids[keep as usize..]
    } else {
        &[]
    };

    let tx = conn.unchecked_transaction()?;
    for sid in to_prune {
        tx.execute(
            "UPDATE automation_activities SET session_id = NULL WHERE session_id = ?1",
            [sid],
        )?;
        tx.execute("DELETE FROM agent_messages WHERE session_id = ?1", [sid])?;
        tx.execute("DELETE FROM agent_sessions WHERE id = ?1", [sid])?;
    }
    tx.commit()?;
    Ok(to_prune.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn insert_spec(conn: &Connection, id: &str, space_id: Option<&str>) {
        conn.execute(
            "INSERT INTO automation_specs
             (id, name, version, author, description, system_prompt,
              spec_yaml, spec_json, space_id, enabled, created_at, updated_at)
             VALUES (?1,'t','1.0','a','d','sys','','{}',?2,1,1,1)",
            rusqlite::params![id, space_id],
        ).unwrap();
    }

    #[test]
    fn ensure_automations_space_is_idempotent() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        ensure_automations_space(&conn).unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM spaces WHERE id = ?1",
            [AUTOMATIONS_SPACE_ID], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn resolve_home_space_uses_spec_space_else_automations() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        conn.execute(
            "INSERT INTO spaces (id, name, created_at, updated_at)
             VALUES ('proj', 'Project', datetime('now'), datetime('now'))", []).unwrap();
        insert_spec(&conn, "spec-with-space", Some("proj"));
        insert_spec(&conn, "spec-no-space", None);
        assert_eq!(resolve_home_space(&conn, "spec-with-space").unwrap(), "proj");
        assert_eq!(resolve_home_space(&conn, "spec-no-space").unwrap(), AUTOMATIONS_SPACE_ID);
    }

    #[test]
    fn create_run_session_chains_prev_run() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        let s1 = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-1").unwrap();
        let s2 = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-2").unwrap();
        assert_ne!(s1, s2);
        let meta: String = conn.query_row(
            "SELECT metadata_json FROM agent_sessions WHERE id = ?1",
            [&s2], |r| r.get(0)).unwrap();
        assert!(meta.contains(&s1), "s2 metadata should chain to s1");
        assert!(meta.contains("automation:manual"), "origin should be recorded");
    }

    #[test]
    fn persist_transcript_writes_agent_messages() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        let sid = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-1").unwrap();
        let msgs = vec![
            ChatMessage::user("trigger"),
            ChatMessage::assistant("did the thing"),
        ];
        persist_transcript(&conn, &sid, &msgs).unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
            [&sid], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn prune_keeps_most_recent_n_and_nulls_ledger_link() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        let mut sessions = vec![];
        for i in 0..3 {
            let act = format!("act-{}", i);
            conn.execute(
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at)
                 VALUES (?1, 's', 'manual', '{}', 'completed', ?2)",
                rusqlite::params![act, i as i64]).unwrap();
            let sid = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", &act).unwrap();
            conn.execute(
                "UPDATE automation_activities SET session_id = ?1 WHERE id = ?2",
                rusqlite::params![sid, act]).unwrap();
            persist_transcript(&conn, &sid, &[ChatMessage::user("x")]).unwrap();
            sessions.push(sid);
        }
        let pruned = prune_old_run_sessions(&conn, "s", 2).unwrap();
        assert_eq!(pruned, 1);
        let gone: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions WHERE id = ?1",
            [&sessions[0]], |r| r.get(0)).unwrap();
        assert_eq!(gone, 0);
        let link: Option<String> = conn.query_row(
            "SELECT session_id FROM automation_activities WHERE id = 'act-0'",
            [], |r| r.get(0)).unwrap();
        assert!(link.is_none(), "pruned run's ledger link should be NULL");
        let ledger_alive: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_activities WHERE id = 'act-0'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(ledger_alive, 1, "ledger row must never be deleted");
    }
}
