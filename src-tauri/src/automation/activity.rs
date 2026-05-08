use std::sync::Arc;
use serde::{Deserialize, Serialize};
use rusqlite::{params, Connection};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationActivity {
    pub id: String,
    pub spec_id: String,
    pub run_id: String,
    pub trigger: String,
    pub status: ActivityStatus,
    pub result: Option<String>,
    pub error: Option<String>,
    pub duration_ms: i64,
    pub created_at: i64,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

impl std::fmt::Display for ActivityStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Running => write!(f, "running"),
            Self::Completed => write!(f, "completed"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::str::FromStr for ActivityStatus {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "cancelled" => Ok(Self::Cancelled),
            other => Err(format!("Unknown status: {}", other)),
        }
    }
}

pub struct ActivityStore {
    db: Arc<std::sync::Mutex<Connection>>,
}

impl ActivityStore {
    pub fn new(db: Arc<std::sync::Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub fn insert(&self, activity: &AutomationActivity) -> Result<(), String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO automation_activities
             (id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at, completed_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                activity.id,
                activity.spec_id,
                activity.run_id,
                activity.trigger,
                activity.status.to_string(),
                activity.result,
                activity.error,
                activity.duration_ms,
                activity.created_at,
                activity.completed_at,
            ],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn complete(&self, id: &str, result: &str, duration_ms: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE automation_activities SET status='completed', result=?1, duration_ms=?2, completed_at=?3 WHERE id=?4",
            params![result, duration_ms, now, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn fail(&self, id: &str, error: &str, duration_ms: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE automation_activities SET status='failed', error=?1, duration_ms=?2, completed_at=?3 WHERE id=?4",
            params![error, duration_ms, now, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_for_spec(&self, spec_id: &str, limit: usize) -> Result<Vec<AutomationActivity>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT id, spec_id, run_id, trigger, status, result, error, duration_ms, created_at, completed_at
             FROM automation_activities WHERE spec_id=?1 ORDER BY created_at DESC LIMIT ?2"
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map(params![spec_id, limit as i64], |row| {
            let status_str: String = row.get(4)?;
            Ok(AutomationActivity {
                id: row.get(0)?,
                spec_id: row.get(1)?,
                run_id: row.get(2)?,
                trigger: row.get(3)?,
                status: status_str.parse().unwrap_or(ActivityStatus::Failed),
                result: row.get(5)?,
                error: row.get(6)?,
                duration_ms: row.get(7)?,
                created_at: row.get(8)?,
                completed_at: row.get(9)?,
            })
        }).map_err(|e| e.to_string())?;

        rows.filter_map(|r| r.ok()).collect::<Vec<_>>().pipe_ok()
    }
}

trait PipeOk {
    fn pipe_ok(self) -> Result<Self, String> where Self: Sized;
}
impl<T> PipeOk for Vec<T> {
    fn pipe_ok(self) -> Result<Self, String> { Ok(self) }
}
