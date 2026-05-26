//! Local managed runner for Browser runtime-pack operation plans.
//!
//! This runner is deliberately local-first. It can install from an
//! app-managed archive/staging source supplied by the app, verify checksums,
//! run the Rust doctor boundary, clean old packs, and restore rollback packs.
//! It does not use global npm, global Playwright caches, or user-managed
//! browser installs.

use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;

use sha2::{Digest, Sha256};

use super::runtime_pack::{
    diagnose_runtime_pack, probe_runtime_pack_filesystem, BrowserRuntimePackFilesystemProbeOptions,
    BrowserRuntimePackManifest, BrowserRuntimePackPaths, BrowserRuntimePackPlanStep,
    BrowserRuntimePackPlanStepKind, BrowserRuntimePackStepExecutionStatus,
    BrowserRuntimePackStepRunOutcome, BrowserRuntimePackStepRunner,
};

pub struct BrowserRuntimePackLocalStepRunner {
    manifest: BrowserRuntimePackManifest,
    paths: BrowserRuntimePackPaths,
    archive_source_path: Option<PathBuf>,
    staging_source_dir: Option<PathBuf>,
    post_install_probe: Box<dyn BrowserRuntimePackPostInstallProbe>,
    downloaded_archive_path: Option<PathBuf>,
}

pub trait BrowserRuntimePackPostInstallProbe: Send + Sync {
    fn probe(
        &self,
        manifest: &BrowserRuntimePackManifest,
        paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BrowserRuntimePackRealPostInstallProbe;

impl BrowserRuntimePackPostInstallProbe for BrowserRuntimePackRealPostInstallProbe {
    fn probe(
        &self,
        manifest: &BrowserRuntimePackManifest,
        paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions {
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: node_version_matches(manifest, paths) && worker_starts(paths),
            real_page_probe_ok: playwright_real_page_probe(paths),
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        }
    }
}

impl BrowserRuntimePackLocalStepRunner {
    pub fn new(manifest: BrowserRuntimePackManifest, paths: BrowserRuntimePackPaths) -> Self {
        Self {
            manifest,
            paths,
            archive_source_path: None,
            staging_source_dir: None,
            post_install_probe: Box::new(BrowserRuntimePackRealPostInstallProbe),
            downloaded_archive_path: None,
        }
    }

    pub fn with_archive_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.archive_source_path = Some(path.into());
        self
    }

    pub fn with_staging_source_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.staging_source_dir = Some(path.into());
        self
    }

    pub fn with_probe_options(mut self, options: BrowserRuntimePackFilesystemProbeOptions) -> Self {
        self.post_install_probe = Box::new(FixedPostInstallProbe { options });
        self
    }

    pub fn with_post_install_probe<P>(mut self, probe: P) -> Self
    where
        P: BrowserRuntimePackPostInstallProbe + 'static,
    {
        self.post_install_probe = Box::new(probe);
        self
    }

