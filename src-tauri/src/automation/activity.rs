//! Activity audit log for Humane automation runs. Matches V20b schema.

use serde::{Deserialize, Serialize};

// ─── Status enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    WaitingUser,
}

impl ActivityStatus {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Queued       => "queued",
            Self::Running      => "running",
            Self::Completed    => "completed",
            Self::Failed       => "failed",
            Self::Cancelled    => "cancelled",
            Self::WaitingUser  => "waiting_user",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "queued"        => Some(Self::Queued),
            "running"       => Some(Self::Running),
            "completed"     => Some(Self::Completed),
            "failed"        => Some(Self::Failed),
            "cancelled"     => Some(Self::Cancelled),
            "waiting_user"  => Some(Self::WaitingUser),
            _               => None,
        }
    }
}

// ─── TriggerSource enum ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TriggerSource {
    Schedule,
    File,
    Webhook,
    Webpage,
    Rss,
    Wecom,
    Custom,
    Manual,
}

impl TriggerSource {
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Schedule => "schedule",
            Self::File     => "file",
            Self::Webhook  => "webhook",
            Self::Webpage  => "webpage",
            Self::Rss      => "rss",
            Self::Wecom    => "wecom",
            Self::Custom   => "custom",
            Self::Manual   => "manual",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "schedule" => Some(Self::Schedule),
            "file"     => Some(Self::File),
            "webhook"  => Some(Self::Webhook),
            "webpage"  => Some(Self::Webpage),
            "rss"      => Some(Self::Rss),
            "wecom"    => Some(Self::Wecom),
            "custom"   => Some(Self::Custom),
            "manual"   => Some(Self::Manual),
            _          => None,
        }
    }
}

// ─── Core struct ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationActivity {
    pub id: String,
    pub spec_id: String,
    pub subscription_id: Option<String>,
    pub trigger_source_type: TriggerSource,
    pub trigger_payload_json: String,
    pub status: ActivityStatus,
    pub error_text: Option<String>,
    pub queued_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
    /// NOT NULL DEFAULT 0 in the schema; zero until a run finishes.
    pub duration_ms: i64,
    pub llm_iterations: i64,
    pub llm_tokens_in: i64,
    pub llm_tokens_out: i64,
    /// Nullable link to the run's agent_session (V24). None for runs that
    /// never reached the loop (filtered out, deduped, rejected).
    pub session_id: Option<String>,
    /// JSON array of declared products from report_to_user.artifacts (V24).
    pub report_artifacts_json: String,
    pub report_text: Option<String>,
    pub report_outcome: Option<String>,
    pub escalation_id: Option<String>,
    pub resumed_from_activity_id: Option<String>,
    pub resumed_from_escalation_id: Option<String>,
}

// ─── Row mapper (DRY helper) ──────────────────────────────────────────────────

fn row_to_activity(r: &rusqlite::Row<'_>) -> rusqlite::Result<AutomationActivity> {
    let trigger_str: String = r.get(3)?;
    let status_str: String  = r.get(5)?;

    Ok(AutomationActivity {
        id:                          r.get(0)?,
        spec_id:                     r.get(1)?,
        subscription_id:             r.get(2)?,
        trigger_source_type: TriggerSource::from_db_str(&trigger_str)
            .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(format!("unknown trigger source: {trigger_str}"))),
            ))?,
        trigger_payload_json:        r.get(4)?,
        status: ActivityStatus::from_db_str(&status_str)
            .ok_or_else(|| rusqlite::Error::FromSqlConversionFailure(
                5,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::other(format!("unknown status: {status_str}"))),
            ))?,
        error_text:                  r.get(6)?,
        queued_at:                   r.get(7)?,
        started_at:                  r.get(8)?,
        completed_at:                r.get(9)?,
        duration_ms:                 r.get(10)?,
        llm_iterations:              r.get(11)?,
        llm_tokens_in:               r.get(12)?,
        llm_tokens_out:              r.get(13)?,
        session_id:                  r.get(14)?,
        report_artifacts_json:       r.get(15)?,
        report_text:                 r.get(16)?,
        report_outcome:              r.get(17)?,
        escalation_id:               r.get(18)?,
        resumed_from_activity_id:    r.get(19)?,
        resumed_from_escalation_id:  r.get(20)?,
    })
}

