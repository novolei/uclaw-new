use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyAction, MemoryPolicyActionKind,
    MemoryPolicyDecision, MemoryPolicyExecutionMode, MemoryPolicyInput, MemoryPolicySource,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserMemoryPromotionMetadata {
    pub redaction_clean: bool,
    pub approval_ref: Option<String>,
    pub harness_case_ids: Vec<String>,
}

pub fn classify_browser_evidence(
    source_event_id: impl Into<String>,
    task_id: impl Into<String>,
    content: impl Into<String>,
    promotion: Option<BrowserMemoryPromotionMetadata>,
) -> MemoryPolicyDecision {
    let source_event_id = source_event_id.into();
    let task_id = task_id.into();
    let content = content.into();
    let mut decision = classify_memory_policy_input(MemoryPolicyInput {
        source: MemoryPolicySource::BrowserRuntime,
        source_event_id,
        task_id,
        intent_id: None,
        content,
        requested_class: MemoryKnowledgeClass::EpisodicEvidence,
        promoted: promotion.is_some(),
        redaction_clean: promotion
            .as_ref()
            .map(|p| p.redaction_clean)
            .unwrap_or(false),
        approval_ref: promotion.as_ref().and_then(|p| p.approval_ref.clone()),
        harness_case_ids: promotion
            .as_ref()
            .map(|p| p.harness_case_ids.clone())
            .unwrap_or_default(),
    });

    if let Some(promotion) = promotion {
        if promotion.redaction_clean
            && (promotion.approval_ref.is_some() || !promotion.harness_case_ids.is_empty())
        {
            let gbrain_action = MemoryPolicyAction {
                action_id: format!("{}-promote-gbrain", decision.actions[0].action_id),
                kind: MemoryPolicyActionKind::GbrainWrite,
                target: MemoryPolicyActionKind::GbrainWrite.target(),
                execution_mode: MemoryPolicyExecutionMode::BoundedAwait,
                topic: "browser_promoted_knowledge".into(),
                size_bytes: decision.input.content.len(),
                idempotency_key: format!("{}:gbrain:promoted", decision.input.source_event_id),
            };
            decision.actions.push(gbrain_action);
        }
    }
    decision
}
