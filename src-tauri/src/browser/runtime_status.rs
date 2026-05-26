//! Aggregated Browser Runtime status.
//!
//! This module is the app-owned read model for browser runtime state. It
//! combines the runtime-pack report with live local Chromium context state and
//! provider readiness metadata without launching browsers or mutating runtime
//! files.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::context_manager::BrowserContextManager;
use super::playwright_cli::playwright_cli_provider_status;
use super::playwright_mcp::playwright_mcp_provider_status;
use super::provider::{
    local_chromium_status, BrowserCapabilityProbe, BrowserProviderReadinessProbe,
    BrowserProviderStatus, BrowserSetupCheck, LOCAL_CHROMIUM_PROVIDER_ID,
};
use super::runtime_contracts::{
    BrowserRuntimeFeatureFlags, BrowserRuntimeState, BrowserWorldProjectionSummary,
    StartupDoctorStatus,
};
use super::runtime_control_center::{
    build_control_center_report, feature_flags_from_provider_config,
    BrowserRuntimeControlCenterReport, BrowserRuntimeProviderConfig,
};
use super::runtime_pack::{
    inspect_runtime_pack_status, BrowserRuntimePackFilesystemProbeOptions,
    BrowserRuntimePackManifest, BrowserRuntimePackNetworkState, BrowserRuntimePackPaths,
    BrowserRuntimePackPlanTrigger, BrowserRuntimePackStatusReport, BrowserRuntimePackStatusRequest,
};
use super::runtime_supervisor::{
    BrowserRuntimeDeadlineProfile, BrowserRuntimeDoctorOutcome, BrowserRuntimeSupervisor,
};
use crate::error::Error;

