use std::sync::Arc;

use async_trait::async_trait;

use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReceiptStatus,
};
use crate::memu::client::MemUClient;

#[derive(Clone)]
pub struct MemuPolicyTarget {
    client: Option<Arc<MemUClient>>,
}

impl MemuPolicyTarget {
    pub fn new(client: Arc<MemUClient>) -> Self {
        Self {
            client: Some(client),
        }
    }

    pub fn unavailable_for_tests() -> Self {
        Self { client: None }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for MemuPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        let Some(_client) = self.client.as_ref() else {
            return Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Degraded,
                None,
                Some(format!("memory-policy://degraded/{}", action.action_id)),
                Some("memu:unavailable".into()),
                None,
            ));
        };
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Queued,
            None,
            Some(format!("memory-policy://queued/{}", action.action_id)),
            Some("memu:queued".into()),
            None,
        ))
    }
}
