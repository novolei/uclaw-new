use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyInput, MemoryPolicySource,
};
use crate::runtime::context::ContextSource;
use crate::runtime::context_memory_policy::memory_receipt_to_context_artifact;

fn input() -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::ContextFabric,
        source_event_id: "ctx-event-1".into(),
        task_id: "task-1".into(),
        intent_id: None,
        content: "project uclaw memory policy".into(),
        requested_class: MemoryKnowledgeClass::LegacyRead,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: Vec::new(),
    }
}

#[test]
fn receipt_becomes_context_artifact_with_memory_source() {
    let decision = classify_memory_policy_input(input());
    let action = &decision.actions[0];
    let receipt = crate::memory_policy::receipts::build_receipt(
        &decision,
        action,
        crate::memory_policy::MemoryPolicyReceiptStatus::Succeeded,
        None,
        Some("memory-policy://receipt/r1".into()),
        Some("memory_graph:legacy_read".into()),
        None,
    );
    let artifact = memory_receipt_to_context_artifact(&receipt, "legacy recall body");
    assert_eq!(artifact.r#ref.source, ContextSource::Memory);
    assert!(artifact.content.contains("legacy recall body"));
    assert_eq!(artifact.citations.len(), 1);
}
