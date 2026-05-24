//! Data-driven browser provider default selection policy.
//!
//! This module is intentionally pure. It does not mutate settings, launch
//! providers, or change live route ranking; it records the reversible decision
//! contract required before any provider can become the default.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;
use crate::browser::runtime_contracts::{
    browser_provider_capability_cards, BrowserProviderCapabilityCard, BrowserProviderLane,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderDefaultSelectionPolicy {
    pub current_default_provider_id: String,
    pub disabled_provider_ids: Vec<String>,
    pub allow_hosted_default: bool,
    pub min_fixture_cases: u16,
    pub min_success_basis_points: u16,
    pub require_artifact_visibility: bool,
    pub require_policy_boundary_metric: bool,
    pub require_local_first_metric: bool,
}

impl Default for BrowserProviderDefaultSelectionPolicy {
    fn default() -> Self {
        Self {
            current_default_provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            disabled_provider_ids: Vec::new(),
            allow_hosted_default: false,
            min_fixture_cases: 1,
            min_success_basis_points: 10_000,
            require_artifact_visibility: true,
            require_policy_boundary_metric: true,
            require_local_first_metric: true,
        }
    }
}

impl BrowserProviderDefaultSelectionPolicy {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderDefaultEvidence {
    pub provider_id: String,
    pub source: String,
    pub fixture_cases_total: u16,
    pub fixture_cases_passed: u16,
    pub parity_passed: bool,
    pub artifact_visible: bool,
    pub fallback_artifact_visible: bool,
    pub policy_boundary_preserved: bool,
    pub local_first_preserved: bool,
}

impl BrowserProviderDefaultEvidence {
    pub fn from_card(card: &BrowserProviderCapabilityCard) -> Self {
        let metrics = card
            .harness_score
            .tracked_metrics
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        Self {
            provider_id: card.provider_id.to_string(),
            source: card.harness_score.source.to_string(),
            fixture_cases_total: card.harness_score.fixture_cases_total,
            fixture_cases_passed: card.harness_score.fixture_cases_passed,
            parity_passed: card.harness_score.fixture_cases_total > 0
                && card.harness_score.fixture_cases_passed
                    == card.harness_score.fixture_cases_total,
            artifact_visible: artifact_policy_is_visible(card.artifact_policy),
            fallback_artifact_visible: artifact_policy_is_visible(card.artifact_policy),
            policy_boundary_preserved: metrics.contains("policy_boundary"),
            local_first_preserved: card.lane != BrowserProviderLane::Hosted
                && (metrics.contains("local_first") || card.policy_tags.contains(&"local_first")),
        }
    }

    fn success_basis_points(&self) -> u16 {
        if self.fixture_cases_total == 0 {
            return 0;
        }
        ((u32::from(self.fixture_cases_passed) * 10_000) / u32::from(self.fixture_cases_total))
            as u16
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderDefaultDecisionStatus {
    RetainedCurrent,
    Promoted,
    FallbackSelected,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderDefaultCandidate {
    pub provider_id: String,
    pub current_default: bool,
    pub promotion_eligible: bool,
    pub fallback_eligible: bool,
    pub eligible: bool,
    pub reliability_basis_points: u16,
    pub fixture_cases_total: u16,
    pub artifact_visible: bool,
    pub fallback_artifact_visible: bool,
    pub policy_boundary_preserved: bool,
    pub local_first_preserved: bool,
    pub blocked_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderDefaultDecision {
    pub status: BrowserProviderDefaultDecisionStatus,
    pub selected_provider_id: Option<String>,
    pub previous_default_provider_id: String,
    pub rollback_provider_id: Option<String>,
    pub reasons: Vec<String>,
    pub candidates: Vec<BrowserProviderDefaultCandidate>,
}

pub fn default_browser_provider_default_evidence() -> Vec<BrowserProviderDefaultEvidence> {
    browser_provider_capability_cards()
        .iter()
        .map(BrowserProviderDefaultEvidence::from_card)
        .collect()
}

pub fn decide_browser_provider_default(
    policy: &BrowserProviderDefaultSelectionPolicy,
    evidence: &[BrowserProviderDefaultEvidence],
) -> BrowserProviderDefaultDecision {
    decide_browser_provider_default_from_cards(
        policy,
        browser_provider_capability_cards(),
        evidence,
    )
}

pub fn decide_browser_provider_default_from_cards(
    policy: &BrowserProviderDefaultSelectionPolicy,
    cards: &[BrowserProviderCapabilityCard],
    evidence: &[BrowserProviderDefaultEvidence],
) -> BrowserProviderDefaultDecision {
    let evidence_by_provider = evidence
        .iter()
        .map(|item| (item.provider_id.as_str(), item))
        .collect::<BTreeMap<_, _>>();
    let candidates = cards
        .iter()
        .map(|card| {
            build_default_candidate(
                policy,
                card,
                evidence_by_provider.get(card.provider_id).copied(),
            )
        })
        .collect::<Vec<_>>();

    let current = candidates
        .iter()
        .find(|candidate| candidate.current_default);
    let current_disabled = policy
        .disabled_provider_ids
        .iter()
        .any(|provider_id| provider_id == &policy.current_default_provider_id);
    let current_score = current
        .map(|candidate| candidate.reliability_basis_points)
        .unwrap_or_default();

    if current.is_none() || current_disabled {
        if let Some(fallback) = best_fallback_candidate(&candidates) {
            return BrowserProviderDefaultDecision {
                status: BrowserProviderDefaultDecisionStatus::FallbackSelected,
                selected_provider_id: Some(fallback.provider_id.clone()),
                previous_default_provider_id: policy.current_default_provider_id.clone(),
                rollback_provider_id: Some(policy.current_default_provider_id.clone()),
                reasons: vec![
                    "current_default_unavailable_or_disabled".to_string(),
                    "fallback_keeps_artifact_visibility".to_string(),
                ],
                candidates,
            };
        }

        return BrowserProviderDefaultDecision {
            status: BrowserProviderDefaultDecisionStatus::Blocked,
            selected_provider_id: None,
            previous_default_provider_id: policy.current_default_provider_id.clone(),
            rollback_provider_id: Some(policy.current_default_provider_id.clone()),
            reasons: vec!["no_provider_satisfies_default_policy".to_string()],
            candidates,
        };
    }

    if let Some(promotion) = best_promotion_candidate(&candidates, current_score) {
        return BrowserProviderDefaultDecision {
            status: BrowserProviderDefaultDecisionStatus::Promoted,
            selected_provider_id: Some(promotion.provider_id.clone()),
            previous_default_provider_id: policy.current_default_provider_id.clone(),
            rollback_provider_id: Some(policy.current_default_provider_id.clone()),
            reasons: vec![
                "promotion_candidate_beats_current_default".to_string(),
                "rollback_provider_recorded".to_string(),
            ],
            candidates,
        };
    }

    if let Some(current) = current.filter(|candidate| candidate.eligible && !current_disabled) {
        return BrowserProviderDefaultDecision {
            status: BrowserProviderDefaultDecisionStatus::RetainedCurrent,
            selected_provider_id: Some(current.provider_id.clone()),
            previous_default_provider_id: policy.current_default_provider_id.clone(),
            rollback_provider_id: None,
            reasons: vec!["current_default_remains_best_evidence".to_string()],
            candidates,
        };
    }

    if let Some(fallback) = best_fallback_candidate(&candidates) {
        return BrowserProviderDefaultDecision {
            status: BrowserProviderDefaultDecisionStatus::FallbackSelected,
            selected_provider_id: Some(fallback.provider_id.clone()),
            previous_default_provider_id: policy.current_default_provider_id.clone(),
            rollback_provider_id: Some(policy.current_default_provider_id.clone()),
            reasons: vec![
                "current_default_unavailable_or_disabled".to_string(),
                "fallback_keeps_artifact_visibility".to_string(),
            ],
            candidates,
        };
    }

    BrowserProviderDefaultDecision {
        status: BrowserProviderDefaultDecisionStatus::Blocked,
        selected_provider_id: None,
        previous_default_provider_id: policy.current_default_provider_id.clone(),
        rollback_provider_id: Some(policy.current_default_provider_id.clone()),
        reasons: vec!["no_provider_satisfies_default_policy".to_string()],
        candidates,
    }
}

fn build_default_candidate(
    policy: &BrowserProviderDefaultSelectionPolicy,
    card: &BrowserProviderCapabilityCard,
    evidence: Option<&BrowserProviderDefaultEvidence>,
) -> BrowserProviderDefaultCandidate {
    let mut blocked_reasons = Vec::new();
    let current_default = card.provider_id == policy.current_default_provider_id;
    let disabled = policy
        .disabled_provider_ids
        .iter()
        .any(|provider_id| provider_id == card.provider_id);
    if disabled {
        blocked_reasons.push("provider_disabled".to_string());
    }
    if card.lane == BrowserProviderLane::Hosted && !policy.allow_hosted_default {
        blocked_reasons.push("hosted_default_not_allowed".to_string());
    }

    let Some(evidence) = evidence else {
        blocked_reasons.push("missing_default_evidence".to_string());
        return BrowserProviderDefaultCandidate {
            provider_id: card.provider_id.to_string(),
            current_default,
            promotion_eligible: false,
            fallback_eligible: false,
            eligible: false,
            reliability_basis_points: 0,
            fixture_cases_total: 0,
            artifact_visible: false,
            fallback_artifact_visible: false,
            policy_boundary_preserved: false,
            local_first_preserved: false,
            blocked_reasons,
        };
    };

    if !evidence.parity_passed {
        blocked_reasons.push("parity_not_passed".to_string());
    }
    if evidence.fixture_cases_total < policy.min_fixture_cases {
        blocked_reasons.push("insufficient_fixture_cases".to_string());
    }
    if evidence.success_basis_points() < policy.min_success_basis_points {
        blocked_reasons.push("insufficient_reliability".to_string());
    }
    if policy.require_artifact_visibility && !evidence.artifact_visible {
        blocked_reasons.push("artifact_visibility_missing".to_string());
    }
    if policy.require_policy_boundary_metric && !evidence.policy_boundary_preserved {
        blocked_reasons.push("policy_boundary_metric_missing".to_string());
    }
    if policy.require_local_first_metric && !evidence.local_first_preserved {
        blocked_reasons.push("local_first_metric_missing".to_string());
    }

    let baseline_eligible = blocked_reasons.is_empty();
    let promotion_eligible =
        baseline_eligible && card.harness_score.promotion_eligible && !current_default;
    let fallback_eligible = !disabled
        && evidence.parity_passed
        && evidence.fallback_artifact_visible
        && evidence.success_basis_points() >= policy.min_success_basis_points
        && (!policy.require_policy_boundary_metric || evidence.policy_boundary_preserved)
        && (!policy.require_local_first_metric || evidence.local_first_preserved)
        && (card.lane != BrowserProviderLane::Hosted || policy.allow_hosted_default);

    BrowserProviderDefaultCandidate {
        provider_id: card.provider_id.to_string(),
        current_default,
        promotion_eligible,
        fallback_eligible,
        eligible: baseline_eligible && (current_default || promotion_eligible),
        reliability_basis_points: evidence.success_basis_points(),
        fixture_cases_total: evidence.fixture_cases_total,
        artifact_visible: evidence.artifact_visible,
        fallback_artifact_visible: evidence.fallback_artifact_visible,
        policy_boundary_preserved: evidence.policy_boundary_preserved,
        local_first_preserved: evidence.local_first_preserved,
        blocked_reasons,
    }
}

fn best_promotion_candidate(
    candidates: &[BrowserProviderDefaultCandidate],
    current_score: u16,
) -> Option<&BrowserProviderDefaultCandidate> {
    candidates
        .iter()
        .filter(|candidate| {
            candidate.promotion_eligible && candidate.reliability_basis_points > current_score
        })
        .max_by_key(|candidate| {
            (
                candidate.reliability_basis_points,
                candidate.fixture_cases_total,
                std::cmp::Reverse(candidate.provider_id.as_str()),
            )
        })
}

fn best_fallback_candidate(
    candidates: &[BrowserProviderDefaultCandidate],
) -> Option<&BrowserProviderDefaultCandidate> {
    candidates
        .iter()
        .filter(|candidate| !candidate.current_default && candidate.fallback_eligible)
        .max_by_key(|candidate| {
            (
                candidate.reliability_basis_points,
                candidate.fixture_cases_total,
                std::cmp::Reverse(candidate.provider_id.as_str()),
            )
        })
}

fn artifact_policy_is_visible(policy: &str) -> bool {
    !policy.trim().is_empty() && policy != "none"
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::runtime_contracts::{
        BrowserProviderCapabilityCard, BrowserProviderHarnessScore,
    };

    #[test]
    fn default_policy_retains_local_chromium_with_current_cards() {
        let decision = decide_browser_provider_default(
            &BrowserProviderDefaultSelectionPolicy::default(),
            &default_browser_provider_default_evidence(),
        );

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::RetainedCurrent
        );
        assert_eq!(
            decision.selected_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
        assert!(decision.rollback_provider_id.is_none());
        assert!(decision.candidates.iter().any(|candidate| {
            candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
                && !candidate.promotion_eligible
        }));
    }

    #[test]
    fn promotion_requires_better_reliability_and_records_rollback() {
        let cards = vec![
            test_card(
                LOCAL_CHROMIUM_PROVIDER_ID,
                BrowserProviderLane::LocalChromium,
                3,
                2,
                true,
                &["policy_boundary", "local_first"],
            ),
            test_card(
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID,
                BrowserProviderLane::PlaywrightCli,
                6,
                6,
                true,
                &["policy_boundary", "local_first"],
            ),
        ];
        let evidence = cards
            .iter()
            .map(BrowserProviderDefaultEvidence::from_card)
            .collect::<Vec<_>>();

        let decision = decide_browser_provider_default_from_cards(
            &BrowserProviderDefaultSelectionPolicy {
                min_success_basis_points: 6_000,
                ..BrowserProviderDefaultSelectionPolicy::default()
            },
            &cards,
            &evidence,
        );

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::Promoted
        );
        assert_eq!(
            decision.selected_provider_id.as_deref(),
            Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
        );
        assert_eq!(
            decision.rollback_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
    }

    #[test]
    fn hosted_provider_cannot_become_default_without_explicit_policy() {
        let cards = vec![
            test_card(
                LOCAL_CHROMIUM_PROVIDER_ID,
                BrowserProviderLane::LocalChromium,
                3,
                3,
                true,
                &["policy_boundary", "local_first"],
            ),
            test_card(
                "browser.hosted",
                BrowserProviderLane::Hosted,
                10,
                10,
                true,
                &["policy_boundary"],
            ),
        ];
        let evidence = cards
            .iter()
            .map(BrowserProviderDefaultEvidence::from_card)
            .collect::<Vec<_>>();

        let decision = decide_browser_provider_default_from_cards(
            &BrowserProviderDefaultSelectionPolicy {
                ..BrowserProviderDefaultSelectionPolicy::default()
            },
            &cards,
            &evidence,
        );

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::RetainedCurrent
        );
        assert_eq!(
            decision.selected_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
        assert!(decision.candidates.iter().any(|candidate| {
            candidate.provider_id == "browser.hosted"
                && candidate
                    .blocked_reasons
                    .iter()
                    .any(|reason| reason == "hosted_default_not_allowed")
        }));
    }

    #[test]
    fn disabled_current_default_selects_artifact_visible_fallback_without_promotion() {
        let policy = BrowserProviderDefaultSelectionPolicy::default()
            .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);

        let decision =
            decide_browser_provider_default(&policy, &default_browser_provider_default_evidence());

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::FallbackSelected
        );
        assert_eq!(
            decision.selected_provider_id.as_deref(),
            Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
        );
        assert_eq!(
            decision.rollback_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
    }

    #[test]
    fn disabled_current_default_does_not_promote_even_when_fallback_is_promotion_eligible() {
        let policy = BrowserProviderDefaultSelectionPolicy::default()
            .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
        let cards = vec![
            test_card(
                LOCAL_CHROMIUM_PROVIDER_ID,
                BrowserProviderLane::LocalChromium,
                1,
                1,
                false,
                &["policy_boundary", "local_first"],
            ),
            test_card(
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID,
                BrowserProviderLane::PlaywrightCli,
                3,
                3,
                true,
                &["policy_boundary", "local_first"],
            ),
        ];
        let evidence = cards
            .iter()
            .map(BrowserProviderDefaultEvidence::from_card)
            .collect::<Vec<_>>();

        let decision = decide_browser_provider_default_from_cards(&policy, &cards, &evidence);

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::FallbackSelected
        );
        assert_eq!(
            decision.selected_provider_id.as_deref(),
            Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
        );
        assert_eq!(
            decision.rollback_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
        assert!(decision.candidates.iter().any(|candidate| {
            candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
                && candidate.promotion_eligible
                && candidate.fallback_eligible
        }));
    }

    #[test]
    fn disabled_current_default_blocks_when_promotion_candidate_is_not_valid_fallback() {
        let policy = BrowserProviderDefaultSelectionPolicy::default()
            .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
        let cards = vec![
            test_card(
                LOCAL_CHROMIUM_PROVIDER_ID,
                BrowserProviderLane::LocalChromium,
                1,
                1,
                false,
                &["policy_boundary", "local_first"],
            ),
            test_card(
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID,
                BrowserProviderLane::PlaywrightCli,
                3,
                3,
                true,
                &["policy_boundary", "local_first"],
            ),
        ];
        let mut evidence = cards
            .iter()
            .map(BrowserProviderDefaultEvidence::from_card)
            .collect::<Vec<_>>();
        let cli_evidence = evidence
            .iter_mut()
            .find(|item| item.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
            .expect("test includes CLI evidence");
        cli_evidence.fallback_artifact_visible = false;

        let decision = decide_browser_provider_default_from_cards(&policy, &cards, &evidence);

        assert_eq!(
            decision.status,
            BrowserProviderDefaultDecisionStatus::Blocked
        );
        assert!(decision.selected_provider_id.is_none());
        assert_eq!(
            decision.rollback_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
        assert!(decision.candidates.iter().any(|candidate| {
            candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
                && candidate.promotion_eligible
                && !candidate.fallback_eligible
        }));
    }

    fn test_card(
        provider_id: &'static str,
        lane: BrowserProviderLane,
        total: u16,
        passed: u16,
        promotion_eligible: bool,
        tracked_metrics: &'static [&'static str],
    ) -> BrowserProviderCapabilityCard {
        BrowserProviderCapabilityCard {
            provider_id,
            lane,
            display_name: provider_id,
            summary: provider_id,
            feature_flag: None,
            enabled_by_default: provider_id == LOCAL_CHROMIUM_PROVIDER_ID,
            requires_runtime_pack: false,
            uses_isolated_profile_by_default: true,
            supports_identity: lane != BrowserProviderLane::Hosted,
            allows_raw_script_by_default: false,
            supported_actions: &["navigate", "click"],
            observation_modes: &["screenshot"],
            artifact_policy: "risk_based",
            policy_tags: &["local_first"],
            harness_subjects: &["browser.default_policy"],
            harness_score: BrowserProviderHarnessScore {
                fixture_cases_total: total,
                fixture_cases_passed: passed,
                tracked_metrics,
                promotion_eligible,
                source: "test_scorecard",
            },
            disable_path: "test disable path",
        }
    }
}
