use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::mcp::SharedMcpManager;
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
    mcp: Option<SharedMcpManager>,
}

impl GbrainPolicyTarget {
    pub fn new(mcp: SharedMcpManager) -> Self {
        Self { mcp: Some(mcp) }
    }

    pub fn unavailable_for_tests() -> Self {
        Self { mcp: None }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for GbrainPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        let Some(mcp) = self.mcp.as_ref() else {
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
            crate::gbrain::browse::put_page(mcp, &request.slug, &request.content),
        )
        .await;
        match result {
            Ok(Ok(page)) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Succeeded,
                None,
                Some(format!("gbrain://{}", page.slug)),
                Some(page.slug),
                None,
            )),
            Ok(Err(err)) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some(err.to_command_string()),
            )),
            Err(_) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some("gbrain write timed out after 5s".into()),
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
