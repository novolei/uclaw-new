//! IPC boundary for Browser runtime-pack status and no-side-effect dry runs.

use tauri::State;

use crate::app::AppState;
use crate::error::Error;

use super::runtime_control_center::BrowserRuntimeControlCenterReport;
use super::runtime_pack::{
    diagnose_runtime_pack, execute_runtime_pack_plan_dry_run, inspect_runtime_pack_status,
    plan_runtime_pack_operation, probe_runtime_pack_filesystem, BrowserRuntimePackAction,
    BrowserRuntimePackExecutionReport, BrowserRuntimePackFilesystemProbeOptions,
    BrowserRuntimePackManifest, BrowserRuntimePackNetworkState, BrowserRuntimePackOperation,
    BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths, BrowserRuntimePackPlanTrigger,
    BrowserRuntimePackStatusReport, BrowserRuntimePackStatusRequest,
};
use super::runtime_provider_probe::{
    append_probe_history, probe_provider_from_status, BrowserRuntimeProviderProbeClock,
    BrowserRuntimeProviderProbeSummary,
};
use super::runtime_status::BrowserRuntimeStatusReport;

#[tauri::command]
pub async fn get_browser_runtime_status(
    state: State<'_, AppState>,
) -> Result<BrowserRuntimeStatusReport, Error> {
    let provider_config = {
        let settings = state.settings.read().await;
        settings.browser_runtime_provider_config.clone()
    };
    state
        .browser_runtime_status_service
        .inspect_with_provider_config(provider_config)
        .await
}

#[tauri::command]
pub async fn get_browser_runtime_control_center(
    state: State<'_, AppState>,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    let status = get_browser_runtime_status(state).await?;
    Ok(status.control_center)
}

#[tauri::command]
pub async fn set_browser_runtime_provider_enabled(
    state: State<'_, AppState>,
    provider_id: String,
    enabled: bool,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .set_enabled(&provider_id, enabled)?;
        settings.save(&state.config_path)?;
    }

    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn set_browser_runtime_provider_priority(
    state: State<'_, AppState>,
    provider_ids: Vec<String>,
) -> Result<BrowserRuntimeControlCenterReport, Error> {
    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .set_priority(provider_ids)?;
        settings.save(&state.config_path)?;
    }

    get_browser_runtime_control_center(state).await
}

#[tauri::command]
pub async fn run_browser_runtime_provider_probe(
    state: State<'_, AppState>,
    provider_id: String,
) -> Result<BrowserRuntimeProviderProbeSummary, Error> {
    let runtime_status = get_browser_runtime_status(state.clone()).await?;
    let summary = probe_provider_from_status(
        &provider_id,
        runtime_status.runtime_pack.ready && runtime_status.runtime_pack.can_run_browser_tasks,
        BrowserRuntimeProviderProbeClock::utc_now(),
    );

    {
        let mut settings = state.settings.write().await;
        settings
            .browser_runtime_provider_config
            .provider_probe_cache
            .insert(provider_id.clone(), summary.clone());
        let history = settings
            .browser_runtime_provider_config
            .provider_probe_history
            .remove(&provider_id)
            .unwrap_or_default();
        settings
            .browser_runtime_provider_config
            .provider_probe_history
            .insert(provider_id, append_probe_history(history, summary.clone()));
        settings.save(&state.config_path)?;
    }

    Ok(summary)
}

#[tauri::command]
pub async fn dry_run_browser_runtime_action(
    action: BrowserRuntimePackAction,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    dry_run_default_browser_runtime_action(action)
}

pub fn inspect_default_browser_runtime_status() -> Result<BrowserRuntimePackStatusReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    Ok(inspect_browser_runtime_status(&manifest, &paths))
}

pub fn dry_run_default_browser_runtime_action(
    action: BrowserRuntimePackAction,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    Ok(dry_run_browser_runtime_action_for_paths(
        &manifest, &paths, action,
    ))
}

