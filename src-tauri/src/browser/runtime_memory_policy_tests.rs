use crate::browser::runtime_memory_policy::{
    classify_browser_evidence, BrowserMemoryPromotionMetadata,
};
use crate::memory_policy::{MemoryKnowledgeClass, MemoryPolicyActionKind, MemoryPolicySource};

#[test]
fn browser_checkpoint_defaults_to_artifact_evidence() {
    let decision = classify_browser_evidence("event-1", "task-1", "checkpoint payload", None);
    assert_eq!(decision.input.source, MemoryPolicySource::BrowserRuntime);
    assert_eq!(
        decision.knowledge_class,
        MemoryKnowledgeClass::EpisodicEvidence
    );
    assert_eq!(
        decision.actions[0].kind,
        MemoryPolicyActionKind::BrowserArtifactWrite
    );
}

#[test]
fn promoted_browser_knowledge_adds_gbrain_write() {
    let decision = classify_browser_evidence(
        "event-2",
        "task-1",
        "stable selector: button[type=submit]",
        Some(BrowserMemoryPromotionMetadata {
            redaction_clean: true,
            approval_ref: Some("approval-1".into()),
            harness_case_ids: vec!["browser.login.replay".into()],
        }),
    );
    let kinds: Vec<_> = decision.actions.iter().map(|action| action.kind).collect();
    assert!(kinds.contains(&MemoryPolicyActionKind::BrowserArtifactWrite));
    assert!(kinds.contains(&MemoryPolicyActionKind::GbrainWrite));
}

#[test]
fn unpromoted_browser_payload_never_adds_gbrain_action() {
    let decision = classify_browser_evidence(
        "event-3",
        "task-1",
        "{\"screenshotRef\":\"browser://shot\"}",
        None,
    );
    assert!(!decision
        .actions
        .iter()
        .any(|action| action.kind == MemoryPolicyActionKind::GbrainWrite));
}
