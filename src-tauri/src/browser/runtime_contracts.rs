//! Browser Runtime Supervisor Phase 0 contracts.
//!
//! This module is intentionally pure metadata. It does not launch browsers,
//! spawn workers, touch profiles, call CDP, or mutate runtime state.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeState {
    Starting,
    Ready,
    Acting,
    Idle,
    Recovering,
    Degraded,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeTransition {
    pub from: BrowserRuntimeState,
    pub to: BrowserRuntimeState,
}

pub fn is_allowed_browser_runtime_transition(
    from: BrowserRuntimeState,
    to: BrowserRuntimeState,
) -> bool {
    use BrowserRuntimeState::*;

    if from == to {
        return true;
    }

    matches!(
        (from, to),
        (Stopped, Starting)
            | (Starting, Ready)
            | (Starting, Degraded)
            | (Starting, Stopped)
            | (Ready, Acting)
            | (Ready, Idle)
            | (Ready, Recovering)
            | (Ready, Degraded)
            | (Ready, Stopped)
            | (Acting, Idle)
            | (Acting, Recovering)
            | (Acting, Degraded)
            | (Acting, Stopped)
            | (Idle, Acting)
            | (Idle, Recovering)
            | (Idle, Degraded)
            | (Idle, Stopped)
            | (Recovering, Ready)
            | (Recovering, Idle)
            | (Recovering, Degraded)
            | (Recovering, Stopped)
            | (Degraded, Recovering)
            | (Degraded, Stopped)
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeFeatureFlags {
    pub playwright_cli: bool,
    pub playwright_mcp: bool,
    pub hosted_providers: bool,
    pub runtime_auto_prepare: bool,
    pub developer_upstream_fallback: bool,
    pub external_real_profile_attach: bool,
}

impl BrowserRuntimeFeatureFlags {
    pub const fn safe_defaults() -> Self {
        Self {
            playwright_cli: false,
            playwright_mcp: false,
            hosted_providers: false,
            runtime_auto_prepare: true,
            developer_upstream_fallback: false,
            external_real_profile_attach: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderLane {
    LocalChromium,
    PlaywrightCli,
    PlaywrightMcp,
    RawCdp,
    Hosted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderCapabilityCard {
    pub provider_id: &'static str,
    pub lane: BrowserProviderLane,
    pub display_name: &'static str,
    pub summary: &'static str,
    pub feature_flag: Option<&'static str>,
    pub enabled_by_default: bool,
    pub requires_runtime_pack: bool,
    pub uses_isolated_profile_by_default: bool,
    pub supports_identity: bool,
    pub allows_raw_script_by_default: bool,
    pub supported_actions: &'static [&'static str],
    pub observation_modes: &'static [&'static str],
    pub artifact_policy: &'static str,
    pub policy_tags: &'static [&'static str],
    pub harness_subjects: &'static [&'static str],
    pub disable_path: &'static str,
}

pub const BROWSER_PROVIDER_CAPABILITY_CARDS: &[BrowserProviderCapabilityCard] = &[
    BrowserProviderCapabilityCard {
        provider_id: "browser.local_chromium",
        lane: BrowserProviderLane::LocalChromium,
        display_name: "Local Chromium",
        summary: "Current chromiumoxide-backed local browser lane.",
        feature_flag: None,
        enabled_by_default: true,
        requires_runtime_pack: false,
        uses_isolated_profile_by_default: true,
        supports_identity: true,
        allows_raw_script_by_default: false,
        supported_actions: &[
            "navigate",
            "click",
            "type",
            "scroll",
            "send_keys",
            "dom_snapshot",
            "screenshot",
            "file_upload",
            "checkpoint_resume",
        ],
        observation_modes: &["dom_snapshot", "screencast", "screenshot", "task_events"],
        artifact_policy: "risk_based",
        policy_tags: &["local_first", "isolated_profile", "supervised_actions"],
        harness_subjects: &["browser.navigation", "browser.checkpoint", "browser.recovery"],
        disable_path: "Disable the local browser provider feature flag or fall back to no-browser lanes.",
    },
    BrowserProviderCapabilityCard {
        provider_id: "browser.playwright_cli",
        lane: BrowserProviderLane::PlaywrightCli,
        display_name: "Playwright CLI",
        summary: "Fast supervised short-command Playwright lane using the uClaw runtime pack.",
        feature_flag: Some("playwright_cli"),
        enabled_by_default: false,
        requires_runtime_pack: true,
        uses_isolated_profile_by_default: true,
        supports_identity: true,
        allows_raw_script_by_default: false,
        supported_actions: &["navigate", "click", "type", "screenshot", "extract", "wait"],
        observation_modes: &["locator", "accessibility_snapshot", "screenshot", "result_diff"],
        artifact_policy: "risk_based",
        policy_tags: &["runtime_pack", "short_lived_worker", "declarative_actions"],
        harness_subjects: &["browser.playwright_cli", "browser.runtime_pack"],
        disable_path: "Turn off the playwright_cli feature flag or disable the provider card.",
    },
    BrowserProviderCapabilityCard {
        provider_id: "browser.playwright_mcp",
        lane: BrowserProviderLane::PlaywrightMcp,
        display_name: "Playwright MCP",
        summary: "Supervised MCP sidecar lane for locator discovery, accessibility snapshots, and traces.",
        feature_flag: Some("playwright_mcp"),
        enabled_by_default: false,
        requires_runtime_pack: true,
        uses_isolated_profile_by_default: true,
        supports_identity: true,
        allows_raw_script_by_default: false,
        supported_actions: &["navigate", "click", "type", "screenshot", "extract", "wait", "trace"],
        observation_modes: &["accessibility_snapshot", "locator_discovery", "trace", "network_console"],
        artifact_policy: "provider_artifacts",
        policy_tags: &["runtime_pack", "mcp_sidecar", "no_raw_mcp_tools"],
        harness_subjects: &["browser.playwright_mcp", "browser.trace"],
        disable_path: "Turn off the playwright_mcp feature flag or disable the provider card.",
    },
    BrowserProviderCapabilityCard {
        provider_id: "browser.raw_cdp",
        lane: BrowserProviderLane::RawCdp,
        display_name: "Raw CDP Fallback",
        summary: "Guarded low-level fallback for compositor input, repair, dialogs, downloads, and target mechanics.",
        feature_flag: None,
        enabled_by_default: true,
        requires_runtime_pack: false,
        uses_isolated_profile_by_default: true,
        supports_identity: true,
        allows_raw_script_by_default: false,
        supported_actions: &["coordinate_click", "target_repair", "dialog", "download", "upload", "screenshot"],
        observation_modes: &["cdp_events", "screencast", "target_state"],
        artifact_policy: "failure_and_recovery",
        policy_tags: &["guarded_escape_hatch", "supervisor_only"],
        harness_subjects: &["browser.raw_cdp", "browser.recovery"],
        disable_path: "Disable the raw CDP fallback policy lane.",
    },
    BrowserProviderCapabilityCard {
        provider_id: "browser.hosted",
        lane: BrowserProviderLane::Hosted,
        display_name: "Hosted Browser Provider",
        summary: "Opt-in remote provider lane for hostile sites, scaling, proxy, or deployment constraints.",
        feature_flag: Some("hosted_providers"),
        enabled_by_default: false,
        requires_runtime_pack: false,
        uses_isolated_profile_by_default: true,
        supports_identity: false,
        allows_raw_script_by_default: false,
        supported_actions: &["navigate", "click", "type", "screenshot", "extract", "wait"],
        observation_modes: &["remote_browser", "artifact_replay", "manual_takeover"],
        artifact_policy: "explicit_data_boundary",
        policy_tags: &["opt_in", "data_boundary", "cost_visible"],
        harness_subjects: &["browser.hosted", "browser.data_boundary"],
        disable_path: "Turn off hosted_providers or remove the hosted provider credential.",
    },
];

pub fn browser_provider_capability_cards() -> &'static [BrowserProviderCapabilityCard] {
    BROWSER_PROVIDER_CAPABILITY_CARDS
}

pub fn browser_provider_capability_card(
    provider_id: &str,
) -> Option<&'static BrowserProviderCapabilityCard> {
    BROWSER_PROVIDER_CAPABILITY_CARDS
        .iter()
        .find(|card| card.provider_id == provider_id)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTaskEventName {
    StartupDoctorCheck,
    StartupDoctorFailed,
    RuntimeStateChanged,
    RuntimeHeartbeatMissed,
    RuntimeArtifactPackCreated,
    ProviderSelected,
    ProviderDegraded,
    ProviderRolledBack,
    IdentityAuthorized,
    IdentityRevoked,
    TaskPausedWaitingForRuntime,
    TaskPausedCheckpointed,
}

impl BrowserTaskEventName {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::StartupDoctorCheck => "browser.startup_doctor.check",
            Self::StartupDoctorFailed => "browser.startup_doctor.failed",
            Self::RuntimeStateChanged => "browser.runtime.state_changed",
            Self::RuntimeHeartbeatMissed => "browser.runtime.heartbeat_missed",
            Self::RuntimeArtifactPackCreated => "browser.runtime.artifact_pack_created",
            Self::ProviderSelected => "browser.provider.selected",
            Self::ProviderDegraded => "browser.provider.degraded",
            Self::ProviderRolledBack => "browser.provider.rolled_back",
            Self::IdentityAuthorized => "browser.identity.authorized",
            Self::IdentityRevoked => "browser.identity.revoked",
            Self::TaskPausedWaitingForRuntime => "browser.task.paused_waiting_for_runtime",
            Self::TaskPausedCheckpointed => "browser.task.paused_checkpointed",
        }
    }
}

