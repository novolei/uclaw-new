use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::browser::session_state::{
    BrowserTaskRun, BrowserTaskStatus, BrowserTaskStep, BrowserTaskStepPhase,
};
use crate::browser::types::TabInfo;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskMemory {
    pub session_id: String,
    pub task_summary: String,
    pub facts: Vec<String>,
    pub visited_urls: Vec<String>,
    pub open_tabs: Vec<TabInfo>,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskCheckpoint {
    pub checkpoint_id: String,
    pub run_id: String,
    pub session_id: String,
    pub step_index: u32,
    pub active_tab_id: Option<String>,
    pub memory: Option<BrowserTaskMemory>,
    pub loop_state: serde_json::Value,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct BrowserTaskStore {
    db: Arc<Mutex<Connection>>,
}

impl BrowserTaskStore {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    pub fn persist_run(&self, run: &BrowserTaskRun) -> rusqlite::Result<()> {
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        let now = chrono::Utc::now().timestamp_millis();
        let created_at = run.steps.first().map(|s| s.timestamp_ms).unwrap_or(now);
        conn.execute(
            "INSERT INTO browser_task_runs (run_id, session_id, task, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(run_id) DO UPDATE SET
                session_id=excluded.session_id,
                task=excluded.task,
                status=excluded.status,
                updated_at=excluded.updated_at",
            params![
                run.run_id,
                run.session_id,
                run.task,
                status_to_str(&run.status),
                created_at,
                now,
            ],
        )?;

        for step in &run.steps {
            conn.execute(
                "INSERT INTO browser_task_steps (
                    run_id, step_index, phase, observation_summary, reasoning,
                    action_name, action_args_json, ok, message, error, timestamp_ms
                 )
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
                 ON CONFLICT(run_id, step_index) DO UPDATE SET
                    phase=excluded.phase,
                    observation_summary=excluded.observation_summary,
                    reasoning=excluded.reasoning,
                    action_name=excluded.action_name,
                    action_args_json=excluded.action_args_json,
                    ok=excluded.ok,
                    message=excluded.message,
                    error=excluded.error,
                    timestamp_ms=excluded.timestamp_ms",
                params![
                    run.run_id,
                    step.step_index,
                    phase_to_str(&step.phase),
                    step.observation_summary,
                    step.reasoning,
                    step.action_name,
                    serde_json::to_string(&step.action_args).unwrap_or_else(|_| "{}".to_string()),
                    if step.ok { 1 } else { 0 },
                    step.message,
                    step.error,
                    step.timestamp_ms,
                ],
            )?;
        }
        Ok(())
    }

    pub fn load_run(&self, run_id: &str) -> rusqlite::Result<Option<BrowserTaskRun>> {
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        let mut run = conn
            .query_row(
                "SELECT run_id, session_id, task, status FROM browser_task_runs WHERE run_id = ?1",
                params![run_id],
                |row| {
                    Ok(BrowserTaskRun {
                        run_id: row.get(0)?,
                        session_id: row.get(1)?,
                        task: row.get(2)?,
                        status: status_from_str(row.get::<_, String>(3)?.as_str()),
                        steps: Vec::new(),
                    })
                },
            )
            .optional()?;

        if let Some(run) = run.as_mut() {
            let mut stmt = conn.prepare(
                "SELECT step_index, phase, observation_summary, reasoning, action_name,
                        action_args_json, ok, message, error, timestamp_ms
                 FROM browser_task_steps
                 WHERE run_id = ?1
                 ORDER BY step_index ASC",
            )?;
            let rows = stmt.query_map(params![run_id], |row| {
                let args: String = row.get(5)?;
                Ok(BrowserTaskStep {
                    step_index: row.get::<_, i64>(0)? as u32,
                    phase: phase_from_str(row.get::<_, String>(1)?.as_str()),
                    observation_summary: row.get(2)?,
                    reasoning: row.get(3)?,
                    action_name: row.get(4)?,
                    action_args: serde_json::from_str(&args).unwrap_or(serde_json::Value::Null),
                    ok: row.get::<_, i64>(6)? != 0,
                    message: row.get(7)?,
                    error: row.get(8)?,
                    timestamp_ms: row.get(9)?,
                })
            })?;
            run.steps = rows.collect::<rusqlite::Result<Vec<_>>>()?;
        }
        Ok(run)
    }

    pub fn load_memory(&self, session_id: &str) -> rusqlite::Result<Option<BrowserTaskMemory>> {
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        conn.query_row(
            "SELECT session_id, task_summary, facts_json, visited_urls_json, open_tabs_json, updated_at
             FROM browser_task_memory WHERE session_id = ?1",
            params![session_id],
            |row| {
                let facts_json: String = row.get(2)?;
                let urls_json: String = row.get(3)?;
                let tabs_json: String = row.get(4)?;
                Ok(BrowserTaskMemory {
                    session_id: row.get(0)?,
                    task_summary: row.get(1)?,
                    facts: serde_json::from_str(&facts_json).unwrap_or_default(),
                    visited_urls: serde_json::from_str(&urls_json).unwrap_or_default(),
                    open_tabs: serde_json::from_str(&tabs_json).unwrap_or_default(),
                    updated_at: row.get(5)?,
                })
            },
        ).optional()
    }

    pub fn merge_observation(
        &self,
        session_id: &str,
        task: &str,
        observation_json: &serde_json::Value,
    ) -> rusqlite::Result<BrowserTaskMemory> {
        let mut memory = self
            .load_memory(session_id)?
            .unwrap_or_else(|| BrowserTaskMemory {
                session_id: session_id.to_string(),
                task_summary: task.to_string(),
                facts: Vec::new(),
                visited_urls: Vec::new(),
                open_tabs: Vec::new(),
                updated_at: chrono::Utc::now().timestamp_millis(),
            });
        memory.task_summary = task.to_string();
        if let Some(url) = observation_json.get("url").and_then(|v| v.as_str()) {
            push_unique(&mut memory.visited_urls, url.to_string(), 40);
        }
        if let Some(title) = observation_json.get("title").and_then(|v| v.as_str()) {
            if !title.trim().is_empty() {
                push_unique(&mut memory.facts, format!("Last page title: {title}"), 20);
            }
        }
        if let Some(tabs) = observation_json.get("tabs") {
            memory.open_tabs = serde_json::from_value(tabs.clone()).unwrap_or_default();
        }
        memory.updated_at = chrono::Utc::now().timestamp_millis();
        self.persist_memory(&memory)?;
        Ok(memory)
    }

    pub fn persist_checkpoint(
        &self,
        run: &BrowserTaskRun,
        step_index: u32,
        active_tab_id: Option<&str>,
        memory: Option<&BrowserTaskMemory>,
        loop_state: serde_json::Value,
    ) -> rusqlite::Result<BrowserTaskCheckpoint> {
        let checkpoint = BrowserTaskCheckpoint {
            checkpoint_id: uuid::Uuid::new_v4().to_string(),
            run_id: run.run_id.clone(),
            session_id: run.session_id.clone(),
            step_index,
            active_tab_id: active_tab_id.map(|s| s.to_string()),
            memory: memory.cloned(),
            loop_state,
            created_at: chrono::Utc::now().timestamp_millis(),
        };
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        conn.execute(
            "INSERT INTO browser_task_checkpoints (
                checkpoint_id, run_id, session_id, step_index, active_tab_id,
                memory_json, loop_state_json, created_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                checkpoint.checkpoint_id,
                checkpoint.run_id,
                checkpoint.session_id,
                checkpoint.step_index,
                checkpoint.active_tab_id,
                checkpoint
                    .memory
                    .as_ref()
                    .and_then(|m| serde_json::to_string(m).ok()),
                serde_json::to_string(&checkpoint.loop_state).unwrap_or_else(|_| "{}".to_string()),
                checkpoint.created_at,
            ],
        )?;
        Ok(checkpoint)
    }

    pub fn latest_checkpoint(
        &self,
        run_id: &str,
    ) -> rusqlite::Result<Option<BrowserTaskCheckpoint>> {
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        conn.query_row(
            "SELECT checkpoint_id, run_id, session_id, step_index, active_tab_id,
                    memory_json, loop_state_json, created_at
             FROM browser_task_checkpoints
             WHERE run_id = ?1
             ORDER BY step_index DESC, created_at DESC
             LIMIT 1",
            params![run_id],
            |row| {
                let memory_json: Option<String> = row.get(5)?;
                let loop_state_json: String = row.get(6)?;
                Ok(BrowserTaskCheckpoint {
                    checkpoint_id: row.get(0)?,
                    run_id: row.get(1)?,
                    session_id: row.get(2)?,
                    step_index: row.get::<_, i64>(3)? as u32,
                    active_tab_id: row.get(4)?,
                    memory: memory_json.and_then(|json| serde_json::from_str(&json).ok()),
                    loop_state: serde_json::from_str(&loop_state_json)
                        .unwrap_or(serde_json::Value::Null),
                    created_at: row.get(7)?,
                })
            },
        )
        .optional()
    }

    fn persist_memory(&self, memory: &BrowserTaskMemory) -> rusqlite::Result<()> {
        let conn = self.db.lock().expect("browser task db mutex poisoned");
        conn.execute(
            "INSERT INTO browser_task_memory (
                session_id, task_summary, facts_json, visited_urls_json, open_tabs_json, updated_at
             )
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(session_id) DO UPDATE SET
                task_summary=excluded.task_summary,
                facts_json=excluded.facts_json,
                visited_urls_json=excluded.visited_urls_json,
                open_tabs_json=excluded.open_tabs_json,
                updated_at=excluded.updated_at",
            params![
                memory.session_id,
                memory.task_summary,
                serde_json::to_string(&memory.facts).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&memory.visited_urls).unwrap_or_else(|_| "[]".to_string()),
                serde_json::to_string(&memory.open_tabs).unwrap_or_else(|_| "[]".to_string()),
                memory.updated_at,
            ],
        )?;
        Ok(())
    }
}

