use std::path::{Path, PathBuf};

use async_trait::async_trait;
use serde_json::json;

use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReceiptStatus,
};

#[derive(Debug, Clone)]
pub struct BrowserArtifactPolicyTarget {
    root: PathBuf,
}

impl BrowserArtifactPolicyTarget {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            root: root.as_ref().to_path_buf(),
        }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for BrowserArtifactPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        std::fs::create_dir_all(&self.root)
            .map_err(|err| MemoryPolicyTargetError::Failed(err.to_string()))?;
        let path = self.root.join(format!("{}.json", action.action_id));
        let value = json!({
            "decisionId": decision.decision_id,
            "actionId": action.action_id,
            "sourceEventId": decision.input.source_event_id,
            "taskId": decision.input.task_id,
            "knowledgeClass": decision.knowledge_class,
            "target": action.target,
            "content": decision.input.content,
        });
        std::fs::write(
            &path,
            serde_json::to_vec_pretty(&value)
                .map_err(|err| MemoryPolicyTargetError::Failed(err.to_string()))?,
        )
        .map_err(|err| MemoryPolicyTargetError::Failed(err.to_string()))?;
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Succeeded,
            None,
            Some(format!("file://{}", path.to_string_lossy())),
            Some(format!("browser_artifact:{}", action.action_id)),
            None,
        ))
    }
}
