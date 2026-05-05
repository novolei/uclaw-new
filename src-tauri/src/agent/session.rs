use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::agent::types::*;

/// Session state for a single conversation
#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub space_id: String,
    pub messages: Vec<ChatMessage>,
    pub created_at: String,
    pub updated_at: String,
    /// Cumulative input tokens across all turns in this session.
    pub cumulative_input_tokens: u32,
    /// Cumulative output tokens across all turns in this session.
    pub cumulative_output_tokens: u32,
}

impl Session {
    pub fn new(id: String, title: String, space_id: String) -> Self {
        Self {
            id,
            title,
            space_id,
            messages: Vec::new(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
        }
    }
}

/// Serializable session summary for IPC
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub space_id: String,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

impl From<&Session> for SessionSummary {
    fn from(s: &Session) -> Self {
        Self {
            id: s.id.clone(),
            title: s.title.clone(),
            space_id: s.space_id.clone(),
            message_count: s.messages.len(),
            created_at: s.created_at.clone(),
            updated_at: s.updated_at.clone(),
        }
    }
}

/// Session manager: in-memory session store with database persistence
pub struct SessionManager {
    sessions: HashMap<String, Session>,
    db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl SessionManager {
    pub fn new(db: std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { sessions: HashMap::new(), db }
    }

    pub fn create(&mut self, title: &str, space_id: &str) -> SessionSummary {
        let id = uuid::Uuid::new_v4().to_string();
        let session = Session::new(id.clone(), title.to_string(), space_id.to_string());

        // Persist to database
        if let Ok(conn) = self.db.lock() {
            let _ = conn.execute(
                "INSERT INTO conversations (id, space_id, title, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![session.id, session.space_id, session.title, session.created_at, session.updated_at],
            );
        }

        let summary = SessionSummary::from(&session);
        self.sessions.insert(session.id.clone(), session);
        summary
    }

    pub fn get(&self, id: &str) -> Option<&Session> {
        self.sessions.get(id)
    }

    pub fn get_mut(&mut self, id: &str) -> Option<&mut Session> {
        self.sessions.get_mut(id)
    }

    pub fn add_message(&mut self, session_id: &str, message: ChatMessage) {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.messages.push(message);
            session.updated_at = chrono::Utc::now().to_rfc3339();

            // Persist to database
            if let Ok(conn) = self.db.lock() {
                let msg_id = uuid::Uuid::new_v4().to_string();
                let role = match session.messages.last() {
                    Some(m) => match m.role {
                        MessageRole::System => "system",
                        MessageRole::User => "user",
                        MessageRole::Assistant => "assistant",
                    },
                    None => "user",
                };
                let content = serde_json::to_string(&session.messages.last().map(|m| &m.content)).unwrap_or_default();
                let _ = conn.execute(
                    "INSERT INTO messages (id, conversation_id, role, content, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
                    rusqlite::params![msg_id, session_id, role, content, chrono::Utc::now().to_rfc3339()],
                );
                let _ = conn.execute(
                    "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![session.updated_at, session_id],
                );
            }
        }
    }

    pub fn list(&self) -> Vec<SessionSummary> {
        let mut summaries: Vec<SessionSummary> = self.sessions.values().map(|s| s.into()).collect();
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }

    pub fn delete(&mut self, id: &str) -> bool {
        if self.sessions.remove(id).is_some() {
            if let Ok(conn) = self.db.lock() {
                let _ = conn.execute("DELETE FROM messages WHERE conversation_id = ?1", rusqlite::params![id]);
                let _ = conn.execute("DELETE FROM conversations WHERE id = ?1", rusqlite::params![id]);
            }
            true
        } else {
            false
        }
    }

    /// Get the space_id for a conversation by its id.
    /// First checks the in-memory session; falls back to a database query.
    /// Returns None if the conversation is not found or on any error.
    pub fn get_space_id(&self, conversation_id: &str) -> Option<String> {
        // Fast path: session is already loaded in memory
        if let Some(session) = self.sessions.get(conversation_id) {
            return Some(session.space_id.clone());
        }
        // Fallback: query the database
        self.db.lock().ok().and_then(|conn| {
            conn.query_row(
                "SELECT space_id FROM conversations WHERE id = ?1",
                rusqlite::params![conversation_id],
                |row| row.get::<_, String>(0),
            ).ok()
        })
    }

    /// Get a reference to the database connection
    pub fn db(&self) -> &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        &self.db
    }
}
