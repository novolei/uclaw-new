use crate::agent::types::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Optional per-message metadata persisted alongside the message content.
/// All fields are optional — pass `MessageMeta::default()` for plain
/// user messages or non-streaming assistant messages.
#[derive(Debug, Clone, Default)]
pub struct MessageMeta {
    /// Concatenated thinking-block text for assistant messages.
    pub reasoning: Option<String>,
    /// JSON-serialized array of tool activity records (frontend ChatToolActivity shape).
    pub tool_activities_json: Option<String>,
    /// Model identifier used for the assistant turn.
    pub model: Option<String>,
    /// JSON-serialized attachment array.
    pub attachments_json: Option<String>,
}

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
        Self {
            sessions: HashMap::new(),
            db,
        }
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
        self.add_message_with_meta(session_id, message, MessageMeta::default())
    }

    /// Add a message with optional reasoning + tool-activity metadata.
    /// Used by the chat send_message path so historical messages can re-render
    /// the thinking block and the tool-call cards after reload.
    pub fn add_message_with_meta(
        &mut self,
        session_id: &str,
        message: ChatMessage,
        meta: MessageMeta,
    ) {
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
                let content = serde_json::to_string(&session.messages.last().map(|m| &m.content))
                    .unwrap_or_default();
                let _ = conn.execute(
                    "INSERT INTO messages (id, conversation_id, role, content, created_at, reasoning, tool_activities_json, model, attachments_json) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        msg_id,
                        session_id,
                        role,
                        content,
                        chrono::Utc::now().to_rfc3339(),
                        meta.reasoning,
                        meta.tool_activities_json,
                        meta.model,
                        meta.attachments_json,
                    ],
                );
                let _ = conn.execute(
                    "UPDATE conversations SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![session.updated_at, session_id],
                );
            }
        }
    }

    /// Load a session's messages back from SQLite into the in-memory cache.
    /// No-op if the session is already loaded.
    /// Returns true if the session is now in memory after the call.
    pub fn ensure_loaded(&mut self, conversation_id: &str) -> bool {
        if self.sessions.contains_key(conversation_id) {
            return true;
        }
        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(_) => return false,
        };

        // Pull conversation row first
        let conv: Option<(String, String, Option<String>, String, String)> = conn
            .query_row(
                "SELECT id, space_id, title, created_at, updated_at FROM conversations WHERE id = ?1",
                rusqlite::params![conversation_id],
                |row| Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                )),
            )
            .ok();

        let Some((id, space_id, title, created_at, updated_at)) = conv else {
            return false;
        };

        let mut session = Session {
            id: id.clone(),
            title: title.unwrap_or_default(),
            space_id,
            messages: Vec::new(),
            created_at,
            updated_at,
            cumulative_input_tokens: 0,
            cumulative_output_tokens: 0,
        };

        // Load messages (just role+content_json — meta columns are read by get_messages).
        // Collect into a Vec first so the prepared statement borrow ends before we
        // mutate `session.messages`.
        let collected: Vec<(String, String)> = {
            let mut stmt = match conn.prepare(
                "SELECT role, content FROM messages WHERE conversation_id = ?1 ORDER BY created_at ASC",
            ) {
                Ok(s) => s,
                Err(_) => return false,
            };
            let rows = match stmt.query_map(rusqlite::params![conversation_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                Ok(r) => r,
                Err(_) => return false,
            };
            rows.flatten().collect()
        };

        for (role_str, content_str) in collected {
            let role = match role_str.as_str() {
                "user" => MessageRole::User,
                "system" => MessageRole::System,
                _ => MessageRole::Assistant,
            };
            // content was stored as JSON of Option<&Vec<ContentBlock>>.
            // Try the new shape first, fall back to wrapping plain text.
            let content: Vec<ContentBlock> =
                serde_json::from_str::<Option<Vec<ContentBlock>>>(&content_str)
                    .ok()
                    .flatten()
                    .or_else(|| serde_json::from_str::<Vec<ContentBlock>>(&content_str).ok())
                    .unwrap_or_else(|| {
                        vec![ContentBlock::Text {
                            text: content_str.clone(),
                        }]
                    });
            session.messages.push(ChatMessage {
                role,
                content,
                compacted: false,
            });
        }

        drop(conn);
        self.sessions.insert(id, session);
        true
    }

    pub fn list(&self) -> Vec<SessionSummary> {
        // Prefer the database as the source of truth so conversations created
        // before the current session-manager-cache is populated (e.g. after
        // app restart) still show up. Falls back to in-memory if DB read fails.
        if let Ok(conn) = self.db.lock() {
            let mut stmt = conn.prepare(
                "SELECT c.id, c.space_id, c.title, c.created_at, c.updated_at, \
                        (SELECT COUNT(*) FROM messages m WHERE m.conversation_id = c.id) \
                 FROM conversations c \
                 ORDER BY c.updated_at DESC",
            );
            if let Ok(ref mut s) = stmt {
                if let Ok(rows) = s.query_map([], |row| {
                    Ok(SessionSummary {
                        id: row.get::<_, String>(0)?,
                        space_id: row.get::<_, String>(1)?,
                        title: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        created_at: row.get::<_, String>(3)?,
                        updated_at: row.get::<_, String>(4)?,
                        message_count: row.get::<_, i64>(5)? as usize,
                    })
                }) {
                    let collected: Vec<SessionSummary> = rows.flatten().collect();
                    if !collected.is_empty() {
                        return collected;
                    }
                }
            }
        }
        // Fallback: in-memory snapshot
        let mut summaries: Vec<SessionSummary> = self.sessions.values().map(|s| s.into()).collect();
        summaries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
        summaries
    }

    pub fn delete(&mut self, id: &str) -> bool {
        if self.sessions.remove(id).is_some() {
            if let Ok(conn) = self.db.lock() {
                let _ = conn.execute(
                    "DELETE FROM messages WHERE conversation_id = ?1",
                    rusqlite::params![id],
                );
                let _ = conn.execute(
                    "DELETE FROM conversations WHERE id = ?1",
                    rusqlite::params![id],
                );
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
            )
            .ok()
        })
    }

    /// Get a reference to the database connection
    pub fn db(&self) -> &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        &self.db
    }
}
