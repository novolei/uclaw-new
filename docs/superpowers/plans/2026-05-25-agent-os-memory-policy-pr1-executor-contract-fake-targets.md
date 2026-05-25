# Agent OS Memory Policy PR1 Executor Contract And Fake Targets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the additive `memory_policy` contract, classifier, executor, fake targets, memory_graph rejection target, receipts, and TaskEvent mapping.

**Architecture:** PR1 is pure Rust contract work with no real gbrain, memU, or browser artifact side effects. The executor runs explicit `MemoryPolicyDecision.actions`, gates writes through hook decisions, emits receipts, and rejects memory_graph writes even when a hook allows them.

**Tech Stack:** Rust, `async-trait`, `serde`, `serde_json`, `uuid`, `chrono`, `thiserror`, existing `HookBus`, existing `TaskEvent`.

---

## File Structure

- Modify: `src-tauri/src/lib.rs`
  - Add `pub mod memory_policy;`.
- Create: `src-tauri/src/memory_policy/mod.rs`
  - Module exports.
- Create: `src-tauri/src/memory_policy/types.rs`
  - Input, classification, action, target, status, execution mode, and receipt types.
- Create: `src-tauri/src/memory_policy/classifier.rs`
  - Deterministic input-to-action classification.
- Create: `src-tauri/src/memory_policy/receipts.rs`
  - Receipt constructors and `TaskEvent` conversion helpers.
- Create: `src-tauri/src/memory_policy/executor.rs`
  - Hook-gated action fan-out and aggregate execution.
- Create: `src-tauri/src/memory_policy/targets/mod.rs`
  - Target adapter trait plus fake adapters.
- Create: `src-tauri/src/memory_policy/targets/memory_graph.rs`
  - Legacy read target and write rejection target.
- Create: `src-tauri/src/memory_policy/tests.rs`
  - Unit tests for classification, gate behavior, receipts, and TaskEvent mapping.

## Task 1: Register The Module And Core Types

**Files:**
- Modify: `src-tauri/src/lib.rs`
- Create: `src-tauri/src/memory_policy/mod.rs`
- Create: `src-tauri/src/memory_policy/types.rs`

- [x] **Step 1: Run the missing module test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: FAIL or zero tests because `memory_policy` does not exist.

- [x] **Step 2: Add module registration**

In `src-tauri/src/lib.rs`, add near `pub mod memory_contract;`:

```rust
// Agent OS Memory Policy spine.
pub mod memory_policy;
```

- [x] **Step 3: Create module exports**

Create `src-tauri/src/memory_policy/mod.rs`:

```rust
pub mod classifier;
pub mod executor;
pub mod receipts;
pub mod targets;
pub mod types;

#[cfg(test)]
mod tests;

pub use classifier::classify_memory_policy_input;
pub use executor::{MemoryPolicyExecutor, MemoryPolicyExecutorError};
pub use receipts::{receipt_artifact_ref, receipt_to_task_event};
pub use types::*;
```

- [x] **Step 4: Create core types**

Create `src-tauri/src/memory_policy/types.rs` with these definitions:

```rust
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
```

- [x] **Step 5: Run compile check**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: compile failure for missing `classifier`, `executor`, `receipts`, and `targets` modules.

## Task 2: Add Classifier And Receipt Helpers

**Files:**
- Create: `src-tauri/src/memory_policy/classifier.rs`
- Create: `src-tauri/src/memory_policy/receipts.rs`
- Test: `src-tauri/src/memory_policy/tests.rs`

- [x] **Step 1: Write classifier tests**

Create `src-tauri/src/memory_policy/tests.rs` with:

```rust
use super::*;

fn input(class: MemoryKnowledgeClass) -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::AgentLoop,
        source_event_id: "event-1".into(),
        task_id: "task-1".into(),
        intent_id: Some("intent-1".into()),
        content: "Ryan prefers gbrain as durable memory.".into(),
        requested_class: class,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: Vec::new(),
    }
}

#[test]
fn durable_fact_routes_to_gbrain_write() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    assert_eq!(decision.knowledge_class, MemoryKnowledgeClass::DurableKnowledge);
    assert_eq!(decision.actions.len(), 1);
    assert_eq!(decision.actions[0].kind, MemoryPolicyActionKind::GbrainWrite);
    assert_eq!(decision.actions[0].target, MemoryPolicyTarget::Gbrain);
}

#[test]
fn browser_evidence_routes_to_artifact_not_gbrain() {
    let mut event = input(MemoryKnowledgeClass::EpisodicEvidence);
    event.source = MemoryPolicySource::BrowserRuntime;
    let decision = classify_memory_policy_input(event);
    assert_eq!(decision.actions.len(), 1);
    assert_eq!(decision.actions[0].kind, MemoryPolicyActionKind::BrowserArtifactWrite);
}

#[test]
fn memory_graph_write_input_is_forbidden_action() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    assert_eq!(decision.knowledge_class, MemoryKnowledgeClass::Forbidden);
    assert_eq!(decision.actions[0].kind, MemoryPolicyActionKind::MemoryGraphWrite);
}
```