const SELECT_COLS: &str =
    "id, spec_id, subscription_id, trigger_source_type, trigger_payload_json,
     status, error_text, queued_at, started_at, completed_at, duration_ms,
     llm_iterations, llm_tokens_in, llm_tokens_out, session_id, report_artifacts_json,
     report_text, report_outcome, escalation_id,
     resumed_from_activity_id, resumed_from_escalation_id";

// ─── Public CRUD ──────────────────────────────────────────────────────────────

pub fn insert_activity(conn: &rusqlite::Connection, a: &AutomationActivity) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO automation_activities (
            id, spec_id, subscription_id, trigger_source_type, trigger_payload_json,
            status, error_text, queued_at, started_at, completed_at, duration_ms,
            llm_iterations, llm_tokens_in, llm_tokens_out, session_id, report_artifacts_json,
            report_text, report_outcome, escalation_id,
            resumed_from_activity_id, resumed_from_escalation_id
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
        )",
        rusqlite::params![
            a.id, a.spec_id, a.subscription_id,
            a.trigger_source_type.as_db_str(), a.trigger_payload_json,
            a.status.as_db_str(), a.error_text,
            a.queued_at, a.started_at, a.completed_at, a.duration_ms,
            a.llm_iterations, a.llm_tokens_in, a.llm_tokens_out, a.session_id, a.report_artifacts_json,
            a.report_text, a.report_outcome, a.escalation_id,
            a.resumed_from_activity_id, a.resumed_from_escalation_id,
        ],
    )?;
    Ok(())
}

pub fn get_activity(conn: &rusqlite::Connection, id: &str) -> rusqlite::Result<Option<AutomationActivity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS} FROM automation_activities WHERE id = ?1"
    ))?;
    match stmt.query_row([id], row_to_activity) {
        Ok(a)                                        => Ok(Some(a)),
        Err(rusqlite::Error::QueryReturnedNoRows)    => Ok(None),
        Err(e)                                       => Err(e),
    }
}

pub fn list_activities_for_spec(
    conn: &rusqlite::Connection,
    spec_id: &str,
    limit: u32,
) -> rusqlite::Result<Vec<AutomationActivity>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {SELECT_COLS}
         FROM automation_activities
         WHERE spec_id = ?1
         ORDER BY queued_at DESC
         LIMIT ?2"
    ))?;
    let rows = stmt.query_map(rusqlite::params![spec_id, limit], row_to_activity)?;
    rows.collect()
}

// ─── Legacy shim (kept for old consumers in runtime.rs / service.rs) ─────────
//
// FIXME(Task 19): remove ActivityStore once runtime.rs and service.rs are
// rewritten against the V20b schema.  The struct wraps the free functions above
// so that the old call sites continue to compile unchanged.

use std::sync::Arc;

pub struct ActivityStore {
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl ActivityStore {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    pub fn insert(&self, activity: &AutomationActivity) -> Result<(), String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        insert_activity(&conn, activity).map_err(|e| e.to_string())
    }

