use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::browser::provider::{
    decide_browser_provider_route, BrowserCapabilityProbe, BrowserProviderCapabilities,
    BrowserProviderReadinessProbe, BrowserProviderRouteDecision,
    BrowserProviderRouteDecisionStatus, BrowserProviderRouteRequest, BrowserProviderStatus,
    BrowserSetupCheck, LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{
    browser_provider_capability_card, browser_provider_capability_cards,
    BrowserProviderCapabilityCard, BrowserProviderSelectionRequest,
};
use crate::eval::artifacts::{ArtifactStoreError, HarnessArtifact};
use crate::eval::runtime::EvalRuntime;

pub const BROWSER_PROVIDER_PARITY_MATRIX_ARTIFACT_KIND: &str = "browser_provider_parity_matrix";
pub const MOCK_HOSTED_PROVIDER_ID: &str = "browser.hosted";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderParityCase {
    pub id: String,
    pub title: String,
    pub selection: BrowserProviderSelectionRequest,
    pub expected_provider_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderParityMatrixReport {
    pub passed: bool,
    pub cases: Vec<BrowserProviderParityCaseReport>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderParityCaseReport {
    pub case_id: String,
    pub title: String,
    pub selection: BrowserProviderSelectionRequest,
    pub passed: bool,
    pub missing_provider_ids: Vec<String>,
    pub provider_results: Vec<BrowserProviderParityProviderResult>,
    pub fallback_result: BrowserProviderParityFallbackResult,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderParityProviderResult {
    pub provider_id: String,
    pub selected: bool,
    pub route_status: BrowserProviderRouteDecisionStatus,
    pub artifact_policy: String,
    pub artifact_visible: bool,
    pub promotion_eligible: bool,
    pub route_decision: BrowserProviderRouteDecision,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderParityFallbackResult {
    pub disabled_provider_id: String,
    pub selected_provider_id: Option<String>,
    pub route_status: BrowserProviderRouteDecisionStatus,
    pub artifact_visible: bool,
    pub route_decision: BrowserProviderRouteDecision,
}

pub fn default_browser_provider_parity_cases() -> Vec<BrowserProviderParityCase> {
    let shared_provider_ids = vec![
        LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
        crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        MOCK_HOSTED_PROVIDER_ID.to_string(),
    ];

    vec![
        BrowserProviderParityCase {
            id: "browser_provider.parity.navigate".to_string(),
            title: "Shared navigate action routes across browser providers".to_string(),
            selection: BrowserProviderSelectionRequest {
                action: Some("navigate".to_string()),
                observation_mode: None,
                requires_mcp_specific_capability: false,
            },
            expected_provider_ids: shared_provider_ids.clone(),
        },
        BrowserProviderParityCase {
            id: "browser_provider.parity.click".to_string(),
            title: "Shared click action routes across browser providers".to_string(),
            selection: BrowserProviderSelectionRequest {
                action: Some("click".to_string()),
                observation_mode: None,
                requires_mcp_specific_capability: false,
            },
            expected_provider_ids: shared_provider_ids,
        },
    ]
}

pub fn build_browser_provider_parity_matrix_report(
    cases: &[BrowserProviderParityCase],
) -> BrowserProviderParityMatrixReport {
    let reports = cases
        .iter()
        .map(build_browser_provider_parity_case_report)
        .collect::<Vec<_>>();
    BrowserProviderParityMatrixReport {
        passed: reports.iter().all(|report| report.passed),
        cases: reports,
    }
}

pub fn attach_browser_provider_parity_matrix_report(
    runtime: &EvalRuntime,
    run_id: &str,
    report: &BrowserProviderParityMatrixReport,
) -> Result<Option<HarnessArtifact>, ArtifactStoreError> {
    let value = serde_json::to_value(report).map_err(ArtifactStoreError::Serde)?;
    runtime.attach_json_artifact(run_id, BROWSER_PROVIDER_PARITY_MATRIX_ARTIFACT_KIND, &value)
}

fn build_browser_provider_parity_case_report(
    case: &BrowserProviderParityCase,
) -> BrowserProviderParityCaseReport {
    let statuses = ready_statuses_for_cards();
    let provider_results = case
        .expected_provider_ids
        .iter()
        .filter_map(|provider_id| {
            browser_provider_capability_card(provider_id)
                .map(|card| route_for_forced_provider(case, card, &statuses))
        })
        .collect::<Vec<_>>();
    let missing_provider_ids = case
        .expected_provider_ids
        .iter()
        .filter(|provider_id| browser_provider_capability_card(provider_id).is_none())
        .cloned()
        .collect::<Vec<_>>();
    let fallback_result = route_fallback_for_case(case, &statuses);
    let passed = missing_provider_ids.is_empty()
        && provider_results.len() == case.expected_provider_ids.len()
        && provider_results
            .iter()
            .all(|result| result.selected && result.artifact_visible && !result.promotion_eligible)
        && fallback_result.selected_provider_id.is_some()
        && fallback_result.route_status == BrowserProviderRouteDecisionStatus::RolledBack
        && fallback_result.artifact_visible;

    BrowserProviderParityCaseReport {
        case_id: case.id.clone(),
        title: case.title.clone(),
        selection: case.selection.clone(),
        passed,
        missing_provider_ids,
        provider_results,
        fallback_result,
    }
}

fn route_for_forced_provider(
    case: &BrowserProviderParityCase,
    target: &BrowserProviderCapabilityCard,
    statuses: &[BrowserProviderStatus],
) -> BrowserProviderParityProviderResult {
    let disabled_provider_ids = case
        .expected_provider_ids
        .iter()
        .filter(|provider_id| provider_id.as_str() != target.provider_id)
        .cloned()
        .collect::<Vec<_>>();
    let decision = decide_browser_provider_route(
        &BrowserProviderRouteRequest {
            selection: case.selection.clone(),
            disabled_provider_ids,
            previous_provider_id: None,
        },
        statuses,
    );
    let selected = decision.selected_provider_id.as_deref() == Some(target.provider_id);

    BrowserProviderParityProviderResult {
        provider_id: target.provider_id.to_string(),
        selected,
        route_status: decision.status,
        artifact_policy: target.artifact_policy.to_string(),
        artifact_visible: artifact_policy_is_visible(target.artifact_policy),
        promotion_eligible: target.harness_score.promotion_eligible
            && target.provider_id != LOCAL_CHROMIUM_PROVIDER_ID,
        route_decision: decision,
    }
}

fn route_fallback_for_case(
    case: &BrowserProviderParityCase,
    statuses: &[BrowserProviderStatus],
) -> BrowserProviderParityFallbackResult {
    let disabled_provider_id = case
        .expected_provider_ids
        .first()
        .cloned()
        .unwrap_or_else(|| LOCAL_CHROMIUM_PROVIDER_ID.to_string());
    let decision = decide_browser_provider_route(
        &BrowserProviderRouteRequest {
            selection: case.selection.clone(),
            disabled_provider_ids: vec![disabled_provider_id.clone()],
            previous_provider_id: Some(disabled_provider_id.clone()),
        },
        statuses,
    );
    let artifact_visible = decision
        .selected_provider_id
        .as_deref()
        .and_then(browser_provider_capability_card)
        .is_some_and(|card| artifact_policy_is_visible(card.artifact_policy));

    BrowserProviderParityFallbackResult {
        disabled_provider_id,
        selected_provider_id: decision.selected_provider_id.clone(),
        route_status: decision.status,
        artifact_visible,
        route_decision: decision,
    }
}

fn ready_statuses_for_cards() -> Vec<BrowserProviderStatus> {
    browser_provider_capability_cards()
        .iter()
        .map(|card| {
            BrowserProviderStatus::from_probe(
                provider_capabilities_from_card(card),
                BrowserProviderReadinessProbe {
                    provider_id: card.provider_id.to_string(),
                    setup_checks: vec![BrowserSetupCheck::passed(
                        "provider_parity_matrix",
                        "Provider parity matrix harness fixture",
                    )],
                    capability_probes: card
                        .supported_actions
                        .iter()
                        .map(|action| BrowserCapabilityProbe::passed(*action, true))
                        .collect(),
                    active_contexts: 0,
                    notes: vec!["model_free_provider_parity_matrix".to_string()],
                },
            )
        })
        .collect()
}

fn provider_capabilities_from_card(
    card: &BrowserProviderCapabilityCard,
) -> BrowserProviderCapabilities {
    BrowserProviderCapabilities {
        provider_id: card.provider_id.to_string(),
        family: "browser".to_string(),
        display_name: card.display_name.to_string(),
        actions: card
            .supported_actions
            .iter()
            .map(|action| action.to_string())
            .collect(),
        features: card.policy_tags.iter().map(|tag| tag.to_string()).collect(),
        harness_subjects: card
            .harness_subjects
            .iter()
            .map(|subject| subject.to_string())
            .collect(),
    }
}

fn artifact_policy_is_visible(policy: &str) -> bool {
    !policy.trim().is_empty() && policy != "none"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};

    #[test]
    fn default_matrix_routes_shared_cases_across_all_phase8_provider_lanes() {
        let cases = default_browser_provider_parity_cases();

        let report = build_browser_provider_parity_matrix_report(&cases);

        assert!(report.passed, "{report:#?}");
        for case in &report.cases {
            let provider_ids = case
                .provider_results
                .iter()
                .map(|result| result.provider_id.as_str())
                .collect::<Vec<_>>();
            assert!(provider_ids.contains(&LOCAL_CHROMIUM_PROVIDER_ID));
            assert!(provider_ids.contains(&crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID));
            assert!(provider_ids.contains(&crate::browser::PLAYWRIGHT_MCP_PROVIDER_ID));
            assert!(provider_ids.contains(&MOCK_HOSTED_PROVIDER_ID));
            assert!(case.provider_results.iter().all(|result| {
                result.selected
                    && result.route_status == BrowserProviderRouteDecisionStatus::Selected
                    && result.artifact_visible
                    && !result.promotion_eligible
            }));
        }
    }

    #[test]
    fn disabling_local_provider_falls_back_with_artifact_policy_visible() {
        let case = default_browser_provider_parity_cases()
            .into_iter()
            .find(|case| case.id == "browser_provider.parity.navigate")
            .expect("navigate case");

        let report = build_browser_provider_parity_case_report(&case);

        assert_eq!(
            report.fallback_result.disabled_provider_id,
            LOCAL_CHROMIUM_PROVIDER_ID
        );
        assert_eq!(
            report.fallback_result.route_status,
            BrowserProviderRouteDecisionStatus::RolledBack
        );
        assert_eq!(
            report.fallback_result.selected_provider_id.as_deref(),
            Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
        );
        assert!(report.fallback_result.artifact_visible);
        assert!(report
            .fallback_result
            .route_decision
            .event_intents
            .iter()
            .any(|intent| intent.reason == "fallback_provider_selected"));
    }

    #[test]
    fn attach_matrix_report_writes_harness_artifact() {
        let temp = tempfile::tempdir().expect("tempdir");
        let runtime = EvalRuntime::new(temp.path());
        let case = HarnessCase {
            id: "browser-provider-parity".to_string(),
            subject: HarnessSubject::Browser,
            title: "Provider parity matrix".to_string(),
            prompt: "Build provider parity matrix".to_string(),
            setup: Vec::new(),
            policy: HarnessPolicy::default(),
            budgets: HarnessBudget::default(),
            assertions: Vec::new(),
            graders: Vec::new(),
        };
        let episode = runtime.start_episode(&case);
        let report =
            build_browser_provider_parity_matrix_report(&default_browser_provider_parity_cases());

        let artifact =
            attach_browser_provider_parity_matrix_report(&runtime, &episode.run_id, &report)
                .expect("write artifact")
                .expect("attached artifact");

        assert_eq!(artifact.kind, BROWSER_PROVIDER_PARITY_MATRIX_ARTIFACT_KIND);
        let value: Value =
            serde_json::from_str(&std::fs::read_to_string(&artifact.path).unwrap()).unwrap();
        assert_eq!(value["passed"], true);
        assert_eq!(
            value["cases"][0]["providerResults"]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }

    #[test]
    fn missing_expected_provider_status_fails_case_report() {
        let mut case = default_browser_provider_parity_cases()
            .into_iter()
            .next()
            .expect("case");
        case.expected_provider_ids
            .push("browser.missing".to_string());

        let report = build_browser_provider_parity_case_report(&case);

        assert!(!report.passed);
        assert_eq!(report.missing_provider_ids, vec!["browser.missing"]);
        assert_eq!(report.provider_results.len(), 4);
    }
}