- [x] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::durable_fact_routes_to_gbrain_write --lib
```

Expected: FAIL because `classify_memory_policy_input` is not defined.

- [x] **Step 3: Implement classifier**

Create `src-tauri/src/memory_policy/classifier.rs`:

```rust
use uuid::Uuid;

use super::types::{
    MemoryKnowledgeClass, MemoryPolicyAction, MemoryPolicyActionKind, MemoryPolicyDecision,
    MemoryPolicyExecutionMode, MemoryPolicyInput, MemoryPolicySource,
};

pub fn classify_memory_policy_input(input: MemoryPolicyInput) -> MemoryPolicyDecision {
    let decision_id = format!("decision-{}", Uuid::new_v4());
    let action_kind = match input.requested_class {
        MemoryKnowledgeClass::DurableKnowledge => MemoryPolicyActionKind::GbrainWrite,
        MemoryKnowledgeClass::EpisodicEvidence => MemoryPolicyActionKind::BrowserArtifactWrite,
        MemoryKnowledgeClass::ScratchContext => MemoryPolicyActionKind::BrowserArtifactWrite,
        MemoryKnowledgeClass::AuxiliaryRecall => MemoryPolicyActionKind::MemuWriteOrIndex,
        MemoryKnowledgeClass::LegacyRead => MemoryPolicyActionKind::MemoryGraphRead,
        MemoryKnowledgeClass::Forbidden => MemoryPolicyActionKind::MemoryGraphWrite,
    };
    let execution_mode = match action_kind {
        MemoryPolicyActionKind::BrowserArtifactWrite | MemoryPolicyActionKind::MemoryGraphRead => {
            MemoryPolicyExecutionMode::Synchronous
        }
        MemoryPolicyActionKind::GbrainWrite | MemoryPolicyActionKind::MemuWriteOrIndex => {
            MemoryPolicyExecutionMode::BoundedAwait
        }
        MemoryPolicyActionKind::MemoryGraphWrite => MemoryPolicyExecutionMode::RejectOnly,
    };
    let topic = topic_for(&input);
    let action_id = format!("action-{}", Uuid::new_v4());
    let action = MemoryPolicyAction {
        action_id,
        kind: action_kind,
        target: action_kind.target(),
        execution_mode,
        topic,
        size_bytes: input.content.len(),
        idempotency_key: format!(
            "{}:{}:{}",
            input.source_event_id,
            action_kind.target().as_task_event_target(),
            input.requested_class as u8
        ),
    };
    MemoryPolicyDecision {
        decision_id,
        knowledge_class: input.requested_class,
        input,
        actions: vec![action],
    }
}

fn topic_for(input: &MemoryPolicyInput) -> String {
    match input.source {
        MemoryPolicySource::BrowserRuntime => "browser_evidence".into(),
        MemoryPolicySource::ContextFabric => "context_recall".into(),
        _ => input
            .content
            .split_whitespace()
            .take(6)
            .collect::<Vec<_>>()
            .join(" "),
    }
}
```

- [x] **Step 4: Replace the enum cast idempotency expression**

In `classifier.rs`, replace `input.requested_class as u8` with this helper to keep the code stable:

```rust
fn class_key(class: MemoryKnowledgeClass) -> &'static str {
    match class {
        MemoryKnowledgeClass::DurableKnowledge => "durable_knowledge",
        MemoryKnowledgeClass::EpisodicEvidence => "episodic_evidence",
        MemoryKnowledgeClass::ScratchContext => "scratch_context",
        MemoryKnowledgeClass::AuxiliaryRecall => "auxiliary_recall",
        MemoryKnowledgeClass::LegacyRead => "legacy_read",
        MemoryKnowledgeClass::Forbidden => "forbidden",
    }
}
```

Use:

```rust
idempotency_key: format!(
    "{}:{}:{}",
    input.source_event_id,
    action_kind.target().as_task_event_target(),
    class_key(input.requested_class)
),
```

- [x] **Step 5: Implement receipt helpers**

Create `src-tauri/src/memory_policy/receipts.rs`:

```rust
use chrono::Utc;
use uuid::Uuid;

