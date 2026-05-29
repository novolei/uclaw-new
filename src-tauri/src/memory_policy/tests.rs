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
