//! Index layer for per-(spec, identity) long-lived `automation:chat` sessions.
//!
//! Each spec can have multiple chat threads — one per identity. Identities:
//!   - "local"                   → spec owner
//!   - "app-chat:{spec_id}:{channel_type}:{chat_id}" → per-IM-user thread
//!
//! See spec: docs/superpowers/specs/2026-05-18-automation-phase2b-messaging-design.md

use anyhow::Result;
use rusqlite::Connection;

/// Build the canonical Halo-compatible identity key for an IM-triggered
/// automation chat thread.
pub fn automation_im_identity_key(spec_id: &str, channel_type: &str, chat_id: &str) -> String {
    format!("app-chat:{spec_id}:{channel_type}:{chat_id}")
}

/// Idempotently get-or-create the agent_session for this (spec_id, identity_key)
/// pair. Returns the agent_session id.
///
/// First call inserts a new agent_session with metadata
/// `{origin: "automation:chat", spec_id, identity_key}` and a row in
/// automation_chat_sessions. Subsequent calls return the existing id.
pub fn get_or_create_chat_session(
    conn: &Connection,
    spec_id: &str,
    identity_key: &str,
    space_id: &str,
) -> Result<String> {
    // Fast path: existing row.
    if let Some(id) = conn
        .query_row(
            "SELECT agent_session_id FROM automation_chat_sessions
             WHERE spec_id = ?1 AND identity_key = ?2",
            rusqlite::params![spec_id, identity_key],
            |r| r.get::<_, String>(0),
        )
        .ok()
    {
        return Ok(id);
    }

    // Create new agent_session + index row in one transaction so a race
    // between two concurrent fires doesn't leave a stranded session.
    let session_id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();
    let metadata = serde_json::json!({
        "origin": "automation:chat",
        "spec_id": spec_id,
        "identity_key": identity_key,
    });
    let title = format!("Chat · {} · {}", spec_id, identity_key);

    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        rusqlite::params![session_id, space_id, title, metadata.to_string(), now_ms],
    )?;
    let inserted = tx.execute(
        "INSERT OR IGNORE INTO automation_chat_sessions
         (spec_id, identity_key, agent_session_id, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?4)",
        rusqlite::params![spec_id, identity_key, session_id, now_ms],
    )?;

    if inserted == 0 {
        // Race: another fire won the insert. Drop our stranded agent_session
        // and return the winner's id.
        tx.execute(
            "DELETE FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
        )?;
        tx.commit()?;
        let winner: String = conn.query_row(
            "SELECT agent_session_id FROM automation_chat_sessions
             WHERE spec_id = ?1 AND identity_key = ?2",
            rusqlite::params![spec_id, identity_key],
            |r| r.get(0),
        )?;
        return Ok(winner);
    }

    tx.commit()?;
    Ok(session_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    #[test]
    fn get_or_create_chat_session_dedups_per_identity() {
        let conn = setup_db();
        let a = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let b = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        assert_eq!(a, b, "second call must return the same session id");

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions
                 WHERE spec_id='spec1' AND identity_key='local'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn get_or_create_chat_session_creates_distinct_for_different_identities() {
        let conn = setup_db();
        let local = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let im_a_key = automation_im_identity_key("spec1", "wechat_ilink", "UIN_a");
        let im_b_key = automation_im_identity_key("spec1", "wechat_ilink", "UIN_b");
        let im_a = get_or_create_chat_session(&conn, "spec1", &im_a_key, "default").unwrap();
        let im_b = get_or_create_chat_session(&conn, "spec1", &im_b_key, "default").unwrap();

        assert_ne!(local, im_a);
        assert_ne!(local, im_b);
        assert_ne!(im_a, im_b);

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions WHERE spec_id='spec1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3);
    }

    #[test]
    fn get_or_create_chat_session_writes_chat_origin_metadata() {
        let conn = setup_db();
        let id = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        let meta: String = conn
            .query_row(
                "SELECT metadata_json FROM agent_sessions WHERE id = ?1",
                rusqlite::params![id],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&meta).unwrap();
        assert_eq!(v["origin"], "automation:chat");
        assert_eq!(v["spec_id"], "spec1");
        assert_eq!(v["identity_key"], "local");
    }

    #[test]
    fn automation_im_identity_key_is_app_scoped() {
        assert_eq!(
            automation_im_identity_key("spec1", "wechat_ilink", "UIN_a"),
            "app-chat:spec1:wechat_ilink:UIN_a"
        );
        assert_eq!(
            automation_im_identity_key("spec1", "unknown", "chat:with:colon"),
            "app-chat:spec1:unknown:chat:with:colon"
        );
    }

    #[test]
    fn cascade_on_agent_session_delete_clears_index_row() {
        let conn = setup_db();
        let id = get_or_create_chat_session(&conn, "spec1", "local", "default").unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        conn.execute("DELETE FROM agent_sessions WHERE id = ?1", rusqlite::params![id]).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM automation_chat_sessions WHERE spec_id='spec1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 0, "FK CASCADE should have cleared the index row");
    }
}