use crate::runtime::contracts::{TaskEvent, TaskEventSource};

use super::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReasonCode, MemoryPolicyReceiptStatus,
};

pub fn build_receipt(
    decision: &MemoryPolicyDecision,
    action: &MemoryPolicyAction,
    status: MemoryPolicyReceiptStatus,
    reason_code: Option<MemoryPolicyReasonCode>,
    artifact_ref: Option<String>,
    target_ref: Option<String>,
    error: Option<String>,
) -> MemoryPolicyExecutionReceipt {
    let now = Utc::now().to_rfc3339();
    MemoryPolicyExecutionReceipt {
        receipt_id: format!("receipt-{}", Uuid::new_v4()),
        decision_id: decision.decision_id.clone(),
        action_id: action.action_id.clone(),
        source: decision.input.source,
        source_event_id: decision.input.source_event_id.clone(),
        task_id: decision.input.task_id.clone(),
        intent_id: decision.input.intent_id.clone(),
        correlation_id: format!("{}:{}", decision.input.source_event_id, action.action_id),
        knowledge_class: decision.knowledge_class,
        action: action.kind,
        target: action.target,
        status,
        reason_code,
        artifact_ref,
        target_ref,
        idempotency_key: action.idempotency_key.clone(),
        created_at: now.clone(),
        completed_at: if matches!(status, MemoryPolicyReceiptStatus::Succeeded | MemoryPolicyReceiptStatus::Rejected | MemoryPolicyReceiptStatus::Failed) {
            Some(now)
        } else {
            None
        },
        error,
    }
}

pub fn receipt_artifact_ref(receipt: &MemoryPolicyExecutionReceipt) -> String {
    receipt
        .artifact_ref
        .clone()
        .unwrap_or_else(|| format!("memory-policy://receipt/{}", receipt.receipt_id))
}

pub fn receipt_to_task_event(receipt: &MemoryPolicyExecutionReceipt) -> TaskEvent {
    let source = TaskEventSource::Memory;
    let artifact_ref = receipt_artifact_ref(receipt);
    match receipt.status {
        MemoryPolicyReceiptStatus::Succeeded => TaskEvent::MemoryWrite {
            ts: receipt.created_at.clone(),
            source,
            task_id: receipt.task_id.clone(),
            target: receipt.target.as_task_event_target().into(),
            artifact_ref,
        },
        MemoryPolicyReceiptStatus::Rejected | MemoryPolicyReceiptStatus::Deferred => {
            TaskEvent::Signal {
                ts: receipt.created_at.clone(),
                source,
                task_id: receipt.task_id.clone(),
                code: format!("{:?}", receipt.status).to_ascii_lowercase(),
                message: format!(
                    "memory policy {:?} for target {}",
                    receipt.status,
                    receipt.target.as_task_event_target()
                ),
            }
        }
        _ => TaskEvent::Warning {
            ts: receipt.created_at.clone(),
            source,
            task_id: receipt.task_id.clone(),
            code: "memory_policy_non_terminal".into(),
            message: format!("memory policy status {:?}", receipt.status),
        },
    }
}
```

- [x] **Step 6: Run classifier tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::durable_fact_routes_to_gbrain_write --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::browser_evidence_routes_to_artifact_not_gbrain --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::memory_graph_write_input_is_forbidden_action --lib
```

Expected: PASS for all three tests.

## Task 3: Add Target Trait, Fake Targets, And MemoryGraph Rejection

**Files:**
- Create: `src-tauri/src/memory_policy/targets/mod.rs`
- Create: `src-tauri/src/memory_policy/targets/memory_graph.rs`
- Test: `src-tauri/src/memory_policy/tests.rs`

- [x] **Step 1: Add target execution tests**

Append tests:

```rust
#[tokio::test]
async fn memory_graph_write_receipt_is_rejected() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts.len(), 1);
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Rejected);
    assert_eq!(receipts[0].reason_code, Some(MemoryPolicyReasonCode::MemoryGraphFrozen));
}

#[tokio::test]
async fn fake_gbrain_target_succeeds_for_durable_fact() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts[0].target, MemoryPolicyTarget::Gbrain);
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Succeeded);
}
```

- [x] **Step 2: Add target trait and fake adapter**

Create `src-tauri/src/memory_policy/targets/mod.rs`:

```rust
pub mod memory_graph;

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
```

