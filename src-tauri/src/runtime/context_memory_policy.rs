use crate::memory_policy::MemoryPolicyExecutionReceipt;
use crate::runtime::context::{Citation, ContextArtifact, ContextRef, ContextSource};

pub fn memory_receipt_to_context_artifact(
    receipt: &MemoryPolicyExecutionReceipt,
    content: impl Into<String>,
) -> ContextArtifact {
    let target = receipt.target.as_task_event_target();
    ContextArtifact {
        r#ref: ContextRef::new(
            ContextSource::Memory,
            format!("memory-policy/{}/{}", target, receipt.receipt_id),
        )
        .with_label(format!("memory policy {}", target)),
        content: content.into(),
        citations: vec![Citation {
            line: None,
            evidence_ref: receipt
                .artifact_ref
                .clone()
                .unwrap_or_else(|| format!("memory-policy://receipt/{}", receipt.receipt_id)),
        }],
        retrieval_ts: chrono::Utc::now().to_rfc3339(),
    }
}
