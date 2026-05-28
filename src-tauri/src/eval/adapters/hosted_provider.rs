use serde::{Deserialize, Serialize};

use crate::browser::hosted_provider::{
    evaluate_hosted_browser_provider_gate, hosted_browser_provider_status,
    BrowserHostedArtifactPolicy, BrowserHostedCostPolicy, BrowserHostedDataBoundaryPrompt,
    BrowserHostedProviderGateReport, BrowserHostedProviderGateStatus, BrowserHostedProviderPolicy,
    BrowserHostedProviderUseCase, HOSTED_BROWSER_PROVIDER_ID,
};
use crate::browser::provider::{BrowserProviderReadiness, BrowserProviderStatus};
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::eval::artifacts::{ArtifactStoreError, HarnessArtifact};
use crate::eval::runtime::EvalRuntime;

pub const BROWSER_HOSTED_PROVIDER_HARNESS_ARTIFACT_KIND: &str =
    "browser_hosted_provider_harness_matrix";

const REQUIRED_CASE_IDS: &[&str] = &[
    "browser_hosted.disabled_fallback",
    "browser_hosted.data_boundary_prompt",
    "browser_hosted.artifact_capture",
    "browser_hosted.cost_visibility",
    "browser_hosted.local_fallback",
    "browser_hosted.opt_in_ready",
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostedProviderHarnessCase {
    pub id: String,
    pub title: String,
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub policy: BrowserHostedProviderPolicy,
    pub expected_status: BrowserHostedProviderGateStatus,
    pub expected_readiness: BrowserProviderReadiness,
    pub expected_blockers: Vec<String>,
    pub expects_local_fallback: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostedProviderHarnessMatrixReport {
    pub passed: bool,
    pub required_case_ids: Vec<String>,
    pub missing_required_case_ids: Vec<String>,
    pub cases: Vec<BrowserHostedProviderHarnessCaseReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostedProviderHarnessCaseReport {
    pub case_id: String,
    pub title: String,
    pub passed: bool,
    pub gate_report: BrowserHostedProviderGateReport,
    pub provider_status: BrowserProviderStatus,
    pub missing_expected_blockers: Vec<String>,
    pub unexpected_blockers: Vec<String>,
    pub local_fallback_visible: bool,
    pub artifact_capture_visible: bool,
    pub data_boundary_prompt_visible: bool,
    pub cost_visibility_visible: bool,
}

pub fn default_hosted_provider_harness_cases() -> Vec<BrowserHostedProviderHarnessCase> {
    vec![
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.disabled_fallback".to_string(),
            title: "Hosted provider disabled flag falls back to local provider".to_string(),
            feature_flags: BrowserRuntimeFeatureFlags::safe_defaults(),
            policy: ready_hosted_policy(),
            expected_status: BrowserHostedProviderGateStatus::FallbackToLocal,
            expected_readiness: BrowserProviderReadiness::NeedsSetup,
            expected_blockers: vec!["hosted_providers_feature_disabled".to_string()],
            expects_local_fallback: true,
        },
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.data_boundary_prompt".to_string(),
            title: "Hosted provider requires accepted data-boundary prompt".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: BrowserHostedProviderPolicy {
                data_boundary_prompt: BrowserHostedDataBoundaryPrompt::Presented,
                ..ready_hosted_policy()
            },
            expected_status: BrowserHostedProviderGateStatus::FallbackToLocal,
            expected_readiness: BrowserProviderReadiness::NeedsSetup,
            expected_blockers: vec!["data_boundary_prompt_required".to_string()],
            expects_local_fallback: true,
        },
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.artifact_capture".to_string(),
            title: "Hosted provider requires action artifact capture".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: BrowserHostedProviderPolicy {
                artifact_policy: BrowserHostedArtifactPolicy::Missing,
                ..ready_hosted_policy()
            },
            expected_status: BrowserHostedProviderGateStatus::FallbackToLocal,
            expected_readiness: BrowserProviderReadiness::NeedsSetup,
            expected_blockers: vec!["artifact_capture_required".to_string()],
            expects_local_fallback: true,
        },
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.cost_visibility".to_string(),
            title: "Hosted provider requires cost visibility before routing".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: BrowserHostedProviderPolicy {
                cost_policy: BrowserHostedCostPolicy::Missing,
                ..ready_hosted_policy()
            },
            expected_status: BrowserHostedProviderGateStatus::FallbackToLocal,
            expected_readiness: BrowserProviderReadiness::NeedsSetup,
            expected_blockers: vec!["cost_visibility_required".to_string()],
            expects_local_fallback: true,
        },
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.local_fallback".to_string(),
            title: "Hosted provider blocks when no local fallback is ready".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: BrowserHostedProviderPolicy {
                local_fallback_ready: false,
                ..ready_hosted_policy()
            },
            expected_status: BrowserHostedProviderGateStatus::Blocked,
            expected_readiness: BrowserProviderReadiness::NeedsSetup,
            expected_blockers: vec!["local_provider_fallback_unavailable".to_string()],
            expects_local_fallback: false,
        },
        BrowserHostedProviderHarnessCase {
            id: "browser_hosted.opt_in_ready".to_string(),
            title: "Opt-in mock hosted provider is ready after all policy gates pass".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: ready_hosted_policy(),
            expected_status: BrowserHostedProviderGateStatus::Ready,
            expected_readiness: BrowserProviderReadiness::Ready,
            expected_blockers: Vec::new(),
            expects_local_fallback: true,
        },
    ]
}

