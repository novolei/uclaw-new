//! Playwright CLI provider contract shell.
//!
//! This module is intentionally pure. It defines the feature-flagged provider
//! readiness shape and JSON request envelope for future short-lived Playwright
//! child workers, but it does not spawn Node, launch Playwright, mutate runtime
//! packs, or execute browser actions.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaywrightCliEnvelopeError {
    RuntimeNotReady,
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

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::runtime_pack::{
        diagnose_runtime_pack, plan_runtime_pack_operation, BrowserRuntimePackAction,
        BrowserRuntimePackFilesystemProbeReport, BrowserRuntimePackFilesystemSnapshot,
        BrowserRuntimePackManifest, BrowserRuntimePackManifestLoadOutcome,
        BrowserRuntimePackManifestLoadStatus, BrowserRuntimePackNetworkState,
        BrowserRuntimePackOperation, BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths,
        BrowserRuntimePackPlanTrigger, BrowserRuntimePackProbe, BrowserRuntimePackStatusReport,
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

    fn ready_runtime_report() -> BrowserRuntimePackStatusReport {
        runtime_report_from_probe(BrowserRuntimePackProbe::ready())
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
