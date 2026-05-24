//! Supervised Playwright MCP sidecar process boundary.
//!
//! This module only starts and supervises the app-managed MCP server process.
//! It does not speak raw MCP tools, promote providers, or route Browser tasks.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncReadExt;
use tokio::process::{Child, Command};
use tokio::task::JoinHandle;
use tokio::time::timeout;

use super::playwright_mcp::{
    PlaywrightMcpRequestEnvelope, PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
    PLAYWRIGHT_MCP_PROVIDER_ID,
};
use super::runtime_pack::{BrowserRuntimePackEnvVar, BrowserRuntimePackStatusReport};

pub const DEFAULT_PLAYWRIGHT_MCP_STARTUP_TIMEOUT_MS: u64 = 250;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightMcpSidecarRunnerConfig {
    pub node_binary_path: PathBuf,
    pub mcp_cli_path: PathBuf,
    pub current_pack_dir: PathBuf,
    pub env: Vec<BrowserRuntimePackEnvVar>,
    pub startup_timeout_ms: u64,
}

impl PlaywrightMcpSidecarRunnerConfig {
    pub fn from_runtime_report(runtime_report: &BrowserRuntimePackStatusReport) -> Self {
        Self {
            node_binary_path: runtime_report
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            mcp_cli_path: runtime_report
                .current_pack_dir
                .join("node_modules")
                .join("@playwright")
                .join("mcp")
                .join("cli.js"),
            current_pack_dir: runtime_report.current_pack_dir.clone(),
            env: runtime_report.operation_plan.env.clone(),
            startup_timeout_ms: DEFAULT_PLAYWRIGHT_MCP_STARTUP_TIMEOUT_MS,
        }
    }