pub fn build_hosted_provider_harness_matrix_report(
    cases: &[BrowserHostedProviderHarnessCase],
) -> BrowserHostedProviderHarnessMatrixReport {
    let case_reports = cases
        .iter()
        .map(build_hosted_provider_harness_case_report)
        .collect::<Vec<_>>();
    let missing_required_case_ids = REQUIRED_CASE_IDS
        .iter()
        .filter(|required| cases.iter().all(|case| case.id != **required))
        .map(|required| required.to_string())
        .collect::<Vec<_>>();
    let passed = missing_required_case_ids.is_empty()
        && case_reports.iter().all(|case_report| case_report.passed);

    BrowserHostedProviderHarnessMatrixReport {
        passed,
        required_case_ids: REQUIRED_CASE_IDS
            .iter()
            .map(|required| required.to_string())
            .collect(),
        missing_required_case_ids,
        cases: case_reports,
    }
}

pub fn attach_hosted_provider_harness_matrix_report(
    runtime: &EvalRuntime,
    run_id: &str,
    report: &BrowserHostedProviderHarnessMatrixReport,
) -> Result<Option<HarnessArtifact>, ArtifactStoreError> {
    let value = serde_json::to_value(report).map_err(ArtifactStoreError::Serde)?;
    runtime.attach_json_artifact(
        run_id,
        BROWSER_HOSTED_PROVIDER_HARNESS_ARTIFACT_KIND,
        &value,
    )
}

fn build_hosted_provider_harness_case_report(
    case: &BrowserHostedProviderHarnessCase,
) -> BrowserHostedProviderHarnessCaseReport {
    let gate_report = evaluate_hosted_browser_provider_gate(case.feature_flags, &case.policy);
    let provider_status = hosted_browser_provider_status(case.feature_flags, &case.policy);
    let missing_expected_blockers = case
        .expected_blockers
        .iter()
        .filter(|expected| !gate_report.blockers.contains(expected))
        .cloned()
        .collect::<Vec<_>>();
    let unexpected_blockers = gate_report
        .blockers
        .iter()
        .filter(|blocker| !case.expected_blockers.contains(blocker))
        .cloned()
        .collect::<Vec<_>>();
    let local_fallback_visible = gate_report.local_fallback_ready
        && !gate_report.local_fallback_provider_id.trim().is_empty()
        && gate_report.local_fallback_provider_id != HOSTED_BROWSER_PROVIDER_ID;
    let artifact_capture_visible = !gate_report.artifact_capture_required;
    let data_boundary_prompt_visible = !gate_report.data_boundary_prompt_required;
    let cost_visibility_visible = !gate_report.cost_visibility_required;
    let passed = gate_report.status == case.expected_status
        && provider_status.readiness == case.expected_readiness
        && missing_expected_blockers.is_empty()
        && unexpected_blockers.is_empty()
        && local_fallback_visible == case.expects_local_fallback
        && case.expected_blockers.iter().all(|expected| {
            provider_status
                .remediation
                .iter()
                .any(|item| item.contains(expected))
        });

    BrowserHostedProviderHarnessCaseReport {
        case_id: case.id.clone(),
        title: case.title.clone(),
        passed,
        gate_report,
        provider_status,
        missing_expected_blockers,
        unexpected_blockers,
        local_fallback_visible,
        artifact_capture_visible,
        data_boundary_prompt_visible,
        cost_visibility_visible,
    }
}

