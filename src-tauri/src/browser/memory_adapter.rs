use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::boundary::BrowserBoundaryEvent;
use crate::browser::identity::BrowserIdentityProfile;
use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus};
use crate::browser::task_store::BrowserTaskMemory;
use crate::browser::runtime_memory_policy::{
    classify_browser_evidence, BrowserRuntimeMemoryPolicyExecutor,
};
use crate::mcp::SharedMcpManager;
use crate::memory::MemoryStore;

#[cfg(test)]
const BROWSER_MEMORY_NAMESPACE: &str = "browser_task";

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
    policy_executor: BrowserRuntimeMemoryPolicyExecutor,
}

impl BrowserLongTermMemoryAdapter {
    pub fn new(memory_store: Arc<MemoryStore>, gbrain_manager: Option<SharedMcpManager>) -> Self {
        Self {
            policy_executor: BrowserRuntimeMemoryPolicyExecutor::new(memory_store, gbrain_manager),
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

        let policy_decision = classify_browser_evidence(
            event.event_id.clone(),
            event.run_id.clone(),
            serde_json::to_string(&event).unwrap_or_else(|_| "{}".into()),
            None,
        );
        let receipts = self
            .policy_executor
            .execute_decision(&policy_decision)
            .await;
        for receipt in &receipts {
            tracing::debug!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                action_id = %receipt.action_id,
                status = ?receipt.status,
                "browser long-term memory policy receipt"
            );
        }
        if receipts.is_empty() {
            tracing::warn!(
                run_id = %event.run_id,
                event_kind = event.kind.as_str(),
                "browser long-term memory policy produced no receipts"
            );
        }
    }
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
}