    fn run_download_archive(&mut self) -> BrowserRuntimePackStepRunOutcome {
        if self.staging_source_dir.is_some() && self.archive_source_path.is_none() {
            return BrowserRuntimePackStepRunOutcome::completed();
        }
        let Some(source) = self.archive_source_path.as_ref() else {
            return BrowserRuntimePackStepRunOutcome::failed(
                "No app-managed runtime archive source is configured.",
            );
        };
        if !source.is_file() {
            return BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime archive source does not exist: {}",
                source.display()
            ));
        }

        let downloads_dir = self.paths.runtime_root.join("downloads");
        if let Err(error) = fs::create_dir_all(&downloads_dir) {
            return io_failed("create runtime downloads directory", error);
        }
        let file_name = source
            .file_name()
            .map(|name| name.to_owned())
            .unwrap_or_else(|| "runtime-pack.archive".into());
        let destination = downloads_dir.join(file_name);
        if let Err(error) = fs::copy(source, &destination) {
            return io_failed("copy runtime archive into managed storage", error);
        }
        self.downloaded_archive_path = Some(destination);
        BrowserRuntimePackStepRunOutcome::completed()
    }

    fn run_verify_sha256(&self) -> BrowserRuntimePackStepRunOutcome {
        if self.staging_source_dir.is_some() && self.downloaded_archive_path.is_none() {
            return BrowserRuntimePackStepRunOutcome::completed();
        }
        let Some(archive_path) = self.downloaded_archive_path.as_ref() else {
            return BrowserRuntimePackStepRunOutcome::failed(
                "No downloaded runtime archive is available for sha256 verification.",
            );
        };
        let Some(expected) = normalized_sha256(&self.manifest.sha256) else {
            return BrowserRuntimePackStepRunOutcome::failed(
                "Runtime manifest does not contain a trusted sha256 digest.",
            );
        };

        match sha256_file_hex(archive_path) {
            Ok(actual) if actual == expected => BrowserRuntimePackStepRunOutcome::completed(),
            Ok(actual) => BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime archive sha256 mismatch: expected {expected}, got {actual}."
            )),
            Err(error) => io_failed("read runtime archive for sha256 verification", error),
        }
    }

    fn run_unpack_staging(
        &self,
        step: &BrowserRuntimePackPlanStep,
    ) -> BrowserRuntimePackStepRunOutcome {
        let Some(source) = self.staging_source_dir.as_ref() else {
            return BrowserRuntimePackStepRunOutcome::failed(
                "No app-managed staging source directory is configured.",
            );
        };
        if !source.is_dir() {
            return BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime staging source does not exist: {}",
                source.display()
            ));
        }
        let destination = step
            .path
            .clone()
            .unwrap_or_else(|| self.paths.current_pack_dir.with_extension("staging"));
        if destination.exists() {
            if let Err(error) = fs::remove_dir_all(&destination) {
                return io_failed("clear existing runtime staging directory", error);
            }
        }
        match copy_dir_recursive(source, &destination) {
            Ok(()) => BrowserRuntimePackStepRunOutcome::completed(),
            Err(error) => io_failed("copy runtime staging directory", error),
        }
    }

    fn run_install_pack(&self) -> BrowserRuntimePackStepRunOutcome {
        let staging = self.paths.current_pack_dir.with_extension("staging");
        if !staging.is_dir() {
            return BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime staging directory is missing: {}",
                staging.display()
            ));
        }
        if self.paths.current_pack_dir.exists() {
            if let Err(error) = fs::remove_dir_all(&self.paths.current_pack_dir) {
                return io_failed("replace current runtime pack", error);
            }
        }
        if let Err(error) = copy_dir_recursive(&staging, &self.paths.current_pack_dir) {
            return io_failed("install staged runtime pack", error);
        }
        if let Err(error) = fs::remove_dir_all(staging) {
            return io_failed("remove runtime staging directory", error);
        }
        BrowserRuntimePackStepRunOutcome::completed()
    }

    fn run_doctor(&self) -> BrowserRuntimePackStepRunOutcome {
        let probe_options = self.post_install_probe.probe(&self.manifest, &self.paths);
        let filesystem = probe_runtime_pack_filesystem(&self.manifest, &self.paths, probe_options);
        let doctor = diagnose_runtime_pack(&self.manifest, &filesystem.probe);
        if doctor.ready {
            BrowserRuntimePackStepRunOutcome::completed()
        } else {
            BrowserRuntimePackStepRunOutcome::failed(format!(
                "Browser runtime doctor did not report ready: {:?}",
                doctor.issue
            ))
        }
    }

    fn run_promote_pack(&self) -> BrowserRuntimePackStepRunOutcome {
        if self.paths.current_pack_dir.is_dir() {
            BrowserRuntimePackStepRunOutcome::completed()
        } else {
            BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime pack cannot be promoted because current pack is missing: {}",
                self.paths.current_pack_dir.display()
            ))
        }
    }

    fn run_cleanup_old_packs(&self) -> BrowserRuntimePackStepRunOutcome {
        if !self.paths.packs_dir.exists() {
            return BrowserRuntimePackStepRunOutcome::completed();
        }
        let current_name = self
            .paths
            .current_pack_dir
            .file_name()
            .map(|name| name.to_owned());
        let rollback_name = self
            .manifest
            .rollback_pack_version
            .as_ref()
            .map(std::ffi::OsString::from);

        let entries = match fs::read_dir(&self.paths.packs_dir) {
            Ok(entries) => entries,
            Err(error) => return io_failed("read runtime packs directory", error),
        };

        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(error) => return io_failed("read runtime pack entry", error),
            };
            let file_name = entry.file_name();
            if Some(&file_name) == current_name.as_ref()
                || Some(&file_name) == rollback_name.as_ref()
            {
                continue;
            }
            let path = entry.path();
            if path.is_dir() {
                if let Err(error) = fs::remove_dir_all(&path) {
                    return io_failed("remove old runtime pack", error);
                }
            }
        }

        BrowserRuntimePackStepRunOutcome::completed()
    }

    fn run_restore_rollback(&self) -> BrowserRuntimePackStepRunOutcome {
        let Some(rollback_version) = self.manifest.rollback_pack_version.as_ref() else {
            return BrowserRuntimePackStepRunOutcome::failed(
                "Runtime rollback is not configured in the manifest.",
            );
        };
        let rollback_dir = self.paths.packs_dir.join(rollback_version);
        if !rollback_dir.is_dir() {
            return BrowserRuntimePackStepRunOutcome::failed(format!(
                "Runtime rollback pack is missing: {}",
                rollback_dir.display()
            ));
        }
        if self.paths.current_pack_dir.exists() {
            if let Err(error) = fs::remove_dir_all(&self.paths.current_pack_dir) {
                return io_failed("remove current runtime pack before rollback", error);
            }
        }
        match copy_dir_recursive(&rollback_dir, &self.paths.current_pack_dir) {
            Ok(()) => BrowserRuntimePackStepRunOutcome::completed(),
            Err(error) => io_failed("restore runtime rollback pack", error),
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct FixedPostInstallProbe {
    options: BrowserRuntimePackFilesystemProbeOptions,
}

impl BrowserRuntimePackPostInstallProbe for FixedPostInstallProbe {
    fn probe(
        &self,
        _manifest: &BrowserRuntimePackManifest,
        _paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions {
        self.options
    }
}

impl BrowserRuntimePackStepRunner for BrowserRuntimePackLocalStepRunner {
    fn run_step(&mut self, step: &BrowserRuntimePackPlanStep) -> BrowserRuntimePackStepRunOutcome {
        match step.kind {
            BrowserRuntimePackPlanStepKind::CheckManifest
            | BrowserRuntimePackPlanStepKind::CheckNetworkPolicy
            | BrowserRuntimePackPlanStepKind::CleanupPreview
            | BrowserRuntimePackPlanStepKind::KeepCurrent
            | BrowserRuntimePackPlanStepKind::Defer
            | BrowserRuntimePackPlanStepKind::RetainRollback => {
                BrowserRuntimePackStepRunOutcome::completed()
            }
            BrowserRuntimePackPlanStepKind::RequireUserConfirmation => {
                BrowserRuntimePackStepRunOutcome::failed(
                    "User confirmation must be resolved before managed execution.",
                )
            }
            BrowserRuntimePackPlanStepKind::DownloadArchive => self.run_download_archive(),
            BrowserRuntimePackPlanStepKind::VerifySha256 => self.run_verify_sha256(),
            BrowserRuntimePackPlanStepKind::UnpackStaging => self.run_unpack_staging(step),
            BrowserRuntimePackPlanStepKind::InstallPack => self.run_install_pack(),
            BrowserRuntimePackPlanStepKind::RunDoctor => self.run_doctor(),
            BrowserRuntimePackPlanStepKind::PromotePack => self.run_promote_pack(),
            BrowserRuntimePackPlanStepKind::CleanupOldPacks => self.run_cleanup_old_packs(),
            BrowserRuntimePackPlanStepKind::RestoreRollback => self.run_restore_rollback(),
        }
    }
}

fn normalized_sha256(value: &str) -> Option<String> {
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() == 64 && digest.chars().all(|ch| ch.is_ascii_hexdigit()) {
        Some(digest.to_ascii_lowercase())
    } else {
        None
    }
}

fn sha256_file_hex(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 16 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir_all(destination)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(&source_path, &destination_path)?;
        }
    }
    Ok(())
}

fn node_version_matches(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
) -> bool {
    let Ok(output) = Command::new(&paths.node_binary_path)
        .arg("--version")
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.trim() == format!("v{}", manifest.node_version)
}

fn worker_starts(paths: &BrowserRuntimePackPaths) -> bool {
    let Ok(output) = Command::new(&paths.node_binary_path)
        .arg(&paths.worker_script_path)
        .arg("--health-check")
        .current_dir(&paths.current_pack_dir)
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.contains("uclaw.playwright.worker.ready")
}

fn playwright_real_page_probe(paths: &BrowserRuntimePackPaths) -> bool {
    if !paths.chromium_binary_path.is_file() {
        return false;
    }
    let script = r#"
const { chromium } = require('playwright');
(async () => {
  const browser = await chromium.launch({ headless: true });
  const page = await browser.newPage();
  await page.goto('data:text/html,<title>uclaw-runtime-probe</title>');
  const title = await page.title();
  await browser.close();
  if (title !== 'uclaw-runtime-probe') process.exit(2);
})().catch((error) => {
  console.error(error && error.stack ? error.stack : error);
  process.exit(1);
});
"#;
    let Ok(output) = Command::new(&paths.node_binary_path)
        .arg("-e")
        .arg(script)
        .current_dir(&paths.current_pack_dir)
        .env("PLAYWRIGHT_BROWSERS_PATH", &paths.playwright_browsers_path)
        .env("PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD", "1")
        .output()
    else {
        return false;
    };
    output.status.success()
}

fn io_failed(action: &str, error: io::Error) -> BrowserRuntimePackStepRunOutcome {
    BrowserRuntimePackStepRunOutcome {
        status: BrowserRuntimePackStepExecutionStatus::Failed,
        error: Some(format!("{action}: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::super::runtime_pack::{
        diagnose_runtime_pack, execute_runtime_pack_plan_with_runner, plan_runtime_pack_operation,
        BrowserRuntimePackExecutorPolicy, BrowserRuntimePackNetworkState,
        BrowserRuntimePackOperation, BrowserRuntimePackOperationRequest,
        BrowserRuntimePackPlanStatus, BrowserRuntimePackPlanTrigger,
        BrowserRuntimePackStepExecutionStatus,
    };
    use super::*;

    fn ready_probe_options() -> BrowserRuntimePackFilesystemProbeOptions {
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: true,
            real_page_probe_ok: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        }
    }

    fn manifest_with_archive_sha(archive_path: &Path) -> BrowserRuntimePackManifest {
        BrowserRuntimePackManifest {
            sha256: format!(
                "sha256:{}",
                sha256_file_hex(archive_path).expect("archive sha")
            ),
            archive_size_bytes: fs::metadata(archive_path).expect("archive metadata").len(),
            ..BrowserRuntimePackManifest::v1_default()
        }
    }

    fn write_runtime_pack_fixture(
        paths: &BrowserRuntimePackPaths,
        manifest: &BrowserRuntimePackManifest,
    ) {
        fs::create_dir_all(paths.node_binary_path.parent().expect("node parent"))
            .expect("node parent");
        fs::write(&paths.node_binary_path, "node").expect("node binary");
        fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
        fs::create_dir_all(&paths.playwright_mcp_package_dir).expect("playwright mcp package");
        fs::create_dir_all(paths.worker_script_path.parent().expect("worker parent"))
            .expect("worker parent");
        fs::write(&paths.worker_script_path, "worker").expect("worker script");
        fs::create_dir_all(
            paths
                .chromium_binary_path
                .parent()
                .expect("chromium parent"),
        )
        .expect("chromium parent");
        fs::write(&paths.chromium_binary_path, "chromium").expect("chromium binary");
        fs::write(
            &paths.manifest_path,
            serde_json::to_string_pretty(manifest).expect("manifest json"),
        )
        .expect("manifest");
    }

    fn plan(
        manifest: &BrowserRuntimePackManifest,
        paths: &BrowserRuntimePackPaths,
        operation: BrowserRuntimePackOperation,
    ) -> super::super::runtime_pack::BrowserRuntimePackOperationPlan {
        let probe = probe_runtime_pack_filesystem(manifest, paths, ready_probe_options());
        let doctor = diagnose_runtime_pack(manifest, &probe.probe);
        plan_runtime_pack_operation(
            manifest,
            paths,
            &doctor,
            BrowserRuntimePackOperationRequest {
                operation,
                trigger: BrowserRuntimePackPlanTrigger::Settings,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: true,
                active_tasks: 0,
            },
        )
    }

    #[test]
    fn local_runner_fails_closed_without_archive_source() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);
        let probe = probe_runtime_pack_filesystem(&manifest, &paths, ready_probe_options());
        let doctor = diagnose_runtime_pack(&manifest, &probe.probe);
        let plan = plan_runtime_pack_operation(
            &manifest,
            &paths,
            &doctor,
            BrowserRuntimePackOperationRequest {
                operation: BrowserRuntimePackOperation::Prepare,
                trigger: BrowserRuntimePackPlanTrigger::Settings,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: true,
                active_tasks: 0,
            },
        );
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest, paths);

        let report = execute_runtime_pack_plan_with_runner(
            &plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: true,
                allow_destructive: false,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Failed
        );
        assert_eq!(
            report.step_reports.last().map(|step| step.step),
            Some(BrowserRuntimePackPlanStepKind::DownloadArchive)
        );
        assert!(report
            .step_reports
            .last()
            .and_then(|step| step.error.as_ref())
            .expect("download failure")
            .contains("No app-managed runtime archive source"));
    }

    #[test]
    fn local_runner_installs_from_app_managed_fixture_and_requires_real_probes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let archive = temp.path().join("runtime-pack-fixture.tar.zst");
        fs::write(&archive, "signed runtime archive").expect("archive");
        let manifest = manifest_with_archive_sha(&archive);
        let paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let staging_source =
            BrowserRuntimePackPaths::from_root(temp.path().join("staging-source"), &manifest);
        write_runtime_pack_fixture(&staging_source, &manifest);

        let probe = probe_runtime_pack_filesystem(&manifest, &paths, ready_probe_options());
        let doctor = diagnose_runtime_pack(&manifest, &probe.probe);
        let plan = plan_runtime_pack_operation(
            &manifest,
            &paths,
            &doctor,
            BrowserRuntimePackOperationRequest {
                operation: BrowserRuntimePackOperation::Prepare,
                trigger: BrowserRuntimePackPlanTrigger::Settings,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: true,
                active_tasks: 0,
            },
        );
        assert_eq!(plan.status, BrowserRuntimePackPlanStatus::Planned);

        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths.clone())
            .with_archive_source(&archive)
            .with_staging_source_dir(&staging_source.current_pack_dir)
            .with_probe_options(ready_probe_options());

        let report = execute_runtime_pack_plan_with_runner(
            &plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: true,
                allow_destructive: false,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Succeeded
        );
        assert!(paths.current_pack_dir.is_dir());
        assert!(paths.worker_script_path.is_file());
        assert!(report
            .step_reports
            .iter()
            .all(|step| step.status == BrowserRuntimePackStepExecutionStatus::Completed));

        let strict_probe = probe_runtime_pack_filesystem(
            &manifest,
            &paths,
            BrowserRuntimePackFilesystemProbeOptions::default(),
        );
        let strict_doctor = diagnose_runtime_pack(&manifest, &strict_probe.probe);
        assert!(!strict_doctor.ready);

        let verified_probe =
            probe_runtime_pack_filesystem(&manifest, &paths, ready_probe_options());
        let verified_doctor = diagnose_runtime_pack(&manifest, &verified_probe.probe);
        assert!(verified_doctor.ready);
    }

    #[derive(Clone)]
    struct FixturePostInstallProbe {
        options: BrowserRuntimePackFilesystemProbeOptions,
    }

    impl BrowserRuntimePackPostInstallProbe for FixturePostInstallProbe {
        fn probe(
            &self,
            _manifest: &BrowserRuntimePackManifest,
            _paths: &BrowserRuntimePackPaths,
        ) -> BrowserRuntimePackFilesystemProbeOptions {
            self.options
        }
    }

    #[test]
    fn local_runner_uses_post_install_probe_for_doctor() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let source_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("source"), &manifest);
        write_runtime_pack_fixture(&source_paths, &manifest);
        let install_plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Prepare);
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths)
            .with_staging_source_dir(&source_paths.current_pack_dir)
            .with_post_install_probe(FixturePostInstallProbe {
                options: ready_probe_options(),
            });

        let report = execute_runtime_pack_plan_with_runner(
            &install_plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: true,
                allow_destructive: false,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Succeeded
        );
        assert!(report.step_reports.iter().any(|step| {
            step.step == BrowserRuntimePackPlanStepKind::RunDoctor
                && step.status == BrowserRuntimePackStepExecutionStatus::Completed
        }));
    }

    #[test]
    fn local_runner_default_probe_does_not_force_runtime_ready() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
        let source_paths =
            BrowserRuntimePackPaths::from_root(temp.path().join("source"), &manifest);
        write_runtime_pack_fixture(&source_paths, &manifest);
        let install_plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Prepare);
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths)
            .with_staging_source_dir(&source_paths.current_pack_dir);

        let report = execute_runtime_pack_plan_with_runner(
            &install_plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: true,
                allow_destructive: false,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Failed
        );
        assert!(report
            .step_reports
            .last()
            .and_then(|step| step.error.as_ref())
            .expect("doctor failure")
            .contains("Browser runtime doctor did not report ready"));
    }

    #[test]
    fn local_runner_cleans_old_packs_without_removing_current_or_rollback() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);
        fs::create_dir_all(&paths.current_pack_dir).expect("current");
        let rollback_dir = paths.packs_dir.join("browser-runtime-pack-v0");
        fs::create_dir_all(&rollback_dir).expect("rollback");
        let stale_dir = paths.packs_dir.join("browser-runtime-pack-old");
        fs::create_dir_all(&stale_dir).expect("stale");
        let cleanup_plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Cleanup);
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest, paths.clone());

        let report = execute_runtime_pack_plan_with_runner(
            &cleanup_plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: false,
                allow_destructive: true,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Succeeded
        );
        assert!(paths.current_pack_dir.exists());
        assert!(rollback_dir.exists());
        assert!(!stale_dir.exists());
    }

    #[test]
    fn local_runner_restores_configured_rollback_pack_before_doctor_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);
        fs::create_dir_all(&paths.current_pack_dir).expect("current");
        fs::write(paths.current_pack_dir.join("marker.txt"), "current").expect("current marker");
        let rollback_dir = paths.packs_dir.join("browser-runtime-pack-v0");
        fs::create_dir_all(&rollback_dir).expect("rollback");
        fs::write(rollback_dir.join("marker.txt"), "rollback").expect("rollback marker");
        let rollback_plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Rollback);
        let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest, paths.clone())
            .with_probe_options(ready_probe_options());

        let report = execute_runtime_pack_plan_with_runner(
            &rollback_plan,
            BrowserRuntimePackExecutorPolicy {
                allow_network: false,
                allow_destructive: true,
            },
            &mut runner,
        );

        assert_eq!(
            report.status,
            super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Failed
        );
        assert_eq!(
            fs::read_to_string(paths.current_pack_dir.join("marker.txt")).expect("marker"),
            "rollback"
        );
        assert_eq!(
            report.step_reports.last().map(|step| step.step),
            Some(BrowserRuntimePackPlanStepKind::RunDoctor)
        );
    }
}