- [x] **Step 3: Add memory_graph target**

Create `src-tauri/src/memory_policy/targets/memory_graph.rs`:

```rust
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
```

## Task 4: Add Executor And Hook Gate

**Files:**
- Create: `src-tauri/src/memory_policy/executor.rs`
- Test: `src-tauri/src/memory_policy/tests.rs`

- [x] **Step 1: Add hook denial test**

Append:

```rust
#[tokio::test]
async fn hook_denial_blocks_gbrain_target_execution() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_deny_all();
    let receipts = executor.execute(decision).await.unwrap();
    assert_eq!(receipts[0].status, MemoryPolicyReceiptStatus::Rejected);
    assert_eq!(receipts[0].reason_code, Some(MemoryPolicyReasonCode::PolicyDenied));
}
```

- [x] **Step 2: Implement executor**

Create `src-tauri/src/memory_policy/executor.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;

use crate::agent::hook_bus::{HookBus, HookEvent};
use crate::runtime::contracts::HookDecision;

use super::receipts::build_receipt;
use super::targets::memory_graph::MemoryGraphPolicyTarget;
use super::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use super::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReasonCode, MemoryPolicyReceiptStatus, MemoryPolicyTarget,
};

#[derive(Debug, thiserror::Error)]
pub enum MemoryPolicyExecutorError {
    #[error("target error: {0}")]
    Target(#[from] MemoryPolicyTargetError),
}

pub struct MemoryPolicyExecutor {
    hook_bus: HookBus,
    gbrain: Arc<dyn MemoryPolicyTargetAdapter>,
    memu: Arc<dyn MemoryPolicyTargetAdapter>,
    browser_artifact: Arc<dyn MemoryPolicyTargetAdapter>,
    memory_graph: Arc<dyn MemoryPolicyTargetAdapter>,
}

impl MemoryPolicyExecutor {
    pub fn new(
        hook_bus: HookBus,
        gbrain: Arc<dyn MemoryPolicyTargetAdapter>,
        memu: Arc<dyn MemoryPolicyTargetAdapter>,
        browser_artifact: Arc<dyn MemoryPolicyTargetAdapter>,
    ) -> Self {
        Self {
            hook_bus,
            gbrain,
            memu,
            browser_artifact,
            memory_graph: Arc::new(MemoryGraphPolicyTarget),
        }
    }

    pub fn for_tests_allow_all() -> Self {
        Self::new(
            HookBus::new(),
            Arc::new(FakeTarget::succeeded("gbrain")),
            Arc::new(FakeTarget::succeeded("memu")),
            Arc::new(FakeTarget::succeeded("browser_artifact")),
        )
    }

    pub fn for_tests_deny_all() -> Self {
        let mut bus = HookBus::new();
        bus.register(Arc::new(DenyMemoryWrites)).unwrap();
        Self::new(
            bus,
            Arc::new(FakeTarget::succeeded("gbrain")),
            Arc::new(FakeTarget::succeeded("memu")),
            Arc::new(FakeTarget::succeeded("browser_artifact")),
        )
    }

    pub async fn execute(
        &mut self,
        decision: MemoryPolicyDecision,
    ) -> Result<Vec<MemoryPolicyExecutionReceipt>, MemoryPolicyExecutorError> {
        let mut receipts = Vec::new();
        for action in &decision.actions {
            if action.kind.is_write() && action.target != MemoryPolicyTarget::MemoryGraph {
                match self.gate_write(&decision, action).await {
                    HookDecision::Deny { .. } => {
                        receipts.push(build_receipt(
                            &decision,
                            action,
                            MemoryPolicyReceiptStatus::Rejected,
                            Some(MemoryPolicyReasonCode::PolicyDenied),
                            Some(format!("memory-policy://rejected/{}", action.action_id)),
                            None,
                            None,
                        ));
                        continue;
                    }
                    HookDecision::AskUser { .. } => {
                        receipts.push(build_receipt(
                            &decision,
                            action,
                            MemoryPolicyReceiptStatus::Deferred,
                            Some(MemoryPolicyReasonCode::ApprovalRequired),
                            Some(format!("memory-policy://deferred/{}", action.action_id)),
                            None,
                            None,
                        ));
                        continue;
                    }
                    HookDecision::Allow => {}
                }
            }
            receipts.push(self.target_for(action).execute(&decision, action).await?);
        }
        Ok(receipts)
    }

    async fn gate_write(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> HookDecision {
        let event = HookEvent::MemoryWrite {
            task_id: decision.input.task_id.clone(),
            topic: action.topic.clone(),
            size_bytes: action.size_bytes,
        };
        self.hook_bus.dispatch_with_decision(&event).await
    }

    fn target_for(&self, action: &MemoryPolicyAction) -> &dyn MemoryPolicyTargetAdapter {
        match action.target {
            MemoryPolicyTarget::Gbrain => self.gbrain.as_ref(),
            MemoryPolicyTarget::Memu => self.memu.as_ref(),
            MemoryPolicyTarget::BrowserArtifact => self.browser_artifact.as_ref(),
            MemoryPolicyTarget::MemoryGraph => self.memory_graph.as_ref(),
        }
    }
}

#[derive(Debug)]
struct FakeTarget {
    target_ref: String,
}

impl FakeTarget {
    fn succeeded(target_ref: impl Into<String>) -> Self {
        Self {
            target_ref: target_ref.into(),
        }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for FakeTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        Ok(build_receipt(
            decision,
            action,
            MemoryPolicyReceiptStatus::Succeeded,
            None,
            Some(format!("memory-policy://receipt/{}", action.action_id)),
            Some(self.target_ref.clone()),
            None,
        ))
    }
}

struct DenyMemoryWrites;

#[async_trait]
impl crate::agent::hook_bus::HookSubscriber for DenyMemoryWrites {
    fn id(&self) -> crate::agent::hook_bus::SubscriberId {
        crate::agent::hook_bus::SubscriberId::new("deny-memory-writes")
    }

    fn interest_in(&self) -> &'static [crate::agent::hook_bus::HookEventKind] {
        &[crate::agent::hook_bus::HookEventKind::MemoryWrite]
    }

    async fn on_event(&self, _event: &HookEvent) -> Option<HookDecision> {
        Some(HookDecision::Deny {
            reason: "test denial".into(),
        })
    }
}
```

