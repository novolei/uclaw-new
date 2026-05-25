use async_trait::async_trait;

use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyActionKind, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReasonCode, MemoryPolicyReceiptStatus,
};

#[derive(Debug, Default)]
pub struct MemoryGraphPolicyTarget;

#[async_trait]
impl MemoryPolicyTargetAdapter for MemoryGraphPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        if action.kind == MemoryPolicyActionKind::MemoryGraphRead {
            return Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Succeeded,
                None,
                Some(format!("memory-policy://legacy-read/{}", action.action_id)),
                Some("memory_graph:legacy_read".into()),
                None,
            ));
        }
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Rejected,
            Some(MemoryPolicyReasonCode::MemoryGraphFrozen),
            Some(format!("memory-policy://rejected/{}", action.action_id)),
            Some("memory_graph:frozen".into()),
            None,
        ))
    }
}
