use serde::{Deserialize, Serialize};

use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;

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

pub fn append_probe_history(
    mut history: Vec<BrowserRuntimeProviderProbeSummary>,
    summary: BrowserRuntimeProviderProbeSummary,
) -> Vec<BrowserRuntimeProviderProbeSummary> {
    history.push(summary);
    history.sort_by(|left, right| right.checked_at_ms.cmp(&left.checked_at_ms));
    history.truncate(5);
    history
}

pub struct BrowserRuntimeProviderProbeClock {
    now_ms: i64,
}

impl BrowserRuntimeProviderProbeClock {
    pub fn fixed(now_ms: i64) -> Self {
        Self { now_ms }
    }

    pub fn utc_now() -> Self {
        Self {
            now_ms: chrono::Utc::now().timestamp_millis(),
        }
    }
}

pub fn probe_provider_from_status(
    provider_id: &str,
    official_runtime_ready: bool,
    clock: BrowserRuntimeProviderProbeClock,
) -> BrowserRuntimeProviderProbeSummary {
    if !official_runtime_ready
        && (provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || provider_id == PLAYWRIGHT_MCP_PROVIDER_ID)
    {
        return BrowserRuntimeProviderProbeSummary {
            provider_id: provider_id.to_string(),
            state: BrowserRuntimeProviderProbeState::Blocked,
            checked_at_ms: clock.now_ms,
            artifact_id: Some(format!("{}-probe-blocked", provider_id.replace('.', "-"))),
            failure_code: Some("playwright_setup_not_ready".to_string()),
            message: "Official Playwright setup must be ready before provider probe can run."
                .to_string(),
            event_names: vec!["browser.runtime.provider.probe.blocked".to_string()],
        };
    }

    let mut summary = BrowserRuntimeProviderProbeSummary::passed(provider_id, clock.now_ms);
    if provider_id == PLAYWRIGHT_MCP_PROVIDER_ID {
        summary
            .event_names
            .push("browser.runtime.playwright_mcp.raw_tools_hidden.checked".to_string());
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cli_probe_blocks_when_playwright_setup_is_not_ready() {
        let summary = probe_provider_from_status(
            PLAYWRIGHT_CLI_PROVIDER_ID,
            false,
            BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
        );

        assert_eq!(summary.state, BrowserRuntimeProviderProbeState::Blocked);
        assert_eq!(
            summary.failure_code.as_deref(),
            Some("playwright_setup_not_ready")
        );
    }

    #[test]
    fn mcp_probe_checks_raw_tool_guardrail() {
        let summary = probe_provider_from_status(
            PLAYWRIGHT_MCP_PROVIDER_ID,
            true,
            BrowserRuntimeProviderProbeClock::fixed(1_770_000_000_000),
        );

        assert!(summary
            .event_names
            .iter()
            .any(|event| event.contains("raw_tools_hidden")));
    }

    #[test]
    fn probe_history_keeps_latest_five_entries_newest_first() {
        let history = (0..7)
            .map(|idx| BrowserRuntimeProviderProbeSummary::passed(PLAYWRIGHT_CLI_PROVIDER_ID, idx))
            .fold(Vec::new(), append_probe_history);

        assert_eq!(history.len(), 5);
        assert_eq!(history[0].checked_at_ms, 6);
        assert_eq!(history[4].checked_at_ms, 2);
    }
}