fn push_unique(values: &mut Vec<String>, value: String, max_len: usize) {
    if values.iter().any(|v| v == &value) {
        return;
    }
    values.push(value);
    if values.len() > max_len {
        let overflow = values.len() - max_len;
        values.drain(0..overflow);
    }
}

fn status_to_str(status: &BrowserTaskStatus) -> &'static str {
    match status {
        BrowserTaskStatus::Running => "running",
        BrowserTaskStatus::Completed => "completed",
        BrowserTaskStatus::Failed => "failed",
        BrowserTaskStatus::Stopped => "stopped",
        BrowserTaskStatus::NeedsUserIntervention => "needs_user_intervention",
        BrowserTaskStatus::PausedWaitingForBrowserRuntime => "paused_waiting_for_browser_runtime",
        BrowserTaskStatus::PausedCheckpointed => "paused_checkpointed",
    }
}

fn status_from_str(value: &str) -> BrowserTaskStatus {
    match value {
        "completed" => BrowserTaskStatus::Completed,
        "failed" => BrowserTaskStatus::Failed,
        "stopped" => BrowserTaskStatus::Stopped,
        "needs_user_intervention" => BrowserTaskStatus::NeedsUserIntervention,
        "paused_waiting_for_browser_runtime" => BrowserTaskStatus::PausedWaitingForBrowserRuntime,
        "paused_checkpointed" => BrowserTaskStatus::PausedCheckpointed,
        _ => BrowserTaskStatus::Running,
    }
}

