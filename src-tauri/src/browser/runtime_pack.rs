//! Browser Runtime Supervisor Phase 2 runtime-pack shell.
//!
//! This module defines uClaw-managed Playwright runtime-pack metadata, path
//! policy, doctor classifications, and remediation planning. It intentionally
//! does not download archives, spawn Node, run Playwright, or delete files.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackReleaseChannel {
    Stable,
    Security,
    Canary,
    Dev,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackManifest {
    pub pack_version: String,
    pub node_version: String,
    pub playwright_version: String,
    pub worker_version: String,
    pub chromium_revision: String,
    pub download_url: String,
    pub archive_size_bytes: u64,
    pub sha256: String,
    pub minimum_app_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_pack_version: Option<String>,
    pub release_channel: BrowserRuntimePackReleaseChannel,
}

impl BrowserRuntimePackManifest {
    pub fn v1_default() -> Self {
        Self {
            pack_version: "browser-runtime-pack-v1".to_string(),
            node_version: "22.16.0".to_string(),
            playwright_version: "1.53.0".to_string(),
            worker_version: "0.1.0".to_string(),
            chromium_revision: "1181".to_string(),
            download_url: "https://runtime.uclaw.local/browser-runtime-pack-v1.tar.zst".to_string(),
            archive_size_bytes: 0,
            sha256: "sha256-placeholder-for-signed-release-manifest".to_string(),
            minimum_app_version: "0.1.0".to_string(),
            rollback_pack_version: Some("browser-runtime-pack-v0".to_string()),
            release_channel: BrowserRuntimePackReleaseChannel::Stable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackPaths {
    pub runtime_root: PathBuf,
    pub packs_dir: PathBuf,
    pub current_pack_dir: PathBuf,
    pub manifest_path: PathBuf,
    pub node_binary_path: PathBuf,
    pub playwright_package_dir: PathBuf,
    pub worker_script_path: PathBuf,
    pub playwright_browsers_path: PathBuf,
    pub chromium_binary_path: PathBuf,
}

impl BrowserRuntimePackPaths {
    pub fn from_uclaw_home(manifest: &BrowserRuntimePackManifest) -> std::io::Result<Self> {
        let home = uclaw_utils_home::uclaw_home_pathbuf()?;
        Ok(Self::from_root(home.join("browser-runtime"), manifest))
    }

    pub fn from_root(
        runtime_root: impl AsRef<Path>,
        manifest: &BrowserRuntimePackManifest,
    ) -> Self {
        let runtime_root = runtime_root.as_ref().to_path_buf();
        let packs_dir = runtime_root.join("packs");
        let current_pack_dir = packs_dir.join(&manifest.pack_version);
        let playwright_browsers_path = current_pack_dir.join("ms-playwright");
        let chromium_binary_path =
            chromium_binary_path(&playwright_browsers_path, &manifest.chromium_revision);

        Self {
            manifest_path: current_pack_dir.join("runtime-pack.manifest.json"),
            node_binary_path: current_pack_dir.join("node").join("bin").join("node"),
            playwright_package_dir: current_pack_dir.join("node_modules").join("playwright"),
            worker_script_path: current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            runtime_root,
            packs_dir,
            current_pack_dir,
            playwright_browsers_path,
            chromium_binary_path,
        }
    }

    pub fn playwright_browsers_env(&self) -> (&'static str, PathBuf) {
        (
            "PLAYWRIGHT_BROWSERS_PATH",
            self.playwright_browsers_path.clone(),
        )
    }
}

fn chromium_binary_path(browsers_path: &Path, revision: &str) -> PathBuf {
    let chromium_root = browsers_path.join(format!("chromium-{revision}"));
    match std::env::consts::OS {
        "macos" => chromium_root
            .join("chrome-mac")
            .join("Chromium.app")
            .join("Contents")
            .join("MacOS")
            .join("Chromium"),
        "windows" => chromium_root.join("chrome-win").join("chrome.exe"),
        _ => chromium_root.join("chrome-linux").join("chrome"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackProbe {
    pub manifest_present: bool,
    pub node_present: bool,
    pub playwright_package_present: bool,
    pub browser_binary_present: bool,
    pub cache_corrupt: bool,
    pub versions_match: bool,
    pub worker_startup_ok: bool,
    pub offline: bool,
    pub real_page_probe_ok: bool,
    pub previous_pack_available: bool,
    pub active_tasks: usize,
}

impl BrowserRuntimePackProbe {
    pub const fn ready() -> Self {
        Self {
            manifest_present: true,
            node_present: true,
            playwright_package_present: true,
            browser_binary_present: true,
            cache_corrupt: false,
            versions_match: true,
            worker_startup_ok: true,
            offline: false,
            real_page_probe_ok: true,
            previous_pack_available: true,
            active_tasks: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackManifestLoadStatus {
    Loaded,
    Missing,
    InvalidJson,
    IoError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackManifestLoadOutcome {
    pub status: BrowserRuntimePackManifestLoadStatus,
    pub path: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<BrowserRuntimePackManifest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackFilesystemProbeOptions {
    pub active_tasks: usize,
    pub offline: bool,
    pub worker_startup_ok: bool,
    pub real_page_probe_ok: bool,
    pub cache_corrupt: bool,
}

impl Default for BrowserRuntimePackFilesystemProbeOptions {
    fn default() -> Self {
        Self {
            active_tasks: 0,
            offline: false,
            worker_startup_ok: true,
            real_page_probe_ok: true,
            cache_corrupt: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackFilesystemSnapshot {
    pub current_pack_dir: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_pack_dir: Option<PathBuf>,
    pub manifest_path: PathBuf,
    pub manifest_status: BrowserRuntimePackManifestLoadStatus,
    pub manifest_present: bool,
    pub node_present: bool,
    pub playwright_package_present: bool,
    pub worker_script_present: bool,
    pub browser_binary_present: bool,
    pub previous_pack_available: bool,
    pub versions_match: bool,
    pub cache_corrupt: bool,
    pub active_tasks: usize,
    pub offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackFilesystemProbeReport {
    pub snapshot: BrowserRuntimePackFilesystemSnapshot,
    pub probe: BrowserRuntimePackProbe,
    pub manifest_load: BrowserRuntimePackManifestLoadOutcome,
}

pub fn load_runtime_pack_manifest(path: impl AsRef<Path>) -> BrowserRuntimePackManifestLoadOutcome {
    let path = path.as_ref().to_path_buf();
    if !path.exists() {
        return BrowserRuntimePackManifestLoadOutcome {
            status: BrowserRuntimePackManifestLoadStatus::Missing,
            path,
            manifest: None,
            error: None,
        };
    }

    match fs::read_to_string(&path) {
        Ok(contents) => match serde_json::from_str::<BrowserRuntimePackManifest>(&contents) {
            Ok(manifest) => BrowserRuntimePackManifestLoadOutcome {
                status: BrowserRuntimePackManifestLoadStatus::Loaded,
                path,
                manifest: Some(manifest),
                error: None,
            },
            Err(error) => BrowserRuntimePackManifestLoadOutcome {
                status: BrowserRuntimePackManifestLoadStatus::InvalidJson,
                path,
                manifest: None,
                error: Some(error.to_string()),
            },
        },
        Err(error) => BrowserRuntimePackManifestLoadOutcome {
            status: BrowserRuntimePackManifestLoadStatus::IoError,
            path,
            manifest: None,
            error: Some(error.to_string()),
        },
    }
}

pub fn probe_runtime_pack_filesystem(
    expected_manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    options: BrowserRuntimePackFilesystemProbeOptions,
) -> BrowserRuntimePackFilesystemProbeReport {
    let manifest_load = load_runtime_pack_manifest(&paths.manifest_path);
    let previous_pack_dir = expected_manifest
        .rollback_pack_version
        .as_ref()
        .map(|version| paths.packs_dir.join(version));
    let previous_pack_available = previous_pack_dir
        .as_ref()
        .map(|path| path.exists())
        .unwrap_or(false);
    let manifest_present = !matches!(
        manifest_load.status,
        BrowserRuntimePackManifestLoadStatus::Missing
    );
    let versions_match = manifest_load
        .manifest
        .as_ref()
        .map(|installed| runtime_pack_manifest_versions_match(expected_manifest, installed))
        .unwrap_or(false);
    let cache_corrupt = options.cache_corrupt
        || matches!(
            manifest_load.status,
            BrowserRuntimePackManifestLoadStatus::InvalidJson
                | BrowserRuntimePackManifestLoadStatus::IoError
        );
    let node_present = paths.node_binary_path.exists();
    let playwright_package_present = paths.playwright_package_dir.exists();
    let worker_script_present = paths.worker_script_path.exists();
    let browser_binary_present = paths.chromium_binary_path.exists();

    let snapshot = BrowserRuntimePackFilesystemSnapshot {
        current_pack_dir: paths.current_pack_dir.clone(),
        previous_pack_dir,
        manifest_path: paths.manifest_path.clone(),
        manifest_status: manifest_load.status,
        manifest_present,
        node_present,
        playwright_package_present,
        worker_script_present,
        browser_binary_present,
        previous_pack_available,
        versions_match,
        cache_corrupt,
        active_tasks: options.active_tasks,
        offline: options.offline,
    };
    let probe = BrowserRuntimePackProbe {
        manifest_present,
        node_present,
        playwright_package_present,
        browser_binary_present,
        cache_corrupt,
        versions_match,
        worker_startup_ok: worker_script_present && options.worker_startup_ok,
        offline: options.offline,
        real_page_probe_ok: options.real_page_probe_ok,
        previous_pack_available,
        active_tasks: options.active_tasks,
    };

    BrowserRuntimePackFilesystemProbeReport {
        snapshot,
        probe,
        manifest_load,
    }
}

fn runtime_pack_manifest_versions_match(
    expected: &BrowserRuntimePackManifest,
    installed: &BrowserRuntimePackManifest,
) -> bool {
    expected.pack_version == installed.pack_version
        && expected.node_version == installed.node_version
        && expected.playwright_version == installed.playwright_version
        && expected.worker_version == installed.worker_version
        && expected.chromium_revision == installed.chromium_revision
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackStatusRequest {
    pub trigger: BrowserRuntimePackPlanTrigger,
    pub network_state: BrowserRuntimePackNetworkState,
    pub auto_prepare_enabled: bool,
    pub user_confirmed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackStatusReport {
    pub manifest_pack_version: String,
    pub runtime_root: PathBuf,
    pub current_pack_dir: PathBuf,
    pub filesystem: BrowserRuntimePackFilesystemProbeReport,
    pub doctor: BrowserRuntimePackDoctorOutcome,
    pub primary_action: BrowserRuntimePackAction,
    pub operation_plan: BrowserRuntimePackOperationPlan,
    pub ready: bool,
    pub can_run_browser_tasks: bool,
    pub event_names: Vec<String>,
}

pub fn inspect_runtime_pack_status(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    filesystem_options: BrowserRuntimePackFilesystemProbeOptions,
    request: BrowserRuntimePackStatusRequest,
) -> BrowserRuntimePackStatusReport {
    let filesystem = probe_runtime_pack_filesystem(manifest, paths, filesystem_options);
    let doctor = diagnose_runtime_pack(manifest, &filesystem.probe);
    let primary_action = doctor
        .actions
        .first()
        .copied()
        .unwrap_or(BrowserRuntimePackAction::KeepCurrent);
    let operation = BrowserRuntimePackOperation::from_action(primary_action);
    let operation_plan = plan_runtime_pack_operation(
        manifest,
        paths,
        &doctor,
        BrowserRuntimePackOperationRequest {
            operation,
            trigger: request.trigger,
            network_state: request.network_state,
            auto_prepare_enabled: request.auto_prepare_enabled,
            user_confirmed: request.user_confirmed,
            active_tasks: filesystem.probe.active_tasks,
        },
    );
    let mut event_names = vec![
        "browser.runtime.manifest.checked".to_string(),
        "browser.runtime.filesystem.probed".to_string(),
        "browser.runtime.doctor.completed".to_string(),
    ];
    event_names.extend(operation_plan.event_names.clone());

    BrowserRuntimePackStatusReport {
        manifest_pack_version: manifest.pack_version.clone(),
        runtime_root: paths.runtime_root.clone(),
        current_pack_dir: paths.current_pack_dir.clone(),
        ready: doctor.ready,
        can_run_browser_tasks: doctor.ready,
        filesystem,
        doctor,
        primary_action,
        operation_plan,
        event_names,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackIssue {
    MissingManifest,
    MissingNodeRuntime,
    MissingPlaywrightPackage,
    MissingBrowserBinary,
    CorruptCache,
    VersionMismatch,
    WorkerStartupFailure,
    OfflineDownload,
    FailedRealPageProbe,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackDoctorStatus {
    Ready,
    NeedsPrepare,
    NeedsRepair,
    NeedsUpdate,
    Deferred,
    Degraded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackAction {
    Prepare,
    Repair,
    Reinstall,
    Cleanup,
    Rollback,
    Defer,
    RetryWhenOnline,
    KeepCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackOperation {
    Prepare,
    Repair,
    Reinstall,
    Cleanup,
    Rollback,
    KeepCurrent,
}

impl BrowserRuntimePackOperation {
    pub const fn from_action(action: BrowserRuntimePackAction) -> Self {
        match action {
            BrowserRuntimePackAction::Prepare => Self::Prepare,
            BrowserRuntimePackAction::Repair => Self::Repair,
            BrowserRuntimePackAction::Reinstall => Self::Reinstall,
            BrowserRuntimePackAction::Cleanup => Self::Cleanup,
            BrowserRuntimePackAction::Rollback => Self::Rollback,
            BrowserRuntimePackAction::KeepCurrent => Self::KeepCurrent,
            BrowserRuntimePackAction::Defer | BrowserRuntimePackAction::RetryWhenOnline => {
                Self::Prepare
            }
        }
    }

    const fn event_slug(self) -> &'static str {
        match self {
            Self::Prepare => "prepare",
            Self::Repair => "repair",
            Self::Reinstall => "reinstall",
            Self::Cleanup => "cleanup",
            Self::Rollback => "rollback",
            Self::KeepCurrent => "keep_current",
        }
    }

    const fn uses_network(self) -> bool {
        matches!(self, Self::Prepare | Self::Repair | Self::Reinstall)
    }

    const fn can_disrupt_active_tasks(self) -> bool {
        matches!(self, Self::Reinstall | Self::Cleanup | Self::Rollback)
    }

    const fn is_destructive(self) -> bool {
        matches!(self, Self::Cleanup | Self::Rollback | Self::Reinstall)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackPlanTrigger {
    StartupAuto,
    TaskTime,
    Settings,
    DoctorRepair,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackNetworkState {
    Online,
    Offline,
    Metered,
    Cellular,
    CaptivePortal,
    Restricted,
}

impl BrowserRuntimePackNetworkState {
    const fn blocks_download(self) -> bool {
        matches!(self, Self::Offline | Self::CaptivePortal)
    }

    const fn needs_confirmation(self) -> bool {
        matches!(self, Self::Metered | Self::Cellular | Self::Restricted)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackPlanStatus {
    Ready,
    Planned,
    RequiresConfirmation,
    Deferred,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackPlanStepKind {
    CheckManifest,
    CheckNetworkPolicy,
    RequireUserConfirmation,
    DownloadArchive,
    VerifySha256,
    UnpackStaging,
    InstallPack,
    RunDoctor,
    PromotePack,
    RetainRollback,
    CleanupPreview,
    CleanupOldPacks,
    RestoreRollback,
    KeepCurrent,
    Defer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackPlanStep {
    pub kind: BrowserRuntimePackPlanStepKind,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub uses_network: bool,
    pub destructive: bool,
    pub requires_confirmation: bool,
}

impl BrowserRuntimePackPlanStep {
    fn new(kind: BrowserRuntimePackPlanStepKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
            path: None,
            uses_network: false,
            destructive: false,
            requires_confirmation: false,
        }
    }

    fn path(mut self, path: impl Into<PathBuf>) -> Self {
        self.path = Some(path.into());
        self
    }

    fn network(mut self) -> Self {
        self.uses_network = true;
        self
    }

    fn destructive(mut self) -> Self {
        self.destructive = true;
        self
    }

    fn confirmation(mut self) -> Self {
        self.requires_confirmation = true;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackEnvVar {
    pub name: String,
    pub value: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackOperationRequest {
    pub operation: BrowserRuntimePackOperation,
    pub trigger: BrowserRuntimePackPlanTrigger,
    pub network_state: BrowserRuntimePackNetworkState,
    pub auto_prepare_enabled: bool,
    pub user_confirmed: bool,
    pub active_tasks: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackOperationPlan {
    pub operation: BrowserRuntimePackOperation,
    pub status: BrowserRuntimePackPlanStatus,
    pub summary: String,
    pub steps: Vec<BrowserRuntimePackPlanStep>,
    pub env: Vec<BrowserRuntimePackEnvVar>,
    pub event_names: Vec<String>,
    pub manifest_pack_version: String,
    pub runtime_root: PathBuf,
    pub current_pack_dir: PathBuf,
    pub uses_network: bool,
    pub requires_confirmation: bool,
    pub keeps_current_pack: bool,
    pub destructive: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackExecutionMode {
    DryRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackExecutionStatus {
    Succeeded,
    NoOp,
    RequiresConfirmation,
    Deferred,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackStepExecutionStatus {
    WouldRun,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackStepExecutionReport {
    pub step: BrowserRuntimePackPlanStepKind,
    pub status: BrowserRuntimePackStepExecutionStatus,
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
    pub uses_network: bool,
    pub destructive: bool,
    pub requires_confirmation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackExecutionReport {
    pub operation: BrowserRuntimePackOperation,
    pub mode: BrowserRuntimePackExecutionMode,
    pub status: BrowserRuntimePackExecutionStatus,
    pub summary: String,
    pub artifact_id: String,
    pub event_names: Vec<String>,
    pub step_reports: Vec<BrowserRuntimePackStepExecutionReport>,
    pub manifest_pack_version: String,
    pub runtime_root: PathBuf,
    pub current_pack_dir: PathBuf,
    pub uses_network: bool,
    pub destructive: bool,
    pub requires_confirmation: bool,
    pub keeps_current_pack: bool,
}

pub fn execute_runtime_pack_plan_dry_run(
    plan: &BrowserRuntimePackOperationPlan,
) -> BrowserRuntimePackExecutionReport {
    let status = execution_status_for_plan(plan);
    let should_report_steps = matches!(
        status,
        BrowserRuntimePackExecutionStatus::Succeeded | BrowserRuntimePackExecutionStatus::NoOp
    );
    let step_status = if should_report_steps {
        BrowserRuntimePackStepExecutionStatus::WouldRun
    } else {
        BrowserRuntimePackStepExecutionStatus::Skipped
    };

    let step_reports = if should_report_steps {
        plan.steps
            .iter()
            .map(|step| BrowserRuntimePackStepExecutionReport {
                step: step.kind,
                status: step_status,
                label: step.label.clone(),
                path: step.path.clone(),
                uses_network: step.uses_network,
                destructive: step.destructive,
                requires_confirmation: step.requires_confirmation,
            })
            .collect()
    } else {
        Vec::new()
    };

    BrowserRuntimePackExecutionReport {
        operation: plan.operation,
        mode: BrowserRuntimePackExecutionMode::DryRun,
        status,
        summary: execution_summary_for_status(plan, status),
        artifact_id: runtime_pack_artifact_id(plan, status),
        event_names: execution_event_names(plan, status),
        step_reports,
        manifest_pack_version: plan.manifest_pack_version.clone(),
        runtime_root: plan.runtime_root.clone(),
        current_pack_dir: plan.current_pack_dir.clone(),
        uses_network: plan.uses_network,
        destructive: plan.destructive,
        requires_confirmation: plan.requires_confirmation,
        keeps_current_pack: plan.keeps_current_pack,
    }
}

fn execution_status_for_plan(
    plan: &BrowserRuntimePackOperationPlan,
) -> BrowserRuntimePackExecutionStatus {
    match plan.status {
        BrowserRuntimePackPlanStatus::Ready => BrowserRuntimePackExecutionStatus::NoOp,
        BrowserRuntimePackPlanStatus::Planned => BrowserRuntimePackExecutionStatus::Succeeded,
        BrowserRuntimePackPlanStatus::RequiresConfirmation => {
            BrowserRuntimePackExecutionStatus::RequiresConfirmation
        }
        BrowserRuntimePackPlanStatus::Deferred => BrowserRuntimePackExecutionStatus::Deferred,
        BrowserRuntimePackPlanStatus::Blocked => BrowserRuntimePackExecutionStatus::Blocked,
    }
}

fn execution_summary_for_status(
    plan: &BrowserRuntimePackOperationPlan,
    status: BrowserRuntimePackExecutionStatus,
) -> String {
    match status {
        BrowserRuntimePackExecutionStatus::Succeeded => {
            format!("Dry-run succeeded: {}", plan.summary)
        }
        BrowserRuntimePackExecutionStatus::NoOp => {
            format!("Dry-run no-op: {}", plan.summary)
        }
        BrowserRuntimePackExecutionStatus::RequiresConfirmation => {
            format!("Dry-run blocked for confirmation: {}", plan.summary)
        }
        BrowserRuntimePackExecutionStatus::Deferred => {
            format!("Dry-run deferred: {}", plan.summary)
        }
        BrowserRuntimePackExecutionStatus::Blocked => {
            format!("Dry-run blocked: {}", plan.summary)
        }
    }
}

fn runtime_pack_artifact_id(
    plan: &BrowserRuntimePackOperationPlan,
    status: BrowserRuntimePackExecutionStatus,
) -> String {
    format!(
        "browser-runtime-{}-{}-{:?}",
        plan.operation.event_slug(),
        plan.manifest_pack_version,
        status
    )
    .to_ascii_lowercase()
}

fn execution_event_names(
    plan: &BrowserRuntimePackOperationPlan,
    status: BrowserRuntimePackExecutionStatus,
) -> Vec<String> {
    let mut events = plan.event_names.clone();
    let suffix = match status {
        BrowserRuntimePackExecutionStatus::Succeeded => "dry_run_succeeded",
        BrowserRuntimePackExecutionStatus::NoOp => "dry_run_noop",
        BrowserRuntimePackExecutionStatus::RequiresConfirmation => "dry_run_confirmation_required",
        BrowserRuntimePackExecutionStatus::Deferred => "dry_run_deferred",
        BrowserRuntimePackExecutionStatus::Blocked => "dry_run_blocked",
    };
    events.push(format!(
        "browser.runtime.{}.{}",
        plan.operation.event_slug(),
        suffix
    ));
    events
}

pub fn plan_runtime_pack_operation(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    doctor: &BrowserRuntimePackDoctorOutcome,
    request: BrowserRuntimePackOperationRequest,
) -> BrowserRuntimePackOperationPlan {
    let mut plan = BrowserRuntimePackOperationPlan {
        operation: request.operation,
        status: BrowserRuntimePackPlanStatus::Planned,
        summary: "Browser runtime operation is planned.".to_string(),
        steps: Vec::new(),
        env: vec![BrowserRuntimePackEnvVar {
            name: "PLAYWRIGHT_BROWSERS_PATH".to_string(),
            value: paths.playwright_browsers_path.clone(),
        }],
        event_names: Vec::new(),
        manifest_pack_version: manifest.pack_version.clone(),
        runtime_root: paths.runtime_root.clone(),
        current_pack_dir: paths.current_pack_dir.clone(),
        uses_network: request.operation.uses_network(),
        requires_confirmation: false,
        keeps_current_pack: request.active_tasks > 0,
        destructive: request.operation.is_destructive(),
    };

    plan.steps.push(
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::CheckManifest,
            "Read the pinned Browser runtime manifest.",
        )
        .path(paths.manifest_path.clone()),
    );

    if request.operation == BrowserRuntimePackOperation::KeepCurrent {
        plan.status = BrowserRuntimePackPlanStatus::Ready;
        plan.summary = "Current Browser runtime pack remains selected.".to_string();
        plan.keeps_current_pack = true;
        plan.steps.push(BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::KeepCurrent,
            "Keep the current Browser runtime pack.",
        ));
        plan.event_names
            .push("browser.runtime.keep_current.planned".to_string());
        return plan;
    }

    if !request.auto_prepare_enabled
        && request.trigger == BrowserRuntimePackPlanTrigger::StartupAuto
        && request.operation.uses_network()
    {
        plan.status = BrowserRuntimePackPlanStatus::Deferred;
        plan.summary =
            "Startup auto-preparation is disabled; defer runtime preparation until task-time or Settings."
                .to_string();
        plan.keeps_current_pack = true;
        plan.steps.push(BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::Defer,
            "Wait for a task-time or Settings confirmation before preparing the runtime.",
        ));
        push_event(&mut plan, "deferred");
        return plan;
    }

    if request.operation.uses_network() {
        plan.steps.push(BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::CheckNetworkPolicy,
            "Check network policy before downloading the runtime pack.",
        ));

        if request.network_state.blocks_download()
            || doctor.status == BrowserRuntimePackDoctorStatus::Deferred
        {
            plan.status = BrowserRuntimePackPlanStatus::Deferred;
            plan.summary =
                "Runtime preparation is deferred until network access is available.".to_string();
            plan.keeps_current_pack = true;
            plan.steps.push(BrowserRuntimePackPlanStep::new(
                BrowserRuntimePackPlanStepKind::Defer,
                "Retry runtime preparation when the network is available.",
            ));
            push_event(&mut plan, "deferred");
            return plan;
        }
    }

    if request.operation.can_disrupt_active_tasks() && request.active_tasks > 0 {
        plan.status = BrowserRuntimePackPlanStatus::Deferred;
        plan.summary =
            "Runtime operation is deferred because active browser tasks are using the current pack."
                .to_string();
        plan.keeps_current_pack = true;
        plan.steps.push(BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::KeepCurrent,
            "Keep the current pack until active browser tasks finish.",
        ));
        push_event(&mut plan, "deferred");
        return plan;
    }

    if request.operation == BrowserRuntimePackOperation::Rollback && !doctor.rollback_available {
        plan.status = BrowserRuntimePackPlanStatus::Blocked;
        plan.summary = "Runtime rollback is blocked because no previous working pack is available."
            .to_string();
        plan.steps.push(BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::KeepCurrent,
            "Keep the current pack and request repair instead of rollback.",
        ));
        push_event(&mut plan, "blocked");
        return plan;
    }

    let large_download = manifest.archive_size_bytes >= 200 * 1024 * 1024;
    let confirmation_reason = request.operation.is_destructive()
        || (request.operation.uses_network()
            && (large_download || request.network_state.needs_confirmation()));

    if confirmation_reason && !request.user_confirmed {
        plan.status = BrowserRuntimePackPlanStatus::RequiresConfirmation;
        plan.summary = "Runtime operation requires lightweight user confirmation.".to_string();
        plan.requires_confirmation = true;
        plan.steps.push(
            BrowserRuntimePackPlanStep::new(
                BrowserRuntimePackPlanStepKind::RequireUserConfirmation,
                "Ask the user to confirm this Browser runtime operation.",
            )
            .confirmation(),
        );
        push_event(&mut plan, "confirmation_required");
        return plan;
    }

    match request.operation {
        BrowserRuntimePackOperation::Prepare => add_prepare_steps(&mut plan, manifest, paths),
        BrowserRuntimePackOperation::Repair => add_repair_steps(&mut plan, manifest, paths),
        BrowserRuntimePackOperation::Reinstall => add_reinstall_steps(&mut plan, manifest, paths),
        BrowserRuntimePackOperation::Cleanup => add_cleanup_steps(&mut plan, paths),
        BrowserRuntimePackOperation::Rollback => add_rollback_steps(&mut plan, paths),
        BrowserRuntimePackOperation::KeepCurrent => {}
    }

    if request.active_tasks == 0 {
        plan.keeps_current_pack = matches!(
            request.operation,
            BrowserRuntimePackOperation::Cleanup | BrowserRuntimePackOperation::KeepCurrent
        );
    }

    push_event(&mut plan, "planned");
    plan
}

fn add_prepare_steps(
    plan: &mut BrowserRuntimePackOperationPlan,
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) {
    plan.summary = "Prepare the pinned Browser runtime pack in uClaw-managed storage.".to_string();
    plan.steps.extend([
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::DownloadArchive,
            format!("Download runtime pack from {}.", manifest.download_url),
        )
        .network(),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::VerifySha256,
            format!("Verify runtime pack sha256 {}.", manifest.sha256),
        ),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::UnpackStaging,
            "Unpack runtime pack into a staging directory.",
        )
        .path(paths.current_pack_dir.with_extension("staging")),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::InstallPack,
            "Install the staged runtime pack.",
        )
        .path(paths.current_pack_dir.clone()),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::RunDoctor,
            "Run Browser runtime doctor before promotion.",
        ),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::PromotePack,
            "Promote the verified runtime pack.",
        )
        .path(paths.current_pack_dir.clone()),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::RetainRollback,
            "Retain the previous working runtime pack for rollback.",
        ),
    ]);
}

fn add_repair_steps(
    plan: &mut BrowserRuntimePackOperationPlan,
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) {
    add_prepare_steps(plan, manifest, paths);
    plan.operation = BrowserRuntimePackOperation::Repair;
    plan.summary = "Repair the Browser runtime pack and retain rollback evidence.".to_string();
}

fn add_reinstall_steps(
    plan: &mut BrowserRuntimePackOperationPlan,
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) {
    plan.steps.push(
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::CleanupPreview,
            "Preview the current runtime pack cleanup before reinstall.",
        )
        .path(paths.current_pack_dir.clone()),
    );
    add_prepare_steps(plan, manifest, paths);
    plan.operation = BrowserRuntimePackOperation::Reinstall;
    plan.summary = "Reinstall the Browser runtime pack after confirmation.".to_string();
}

fn add_cleanup_steps(plan: &mut BrowserRuntimePackOperationPlan, paths: &BrowserRuntimePackPaths) {
    plan.summary = "Plan Browser runtime cleanup without deleting files in this step.".to_string();
    plan.keeps_current_pack = true;
    plan.steps.extend([
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::CleanupPreview,
            "Preview runtime-pack cache and old-pack cleanup candidates.",
        )
        .path(paths.packs_dir.clone()),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::CleanupOldPacks,
            "Cleanup old runtime packs after explicit executor confirmation.",
        )
        .path(paths.packs_dir.clone())
        .destructive(),
    ]);
}

fn add_rollback_steps(plan: &mut BrowserRuntimePackOperationPlan, paths: &BrowserRuntimePackPaths) {
    plan.summary = "Roll back to the previous working Browser runtime pack.".to_string();
    plan.steps.extend([
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::RestoreRollback,
            "Restore the previous working runtime pack.",
        )
        .path(paths.packs_dir.clone())
        .destructive(),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::RunDoctor,
            "Run Browser runtime doctor on the rollback pack.",
        ),
        BrowserRuntimePackPlanStep::new(
            BrowserRuntimePackPlanStepKind::PromotePack,
            "Promote the rollback pack if doctor passes.",
        ),
    ]);
}

fn push_event(plan: &mut BrowserRuntimePackOperationPlan, suffix: &str) {
    plan.event_names.push(format!(
        "browser.runtime.{}.{}",
        plan.operation.event_slug(),
        suffix
    ));
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackDoctorOutcome {
    pub status: BrowserRuntimePackDoctorStatus,
    pub ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issue: Option<BrowserRuntimePackIssue>,
    pub remediation: String,
    pub actions: Vec<BrowserRuntimePackAction>,
    pub manifest_pack_version: String,
    pub rollback_available: bool,
    pub active_tasks: usize,
}

pub fn diagnose_runtime_pack(
    manifest: &BrowserRuntimePackManifest,
    probe: &BrowserRuntimePackProbe,
) -> BrowserRuntimePackDoctorOutcome {
    let (status, issue, remediation, actions) = if probe.offline
        && (!probe.manifest_present
            || !probe.node_present
            || !probe.playwright_package_present
            || !probe.browser_binary_present)
    {
        (
            BrowserRuntimePackDoctorStatus::Deferred,
            Some(BrowserRuntimePackIssue::OfflineDownload),
            "Browser runtime preparation is waiting for network access.".to_string(),
            vec![
                BrowserRuntimePackAction::RetryWhenOnline,
                BrowserRuntimePackAction::Defer,
            ],
        )
    } else if !probe.manifest_present {
        (
            BrowserRuntimePackDoctorStatus::NeedsPrepare,
            Some(BrowserRuntimePackIssue::MissingManifest),
            "Prepare the Browser runtime pack before running Playwright providers.".to_string(),
            vec![BrowserRuntimePackAction::Prepare],
        )
    } else if !probe.node_present {
        (
            BrowserRuntimePackDoctorStatus::NeedsPrepare,
            Some(BrowserRuntimePackIssue::MissingNodeRuntime),
            "Install or repair the pinned Node runtime in the Browser runtime pack.".to_string(),
            vec![
                BrowserRuntimePackAction::Prepare,
                BrowserRuntimePackAction::Repair,
            ],
        )
    } else if !probe.playwright_package_present {
        (
            BrowserRuntimePackDoctorStatus::NeedsPrepare,
            Some(BrowserRuntimePackIssue::MissingPlaywrightPackage),
            "Install or repair the pinned Playwright package in the Browser runtime pack."
                .to_string(),
            vec![
                BrowserRuntimePackAction::Prepare,
                BrowserRuntimePackAction::Repair,
            ],
        )
    } else if !probe.browser_binary_present {
        (
            BrowserRuntimePackDoctorStatus::NeedsPrepare,
            Some(BrowserRuntimePackIssue::MissingBrowserBinary),
            "Install or repair the pinned Chromium binary in the Browser runtime pack.".to_string(),
            vec![
                BrowserRuntimePackAction::Prepare,
                BrowserRuntimePackAction::Repair,
            ],
        )
    } else if probe.cache_corrupt {
        (
            BrowserRuntimePackDoctorStatus::NeedsRepair,
            Some(BrowserRuntimePackIssue::CorruptCache),
            "Repair the Browser runtime cache; cleanup is available after checkpointing."
                .to_string(),
            repair_actions(probe),
        )
    } else if !probe.versions_match {
        (
            BrowserRuntimePackDoctorStatus::NeedsUpdate,
            Some(BrowserRuntimePackIssue::VersionMismatch),
            "Update the Browser runtime pack when the app is idle; keep the current pack for active tasks."
                .to_string(),
            if probe.active_tasks > 0 {
                vec![BrowserRuntimePackAction::KeepCurrent, BrowserRuntimePackAction::Defer]
            } else {
                vec![BrowserRuntimePackAction::Prepare]
            },
        )
    } else if !probe.worker_startup_ok {
        (
            BrowserRuntimePackDoctorStatus::NeedsRepair,
            Some(BrowserRuntimePackIssue::WorkerStartupFailure),
            "Repair the Browser runtime worker or roll back to the previous working pack."
                .to_string(),
            repair_actions(probe),
        )
    } else if !probe.real_page_probe_ok {
        (
            BrowserRuntimePackDoctorStatus::Degraded,
            Some(BrowserRuntimePackIssue::FailedRealPageProbe),
            "Browser runtime exists but failed the real-page probe; keep artifacts and retry after repair."
                .to_string(),
            repair_actions(probe),
        )
    } else {
        (
            BrowserRuntimePackDoctorStatus::Ready,
            None,
            "Browser runtime pack is ready.".to_string(),
            vec![BrowserRuntimePackAction::KeepCurrent],
        )
    };

    BrowserRuntimePackDoctorOutcome {
        status,
        ready: status == BrowserRuntimePackDoctorStatus::Ready,
        issue,
        remediation,
        actions,
        manifest_pack_version: manifest.pack_version.clone(),
        rollback_available: probe.previous_pack_available
            || manifest.rollback_pack_version.is_some(),
        active_tasks: probe.active_tasks,
    }
}

fn repair_actions(probe: &BrowserRuntimePackProbe) -> Vec<BrowserRuntimePackAction> {
    let mut actions = vec![BrowserRuntimePackAction::Repair];
    if probe.previous_pack_available {
        actions.push(BrowserRuntimePackAction::Rollback);
    }
    actions.push(BrowserRuntimePackAction::Cleanup);
    actions
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackUpdateKind {
    None,
    Security,
    Ordinary,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackUpdatePolicy {
    pub update_kind: BrowserRuntimePackUpdateKind,
    pub active_tasks: usize,
    pub app_idle: bool,
    pub rollback_available: bool,
    pub offline: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackUpdateDecision {
    pub action: BrowserRuntimePackAction,
    pub prompt_user: bool,
    pub keep_current_pack: bool,
    pub reason: String,
}

pub fn decide_runtime_pack_update(
    policy: BrowserRuntimePackUpdatePolicy,
) -> BrowserRuntimePackUpdateDecision {
    if policy.offline {
        return BrowserRuntimePackUpdateDecision {
            action: BrowserRuntimePackAction::RetryWhenOnline,
            prompt_user: false,
            keep_current_pack: true,
            reason: "Runtime update is deferred until network access is available.".to_string(),
        };
    }

    match policy.update_kind {
        BrowserRuntimePackUpdateKind::None => BrowserRuntimePackUpdateDecision {
            action: BrowserRuntimePackAction::KeepCurrent,
            prompt_user: false,
            keep_current_pack: true,
            reason: "Runtime pack is already current.".to_string(),
        },
        BrowserRuntimePackUpdateKind::Security => BrowserRuntimePackUpdateDecision {
            action: BrowserRuntimePackAction::Prepare,
            prompt_user: policy.active_tasks > 0,
            keep_current_pack: policy.active_tasks > 0,
            reason: "Security runtime update should be prepared with priority.".to_string(),
        },
        BrowserRuntimePackUpdateKind::Ordinary if policy.active_tasks > 0 || !policy.app_idle => {
            BrowserRuntimePackUpdateDecision {
                action: BrowserRuntimePackAction::Defer,
                prompt_user: false,
                keep_current_pack: true,
                reason: "Ordinary runtime update is deferred until idle or next launch."
                    .to_string(),
            }
        }
        BrowserRuntimePackUpdateKind::Ordinary => BrowserRuntimePackUpdateDecision {
            action: BrowserRuntimePackAction::Prepare,
            prompt_user: false,
            keep_current_pack: policy.rollback_available,
            reason: "Ordinary runtime update can be prepared during idle time.".to_string(),
        },
    }
}

#[cfg(test)]
#[path = "runtime_pack_tests.rs"]
mod tests;
