//! Playwright MCP provider contract shell.
//!
//! This module defines the feature-flagged provider readiness shape, sidecar
//! launch specification, and uClaw-level action envelope for Playwright MCP.
//! The supervised process runner lives in `playwright_mcp_sidecar`. This module
//! does not expose raw MCP tools, mutate profiles, or route Browser tasks.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::playwright_mcp_sidecar::{
    PlaywrightMcpSidecarActionResult, PlaywrightMcpSidecarArtifactKind,
    PlaywrightMcpSidecarRunnerError,
};
use super::provider::{
    BrowserCapabilityProbe, BrowserProbeStatus, BrowserProviderCapabilities,
    BrowserProviderReadinessProbe, BrowserProviderStatus, BrowserSetupCheck,
};
use super::runtime_contracts::{BrowserRuntimeFeatureFlags, BrowserTaskEventName};

pub const PLAYWRIGHT_MCP_PROVIDER_ID: &str = "browser.playwright_mcp";
pub const PLAYWRIGHT_MCP_PACKAGE_NAME: &str = "@playwright/mcp";
pub const PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_PLAYWRIGHT_MCP_ACTION_TIMEOUT_MS: u64 = 5_000;
pub const DEFAULT_PLAYWRIGHT_MCP_NAVIGATION_TIMEOUT_MS: u64 = 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpBrowserName {
    Chrome,
    Firefox,
    Webkit,
    Msedge,
}

