use std::{
    sync::{
        atomic::{AtomicBool, AtomicI64, Ordering},
        Arc,
    },
    time::Duration,
};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::boundary::BrowserBoundaryEvent;
use crate::browser::identity::BrowserIdentityProfile;
use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus};
use crate::browser::task_store::BrowserTaskMemory;
use crate::mcp::{CallToolResult, JsonRpcRequest, SharedMcpManager};
use crate::memory::{MemoryKind, MemoryStore, SetMemoryOpts};

const BROWSER_MEMORY_NAMESPACE: &str = "browser_task";
const GBRAIN_SERVER_ID: &str = "gbrain";
const GBRAIN_PUT_PAGE: &str = "put_page";
const GBRAIN_WRITE_TIMEOUT: Duration = Duration::from_secs(5);
const GBRAIN_WRITE_COOLDOWN_MS: i64 = 60_000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserLongTermMemoryEventKind {
    AuthProfileApplied,
    VisualObservation,
    Boundary,
    Checkpoint,
    FinalState,
}

impl BrowserLongTermMemoryEventKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::AuthProfileApplied => "auth_profile_applied",
            Self::VisualObservation => "visual_observation",
            Self::Boundary => "boundary",
            Self::Checkpoint => "checkpoint",
            Self::FinalState => "final_state",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserLongTermMemoryEvent {
    pub event_id: String,
    pub kind: BrowserLongTermMemoryEventKind,
    pub run_id: String,
    pub session_id: String,
    pub task: String,
    pub status: BrowserTaskStatus,
    pub url: Option<String>,
    pub title: Option<String>,
    pub payload: Value,
    pub created_at_ms: i64,
}

#[derive(Clone)]
pub struct BrowserLongTermMemoryAdapter {
    memory_store: Arc<MemoryStore>,
    gbrain_manager: Option<SharedMcpManager>,
    gbrain_write_in_flight: Arc<AtomicBool>,
    gbrain_write_disabled_until_ms: Arc<AtomicI64>,
}

impl BrowserLongTermMemoryAdapter {
    pub fn new(memory_store: Arc<MemoryStore>, gbrain_manager: Option<SharedMcpManager>) -> Self {
        Self {
            memory_store,
            gbrain_manager,
            gbrain_write_in_flight: Arc::new(AtomicBool::new(false)),
            gbrain_write_disabled_until_ms: Arc::new(AtomicI64::new(0)),
        }
    }

    pub async fn record_auth_profile_applied(
        &self,
        run: &BrowserTaskRun,
        profile: &BrowserIdentityProfile,
        tab_id: &str,
    ) {
        let payload = serde_json::json!({
            "profileId": profile.id,
            "label": profile.label,
            "originPattern": profile.origin_pattern,
            "kind": profile.kind,
            "provider": profile.provider,
            "scope": profile.scope,
            "status": profile.status,
            "tabId": tab_id,
        });
        self.record(
            run,
            BrowserLongTermMemoryEventKind::AuthProfileApplied,
            None,
            None,
            payload,
        )
        .await;
    }

    pub async fn record_visual_observation(&self, run: &BrowserTaskRun, observation_json: &Value) {
        let Some(visual) = observation_json.get("visualObservation") else {
            return;
        };
        let url = observation_json
            .get("url")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let title = observation_json
            .get("title")
            .and_then(|value| value.as_str())
            .map(str::to_string);
        let payload = serde_json::json!({
            "url": url,
            "title": title,
            "visualObservation": visual,
        });
        self.record(
            run,
            BrowserLongTermMemoryEventKind::VisualObservation,
            url.as_deref(),
            title.as_deref(),
            payload,
        )
        .await;
    }

    pub async fn record_boundary(&self, run: &BrowserTaskRun, boundary: &BrowserBoundaryEvent) {
        self.record(
            run,
            BrowserLongTermMemoryEventKind::Boundary,
            Some(boundary.url.as_str()),
            Some(boundary.title.as_str()),
            serde_json::to_value(boundary).unwrap_or_else(|_| Value::Null),
        )
        .await;
    }

    pub async fn record_checkpoint(
        &self,
        run: &BrowserTaskRun,
        step_index: u32,
        active_tab_id: Option<&str>,
        memory: Option<&BrowserTaskMemory>,
        reason: &str,
    ) {
        let payload = serde_json::json!({
            "stepIndex": step_index,
            "activeTabId": active_tab_id,
            "reason": reason,
            "memory": memory,
        });
        self.record(
            run,
            BrowserLongTermMemoryEventKind::Checkpoint,
            None,
            None,
            payload,
        )
        .await;
    }