- [x] **Step 3: Run executor tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::memory_graph_write_receipt_is_rejected --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::fake_gbrain_target_succeeds_for_durable_fact --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::hook_denial_blocks_gbrain_target_execution --lib
```

Expected: PASS for all three tests.

## Task 5: TaskEvent Mapping Tests

**Files:**
- Modify: `src-tauri/src/memory_policy/tests.rs`
- Modify: `src-tauri/src/memory_policy/receipts.rs`

- [x] **Step 1: Add mapping tests**

Append:

```rust
#[tokio::test]
async fn succeeded_receipt_maps_to_memory_write_task_event() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipt = executor.execute(decision).await.unwrap().remove(0);
    let event = receipt_to_task_event(&receipt);
    assert_eq!(event.kind(), "memory_write");
    assert_eq!(event.task_id(), "task-1");
}

#[tokio::test]
async fn rejected_receipt_maps_to_signal_task_event() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::Forbidden));
    let mut executor = MemoryPolicyExecutor::for_tests_allow_all();
    let receipt = executor.execute(decision).await.unwrap().remove(0);
    let event = receipt_to_task_event(&receipt);
    assert_eq!(event.kind(), "signal");
    assert_eq!(event.task_id(), "task-1");
}
```

- [x] **Step 2: Run mapping tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::succeeded_receipt_maps_to_memory_write_task_event --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::rejected_receipt_maps_to_signal_task_event --lib
```

Expected: PASS.

## Task 6: Full PR1 Verification And Commit

**Files:**
- All PR1 files listed above.

- [x] **Step 1: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: PASS for all memory_policy tests.

- [x] **Step 2: Run formatting**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git diff --check -- src-tauri/src/lib.rs src-tauri/src/memory_policy
```

Expected: no diff-check output.

- [x] **Step 3: Run GitNexus detect-changes**

Use GitNexus `detect_changes` with `scope=staged` after staging only PR1 files.

Expected: changed symbols are limited to the new `memory_policy` module and `src-tauri/src/lib.rs` module export. HIGH/CRITICAL risk requires stopping for review before commit.

- [x] **Step 4: Commit PR1**

Run:

```bash
git add src-tauri/src/lib.rs src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr1-executor-contract-fake-targets.md
git commit -m "feat(agent-os): add memory policy executor contract" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib; cargo fmt --manifest-path src-tauri/Cargo.toml; git diff --check; GitNexus detect_changes scope=staged."
```

Expected: commit succeeds.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr1-executor-contract-fake-targets.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch a fresh subagent for PR1, review the contract carefully, then proceed to PR2.

**2. Inline Execution** - execute PR1 in this session using executing-plans, checkpointing after each task.