impl PlaywrightMcpBrowserName {
    pub const fn as_arg(self) -> &'static str {
        match self {
            Self::Chrome => "chrome",
            Self::Firefox => "firefox",
            Self::Webkit => "webkit",
            Self::Msedge => "msedge",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpProfileMode {
    Isolated,
    Persistent,
    StorageState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpCapability {
    Core,
    CoreNavigation,
    CoreTabs,
    CoreInput,
    Network,
    Testing,
    Vision,
    Devtools,
}

impl PlaywrightMcpCapability {
    pub const fn as_arg(self) -> &'static str {
        match self {
            Self::Core => "core",
            Self::CoreNavigation => "core-navigation",
            Self::CoreTabs => "core-tabs",
            Self::CoreInput => "core-input",
            Self::Network => "network",
            Self::Testing => "testing",
            Self::Vision => "vision",
            Self::Devtools => "devtools",
        }
    }
}

pub const PLAYWRIGHT_MCP_DEFAULT_CAPABILITIES: &[PlaywrightMcpCapability] = &[
    PlaywrightMcpCapability::Core,
    PlaywrightMcpCapability::CoreNavigation,
    PlaywrightMcpCapability::CoreTabs,
    PlaywrightMcpCapability::CoreInput,
    PlaywrightMcpCapability::Network,
    PlaywrightMcpCapability::Testing,
];

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpSidecarSpec {
    pub provider_id: String,
    pub package_name: String,
    pub package_version: String,
    pub browser: PlaywrightMcpBrowserName,
    pub profile_mode: PlaywrightMcpProfileMode,
    pub output_dir: PathBuf,
    pub user_data_dir: PathBuf,
    pub storage_state_path: Option<PathBuf>,
    pub capabilities: Vec<PlaywrightMcpCapability>,
    pub action_timeout_ms: u64,
    pub navigation_timeout_ms: u64,
    pub expose_raw_tools: bool,
}

impl PlaywrightMcpSidecarSpec {
    pub fn package_spec(&self) -> String {
        format!("{}@{}", self.package_name, self.package_version)
    }

    pub fn args(&self) -> Vec<String> {
        let caps = self
            .capabilities
            .iter()
            .map(|capability| capability.as_arg())
            .collect::<Vec<_>>()
            .join(",");
        let mut args = vec![
            format!("--browser={}", self.browser.as_arg()),
            format!("--output-dir={}", self.output_dir.display()),
            format!("--user-data-dir={}", self.user_data_dir.display()),
            format!("--caps={caps}"),
            format!("--timeout-action={}", self.action_timeout_ms),
            format!("--timeout-navigation={}", self.navigation_timeout_ms),
        ];
        if self.profile_mode == PlaywrightMcpProfileMode::Isolated {
            args.push("--isolated".to_string());
        }
        if let Some(storage_state_path) = &self.storage_state_path {
            args.push(format!("--storage-state={}", storage_state_path.display()));
        }
        args
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightMcpSidecarSpecRequest {
    pub package_version: String,
    pub browser: PlaywrightMcpBrowserName,
    pub profile_mode: PlaywrightMcpProfileMode,
    pub output_dir: PathBuf,
    pub user_data_dir: PathBuf,
    pub storage_state_path: Option<PathBuf>,
    pub capabilities: Vec<PlaywrightMcpCapability>,
    pub action_timeout_ms: Option<u64>,
    pub navigation_timeout_ms: Option<u64>,
    pub expose_raw_tools: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaywrightMcpSidecarSpecError {
    MissingPinnedPackageVersion,
    MissingOutputDir,
    MissingUserDataDir,
    MissingStorageState,
    RawToolExposureBlocked,
}

pub fn build_playwright_mcp_sidecar_spec(
    request: PlaywrightMcpSidecarSpecRequest,
) -> Result<PlaywrightMcpSidecarSpec, PlaywrightMcpSidecarSpecError> {
    if request.package_version.trim().is_empty() {
        return Err(PlaywrightMcpSidecarSpecError::MissingPinnedPackageVersion);
    }
    if request.output_dir.as_os_str().is_empty() {
        return Err(PlaywrightMcpSidecarSpecError::MissingOutputDir);
    }
    if request.user_data_dir.as_os_str().is_empty() {
        return Err(PlaywrightMcpSidecarSpecError::MissingUserDataDir);
    }
    if request.profile_mode == PlaywrightMcpProfileMode::StorageState
        && request.storage_state_path.is_none()
    {
        return Err(PlaywrightMcpSidecarSpecError::MissingStorageState);
    }
    if request.expose_raw_tools {
        return Err(PlaywrightMcpSidecarSpecError::RawToolExposureBlocked);
    }

    Ok(PlaywrightMcpSidecarSpec {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        package_name: PLAYWRIGHT_MCP_PACKAGE_NAME.to_string(),
        package_version: request.package_version,
        browser: request.browser,
        profile_mode: request.profile_mode,
        output_dir: request.output_dir,
        user_data_dir: request.user_data_dir,
        storage_state_path: request.storage_state_path,
        capabilities: if request.capabilities.is_empty() {
            PLAYWRIGHT_MCP_DEFAULT_CAPABILITIES.to_vec()
        } else {
            request.capabilities
        },
        action_timeout_ms: request
            .action_timeout_ms
            .unwrap_or(DEFAULT_PLAYWRIGHT_MCP_ACTION_TIMEOUT_MS),
        navigation_timeout_ms: request
            .navigation_timeout_ms
            .unwrap_or(DEFAULT_PLAYWRIGHT_MCP_NAVIGATION_TIMEOUT_MS),
        expose_raw_tools: false,
    })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpRequestEnvelope {
    pub schema_version: u16,
    pub provider_id: String,
    pub request_id: String,
    pub action: PlaywrightMcpAction,
    pub timeout_ms: u64,
    pub artifact_policy: String,
    pub sidecar: PlaywrightMcpSidecarSpec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaywrightMcpEnvelopeError {
    FeatureFlagDisabled,
    RuntimeNotReady,
    RawToolExposureBlocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightMcpProviderExecutionStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpProviderArtifactRef {
    pub kind: PlaywrightMcpSidecarArtifactKind,
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

pub fn build_playwright_mcp_request_envelope(
    request_id: impl Into<String>,
    flags: BrowserRuntimeFeatureFlags,
    runtime_ready: bool,
    action: PlaywrightMcpAction,
    sidecar: PlaywrightMcpSidecarSpec,
) -> Result<PlaywrightMcpRequestEnvelope, PlaywrightMcpEnvelopeError> {
    if !flags.playwright_mcp {
        return Err(PlaywrightMcpEnvelopeError::FeatureFlagDisabled);
    }
    if !runtime_ready {
        return Err(PlaywrightMcpEnvelopeError::RuntimeNotReady);
    }
    if sidecar.expose_raw_tools {
        return Err(PlaywrightMcpEnvelopeError::RawToolExposureBlocked);
    }

    Ok(PlaywrightMcpRequestEnvelope {
        schema_version: PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id: request_id.into(),
        timeout_ms: sidecar.action_timeout_ms,
        artifact_policy: "provider_artifacts".to_string(),
        action,
        sidecar,
    })
}

pub fn playwright_mcp_provider_result_from_sidecar_result(
    sidecar_result: PlaywrightMcpSidecarActionResult,
) -> PlaywrightMcpProviderExecutionResult {
    if sidecar_result.raw_tools_exposed {
        return playwright_mcp_provider_error_result(
            sidecar_result.request_id,
            sidecar_result.action_kind,
            None,
            false,
            true,
            PlaywrightMcpProviderExecutionStatus::Failed,
            PlaywrightMcpProviderExecutionError {
                code: "raw_mcp_tools_exposed".to_string(),
                message: "Playwright MCP returned a result with raw tool exposure enabled"
                    .to_string(),
                retryable: false,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            },
        );
    }

    let artifact_refs = sidecar_result
        .artifact_refs
        .into_iter()
        .map(|artifact| PlaywrightMcpProviderArtifactRef {
            kind: artifact.kind,
            description: artifact.description,
            path: artifact.path,
            event_name: BrowserTaskEventName::RuntimeArtifactPackCreated.as_str(),
        })
        .collect();
    let action_name = sidecar_result.action_kind.as_str();
    let tool_name = sidecar_result.mcp_tool_name;

    PlaywrightMcpProviderExecutionResult {
        provider_id: sidecar_result.provider_id,
        request_id: sidecar_result.request_id,
        action_kind: sidecar_result.action_kind,
        status: PlaywrightMcpProviderExecutionStatus::Succeeded,
        summary: format!("Playwright MCP {action_name} completed via {tool_name}"),
        mcp_tool_name: Some(tool_name),
        read_only: sidecar_result.read_only,
        raw_tools_exposed: false,
        artifact_refs,
        event_name: BrowserTaskEventName::RuntimeArtifactPackCreated.as_str(),
        output: Some(sidecar_result.result),
        error: None,
    }
}

pub fn playwright_mcp_provider_result_from_runner_error(
    request_id: impl Into<String>,
    action_kind: PlaywrightMcpActionKind,
    error: PlaywrightMcpSidecarRunnerError,
) -> PlaywrightMcpProviderExecutionResult {
    let error = playwright_mcp_provider_error_from_runner_error(error);
    playwright_mcp_provider_error_result(
        request_id.into(),
        action_kind,
        None,
        false,
        false,
        PlaywrightMcpProviderExecutionStatus::Failed,
        error,
    )
}

pub fn playwright_mcp_provider_result_from_envelope_error(
    request_id: impl Into<String>,
    action_kind: PlaywrightMcpActionKind,
    error: PlaywrightMcpEnvelopeError,
) -> PlaywrightMcpProviderExecutionResult {
    let (code, message, retryable) = match error {
        PlaywrightMcpEnvelopeError::FeatureFlagDisabled => (
            "feature_flag_disabled",
            "playwright_mcp feature flag is disabled",
            false,
        ),
        PlaywrightMcpEnvelopeError::RuntimeNotReady => (
            "runtime_not_ready",
            "Browser runtime pack is not ready for Playwright MCP actions",
            true,
        ),
        PlaywrightMcpEnvelopeError::RawToolExposureBlocked => (
            "raw_tool_exposure_blocked",
            "Raw Playwright MCP tool exposure is blocked by the uClaw provider contract",
            false,
        ),
    };
    playwright_mcp_provider_error_result(
        request_id.into(),
        action_kind,
        None,
        false,
        false,
        PlaywrightMcpProviderExecutionStatus::Blocked,
        PlaywrightMcpProviderExecutionError {
            code: code.to_string(),
            message: message.to_string(),
            retryable,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: false,
        },
    )
}

fn playwright_mcp_provider_error_result(
    request_id: String,
    action_kind: PlaywrightMcpActionKind,
    mcp_tool_name: Option<String>,
    read_only: bool,
    raw_tools_exposed: bool,
    status: PlaywrightMcpProviderExecutionStatus,
    error: PlaywrightMcpProviderExecutionError,
) -> PlaywrightMcpProviderExecutionResult {
    PlaywrightMcpProviderExecutionResult {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id,
        action_kind,
        status,
        summary: error.message.clone(),
        mcp_tool_name,
        read_only,
        raw_tools_exposed,
        artifact_refs: vec![],
        event_name: error.event_name,
        output: None,
        error: Some(error),
    }
}

fn playwright_mcp_provider_error_from_runner_error(
    error: PlaywrightMcpSidecarRunnerError,
) -> PlaywrightMcpProviderExecutionError {
    match error {
        PlaywrightMcpSidecarRunnerError::RuntimePathEscapesPack { path, pack_dir } => {
            PlaywrightMcpProviderExecutionError {
                code: "runtime_path_escapes_pack".to_string(),
                message: format!(
                    "MCP sidecar runtime path {} escapes app-managed pack {}",
                    path.display(),
                    pack_dir.display()
                ),
                retryable: false,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            }
        }
        PlaywrightMcpSidecarRunnerError::SpawnFailed(message) => {
            PlaywrightMcpProviderExecutionError {
                code: "sidecar_spawn_failed".to_string(),
                message,
                retryable: true,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            }
        }
        PlaywrightMcpSidecarRunnerError::StdinUnavailable => PlaywrightMcpProviderExecutionError {
            code: "sidecar_stdin_unavailable".to_string(),
            message: "Playwright MCP sidecar stdin is unavailable".to_string(),
            retryable: true,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: true,
        },
        PlaywrightMcpSidecarRunnerError::StdoutUnavailable => PlaywrightMcpProviderExecutionError {
            code: "sidecar_stdout_unavailable".to_string(),
            message: "Playwright MCP sidecar stdout is unavailable".to_string(),
            retryable: true,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: true,
        },
        PlaywrightMcpSidecarRunnerError::StderrUnavailable => PlaywrightMcpProviderExecutionError {
            code: "sidecar_stderr_unavailable".to_string(),
            message: "Playwright MCP sidecar stderr is unavailable".to_string(),
            retryable: true,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: true,
        },
        PlaywrightMcpSidecarRunnerError::RawToolExposureBlocked => {
            PlaywrightMcpProviderExecutionError {
                code: "raw_tool_exposure_blocked".to_string(),
                message: "Raw Playwright MCP tool exposure is blocked".to_string(),
                retryable: false,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: false,
            }
        }
        PlaywrightMcpSidecarRunnerError::ExitedDuringStartup { code, stderr } => {
            PlaywrightMcpProviderExecutionError {
                code: "sidecar_exited_during_startup".to_string(),
                message: format!(
                    "Playwright MCP sidecar exited during startup with code {:?}: {}",
                    code,
                    stderr.trim()
                ),
                retryable: true,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            }
        }
        PlaywrightMcpSidecarRunnerError::JsonRpcTimeout { method, timeout_ms } => {
            PlaywrightMcpProviderExecutionError {
                code: "timeout".to_string(),
                message: format!(
                    "Playwright MCP JSON-RPC method {method} timed out after {timeout_ms} ms"
                ),
                retryable: true,
                event_name: BrowserTaskEventName::RuntimeHeartbeatMissed.as_str(),
                artifact_recommended: true,
            }
        }
        PlaywrightMcpSidecarRunnerError::JsonRpcProtocol(message) => {
            PlaywrightMcpProviderExecutionError {
                code: "json_rpc_protocol".to_string(),
                message,
                retryable: true,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            }
        }
        PlaywrightMcpSidecarRunnerError::JsonRpcError { code, message } => {
            PlaywrightMcpProviderExecutionError {
                code: format!("json_rpc_error_{code}"),
                message,
                retryable: false,
                event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
                artifact_recommended: true,
            }
        }
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
            "supervised_mcp_sidecar",
            "accessibility_snapshot",
            "locator_discovery",
            "trace_capture",
            "controlled_output_dir",
            "controlled_profile_dir",
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

    use super::super::playwright_mcp_sidecar::{
        PlaywrightMcpSidecarActionResult, PlaywrightMcpSidecarArtifactKind,
        PlaywrightMcpSidecarArtifactRef, PlaywrightMcpSidecarRunnerError,
    };
    use super::*;

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
            .actions
            .contains(&"accessibility_snapshot".to_string()));
        assert!(status
            .capabilities
            .actions
            .contains(&"discover_locators".to_string()));
        assert!(status.capabilities.actions.contains(&"trace".to_string()));
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Passed));
    }

    #[test]
    fn sidecar_spec_builds_pinned_args_with_controlled_dirs() {
        let spec = build_playwright_mcp_sidecar_spec(PlaywrightMcpSidecarSpecRequest {
            package_version: "0.0.75".to_string(),
            browser: PlaywrightMcpBrowserName::Chrome,
            profile_mode: PlaywrightMcpProfileMode::Isolated,
            output_dir: PathBuf::from("/tmp/uclaw/browser-artifacts/run-1"),
            user_data_dir: PathBuf::from("/tmp/uclaw/browser-profiles/run-1"),
            storage_state_path: None,
            capabilities: Vec::new(),
            action_timeout_ms: None,
            navigation_timeout_ms: None,
            expose_raw_tools: false,
        })
        .expect("sidecar spec");

        assert_eq!(spec.package_spec(), "@playwright/mcp@0.0.75");
        let args = spec.args();
        assert!(!args.contains(&"@playwright/mcp@0.0.75".to_string()));
        assert!(args.contains(&"--browser=chrome".to_string()));
        assert!(args.contains(&"--isolated".to_string()));
        assert!(args.iter().any(|arg| arg.starts_with("--output-dir=")));
        assert!(args.iter().any(|arg| arg.starts_with("--user-data-dir=")));
        assert!(args
            .iter()
            .any(|arg| arg == "--caps=core,core-navigation,core-tabs,core-input,network,testing"));
    }

    #[test]
    fn sidecar_spec_blocks_raw_tool_exposure() {
        let err = build_playwright_mcp_sidecar_spec(PlaywrightMcpSidecarSpecRequest {
            package_version: "0.0.75".to_string(),
            browser: PlaywrightMcpBrowserName::Chrome,
            profile_mode: PlaywrightMcpProfileMode::Isolated,
            output_dir: PathBuf::from("/tmp/out"),
            user_data_dir: PathBuf::from("/tmp/profile"),
            storage_state_path: None,
            capabilities: Vec::new(),
            action_timeout_ms: None,
            navigation_timeout_ms: None,
            expose_raw_tools: true,
        })
        .unwrap_err();

        assert_eq!(err, PlaywrightMcpSidecarSpecError::RawToolExposureBlocked);
    }

    #[test]
    fn storage_state_profile_requires_path() {
        let err = build_playwright_mcp_sidecar_spec(PlaywrightMcpSidecarSpecRequest {
            package_version: "0.0.75".to_string(),
            browser: PlaywrightMcpBrowserName::Chrome,
            profile_mode: PlaywrightMcpProfileMode::StorageState,
            output_dir: PathBuf::from("/tmp/out"),
            user_data_dir: PathBuf::from("/tmp/profile"),
            storage_state_path: None,
            capabilities: Vec::new(),
            action_timeout_ms: None,
            navigation_timeout_ms: None,
            expose_raw_tools: false,
        })
        .unwrap_err();

        assert_eq!(err, PlaywrightMcpSidecarSpecError::MissingStorageState);
    }

    #[test]
    fn envelope_uses_uclaw_actions_not_raw_mcp_tools() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_mcp = true;
        let spec = build_playwright_mcp_sidecar_spec(PlaywrightMcpSidecarSpecRequest {
            package_version: "0.0.75".to_string(),
            browser: PlaywrightMcpBrowserName::Chrome,
            profile_mode: PlaywrightMcpProfileMode::Isolated,
            output_dir: PathBuf::from("/tmp/out"),
            user_data_dir: PathBuf::from("/tmp/profile"),
            storage_state_path: None,
            capabilities: Vec::new(),
            action_timeout_ms: Some(7_500),
            navigation_timeout_ms: None,
            expose_raw_tools: false,
        })
        .expect("sidecar spec");

        let envelope = build_playwright_mcp_request_envelope(
            "req-1",
            flags,
            true,
            PlaywrightMcpAction::DiscoverLocators {
                url: Some("https://example.test".to_string()),
                goal: "Find the checkout button".to_string(),
            },
            spec,
        )
        .expect("request envelope");

        assert_eq!(envelope.timeout_ms, 7_500);
        let json = serde_json::to_string(&envelope).expect("json");
        assert!(json.contains("discover_locators"));
        assert!(!json.contains("mcp__"));
        assert!(!json.contains("browser_snapshot"));
        assert!(!json.contains("browser_click"));
    }

    #[test]
    fn sidecar_success_becomes_provider_artifact_result() {
        let result =
            playwright_mcp_provider_result_from_sidecar_result(PlaywrightMcpSidecarActionResult {
                schema_version: PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
                provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
                request_id: "req-mcp-1".to_string(),
                action_kind: PlaywrightMcpActionKind::DiscoverLocators,
                mcp_tool_name: "browser_snapshot".to_string(),
                read_only: true,
                raw_tools_exposed: false,
                artifact_refs: vec![PlaywrightMcpSidecarArtifactRef {
                    kind: PlaywrightMcpSidecarArtifactKind::LocatorDiscovery,
                    description: "MCP locator snapshot".to_string(),
                    path: Some(PathBuf::from("/tmp/uclaw/mcp/snapshot.json")),
                }],
                result: json!({
                    "content": [
                        {
                            "type": "text",
                            "text": "button Checkout [ref=e1]"
                        }
                    ]
                }),
            });

        assert_eq!(
            result.status,
            PlaywrightMcpProviderExecutionStatus::Succeeded
        );
        assert_eq!(result.provider_id, PLAYWRIGHT_MCP_PROVIDER_ID);
        assert_eq!(result.request_id, "req-mcp-1");
        assert_eq!(result.mcp_tool_name.as_deref(), Some("browser_snapshot"));
        assert!(result.read_only);
        assert!(!result.raw_tools_exposed);
        assert_eq!(
            result.event_name,
            BrowserTaskEventName::RuntimeArtifactPackCreated.as_str()
        );
        assert_eq!(result.artifact_refs.len(), 1);
        assert_eq!(
            result.artifact_refs[0].event_name,
            BrowserTaskEventName::RuntimeArtifactPackCreated.as_str()
        );
        assert!(result
            .output
            .as_ref()
            .expect("output")
            .to_string()
            .contains("Checkout"));
        assert!(result.error.is_none());
    }

    #[test]
    fn sidecar_raw_tool_exposure_result_is_provider_failure() {
        let result =
            playwright_mcp_provider_result_from_sidecar_result(PlaywrightMcpSidecarActionResult {
                schema_version: PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
                provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
                request_id: "req-raw".to_string(),
                action_kind: PlaywrightMcpActionKind::AccessibilitySnapshot,
                mcp_tool_name: "tools/list".to_string(),
                read_only: true,
                raw_tools_exposed: true,
                artifact_refs: vec![],
                result: json!({ "unexpected": true }),
            });

        assert_eq!(result.status, PlaywrightMcpProviderExecutionStatus::Failed);
        assert!(result.output.is_none());
        let error = result.error.expect("raw exposure error");
        assert_eq!(error.code, "raw_mcp_tools_exposed");
        assert!(!error.retryable);
        assert_eq!(
            error.event_name,
            BrowserTaskEventName::ProviderDegraded.as_str()
        );
    }

    #[test]
    fn sidecar_timeout_routes_to_retryable_heartbeat_error() {
        let result = playwright_mcp_provider_result_from_runner_error(
            "req-timeout",
            PlaywrightMcpActionKind::AccessibilitySnapshot,
            PlaywrightMcpSidecarRunnerError::JsonRpcTimeout {
                method: "tools/call".to_string(),
                timeout_ms: 500,
            },
        );

        assert_eq!(result.status, PlaywrightMcpProviderExecutionStatus::Failed);
        assert_eq!(
            result.event_name,
            BrowserTaskEventName::RuntimeHeartbeatMissed.as_str()
        );
        let error = result.error.expect("timeout error");
        assert_eq!(error.code, "timeout");
        assert!(error.retryable);
        assert!(error.artifact_recommended);
    }

    #[test]
    fn sidecar_json_rpc_error_routes_to_provider_degraded() {
        let result = playwright_mcp_provider_result_from_runner_error(
            "req-jsonrpc",
            PlaywrightMcpActionKind::Click,
            PlaywrightMcpSidecarRunnerError::JsonRpcError {
                code: -32000,
                message: "snapshot target stale".to_string(),
            },
        );

        assert_eq!(result.status, PlaywrightMcpProviderExecutionStatus::Failed);
        assert_eq!(
            result.event_name,
            BrowserTaskEventName::ProviderDegraded.as_str()
        );
        let error = result.error.expect("json-rpc error");
        assert_eq!(error.code, "json_rpc_error_-32000");
        assert_eq!(error.message, "snapshot target stale");
        assert!(!error.retryable);
        assert!(error.artifact_recommended);
    }

    #[test]
    fn envelope_errors_route_to_blocked_provider_result() {
        let result = playwright_mcp_provider_result_from_envelope_error(
            "req-disabled",
            PlaywrightMcpActionKind::Trace,
            PlaywrightMcpEnvelopeError::FeatureFlagDisabled,
        );

        assert_eq!(result.status, PlaywrightMcpProviderExecutionStatus::Blocked);
        assert_eq!(result.provider_id, PLAYWRIGHT_MCP_PROVIDER_ID);
        assert_eq!(result.action_kind, PlaywrightMcpActionKind::Trace);
        assert_eq!(result.artifact_refs.len(), 0);
        let error = result.error.expect("blocked error");
        assert_eq!(error.code, "feature_flag_disabled");
        assert!(!error.retryable);
        assert!(!error.artifact_recommended);
    }
}
