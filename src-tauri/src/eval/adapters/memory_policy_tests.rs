use crate::eval::adapters::memory_policy::attach_memory_policy_receipt;
use crate::eval::case::{HarnessBudget, EvalCase, HarnessPolicy, EvalSubject};
use crate::eval::runtime::EvalRuntime;
use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyInput, MemoryPolicyReasonCode,
    MemoryPolicyReceiptStatus, MemoryPolicySource,
};

fn input() -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::Harness,
        source_event_id: "harness-event-1".into(),
        task_id: "task-1".into(),
        intent_id: None,
        content: "memory policy receipt harness test".into(),
        requested_class: MemoryKnowledgeClass::Forbidden,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: vec!["memory.policy.freeze".into()],
    }
}

fn eval_case() -> EvalCase {
    EvalCase {
        id: "memory.policy.freeze".into(),
        subject: EvalSubject::Memory,
        title: "memory policy freeze".into(),
        prompt: "verify freeze".into(),
        setup: Vec::new(),
        policy: HarnessPolicy::default(),
        budgets: HarnessBudget::default(),
        assertions: Vec::new(),
        graders: Vec::new(),
    }
}

fn frozen_receipt() -> crate::memory_policy::MemoryPolicyExecutionReceipt {
    let decision = classify_memory_policy_input(input());
    crate::memory_policy::receipts::build_receipt(
        &decision,
        &decision.actions[0],
        MemoryPolicyReceiptStatus::Rejected,
        Some(MemoryPolicyReasonCode::MemoryGraphFrozen),
        Some("memory-policy://rejected/action".into()),
        Some("memory_graph:frozen".into()),
        None,
    )
}

#[test]
fn attaches_memory_policy_receipt_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = EvalRuntime::new(tmp.path());
    let episode = runtime.start_episode(&eval_case());
    let receipt = frozen_receipt();

    let artifact = attach_memory_policy_receipt(&runtime, &episode.run_id, &receipt)
        .unwrap()
        .unwrap();

    assert_eq!(artifact.kind, "memory_policy_receipt");
    let stored = runtime.get_episode(&episode.run_id).unwrap();
    assert_eq!(stored.artifacts.len(), 1);
}

#[test]
fn memory_graph_frozen_receipt_maps_to_eval_memory_write() {
    let receipt = frozen_receipt();

    let event = crate::memory_policy::receipts::receipt_to_eval_event(&receipt);

    assert_eq!(event.kind(), "memory_write");
}
