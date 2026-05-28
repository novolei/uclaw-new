use std::sync::{Arc, Mutex};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TurnRecord {
    pub id: String,
    pub session_id: String,
    pub turn_index: u32,
    pub role: String,
    pub content: Option<String>,
    pub tool_name: Option<String>,
    pub tool_args: Option<String>,
    pub tool_result: Option<String>,
    pub reasoning: Option<String>,
    pub is_error: bool,
    pub duration_ms: u64,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrajectorySearchHit {
    pub session_id: String,
    pub turn_index: u32,
    pub tool_name: Option<String>,
    pub snippet: String,
    pub created_at: i64,
}

pub struct TrajectoryStore {
    db: Arc<Mutex<Connection>>,
}

impl TrajectoryStore {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub fn record_turn(&self, record: &TurnRecord) -> Result<(), rusqlite::Error> {
        let conn = self.db.lock().unwrap();
        let tool_result_truncated = record.tool_result.as_deref().map(|r| {
            if r.len() > 8192 {
                let end = r.floor_char_boundary(8192);
                &r[..end]
            } else {
                r
            }
        });
        conn.execute(
            "INSERT INTO agent_turns
             (id, session_id, turn_index, role, content, tool_name, tool_args,
              tool_result, reasoning, is_error, duration_ms, created_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            params![
                record.id,
                record.session_id,
                record.turn_index,
                record.role,
                record.content,
                record.tool_name,
                record.tool_args,
                tool_result_truncated,
                record.reasoning,
                record.is_error as i32,
                record.duration_ms as i64,
                record.created_at,
            ],
        )?;
        Ok(())
    }

    pub fn get_session_turns(&self, session_id: &str) -> Vec<TurnRecord> {
        let conn = self.db.lock().unwrap();
        let mut stmt = match conn.prepare(
            "SELECT id, session_id, turn_index, role, content, tool_name, tool_args,
                    tool_result, reasoning, is_error, duration_ms, created_at
             FROM agent_turns WHERE session_id = ?1 ORDER BY turn_index ASC"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("get_session_turns prepare failed: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![session_id], |row| {
            Ok(TurnRecord {
                id: row.get(0)?,
                session_id: row.get(1)?,
                turn_index: row.get::<_, i64>(2)? as u32,
                role: row.get(3)?,
                content: row.get(4)?,
                tool_name: row.get(5)?,
                tool_args: row.get(6)?,
                tool_result: row.get(7)?,
                reasoning: row.get(8)?,
                is_error: row.get::<_, i32>(9)? != 0,
                duration_ms: row.get::<_, i64>(10)? as u64,
                created_at: row.get(11)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_else(|e| { tracing::error!("get_session_turns query failed: {e}"); vec![] })
    }

    pub fn search(&self, query: &str, limit: u32) -> Vec<TrajectorySearchHit> {
        let conn = self.db.lock().unwrap();
        let sql = "SELECT t.session_id, t.turn_index, t.tool_name,
                          snippet(agent_turns_fts, 1, '<b>', '</b>', '...', 20) as snip,
                          t.created_at
                   FROM agent_turns_fts f
                   JOIN agent_turns t ON t.rowid = f.rowid
                   WHERE agent_turns_fts MATCH ?1
                   ORDER BY rank LIMIT ?2";
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(e) => {
                tracing::error!("search_trajectories prepare failed: {e}");
                return vec![];
            }
        };
        stmt.query_map(params![query, limit as i64], |row| {
            Ok(TrajectorySearchHit {
                session_id: row.get(0)?,
                turn_index: row.get::<_, i64>(1)? as u32,
                tool_name: row.get(2)?,
                snippet: row.get(3)?,
                created_at: row.get(4)?,
            })
        })
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_else(|e| { tracing::error!("search_trajectories query failed: {e}"); vec![] })
    }
}
