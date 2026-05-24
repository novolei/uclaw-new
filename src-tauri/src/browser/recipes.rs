use serde::{Deserialize, Serialize};

pub const BROWSER_RECIPE_SCHEMA_VERSION: u16 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeKey {
    pub site_origin: String,
    pub route_pattern: String,
    pub dom_fingerprint: String,
    pub instruction_family: String,
    pub provider_id: String,
    pub provider_version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRecipeActionKind {
    Navigate,
    Click,
    Type,
    Wait,
    Screenshot,
    Extract,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum BrowserRecipeAddressing {
    SemanticLocator {
        role: Option<String>,
        label: Option<String>,
        text: Option<String>,
        test_id: Option<String>,
    },
    DomIndex {
        index: u32,
        source_artifact_ref: String,
    },
    CoordinateFallback {
        x: u32,
        y: u32,
        reason: String,
    },
    NoElement,
}

impl BrowserRecipeAddressing {
    fn is_transient_coordinate(&self) -> bool {
        matches!(self, Self::CoordinateFallback { .. })
    }

    fn has_stable_locator(&self) -> bool {
        match self {
            Self::SemanticLocator {
                role,
                label,
                text,
                test_id,
            } => {
                option_has_text(role)
                    || option_has_text(label)
                    || option_has_text(text)
                    || option_has_text(test_id)
            }
            Self::DomIndex {
                source_artifact_ref,
                ..
            } => !source_artifact_ref.trim().is_empty(),
            Self::CoordinateFallback { .. } => false,
            Self::NoElement => true,
        }
    }
}

fn option_has_text(value: &Option<String>) -> bool {
    value
        .as_deref()
        .map(|text| !text.trim().is_empty())
        .unwrap_or(false)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeActionTemplate {
    pub action_id: String,
    pub kind: BrowserRecipeActionKind,
    pub addressing: BrowserRecipeAddressing,
    pub wait_condition: Option<String>,
    pub expected_state_diff: Option<String>,
    pub artifact_refs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeActionObservation {
    pub action_id: String,
    pub kind: BrowserRecipeActionKind,
    pub addressing: BrowserRecipeAddressing,
    pub wait_condition: Option<String>,
    pub expected_state_diff: Option<String>,
    pub artifact_refs: Vec<String>,
    pub succeeded: bool,
}

impl BrowserRecipeActionObservation {
    fn into_template(self) -> BrowserRecipeActionTemplate {
        BrowserRecipeActionTemplate {
            action_id: self.action_id,
            kind: self.kind,
            addressing: self.addressing,
            wait_condition: self.wait_condition,
            expected_state_diff: self.expected_state_diff,
            artifact_refs: unique_non_empty(self.artifact_refs),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDomainSkillCandidate {
    pub stable_url_patterns: Vec<String>,
    pub selector_notes: Vec<String>,
    pub private_api_shapes: Vec<String>,
    pub wait_conditions: Vec<String>,
    pub iframe_shadow_dom_notes: Vec<String>,
    pub auth_boundaries: Vec<String>,
    pub known_traps: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeRedactionReport {
    pub reviewed: bool,
    pub secrets_detected: Vec<String>,
    pub private_user_data_detected: Vec<String>,
    pub task_diary_detected: bool,
    pub transient_pixel_coordinates_detected: bool,
}

impl BrowserRecipeRedactionReport {
    pub fn clean_reviewed() -> Self {
        Self {
            reviewed: true,
            ..Self::default()
        }
    }

    fn rejection_reasons(&self) -> Vec<String> {
        let mut reasons = Vec::new();
        if !self.reviewed {
            reasons.push("redaction_review_missing".to_string());
        }
        if !self.secrets_detected.is_empty() {
            reasons.push("secrets_detected".to_string());
        }
        if !self.private_user_data_detected.is_empty() {
            reasons.push("private_user_data_detected".to_string());
        }
        if self.task_diary_detected {
            reasons.push("task_diary_detected".to_string());
        }
        if self.transient_pixel_coordinates_detected {
            reasons.push("transient_pixel_coordinates_detected".to_string());
        }
        reasons
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeEvidence {
    pub artifact_refs: Vec<String>,
    pub harness_case_ids: Vec<String>,
    pub replay_success_count: u16,
    pub replay_failure_count: u16,
    pub redaction_review_artifact_ref: Option<String>,
}

impl BrowserRecipeEvidence {
    fn has_promotion_evidence(&self) -> bool {
        !self.artifact_refs.is_empty()
            && !self.harness_case_ids.is_empty()
            && self.replay_success_count > 0
            && self.replay_failure_count == 0
            && self.redaction_review_artifact_ref.is_some()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRecipePromotionState {
    Candidate,
    RedactionRejected,
    HarnessReady,
    Promoted,
    RolledBack,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeCandidate {
    pub schema_version: u16,
    pub recipe_id: String,
    pub key: BrowserRecipeKey,
    pub actions: Vec<BrowserRecipeActionTemplate>,
    pub domain_skill_candidate: Option<BrowserDomainSkillCandidate>,
    pub redaction: BrowserRecipeRedactionReport,
    pub evidence: BrowserRecipeEvidence,
    pub promotion_state: BrowserRecipePromotionState,
    pub rollback_recipe_id: Option<String>,
}

impl BrowserRecipeCandidate {
    fn has_rollback_recipe_id(&self) -> bool {
        self.rollback_recipe_id
            .as_deref()
            .map(|recipe_id| !recipe_id.trim().is_empty())
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRecipeCandidateStatus {
    CandidateReady,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeCandidateValidation {
    pub status: BrowserRecipeCandidateStatus,
    pub promotion_ready: bool,
    pub rejection_reasons: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeNormalizationInput {
    pub recipe_id: String,
    pub key: BrowserRecipeKey,
    pub actions: Vec<BrowserRecipeActionObservation>,
    pub domain_skill_candidate: Option<BrowserDomainSkillCandidate>,
    pub redaction: BrowserRecipeRedactionReport,
    pub artifact_refs: Vec<String>,
    pub harness_case_ids: Vec<String>,
    pub replay_success_count: u16,
    pub replay_failure_count: u16,
    pub redaction_review_artifact_ref: Option<String>,
    pub rollback_recipe_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRecipeNormalizationStatus {
    CandidateBuilt,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeNormalizationResult {
    pub status: BrowserRecipeNormalizationStatus,
    pub candidate: BrowserRecipeCandidate,
    pub validation: BrowserRecipeCandidateValidation,
    pub rejected_action_ids: Vec<String>,
    pub normalization_reasons: Vec<String>,
}

pub fn normalize_browser_recipe_candidate(
    input: BrowserRecipeNormalizationInput,
) -> BrowserRecipeNormalizationResult {
    let mut rejected_action_ids = Vec::new();
    let mut normalization_reasons = Vec::new();
    let mut action_templates = Vec::new();
    let mut action_artifact_refs = Vec::new();
    let mut replay_failure_count = input.replay_failure_count;

    for action in input.actions {
        if action.succeeded {
            action_artifact_refs.extend(action.artifact_refs.iter().cloned());
            action_templates.push(action.into_template());
        } else {
            rejected_action_ids.push(action.action_id.clone());
            replay_failure_count = replay_failure_count.saturating_add(1);
        }
    }
    rejected_action_ids.sort();
    rejected_action_ids.dedup();

    if !rejected_action_ids.is_empty() {
        normalization_reasons.push("failed_action_observation".to_string());
    }

    let mut artifact_refs = input.artifact_refs;
    artifact_refs.extend(action_artifact_refs);

    let candidate = BrowserRecipeCandidate {
        schema_version: BROWSER_RECIPE_SCHEMA_VERSION,
        recipe_id: input.recipe_id,
        key: input.key,
        actions: action_templates,
        domain_skill_candidate: input.domain_skill_candidate,
        redaction: input.redaction,
        evidence: BrowserRecipeEvidence {
            artifact_refs: unique_non_empty(artifact_refs),
            harness_case_ids: unique_non_empty(input.harness_case_ids),
            replay_success_count: input.replay_success_count,
            replay_failure_count,
            redaction_review_artifact_ref: input
                .redaction_review_artifact_ref
                .filter(|artifact_ref| !artifact_ref.trim().is_empty()),
        },
        promotion_state: BrowserRecipePromotionState::Candidate,
        rollback_recipe_id: input
            .rollback_recipe_id
            .filter(|recipe_id| !recipe_id.trim().is_empty()),
    };

    let validation = validate_browser_recipe_candidate(&candidate);
    normalization_reasons.extend(validation.rejection_reasons.iter().cloned());
    normalization_reasons.sort();
    normalization_reasons.dedup();

    let status = if normalization_reasons.is_empty() {
        BrowserRecipeNormalizationStatus::CandidateBuilt
    } else {
        BrowserRecipeNormalizationStatus::Rejected
    };

    BrowserRecipeNormalizationResult {
        status,
        candidate,
        validation,
        rejected_action_ids,
        normalization_reasons,
    }
}

pub fn validate_browser_recipe_candidate(
    candidate: &BrowserRecipeCandidate,
) -> BrowserRecipeCandidateValidation {
    let mut rejection_reasons = Vec::new();

    if candidate.schema_version != BROWSER_RECIPE_SCHEMA_VERSION {
        rejection_reasons.push("schema_version_mismatch".to_string());
    }
    if candidate.recipe_id.trim().is_empty() {
        rejection_reasons.push("recipe_id_missing".to_string());
    }
    if candidate.key.site_origin.trim().is_empty() {
        rejection_reasons.push("site_origin_missing".to_string());
    }
    if candidate.key.route_pattern.trim().is_empty() {
        rejection_reasons.push("route_pattern_missing".to_string());
    }
    if candidate.key.dom_fingerprint.trim().is_empty() {
        rejection_reasons.push("dom_fingerprint_missing".to_string());
    }
    if candidate.key.instruction_family.trim().is_empty() {
        rejection_reasons.push("instruction_family_missing".to_string());
    }
    if candidate.key.provider_id.trim().is_empty() {
        rejection_reasons.push("provider_id_missing".to_string());
    }
    if candidate.key.provider_version.trim().is_empty() {
        rejection_reasons.push("provider_version_missing".to_string());
    }
    if candidate.actions.is_empty() {
        rejection_reasons.push("actions_missing".to_string());
    }
    if !candidate.has_rollback_recipe_id() {
        rejection_reasons.push("rollback_recipe_id_missing".to_string());
    }

    for action in &candidate.actions {
        if action.action_id.trim().is_empty() {
            rejection_reasons.push("action_id_missing".to_string());
        }
        if action.addressing.is_transient_coordinate() {
            rejection_reasons.push("transient_pixel_coordinates_detected".to_string());
        }
        if !action.addressing.has_stable_locator() {
            rejection_reasons.push("stable_locator_missing".to_string());
        }
    }

    rejection_reasons.extend(candidate.redaction.rejection_reasons());
    rejection_reasons.sort();
    rejection_reasons.dedup();

    let promotion_ready = rejection_reasons.is_empty()
        && candidate.evidence.has_promotion_evidence()
        && candidate.has_rollback_recipe_id()
        && matches!(
            candidate.promotion_state,
            BrowserRecipePromotionState::Candidate | BrowserRecipePromotionState::HarnessReady
        );

    BrowserRecipeCandidateValidation {
        status: if rejection_reasons.is_empty() {
            BrowserRecipeCandidateStatus::CandidateReady
        } else {
            BrowserRecipeCandidateStatus::Rejected
        },
        promotion_ready,
        rejection_reasons,
    }
}

fn unique_non_empty(values: Vec<String>) -> Vec<String> {
    let mut values: Vec<String> = values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect();
    values.sort();
    values.dedup();
    values
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeReplayRequest {
    pub recipe_id: String,
    pub current_dom_fingerprint: String,
    pub current_provider_id: String,
    pub current_provider_version: String,
    pub production_replay_allowed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRecipeReplayDecisionStatus {
    ReplayAllowed,
    RecipeIdMismatch,
    FingerprintMismatch,
    ProviderVersionMismatch,
    CandidateInvalid,
    NotPromoted,
    ProductionReplayDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRecipeReplayDecision {
    pub status: BrowserRecipeReplayDecisionStatus,
    pub recipe_id: String,
    pub fallback_to_observation: bool,
    pub reasons: Vec<String>,
}

pub fn decide_browser_recipe_replay(
    candidate: &BrowserRecipeCandidate,
    request: &BrowserRecipeReplayRequest,
) -> BrowserRecipeReplayDecision {
    if candidate.recipe_id != request.recipe_id {
        return replay_decision(
            request,
            BrowserRecipeReplayDecisionStatus::RecipeIdMismatch,
            true,
            "recipe_id_mismatch",
        );
    }

    let validation = validate_browser_recipe_candidate(candidate);
    if validation.status == BrowserRecipeCandidateStatus::Rejected {
        return replay_decision_with_reasons(
            request,
            BrowserRecipeReplayDecisionStatus::CandidateInvalid,
            true,
            validation.rejection_reasons,
        );
    }

    let mut replay_integrity_reasons = Vec::new();
    if !candidate.evidence.has_promotion_evidence() {
        replay_integrity_reasons.push("promotion_evidence_missing".to_string());
    }
    if !candidate.has_rollback_recipe_id() {
        replay_integrity_reasons.push("rollback_recipe_id_missing".to_string());
    }
    if !replay_integrity_reasons.is_empty() {
        return replay_decision_with_reasons(
            request,
            BrowserRecipeReplayDecisionStatus::CandidateInvalid,
            true,
            replay_integrity_reasons,
        );
    }

    if candidate.promotion_state != BrowserRecipePromotionState::Promoted {
        return replay_decision(
            request,
            BrowserRecipeReplayDecisionStatus::NotPromoted,
            true,
            "recipe_not_promoted",
        );
    }
    if !request.production_replay_allowed {
        return replay_decision(
            request,
            BrowserRecipeReplayDecisionStatus::ProductionReplayDisabled,
            true,
            "production_replay_disabled",
        );
    }
    if candidate.key.dom_fingerprint != request.current_dom_fingerprint {
        return replay_decision(
            request,
            BrowserRecipeReplayDecisionStatus::FingerprintMismatch,
            true,
            "dom_fingerprint_mismatch",
        );
    }
    if candidate.key.provider_id != request.current_provider_id
        || candidate.key.provider_version != request.current_provider_version
    {
        return replay_decision(
            request,
            BrowserRecipeReplayDecisionStatus::ProviderVersionMismatch,
            true,
            "provider_version_mismatch",
        );
    }

    replay_decision(
        request,
        BrowserRecipeReplayDecisionStatus::ReplayAllowed,
        false,
        "recipe_replay_allowed",
    )
}

fn replay_decision(
    request: &BrowserRecipeReplayRequest,
    status: BrowserRecipeReplayDecisionStatus,
    fallback_to_observation: bool,
    reason: &str,
) -> BrowserRecipeReplayDecision {
    replay_decision_with_reasons(
        request,
        status,
        fallback_to_observation,
        vec![reason.to_string()],
    )
}

fn replay_decision_with_reasons(
    request: &BrowserRecipeReplayRequest,
    status: BrowserRecipeReplayDecisionStatus,
    fallback_to_observation: bool,
    reasons: Vec<String>,
) -> BrowserRecipeReplayDecision {
    BrowserRecipeReplayDecision {
        status,
        recipe_id: request.recipe_id.clone(),
        fallback_to_observation,
        reasons,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_recipe_candidate_is_ready_for_promotion_when_evidence_and_rollback_exist() {
        let candidate = base_candidate();

        let validation = validate_browser_recipe_candidate(&candidate);

        assert_eq!(
            validation.status,
            BrowserRecipeCandidateStatus::CandidateReady
        );
        assert!(validation.promotion_ready);
        assert!(validation.rejection_reasons.is_empty());
    }

    #[test]
    fn replay_failures_block_promotion_readiness() {
        let mut candidate = base_candidate();
        candidate.evidence.replay_failure_count = 1;

        let validation = validate_browser_recipe_candidate(&candidate);

        assert_eq!(
            validation.status,
            BrowserRecipeCandidateStatus::CandidateReady
        );
        assert!(!validation.promotion_ready);
    }

    #[test]
    fn blank_rollback_recipe_id_rejects_candidate_and_blocks_replay() {
        let mut candidate = base_candidate();
        candidate.rollback_recipe_id = Some("   ".to_string());

        let validation = validate_browser_recipe_candidate(&candidate);

        assert_eq!(validation.status, BrowserRecipeCandidateStatus::Rejected);
        assert!(!validation.promotion_ready);
        assert!(validation
            .rejection_reasons
            .contains(&"rollback_recipe_id_missing".to_string()));

        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = matching_replay_request(&candidate);
        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::CandidateInvalid
        );
        assert!(decision.fallback_to_observation);
        assert!(decision
            .reasons
            .contains(&"rollback_recipe_id_missing".to_string()));
    }

    #[test]
    fn candidate_rejects_secrets_private_data_diaries_and_transient_coordinates() {
        let mut candidate = base_candidate();
        candidate.redaction = BrowserRecipeRedactionReport {
            reviewed: true,
            secrets_detected: vec!["password".to_string()],
            private_user_data_detected: vec!["email".to_string()],
            task_diary_detected: true,
            transient_pixel_coordinates_detected: true,
        };
        candidate.actions[0].addressing = BrowserRecipeAddressing::CoordinateFallback {
            x: 120,
            y: 480,
            reason: "last successful click".to_string(),
        };

        let validation = validate_browser_recipe_candidate(&candidate);

        assert_eq!(validation.status, BrowserRecipeCandidateStatus::Rejected);
        assert!(!validation.promotion_ready);
        assert!(validation
            .rejection_reasons
            .contains(&"secrets_detected".to_string()));
        assert!(validation
            .rejection_reasons
            .contains(&"private_user_data_detected".to_string()));
        assert!(validation
            .rejection_reasons
            .contains(&"task_diary_detected".to_string()));
        assert!(validation
            .rejection_reasons
            .contains(&"transient_pixel_coordinates_detected".to_string()));
    }

    #[test]
    fn blank_semantic_locator_rejects_candidate_and_blocks_replay() {
        let mut candidate = base_candidate();
        candidate.actions[0].addressing = BrowserRecipeAddressing::SemanticLocator {
            role: Some("   ".to_string()),
            label: Some(String::new()),
            text: None,
            test_id: None,
        };

        let validation = validate_browser_recipe_candidate(&candidate);

        assert_eq!(validation.status, BrowserRecipeCandidateStatus::Rejected);
        assert!(validation
            .rejection_reasons
            .contains(&"stable_locator_missing".to_string()));

        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = matching_replay_request(&candidate);
        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::CandidateInvalid
        );
        assert!(decision.fallback_to_observation);
        assert!(decision
            .reasons
            .contains(&"stable_locator_missing".to_string()));
    }

    #[test]
    fn fingerprint_mismatch_blocks_replay_and_falls_back_to_observation() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = BrowserRecipeReplayRequest {
            recipe_id: candidate.recipe_id.clone(),
            current_dom_fingerprint: "different-fingerprint".to_string(),
            current_provider_id: candidate.key.provider_id.clone(),
            current_provider_version: candidate.key.provider_version.clone(),
            production_replay_allowed: true,
        };

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::FingerprintMismatch
        );
        assert!(decision.fallback_to_observation);
    }

    #[test]
    fn provider_version_mismatch_invalidates_replay() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = BrowserRecipeReplayRequest {
            recipe_id: candidate.recipe_id.clone(),
            current_dom_fingerprint: candidate.key.dom_fingerprint.clone(),
            current_provider_id: candidate.key.provider_id.clone(),
            current_provider_version: "playwright-cli@2.0.0".to_string(),
            production_replay_allowed: true,
        };

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::ProviderVersionMismatch
        );
        assert!(decision.fallback_to_observation);
    }

    #[test]
    fn recipe_id_mismatch_blocks_replay() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = BrowserRecipeReplayRequest {
            recipe_id: "recipe:other:1".to_string(),
            current_dom_fingerprint: candidate.key.dom_fingerprint.clone(),
            current_provider_id: candidate.key.provider_id.clone(),
            current_provider_version: candidate.key.provider_version.clone(),
            production_replay_allowed: true,
        };

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::RecipeIdMismatch
        );
        assert!(decision.fallback_to_observation);
    }

    #[test]
    fn promoted_but_invalid_candidate_blocks_replay() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        candidate.redaction.secrets_detected = vec!["api_token".to_string()];
        let request = matching_replay_request(&candidate);

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::CandidateInvalid
        );
        assert!(decision.fallback_to_observation);
        assert!(decision.reasons.contains(&"secrets_detected".to_string()));
    }

    #[test]
    fn production_replay_stays_disabled_even_for_promoted_recipe_without_policy() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = BrowserRecipeReplayRequest {
            recipe_id: candidate.recipe_id.clone(),
            current_dom_fingerprint: candidate.key.dom_fingerprint.clone(),
            current_provider_id: candidate.key.provider_id.clone(),
            current_provider_version: candidate.key.provider_version.clone(),
            production_replay_allowed: false,
        };

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::ProductionReplayDisabled
        );
        assert!(decision.fallback_to_observation);
    }

    #[test]
    fn promoted_recipe_with_matching_fingerprint_and_provider_allows_replay_contract() {
        let mut candidate = base_candidate();
        candidate.promotion_state = BrowserRecipePromotionState::Promoted;
        let request = matching_replay_request(&candidate);

        let decision = decide_browser_recipe_replay(&candidate, &request);

        assert_eq!(
            decision.status,
            BrowserRecipeReplayDecisionStatus::ReplayAllowed
        );
        assert!(!decision.fallback_to_observation);
    }

    #[test]
    fn normalization_builds_candidate_from_successful_action_evidence() {
        let result = normalize_browser_recipe_candidate(base_normalization_input());

        assert_eq!(
            result.status,
            BrowserRecipeNormalizationStatus::CandidateBuilt
        );
        assert!(result.rejected_action_ids.is_empty());
        assert!(result.normalization_reasons.is_empty());
        assert_eq!(result.candidate.actions.len(), 2);
        assert_eq!(result.candidate.evidence.replay_success_count, 2);
        assert_eq!(result.candidate.evidence.replay_failure_count, 0);
        assert_eq!(
            result.candidate.evidence.artifact_refs,
            vec![
                "artifact://browser/action-click".to_string(),
                "artifact://browser/action-navigate".to_string(),
                "artifact://browser/run".to_string(),
            ]
        );
        assert_eq!(
            result.validation.status,
            BrowserRecipeCandidateStatus::CandidateReady
        );
        assert!(result.validation.promotion_ready);
    }

    #[test]
    fn normalization_rejects_failed_action_observations() {
        let mut input = base_normalization_input();
        input.actions.push(BrowserRecipeActionObservation {
            action_id: "wait-failed".to_string(),
            kind: BrowserRecipeActionKind::Wait,
            addressing: BrowserRecipeAddressing::NoElement,
            wait_condition: Some("selector:#missing".to_string()),
            expected_state_diff: None,
            artifact_refs: vec!["artifact://browser/wait-timeout".to_string()],
            succeeded: false,
        });

        let result = normalize_browser_recipe_candidate(input);

        assert_eq!(result.status, BrowserRecipeNormalizationStatus::Rejected);
        assert_eq!(result.rejected_action_ids, vec!["wait-failed".to_string()]);
        assert!(result
            .normalization_reasons
            .contains(&"failed_action_observation".to_string()));
        assert_eq!(result.candidate.actions.len(), 2);
        assert_eq!(result.candidate.evidence.replay_failure_count, 1);
        assert!(!result.validation.promotion_ready);
    }

    #[test]
    fn normalization_keeps_redaction_rejection_visible() {
        let mut input = base_normalization_input();
        input.redaction.private_user_data_detected = vec!["email".to_string()];

        let result = normalize_browser_recipe_candidate(input);

        assert_eq!(result.status, BrowserRecipeNormalizationStatus::Rejected);
        assert_eq!(
            result.validation.status,
            BrowserRecipeCandidateStatus::Rejected
        );
        assert!(result
            .normalization_reasons
            .contains(&"private_user_data_detected".to_string()));
    }

    #[test]
    fn normalization_rejects_transient_coordinate_observations() {
        let mut input = base_normalization_input();
        input.actions[1].addressing = BrowserRecipeAddressing::CoordinateFallback {
            x: 32,
            y: 48,
            reason: "last click position".to_string(),
        };

        let result = normalize_browser_recipe_candidate(input);

        assert_eq!(result.status, BrowserRecipeNormalizationStatus::Rejected);
        assert_eq!(
            result.validation.status,
            BrowserRecipeCandidateStatus::Rejected
        );
        assert!(result
            .normalization_reasons
            .contains(&"transient_pixel_coordinates_detected".to_string()));
    }

    #[test]
    fn normalization_filters_blank_evidence_and_rollback_before_validation() {
        let mut input = base_normalization_input();
        input.artifact_refs = vec!["   ".to_string(), String::new()];
        input.actions.clear();
        input.harness_case_ids = vec!["   ".to_string()];
        input.redaction_review_artifact_ref = Some("   ".to_string());
        input.rollback_recipe_id = Some("   ".to_string());

        let result = normalize_browser_recipe_candidate(input);

        assert_eq!(result.status, BrowserRecipeNormalizationStatus::Rejected);
        assert!(result.candidate.actions.is_empty());
        assert!(result.candidate.evidence.artifact_refs.is_empty());
        assert!(result.candidate.evidence.harness_case_ids.is_empty());
        assert!(result
            .candidate
            .evidence
            .redaction_review_artifact_ref
            .is_none());
        assert!(result.candidate.rollback_recipe_id.is_none());
        assert!(result
            .normalization_reasons
            .contains(&"actions_missing".to_string()));
        assert!(result
            .normalization_reasons
            .contains(&"rollback_recipe_id_missing".to_string()));
        assert!(!result.validation.promotion_ready);
    }

    fn matching_replay_request(candidate: &BrowserRecipeCandidate) -> BrowserRecipeReplayRequest {
        BrowserRecipeReplayRequest {
            recipe_id: candidate.recipe_id.clone(),
            current_dom_fingerprint: candidate.key.dom_fingerprint.clone(),
            current_provider_id: candidate.key.provider_id.clone(),
            current_provider_version: candidate.key.provider_version.clone(),
            production_replay_allowed: true,
        }
    }

    fn base_candidate() -> BrowserRecipeCandidate {
        BrowserRecipeCandidate {
            schema_version: BROWSER_RECIPE_SCHEMA_VERSION,
            recipe_id: "recipe:example-login:1".to_string(),
            key: BrowserRecipeKey {
                site_origin: "https://example.test".to_string(),
                route_pattern: "/login".to_string(),
                dom_fingerprint: "dom:a11y-v1:abcd".to_string(),
                instruction_family: "login-status-check".to_string(),
                provider_id: "browser.playwright_cli".to_string(),
                provider_version: "playwright-cli@1.53.0".to_string(),
            },
            actions: vec![BrowserRecipeActionTemplate {
                action_id: "click-submit".to_string(),
                kind: BrowserRecipeActionKind::Click,
                addressing: BrowserRecipeAddressing::SemanticLocator {
                    role: Some("button".to_string()),
                    label: Some("Continue".to_string()),
                    text: None,
                    test_id: None,
                },
                wait_condition: Some("url_contains:/dashboard".to_string()),
                expected_state_diff: Some("url changed".to_string()),
                artifact_refs: vec!["artifact://browser/trace-1".to_string()],
            }],
            domain_skill_candidate: Some(BrowserDomainSkillCandidate {
                stable_url_patterns: vec!["/login".to_string()],
                selector_notes: vec!["Prefer button role labels".to_string()],
                private_api_shapes: vec!["GET /api/session".to_string()],
                wait_conditions: vec!["dashboard navigation".to_string()],
                iframe_shadow_dom_notes: Vec::new(),
                auth_boundaries: vec!["requires managed identity".to_string()],
                known_traps: vec!["avoid saving entered credentials".to_string()],
            }),
            redaction: BrowserRecipeRedactionReport::clean_reviewed(),
            evidence: BrowserRecipeEvidence {
                artifact_refs: vec!["artifact://browser/trace-1".to_string()],
                harness_case_ids: vec!["browser.recipe.example-login".to_string()],
                replay_success_count: 3,
                replay_failure_count: 0,
                redaction_review_artifact_ref: Some("artifact://redaction/review-1".to_string()),
            },
            promotion_state: BrowserRecipePromotionState::HarnessReady,
            rollback_recipe_id: Some("recipe:example-login:0".to_string()),
        }
    }

    fn base_normalization_input() -> BrowserRecipeNormalizationInput {
        BrowserRecipeNormalizationInput {
            recipe_id: "recipe:example-login:1".to_string(),
            key: BrowserRecipeKey {
                site_origin: "https://example.test".to_string(),
                route_pattern: "/login".to_string(),
                dom_fingerprint: "dom:a11y-v1:abcd".to_string(),
                instruction_family: "login-status-check".to_string(),
                provider_id: "browser.playwright_cli".to_string(),
                provider_version: "playwright-cli@1.53.0".to_string(),
            },
            actions: vec![
                BrowserRecipeActionObservation {
                    action_id: "navigate-login".to_string(),
                    kind: BrowserRecipeActionKind::Navigate,
                    addressing: BrowserRecipeAddressing::NoElement,
                    wait_condition: Some("url_contains:/login".to_string()),
                    expected_state_diff: Some("url changed".to_string()),
                    artifact_refs: vec![
                        "artifact://browser/action-navigate".to_string(),
                        "artifact://browser/action-navigate".to_string(),
                    ],
                    succeeded: true,
                },
                BrowserRecipeActionObservation {
                    action_id: "click-submit".to_string(),
                    kind: BrowserRecipeActionKind::Click,
                    addressing: BrowserRecipeAddressing::SemanticLocator {
                        role: Some("button".to_string()),
                        label: Some("Continue".to_string()),
                        text: None,
                        test_id: None,
                    },
                    wait_condition: Some("url_contains:/dashboard".to_string()),
                    expected_state_diff: Some("url changed".to_string()),
                    artifact_refs: vec!["artifact://browser/action-click".to_string()],
                    succeeded: true,
                },
            ],
            domain_skill_candidate: Some(BrowserDomainSkillCandidate {
                stable_url_patterns: vec!["/login".to_string()],
                selector_notes: vec!["Prefer button role labels".to_string()],
                private_api_shapes: vec!["GET /api/session".to_string()],
                wait_conditions: vec!["dashboard navigation".to_string()],
                iframe_shadow_dom_notes: Vec::new(),
                auth_boundaries: vec!["requires managed identity".to_string()],
                known_traps: vec!["avoid saving entered credentials".to_string()],
            }),
            redaction: BrowserRecipeRedactionReport::clean_reviewed(),
            artifact_refs: vec![
                " artifact://browser/run ".to_string(),
                String::new(),
                "artifact://browser/action-click".to_string(),
            ],
            harness_case_ids: vec!["browser.recipe.example-login".to_string()],
            replay_success_count: 2,
            replay_failure_count: 0,
            redaction_review_artifact_ref: Some("artifact://redaction/review-1".to_string()),
            rollback_recipe_id: Some("recipe:example-login:0".to_string()),
        }
    }
}
