# Browser Runtime Real Pack Install PR1 Resolver And Execute IPC Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the Rust source resolver, source validation, post-install smoke-probe boundary, and confirmed `execute_browser_runtime_action` IPC for real `prepare` and `repair` runtime-pack installation.

**Architecture:** Keep runtime-pack source discovery in a focused `runtime_pack_source` module, keep local file installation in `runtime_pack_runner`, and keep Tauri command logic in `runtime_pack_ipc`. The installer resolves a trusted source pack, validates it, plans the operation with `user_confirmed=true`, runs the existing managed runner, and returns the same execution report shape already used by dry-run.

**Tech Stack:** Rust, Tauri command IPC, existing `runtime_pack` planning/execution types, focused Rust unit tests.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/browser/runtime_pack_source.rs` | Resolve and validate env/bundle/dev staging runtime-pack sources. |
| `src-tauri/src/browser/runtime_pack_runner.rs` | Add a post-install smoke probe abstraction and use it for `RunDoctor`. |
| `src-tauri/src/browser/runtime_pack_ipc.rs` | Add confirmed real execution command and source-aware execution helpers. |
| `src-tauri/src/browser/mod.rs` | Export the new source module/types. |
| `src-tauri/src/main.rs` | Register `execute_browser_runtime_action` in Tauri invoke handler. |

## Boundaries

- This PR does not create generator scripts.
- This PR does not change the Settings UI.
- This PR does not execute `reinstall`, `cleanup`, or `rollback` for real.
- This PR uses small fixture runtime packs in tests, never real Node/Chromium artifacts.
- This PR must not use global npm or global Playwright caches inside installer code.

## ADR 18 Answers

1. Intent: let the app install a real browser runtime pack from an app-managed source.
2. Autonomy: local, confirmed file-system installation under uClaw-managed storage.
3. Truth source: Rust source resolver, filesystem validator, and runtime doctor.
4. TaskEvent: existing execution reports carry step events/artifacts.
5. Context: manifest, source resolution, current filesystem probe, user confirmation.
6. Capability: prepares the runtime pack required by Playwright CLI/MCP routing.
7. Hooks: source validation, confirmation gate, post-install smoke probe, focused tests, GitNexus detect.
8. Projection: Control Center can later show execution result and refreshed runtime truth.
9. Harness: Rust unit tests cover resolver, validation, execution gates, and fixture install.
10. Rollback: disable CLI/MCP, keep Local Chromium fallback, or remove generated dev staging.
11. Non-ownership: no remote download, no UI, no generator, no platform expansion.

### Task 1: Add Source Resolver And Validator Types

**Files:**
- Create: `src-tauri/src/browser/runtime_pack_source.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [ ] **Step 1: Write resolver unit tests**

