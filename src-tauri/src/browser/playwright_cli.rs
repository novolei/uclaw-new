//! Playwright CLI provider contract shell.
//!
//! This module is intentionally pure. It defines the feature-flagged provider
//! readiness shape, JSON request envelope, and supervised child-worker boundary
//! for short-lived Playwright workers. Provider promotion and task routing stay
//! outside this module.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio::time::timeout;

use super::provider::{
    BrowserCapabilityProbe, BrowserProbeStatus, BrowserProviderCapabilities,
    BrowserProviderReadinessProbe, BrowserProviderStatus, BrowserSetupCheck,
};
use super::runtime_contracts::BrowserRuntimeFeatureFlags;
use super::runtime_pack::{BrowserRuntimePackEnvVar, BrowserRuntimePackStatusReport};

pub const PLAYWRIGHT_CLI_PROVIDER_ID: &str = "browser.playwright_cli";
pub const PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION: u16 = 1;
pub const DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS: u64 = 15_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightCliActionKind {
    Navigate,
    Click,
    Type,
    Screenshot,
    Extract,
    Wait,
}

impl PlaywrightCliActionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Navigate => "navigate",
            Self::Click => "click",
            Self::Type => "type",
            Self::Screenshot => "screenshot",
            Self::Extract => "extract",
            Self::Wait => "wait",
        }
    }
}

