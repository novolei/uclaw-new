// SPDX-License-Identifier: Apache-2.0
//! memU memory-policy target — stub adapter (memU removed, Step 3b-4).
//!
//! The MemuPolicyTarget is retained as a no-op stub so callers that were
//! already stubbed out by earlier teardown tasks compile without changes.
//! All execute() calls return Degraded immediately.

use async_trait::async_trait;

use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReceiptStatus,
};

#[derive(Clone)]
pub struct MemuPolicyTarget;

impl MemuPolicyTarget {
    pub fn new() -> Self {
        Self
    }

    pub fn unavailable_for_tests() -> Self {
        Self
    }
}

impl Default for MemuPolicyTarget {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for MemuPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Degraded,
            None,
            Some(format!("memory-policy://degraded/{}", action.action_id)),
            Some("memu:removed".into()),
            None,
        ))
    }
}
