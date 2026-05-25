use uuid::Uuid;

use super::types::{
    MemoryKnowledgeClass, MemoryPolicyAction, MemoryPolicyActionKind, MemoryPolicyDecision,
    MemoryPolicyExecutionMode, MemoryPolicyInput, MemoryPolicySource,
};

pub fn classify_memory_policy_input(input: MemoryPolicyInput) -> MemoryPolicyDecision {
    let decision_id = format!("decision-{}", Uuid::new_v4());
    let action_kind = match input.requested_class {
        MemoryKnowledgeClass::DurableKnowledge => MemoryPolicyActionKind::GbrainWrite,
        MemoryKnowledgeClass::EpisodicEvidence => MemoryPolicyActionKind::BrowserArtifactWrite,
        MemoryKnowledgeClass::ScratchContext => MemoryPolicyActionKind::BrowserArtifactWrite,
        MemoryKnowledgeClass::AuxiliaryRecall => MemoryPolicyActionKind::MemuWriteOrIndex,
        MemoryKnowledgeClass::LegacyRead => MemoryPolicyActionKind::MemoryGraphRead,
        MemoryKnowledgeClass::Forbidden => MemoryPolicyActionKind::MemoryGraphWrite,
    };
    let execution_mode = match action_kind {
        MemoryPolicyActionKind::BrowserArtifactWrite | MemoryPolicyActionKind::MemoryGraphRead => {
            MemoryPolicyExecutionMode::Synchronous
        }
        MemoryPolicyActionKind::GbrainWrite | MemoryPolicyActionKind::MemuWriteOrIndex => {
            MemoryPolicyExecutionMode::BoundedAwait
        }
        MemoryPolicyActionKind::MemoryGraphWrite => MemoryPolicyExecutionMode::RejectOnly,
    };
    let topic = topic_for(&input);
    let action_id = format!("action-{}", Uuid::new_v4());
    let action = MemoryPolicyAction {
        action_id,
        kind: action_kind,
        target: action_kind.target(),
        execution_mode,
        topic,
        size_bytes: input.content.len(),
        idempotency_key: format!(
            "{}:{}:{}",
            input.source_event_id,
            action_kind.target().as_task_event_target(),
            class_key(input.requested_class)
        ),
    };
    MemoryPolicyDecision {
        decision_id,
        knowledge_class: input.requested_class,
        input,
        actions: vec![action],
    }
}

fn topic_for(input: &MemoryPolicyInput) -> String {
    match input.source {
        MemoryPolicySource::BrowserRuntime => "browser_evidence".into(),
        MemoryPolicySource::ContextFabric => "context_recall".into(),
        _ => input
            .content
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" "),
    }
}

fn class_key(class: MemoryKnowledgeClass) -> &'static str {
    match class {
        MemoryKnowledgeClass::DurableKnowledge => "durable_knowledge",
        MemoryKnowledgeClass::EpisodicEvidence => "episodic_evidence",
        MemoryKnowledgeClass::ScratchContext => "scratch_context",
        MemoryKnowledgeClass::AuxiliaryRecall => "auxiliary_recall",
        MemoryKnowledgeClass::LegacyRead => "legacy_read",
        MemoryKnowledgeClass::Forbidden => "forbidden",
    }
}