    /// Mark an activity completed and store its report text.
    pub fn complete(&self, id: &str, result: &str, duration_ms: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE automation_activities
             SET status = 'completed', report_text = ?1, duration_ms = ?2,
                 completed_at = ?3
             WHERE id = ?4",
            rusqlite::params![result, duration_ms, now, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Mark an activity failed and store its error text.
    pub fn fail(&self, id: &str, error: &str, duration_ms: i64) -> Result<(), String> {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        conn.execute(
            "UPDATE automation_activities
             SET status = 'failed', error_text = ?1, duration_ms = ?2,
                 completed_at = ?3
             WHERE id = ?4",
            rusqlite::params![error, duration_ms, now, id],
        ).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn list_for_spec(&self, spec_id: &str, limit: usize) -> Result<Vec<AutomationActivity>, String> {
        let conn = self.db.lock().map_err(|e| e.to_string())?;
        list_activities_for_spec(&conn, spec_id, limit as u32).map_err(|e| e.to_string())
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Run the full migration stack (including V24 which adds session_id /
        // report_artifacts_json and drops tool_calls_json).
        crate::db::migrations::run(&conn).unwrap();
        // Insert a parent spec so the FK passes.
        conn.execute(
            "INSERT INTO automation_specs
             (id, name, version, author, description, system_prompt,
              spec_yaml, spec_json, created_at, updated_at)
             VALUES ('s1', 'x', '0.1.0', 'x', 'x', 'x', 'type: automation', '{}', 1, 1)",
            [],
        ).unwrap();
        conn
    }

    fn make_activity(id: &str) -> AutomationActivity {
        AutomationActivity {
            id:                          id.into(),
            spec_id:                     "s1".into(),
            subscription_id:             None,
            trigger_source_type:         TriggerSource::Manual,
            trigger_payload_json:        "{}".into(),
            status:                      ActivityStatus::Queued,
            error_text:                  None,
            queued_at:                   1,
            started_at:                  None,
            completed_at:                None,
            duration_ms:                 0,
            llm_iterations:              0,
            llm_tokens_in:               0,
            llm_tokens_out:              0,
            session_id:                  None,
            report_artifacts_json:       "[]".into(),
            report_text:                 None,
            report_outcome:              None,
            escalation_id:               None,
            resumed_from_activity_id:    None,
            resumed_from_escalation_id:  None,
        }
    }

    #[test]
    fn roundtrip_activity() {
        let conn = setup_test_db();
        let activity = make_activity("a1");
        insert_activity(&conn, &activity).unwrap();
        let loaded = get_activity(&conn, "a1").unwrap().unwrap();
        assert_eq!(loaded.spec_id, "s1");
        assert_eq!(loaded.status, ActivityStatus::Queued);
        assert!(matches!(loaded.trigger_source_type, TriggerSource::Manual));
        assert_eq!(loaded.trigger_payload_json, "{}");
        assert_eq!(loaded.report_artifacts_json, "[]");
        assert_eq!(loaded.duration_ms, 0);
    }

    #[test]
    fn get_activity_missing_returns_none() {
        let conn = setup_test_db();
        let result = get_activity(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn list_activities_for_spec_ordering_and_limit() {
        let conn = setup_test_db();
        // Insert 3 activities with distinct queued_at values.
        for (id, queued_at) in [("a1", 10i64), ("a2", 30i64), ("a3", 20i64)] {
            let mut a = make_activity(id);
            a.queued_at = queued_at;
            insert_activity(&conn, &a).unwrap();
        }
        // Limit 2 → newest two by queued_at DESC.
        let rows = list_activities_for_spec(&conn, "s1", 2).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].id, "a2"); // queued_at = 30
        assert_eq!(rows[1].id, "a3"); // queued_at = 20
    }

    #[test]
    fn all_status_variants_roundtrip() {
        use ActivityStatus::*;
        for status in [Queued, Running, Completed, Failed, Cancelled, WaitingUser] {
            let s = status.as_db_str();
            assert_eq!(ActivityStatus::from_db_str(s), Some(status));
        }
        assert!(ActivityStatus::from_db_str("bogus").is_none());
    }

    #[test]
    fn all_trigger_source_variants_roundtrip() {
        use TriggerSource::*;
        for src in [Schedule, File, Webhook, Webpage, Rss, Wecom, Custom, Manual] {
            let s = src.as_db_str();
            assert_eq!(TriggerSource::from_db_str(s), Some(src));
        }
        assert!(TriggerSource::from_db_str("bogus").is_none());
    }

    #[test]
    fn activity_store_shim_complete_and_fail() {
        let conn = setup_test_db();
        let db = Arc::new(std::sync::Mutex::new(conn));
        let store = ActivityStore::new(Arc::clone(&db));

        let mut a = make_activity("b1");
        a.status = ActivityStatus::Running;
        store.insert(&a).unwrap();
        store.complete("b1", "ok result", 1234).unwrap();

        let loaded = {
            let c = db.lock().unwrap();
            get_activity(&c, "b1").unwrap().unwrap()
        };
        assert_eq!(loaded.status, ActivityStatus::Completed);
        assert_eq!(loaded.duration_ms, 1234);
        assert_eq!(loaded.report_text.as_deref(), Some("ok result"));
    }
}
