//! Playwright MCP provider contract shell.
//!
//! Playwright MCP is an official built-in MCP server managed by `McpManager`.
//! Browser Runtime code talks to it through `playwright_mcp_adapter`, not by
//! exposing raw MCP tools directly to the model.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::playwright_mcp_adapter::PlaywrightMcpAdapterToolCall;
use super::provider::{
    BrowserCapabilityProbe, BrowserProbeStatus, BrowserProviderCapabilities,
    BrowserProviderReadinessProbe, BrowserProviderStatus, BrowserSetupCheck,
};
use super::runtime_contracts::{BrowserRuntimeFeatureFlags, BrowserTaskEventName};

pub const PLAYWRIGHT_MCP_PROVIDER_ID: &str = "browser.playwright_mcp";
pub const PLAYWRIGHT_MCP_PACKAGE_NAME: &str = "@playwright/mcp";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpActionKind {
    AccessibilitySnapshot,
    DiscoverLocators,
    Trace,
    Navigate,
    Click,
    Type,
}

impl PlaywrightMcpActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AccessibilitySnapshot => "accessibility_snapshot",
            Self::DiscoverLocators => "discover_locators",
            Self::Trace => "trace",
            Self::Navigate => "navigate",
            Self::Click => "click",
            Self::Type => "type",
        }
    }
}