pub const BROWSER_TASK_EVENT_NAMES: &[BrowserTaskEventName] = &[
    BrowserTaskEventName::StartupDoctorCheck,
    BrowserTaskEventName::StartupDoctorFailed,
    BrowserTaskEventName::RuntimeStateChanged,
    BrowserTaskEventName::RuntimeHeartbeatMissed,
    BrowserTaskEventName::RuntimeArtifactPackCreated,
    BrowserTaskEventName::ProviderSelected,
    BrowserTaskEventName::ProviderDegraded,
    BrowserTaskEventName::ProviderRolledBack,
    BrowserTaskEventName::IdentityAuthorized,
    BrowserTaskEventName::IdentityRevoked,
    BrowserTaskEventName::TaskPausedWaitingForRuntime,
    BrowserTaskEventName::TaskPausedCheckpointed,
];

pub fn browser_task_event_names() -> &'static [BrowserTaskEventName] {
    BROWSER_TASK_EVENT_NAMES
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartupDoctorStatus {
    NotStarted,
    Checking,
    Ready,
    Failed,
    Deferred,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserStartupDoctorProjection {
    pub status: StartupDoctorStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_check_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_check: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_code: Option<String>,
    pub detail_visible: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProjection {
    pub state: BrowserRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_artifact_pack_ref: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityMode {
    Isolated,
    UclawManaged,
    ExternalProfile,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentityProjection {
    pub mode: BrowserIdentityMode,
    pub authorized: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_used_at: Option<String>,
    pub active_task_ids: Vec<String>,
    pub revoked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTaskBoundaryStatus {
    None,
    Running,
    PausedWaitingForRuntime,
    PausedCheckpointed,
    WaitingForUser,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskBoundaryProjection {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub status: BrowserTaskBoundaryStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub checkpoint_ref: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserWorldProjectionSummary {
    pub startup_doctor: BrowserStartupDoctorProjection,
    pub runtime: BrowserRuntimeProjection,
    pub identity: BrowserIdentityProjection,
    pub task_boundary: BrowserTaskBoundaryProjection,
}

impl BrowserWorldProjectionSummary {
    pub fn attention_reasons(&self) -> Vec<&'static str> {
        let mut reasons = Vec::new();

        if self.startup_doctor.status == StartupDoctorStatus::Failed {
            reasons.push("startup_doctor_failed");
        }
        if self.runtime.state == BrowserRuntimeState::Degraded {
            reasons.push("runtime_degraded");
        }
        if self.identity.revoked {
            reasons.push("identity_revoked");
        }
        match self.task_boundary.status {
            BrowserTaskBoundaryStatus::PausedWaitingForRuntime => {
                reasons.push("task_paused_waiting_for_runtime")
            }
            BrowserTaskBoundaryStatus::PausedCheckpointed => {
                reasons.push("task_paused_checkpointed")
            }
            BrowserTaskBoundaryStatus::WaitingForUser => reasons.push("task_waiting_for_user"),
            BrowserTaskBoundaryStatus::Failed => reasons.push("task_failed"),
            BrowserTaskBoundaryStatus::None
            | BrowserTaskBoundaryStatus::Running
            | BrowserTaskBoundaryStatus::Completed => {}
        }

        reasons
    }
}

#[cfg(test)]
#[path = "runtime_contracts_tests.rs"]
mod tests;
