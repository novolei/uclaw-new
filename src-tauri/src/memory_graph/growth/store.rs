//! SQLite access for the growth tables (V65). Timestamps are epoch ms.

use rusqlite::{params, Connection, OptionalExtension as _};
use serde::Serialize;

/// The single user-model row id.
const USER_MODEL_ID: &str = "default";

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReflectionRow {
    pub id: String,
    pub insight: String,
    pub confidence: f64,
    pub created_at: i64,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptMessage {
    pub role: String,
    pub content: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModel {
    pub summary: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UserModelHistoryRow {
    pub summary: String,
    pub replaced_at: i64,
}

/// Insert a reflection; returns its id.
pub fn insert_reflection(
    conn: &Connection,
    insight: &str,
    confidence: f64,
    source_message_count: i64,
    watermark_ms: i64,
) -> anyhow::Result<String> {
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO growth_reflections (id, insight, confidence, source_message_count, watermark_ms, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, insight, confidence.clamp(0.0, 1.0), source_message_count, watermark_ms, now_ms()],
    )?;
    Ok(id)
}

/// Newest first; archived rows only when `include_archived`.
pub fn list_reflections(conn: &Connection, limit: usize, include_archived: bool) -> anyhow::Result<Vec<ReflectionRow>> {
    let sql = if include_archived {
        "SELECT id, insight, confidence, created_at, archived_at FROM growth_reflections
         ORDER BY created_at DESC, rowid DESC LIMIT ?1"
    } else {
        "SELECT id, insight, confidence, created_at, archived_at FROM growth_reflections
         WHERE archived_at IS NULL ORDER BY created_at DESC, rowid DESC LIMIT ?1"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(ReflectionRow {
                id: r.get(0)?,
                insight: r.get(1)?,
                confidence: r.get(2)?,
                created_at: r.get(3)?,
                archived_at: r.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

/// Archive (`true`) or restore (`false`) one reflection; returns whether it existed.
pub fn set_reflection_archived(conn: &Connection, id: &str, archived: bool) -> anyhow::Result<bool> {
    let n = if archived {
        conn.execute(
            "UPDATE growth_reflections SET archived_at = COALESCE(archived_at, ?2) WHERE id = ?1",
            params![id, now_ms()],
        )?
    } else {
        conn.execute("UPDATE growth_reflections SET archived_at = NULL WHERE id = ?1", [id])?
    };
    Ok(n > 0)
}

/// Archive exactly these reflections (consolidation's input set).
pub fn archive_reflections(conn: &Connection, ids: &[String]) -> anyhow::Result<()> {
    let now = now_ms();
    for id in ids {
        conn.execute(
            "UPDATE growth_reflections SET archived_at = ?2 WHERE id = ?1 AND archived_at IS NULL",
            params![id, now],
        )?;
    }
    Ok(())
}

/// Newest message any reflection has read (archived ones included); 0 if none.
pub fn latest_watermark(conn: &Connection) -> anyhow::Result<i64> {
    Ok(conn.query_row("SELECT COALESCE(MAX(watermark_ms), 0) FROM growth_reflections", [], |r| r.get(0))?)
}

/// Assistant `agent_messages` created after `after_ms` — the trigger count.
pub fn count_assistant_messages_since(conn: &Connection, after_ms: i64) -> anyhow::Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM agent_messages WHERE role = 'assistant' AND created_at > ?1",
        [after_ms],
        |r| r.get(0),
    )?)
}

/// The last `limit` user/assistant messages after `after_ms`, oldest first.
pub fn messages_since(conn: &Connection, after_ms: i64, limit: usize) -> anyhow::Result<Vec<TranscriptMessage>> {
    let mut stmt = conn.prepare(
        "SELECT role, content, created_at FROM agent_messages
         WHERE created_at > ?1 AND role IN ('user', 'assistant')
         ORDER BY created_at DESC, rowid DESC LIMIT ?2",
    )?;
    let mut rows = stmt
        .query_map(params![after_ms, limit as i64], |r| {
            Ok(TranscriptMessage { role: r.get(0)?, content: r.get(1)?, created_at: r.get(2)? })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    rows.reverse();
    Ok(rows)
}

pub fn get_user_model(conn: &Connection) -> anyhow::Result<Option<UserModel>> {
    Ok(conn
        .query_row(
            "SELECT summary, updated_at FROM growth_user_model WHERE id = ?1",
            [USER_MODEL_ID],
            |r| Ok(UserModel { summary: r.get(0)?, updated_at: r.get(1)? }),
        )
        .optional()?)
}

/// Upsert the user model. When a different summary existed, it moves to
/// history first. Returns whether the summary changed.
pub fn set_user_model(conn: &Connection, summary: &str) -> anyhow::Result<bool> {
    let now = now_ms();
    if let Some(prev) = get_user_model(conn)? {
        if prev.summary == summary {
            return Ok(false);
        }
        conn.execute(
            "INSERT INTO growth_user_model_history (id, summary, replaced_at) VALUES (?1, ?2, ?3)",
            params![uuid::Uuid::new_v4().to_string(), prev.summary, now],
        )?;
    }
    conn.execute(
        "INSERT INTO growth_user_model (id, summary, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET summary = excluded.summary, updated_at = excluded.updated_at",
        params![USER_MODEL_ID, summary, now],
    )?;
    Ok(true)
}

/// Newest first.
pub fn list_user_model_history(conn: &Connection, limit: usize) -> anyhow::Result<Vec<UserModelHistoryRow>> {
    let mut stmt = conn.prepare(
        "SELECT summary, replaced_at FROM growth_user_model_history ORDER BY replaced_at DESC, rowid DESC LIMIT ?1",
    )?;
    let rows = stmt
        .query_map([limit as i64], |r| Ok(UserModelHistoryRow { summary: r.get(0)?, replaced_at: r.get(1)? }))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute("INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)", [])
            .unwrap();
        conn
    }

    pub(crate) fn add_message(conn: &Connection, role: &str, content: &str, created_at: i64) {
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at) VALUES (?1, 's1', ?2, ?3, ?4)",
            params![uuid::Uuid::new_v4().to_string(), role, content, created_at],
        )
        .unwrap();
    }

    #[test]
    fn insert_and_list_newest_first() {
        let c = db();
        let a = insert_reflection(&c, "first", 0.4, 3, 100).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(3));
        let b = insert_reflection(&c, "second", 0.9, 5, 200).unwrap();
        let rows = list_reflections(&c, 10, false).unwrap();
        assert_eq!(rows.iter().map(|r| r.id.clone()).collect::<Vec<_>>(), vec![b, a]);
        assert_eq!(rows[0].insight, "second");
        assert!((rows[0].confidence - 0.9).abs() < 1e-9);
        assert_eq!(list_reflections(&c, 1, false).unwrap().len(), 1);
    }

    #[test]
    fn archived_hidden_unless_requested() {
        let c = db();
        let a = insert_reflection(&c, "a", 0.5, 1, 1).unwrap();
        let b = insert_reflection(&c, "b", 0.5, 1, 2).unwrap();
        assert!(set_reflection_archived(&c, &a, true).unwrap());
        assert!(!set_reflection_archived(&c, "missing", true).unwrap());
        let live: Vec<String> = list_reflections(&c, 10, false).unwrap().into_iter().map(|r| r.id).collect();
        assert_eq!(live, vec![b.clone()]);
        let all = list_reflections(&c, 10, true).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().find(|r| r.id == a).unwrap().archived_at.is_some());
        assert!(set_reflection_archived(&c, &a, false).unwrap());
        assert_eq!(list_reflections(&c, 10, false).unwrap().len(), 2);
        archive_reflections(&c, &[a, b]).unwrap();
        assert!(list_reflections(&c, 10, false).unwrap().is_empty());
    }

    #[test]
    fn watermark_and_counts_track_new_assistant_messages() {
        let c = db();
        assert_eq!(latest_watermark(&c).unwrap(), 0);
        for (i, role) in ["user", "assistant", "user", "assistant", "tool", "assistant"].iter().enumerate() {
            add_message(&c, role, "x", 1000 + i as i64);
        }
        assert_eq!(count_assistant_messages_since(&c, 0).unwrap(), 3);
        let id = insert_reflection(&c, "r", 0.5, 4, 1003).unwrap();
        assert_eq!(latest_watermark(&c).unwrap(), 1003);
        assert_eq!(count_assistant_messages_since(&c, 1003).unwrap(), 1);
        set_reflection_archived(&c, &id, true).unwrap();
        assert_eq!(latest_watermark(&c).unwrap(), 1003, "archived rows still hold the watermark");
    }

    #[test]
    fn messages_since_returns_last_n_oldest_first_user_and_assistant_only() {
        let c = db();
        add_message(&c, "user", "old", 10);
        add_message(&c, "user", "u1", 20);
        add_message(&c, "tool", "t", 25);
        add_message(&c, "assistant", "a1", 30);
        add_message(&c, "user", "u2", 40);
        let msgs = messages_since(&c, 10, 2).unwrap();
        let got: Vec<(&str, &str)> = msgs.iter().map(|m| (m.role.as_str(), m.content.as_str())).collect();
        assert_eq!(got, vec![("assistant", "a1"), ("user", "u2")]);
        assert_eq!(messages_since(&c, 10, 50).unwrap().len(), 3, "after the watermark, no tool rows");
    }

    #[test]
    fn set_user_model_writes_history_only_on_change() {
        let c = db();
        assert_eq!(get_user_model(&c).unwrap(), None);
        assert!(set_user_model(&c, "v1").unwrap());
        assert!(list_user_model_history(&c, 10).unwrap().is_empty(), "first write has no predecessor");
        assert!(!set_user_model(&c, "v1").unwrap(), "same summary is not a change");
        assert!(list_user_model_history(&c, 10).unwrap().is_empty());
        assert!(set_user_model(&c, "v2").unwrap());
        assert_eq!(get_user_model(&c).unwrap().unwrap().summary, "v2");
        let hist = list_user_model_history(&c, 10).unwrap();
        assert_eq!(hist.len(), 1);
        assert_eq!(hist[0].summary, "v1");
    }
}
