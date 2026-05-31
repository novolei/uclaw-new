use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use serde_json::json;

use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReceiptStatus,
};
use crate::memory::{MemoryKind, MemoryStore, SetMemoryOpts};

#[derive(Clone)]
pub struct BrowserArtifactPolicyTarget {
    backend: BrowserArtifactPolicyBackend,
}

#[derive(Clone)]
enum BrowserArtifactPolicyBackend {
    FileRoot(PathBuf),
    MemoryStore {
        store: Arc<MemoryStore>,
        namespace: String,
    },
}

impl BrowserArtifactPolicyTarget {
    pub fn new(root: impl AsRef<Path>) -> Self {
        Self {
            backend: BrowserArtifactPolicyBackend::FileRoot(root.as_ref().to_path_buf()),
        }
    }

    pub fn new_memory_store(store: Arc<MemoryStore>, namespace: impl Into<String>) -> Self {
        Self {
            backend: BrowserArtifactPolicyBackend::MemoryStore {
                store,
                namespace: namespace.into(),
            },
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
        let value = json!({
            "decisionId": decision.decision_id,
            "actionId": action.action_id,
            "sourceEventId": decision.input.source_event_id,
            "taskId": decision.input.task_id,
            "knowledgeClass": decision.knowledge_class,
            "target": action.target,
            "content": decision.input.content,
        });
        match &self.backend {
            BrowserArtifactPolicyBackend::FileRoot(root) => {
                std::fs::create_dir_all(root)
                    .map_err(|err| MemoryPolicyTargetError::Failed(err.to_string()))?;
                let path = root.join(format!("{}.json", action.action_id));
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
            BrowserArtifactPolicyBackend::MemoryStore { store, namespace } => {
                let key = format!(
                    "{}:{}",
                    decision.input.source_event_id, action.action_id
                );
                store
                    .set_full(SetMemoryOpts {
                        space_id: "global".to_string(),
                        namespace: namespace.clone(),
                        key: key.clone(),
                        value,
                        kind: MemoryKind::Context,
                        tags: vec![
                            "browser_task".to_string(),
                            "memory_policy".to_string(),
                            format!("target:{}", action.target.as_task_event_target()),
                            format!("task:{}", decision.input.task_id),
                        ],
                        metadata: Some(json!({
                            "decisionId": decision.decision_id,
                            "actionId": action.action_id,
                            "sourceEventId": decision.input.source_event_id,
                            "target": action.target,
                        })),
                        ttl_seconds: None,
                    })
                    .map_err(|err| MemoryPolicyTargetError::Failed(err.to_string()))?;
                Ok(build_receipt(
                    decision,
                    action,
                    MemoryPolicyReceiptStatus::Succeeded,
                    None,
                    Some(format!("memory://{namespace}/{key}")),
                    Some(format!("browser_artifact:{key}")),
                    None,
                ))
            }
        }
    }
}
