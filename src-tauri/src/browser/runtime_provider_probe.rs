use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeProviderProbeState {
    NotRun,
    Running,
    Passed,
    Failed,
    Stale,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderProbeSummary {
    pub provider_id: String,
    pub state: BrowserRuntimeProviderProbeState,
    pub checked_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    pub message: String,
    pub event_names: Vec<String>,
}

impl BrowserRuntimeProviderProbeSummary {
    pub fn passed(provider_id: impl Into<String>, checked_at_ms: i64) -> Self {
        let provider_id = provider_id.into();
        Self {
            event_names: vec![format!("{}.probe.passed", provider_id.replace('.', "_"))],
            provider_id,
            state: BrowserRuntimeProviderProbeState::Passed,
            checked_at_ms,
            artifact_id: Some("browser-runtime-provider-probe-passed".to_string()),
            failure_code: None,
            message: "Provider probe passed.".to_string(),
        }
    }

    pub fn failed(
        provider_id: impl Into<String>,
        checked_at_ms: i64,
        failure_code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        let provider_id = provider_id.into();
        Self {
            event_names: vec![format!("{}.probe.failed", provider_id.replace('.', "_"))],
            provider_id,
            state: BrowserRuntimeProviderProbeState::Failed,
            checked_at_ms,
            artifact_id: Some("browser-runtime-provider-probe-failed".to_string()),
            failure_code: Some(failure_code.into()),
            message: message.into(),
        }
    }
}