fn phase_to_str(phase: &BrowserTaskStepPhase) -> &'static str {
    match phase {
        BrowserTaskStepPhase::Observe => "observe",
        BrowserTaskStepPhase::Decide => "decide",
        BrowserTaskStepPhase::Act => "act",
        BrowserTaskStepPhase::Recover => "recover",
        BrowserTaskStepPhase::UserIntervention => "user_intervention",
        BrowserTaskStepPhase::Done => "done",
    }
}

fn phase_from_str(value: &str) -> BrowserTaskStepPhase {
    match value {
        "decide" => BrowserTaskStepPhase::Decide,
        "act" => BrowserTaskStepPhase::Act,
        "recover" => BrowserTaskStepPhase::Recover,
        "user_intervention" => BrowserTaskStepPhase::UserIntervention,
        "done" => BrowserTaskStepPhase::Done,
        _ => BrowserTaskStepPhase::Observe,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_run_steps_and_memory_notebook() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let store = BrowserTaskStore::new(Arc::new(Mutex::new(conn)));
        let run = BrowserTaskRun {
            run_id: "run-1".to_string(),
            session_id: "session-1".to_string(),
            task: "Research browser-use".to_string(),
            status: BrowserTaskStatus::Running,
            steps: vec![BrowserTaskStep {
                step_index: 0,
                phase: BrowserTaskStepPhase::Observe,
                observation_summary: "https://example.test · Example".to_string(),
                reasoning: "Captured state".to_string(),
                action_name: "observe".to_string(),
                action_args: serde_json::json!({"tabId": "tab-1"}),
                ok: true,
                message: Some("Observed".to_string()),
                error: None,
                timestamp_ms: 1710000000000,
            }],
        };

        store.persist_run(&run).unwrap();
        store.merge_observation(
            "session-1",
            "Research browser-use",
            &serde_json::json!({
                "url": "https://example.test",
                "title": "Example",
                "tabs": [{"tabId": "tab-1", "url": "https://example.test", "title": "Example", "active": true}],
                "pageText": "Example content"
            }),
        ).unwrap();

        let loaded = store.load_run("run-1").unwrap().expect("run exists");
        assert_eq!(loaded.steps.len(), 1);
        assert_eq!(loaded.status, BrowserTaskStatus::Running);

        let memory = store
            .load_memory("session-1")
            .unwrap()
            .expect("memory exists");
        assert_eq!(memory.session_id, "session-1");
        assert!(memory
            .visited_urls
            .contains(&"https://example.test".to_string()));
        assert_eq!(memory.open_tabs.len(), 1);
    }

    #[test]
    fn persists_latest_checkpoint_for_resume() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let store = BrowserTaskStore::new(Arc::new(Mutex::new(conn)));
        let run = BrowserTaskRun {
            run_id: "run-checkpoint".to_string(),
            session_id: "session-1".to_string(),
            task: "Long research".to_string(),
            status: BrowserTaskStatus::PausedCheckpointed,
            steps: vec![],
        };
        store.persist_run(&run).unwrap();
        store
            .persist_checkpoint(
                &run,
                4,
                Some("tab-9"),
                None,
                serde_json::json!({"loop": "state"}),
            )
            .unwrap();

        let checkpoint = store
            .latest_checkpoint("run-checkpoint")
            .unwrap()
            .expect("checkpoint exists");
        assert_eq!(checkpoint.run_id, "run-checkpoint");
        assert_eq!(checkpoint.step_index, 4);
        assert_eq!(checkpoint.active_tab_id.as_deref(), Some("tab-9"));
    }

    #[test]
    fn roundtrips_paused_waiting_for_browser_runtime_status() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let store = BrowserTaskStore::new(Arc::new(Mutex::new(conn)));
        let run = BrowserTaskRun {
            run_id: "run-waiting-runtime".to_string(),
            session_id: "session-1".to_string(),
            task: "Runtime gated research".to_string(),
            status: BrowserTaskStatus::PausedWaitingForBrowserRuntime,
            steps: vec![],
        };

        store.persist_run(&run).unwrap();

        let loaded = store
            .load_run("run-waiting-runtime")
            .unwrap()
            .expect("run exists");
        assert_eq!(
            loaded.status,
            BrowserTaskStatus::PausedWaitingForBrowserRuntime
        );
    }
}