    pub async fn record_final_state(&self, run: &BrowserTaskRun) {
        let payload = serde_json::json!({
            "status": run.status,
            "stepCount": run.steps.len(),
            "lastStep": run.steps.last(),
        });
        self.record(
            run,
            BrowserLongTermMemoryEventKind::FinalState,
            None,
            None,
            payload,
        )
        .await;
    }

    async fn record(
        &self,
        run: &BrowserTaskRun,
        kind: BrowserLongTermMemoryEventKind,
        url: Option<&str>,
        title: Option<&str>,
        payload: Value,
    ) {
        let event = BrowserLongTermMemoryEvent {
            event_id: uuid::Uuid::new_v4().to_string(),
            kind,
            run_id: run.run_id.clone(),
            session_id: run.session_id.clone(),
            task: run.task.clone(),
            status: run.status.clone(),
            url: url.map(str::to_string),
            title: title.map(str::to_string),
            payload,
            created_at_ms: chrono::Utc::now().timestamp_millis(),
        };

        if let Err(error) = self.write_memory_system(&event) {
            tracing::warn!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                error = %error,
                "browser long-term memory write failed"
            );
        }

        self.schedule_gbrain_write(event);
    }

    fn schedule_gbrain_write(&self, event: BrowserLongTermMemoryEvent) {
        if self.gbrain_manager.is_none() {
            tracing::debug!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                "browser long-term gbrain write skipped: manager unavailable"
            );
            return;
        }

        let now_ms = chrono::Utc::now().timestamp_millis();
        let disabled_until = self.gbrain_write_disabled_until_ms.load(Ordering::Relaxed);
        if now_ms < disabled_until {
            tracing::debug!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                disabled_until_ms = disabled_until,
                "browser long-term gbrain write skipped: cooldown active"
            );
            return;
        }

        if self
            .gbrain_write_in_flight
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            tracing::debug!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                "browser long-term gbrain write skipped: another write is in flight"
            );
            return;
        }

        let adapter = self.clone();
        tauri::async_runtime::spawn(async move {
            match tokio::time::timeout(GBRAIN_WRITE_TIMEOUT, adapter.write_gbrain(&event)).await {
                Ok(Ok(())) => {
                    tracing::debug!(
                        run_id = %event.run_id,
                        event_kind = event.kind.as_str(),
                        "browser long-term gbrain write completed"
                    );
                }
                Ok(Err(error)) => {
                    adapter.disable_gbrain_writes_for_cooldown();
                    tracing::debug!(
                        run_id = %event.run_id,
                        event_kind = event.kind.as_str(),
                        error = %error,
                        "browser long-term gbrain write skipped or failed"
                    );
                }
                Err(_) => {
                    adapter.disable_gbrain_writes_for_cooldown();
                    tracing::warn!(
                        run_id = %event.run_id,
                        event_kind = event.kind.as_str(),
                        timeout_ms = GBRAIN_WRITE_TIMEOUT.as_millis(),
                        "browser long-term gbrain write timed out"
                    );
                }
            }
            adapter
                .gbrain_write_in_flight
                .store(false, Ordering::Release);
        });
    }

    fn disable_gbrain_writes_for_cooldown(&self) {
        self.gbrain_write_disabled_until_ms.store(
            chrono::Utc::now().timestamp_millis() + GBRAIN_WRITE_COOLDOWN_MS,
            Ordering::Relaxed,
        );
    }

    fn write_memory_system(&self, event: &BrowserLongTermMemoryEvent) -> Result<()> {
        let key = format!(
            "{}:{}:{}",
            event.run_id,
            event.created_at_ms,
            event.kind.as_str()
        );
        self.memory_store.set_full(SetMemoryOpts {
            space_id: "global".to_string(),
            namespace: BROWSER_MEMORY_NAMESPACE.to_string(),
            key,
            value: serde_json::to_value(event)?,
            kind: MemoryKind::Context,
            tags: vec![
                "browser_task".to_string(),
                event.kind.as_str().to_string(),
                format!("run:{}", event.run_id),
                format!("session:{}", event.session_id),
            ],
            metadata: Some(serde_json::json!({
                "runId": event.run_id,
                "sessionId": event.session_id,
                "eventKind": event.kind,
                "url": event.url,
                "title": event.title,
            })),
            ttl_seconds: None,
        })?;
        Ok(())
    }

    async fn write_gbrain(&self, event: &BrowserLongTermMemoryEvent) -> Result<()> {
        let Some(manager) = self.gbrain_manager.as_ref() else {
            return Err(anyhow!("gbrain manager unavailable"));
        };

        let has_put_page = {
            let mgr = manager.read().await;
            mgr.all_tools()
                .iter()
                .any(|tool| tool.server_id == GBRAIN_SERVER_ID && tool.name == GBRAIN_PUT_PAGE)
        };
        if !has_put_page {
            return Err(anyhow!("gbrain put_page is not connected"));
        }

        let (transport, req_id) = {
            let mgr = manager.read().await;
            mgr.get_transport(GBRAIN_SERVER_ID)?
        };
        let request = JsonRpcRequest::call_tool(
            req_id,
            GBRAIN_PUT_PAGE,
            serde_json::json!({
                "slug": gbrain_slug(event),
                "content": gbrain_content(event),
            }),
        );
        let response = transport.send(&request).await?;
        if let Some(error) = response.error {
            return Err(anyhow!("gbrain put_page failed: {error}"));
        }
        let result: CallToolResult = serde_json::from_value(
            response
                .result
                .ok_or_else(|| anyhow!("gbrain put_page returned no result"))?,
        )?;
        if result.is_error {
            return Err(anyhow!("gbrain put_page returned isError"));
        }
        Ok(())
    }
}

