pub mod browser_artifact;
pub mod gbrain;
pub mod memory_graph;
pub mod memu;

use async_trait::async_trait;

use super::types::{MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt};

#[derive(Debug, thiserror::Error)]
pub enum MemoryPolicyTargetError {
    #[error("target unavailable: {0}")]
    Unavailable(String),
    #[error("target failed: {0}")]
    Failed(String),
}

#[async_trait]
pub trait MemoryPolicyTargetAdapter: Send + Sync {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError>;
}