Create `src-tauri/src/browser/runtime_pack_source.rs` with the tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn write_pack_fixture(root: &std::path::Path, manifest: &BrowserRuntimePackManifest) {
        fs::create_dir_all(root.join("node/bin")).expect("node bin");
        fs::write(root.join("node/bin/node"), "node").expect("node");
        fs::create_dir_all(root.join("node_modules/playwright")).expect("playwright");
        fs::create_dir_all(root.join("node_modules/@playwright/mcp")).expect("mcp");
        fs::create_dir_all(root.join("worker")).expect("worker dir");
        fs::write(root.join("worker/uclaw-playwright-worker.mjs"), "console.log('worker')\n")
            .expect("worker");
        fs::create_dir_all(
            root.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS"),
        )
        .expect("chromium dir");
        fs::write(
            root.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS/Chromium"),
            "chromium",
        )
        .expect("chromium");
        fs::write(
            root.join("runtime-pack.manifest.json"),
            serde_json::to_string_pretty(manifest).expect("manifest json"),
        )
        .expect("manifest");
    }

    #[test]
    fn env_override_wins_over_bundle_and_dev_candidates() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let env = temp.path().join("env/browser-runtime-pack-v1");
        let bundle = temp.path().join("bundle/browser-runtime-pack-v1");
        let dev = temp.path().join("dev/browser-runtime-pack-v1");
        write_pack_fixture(&env, &manifest);
        write_pack_fixture(&bundle, &manifest);
        write_pack_fixture(&dev, &manifest);

        let resolver = BrowserRuntimePackSourceResolver::for_test(
            Some(env.clone()),
            Some(bundle),
            Some(dev),
        );
        let resolution = resolver.resolve(&manifest);

        assert_eq!(resolution.status, BrowserRuntimePackSourceResolutionStatus::Found);
        assert_eq!(resolution.source_kind, Some(BrowserRuntimePackSourceKind::EnvOverride));
        assert_eq!(resolution.source_dir.as_deref(), Some(env.as_path()));
        assert!(resolution.validation_errors.is_empty());
    }

    #[test]
    fn dev_staging_is_used_when_env_and_bundle_are_missing() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let dev = temp.path().join("src-tauri/.runtime-pack-staging/browser-runtime-pack-v1");
        write_pack_fixture(&dev, &manifest);

        let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, Some(dev.clone()));
        let resolution = resolver.resolve(&manifest);

        assert_eq!(resolution.status, BrowserRuntimePackSourceResolutionStatus::Found);
        assert_eq!(resolution.source_kind, Some(BrowserRuntimePackSourceKind::DevStaging));
        assert_eq!(resolution.source_dir.as_deref(), Some(dev.as_path()));
    }

    #[test]
    fn invalid_source_reports_missing_required_paths() {
        let temp = tempfile::tempdir().expect("temp");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let source = temp.path().join("broken/browser-runtime-pack-v1");
        fs::create_dir_all(&source).expect("source");
        fs::write(
            source.join("runtime-pack.manifest.json"),
            serde_json::to_string_pretty(&manifest).expect("manifest json"),
        )
        .expect("manifest");

        let resolver = BrowserRuntimePackSourceResolver::for_test(Some(source), None, None);
        let resolution = resolver.resolve(&manifest);

        assert_eq!(resolution.status, BrowserRuntimePackSourceResolutionStatus::Invalid);
        assert!(resolution
            .validation_errors
            .iter()
            .any(|error| error.contains("node/bin/node")));
        assert!(resolution
            .validation_errors
            .iter()
            .any(|error| error.contains("node_modules/playwright")));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_source
```

Expected: FAIL with unresolved module/export errors for `runtime_pack_source`.

- [ ] **Step 3: Implement source resolver**

Add the implementation above the tests in `src-tauri/src/browser/runtime_pack_source.rs`:

```rust
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::runtime_pack::{
    load_runtime_pack_manifest, BrowserRuntimePackManifest,
    BrowserRuntimePackManifestLoadStatus,
};

const ENV_SOURCE_VAR: &str = "UCLAW_BROWSER_RUNTIME_PACK_SOURCE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackSourceResolutionStatus {
    Found,
    Missing,
    Invalid,
    UnsupportedPlatform,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimePackSourceKind {
    EnvOverride,
    BundleResource,
    DevStaging,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimePackSourceResolution {
    pub status: BrowserRuntimePackSourceResolutionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_kind: Option<BrowserRuntimePackSourceKind>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_dir: Option<PathBuf>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest: Option<BrowserRuntimePackManifest>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_errors: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BrowserRuntimePackSourceResolver {
    env_override_dir: Option<PathBuf>,
    bundle_resource_dir: Option<PathBuf>,
    dev_staging_dir: Option<PathBuf>,
}

impl BrowserRuntimePackSourceResolver {
    pub fn new(bundle_resource_dir: Option<PathBuf>, dev_staging_dir: Option<PathBuf>) -> Self {
        Self {
            env_override_dir: std::env::var_os(ENV_SOURCE_VAR).map(PathBuf::from),
            bundle_resource_dir,
            dev_staging_dir,
        }
    }

    pub fn from_current_process() -> Self {
        let dev_staging_dir = std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join("src-tauri/.runtime-pack-staging/browser-runtime-pack-v1"));
        Self::new(None, dev_staging_dir)
    }

    pub fn for_test(
        env_override_dir: Option<PathBuf>,
        bundle_resource_dir: Option<PathBuf>,
        dev_staging_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            env_override_dir,
            bundle_resource_dir,
            dev_staging_dir,
        }
    }

    pub fn resolve(
        &self,
        expected: &BrowserRuntimePackManifest,
    ) -> BrowserRuntimePackSourceResolution {
        if !is_supported_platform() {
            return BrowserRuntimePackSourceResolution {
                status: BrowserRuntimePackSourceResolutionStatus::UnsupportedPlatform,
                source_kind: None,
                source_dir: None,
                manifest: None,
                validation_errors: vec!["Browser runtime pack v1 supports macOS arm64 only.".to_string()],
            };
        }

        for (kind, candidate) in self.candidates() {
            let Some(source_dir) = candidate else {
                continue;
            };
            if !source_dir.exists() {
                continue;
            }
            let validation = validate_source(expected, &source_dir);
            return BrowserRuntimePackSourceResolution {
                status: if validation.errors.is_empty() {
                    BrowserRuntimePackSourceResolutionStatus::Found
                } else {
                    BrowserRuntimePackSourceResolutionStatus::Invalid
                },
                source_kind: Some(kind),
                source_dir: Some(source_dir),
                manifest: validation.manifest,
                validation_errors: validation.errors,
            };
        }

        BrowserRuntimePackSourceResolution {
            status: BrowserRuntimePackSourceResolutionStatus::Missing,
            source_kind: None,
            source_dir: None,
            manifest: None,
            validation_errors: vec![
                "Runtime pack source not found. Generate the dev pack or install an app bundle that includes it.".to_string(),
            ],
        }
    }

    fn candidates(&self) -> [(BrowserRuntimePackSourceKind, Option<PathBuf>); 3] {
        [
            (BrowserRuntimePackSourceKind::EnvOverride, self.env_override_dir.clone()),
            (BrowserRuntimePackSourceKind::BundleResource, self.bundle_resource_dir.clone()),
            (BrowserRuntimePackSourceKind::DevStaging, self.dev_staging_dir.clone()),
        ]
    }
}

struct SourceValidation {
    manifest: Option<BrowserRuntimePackManifest>,
    errors: Vec<String>,
}

pub fn validate_runtime_pack_source(
    expected: &BrowserRuntimePackManifest,
    source_dir: &Path,
) -> BrowserRuntimePackSourceResolution {
    let validation = validate_source(expected, source_dir);
    BrowserRuntimePackSourceResolution {
        status: if validation.errors.is_empty() {
            BrowserRuntimePackSourceResolutionStatus::Found
        } else {
            BrowserRuntimePackSourceResolutionStatus::Invalid
        },
        source_kind: None,
        source_dir: Some(source_dir.to_path_buf()),
        manifest: validation.manifest,
        validation_errors: validation.errors,
    }
}

fn validate_source(expected: &BrowserRuntimePackManifest, source_dir: &Path) -> SourceValidation {
    let mut errors = Vec::new();
    let manifest_path = source_dir.join("runtime-pack.manifest.json");
    let manifest_load = load_runtime_pack_manifest(&manifest_path);
    let manifest = manifest_load.manifest;
    match manifest_load.status {
        BrowserRuntimePackManifestLoadStatus::Loaded => {}
        BrowserRuntimePackManifestLoadStatus::Missing => {
            errors.push("missing runtime-pack.manifest.json".to_string());
        }
        BrowserRuntimePackManifestLoadStatus::InvalidJson => {
            errors.push("invalid runtime-pack.manifest.json".to_string());
        }
        BrowserRuntimePackManifestLoadStatus::IoError => {
            errors.push("could not read runtime-pack.manifest.json".to_string());
        }
    }
    if let Some(installed) = manifest.as_ref() {
        if installed.pack_version != expected.pack_version {
            errors.push(format!(
                "pack_version mismatch: expected {}, got {}",
                expected.pack_version, installed.pack_version
            ));
        }
        if installed.node_version != expected.node_version {
            errors.push(format!(
                "node_version mismatch: expected {}, got {}",
                expected.node_version, installed.node_version
            ));
        }
        if installed.playwright_version != expected.playwright_version {
            errors.push(format!(
                "playwright_version mismatch: expected {}, got {}",
                expected.playwright_version, installed.playwright_version
            ));
        }
        if installed.playwright_mcp_version != expected.playwright_mcp_version {
            errors.push(format!(
                "playwright_mcp_version mismatch: expected {}, got {}",
                expected.playwright_mcp_version, installed.playwright_mcp_version
            ));
        }
        if installed.worker_version != expected.worker_version {
            errors.push(format!(
                "worker_version mismatch: expected {}, got {}",
                expected.worker_version, installed.worker_version
            ));
        }
        if installed.chromium_revision != expected.chromium_revision {
            errors.push(format!(
                "chromium_revision mismatch: expected {}, got {}",
                expected.chromium_revision, installed.chromium_revision
            ));
        }
    }

    for required in required_source_paths(expected) {
        if !source_dir.join(required).exists() {
            errors.push(format!("missing {}", required.display()));
        }
    }

    SourceValidation { manifest, errors }
}

fn required_source_paths(expected: &BrowserRuntimePackManifest) -> Vec<PathBuf> {
    vec![
        PathBuf::from("runtime-pack.manifest.json"),
        PathBuf::from("node/bin/node"),
        PathBuf::from("node_modules/playwright"),
        PathBuf::from("node_modules/@playwright/mcp"),
        PathBuf::from("worker/uclaw-playwright-worker.mjs"),
        PathBuf::from(format!(
            "ms-playwright/chromium-{}/chrome-mac/Chromium.app/Contents/MacOS/Chromium",
            expected.chromium_revision
        )),
    ]
}

fn is_supported_platform() -> bool {
    cfg!(target_os = "macos") && cfg!(target_arch = "aarch64")
}
```

- [ ] **Step 4: Export module**

Modify `src-tauri/src/browser/mod.rs`:

```rust
pub mod runtime_pack_source;
```

Add exports near existing runtime-pack exports:

```rust
pub use runtime_pack_source::{
    validate_runtime_pack_source, BrowserRuntimePackSourceKind,
    BrowserRuntimePackSourceResolution, BrowserRuntimePackSourceResolutionStatus,
    BrowserRuntimePackSourceResolver,
};
```

- [ ] **Step 5: Run resolver tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_source
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_pack_source.rs src-tauri/src/browser/mod.rs
git commit -m "feat(browser-runtime): resolve runtime pack sources" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_source (expected PASS)"
```

### Task 2: Add Post-Install Smoke Probe Boundary

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_runner.rs`

- [ ] **Step 1: Write tests for fixture and production probe behavior**

Add tests in `runtime_pack_runner.rs` test module:

```rust
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
    let source = temp.path().join("source");
    let paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &BrowserRuntimePackManifest::v1_default());
    let manifest = BrowserRuntimePackManifest::v1_default();
    let source_paths = BrowserRuntimePackPaths::from_root(&source, &manifest);
    write_runtime_pack_fixture(&source_paths, &manifest);
    let plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Prepare);
    let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths)
        .with_staging_source_dir(&source_paths.current_pack_dir)
        .with_post_install_probe(FixturePostInstallProbe {
            options: ready_probe_options(),
        });

    let report = execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );

    assert_eq!(report.status, super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Succeeded);
    assert!(report.step_reports.iter().any(|step| {
        step.step == BrowserRuntimePackPlanStepKind::RunDoctor
            && step.status == BrowserRuntimePackStepExecutionStatus::Completed
    }));
}

