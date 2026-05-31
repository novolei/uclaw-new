use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyAction, MemoryPolicyActionKind,
    MemoryPolicyDecision, MemoryPolicyExecutionMode, MemoryPolicyInput, MemoryPolicyReasonCode,
    MemoryPolicyReceiptStatus, MemoryPolicySource, MemoryPolicyTargetAdapter,
};
use crate::memory_policy::targets::browser_artifact::BrowserArtifactPolicyTarget;
use crate::memory_policy::targets::gbrain::GbrainPolicyTarget;
use crate::mcp::SharedMcpManager;
use crate::memory::MemoryStore;
use std::sync::Arc;

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

#[derive(Clone)]
pub struct BrowserRuntimeMemoryPolicyExecutor {
    browser_artifact: BrowserArtifactPolicyTarget,
    gbrain: GbrainPolicyTarget,
}

impl BrowserRuntimeMemoryPolicyExecutor {
    pub fn new(memory_store: Arc<MemoryStore>, gbrain_manager: Option<SharedMcpManager>) -> Self {
        Self {
            browser_artifact: BrowserArtifactPolicyTarget::new_memory_store(
                memory_store,
                "browser_task",
            ),
            gbrain: gbrain_manager
                .map(GbrainPolicyTarget::new)
                .unwrap_or_else(GbrainPolicyTarget::unavailable_for_tests),
        }
    }

    pub async fn execute_decision(
        &self,
        decision: &MemoryPolicyDecision,
    ) -> Vec<crate::memory_policy::MemoryPolicyExecutionReceipt> {
        let mut receipts = Vec::with_capacity(decision.actions.len());
        for action in &decision.actions {
            let result = match action.kind {
                MemoryPolicyActionKind::BrowserArtifactWrite => {
                    self.browser_artifact.execute(decision, action).await
                }
                MemoryPolicyActionKind::GbrainWrite => self.gbrain.execute(decision, action).await,
                MemoryPolicyActionKind::MemoryGraphWrite => Ok(build_receipt(
                    decision,
                    action,
                    MemoryPolicyReceiptStatus::Rejected,
                    Some(MemoryPolicyReasonCode::MemoryGraphFrozen),
                    Some(format!("memory-policy://rejected/{}", action.action_id)),
                    None,
                    Some("memory_graph writes are frozen".into()),
                )),
                MemoryPolicyActionKind::MemoryGraphRead | MemoryPolicyActionKind::MemuWriteOrIndex => {
                    Ok(build_receipt(
                        decision,
                        action,
                        MemoryPolicyReceiptStatus::Deferred,
                        Some(MemoryPolicyReasonCode::PolicyDenied),
                        Some(format!("memory-policy://deferred/{}", action.action_id)),
                        None,
                        Some("browser runtime memory policy executor does not handle this target".into()),
                    ))
                }
            };
            receipts.push(result.unwrap_or_else(|error| {
                build_receipt(
                    decision,
                    action,
                    MemoryPolicyReceiptStatus::Failed,
                    Some(MemoryPolicyReasonCode::TargetError),
                    Some(format!("memory-policy://failed/{}", action.action_id)),
                    None,
                    Some(error.to_string()),
                )
            }));
        }
        receipts
    }
}
