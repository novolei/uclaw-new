//! Browser Runtime Supervisor Phase 2 runtime-pack shell.
//!
//! This module defines uClaw-managed Playwright runtime-pack metadata, path
//! policy, doctor classifications, and remediation planning. It intentionally
//! does not download archives, spawn Node, run Playwright, or delete files.

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