    pub fn with_startup_timeout_ms(mut self, startup_timeout_ms: u64) -> Self {
        self.startup_timeout_ms = startup_timeout_ms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpSidecarLaunchSummary {
    pub schema_version: u16,
    pub provider_id: String,
    pub request_id: String,
    pub package_spec: String,
    pub node_binary_path: PathBuf,
    pub mcp_cli_path: PathBuf,
    pub current_pack_dir: PathBuf,
    pub args: Vec<String>,
    pub raw_tools_exposed: bool,
}

#[derive(Debug)]
pub struct PlaywrightMcpSidecarHandle {
    pub summary: PlaywrightMcpSidecarLaunchSummary,
    child: Child,
    stderr_task: JoinHandle<std::io::Result<String>>,
}

impl PlaywrightMcpSidecarHandle {
    pub async fn terminate(mut self) -> Result<(), PlaywrightMcpSidecarRunnerError> {
        let _ = self.child.kill().await;
        let _ = self.child.wait().await;
        self.stderr_task.abort();
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum PlaywrightMcpSidecarRunnerError {
    RuntimePathEscapesPack { path: PathBuf, pack_dir: PathBuf },
    SpawnFailed(String),
    StderrUnavailable,
    RawToolExposureBlocked,
    ExitedDuringStartup { code: Option<i32>, stderr: String },
}

pub async fn start_playwright_mcp_sidecar(
    envelope: &PlaywrightMcpRequestEnvelope,
    config: PlaywrightMcpSidecarRunnerConfig,
) -> Result<PlaywrightMcpSidecarHandle, PlaywrightMcpSidecarRunnerError> {
    if envelope.sidecar.expose_raw_tools {
        return Err(PlaywrightMcpSidecarRunnerError::RawToolExposureBlocked);
    }
    validate_sidecar_path(&config.node_binary_path, &config.current_pack_dir)?;
    validate_sidecar_path(&config.mcp_cli_path, &config.current_pack_dir)?;

    let args = envelope.sidecar.args();
    let summary = PlaywrightMcpSidecarLaunchSummary {
        schema_version: PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id: envelope.request_id.clone(),
        package_spec: envelope.sidecar.package_spec(),
        node_binary_path: config.node_binary_path.clone(),
        mcp_cli_path: config.mcp_cli_path.clone(),
        current_pack_dir: config.current_pack_dir.clone(),
        args: args.clone(),
        raw_tools_exposed: false,
    };

    let mut command = Command::new(&config.node_binary_path);
    command
        .arg(&config.mcp_cli_path)
        .args(&args)
        .current_dir(&config.current_pack_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for env_var in &config.env {
        command.env(&env_var.name, &env_var.value);
    }

    let mut child = command
        .spawn()
        .map_err(|error| PlaywrightMcpSidecarRunnerError::SpawnFailed(error.to_string()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or(PlaywrightMcpSidecarRunnerError::StderrUnavailable)?;
    let stderr_task = tokio::spawn(read_pipe_to_string(stderr));

    match timeout(
        Duration::from_millis(config.startup_timeout_ms),
        child.wait(),
    )
    .await
    {
        Ok(Ok(status)) => {
            let stderr = stderr_task
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            Err(PlaywrightMcpSidecarRunnerError::ExitedDuringStartup {
                code: status.code(),
                stderr,
            })
        }
        Ok(Err(error)) => Err(PlaywrightMcpSidecarRunnerError::SpawnFailed(
            error.to_string(),
        )),
        Err(_) => Ok(PlaywrightMcpSidecarHandle {
            summary,
            child,
            stderr_task,
        }),
    }
}

async fn read_pipe_to_string<R>(mut reader: R) -> std::io::Result<String>
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut output = String::new();
    reader.read_to_string(&mut output).await?;
    Ok(output)
}

fn validate_sidecar_path(
    path: &PathBuf,
    current_pack_dir: &PathBuf,
) -> Result<(), PlaywrightMcpSidecarRunnerError> {
    let canonical_pack_dir = current_pack_dir.canonicalize().map_err(|_| {
        PlaywrightMcpSidecarRunnerError::RuntimePathEscapesPack {
            path: path.clone(),
            pack_dir: current_pack_dir.clone(),
        }
    })?;
    let canonical_path = path.canonicalize().map_err(|_| {
        PlaywrightMcpSidecarRunnerError::RuntimePathEscapesPack {
            path: path.clone(),
            pack_dir: current_pack_dir.clone(),
        }
    })?;
    if canonical_path.starts_with(&canonical_pack_dir) {
        Ok(())
    } else {
        Err(PlaywrightMcpSidecarRunnerError::RuntimePathEscapesPack {
            path: path.clone(),
            pack_dir: current_pack_dir.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    use super::super::playwright_mcp::{
        build_playwright_mcp_request_envelope, build_playwright_mcp_sidecar_spec,
        PlaywrightMcpAction, PlaywrightMcpBrowserName, PlaywrightMcpProfileMode,
        PlaywrightMcpSidecarSpecRequest,
    };
    use super::super::runtime_contracts::BrowserRuntimeFeatureFlags;
    use super::super::runtime_pack::{
        diagnose_runtime_pack, plan_runtime_pack_operation, BrowserRuntimePackAction,
        BrowserRuntimePackEnvVar, BrowserRuntimePackFilesystemProbeReport,
        BrowserRuntimePackFilesystemSnapshot, BrowserRuntimePackManifest,
        BrowserRuntimePackManifestLoadOutcome, BrowserRuntimePackManifestLoadStatus,
        BrowserRuntimePackNetworkState, BrowserRuntimePackOperation,
        BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths, BrowserRuntimePackPlanTrigger,
        BrowserRuntimePackProbe, BrowserRuntimePackStatusReport,
    };
    use super::*;

    #[test]
    fn runner_config_uses_app_managed_node_and_mcp_cli() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report = fixture_runtime_report(temp.path());
        let config = PlaywrightMcpSidecarRunnerConfig::from_runtime_report(&report);

        assert_eq!(
            config.node_binary_path,
            report
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node")
        );
        assert_eq!(
            config.mcp_cli_path,
            report
                .current_pack_dir
                .join("node_modules")
                .join("@playwright")
                .join("mcp")
                .join("cli.js")
        );
        assert_eq!(config.current_pack_dir, report.current_pack_dir);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_runner_starts_app_managed_mcp_process_without_npx() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        let config = prepare_sidecar_fixture(&envelope, 500).expect("sidecar fixture");

        let handle = start_playwright_mcp_sidecar(&envelope, config)
            .await
            .expect("sidecar starts");

        assert_eq!(handle.summary.package_spec, "@playwright/mcp@0.0.75");
        assert!(!handle
            .summary
            .args
            .contains(&"@playwright/mcp@0.0.75".to_string()));
        assert!(handle
            .summary
            .args
            .contains(&"--browser=chrome".to_string()));
        assert!(!handle.summary.raw_tools_exposed);

        handle.terminate().await.expect("terminate sidecar");
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_runner_blocks_raw_tool_exposure_even_for_manual_envelopes() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut envelope = fixture_envelope(temp.path());
        envelope.sidecar.expose_raw_tools = true;
        let config = prepare_sidecar_fixture(&envelope, 50).expect("sidecar fixture");

        let error = start_playwright_mcp_sidecar(&envelope, config)
            .await
            .expect_err("raw tools should stay blocked");

        assert_eq!(
            error,
            PlaywrightMcpSidecarRunnerError::RawToolExposureBlocked
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_runner_rejects_global_node_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        let mut config = prepare_sidecar_fixture(&envelope, 50).expect("sidecar fixture");
        config.node_binary_path = PathBuf::from("/usr/bin/node");

        let error = start_playwright_mcp_sidecar(&envelope, config)
            .await
            .expect_err("global node should be rejected");

        assert!(matches!(
            error,
            PlaywrightMcpSidecarRunnerError::RuntimePathEscapesPack { .. }
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn sidecar_runner_reports_startup_exit() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        let config = prepare_exiting_sidecar_fixture(&envelope).expect("sidecar fixture");

        let error = start_playwright_mcp_sidecar(&envelope, config)
            .await
            .expect_err("startup exit should fail");

        assert!(matches!(
            error,
            PlaywrightMcpSidecarRunnerError::ExitedDuringStartup {
                code: Some(17),
                stderr
            } if stderr.contains("mcp boot failed")
                && stderr.contains("--browser=chrome")
                && !stderr.contains("npx")
                && !stderr.contains("@playwright/mcp@0.0.75")
        ));
    }

    fn fixture_envelope(root: &Path) -> PlaywrightMcpRequestEnvelope {
        let report = fixture_runtime_report(root);
        let sidecar = build_playwright_mcp_sidecar_spec(PlaywrightMcpSidecarSpecRequest {
            package_version: report
                .filesystem
                .manifest_load
                .manifest
                .as_ref()
                .expect("manifest")
                .playwright_mcp_version
                .clone(),
            browser: PlaywrightMcpBrowserName::Chrome,
            profile_mode: PlaywrightMcpProfileMode::Isolated,
            output_dir: report.current_pack_dir.join("mcp-output"),
            user_data_dir: report.current_pack_dir.join("mcp-profile"),
            storage_state_path: None,
            capabilities: Vec::new(),
            action_timeout_ms: Some(250),
            navigation_timeout_ms: Some(500),
            expose_raw_tools: false,
        })
        .expect("sidecar spec");
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_mcp = true;
        build_playwright_mcp_request_envelope(
            "req-mcp-sidecar",
            flags,
            true,
            PlaywrightMcpAction::AccessibilitySnapshot { url: None },
            sidecar,
        )
        .expect("request envelope")
    }

    #[cfg(unix)]
    fn prepare_sidecar_fixture(
        envelope: &PlaywrightMcpRequestEnvelope,
        startup_timeout_ms: u64,
    ) -> std::io::Result<PlaywrightMcpSidecarRunnerConfig> {
        let current_pack_dir = fixture_pack_dir(envelope);
        let node_path = current_pack_dir.join("node").join("bin").join("node");
        let cli_path = current_pack_dir
            .join("node_modules")
            .join("@playwright")
            .join("mcp")
            .join("cli.js");
        write_executable(&node_path, "#!/bin/sh\nsleep 2\n")?;
        write_executable(&cli_path, "#!/bin/sh\n# fake mcp cli\n")?;
        Ok(PlaywrightMcpSidecarRunnerConfig {
            node_binary_path: node_path,
            mcp_cli_path: cli_path,
            current_pack_dir,
            env: vec![BrowserRuntimePackEnvVar {
                name: "PLAYWRIGHT_BROWSERS_PATH".to_string(),
                value: fixture_pack_dir(envelope).join("browsers"),
            }],
            startup_timeout_ms,
        })
    }

    #[cfg(unix)]
    fn prepare_exiting_sidecar_fixture(
        envelope: &PlaywrightMcpRequestEnvelope,
    ) -> std::io::Result<PlaywrightMcpSidecarRunnerConfig> {
        let mut config = prepare_sidecar_fixture(envelope, 500)?;
        write_executable(
            &config.node_binary_path,
            "#!/bin/sh\nshift\necho mcp boot failed >&2\nprintf '%s\\n' \"$@\" >&2\nexit 17\n",
        )?;
        config.startup_timeout_ms = 3_000;
        Ok(config)
    }

    fn fixture_pack_dir(envelope: &PlaywrightMcpRequestEnvelope) -> PathBuf {
        envelope
            .sidecar
            .output_dir
            .parent()
            .expect("output parent")
            .to_path_buf()
    }

    fn fixture_runtime_report(root: &Path) -> BrowserRuntimePackStatusReport {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(root.join("runtime"), &manifest);
        let probe = BrowserRuntimePackProbe::ready();
        let doctor = diagnose_runtime_pack(&manifest, &probe);
        let primary_action = doctor
            .actions
            .first()
            .copied()
            .unwrap_or(BrowserRuntimePackAction::KeepCurrent);
        let operation_plan = plan_runtime_pack_operation(
            &manifest,
            &paths,
            &doctor,
            BrowserRuntimePackOperationRequest {
                operation: BrowserRuntimePackOperation::from_action(primary_action),
                trigger: BrowserRuntimePackPlanTrigger::TaskTime,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
                active_tasks: probe.active_tasks,
            },
        );
        BrowserRuntimePackStatusReport {
            manifest_pack_version: manifest.pack_version.clone(),
            runtime_root: paths.runtime_root.clone(),
            current_pack_dir: paths.current_pack_dir.clone(),
            filesystem: BrowserRuntimePackFilesystemProbeReport {
                snapshot: BrowserRuntimePackFilesystemSnapshot {
                    current_pack_dir: paths.current_pack_dir.clone(),
                    previous_pack_dir: manifest
                        .rollback_pack_version
                        .as_ref()
                        .map(|version| paths.packs_dir.join(version)),
                    manifest_path: paths.manifest_path.clone(),
                    manifest_status: BrowserRuntimePackManifestLoadStatus::Loaded,
                    manifest_present: true,
                    node_present: true,
                    playwright_package_present: true,
                    playwright_mcp_package_present: true,
                    worker_script_present: true,
                    browser_binary_present: true,
                    previous_pack_available: true,
                    versions_match: true,
                    cache_corrupt: false,
                    active_tasks: 0,
                    offline: false,
                },
                probe,
                manifest_load: BrowserRuntimePackManifestLoadOutcome {
                    status: BrowserRuntimePackManifestLoadStatus::Loaded,
                    path: paths.manifest_path,
                    manifest: Some(manifest),
                    error: None,
                },
            },
            doctor,
            primary_action,
            operation_plan,
            ready: true,
            can_run_browser_tasks: true,
            event_names: vec!["browser.runtime.status.reported".to_string()],
        }
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, contents: &str) -> std::io::Result<()> {
        fs::create_dir_all(path.parent().expect("script parent"))?;
        fs::write(path, contents)?;
        let mut permissions = fs::metadata(path)?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
    }
}