fn hosted_enabled_flags() -> BrowserRuntimeFeatureFlags {
    BrowserRuntimeFeatureFlags {
        hosted_providers: true,
        ..BrowserRuntimeFeatureFlags::safe_defaults()
    }
}

fn ready_hosted_policy() -> BrowserHostedProviderPolicy {
    BrowserHostedProviderPolicy {
        credential_configured: true,
        data_boundary_prompt: BrowserHostedDataBoundaryPrompt::AcceptedForTask,
        artifact_policy: BrowserHostedArtifactPolicy::EveryAction,
        cost_policy: BrowserHostedCostPolicy::EstimateAndCapShown,
        request_use_case: Some(BrowserHostedProviderUseCase::HostileSite),
        ..BrowserHostedProviderPolicy::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
    use serde_json::Value;

    #[test]
    fn default_matrix_covers_all_phase10_gate_cases() {
        let report =
            build_hosted_provider_harness_matrix_report(&default_hosted_provider_harness_cases());

        assert!(report.passed, "{report:#?}");
        assert!(report.missing_required_case_ids.is_empty());
        for required in REQUIRED_CASE_IDS {
            assert!(report
                .cases
                .iter()
                .any(|case_report| case_report.case_id == *required));
        }
    }

    #[test]
    fn disabled_fallback_case_keeps_hosted_provider_unready_with_local_fallback() {
        let case = default_hosted_provider_harness_cases()
            .into_iter()
            .find(|case| case.id == "browser_hosted.disabled_fallback")
            .expect("disabled fallback case");

        let report = build_hosted_provider_harness_case_report(&case);

        assert!(report.passed, "{report:#?}");
        assert_eq!(
            report.gate_report.status,
            BrowserHostedProviderGateStatus::FallbackToLocal
        );
        assert!(report
            .gate_report
            .blockers
            .contains(&"hosted_providers_feature_disabled".to_string()));
        assert!(report.local_fallback_visible);
        assert!(!report.provider_status.ready);
    }

    #[test]
    fn missing_required_case_fails_matrix() {
        let cases = vec![BrowserHostedProviderHarnessCase {
            id: "browser_hosted.opt_in_ready".to_string(),
            title: "Only ready case".to_string(),
            feature_flags: hosted_enabled_flags(),
            policy: ready_hosted_policy(),
            expected_status: BrowserHostedProviderGateStatus::Ready,
            expected_readiness: BrowserProviderReadiness::Ready,
            expected_blockers: Vec::new(),
            expects_local_fallback: true,
        }];

        let report = build_hosted_provider_harness_matrix_report(&cases);

        assert!(!report.passed);
        assert!(report
            .missing_required_case_ids
            .contains(&"browser_hosted.data_boundary_prompt".to_string()));
    }

    #[test]
    fn attach_matrix_report_writes_harness_artifact() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = EvalRuntime::new(temp.path());
        let case = HarnessCase {
            id: "browser-hosted-provider-matrix".to_string(),
            subject: HarnessSubject::Browser,
            title: "Hosted provider harness matrix".to_string(),
            prompt: "Build hosted provider matrix".to_string(),
            setup: Vec::new(),
            policy: HarnessPolicy::default(),
            budgets: HarnessBudget::default(),
            assertions: Vec::new(),
            graders: Vec::new(),
        };
        let episode = runtime.start_episode(&case);
        let report =
            build_hosted_provider_harness_matrix_report(&default_hosted_provider_harness_cases());

        let artifact =
            attach_hosted_provider_harness_matrix_report(&runtime, &episode.run_id, &report)
                .expect("write artifact")
                .expect("attached artifact");

        assert_eq!(artifact.kind, BROWSER_HOSTED_PROVIDER_HARNESS_ARTIFACT_KIND);
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&artifact.path).unwrap()).unwrap();
        assert_eq!(value["passed"], true);
        assert_eq!(
            value["cases"].as_array().unwrap().len(),
            REQUIRED_CASE_IDS.len()
        );
        assert_eq!(
            value["cases"][0]["gateReport"]["providerId"],
            HOSTED_BROWSER_PROVIDER_ID
        );
    }
}