fn gbrain_slug(event: &BrowserLongTermMemoryEvent) -> String {
    format!(
        "browser-tasks/{}/{}-{}",
        sanitize_slug_segment(&event.run_id),
        event.created_at_ms,
        event.kind.as_str()
    )
}

fn gbrain_content(event: &BrowserLongTermMemoryEvent) -> String {
    let title = format!(
        "Browser task {} {}",
        event.kind.as_str().replace('_', " "),
        short_id(&event.run_id)
    );
    let payload =
        serde_json::to_string_pretty(&event.payload).unwrap_or_else(|_| "null".to_string());
    format!(
        "---\ntitle: \"{}\"\ntype: browser_task_event\ntags:\n  - browser_task\n  - {}\nrun_id: {}\nsession_id: {}\n---\n\n# {}\n\n- Status: `{:?}`\n- Task: {}\n{}\n\n## Payload\n\n```json\n{}\n```\n",
        yaml_escape(&title),
        event.kind.as_str(),
        event.run_id,
        event.session_id,
        title,
        event.status,
        event.task,
        event.url.as_ref().map(|url| format!("- URL: {url}")).unwrap_or_default(),
        payload,
    )
}

fn sanitize_slug_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unknown".to_string()
    } else {
        trimmed
    }
}

fn short_id(value: &str) -> &str {
    value.get(0..8).unwrap_or(value)
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    use super::*;

    fn memory_store() -> Arc<MemoryStore> {
        let conn = Arc::new(Mutex::new(Connection::open_in_memory().unwrap()));
        let store = Arc::new(MemoryStore::new(conn));
        store.ensure_table();
        store
    }

    fn run() -> BrowserTaskRun {
        BrowserTaskRun {
            run_id: "run-123".to_string(),
            session_id: "session-abc".to_string(),
            task: "Open example and inspect CAPTCHA".to_string(),
            status: BrowserTaskStatus::NeedsUserIntervention,
            steps: Vec::new(),
        }
    }

    #[tokio::test]
    async fn writes_visual_observation_into_memory_system_without_screenshot() {
        let store = memory_store();
        let adapter = BrowserLongTermMemoryAdapter::new(store.clone(), None);
        adapter
            .record_visual_observation(
                &run(),
                &serde_json::json!({
                    "url": "https://example.test/login",
                    "title": "Login",
                    "screenshotB64": "must-not-persist",
                    "visualObservation": {
                        "screenshotRef": "browser://session/tab/1",
                        "provider": "mock",
                        "ocrText": [{"text": "captcha", "confidence": 0.9, "box": {"x": 0, "y": 0, "width": 1, "height": 1}, "source": "mock"}],
                        "detectedControls": []
                    }
                }),
            )
            .await;

        let hits = store.search_full("captcha", Some(BROWSER_MEMORY_NAMESPACE), None, None, 10);
        assert_eq!(hits.len(), 1);
        let serialized = serde_json::to_string(&hits[0].value).unwrap();
        assert!(serialized.contains("visualObservation"));
        assert!(!serialized.contains("must-not-persist"));
    }

    #[test]
    fn gbrain_slug_and_content_are_stable_and_namespaced() {
        let event = BrowserLongTermMemoryEvent {
            event_id: "event-1".to_string(),
            kind: BrowserLongTermMemoryEventKind::Checkpoint,
            run_id: "Run_ABC-123".to_string(),
            session_id: "session".to_string(),
            task: "Task".to_string(),
            status: BrowserTaskStatus::PausedCheckpointed,
            url: Some("https://example.test".to_string()),
            title: Some("Example".to_string()),
            payload: serde_json::json!({"stepIndex": 3}),
            created_at_ms: 42,
        };

        assert_eq!(
            gbrain_slug(&event),
            "browser-tasks/run-abc-123/42-checkpoint"
        );
        let content = gbrain_content(&event);
        assert!(content.contains("type: browser_task_event"));
        assert!(content.contains("browser_task"));
        assert!(content.contains("https://example.test"));
    }
}