#[test]
fn local_runner_default_probe_does_not_force_runtime_ready() {
    let temp = tempfile::tempdir().expect("tempdir");
    let source = temp.path().join("source");
    let paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &BrowserRuntimePackManifest::v1_default());
    let manifest = BrowserRuntimePackManifest::v1_default();
    let source_paths = BrowserRuntimePackPaths::from_root(&source, &manifest);
    write_runtime_pack_fixture(&source_paths, &manifest);
    let plan = plan(&manifest, &paths, BrowserRuntimePackOperation::Prepare);
    let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths)
        .with_staging_source_dir(&source_paths.current_pack_dir);

    let report = execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );

    assert_eq!(report.status, super::super::runtime_pack::BrowserRuntimePackExecutionStatus::Failed);
    assert!(report
        .step_reports
        .last()
        .and_then(|step| step.error.as_ref())
        .expect("doctor error")
        .contains("WorkerStartupFailure"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_runner
```

Expected: FAIL because `BrowserRuntimePackPostInstallProbe` and `with_post_install_probe` do not exist.

- [ ] **Step 3: Implement post-install probe trait**

In `runtime_pack_runner.rs`, add near the runner struct:

```rust
pub trait BrowserRuntimePackPostInstallProbe: Clone {
    fn probe(
        &self,
        manifest: &BrowserRuntimePackManifest,
        paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BrowserRuntimePackDefaultPostInstallProbe;

impl BrowserRuntimePackPostInstallProbe for BrowserRuntimePackDefaultPostInstallProbe {
    fn probe(
        &self,
        _manifest: &BrowserRuntimePackManifest,
        _paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions {
        BrowserRuntimePackFilesystemProbeOptions::default()
    }
}
```

Change the runner type:

```rust
#[derive(Debug, Clone)]
pub struct BrowserRuntimePackLocalStepRunner<P = BrowserRuntimePackDefaultPostInstallProbe>
where
    P: BrowserRuntimePackPostInstallProbe,
{
    manifest: BrowserRuntimePackManifest,
    paths: BrowserRuntimePackPaths,
    archive_source_path: Option<PathBuf>,
    staging_source_dir: Option<PathBuf>,
    post_install_probe: P,
    downloaded_archive_path: Option<PathBuf>,
}
```

Update `new` and existing builder impl:

```rust
impl BrowserRuntimePackLocalStepRunner<BrowserRuntimePackDefaultPostInstallProbe> {
    pub fn new(manifest: BrowserRuntimePackManifest, paths: BrowserRuntimePackPaths) -> Self {
        Self {
            manifest,
            paths,
            archive_source_path: None,
            staging_source_dir: None,
            post_install_probe: BrowserRuntimePackDefaultPostInstallProbe,
            downloaded_archive_path: None,
        }
    }
}

impl<P> BrowserRuntimePackLocalStepRunner<P>
where
    P: BrowserRuntimePackPostInstallProbe,
{
    pub fn with_archive_source(mut self, path: impl Into<PathBuf>) -> Self {
        self.archive_source_path = Some(path.into());
        self
    }

    pub fn with_staging_source_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.staging_source_dir = Some(path.into());
        self
    }

    pub fn with_post_install_probe<Next>(
        self,
        post_install_probe: Next,
    ) -> BrowserRuntimePackLocalStepRunner<Next>
    where
        Next: BrowserRuntimePackPostInstallProbe,
    {
        BrowserRuntimePackLocalStepRunner {
            manifest: self.manifest,
            paths: self.paths,
            archive_source_path: self.archive_source_path,
            staging_source_dir: self.staging_source_dir,
            post_install_probe,
            downloaded_archive_path: self.downloaded_archive_path,
        }
    }
```

Remove `with_probe_options`; replace any test use with `with_post_install_probe(FixturePostInstallProbe { options: ready_probe_options() })`.

Change `run_doctor`:

```rust
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
```

Update the `impl BrowserRuntimePackStepRunner` header:

```rust
impl<P> BrowserRuntimePackStepRunner for BrowserRuntimePackLocalStepRunner<P>
where
    P: BrowserRuntimePackPostInstallProbe,
{
    fn run_step(&mut self, step: &BrowserRuntimePackPlanStep) -> BrowserRuntimePackStepRunOutcome {
        // existing match body
    }
}
```

- [ ] **Step 4: Run runner tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_runner
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/runtime_pack_runner.rs
git commit -m "feat(browser-runtime): probe installed runtime pack before promotion" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_runner (expected PASS)"
```

### Task 3: Add Confirmed Execute IPC

**Files:**
- Modify: `src-tauri/src/browser/runtime_pack_ipc.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Write execution command tests**

Add tests in `runtime_pack_ipc.rs` test module:

```rust
fn write_source_fixture(source_dir: &std::path::Path, manifest: &BrowserRuntimePackManifest) {
    std::fs::create_dir_all(source_dir.join("node/bin")).expect("node parent");
    std::fs::write(source_dir.join("node/bin/node"), "node").expect("node");
    std::fs::create_dir_all(source_dir.join("node_modules/playwright")).expect("playwright");
    std::fs::create_dir_all(source_dir.join("node_modules/@playwright/mcp")).expect("mcp");
    std::fs::create_dir_all(source_dir.join("worker")).expect("worker parent");
    std::fs::write(source_dir.join("worker/uclaw-playwright-worker.mjs"), "worker").expect("worker");
    std::fs::create_dir_all(
        source_dir.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS"),
    )
    .expect("chromium parent");
    std::fs::write(
        source_dir.join("ms-playwright/chromium-1178/chrome-mac/Chromium.app/Contents/MacOS/Chromium"),
        "chromium",
    )
    .expect("chromium");
    std::fs::write(
        source_dir.join("runtime-pack.manifest.json"),
        serde_json::to_string_pretty(manifest).expect("manifest json"),
    )
    .expect("manifest");
}

#[derive(Clone)]
struct ReadyProbe;

impl super::super::runtime_pack_runner::BrowserRuntimePackPostInstallProbe for ReadyProbe {
    fn probe(
        &self,
        _manifest: &BrowserRuntimePackManifest,
        _paths: &BrowserRuntimePackPaths,
    ) -> BrowserRuntimePackFilesystemProbeOptions {
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: true,
            real_page_probe_ok: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        }
    }
}

#[test]
fn execute_prepare_requires_confirmation_before_writing_files() {
    let temp = tempfile::tempdir().expect("temp");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let runtime_paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
    let source_dir = temp.path().join("source");
    write_source_fixture(&source_dir, &manifest);
    let resolver = BrowserRuntimePackSourceResolver::for_test(Some(source_dir), None, None);

    let report = execute_browser_runtime_action_for_paths(
        &manifest,
        &runtime_paths,
        BrowserRuntimePackAction::Prepare,
        false,
        resolver,
        ReadyProbe,
    )
    .expect("confirmation report");

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::RequiresConfirmation);
    assert!(!runtime_paths.current_pack_dir.exists());
}

#[test]
fn execute_prepare_installs_from_resolved_source_after_confirmation() {
    let temp = tempfile::tempdir().expect("temp");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let runtime_paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
    let source_dir = temp.path().join("source");
    write_source_fixture(&source_dir, &manifest);
    let resolver = BrowserRuntimePackSourceResolver::for_test(Some(source_dir), None, None);

    let report = execute_browser_runtime_action_for_paths(
        &manifest,
        &runtime_paths,
        BrowserRuntimePackAction::Prepare,
        true,
        resolver,
        ReadyProbe,
    )
    .expect("execute report");

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
    assert!(runtime_paths.manifest_path.exists());
    assert!(runtime_paths.node_binary_path.exists());
}

#[test]
fn execute_cleanup_is_blocked_in_first_real_installer() {
    let temp = tempfile::tempdir().expect("temp");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let runtime_paths = BrowserRuntimePackPaths::from_root(temp.path().join("runtime"), &manifest);
    let resolver = BrowserRuntimePackSourceResolver::for_test(None, None, None);

    let report = execute_browser_runtime_action_for_paths(
        &manifest,
        &runtime_paths,
        BrowserRuntimePackAction::Cleanup,
        true,
        resolver,
        ReadyProbe,
    )
    .expect("blocked report");

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Blocked);
    assert!(report.summary.contains("not enabled for managed execution"));
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc
```

Expected: FAIL because `execute_browser_runtime_action_for_paths` does not exist.

- [ ] **Step 3: Implement real execute helpers**

In `runtime_pack_ipc.rs`, extend imports:

```rust
use super::runtime_pack::{
    BrowserRuntimePackExecutionMode, BrowserRuntimePackExecutionStatus,
    BrowserRuntimePackExecutorPolicy,
};
use super::runtime_pack_runner::{
    BrowserRuntimePackDefaultPostInstallProbe, BrowserRuntimePackLocalStepRunner,
    BrowserRuntimePackPostInstallProbe,
};
use super::runtime_pack_source::{
    BrowserRuntimePackSourceResolutionStatus, BrowserRuntimePackSourceResolver,
};
```

Add command and helper functions:

```rust
#[tauri::command]
pub async fn execute_browser_runtime_action(
    action: BrowserRuntimePackAction,
    confirmed: bool,
) -> Result<BrowserRuntimePackExecutionReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    execute_browser_runtime_action_for_paths(
        &manifest,
        &paths,
        action,
        confirmed,
        BrowserRuntimePackSourceResolver::from_current_process(),
        BrowserRuntimePackDefaultPostInstallProbe,
    )
}

pub fn execute_browser_runtime_action_for_paths<P>(
    manifest: &BrowserRuntimePackManifest,
    paths: &BrowserRuntimePackPaths,
    action: BrowserRuntimePackAction,
    confirmed: bool,
    resolver: BrowserRuntimePackSourceResolver,
    post_install_probe: P,
) -> Result<BrowserRuntimePackExecutionReport, Error>
where
    P: BrowserRuntimePackPostInstallProbe,
{
    if !matches!(action, BrowserRuntimePackAction::Prepare | BrowserRuntimePackAction::Repair) {
        let mut report = dry_run_browser_runtime_action_for_paths(manifest, paths, action);
        report.mode = BrowserRuntimePackExecutionMode::Managed;
        report.status = BrowserRuntimePackExecutionStatus::Blocked;
        report.summary = format!(
            "{:?} is not enabled for managed execution in this Browser Runtime release.",
            action
        );
        return Ok(report);
    }

    if !confirmed {
        let mut report = dry_run_browser_runtime_action_for_paths(manifest, paths, action);
        report.mode = BrowserRuntimePackExecutionMode::Managed;
        report.status = BrowserRuntimePackExecutionStatus::RequiresConfirmation;
        report.summary = "Confirm Browser runtime pack installation before writing files.".to_string();
        report.requires_confirmation = true;
        return Ok(report);
    }

    let resolution = resolver.resolve(manifest);
    if resolution.status != BrowserRuntimePackSourceResolutionStatus::Found {
        let mut report = dry_run_browser_runtime_action_for_paths(manifest, paths, action);
        report.mode = BrowserRuntimePackExecutionMode::Managed;
        report.status = BrowserRuntimePackExecutionStatus::Failed;
        report.summary = if resolution.validation_errors.is_empty() {
            "Runtime pack source not found.".to_string()
        } else {
            format!("Runtime pack source is invalid: {}", resolution.validation_errors.join("; "))
        };
        return Ok(report);
    }
    let source_dir = resolution
        .source_dir
        .ok_or_else(|| Error::Internal("runtime pack source resolution omitted source_dir".to_string()))?;

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
            user_confirmed: true,
            active_tasks: doctor.active_tasks,
        },
    );
    let mut runner = BrowserRuntimePackLocalStepRunner::new(manifest.clone(), paths.clone())
        .with_staging_source_dir(source_dir)
        .with_post_install_probe(post_install_probe);

    Ok(super::runtime_pack::execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    ))
}
```

- [ ] **Step 4: Register Tauri command**

Modify `src-tauri/src/main.rs` near `dry_run_browser_runtime_action`:

```rust
uclaw_core::browser::runtime_pack_ipc::execute_browser_runtime_action,
```

- [ ] **Step 5: Run IPC tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_pack_ipc.rs src-tauri/src/main.rs
git commit -m "feat(browser-runtime): execute confirmed runtime pack prepare" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc (expected PASS)"
```

### Task 4: Final PR1 Verification

**Files:**
- Verify only.

- [ ] **Step 1: Run focused runtime-pack tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_source
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_runner
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc
```

Expected: PASS.

- [ ] **Step 2: Run formatting/check**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml --check
git diff --check
```

Expected: PASS / exit 0.

- [ ] **Step 3: Run GitNexus staged detect before final PR commit**

Run:

```bash
npx gitnexus detect-changes --scope staged --repo uclaw-new
```

Expected: exit 0. If GitNexus warns the sibling worktree index is stale, include that warning in the PR body.

- [ ] **Step 4: Update tracker if this PR is part of the active Browser Runtime ledger**

If `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` has an active row for real pack install, update only that row. If there is no active row, do not invent one in this PR.

- [ ] **Step 5: Final status**

Run:

```bash
git status --short --branch
```

Expected: clean working tree, branch ahead of `origin/main`.