fn inspect_browser_runtime_status(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) -> BrowserRuntimePackStatusReport {
    inspect_runtime_pack_status(
        manifest,
        paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
        BrowserRuntimePackStatusRequest {
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
        },
    )
}

fn dry_run_browser_runtime_action_for_paths(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    action: BrowserRuntimePackAction,
) -> BrowserRuntimePackExecutionReport {
    let filesystem = probe_runtime_pack_filesystem(
        manifest,
        paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
    );
    let doctor = diagnose_runtime_pack(manifest, &filesystem.probe);
    let operation = BrowserRuntimePackOperation::from_action(action);
    let plan = plan_runtime_pack_operation(
        manifest,
        paths,
        &doctor,
        BrowserRuntimePackOperationRequest {
            operation,
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
            active_tasks: doctor.active_tasks,
        },
    );

    execute_runtime_pack_plan_dry_run(&plan)
}

#[cfg(test)]
mod tests {
    use super::super::runtime_pack::{
        BrowserRuntimePackDoctorStatus, BrowserRuntimePackExecutionMode,
        BrowserRuntimePackExecutionStatus, BrowserRuntimePackOperation,
        BrowserRuntimePackPlanStatus,
    };
    use super::*;

    #[test]
    fn status_query_reports_missing_pack_without_creating_runtime_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = inspect_browser_runtime_status(&manifest, &paths);

        assert_eq!(report.manifest_pack_version, manifest.pack_version);
        assert_eq!(report.current_pack_dir, paths.current_pack_dir);
        assert_eq!(report.primary_action, BrowserRuntimePackAction::Prepare);
        assert_eq!(
            report.doctor.status,
            BrowserRuntimePackDoctorStatus::NeedsPrepare
        );
        assert_eq!(
            report.operation_plan.operation,
            BrowserRuntimePackOperation::Prepare
        );
        assert_eq!(
            report.operation_plan.status,
            BrowserRuntimePackPlanStatus::Planned
        );
        assert!(report.operation_plan.uses_network);
        assert!(!report.operation_plan.destructive);
        assert!(!paths.runtime_root.exists());
    }

    #[test]
    fn status_query_serializes_as_frontend_runtime_status_report() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);
        let report = inspect_browser_runtime_status(&manifest, &paths);

        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(value["manifestPackVersion"], manifest.pack_version);
        assert_eq!(value["primaryAction"], "prepare");
        assert_eq!(value["doctor"]["status"], "needs_prepare");
        assert_eq!(value["operationPlan"]["status"], "planned");
        assert_eq!(value["eventNames"][0], "browser.runtime.manifest.checked",);
    }

    #[test]
    fn dry_run_action_reports_plan_without_creating_runtime_files() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = dry_run_browser_runtime_action_for_paths(
            &manifest,
            &paths,
            BrowserRuntimePackAction::Prepare,
        );

        assert_eq!(report.operation, BrowserRuntimePackOperation::Prepare);
        assert_eq!(report.mode, BrowserRuntimePackExecutionMode::DryRun);
        assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
        assert!(report.uses_network);
        assert!(!report.destructive);
        assert!(!paths.runtime_root.exists());
        assert!(report
            .event_names
            .iter()
            .any(|name| name == "browser.runtime.prepare.dry_run_succeeded"));
    }

    #[test]
    fn dry_run_action_serializes_frontend_execution_report() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);

        let report = dry_run_browser_runtime_action_for_paths(
            &manifest,
            &paths,
            BrowserRuntimePackAction::Prepare,
        );
        let value = serde_json::to_value(&report).expect("serialize execution report");

        assert_eq!(value["operation"], "prepare");
        assert_eq!(value["mode"], "dry_run");
        assert_eq!(value["destructive"], false);
        assert_eq!(value["stepReports"][0]["status"], "would_run");
        assert!(value["artifactId"]
            .as_str()
            .expect("artifact id")
            .contains("prepare"));
        assert!(value["artifactId"]
            .as_str()
            .expect("artifact id")
            .contains("succeeded"));
    }
}