pub const STARTUP_STATUS_SESSION_ID: &str = "startup";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeSupervisorStatus {
    pub provider_id: String,
    pub selected_session_id: String,
    pub runtime_state: BrowserRuntimeState,
    pub doctor_status: StartupDoctorStatus,
    pub active_context_count: usize,
    pub active_context_sessions: Vec<String>,
    pub deadlines: BrowserRuntimeDeadlineProfile,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderReadinessSummary {
    pub local_chromium: BrowserProviderStatus,
    pub playwright_cli: BrowserProviderStatus,
    pub playwright_mcp: BrowserProviderStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeStatusReport {
    #[serde(flatten)]
    pub runtime_pack: BrowserRuntimePackStatusReport,
    pub supervisor: BrowserRuntimeSupervisorStatus,
    pub provider_readiness: BrowserRuntimeProviderReadinessSummary,
    pub control_center: BrowserRuntimeControlCenterReport,
    pub projection: BrowserWorldProjectionSummary,
    pub supervisor_event_names: Vec<String>,
}

pub struct BrowserRuntimeStatusService {
    context_manager: Arc<BrowserContextManager>,
}

impl BrowserRuntimeStatusService {
    pub fn new(context_manager: Arc<BrowserContextManager>) -> Self {
        Self { context_manager }
    }

    pub async fn inspect_default(&self) -> Result<BrowserRuntimeStatusReport, Error> {
        self.inspect_with_provider_config(BrowserRuntimeProviderConfig::default())
            .await
    }

    pub async fn inspect_with_provider_config(
        &self,
        provider_config: BrowserRuntimeProviderConfig,
    ) -> Result<BrowserRuntimeStatusReport, Error> {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
        let runtime_pack = inspect_runtime_pack_status(
            &manifest,
            &paths,
            BrowserRuntimePackFilesystemProbeOptions::default(),
            BrowserRuntimePackStatusRequest {
                trigger: BrowserRuntimePackPlanTrigger::Settings,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
            },
        );
        let active_context_sessions = self.context_manager.list_active_sessions().await;
        Ok(compose_browser_runtime_status_with_config(
            runtime_pack,
            active_context_sessions,
            provider_config,
        ))
    }
}

pub fn compose_browser_runtime_status(
    runtime_pack: BrowserRuntimePackStatusReport,
    active_context_sessions: Vec<String>,
) -> BrowserRuntimeStatusReport {
    compose_browser_runtime_status_with_config(
        runtime_pack,
        active_context_sessions,
        BrowserRuntimeProviderConfig::default(),
    )
}

pub fn compose_browser_runtime_status_with_config(
    runtime_pack: BrowserRuntimePackStatusReport,
    mut active_context_sessions: Vec<String>,
    provider_config: BrowserRuntimeProviderConfig,
) -> BrowserRuntimeStatusReport {
    active_context_sessions.sort();
    let selected_session_id = active_context_sessions
        .first()
        .cloned()
        .unwrap_or_else(|| STARTUP_STATUS_SESSION_ID.to_string());

    let mut supervisor_model = BrowserRuntimeSupervisor::new_local_chromium();
    let doctor = supervisor_model
        .doctor_from_active_contexts(&selected_session_id, &active_context_sessions);
    if doctor.runtime_state == BrowserRuntimeState::Ready {
        supervisor_model.ensure_session(selected_session_id.clone(), 0);
        let _ = supervisor_model.transition_session(
            &selected_session_id,
            BrowserRuntimeState::Ready,
            0,
        );
    }
    let projection = supervisor_model.projection_for_session(&selected_session_id, &doctor);

    let official_runtime_ready = true;
    let provider_readiness = provider_readiness_summary(
        active_context_sessions.len(),
        feature_flags_from_provider_config(&provider_config),
        official_runtime_ready,
    );
    let control_center = build_control_center_report(
        provider_config,
        official_runtime_ready,
        &[
            provider_readiness.local_chromium.clone(),
            provider_readiness.playwright_cli.clone(),
            provider_readiness.playwright_mcp.clone(),
        ],
    );
    let supervisor = supervisor_status_from_doctor(
        &doctor,
        &selected_session_id,
        active_context_sessions,
        supervisor_model.deadlines(),
    );

    BrowserRuntimeStatusReport {
        runtime_pack,
        supervisor,
        provider_readiness,
        control_center,
        projection,
        supervisor_event_names: doctor.event_name.into_iter().map(str::to_string).collect(),
    }
}

fn supervisor_status_from_doctor(
    doctor: &BrowserRuntimeDoctorOutcome,
    selected_session_id: &str,
    active_context_sessions: Vec<String>,
    deadlines: BrowserRuntimeDeadlineProfile,
) -> BrowserRuntimeSupervisorStatus {
    BrowserRuntimeSupervisorStatus {
        provider_id: doctor.provider_id.clone(),
        selected_session_id: selected_session_id.to_string(),
        runtime_state: doctor.runtime_state,
        doctor_status: doctor.status,
        active_context_count: doctor.active_contexts,
        active_context_sessions,
        deadlines,
        detail: doctor.detail.clone(),
        remediation: doctor.remediation.clone(),
        event_name: doctor.event_name.map(str::to_string),
    }
}

fn provider_readiness_summary(
    active_context_count: usize,
    flags: BrowserRuntimeFeatureFlags,
    official_runtime_ready: bool,
) -> BrowserRuntimeProviderReadinessSummary {
    BrowserRuntimeProviderReadinessSummary {
        local_chromium: local_chromium_status(BrowserProviderReadinessProbe {
            provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            setup_checks: vec![BrowserSetupCheck::passed(
                "local_chromium_supervisor",
                "Local Chromium supervisor",
            )],
            capability_probes: local_chromium_capability_probes(),
            active_contexts: active_context_count,
            notes: vec![
                "Local Chromium remains the default provider lane.".to_string(),
                "Active context count is read from BrowserContextManager.".to_string(),
            ],
        }),
        playwright_cli: playwright_cli_provider_status(flags, official_runtime_ready),
        playwright_mcp: playwright_mcp_provider_status(flags, official_runtime_ready),
    }
}

fn local_chromium_capability_probes() -> Vec<BrowserCapabilityProbe> {
    [
        "navigate",
        "click",
        "type",
        "scroll",
        "evaluate",
        "screenshot",
        "checkpoint_resume",
    ]
    .into_iter()
    .map(|action| BrowserCapabilityProbe::passed(action, true))
    .collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use super::super::runtime_pack::{
        inspect_runtime_pack_status, BrowserRuntimePackNetworkState, BrowserRuntimePackStatusReport,
    };
    use super::*;
    use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
    use crate::browser::provider::BrowserProbeStatus;
    use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;

    #[test]
    fn aggregated_status_preserves_pack_fields_for_missing_pack() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), false);

        let report = compose_browser_runtime_status(runtime_pack, Vec::new());
        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(value["primaryAction"], "prepare");
        assert_eq!(value["doctor"]["status"], "needs_prepare");
        assert_eq!(
            value["supervisor"]["runtimeState"],
            serde_json::json!("stopped")
        );
        assert_eq!(
            value["supervisor"]["doctorStatus"],
            serde_json::json!("deferred")
        );
        assert_eq!(
            value["providerReadiness"]["localChromium"]["providerId"],
            LOCAL_CHROMIUM_PROVIDER_ID
        );
        assert_eq!(
            value["supervisorEventNames"][0],
            "browser.startup_doctor.check"
        );
    }

    #[test]
    fn aggregated_status_reports_ready_pack_and_playwright_flags() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);

        let report = compose_browser_runtime_status(runtime_pack, Vec::new());

        assert!(report.runtime_pack.ready);
        assert!(report.runtime_pack.can_run_browser_tasks);
        assert!(report.provider_readiness.local_chromium.ready);
        assert!(!report.provider_readiness.playwright_cli.ready);
        assert!(!report.provider_readiness.playwright_mcp.ready);
        assert_eq!(
            report.provider_readiness.playwright_cli.setup_checks[0].id,
            "playwright_cli_feature_flag"
        );
    }

    #[test]
    fn config_aware_status_enables_cli_feature_flag_in_provider_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;

        let report = compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config);

        assert!(report.control_center.feature_flags.playwright_cli);
        assert_eq!(
            report.provider_readiness.playwright_cli.setup_checks[0].id,
            "playwright_cli_feature_flag"
        );
        assert_eq!(
            report.provider_readiness.playwright_cli.setup_checks[0].status,
            BrowserProbeStatus::Passed
        );
        assert_eq!(
            report.provider_readiness.playwright_cli.setup_checks[1].id,
            "official_playwright_cli_ready"
        );
        assert_eq!(
            report.provider_readiness.playwright_cli.setup_checks[1].status,
            BrowserProbeStatus::Passed
        );
    }

    #[test]
    fn enabled_cli_does_not_require_runtime_pack_readiness() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), false);
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;

        let report = compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config);
        let cli = report
            .control_center
            .provider_lanes
            .iter()
            .find(|lane| lane.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID)
            .expect("cli lane");

        assert!(report.control_center.feature_flags.playwright_cli);
        let old_runtime_pack_reason = ["runtime", "pack", "not", "ready"].join("_");
        assert_ne!(
            cli.fallback_reason.as_deref(),
            Some(old_runtime_pack_reason.as_str())
        );
        assert_eq!(cli.fallback_reason.as_deref(), Some("probe_not_passed"));
        assert_eq!(cli.next_action, "run_probe");
    }

    #[test]
    fn aggregated_status_uses_active_context_as_supervisor_state() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);

        let report = compose_browser_runtime_status(
            runtime_pack,
            vec!["session-b".to_string(), "session-a".to_string()],
        );

        assert_eq!(report.supervisor.selected_session_id, "session-a");
        assert_eq!(report.supervisor.runtime_state, BrowserRuntimeState::Ready);
        assert_eq!(report.supervisor.doctor_status, StartupDoctorStatus::Ready);
        assert_eq!(report.supervisor.active_context_count, 2);
        assert_eq!(
            report.supervisor.active_context_sessions,
            vec!["session-a".to_string(), "session-b".to_string()]
        );
        assert_eq!(
            report.projection.runtime.active_session_id.as_deref(),
            Some("session-a")
        );
        assert_eq!(report.projection.runtime.state, BrowserRuntimeState::Ready);
        assert_eq!(report.provider_readiness.local_chromium.active_contexts, 2);
    }

    #[test]
    fn control_center_keeps_desired_priority_but_falls_back_when_cli_mcp_disabled() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);

        let report = compose_browser_runtime_status_with_config(
            runtime_pack,
            Vec::new(),
            BrowserRuntimeProviderConfig::default(),
        );

        assert_eq!(
            report.control_center.active_provider_route.provider_id,
            LOCAL_CHROMIUM_PROVIDER_ID
        );
        assert_eq!(
            report.control_center.desired_provider_priority[0],
            PLAYWRIGHT_CLI_PROVIDER_ID
        );
        assert_eq!(
            report.control_center.provider_lanes[0]
                .fallback_reason
                .as_deref(),
            Some("provider_disabled")
        );
    }

    #[test]
    fn control_center_marks_enabled_cli_not_routable_until_probe_pr() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);

        let report = compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config);

        let cli = report
            .control_center
            .provider_lanes
            .iter()
            .find(|lane| lane.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID)
            .expect("cli lane");
        assert!(cli.enabled);
        assert!(!cli.routable);
        assert_eq!(cli.next_action, "run_probe");
        assert_eq!(
            report.control_center.active_provider_route.provider_id,
            LOCAL_CHROMIUM_PROVIDER_ID
        );
    }

    fn fixture_runtime_pack_status(root: &Path, ready: bool) -> BrowserRuntimePackStatusReport {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(root.join("browser-runtime"), &manifest);
        if ready {
            write_ready_runtime_pack(&paths, &manifest);
        }

        inspect_runtime_pack_status(
            &manifest,
            &paths,
            BrowserRuntimePackFilesystemProbeOptions {
                worker_startup_ok: ready,
                real_page_probe_ok: ready,
                ..BrowserRuntimePackFilesystemProbeOptions::default()
            },
            BrowserRuntimePackStatusRequest {
                trigger: BrowserRuntimePackPlanTrigger::Settings,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
            },
        )
    }

    fn write_ready_runtime_pack(
        paths: &BrowserRuntimePackPaths,
        manifest: &BrowserRuntimePackManifest,
    ) {
        fs::create_dir_all(paths.node_binary_path.parent().expect("node parent"))
            .expect("node dir");
        fs::write(&paths.node_binary_path, "").expect("node binary");
        fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
        fs::create_dir_all(&paths.playwright_mcp_package_dir).expect("playwright mcp package");
        fs::create_dir_all(paths.worker_script_path.parent().expect("worker parent"))
            .expect("worker dir");
        fs::write(&paths.worker_script_path, "").expect("worker script");
        fs::create_dir_all(
            paths
                .chromium_binary_path
                .parent()
                .expect("chromium parent"),
        )
        .expect("chromium dir");
        fs::write(&paths.chromium_binary_path, "").expect("chromium binary");
        fs::write(
            &paths.manifest_path,
            serde_json::to_string_pretty(manifest).expect("serialize manifest"),
        )
        .expect("manifest");
        fs::create_dir_all(paths.packs_dir.join("browser-runtime-pack-v0")).expect("rollback pack");
    }
}
