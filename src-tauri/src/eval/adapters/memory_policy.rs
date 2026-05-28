use crate::eval::artifacts::{ArtifactStoreError, HarnessArtifact};
use crate::eval::runtime::HarnessRuntime;
use crate::memory_policy::MemoryPolicyExecutionReceipt;

pub const MEMORY_POLICY_RECEIPT_ARTIFACT_KIND: &str = "memory_policy_receipt";

pub fn attach_memory_policy_receipt(
    runtime: &HarnessRuntime,
    run_id: &str,
    receipt: &MemoryPolicyExecutionReceipt,
) -> Result<Option<HarnessArtifact>, ArtifactStoreError> {
    let value = serde_json::to_value(receipt).map_err(ArtifactStoreError::Serde)?;
    runtime.attach_json_artifact(run_id, MEMORY_POLICY_RECEIPT_ARTIFACT_KIND, &value)
}
