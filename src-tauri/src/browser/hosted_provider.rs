//! Pure hosted browser provider policy contract.
//!
//! Phase 10 keeps hosted systems as opt-in escape hatches behind provider
//! policy. This module does not call vendor APIs, store credentials, launch
//! remote browsers, or execute actions.

use serde::{Deserialize, Serialize};

use crate::browser::provider::{
    BrowserCapabilityProbe, BrowserProviderCapabilities, BrowserProviderReadiness,
    BrowserProviderReadinessProbe, BrowserProviderStatus, BrowserSetupCheck,
    LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{
    browser_provider_capability_card, BrowserProviderCapabilityCard, BrowserRuntimeFeatureFlags,
};

pub const HOSTED_BROWSER_PROVIDER_ID: &str = "browser.hosted";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedProviderKind {
    Mock,
    Browserbase,
    BrowserUseCloud,
    Steel,
    Hyperbrowser,
    Custom,
}

impl BrowserHostedProviderKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Mock => "mock",
            Self::Browserbase => "browserbase",
            Self::BrowserUseCloud => "browser_use_cloud",
            Self::Steel => "steel",
            Self::Hyperbrowser => "hyperbrowser",
            Self::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedProviderUseCase {
    HostileSite,
    Isolation,
    Scaling,
    Proxy,
    CaptchaManualTakeover,
    DeploymentConstraint,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedDataBoundaryPrompt {
    Missing,
    Presented,
    AcceptedForTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedProfileStoragePolicy {
    EphemeralRemote,
    UclawManagedIsolated,
    VendorPersistent,
    UserRealProfile,
}

impl BrowserHostedProfileStoragePolicy {
    const fn is_safe_for_default_hosted_gate(self) -> bool {
        matches!(self, Self::EphemeralRemote | Self::UclawManagedIsolated)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedArtifactPolicy {
    Missing,
    FailureOnly,
    EveryAction,
}

impl BrowserHostedArtifactPolicy {
    const fn captures_artifacts(self) -> bool {
        !matches!(self, Self::Missing)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedCostPolicy {
    Missing,
    EstimateShown,
    EstimateAndCapShown,
}

impl BrowserHostedCostPolicy {
    const fn visible_to_user(self) -> bool {
        !matches!(self, Self::Missing)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostedProviderPolicy {
    pub provider_kind: BrowserHostedProviderKind,
    pub disabled_provider_ids: Vec<String>,
    pub credential_configured: bool,
    pub data_boundary_prompt: BrowserHostedDataBoundaryPrompt,
    pub profile_storage_policy: BrowserHostedProfileStoragePolicy,
    pub artifact_policy: BrowserHostedArtifactPolicy,
    pub cost_policy: BrowserHostedCostPolicy,
    pub local_fallback_provider_id: String,
    pub local_fallback_ready: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_use_case: Option<BrowserHostedProviderUseCase>,
}

impl Default for BrowserHostedProviderPolicy {
    fn default() -> Self {
        Self {
            provider_kind: BrowserHostedProviderKind::Mock,
            disabled_provider_ids: Vec::new(),
            credential_configured: false,
            data_boundary_prompt: BrowserHostedDataBoundaryPrompt::Missing,
            profile_storage_policy: BrowserHostedProfileStoragePolicy::EphemeralRemote,
            artifact_policy: BrowserHostedArtifactPolicy::Missing,
            cost_policy: BrowserHostedCostPolicy::Missing,
            local_fallback_provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            local_fallback_ready: true,
            request_use_case: None,
        }
    }
}

impl BrowserHostedProviderPolicy {
    pub fn with_disabled_provider(mut self, provider_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        if !self
            .disabled_provider_ids
            .iter()
            .any(|disabled| disabled == &provider_id)
        {
            self.disabled_provider_ids.push(provider_id);
            self.disabled_provider_ids.sort();
        }
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserHostedProviderGateStatus {
    Ready,
    FallbackToLocal,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserHostedProviderGateReport {
    pub provider_id: String,
    pub provider_kind: BrowserHostedProviderKind,
    pub status: BrowserHostedProviderGateStatus,
    pub ready: bool,
    pub local_fallback_provider_id: String,
    pub local_fallback_ready: bool,
    pub data_boundary_prompt_required: bool,
    pub artifact_capture_required: bool,
    pub cost_visibility_required: bool,
    pub blockers: Vec<String>,
    pub card_data_boundary_policy: String,
    pub card_profile_storage_policy: String,
    pub card_cost_policy: String,
    pub disable_path: String,
}

pub fn hosted_browser_provider_capabilities() -> BrowserProviderCapabilities {
    BrowserProviderCapabilities {
        provider_id: HOSTED_BROWSER_PROVIDER_ID.to_string(),
        family: "browser".to_string(),
        display_name: "Hosted Browser Provider".to_string(),
        actions: vec!["navigate", "click", "type", "screenshot", "extract", "wait"]
            .into_iter()
            .map(String::from)
            .collect(),
        features: vec![
            "opt_in_escape_hatch",
            "explicit_data_boundary",
            "remote_browser_pool",
            "manual_takeover",
            "cost_visibility",
            "local_provider_fallback",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        harness_subjects: vec![
            "browser.hosted",
            "browser.data_boundary",
            "browser.cost_visibility",
            "browser.local_fallback",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }
}

pub fn evaluate_hosted_browser_provider_gate(
    feature_flags: BrowserRuntimeFeatureFlags,
    policy: &BrowserHostedProviderPolicy,
) -> BrowserHostedProviderGateReport {
    let card = browser_provider_capability_card(HOSTED_BROWSER_PROVIDER_ID)
        .expect("hosted provider capability card must exist");
    let mut blockers = hosted_provider_card_blockers(card);

    if !feature_flags.hosted_providers {
        blockers.push("hosted_providers_feature_disabled".to_string());
    }
    if policy
        .disabled_provider_ids
        .iter()
        .any(|provider_id| provider_id == HOSTED_BROWSER_PROVIDER_ID)
    {
        blockers.push("provider_disabled".to_string());
    }
    if !policy.credential_configured {
        blockers.push("hosted_provider_credential_missing".to_string());
    }
    if policy.data_boundary_prompt != BrowserHostedDataBoundaryPrompt::AcceptedForTask {
        blockers.push("data_boundary_prompt_required".to_string());
    }
    if !policy
        .profile_storage_policy
        .is_safe_for_default_hosted_gate()
    {
        blockers.push("profile_storage_policy_unsafe".to_string());
    }
    if !policy.artifact_policy.captures_artifacts() {
        blockers.push("artifact_capture_required".to_string());
    }
    if !policy.cost_policy.visible_to_user() {
        blockers.push("cost_visibility_required".to_string());
    }
    if policy.local_fallback_provider_id.trim().is_empty() || !policy.local_fallback_ready {
        blockers.push("local_provider_fallback_unavailable".to_string());
    }
    if policy.request_use_case.is_none() {
        blockers.push("hosted_escape_hatch_reason_required".to_string());
    }

    blockers.sort();
    blockers.dedup();
    let fallback_available =
        !policy.local_fallback_provider_id.trim().is_empty() && policy.local_fallback_ready;
    let status = if blockers.is_empty() {
        BrowserHostedProviderGateStatus::Ready
    } else if fallback_available {
        BrowserHostedProviderGateStatus::FallbackToLocal
    } else {
        BrowserHostedProviderGateStatus::Blocked
    };

    BrowserHostedProviderGateReport {
        provider_id: HOSTED_BROWSER_PROVIDER_ID.to_string(),
        provider_kind: policy.provider_kind,
        ready: status == BrowserHostedProviderGateStatus::Ready,
        status,
        local_fallback_provider_id: policy.local_fallback_provider_id.clone(),
        local_fallback_ready: policy.local_fallback_ready,
        data_boundary_prompt_required: policy.data_boundary_prompt
            != BrowserHostedDataBoundaryPrompt::AcceptedForTask,
        artifact_capture_required: !policy.artifact_policy.captures_artifacts(),
        cost_visibility_required: !policy.cost_policy.visible_to_user(),
        blockers,
        card_data_boundary_policy: card.data_boundary_policy.to_string(),
        card_profile_storage_policy: card.profile_storage_policy.to_string(),
        card_cost_policy: card.cost_policy.to_string(),
        disable_path: card.disable_path.to_string(),
    }
}

pub fn hosted_browser_provider_status(
    feature_flags: BrowserRuntimeFeatureFlags,
    policy: &BrowserHostedProviderPolicy,
) -> BrowserProviderStatus {
    let report = evaluate_hosted_browser_provider_gate(feature_flags, policy);
    let setup_checks = hosted_provider_setup_checks(&report);
    let capability_probes = hosted_provider_capability_probes(&report);
    let notes = vec![
        format!("provider_kind={}", report.provider_kind.as_str()),
        format!(
            "local_fallback_provider_id={}",
            report.local_fallback_provider_id
        ),
    ];

    BrowserProviderStatus::from_probe(
        hosted_browser_provider_capabilities(),
        BrowserProviderReadinessProbe {
            provider_id: HOSTED_BROWSER_PROVIDER_ID.to_string(),
            setup_checks,
            capability_probes,
            active_contexts: 0,
            notes,
        },
    )
}

fn hosted_provider_card_blockers(card: &BrowserProviderCapabilityCard) -> Vec<String> {
    let mut blockers = Vec::new();
    if card.enabled_by_default {
        blockers.push("hosted_card_must_not_be_enabled_by_default".to_string());
    }
    if card.data_boundary_policy.trim().is_empty() {
        blockers.push("card_data_boundary_policy_missing".to_string());
    }
    if card.profile_storage_policy.trim().is_empty() {
        blockers.push("card_profile_storage_policy_missing".to_string());
    }
    if card.cost_policy.trim().is_empty() {
        blockers.push("card_cost_policy_missing".to_string());
    }
    if !card.policy_tags.contains(&"data_boundary") {
        blockers.push("card_data_boundary_tag_missing".to_string());
    }
    if !card.policy_tags.contains(&"cost_visible") {
        blockers.push("card_cost_visible_tag_missing".to_string());
    }
    blockers
}

fn hosted_provider_setup_checks(
    report: &BrowserHostedProviderGateReport,
) -> Vec<BrowserSetupCheck> {
    let mut checks = vec![if report.blockers.is_empty() {
        BrowserSetupCheck::passed("hosted_policy_gate", "Hosted provider policy gate")
    } else {
        BrowserSetupCheck::failed(
            "hosted_policy_gate",
            "Hosted provider policy gate",
            format!(
                "Resolve hosted provider policy blockers: {}.",
                report.blockers.join(", ")
            ),
        )
    }];

    checks.extend(
        [
            (
                "hosted_feature_flag",
                "Hosted provider feature flag",
                "hosted_providers_feature_disabled",
                "Enable hosted_providers before routing to a hosted browser.",
            ),
            (
                "hosted_provider_credential",
                "Hosted provider credential",
                "hosted_provider_credential_missing",
                "Configure a hosted provider credential before remote routing.",
            ),
            (
                "hosted_data_boundary",
                "Hosted provider data boundary",
                "data_boundary_prompt_required",
                "Present and accept the hosted-provider data-boundary prompt.",
            ),
            (
                "hosted_cost_visibility",
                "Hosted provider cost visibility",
                "cost_visibility_required",
                "Show hosted provider cost estimate or cap before routing.",
            ),
            (
                "hosted_local_fallback",
                "Hosted provider local fallback",
                "local_provider_fallback_unavailable",
                "Keep a local provider fallback available before hosted routing.",
            ),
        ]
        .into_iter()
        .map(|(id, label, blocker, remediation)| {
            if report.blockers.iter().any(|reason| reason == blocker) {
                BrowserSetupCheck::failed(id, label, remediation)
            } else {
                BrowserSetupCheck::passed(id, label)
            }
        }),
    );
    checks
}

fn hosted_provider_capability_probes(
    report: &BrowserHostedProviderGateReport,
) -> Vec<BrowserCapabilityProbe> {
    [
        (
            "remote_browser",
            "hosted_escape_hatch_reason_required",
            "Select a hosted-provider escape-hatch reason before remote routing.",
        ),
        (
            "artifact_capture",
            "artifact_capture_required",
            "Require hosted provider artifacts for remote browser actions.",
        ),
        (
            "profile_storage",
            "profile_storage_policy_unsafe",
            "Use ephemeral or uClaw-managed isolated hosted profile storage.",
        ),
    ]
    .into_iter()
    .map(|(action, blocker, remediation)| {
        if report.blockers.iter().any(|reason| reason == blocker) {
            BrowserCapabilityProbe::failed(action, true, remediation)
        } else {
            BrowserCapabilityProbe::passed(action, true)
        }
    })
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enabled_flags() -> BrowserRuntimeFeatureFlags {
        BrowserRuntimeFeatureFlags {
            hosted_providers: true,
            ..BrowserRuntimeFeatureFlags::safe_defaults()
        }
    }

    fn ready_policy() -> BrowserHostedProviderPolicy {
        BrowserHostedProviderPolicy {
            credential_configured: true,
            data_boundary_prompt: BrowserHostedDataBoundaryPrompt::AcceptedForTask,
            artifact_policy: BrowserHostedArtifactPolicy::EveryAction,
            cost_policy: BrowserHostedCostPolicy::EstimateAndCapShown,
            request_use_case: Some(BrowserHostedProviderUseCase::HostileSite),
            ..BrowserHostedProviderPolicy::default()
        }
    }

    #[test]
    fn hosted_gate_falls_back_to_local_when_feature_is_disabled() {
        let report = evaluate_hosted_browser_provider_gate(
            BrowserRuntimeFeatureFlags::safe_defaults(),
            &ready_policy(),
        );

        assert_eq!(
            report.status,
            BrowserHostedProviderGateStatus::FallbackToLocal
        );
        assert!(!report.ready);
        assert!(report
            .blockers
            .contains(&"hosted_providers_feature_disabled".to_string()));
        assert_eq!(
            report.local_fallback_provider_id,
            LOCAL_CHROMIUM_PROVIDER_ID
        );
    }

    #[test]
    fn hosted_gate_requires_data_boundary_artifacts_cost_and_fallback() {
        let policy = BrowserHostedProviderPolicy {
            credential_configured: true,
            local_fallback_ready: false,
            request_use_case: Some(BrowserHostedProviderUseCase::Proxy),
            ..BrowserHostedProviderPolicy::default()
        };

        let report = evaluate_hosted_browser_provider_gate(enabled_flags(), &policy);

        assert_eq!(report.status, BrowserHostedProviderGateStatus::Blocked);
        for expected in [
            "data_boundary_prompt_required",
            "artifact_capture_required",
            "cost_visibility_required",
            "local_provider_fallback_unavailable",
        ] {
            assert!(
                report.blockers.iter().any(|reason| reason == expected),
                "missing blocker {expected}: {:?}",
                report.blockers
            );
        }
        assert!(report.data_boundary_prompt_required);
        assert!(report.artifact_capture_required);
        assert!(report.cost_visibility_required);
    }

    #[test]
    fn hosted_gate_blocks_unsafe_profile_storage() {
        let policy = BrowserHostedProviderPolicy {
            profile_storage_policy: BrowserHostedProfileStoragePolicy::VendorPersistent,
            ..ready_policy()
        };

        let report = evaluate_hosted_browser_provider_gate(enabled_flags(), &policy);

        assert_eq!(
            report.status,
            BrowserHostedProviderGateStatus::FallbackToLocal
        );
        assert!(report
            .blockers
            .contains(&"profile_storage_policy_unsafe".to_string()));
    }

    #[test]
    fn hosted_gate_accepts_opt_in_mock_with_visible_cost_and_artifacts() {
        let report = evaluate_hosted_browser_provider_gate(enabled_flags(), &ready_policy());

        assert_eq!(report.status, BrowserHostedProviderGateStatus::Ready);
        assert!(report.ready);
        assert!(report.blockers.is_empty());
        assert_eq!(
            report.card_data_boundary_policy,
            "explicit_task_prompt_required"
        );
        assert_eq!(report.card_cost_policy, "estimate_and_cap_required");
    }

    #[test]
    fn hosted_status_remains_unavailable_until_policy_is_accepted() {
        let status = hosted_browser_provider_status(
            BrowserRuntimeFeatureFlags::safe_defaults(),
            &BrowserHostedProviderPolicy::default(),
        );

        assert_eq!(status.provider_id, HOSTED_BROWSER_PROVIDER_ID);
        assert_eq!(status.readiness, BrowserProviderReadiness::NeedsSetup);
        assert!(!status.ready);
        assert!(status.remediation.iter().any(|remediation| {
            remediation.contains("data-boundary prompt") || remediation.contains("cost estimate")
        }));
    }

    #[test]
    fn hosted_status_fails_closed_when_provider_is_disabled_after_policy_acceptance() {
        let policy = ready_policy().with_disabled_provider(HOSTED_BROWSER_PROVIDER_ID);
        let status = hosted_browser_provider_status(enabled_flags(), &policy);

        assert_eq!(status.provider_id, HOSTED_BROWSER_PROVIDER_ID);
        assert_eq!(status.readiness, BrowserProviderReadiness::NeedsSetup);
        assert!(!status.ready);
        assert!(status
            .remediation
            .iter()
            .any(|remediation| remediation.contains("provider_disabled")));
    }
}