pub const PLAYWRIGHT_MCP_UCLAW_ACTIONS: &[PlaywrightMcpActionKind] = &[
    PlaywrightMcpActionKind::AccessibilitySnapshot,
    PlaywrightMcpActionKind::DiscoverLocators,
    PlaywrightMcpActionKind::Trace,
    PlaywrightMcpActionKind::Navigate,
    PlaywrightMcpActionKind::Click,
    PlaywrightMcpActionKind::Type,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaywrightMcpAction {
    AccessibilitySnapshot { url: Option<String> },
    DiscoverLocators { url: Option<String>, goal: String },
    Trace { url: Option<String>, reason: String },
    Navigate { url: String },
    Click { locator: String },
    Type { locator: String, text: String },
}

impl PlaywrightMcpAction {
    pub const fn kind(&self) -> PlaywrightMcpActionKind {
        match self {
            Self::AccessibilitySnapshot { .. } => PlaywrightMcpActionKind::AccessibilitySnapshot,
            Self::DiscoverLocators { .. } => PlaywrightMcpActionKind::DiscoverLocators,
            Self::Trace { .. } => PlaywrightMcpActionKind::Trace,
            Self::Navigate { .. } => PlaywrightMcpActionKind::Navigate,
            Self::Click { .. } => PlaywrightMcpActionKind::Click,
            Self::Type { .. } => PlaywrightMcpActionKind::Type,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpProviderExecutionStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpProviderArtifactKind {
    Snapshot,
    LocatorDiscovery,
    Trace,
    ActionResult,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpProviderArtifactRef {
    pub kind: PlaywrightMcpProviderArtifactKind,
    pub description: String,
    pub path: Option<PathBuf>,
    pub event_name: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpProviderExecutionError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    pub event_name: &'static str,
    pub artifact_recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpProviderExecutionResult {
    pub provider_id: String,
    pub request_id: String,
    pub action_kind: PlaywrightMcpActionKind,
    pub status: PlaywrightMcpProviderExecutionStatus,
    pub summary: String,
    pub mcp_tool_name: Option<String>,
    pub read_only: bool,
    pub raw_tools_exposed: bool,
    pub artifact_refs: Vec<PlaywrightMcpProviderArtifactRef>,
    pub event_name: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PlaywrightMcpProviderExecutionError>,
}

pub fn playwright_mcp_provider_result_from_adapter_call(
    request_id: impl Into<String>,
    call: &PlaywrightMcpAdapterToolCall,
    output: Value,
) -> PlaywrightMcpProviderExecutionResult {
    let action_name = call.action_kind.as_str();
    PlaywrightMcpProviderExecutionResult {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id: request_id.into(),
        action_kind: call.action_kind,
        status: PlaywrightMcpProviderExecutionStatus::Succeeded,
        summary: format!(
            "Playwright MCP {action_name} routed through built-in MCP server {}",
            call.server_id
        ),
        mcp_tool_name: Some(call.tool_name.clone()),
        read_only: call.read_only,
        raw_tools_exposed: false,
        artifact_refs: vec![PlaywrightMcpProviderArtifactRef {
            kind: artifact_kind_for_action(call.action_kind),
            description: format!("Official Playwright MCP {}", call.tool_name),
            path: None,
            event_name: BrowserTaskEventName::RuntimeArtifactPackCreated.as_str(),
        }],
        event_name: BrowserTaskEventName::RuntimeArtifactPackCreated.as_str(),
        output: Some(output),
        error: None,
    }
}

fn artifact_kind_for_action(
    action_kind: PlaywrightMcpActionKind,
) -> PlaywrightMcpProviderArtifactKind {
    match action_kind {
        PlaywrightMcpActionKind::AccessibilitySnapshot => {
            PlaywrightMcpProviderArtifactKind::Snapshot
        }
        PlaywrightMcpActionKind::DiscoverLocators => {
            PlaywrightMcpProviderArtifactKind::LocatorDiscovery
        }
        PlaywrightMcpActionKind::Trace => PlaywrightMcpProviderArtifactKind::Trace,
        PlaywrightMcpActionKind::Navigate
        | PlaywrightMcpActionKind::Click
        | PlaywrightMcpActionKind::Type => PlaywrightMcpProviderArtifactKind::ActionResult,
    }
}

pub fn playwright_mcp_capabilities() -> BrowserProviderCapabilities {
    BrowserProviderCapabilities {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        family: "browser".to_string(),
        display_name: "Playwright MCP".to_string(),
        actions: PLAYWRIGHT_MCP_UCLAW_ACTIONS
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        features: vec![
            "official_mcp_manager",
            "accessibility_snapshot",
            "locator_discovery",
            "trace_capture",
            "no_raw_mcp_tools",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        harness_subjects: vec![
            "browser.playwright_mcp",
            "browser.playwright_mcp.snapshot",
            "browser.playwright_mcp.locator_discovery",
            "browser.playwright_mcp.trace",
            "browser.playwright_mcp.disabled_fallback",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }
}

pub fn playwright_mcp_provider_status(
    flags: BrowserRuntimeFeatureFlags,
    runtime_ready: bool,
) -> BrowserProviderStatus {
    let mut notes = Vec::new();
    let setup_checks = if !flags.playwright_mcp {
        notes.push("playwright_mcp feature flag is off.".to_string());
        vec![BrowserSetupCheck::failed(
            "playwright_mcp_feature_flag",
            "Playwright MCP feature flag",
            "Enable the playwright_mcp feature flag before selecting this provider.",
        )]
    } else if !runtime_ready {
        notes.push("Official Playwright MCP setup is not ready.".to_string());
        vec![
            BrowserSetupCheck::passed("playwright_mcp_feature_flag", "Playwright MCP feature flag"),
            BrowserSetupCheck::failed(
                "official_playwright_mcp_ready",
                "Official Playwright MCP",
                "Install or repair official Playwright MCP before enabling this provider.",
            ),
        ]
    } else {
        vec![
            BrowserSetupCheck::passed("playwright_mcp_feature_flag", "Playwright MCP feature flag"),
            BrowserSetupCheck::passed("official_playwright_mcp_ready", "Official Playwright MCP"),
            BrowserSetupCheck::passed(
                "raw_mcp_tools_hidden",
                "Raw MCP tools are hidden from the model",
            ),
        ]
    };

    let capability_status = if flags.playwright_mcp && runtime_ready {
        BrowserProbeStatus::Passed
    } else {
        BrowserProbeStatus::Skipped
    };
    let capability_probes = PLAYWRIGHT_MCP_UCLAW_ACTIONS
        .iter()
        .map(|action| BrowserCapabilityProbe {
            action: action.as_str().to_string(),
            required: true,
            status: capability_status,
            remediation: None,
        })
        .collect();

    BrowserProviderStatus::from_probe(
        playwright_mcp_capabilities(),
        BrowserProviderReadinessProbe {
            provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
            setup_checks,
            capability_probes,
            active_contexts: 0,
            notes,
        },
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::browser::PlaywrightMcpAdapterToolCall;

    #[test]
    fn disabled_feature_flag_keeps_mcp_unavailable() {
        let status =
            playwright_mcp_provider_status(BrowserRuntimeFeatureFlags::safe_defaults(), true);

        assert_eq!(status.provider_id, PLAYWRIGHT_MCP_PROVIDER_ID);
        assert!(!status.ready);
        assert_eq!(
            status.readiness,
            super::super::provider::BrowserProviderReadiness::NeedsSetup
        );
        assert!(status
            .remediation
            .iter()
            .any(|item| item.contains("playwright_mcp feature flag")));
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Skipped));
    }

    #[test]
    fn official_mcp_missing_needs_setup_when_feature_enabled() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_mcp = true;

        let status = playwright_mcp_provider_status(flags, false);

        assert!(!status.ready);
        assert!(status
            .remediation
            .iter()
            .any(|item| item.contains("official Playwright MCP")));
    }

    #[test]
    fn ready_runtime_marks_uclaw_mcp_capabilities_passed() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_mcp = true;

        let status = playwright_mcp_provider_status(flags, true);

        assert!(status.ready);
        assert!(status
            .capabilities
            .features
            .contains(&"official_mcp_manager".to_string()));
        assert_eq!(
            status.capabilities.features,
            vec![
                "official_mcp_manager".to_string(),
                "accessibility_snapshot".to_string(),
                "locator_discovery".to_string(),
                "trace_capture".to_string(),
                "no_raw_mcp_tools".to_string(),
            ]
        );
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Passed));
    }

    #[test]
    fn adapter_call_becomes_provider_artifact_result_without_raw_tool_exposure() {
        let call = PlaywrightMcpAdapterToolCall::navigate("https://example.test");
        let result = playwright_mcp_provider_result_from_adapter_call(
            "req-mcp-1",
            &call,
            json!({ "routeEvidence": "selected" }),
        );

        assert_eq!(
            result.status,
            PlaywrightMcpProviderExecutionStatus::Succeeded
        );
        assert_eq!(result.provider_id, PLAYWRIGHT_MCP_PROVIDER_ID);
        assert_eq!(result.request_id, "req-mcp-1");
        assert_eq!(result.mcp_tool_name.as_deref(), Some("browser_navigate"));
        assert!(!result.read_only);
        assert!(!result.raw_tools_exposed);
        assert_eq!(
            result.artifact_refs[0].kind,
            PlaywrightMcpProviderArtifactKind::ActionResult
        );
        assert!(result.error.is_none());
    }
}
