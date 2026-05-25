use super::*;

fn input(class: MemoryKnowledgeClass) -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::AgentLoop,
        source_event_id: "event-1".into(),
        task_id: "task-1".into(),
        intent_id: Some("intent-1".into()),
        content: "Ryan prefers gbrain as durable memory.".into(),
        requested_class: class,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: Vec::new(),
    }
}

#[test]
fn durable_fact_routes_to_gbrain_write() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    assert_eq!(
        decision.knowledge_class,
        MemoryKnowledgeClass::DurableKnowledge
    );
    assert_eq!(decision.actions.len(), 1);
    assert_eq!(
        decision.actions[0].kind,
        MemoryPolicyActionKind::GbrainWrite
    );
    assert_eq!(decision.actions[0].target, MemoryPolicyTarget::Gbrain);
}

#[test]
fn browser_evidence_routes_to_artifact_not_gbrain() {
    let mut event = input(MemoryKnowledgeClass::EpisodicEvidence);
    event.source = MemoryPolicySource::BrowserRuntime;
    let decision = classify_memory_policy_input(event);
    assert_eq!(decision.actions.len(), 1);
    assert_eq!(
        decision.actions[0].kind,
        MemoryPolicyActionKind::BrowserArtifactWrite
    );
}

#[test]
fn memory_graph_write_input_is_forbidden_action() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    assert_eq!(decision.knowledge_class, MemoryKnowledgeClass::Forbidden);
    assert_eq!(
        decision.actions[0].kind,
        MemoryPolicyActionKind::MemoryGraphWrite
    );
}

#[test]
fn gbrain_target_formats_slug_and_markdown() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let action = &decision.actions[0];
    let request =
        crate::memory_policy::targets::gbrain::build_gbrain_write_request(&decision, action);
    assert!(request.slug.starts_with("memory-policy/task-1/"));
    assert!(request.content.contains("type: memory_policy_receipt"));
    assert!(request
        .content
        .contains("Ryan prefers gbrain as durable memory."));
}

#[tokio::test]
async fn memory_graph_write_receipt_is_rejected() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Rejected);
    assert_eq!(
        receipts[0].reason_code,
        Some(MemoryPolicyReasonCode::MemoryGraphFrozen)
    );
}

#[tokio::test]
async fn fake_gbrain_target_succeeds_for_durable_fact() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts[0].target, MemoryPolicyTarget::Gbrain);
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Succeeded);
}

#[tokio::test]
async fn browser_artifact_target_writes_receipt_json() {
    let tmp = tempfile::tempdir().unwrap();
    let target = crate::memory_policy::targets::browser_artifact::BrowserArtifactPolicyTarget::new(
        tmp.path(),
    );
    let mut event = input(MemoryKnowledgeClass::EpisodicEvidence);
    event.source = MemoryPolicySource::BrowserRuntime;
    let decision = classify_memory_policy_input(event);
    let receipt = target
        .execute(&decision, &decision.actions[0])
        .await
        .unwrap();
    assert_eq!(receipt.status, MemoryPolicyReceiptStatus::Succeeded);
    let artifact_ref = receipt.artifact_ref.unwrap();
    assert!(artifact_ref.starts_with("file://"));
    assert!(
        std::fs::read_to_string(artifact_ref.trim_start_matches("file://"))
            .unwrap()
            .contains("sourceEventId")
    );
}

#[tokio::test]
async fn gbrain_unavailable_returns_deferred_receipt() {
    let target = crate::memory_policy::targets::gbrain::GbrainPolicyTarget::unavailable_for_tests();
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let receipt = target
        .execute(&decision, &decision.actions[0])
        .await
        .unwrap();
    assert_eq!(receipt.status, MemoryPolicyReceiptStatus::Deferred);
    assert_eq!(
        receipt.reason_code,
        Some(MemoryPolicyReasonCode::GbrainUnavailable)
    );
}

#[tokio::test]
async fn hook_denial_blocks_gbrain_target_execution() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_deny_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Rejected);
    assert_eq!(
        receipts[0].reason_code,
        Some(MemoryPolicyReasonCode::PolicyDenied)
    );
}

#[tokio::test]
async fn succeeded_receipt_maps_to_memory_write_task_event() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipt = executor.execute(decision).await.unwrap().remove(0);
    let event = receipt_to_task_event(&receipt);
    assert_eq!(event.kind(), "memory_write");
    assert_eq!(event.task_id(), "task-1");
}

#[tokio::test]
async fn rejected_receipt_maps_to_signal_task_event() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipt = executor.execute(decision).await.unwrap().remove(0);
    let event = receipt_to_task_event(&receipt);
    assert_eq!(event.kind(), "signal");
    assert_eq!(event.task_id(), "task-1");
}
