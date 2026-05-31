use crate::browser::runtime_memory_policy::{
    classify_browser_evidence, BrowserMemoryPromotionMetadata, BrowserRuntimeMemoryPolicyExecutor,
};
use crate::memory::MemoryStore;
use crate::memory_policy::{
    MemoryKnowledgeClass, MemoryPolicyActionKind, MemoryPolicyReceiptStatus, MemoryPolicyReasonCode,
    MemoryPolicySource,
};
use std::sync::{Arc, Mutex};

fn memory_store() -> Arc<MemoryStore> {
    let conn = Arc::new(Mutex::new(rusqlite::Connection::open_in_memory().unwrap()));
    let store = Arc::new(MemoryStore::new(conn));
    store.ensure_table();
    store
}

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

#[tokio::test]
async fn executor_writes_browser_evidence_to_memory_store_artifact() {
    let store = memory_store();
    let executor = BrowserRuntimeMemoryPolicyExecutor::new(store.clone(), None);
    let decision = classify_browser_evidence(
        "event-4",
        "task-1",
        "{\"visualObservation\":{\"ocrText\":[{\"text\":\"captcha\"}]}}",
        None,
    );

    let receipts = executor.execute_decision(&decision).await;

    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Succeeded);
    let hits = store.search_full("captcha", Some("browser_task"), None, None, 10);
    assert_eq!(hits.len(), 1);
}

#[tokio::test]
async fn executor_defers_promoted_gbrain_when_manager_unavailable() {
    let store = memory_store();
    let executor = BrowserRuntimeMemoryPolicyExecutor::new(store, None);
    let decision = classify_browser_evidence(
        "event-5",
        "task-1",
        "stable selector: button[type=submit]",
        Some(BrowserMemoryPromotionMetadata {
            redaction_clean: true,
            approval_ref: Some("approval-1".into()),
            harness_case_ids: vec![],
        }),
    );

    let receipts = executor.execute_decision(&decision).await;

    assert!(receipts.iter().any(|receipt| {
        receipt.action == MemoryPolicyActionKind::GbrainWrite
            && receipt.status == MemoryPolicyReceiptStatus::Deferred
            && receipt.reason_code == Some(MemoryPolicyReasonCode::GbrainUnavailable)
    }));
}
