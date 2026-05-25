use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKnowledgeClass {
    DurableKnowledge,
    EpisodicEvidence,
    ScratchContext,
    AuxiliaryRecall,
    LegacyRead,
    Forbidden,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicySource {
    AgentLoop,
    BrowserRuntime,
    Automation,
    ContextFabric,
    Harness,
    TauriCommand,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyTarget {
    Gbrain,
    Memu,
    BrowserArtifact,
    MemoryGraph,
}

impl MemoryPolicyTarget {
    pub fn as_task_event_target(self) -> &'static str {
        match self {
            Self::Gbrain => "gbrain",
            Self::Memu => "memu",
            Self::BrowserArtifact => "browser_artifact",
            Self::MemoryGraph => "memory_graph",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyActionKind {
    GbrainWrite,
    MemuWriteOrIndex,
    BrowserArtifactWrite,
    MemoryGraphRead,
    MemoryGraphWrite,
}

impl MemoryPolicyActionKind {
    pub fn target(self) -> MemoryPolicyTarget {
        match self {
            Self::GbrainWrite => MemoryPolicyTarget::Gbrain,
            Self::MemuWriteOrIndex => MemoryPolicyTarget::Memu,
            Self::BrowserArtifactWrite => MemoryPolicyTarget::BrowserArtifact,
            Self::MemoryGraphRead | Self::MemoryGraphWrite => MemoryPolicyTarget::MemoryGraph,
        }
    }

    pub fn is_write(self) -> bool {
        !matches!(self, Self::MemoryGraphRead)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyExecutionMode {
    Synchronous,
    BoundedAwait,
    Queued,
    RejectOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyReceiptStatus {
    Planned,
    Allowed,
    Queued,
    Succeeded,
    Deferred,
    Degraded,
    Rejected,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryPolicyReasonCode {
    MemoryGraphFrozen,
    PolicyDenied,
    ApprovalRequired,
    GbrainUnavailable,
    QueuedForBackgroundWrite,
    RedactionRequired,
    PromotionRejectedOrDeferred,
    TargetError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPolicyInput {
    pub source: MemoryPolicySource,
    pub source_event_id: String,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    pub content: String,
    pub requested_class: MemoryKnowledgeClass,
    #[serde(default)]
    pub promoted: bool,
    #[serde(default)]
    pub redaction_clean: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval_ref: Option<String>,
    #[serde(default)]
    pub harness_case_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPolicyAction {
    pub action_id: String,
    pub kind: MemoryPolicyActionKind,
    pub target: MemoryPolicyTarget,
    pub execution_mode: MemoryPolicyExecutionMode,
    pub topic: String,
    pub size_bytes: usize,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPolicyDecision {
    pub decision_id: String,
    pub input: MemoryPolicyInput,
    pub knowledge_class: MemoryKnowledgeClass,
    pub actions: Vec<MemoryPolicyAction>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryPolicyExecutionReceipt {
    pub receipt_id: String,
    pub decision_id: String,
    pub action_id: String,
    pub source: MemoryPolicySource,
    pub source_event_id: String,
    pub task_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intent_id: Option<String>,
    pub correlation_id: String,
    pub knowledge_class: MemoryKnowledgeClass,
    pub action: MemoryPolicyActionKind,
    pub target: MemoryPolicyTarget,
    pub status: MemoryPolicyReceiptStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<MemoryPolicyReasonCode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_ref: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_ref: Option<String>,
    pub idempotency_key: String,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