pub const PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS: &[PlaywrightCliActionKind] = &[
    PlaywrightCliActionKind::Navigate,
    PlaywrightCliActionKind::Click,
    PlaywrightCliActionKind::Type,
    PlaywrightCliActionKind::Screenshot,
    PlaywrightCliActionKind::Extract,
    PlaywrightCliActionKind::Wait,
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaywrightCliAddress {
    SemanticLocator { locator: String },
    UclawDomElementId { element_id: String },
    Coordinates { x: i32, y: i32 },
}

impl PlaywrightCliAddress {
    pub const fn priority(&self) -> u8 {
        match self {
            Self::SemanticLocator { .. } => 0,
            Self::UclawDomElementId { .. } => 1,
            Self::Coordinates { .. } => 2,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaywrightCliAction {
    Navigate {
        url: String,
    },
    Click {
        target: PlaywrightCliAddress,
    },
    Type {
        target: PlaywrightCliAddress,
        text: String,
    },
    Screenshot {
        full_page: bool,
    },
    Extract {
        target: Option<PlaywrightCliAddress>,
    },
    Wait {
        target: PlaywrightCliAddress,
        timeout_ms: Option<u64>,
    },
}

impl PlaywrightCliAction {
    pub const fn kind(&self) -> PlaywrightCliActionKind {
        match self {
            Self::Navigate { .. } => PlaywrightCliActionKind::Navigate,
            Self::Click { .. } => PlaywrightCliActionKind::Click,
            Self::Type { .. } => PlaywrightCliActionKind::Type,
            Self::Screenshot { .. } => PlaywrightCliActionKind::Screenshot,
            Self::Extract { .. } => PlaywrightCliActionKind::Extract,
            Self::Wait { .. } => PlaywrightCliActionKind::Wait,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliRuntimeEnv {
    pub manifest_pack_version: String,
    pub runtime_root: PathBuf,
    pub current_pack_dir: PathBuf,
    pub env: Vec<BrowserRuntimePackEnvVar>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliRequestEnvelope {
    pub schema_version: u16,
    pub provider_id: String,
    pub request_id: String,
    pub action: PlaywrightCliAction,
    pub timeout_ms: u64,
    pub artifact_policy: String,
    pub runtime: PlaywrightCliRuntimeEnv,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightCliChildWorkerConfig {
    pub node_binary_path: PathBuf,
    pub worker_script_path: PathBuf,
    pub timeout_ms: u64,
}

impl PlaywrightCliChildWorkerConfig {
    pub fn from_runtime_env(runtime: &PlaywrightCliRuntimeEnv) -> Self {
        Self {
            node_binary_path: runtime
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            worker_script_path: runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            timeout_ms: DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
        }
    }

    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightCliWorkerStatus {
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliWorkerErrorEnvelope {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliWorkerResultEnvelope {
    pub schema_version: u16,
    pub provider_id: String,
    pub request_id: String,
    pub status: PlaywrightCliWorkerStatus,
    pub summary: String,
    pub artifact_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PlaywrightCliWorkerErrorEnvelope>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightCliWorkerError {
    RuntimePathEscapesPack { path: PathBuf, pack_dir: PathBuf },
    SpawnFailed(String),
    StdinWriteFailed(String),
    StdoutReadFailed(String),
    StderrReadFailed(String),
    TimedOut { timeout_ms: u64 },
    NonZeroExit { code: Option<i32>, stderr: String },
    InvalidJson(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaywrightCliEnvelopeError {
    RuntimeNotReady,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightCliProviderExecutionStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliProviderExecutionError {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCliProviderExecutionResult {
    pub provider_id: String,
    pub request_id: String,
    pub action_kind: PlaywrightCliActionKind,
    pub status: PlaywrightCliProviderExecutionStatus,
    pub summary: String,
    pub artifact_refs: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<PlaywrightCliProviderExecutionError>,
}

pub fn playwright_cli_capabilities() -> BrowserProviderCapabilities {
    BrowserProviderCapabilities {
        provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        family: "browser".to_string(),
        display_name: "Playwright CLI".to_string(),
        actions: PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS
            .iter()
            .map(|action| action.as_str().to_string())
            .collect(),
        features: vec![
            "feature_flagged",
            "app_managed_runtime_pack",
            "short_lived_child_process",
            "stdin_stdout_json_envelope",
            "risk_based_artifacts",
            "no_raw_script_by_default",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        harness_subjects: vec![
            "browser.playwright_cli.envelope",
            "browser.playwright_cli.timeout",
            "browser.playwright_cli.locator_fallback",
            "browser.playwright_cli.artifacts",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }
}

pub fn playwright_cli_provider_status(
    flags: BrowserRuntimeFeatureFlags,
    runtime_report: Option<&BrowserRuntimePackStatusReport>,
) -> BrowserProviderStatus {
    let runtime_ready = runtime_report
        .map(|report| report.ready && report.can_run_browser_tasks)
        .unwrap_or(false);
    let mut notes = vec![
        "Provider is feature-flagged and disabled by default.".to_string(),
        "Rust supervisor owns future timeout, kill, retry, and recovery.".to_string(),
    ];

    let setup_checks = if !flags.playwright_cli {
        notes.push("playwright_cli feature flag is off.".to_string());
        vec![BrowserSetupCheck::unsupported(
            "playwright_cli_feature_flag",
            "Playwright CLI feature flag",
            "Enable the playwright_cli feature flag before selecting this provider.",
        )]
    } else if runtime_ready {
        vec![
            BrowserSetupCheck::passed("playwright_cli_feature_flag", "Playwright CLI feature flag"),
            BrowserSetupCheck::passed("runtime_pack_ready", "App-managed Playwright runtime pack"),
        ]
    } else {
        notes.push("Runtime pack is not ready for Browser tasks.".to_string());
        vec![
            BrowserSetupCheck::passed("playwright_cli_feature_flag", "Playwright CLI feature flag"),
            BrowserSetupCheck::failed(
                "runtime_pack_ready",
                "App-managed Playwright runtime pack",
                "Prepare or repair the Browser runtime pack before enabling Playwright CLI actions.",
            ),
        ]
    };

    let capability_status = if flags.playwright_cli && runtime_ready {
        BrowserProbeStatus::Passed
    } else {
        BrowserProbeStatus::Skipped
    };
    let capability_probes = PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS
        .iter()
        .map(|action| BrowserCapabilityProbe {
            action: action.as_str().to_string(),
            required: true,
            status: capability_status,
            remediation: None,
        })
        .collect();

    BrowserProviderStatus::from_probe(
        playwright_cli_capabilities(),
        BrowserProviderReadinessProbe {
            provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
            setup_checks,
            capability_probes,
            active_contexts: 0,
            notes,
        },
    )
}

pub fn build_playwright_cli_request_envelope(
    request_id: impl Into<String>,
    action: PlaywrightCliAction,
    runtime_report: &BrowserRuntimePackStatusReport,
) -> Result<PlaywrightCliRequestEnvelope, PlaywrightCliEnvelopeError> {
    if !runtime_report.ready || !runtime_report.can_run_browser_tasks {
        return Err(PlaywrightCliEnvelopeError::RuntimeNotReady);
    }

    Ok(PlaywrightCliRequestEnvelope {
        schema_version: PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION,
        provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        request_id: request_id.into(),
        action,
        timeout_ms: DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
        artifact_policy: "risk_based".to_string(),
        runtime: PlaywrightCliRuntimeEnv {
            manifest_pack_version: runtime_report.manifest_pack_version.clone(),
            runtime_root: runtime_report.runtime_root.clone(),
            current_pack_dir: runtime_report.current_pack_dir.clone(),
            env: runtime_report.operation_plan.env.clone(),
        },
    })
}

pub async fn execute_playwright_cli_provider_action(
    request_id: impl Into<String>,
    flags: BrowserRuntimeFeatureFlags,
    action: PlaywrightCliAction,
    runtime_report: &BrowserRuntimePackStatusReport,
) -> PlaywrightCliProviderExecutionResult {
    execute_playwright_cli_provider_action_with_timeout(
        request_id,
        flags,
        action,
        runtime_report,
        DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
    )
    .await
}

pub async fn execute_playwright_cli_provider_action_with_timeout(
    request_id: impl Into<String>,
    flags: BrowserRuntimeFeatureFlags,
    action: PlaywrightCliAction,
    runtime_report: &BrowserRuntimePackStatusReport,
    worker_timeout_ms: u64,
) -> PlaywrightCliProviderExecutionResult {
    let request_id = request_id.into();
    let action_kind = action.kind();
    if !flags.playwright_cli {
        return provider_blocked_result(
            request_id,
            action_kind,
            "feature_flag_disabled",
            "playwright_cli feature flag is disabled",
            false,
        );
    }

    let envelope =
        match build_playwright_cli_request_envelope(request_id.clone(), action, runtime_report) {
            Ok(envelope) => envelope,
            Err(PlaywrightCliEnvelopeError::RuntimeNotReady) => {
                return provider_blocked_result(
                    request_id,
                    action_kind,
                    "runtime_not_ready",
                    "Browser runtime pack is not ready for Playwright CLI actions",
                    true,
                );
            }
        };
    let config = PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
        .with_timeout_ms(worker_timeout_ms);

    match run_playwright_cli_child_worker(&envelope, config).await {
        Ok(worker_result) => provider_result_from_worker(action_kind, worker_result),
        Err(error) => provider_result_from_runner_error(request_id, action_kind, error),
    }
}

pub async fn run_playwright_cli_child_worker(
    envelope: &PlaywrightCliRequestEnvelope,
    config: PlaywrightCliChildWorkerConfig,
) -> Result<PlaywrightCliWorkerResultEnvelope, PlaywrightCliWorkerError> {
    validate_worker_path(&config.node_binary_path, &envelope.runtime.current_pack_dir)?;
    validate_worker_path(
        &config.worker_script_path,
        &envelope.runtime.current_pack_dir,
    )?;

    let mut command = Command::new(&config.node_binary_path);
    command
        .arg(&config.worker_script_path)
        .current_dir(&envelope.runtime.current_pack_dir)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for env_var in &envelope.runtime.env {
        command.env(&env_var.name, &env_var.value);
    }

    let mut child = command
        .spawn()
        .map_err(|error| PlaywrightCliWorkerError::SpawnFailed(error.to_string()))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| PlaywrightCliWorkerError::StdinWriteFailed("stdin unavailable".into()))?;
    let request_bytes = serde_json::to_vec(envelope)
        .map_err(|error| PlaywrightCliWorkerError::StdinWriteFailed(error.to_string()))?;
    stdin
        .write_all(&request_bytes)
        .await
        .map_err(|error| PlaywrightCliWorkerError::StdinWriteFailed(error.to_string()))?;
    stdin
        .write_all(b"\n")
        .await
        .map_err(|error| PlaywrightCliWorkerError::StdinWriteFailed(error.to_string()))?;
    stdin
        .shutdown()
        .await
        .map_err(|error| PlaywrightCliWorkerError::StdinWriteFailed(error.to_string()))?;
    drop(stdin);

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| PlaywrightCliWorkerError::StdoutReadFailed("stdout unavailable".into()))?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| PlaywrightCliWorkerError::StderrReadFailed("stderr unavailable".into()))?;
    let stdout_task = tokio::spawn(read_pipe_to_string(stdout));
    let stderr_task = tokio::spawn(read_pipe_to_string(stderr));

    let status = match timeout(Duration::from_millis(config.timeout_ms), child.wait()).await {
        Ok(Ok(status)) => status,
        Ok(Err(error)) => return Err(PlaywrightCliWorkerError::SpawnFailed(error.to_string())),
        Err(_) => {
            let _ = child.kill().await;
            stdout_task.abort();
            stderr_task.abort();
            return Err(PlaywrightCliWorkerError::TimedOut {
                timeout_ms: config.timeout_ms,
            });
        }
    };

    let stdout = stdout_task
        .await
        .map_err(|error| PlaywrightCliWorkerError::StdoutReadFailed(error.to_string()))?
        .map_err(|error| PlaywrightCliWorkerError::StdoutReadFailed(error.to_string()))?;
    let stderr = stderr_task
        .await
        .map_err(|error| PlaywrightCliWorkerError::StderrReadFailed(error.to_string()))?
        .map_err(|error| PlaywrightCliWorkerError::StderrReadFailed(error.to_string()))?;

    if !status.success() {
        return Err(PlaywrightCliWorkerError::NonZeroExit {
            code: status.code(),
            stderr,
        });
    }

    let result = serde_json::from_str::<PlaywrightCliWorkerResultEnvelope>(&stdout)
        .map_err(|error| PlaywrightCliWorkerError::InvalidJson(error.to_string()))?;
    if result.schema_version != PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION
        || result.provider_id != PLAYWRIGHT_CLI_PROVIDER_ID
        || result.request_id != envelope.request_id
    {
        return Err(PlaywrightCliWorkerError::InvalidJson(
            "worker result envelope does not match request envelope".into(),
        ));
    }
    Ok(result)
}

fn provider_result_from_worker(
    action_kind: PlaywrightCliActionKind,
    worker_result: PlaywrightCliWorkerResultEnvelope,
) -> PlaywrightCliProviderExecutionResult {
    let status = match worker_result.status {
        PlaywrightCliWorkerStatus::Succeeded => PlaywrightCliProviderExecutionStatus::Succeeded,
        PlaywrightCliWorkerStatus::Failed => PlaywrightCliProviderExecutionStatus::Failed,
    };
    let error = worker_result
        .error
        .map(|error| PlaywrightCliProviderExecutionError {
            code: error.code,
            message: error.message,
            retryable: error.retryable,
        })
        .or_else(|| {
            (status == PlaywrightCliProviderExecutionStatus::Failed).then(|| {
                PlaywrightCliProviderExecutionError {
                    code: "worker_failed".to_string(),
                    message: "Playwright CLI worker failed without a structured error".to_string(),
                    retryable: false,
                }
            })
        });

    PlaywrightCliProviderExecutionResult {
        provider_id: worker_result.provider_id,
        request_id: worker_result.request_id,
        action_kind,
        status,
        summary: worker_result.summary,
        artifact_refs: worker_result.artifact_refs,
        output: worker_result.output,
        error,
    }
}

fn provider_result_from_runner_error(
    request_id: String,
    action_kind: PlaywrightCliActionKind,
    error: PlaywrightCliWorkerError,
) -> PlaywrightCliProviderExecutionResult {
    let error = provider_execution_error_from_runner_error(error);
    PlaywrightCliProviderExecutionResult {
        provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        request_id,
        action_kind,
        status: PlaywrightCliProviderExecutionStatus::Failed,
        summary: "Playwright CLI worker runner failed".to_string(),
        artifact_refs: vec![],
        output: None,
        error: Some(error),
    }
}

fn provider_blocked_result(
    request_id: String,
    action_kind: PlaywrightCliActionKind,
    code: &str,
    message: &str,
    retryable: bool,
) -> PlaywrightCliProviderExecutionResult {
    PlaywrightCliProviderExecutionResult {
        provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        request_id,
        action_kind,
        status: PlaywrightCliProviderExecutionStatus::Blocked,
        summary: message.to_string(),
        artifact_refs: vec![],
        output: None,
        error: Some(PlaywrightCliProviderExecutionError {
            code: code.to_string(),
            message: message.to_string(),
            retryable,
        }),
    }
}

fn provider_execution_error_from_runner_error(
    error: PlaywrightCliWorkerError,
) -> PlaywrightCliProviderExecutionError {
    match error {
        PlaywrightCliWorkerError::RuntimePathEscapesPack { path, pack_dir } => {
            PlaywrightCliProviderExecutionError {
                code: "runtime_path_escapes_pack".to_string(),
                message: format!(
                    "worker runtime path {} escapes app-managed pack {}",
                    path.display(),
                    pack_dir.display()
                ),
                retryable: false,
            }
        }
        PlaywrightCliWorkerError::SpawnFailed(message) => PlaywrightCliProviderExecutionError {
            code: "worker_spawn_failed".to_string(),
            message,
            retryable: true,
        },
        PlaywrightCliWorkerError::StdinWriteFailed(message) => {
            PlaywrightCliProviderExecutionError {
                code: "worker_stdin_failed".to_string(),
                message,
                retryable: true,
            }
        }
        PlaywrightCliWorkerError::StdoutReadFailed(message) => {
            PlaywrightCliProviderExecutionError {
                code: "worker_stdout_failed".to_string(),
                message,
                retryable: true,
            }
        }
        PlaywrightCliWorkerError::StderrReadFailed(message) => {
            PlaywrightCliProviderExecutionError {
                code: "worker_stderr_failed".to_string(),
                message,
                retryable: true,
            }
        }
        PlaywrightCliWorkerError::TimedOut { timeout_ms } => PlaywrightCliProviderExecutionError {
            code: "timeout".to_string(),
            message: format!("Playwright CLI worker timed out after {timeout_ms} ms"),
            retryable: true,
        },
        PlaywrightCliWorkerError::NonZeroExit { code, stderr } => {
            PlaywrightCliProviderExecutionError {
                code: "worker_nonzero_exit".to_string(),
                message: format!(
                    "Playwright CLI worker exited with code {:?}: {}",
                    code,
                    stderr.trim()
                ),
                retryable: false,
            }
        }
        PlaywrightCliWorkerError::InvalidJson(message) => PlaywrightCliProviderExecutionError {
            code: "worker_invalid_json".to_string(),
            message,
            retryable: false,
        },
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

fn validate_worker_path(
    path: &PathBuf,
    current_pack_dir: &PathBuf,
) -> Result<(), PlaywrightCliWorkerError> {
    let canonical_pack_dir = current_pack_dir.canonicalize().map_err(|_| {
        PlaywrightCliWorkerError::RuntimePathEscapesPack {
            path: path.clone(),
            pack_dir: current_pack_dir.clone(),
        }
    })?;
    let canonical_path =
        path.canonicalize()
            .map_err(|_| PlaywrightCliWorkerError::RuntimePathEscapesPack {
                path: path.clone(),
                pack_dir: current_pack_dir.clone(),
            })?;
    if canonical_path.starts_with(&canonical_pack_dir) {
        Ok(())
    } else {
        Err(PlaywrightCliWorkerError::RuntimePathEscapesPack {
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
    fn disabled_feature_flag_keeps_playwright_cli_unavailable() {
        let status = playwright_cli_provider_status(
            BrowserRuntimeFeatureFlags::safe_defaults(),
            Some(&ready_runtime_report()),
        );

        assert_eq!(status.provider_id, PLAYWRIGHT_CLI_PROVIDER_ID);
        assert_eq!(
            status.readiness,
            super::super::provider::BrowserProviderReadiness::Unavailable
        );
        assert!(!status.ready);
        assert!(status
            .remediation
            .iter()
            .any(|item| item.contains("playwright_cli feature flag")));
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Skipped));
    }

    #[test]
    fn enabled_provider_needs_setup_until_runtime_pack_is_ready() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_cli = true;

        let status = playwright_cli_provider_status(flags, Some(&missing_runtime_report()));

        assert_eq!(
            status.readiness,
            super::super::provider::BrowserProviderReadiness::NeedsSetup
        );
        assert!(!status.ready);
        assert!(status
            .remediation
            .iter()
            .any(|item| item.contains("Prepare or repair")));
    }

    #[test]
    fn enabled_provider_is_ready_when_runtime_pack_can_run_browser_tasks() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_cli = true;

        let status = playwright_cli_provider_status(flags, Some(&ready_runtime_report()));

        assert_eq!(
            status.readiness,
            super::super::provider::BrowserProviderReadiness::Ready
        );
        assert!(status.ready);
        assert_eq!(
            status.capabilities.actions,
            vec!["navigate", "click", "type", "screenshot", "extract", "wait"]
        );
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Passed));
    }

    #[test]
    fn enabled_provider_needs_setup_when_runtime_report_is_ready_but_tasks_cannot_run() {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_cli = true;
        let mut report = ready_runtime_report();
        report.can_run_browser_tasks = false;

        let status = playwright_cli_provider_status(flags, Some(&report));

        assert_eq!(
            status.readiness,
            super::super::provider::BrowserProviderReadiness::NeedsSetup
        );
        assert!(!status.ready);
        assert!(status
            .capability_probes
            .iter()
            .all(|probe| probe.status == BrowserProbeStatus::Skipped));
    }

    #[test]
    fn request_envelope_serializes_declarative_action_and_runtime_env() {
        let report = ready_runtime_report();
        let envelope = build_playwright_cli_request_envelope(
            "req-1",
            PlaywrightCliAction::Click {
                target: PlaywrightCliAddress::SemanticLocator {
                    locator: "button[name='Continue']".to_string(),
                },
            },
            &report,
        )
        .expect("ready runtime envelope");
        let value = serde_json::to_value(&envelope).expect("serialize envelope");

        assert_eq!(value["schemaVersion"], 1);
        assert_eq!(value["providerId"], PLAYWRIGHT_CLI_PROVIDER_ID);
        assert_eq!(value["action"]["kind"], "click");
        assert_eq!(value["action"]["target"]["kind"], "semantic_locator");
        assert_eq!(value["artifactPolicy"], "risk_based");
        assert_eq!(
            value["runtime"]["manifestPackVersion"],
            BrowserRuntimePackManifest::v1_default().pack_version
        );
        assert!(value["runtime"]["env"][0]["name"]
            .as_str()
            .expect("env name")
            .contains("PLAYWRIGHT_BROWSERS_PATH"));
    }

    #[test]
    fn request_envelope_rejects_runtime_that_cannot_run_browser_tasks() {
        let mut report = ready_runtime_report();
        report.can_run_browser_tasks = false;
        let err = build_playwright_cli_request_envelope(
            "req-2",
            PlaywrightCliAction::Navigate {
                url: "https://example.com".to_string(),
            },
            &report,
        )
        .expect_err("task-unready runtime should reject envelope");

        assert_eq!(err, PlaywrightCliEnvelopeError::RuntimeNotReady);
    }

    #[tokio::test]
    async fn provider_adapter_blocks_disabled_feature_flag() {
        let result = execute_playwright_cli_provider_action(
            "req-provider-disabled",
            BrowserRuntimeFeatureFlags::safe_defaults(),
            PlaywrightCliAction::Navigate {
                url: "https://example.com".to_string(),
            },
            &ready_runtime_report(),
        )
        .await;

        assert_eq!(result.status, PlaywrightCliProviderExecutionStatus::Blocked);
        let error = result.error.expect("provider error");
        assert_eq!(error.code, "feature_flag_disabled");
        assert!(!error.retryable);
    }

    #[tokio::test]
    async fn provider_adapter_blocks_unready_runtime() {
        let result = execute_playwright_cli_provider_action(
            "req-provider-runtime",
            enabled_playwright_cli_flags(),
            PlaywrightCliAction::Screenshot { full_page: false },
            &missing_runtime_report(),
        )
        .await;

        assert_eq!(result.status, PlaywrightCliProviderExecutionStatus::Blocked);
        let error = result.error.expect("provider error");
        assert_eq!(error.code, "runtime_not_ready");
        assert!(error.retryable);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_adapter_executes_worker_successfully() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report = fixture_runtime_report(temp.path());
        write_executable(
            &report.current_pack_dir.join("node").join("bin").join("node"),
            "#!/bin/sh\ntest -f \"$1\" || exit 9\nsleep 0.1\nprintf '%s\\n' '{\"schemaVersion\":1,\"providerId\":\"browser.playwright_cli\",\"requestId\":\"req-provider-success\",\"status\":\"succeeded\",\"summary\":\"provider action completed\",\"artifactRefs\":[\"artifact://browser/provider\"],\"output\":{\"clicked\":true}}'\n",
        );
        write_executable(
            &report
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\n# provider fixture worker marker\n",
        );

        let result = execute_playwright_cli_provider_action_with_timeout(
            "req-provider-success",
            enabled_playwright_cli_flags(),
            PlaywrightCliAction::Click {
                target: PlaywrightCliAddress::Coordinates { x: 10, y: 20 },
            },
            &report,
            5_000,
        )
        .await;

        assert_eq!(
            result.status,
            PlaywrightCliProviderExecutionStatus::Succeeded
        );
        assert_eq!(result.action_kind, PlaywrightCliActionKind::Click);
        assert_eq!(result.summary, "provider action completed");
        assert_eq!(result.artifact_refs, vec!["artifact://browser/provider"]);
        assert_eq!(result.output.expect("output")["clicked"], true);
        assert!(result.error.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_adapter_preserves_worker_structured_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report = fixture_runtime_report(temp.path());
        write_executable(
            &report.current_pack_dir.join("node").join("bin").join("node"),
            "#!/bin/sh\nsleep 0.1\nprintf '%s\\n' '{\"schemaVersion\":1,\"providerId\":\"browser.playwright_cli\",\"requestId\":\"req-provider-failure\",\"status\":\"failed\",\"summary\":\"provider action failed\",\"artifactRefs\":[],\"error\":{\"code\":\"action_failed\",\"message\":\"missing locator\",\"retryable\":false}}'\n",
        );
        write_executable(
            &report
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\n# provider fixture worker marker\n",
        );

        let result = execute_playwright_cli_provider_action_with_timeout(
            "req-provider-failure",
            enabled_playwright_cli_flags(),
            PlaywrightCliAction::Click {
                target: PlaywrightCliAddress::SemanticLocator {
                    locator: "text=missing".to_string(),
                },
            },
            &report,
            5_000,
        )
        .await;

        assert_eq!(result.status, PlaywrightCliProviderExecutionStatus::Failed);
        let error = result.error.expect("provider error");
        assert_eq!(error.code, "action_failed");
        assert_eq!(error.message, "missing locator");
        assert!(!error.retryable);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn provider_adapter_maps_runner_timeout_to_retryable_failure() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report = fixture_runtime_report(temp.path());
        write_executable(
            &report
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            "#!/bin/sh\nexec \"$1\"\n",
        );
        write_executable(
            &report
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\ncat >/dev/null\nsleep 2\n",
        );

        let result = execute_playwright_cli_provider_action_with_timeout(
            "req-provider-timeout",
            enabled_playwright_cli_flags(),
            PlaywrightCliAction::Wait {
                target: PlaywrightCliAddress::Coordinates { x: 1, y: 2 },
                timeout_ms: Some(25),
            },
            &report,
            25,
        )
        .await;

        assert_eq!(result.status, PlaywrightCliProviderExecutionStatus::Failed);
        let error = result.error.expect("provider error");
        assert_eq!(error.code, "timeout");
        assert!(error.retryable);
    }

    #[test]
    fn addressing_priority_matches_locator_dom_id_coordinate_order() {
        assert_eq!(
            PlaywrightCliAddress::SemanticLocator {
                locator: "#submit".to_string()
            }
            .priority(),
            0
        );
        assert_eq!(
            PlaywrightCliAddress::UclawDomElementId {
                element_id: "node-42".to_string()
            }
            .priority(),
            1
        );
        assert_eq!(
            PlaywrightCliAddress::Coordinates { x: 12, y: 34 }.priority(),
            2
        );
    }

    #[test]
    fn declarative_actions_do_not_include_raw_script_escape_hatch() {
        let action_names: Vec<&str> = PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS
            .iter()
            .map(|action| action.as_str())
            .collect();

        assert!(!action_names.contains(&"script"));
        assert!(!action_names.contains(&"evaluate"));
        assert_eq!(
            action_names,
            vec!["navigate", "click", "type", "screenshot", "extract", "wait"]
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_worker_runs_app_managed_node_and_parses_one_result_envelope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        write_executable(
            &envelope.runtime.current_pack_dir.join("node").join("bin").join("node"),
            "#!/bin/sh\ntest -f \"$1\" || exit 9\nsleep 0.1\nprintf '%s\\n' '{\"schemaVersion\":1,\"providerId\":\"browser.playwright_cli\",\"requestId\":\"req-worker\",\"status\":\"succeeded\",\"summary\":\"fixture action completed\",\"artifactRefs\":[\"artifact://browser/1\"]}'\n",
        );
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\n# fixture worker path marker\n",
        );

        let result = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(5_000),
        )
        .await
        .expect("worker result");

        assert_eq!(
            result.schema_version,
            PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION
        );
        assert_eq!(result.provider_id, PLAYWRIGHT_CLI_PROVIDER_ID);
        assert_eq!(result.request_id, "req-worker");
        assert_eq!(result.status, PlaywrightCliWorkerStatus::Succeeded);
        assert_eq!(result.artifact_refs, vec!["artifact://browser/1"]);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_worker_rejects_node_path_outside_app_managed_pack() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        let config = PlaywrightCliChildWorkerConfig {
            node_binary_path: PathBuf::from("/usr/bin/node"),
            worker_script_path: envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            timeout_ms: 1_000,
        };

        let error = run_playwright_cli_child_worker(&envelope, config)
            .await
            .expect_err("global node path should be rejected");

        assert!(matches!(
            error,
            PlaywrightCliWorkerError::RuntimePathEscapesPack { .. }
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_worker_timeout_kills_hung_worker() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            "#!/bin/sh\nexec \"$1\"\n",
        );
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\ncat >/dev/null\nsleep 2\n",
        );

        let error = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime).with_timeout_ms(25),
        )
        .await
        .expect_err("hung worker should time out");

        assert_eq!(error, PlaywrightCliWorkerError::TimedOut { timeout_ms: 25 });
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_worker_reports_nonzero_exit_with_stderr() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            "#!/bin/sh\nsleep 0.1\necho worker failed >&2\nexit 17\n",
        );
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\n# fixture worker path marker\n",
        );

        let error = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(5_000),
        )
        .await
        .expect_err("worker should report nonzero exit");

        assert!(matches!(
            error,
            PlaywrightCliWorkerError::NonZeroExit {
                code: Some(17),
                stderr
            } if stderr.contains("worker failed")
        ));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn child_worker_rejects_mismatched_result_envelope() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope(temp.path());
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("node")
                .join("bin")
                .join("node"),
            "#!/bin/sh\nsleep 0.1\nprintf '%s\\n' '{\"schemaVersion\":1,\"providerId\":\"browser.playwright_cli\",\"requestId\":\"wrong-request\",\"status\":\"succeeded\",\"summary\":\"fixture action completed\",\"artifactRefs\":[]}'\n",
        );
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            "#!/bin/sh\n# fixture worker path marker\n",
        );

        let error = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(5_000),
        )
        .await
        .expect_err("mismatched result should be rejected");

        assert_eq!(
            error,
            PlaywrightCliWorkerError::InvalidJson(
                "worker result envelope does not match request envelope".into()
            )
        );
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_script_executes_screenshot_with_fake_playwright_module() {
        let temp = tempfile::tempdir().expect("tempdir");
        let mut envelope = fixture_envelope_with_action(
            temp.path(),
            PlaywrightCliAction::Screenshot { full_page: true },
        );
        let artifact_dir = envelope.runtime.current_pack_dir.join("artifacts");
        envelope.runtime.env.push(BrowserRuntimePackEnvVar {
            name: "UCLAW_BROWSER_ARTIFACT_DIR".to_string(),
            value: artifact_dir.clone(),
        });
        prepare_real_worker_fixture(&envelope).expect("worker fixture");

        let result = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(5_000),
        )
        .await
        .expect("worker result");

        assert_eq!(result.status, PlaywrightCliWorkerStatus::Succeeded);
        assert_eq!(result.summary, "captured screenshot (8 bytes)");
        assert_eq!(result.artifact_refs.len(), 1);
        assert!(result.artifact_refs[0].starts_with("file://"));
        assert_eq!(result.output.as_ref().expect("output")["fullPage"], true);
        assert_eq!(result.output.as_ref().expect("output")["bytes"], 8);
        assert!(artifact_dir.join("req-worker-screenshot.png").is_file());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_script_returns_structured_failure_for_locator_error() {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope_with_action(
            temp.path(),
            PlaywrightCliAction::Click {
                target: PlaywrightCliAddress::SemanticLocator {
                    locator: "text=missing".to_string(),
                },
            },
        );
        prepare_real_worker_fixture(&envelope).expect("worker fixture");

        let result = run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(5_000),
        )
        .await
        .expect("worker result");

        assert_eq!(result.status, PlaywrightCliWorkerStatus::Failed);
        let error = result.error.expect("worker error");
        assert_eq!(error.code, "action_failed");
        assert!(!error.retryable);
        assert!(error.message.contains("missing locator"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_script_covers_addressing_fallback_outputs_without_click_artifacts() {
        let cases = vec![
            (
                PlaywrightCliAction::Click {
                    target: PlaywrightCliAddress::SemanticLocator {
                        locator: "text=Continue".to_string(),
                    },
                },
                "semantic_locator",
            ),
            (
                PlaywrightCliAction::Click {
                    target: PlaywrightCliAddress::UclawDomElementId {
                        element_id: "node-42".to_string(),
                    },
                },
                "uclaw_dom_element_id",
            ),
            (
                PlaywrightCliAction::Click {
                    target: PlaywrightCliAddress::Coordinates { x: 32, y: 48 },
                },
                "coordinates",
            ),
        ];

        for (action, expected_kind) in cases {
            let temp = tempfile::tempdir().expect("tempdir");
            let mut envelope = fixture_envelope_with_action(temp.path(), action);
            let artifact_dir = envelope.runtime.current_pack_dir.join("artifacts");
            envelope.runtime.env.push(BrowserRuntimePackEnvVar {
                name: "UCLAW_BROWSER_ARTIFACT_DIR".to_string(),
                value: artifact_dir.clone(),
            });
            prepare_real_worker_fixture(&envelope).expect("worker fixture");

            let result = run_playwright_cli_child_worker(
                &envelope,
                PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                    .with_timeout_ms(5_000),
            )
            .await
            .expect("worker result");

            assert_eq!(result.status, PlaywrightCliWorkerStatus::Succeeded);
            assert_eq!(result.artifact_refs, Vec::<String>::new());
            assert!(!artifact_dir.join("req-worker-screenshot.png").exists());
            assert_eq!(
                result.output.as_ref().expect("output")["addressingKind"],
                expected_kind
            );
            assert_state_diff_observed(&result.output.as_ref().expect("output")["stateDiff"]);
        }
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn worker_script_covers_type_extract_and_wait_outputs() {
        let type_result = run_real_worker_fixture(
            PlaywrightCliAction::Type {
                target: PlaywrightCliAddress::SemanticLocator {
                    locator: "label=Email".to_string(),
                },
                text: "hello".to_string(),
            },
            5_000,
        )
        .await;
        assert_eq!(type_result.status, PlaywrightCliWorkerStatus::Succeeded);
        assert_eq!(
            type_result.output.as_ref().expect("type output")["addressingKind"],
            "semantic_locator"
        );
        assert_eq!(
            type_result.output.as_ref().expect("type output")["textLength"],
            5
        );
        assert_state_diff_observed(&type_result.output.as_ref().expect("type output")["stateDiff"]);

        let extract_result =
            run_real_worker_fixture(PlaywrightCliAction::Extract { target: None }, 5_000).await;
        assert_eq!(extract_result.status, PlaywrightCliWorkerStatus::Succeeded);
        assert_eq!(
            extract_result.output.as_ref().expect("extract output")["text"],
            "body:body"
        );

        let wait_result = run_real_worker_fixture(
            PlaywrightCliAction::Wait {
                target: PlaywrightCliAddress::Coordinates { x: 7, y: 9 },
                timeout_ms: Some(33),
            },
            5_000,
        )
        .await;
        assert_eq!(wait_result.status, PlaywrightCliWorkerStatus::Succeeded);
        assert_eq!(
            wait_result.output.as_ref().expect("wait output")["addressingKind"],
            "coordinates"
        );
        assert_eq!(
            wait_result.output.as_ref().expect("wait output")["timeoutMs"],
            33
        );
        assert_state_diff_observed(&wait_result.output.as_ref().expect("wait output")["stateDiff"]);
    }

    fn ready_runtime_report() -> BrowserRuntimePackStatusReport {
        runtime_report_from_probe(BrowserRuntimePackProbe::ready())
    }

    fn enabled_playwright_cli_flags() -> BrowserRuntimeFeatureFlags {
        let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
        flags.playwright_cli = true;
        flags
    }

    fn fixture_envelope(root: &Path) -> PlaywrightCliRequestEnvelope {
        fixture_envelope_with_action(root, PlaywrightCliAction::Screenshot { full_page: false })
    }

    fn fixture_runtime_report(root: &Path) -> BrowserRuntimePackStatusReport {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(root.join("runtime"), &manifest);
        let mut report = runtime_report_from_probe(BrowserRuntimePackProbe::ready());
        report.runtime_root = paths.runtime_root;
        report.current_pack_dir = paths.current_pack_dir.clone();
        report.operation_plan.env = vec![];
        report.filesystem.snapshot.current_pack_dir = paths.current_pack_dir;
        report.filesystem.snapshot.manifest_path = paths.manifest_path;
        report
    }

    fn fixture_envelope_with_action(
        root: &Path,
        action: PlaywrightCliAction,
    ) -> PlaywrightCliRequestEnvelope {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(root.join("runtime"), &manifest);
        PlaywrightCliRequestEnvelope {
            schema_version: PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION,
            provider_id: PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
            request_id: "req-worker".to_string(),
            action,
            timeout_ms: DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
            artifact_policy: "risk_based".to_string(),
            runtime: PlaywrightCliRuntimeEnv {
                manifest_pack_version: manifest.pack_version,
                runtime_root: paths.runtime_root,
                current_pack_dir: paths.current_pack_dir,
                env: vec![],
            },
        }
    }

    #[cfg(unix)]
    fn prepare_real_worker_fixture(envelope: &PlaywrightCliRequestEnvelope) -> std::io::Result<()> {
        let node_path = envelope
            .runtime
            .current_pack_dir
            .join("node")
            .join("bin")
            .join("node");
        let node_binary = find_node_binary().expect("node binary for fixture");
        write_executable(
            &node_path,
            &format!(
                "#!/bin/sh\nexec \"{}\" \"$@\"\n",
                node_binary.display().to_string().replace('"', "\\\"")
            ),
        );
        write_executable(
            &envelope
                .runtime
                .current_pack_dir
                .join("worker")
                .join("uclaw-playwright-worker.mjs"),
            include_str!("../../resources/browser-runtime/worker/uclaw-playwright-worker.mjs"),
        );
        write_fake_playwright_module(&envelope.runtime.current_pack_dir)
    }

    #[cfg(unix)]
    async fn run_real_worker_fixture(
        action: PlaywrightCliAction,
        timeout_ms: u64,
    ) -> PlaywrightCliWorkerResultEnvelope {
        let temp = tempfile::tempdir().expect("tempdir");
        let envelope = fixture_envelope_with_action(temp.path(), action);
        prepare_real_worker_fixture(&envelope).expect("worker fixture");
        run_playwright_cli_child_worker(
            &envelope,
            PlaywrightCliChildWorkerConfig::from_runtime_env(&envelope.runtime)
                .with_timeout_ms(timeout_ms),
        )
        .await
        .expect("worker result")
    }

    fn assert_state_diff_observed(diff: &serde_json::Value) {
        assert_eq!(diff["observed"], true);
        assert!(diff["before"]["url"].is_string());
        assert!(diff["after"]["url"].is_string());
        assert!(diff["before"]["bodyTextHash"].is_string());
        assert!(diff["after"]["bodyTextHash"].is_string());
        assert!(diff["before"]["bodyTextLength"].is_number());
        assert!(diff["after"]["bodyTextLength"].is_number());
        assert!(diff["changedFields"].is_array());
    }

    #[cfg(unix)]
    fn find_node_binary() -> Option<PathBuf> {
        let output = std::process::Command::new("sh")
            .arg("-lc")
            .arg("command -v node")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let path = String::from_utf8(output.stdout).ok()?.trim().to_string();
        if path.is_empty() {
            None
        } else {
            Some(PathBuf::from(path))
        }
    }

    #[cfg(unix)]
    fn write_fake_playwright_module(pack_dir: &Path) -> std::io::Result<()> {
        let module_dir = pack_dir.join("node_modules").join("playwright");
        fs::create_dir_all(&module_dir)?;
        fs::write(
            module_dir.join("package.json"),
            r#"{"name":"playwright","type":"module","main":"index.js"}"#,
        )?;
        fs::write(
            module_dir.join("index.js"),
            r#"
function makeLocator(name) {
  return {
    async click() {
      if (name.includes('missing')) throw new Error('missing locator');
    },
    async fill(text) {
      if (!text) throw new Error('empty text');
    },
    async textContent() {
      return `text:${name}`;
    },
    async waitFor() {
      if (name.includes('missing')) throw new Error('missing locator');
    }
  };
}

const page = {
  _url: 'about:blank',
  setDefaultTimeout() {},
  async goto(url) {
    this._url = url;
  },
  url() {
    return this._url;
  },
  async title() {
    return 'Fake Page';
  },
  locator(selector) {
    return makeLocator(selector);
  },
  getByText(text) {
    return makeLocator(`text=${text}`);
  },
  getByLabel(label) {
    return makeLocator(`label=${label}`);
  },
  getByTestId(testId) {
    return makeLocator(`testid=${testId}`);
  },
  getByRole(role, options) {
    return makeLocator(`role=${role};name=${options?.name ?? ''}`);
  },
  mouse: {
    async click() {}
  },
  keyboard: {
    async type() {}
  },
  async screenshot() {
    return Buffer.from('fake-png');
  },
  async textContent(selector) {
    return `body:${selector}`;
  },
  async evaluate() {
    return { tagName: 'BODY', id: null, name: null, role: null };
  },
  async waitForTimeout() {}
};

export const chromium = {
  async launch() {
    return {
      async newContext() {
        return {
          async newPage() {
            return page;
          }
        };
      },
      async close() {}
    };
  }
};
"#,
        )
    }

    #[cfg(unix)]
    fn write_executable(path: &Path, contents: &str) {
        fs::create_dir_all(path.parent().expect("script parent")).expect("script parent");
        fs::write(path, contents).expect("script contents");
        let mut permissions = fs::metadata(path).expect("script metadata").permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).expect("script executable");
    }

    fn missing_runtime_report() -> BrowserRuntimePackStatusReport {
        runtime_report_from_probe(BrowserRuntimePackProbe {
            manifest_present: false,
            node_present: false,
            playwright_package_present: false,
            browser_binary_present: false,
            ..BrowserRuntimePackProbe::ready()
        })
    }

    fn runtime_report_from_probe(probe: BrowserRuntimePackProbe) -> BrowserRuntimePackStatusReport {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(Path::new("/tmp/uclaw-browser-runtime"), &manifest);
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
        let ready = doctor.ready;
        BrowserRuntimePackStatusReport {
            manifest_pack_version: manifest.pack_version.clone(),
            runtime_root: paths.runtime_root.clone(),
            current_pack_dir: paths.current_pack_dir.clone(),
            filesystem: filesystem_report(&manifest, &paths, probe),
            doctor,
            primary_action,
            operation_plan,
            ready,
            can_run_browser_tasks: ready,
            event_names: vec!["browser.runtime.status.reported".to_string()],
        }
    }

    fn filesystem_report(
        manifest: &BrowserRuntimePackManifest,
        paths: &BrowserRuntimePackPaths,
        probe: BrowserRuntimePackProbe,
    ) -> BrowserRuntimePackFilesystemProbeReport {
        BrowserRuntimePackFilesystemProbeReport {
            snapshot: BrowserRuntimePackFilesystemSnapshot {
                current_pack_dir: paths.current_pack_dir.clone(),
                previous_pack_dir: manifest
                    .rollback_pack_version
                    .as_ref()
                    .map(|version| paths.packs_dir.join(version)),
                manifest_path: paths.manifest_path.clone(),
                manifest_status: if probe.manifest_present {
                    BrowserRuntimePackManifestLoadStatus::Loaded
                } else {
                    BrowserRuntimePackManifestLoadStatus::Missing
                },
                manifest_present: probe.manifest_present,
                node_present: probe.node_present,
                playwright_package_present: probe.playwright_package_present,
                worker_script_present: probe.playwright_package_present,
                browser_binary_present: probe.browser_binary_present,
                previous_pack_available: probe.previous_pack_available,
                versions_match: probe.versions_match,
                cache_corrupt: probe.cache_corrupt,
                active_tasks: probe.active_tasks,
                offline: probe.offline,
            },
            probe: probe.clone(),
            manifest_load: BrowserRuntimePackManifestLoadOutcome {
                status: if probe.manifest_present {
                    BrowserRuntimePackManifestLoadStatus::Loaded
                } else {
                    BrowserRuntimePackManifestLoadStatus::Missing
                },
                path: paths.manifest_path.clone(),
                manifest: Some(manifest.clone()),
                error: None,
            },
        }
    }
}
