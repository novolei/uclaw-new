//! ImSessionRegistry — persistent per-user IM session mapping.
//!
//! Maps (space_id, channel_type, chat_id) → agent_session_id.
//! Cache is backed by the `im_sessions` DB table and survives app restarts.

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

type SessionKey = (String, String, String); // (space_id, channel_type, chat_id)

pub struct ImSessionRegistry {
    cache: Arc<RwLock<HashMap<SessionKey, String>>>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ImSessionRegistry {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self {
            cache: Arc::new(RwLock::new(HashMap::new())),
            db,
        }
    }

    /// Load all existing im_sessions from DB into the in-memory cache.
    /// Call once at startup.
    pub async fn load_from_db(&self) -> Result<(), String> {
        let rows: Vec<(String, String, String, String)> = {
            let conn = self.db.lock().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT space_id, channel_type, chat_id, agent_session_id \
                     FROM im_sessions",
                )
                .map_err(|e| e.to_string())?;
            let collected: Vec<(String, String, String, String)> = stmt
                .query_map([], |r| {
                    Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?))
                })
                .map_err(|e| e.to_string())?
                .filter_map(|r| r.ok())
                .collect();
            collected
        };

        let mut cache = self.cache.write().await;
        for (space_id, channel_type, chat_id, session_id) in rows {
            cache.insert((space_id, channel_type, chat_id), session_id);
        }
        tracing::info!("ImSessionRegistry: loaded {} sessions from DB", cache.len());
        Ok(())
    }

    /// Return the existing agent_session_id for this (space, channel, user),
    /// or create a new agent_session + im_session record.
    pub async fn get_or_create_session(
        &self,
        space_id: &str,
        channel_type: &str,
        chat_id: &str,
        sender_name: Option<&str>,
    ) -> Result<String, String> {
        let key = (space_id.to_string(), channel_type.to_string(), chat_id.to_string());

        // Fast path: cache hit
        {
            let cache = self.cache.read().await;
            if let Some(session_id) = cache.get(&key) {
                return Ok(session_id.clone());
            }
        }

        // Slow path: create new agent_session + im_session row
        let session_id = uuid::Uuid::new_v4().to_string();
        let im_session_id = uuid::Uuid::new_v4().to_string();
        let now_ms = chrono::Utc::now().timestamp_millis();
        let title = match sender_name {
            Some(name) => format!("{} via {}", name, channel_type),
            None => format!("IM {} {}", channel_type, &chat_id[..chat_id.len().min(8)]),
        };

        {
            let conn = self.db.lock().map_err(|e| e.to_string())?;
            conn.execute(
                "INSERT INTO agent_sessions (id, title, space_id, created_at, updated_at, message_count) \
                 VALUES (?1, ?2, ?3, ?4, ?4, 0)",
                rusqlite::params![session_id, title, space_id, now_ms],
            )
            .map_err(|e| format!("create agent_session: {e}"))?;

            conn.execute(
                "INSERT INTO im_sessions (id, space_id, channel_type, chat_id, agent_session_id, created_at, last_active_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                rusqlite::params![im_session_id, space_id, channel_type, chat_id, session_id, now_ms],
            )
            .map_err(|e| format!("create im_session: {e}"))?;
        }

        let mut cache = self.cache.write().await;
        cache.insert(key, session_id.clone());
        Ok(session_id)
    }

    /// Update last_active_at for a session.
    pub async fn touch(
        &self,
        space_id: &str,
        channel_type: &str,
        chat_id: &str,
    ) -> Result<(), String> {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE im_sessions SET last_active_at = ?1 \
             WHERE space_id = ?2 AND channel_type = ?3 AND chat_id = ?4",
            rusqlite::params![now_ms, space_id, channel_type, chat_id],
        )
        .map_err(|e| format!("touch im_session: {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn in_memory_conn() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn get_or_create_creates_session_on_first_call() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let session_id = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", Some("Alice"))
            .await
            .unwrap();
        assert!(!session_id.is_empty());

        let session_id2 = registry
            .get_or_create_session("space-1", "wecom_bot", "user-1", None)
            .await
            .unwrap();
        assert_eq!(session_id, session_id2);
    }

    #[tokio::test]
    async fn different_users_get_different_sessions() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let s1 = registry.get_or_create_session("space-1", "wecom_bot", "user-A", None).await.unwrap();
        let s2 = registry.get_or_create_session("space-1", "wecom_bot", "user-B", None).await.unwrap();
        assert_ne!(s1, s2);
    }

    #[tokio::test]
    async fn load_from_db_restores_cache() {
        let db = in_memory_conn();
        let registry = ImSessionRegistry::new(db.clone());
        registry.load_from_db().await.unwrap();

        let original = registry
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();

        let registry2 = ImSessionRegistry::new(db.clone());
        registry2.load_from_db().await.unwrap();
        let restored = registry2
            .get_or_create_session("space-1", "email", "user-C", None)
            .await
            .unwrap();
        assert_eq!(original, restored, "session must survive registry restart");
    }
}
