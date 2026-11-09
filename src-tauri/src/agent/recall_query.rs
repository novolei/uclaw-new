//! Recall query for an agent turn (context continuity P3, Decision 16).
//!
//! A bare "this turn's text" query misses follow-ups like "继续" / "do the
//! same for the other file". The recall query is this turn's text followed
//! by the last [`PRIOR_USER_MESSAGES`] user messages of the session
//! (newest → oldest), each cut to [`PRIOR_MAX_CHARS`] characters, joined
//! with `"\n"` and capped at [`QUERY_MAX_CHARS`] characters in total.
//!
//! Prior messages are read from `agent_messages` (the visible conversation),
//! which never carries the per-turn `<turn_context>` block, so no stripping
//! is needed.

use rusqlite::Connection;

/// How many earlier user messages join the recall query.
pub const PRIOR_USER_MESSAGES: usize = 2;
/// Per-message character cap for the earlier user messages.
pub const PRIOR_MAX_CHARS: usize = 200;
/// Character cap for the whole query.
pub const QUERY_MAX_CHARS: usize = 1200;

/// This turn's text first, then `prior_user_texts` in the given order
/// (callers pass newest → oldest), `"\n"`-joined. Character-based cuts
/// never split a UTF-8 code point. Blank parts are skipped.
pub fn build_recall_query(current: &str, prior_user_texts: &[String]) -> String {
    let mut parts: Vec<String> = Vec::with_capacity(1 + PRIOR_USER_MESSAGES);
    let current = current.trim();
    if !current.is_empty() {
        parts.push(current.to_string());
    }
    for prior in prior_user_texts.iter().take(PRIOR_USER_MESSAGES) {
        let prior = prior.trim();
        if prior.is_empty() {
            continue;
        }
        parts.push(take_chars(prior, PRIOR_MAX_CHARS));
    }
    take_chars(&parts.join("\n"), QUERY_MAX_CHARS)
}

/// The session's last `n` user messages from `agent_messages`, newest first,
/// excluding `exclude_message_id` (this turn's row). Best-effort: a DB error
/// yields an empty list (recall falls back to this turn's text alone).
pub fn recent_user_texts(
    conn: &Connection,
    session_id: &str,
    exclude_message_id: &str,
    n: usize,
) -> Vec<String> {
    if n == 0 {
        return Vec::new();
    }
    let result = (|| -> rusqlite::Result<Vec<String>> {
        let mut stmt = conn.prepare(
            "SELECT content FROM agent_messages \
             WHERE session_id = ?1 AND role = 'user' AND id != ?2 \
             ORDER BY created_at DESC, rowid DESC LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![session_id, exclude_message_id, n as i64],
            |row| row.get::<_, String>(0),
        )?;
        rows.collect()
    })();
    match result {
        Ok(texts) => texts,
        Err(e) => {
            tracing::warn!(session_id, error = %e, "recall query: recent user messages unavailable");
            Vec::new()
        }
    }
}

fn take_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES ('s','global','t',0,0)",
            [],
        )
        .ok();
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at) VALUES ('other','global','t',0,0)",
            [],
        )
        .ok();
        conn
    }

    fn row(conn: &Connection, id: &str, session: &str, role: &str, content: &str, at: i64) {
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1,?2,?3,?4,?5)",
            rusqlite::params![id, session, role, content, at],
        )
        .unwrap();
    }

    #[test]
    fn recall_query_includes_recent_user_messages_truncated() {
        let long = "x".repeat(500);
        let q = build_recall_query(
            "now",
            &["newer".to_string(), long.clone(), "third-is-dropped".to_string()],
        );
        let lines: Vec<&str> = q.split('\n').collect();
        assert_eq!(lines.len(), 3, "current + PRIOR_USER_MESSAGES: {q:?}");
        assert_eq!(lines[0], "now");
        assert_eq!(lines[1], "newer");
        assert_eq!(lines[2].chars().count(), PRIOR_MAX_CHARS);
        assert!(!q.contains("third-is-dropped"));
    }

    #[test]
    fn recall_query_current_only_when_no_history() {
        assert_eq!(build_recall_query("  hello world ", &[]), "hello world");
        assert_eq!(build_recall_query("hi", &["   ".to_string()]), "hi");
    }

    #[test]
    fn recall_query_respects_total_cap_on_cjk() {
        let current = "记".repeat(1500);
        let q = build_recall_query(&current, &["之前的问题".to_string()]);
        assert_eq!(q.chars().count(), QUERY_MAX_CHARS);
        assert!(q.chars().all(|c| c == '记'), "cap cuts on char boundaries");

        let current = "忆".repeat(1100);
        let prior = "旧".repeat(300);
        let q = build_recall_query(&current, &[prior]);
        assert_eq!(q.chars().count(), QUERY_MAX_CHARS);
        assert!(q.starts_with(&current), "this turn's text comes first");
    }

    #[test]
    fn recent_user_texts_excludes_current_and_orders_newest_first() {
        let conn = db();
        row(&conn, "u1", "s", "user", "first", 100);
        row(&conn, "a1", "s", "assistant", "reply", 150);
        row(&conn, "u2", "s", "user", "second", 200);
        row(&conn, "sys", "s", "system", "skill prompt", 299);
        row(&conn, "u3", "s", "user", "third", 300);
        row(&conn, "cur", "s", "user", "current", 400);
        row(&conn, "x", "other", "user", "other session", 500);

        let texts = recent_user_texts(&conn, "s", "cur", PRIOR_USER_MESSAGES);
        assert_eq!(texts, vec!["third".to_string(), "second".to_string()]);
        assert!(recent_user_texts(&conn, "s", "cur", 0).is_empty());
        assert!(recent_user_texts(&conn, "missing", "cur", 2).is_empty());
    }
}
