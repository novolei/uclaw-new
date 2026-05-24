//! Read-only IPC boundary for Browser runtime-pack status.

use crate::error::Error;

use super::runtime_pack::{
    inspect_runtime_pack_status, BrowserRuntimePackFilesystemProbeOptions,
    BrowserRuntimePackManifest, BrowserRuntimePackNetworkState, BrowserRuntimePackPaths,
    BrowserRuntimePackPlanTrigger, BrowserRuntimePackStatusReport, BrowserRuntimePackStatusRequest,
};

#[tauri::command]
pub async fn get_browser_runtime_status() -> Result<BrowserRuntimePackStatusReport, Error> {
    inspect_default_browser_runtime_status()
}

pub fn inspect_default_browser_runtime_status() -> Result<BrowserRuntimePackStatusReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    Ok(inspect_browser_runtime_status(&manifest, &paths))
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

#[cfg(test)]
mod tests {
    use super::super::runtime_pack::{
        BrowserRuntimePackAction, BrowserRuntimePackDoctorStatus, BrowserRuntimePackOperation,
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
}
