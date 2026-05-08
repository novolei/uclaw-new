use std::sync::{Arc, Mutex};
use serde::{Deserialize, Serialize};
use tauri::Emitter;
use tokio::sync::broadcast;
use rusqlite::{Connection, params};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub enum ChannelRole {
    Supervisor,
    Worker(String),  // worker_id
    Reviewer,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelMessage {
    pub id: String,
    pub team_id: String,
    pub from_role: ChannelRole,
    pub to_role: Option<ChannelRole>,
    pub message: String,
    pub created_at: i64,
}

pub struct AgentTeamChannel {
    pub team_id: String,
    sender: broadcast::Sender<ChannelMessage>,
    db: Arc<Mutex<Connection>>,
    app_handle: tauri::AppHandle,
}

impl AgentTeamChannel {
    pub fn new(team_id: String, db: Arc<Mutex<Connection>>, app_handle: tauri::AppHandle) -> Self {
        let (sender, _) = broadcast::channel(256);
        Self { team_id, sender, db, app_handle }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<ChannelMessage> {
        self.sender.subscribe()
    }

    pub fn send(&self, from_role: ChannelRole, to_role: Option<ChannelRole>, message: String) {
        let msg = ChannelMessage {
            id: uuid::Uuid::new_v4().to_string(),
            team_id: self.team_id.clone(),
            from_role: from_role.clone(),
            to_role: to_role.clone(),
            message: message.clone(),
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        // Persist to DB (non-fatal)
        match self.db.lock() {
            Ok(conn) => {
                let from_str = serde_json::to_string(&from_role)
                    .unwrap_or_else(|e| {
                        tracing::warn!("AgentTeamChannel::send failed to serialize from_role: {e}");
                        String::new()
                    });
                let to_str = to_role.as_ref().and_then(|r| serde_json::to_string(r).ok());
                let _ = conn.execute(
                    "INSERT INTO team_channel_messages (id, team_id, from_role, to_role, message, created_at) VALUES (?1,?2,?3,?4,?5,?6)",
                    params![msg.id, msg.team_id, from_str, to_str, message, msg.created_at],
                );
            }
            Err(e) => tracing::error!("AgentTeamChannel::send DB lock failed: {e}"),
        }
        // Emit to frontend (non-fatal)
        let _ = self.app_handle.emit("agent:team-message", &msg);
        // Broadcast to internal subscribers (ignore if no subscribers)
        let _ = self.sender.send(msg);
    }

    pub fn get_messages(&self) -> Vec<ChannelMessage> {
        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("AgentTeamChannel::get_messages DB lock failed: {e}");
                return vec![];
            }
        };
        let mut stmt = match conn.prepare(
            "SELECT id, team_id, from_role, to_role, message, created_at FROM team_channel_messages WHERE team_id = ?1 ORDER BY created_at ASC LIMIT 500"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("AgentTeamChannel::get_messages prepare failed: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![self.team_id], |row| {
            let from_str: String = row.get(2)?;
            let to_str: Option<String> = row.get(3)?;
            Ok(ChannelMessage {
                id: row.get(0)?,
                team_id: row.get(1)?,
                from_role: serde_json::from_str(&from_str).unwrap_or(ChannelRole::User),
                to_role: to_str.and_then(|s| serde_json::from_str(&s).ok()),
                message: row.get(4)?,
                created_at: row.get(5)?,
            })
        }).ok()
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
    }
}
