use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::memory_graph::store::MemoryGraphStore;
use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt, MemoryPolicyReasonCode,
    MemoryPolicyReceiptStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GbrainPolicyWriteRequest {
    pub slug: String,
    pub content: String,
}

pub fn build_gbrain_write_request(
    decision: &MemoryPolicyDecision,
    action: &MemoryPolicyAction,
) -> GbrainPolicyWriteRequest {
    let slug = format!(
        "memory-policy/{}/{}",
        sanitize_slug_segment(&decision.input.task_id),
        sanitize_slug_segment(&action.action_id)
    );
    let content = format!(
        "---\ntitle: \"Memory policy write {}\"\ntype: memory_policy_receipt\ntags:\n  - memory_policy\n  - {}\ntask_id: {}\nsource_event_id: {}\n---\n\n# Memory Policy Durable Knowledge\n\n{}\n",
        yaml_escape(&action.action_id),
        action.target.as_task_event_target(),
        decision.input.task_id,
        decision.input.source_event_id,
        decision.input.content,
    );
    GbrainPolicyWriteRequest { slug, content }
}

#[derive(Clone)]
pub struct GbrainPolicyTarget {
    store: Option<Arc<MemoryGraphStore>>,
    adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
}

impl GbrainPolicyTarget {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
    ) -> Self {
        Self { store: Some(store), adapter: Some(adapter) }
    }

    pub fn unavailable_for_tests() -> Self {
        Self { store: None, adapter: None }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for GbrainPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        let (Some(store), Some(adapter)) = (self.store.as_ref(), self.adapter.as_ref()) else {
            return Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                None,
            ));
        };
        let request = build_gbrain_write_request(decision, action);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            crate::memory_adapter::page_dual_write::write_page(
                store,
                adapter,
                crate::memory_graph::DEFAULT_SPACE_ID,
                &request.slug,
                &request.content,
            ),
        )
        .await;
        match result {
            Ok(Ok(())) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Succeeded,
                None,
                // request.slug is already lowercase-normalized by sanitize_slug_segment,
                // matching entity_page_put's own to_lowercase, so it equals the stored
                // slug — safe to echo in the receipt without round-tripping the store.
                Some(format!("entity-page://{}", request.slug)),
                Some(request.slug),
                None,
            )),
            Ok(Err(err)) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some(err.to_string()),
            )),
            Err(_) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some("page write timed out after 5s".into()),
            )),
        }
    }
}

fn sanitize_slug_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "unknown".into()
    } else {
        trimmed
    }
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
