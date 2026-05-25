# Agent OS Memory Policy PR3 MemU Target And Context Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the memU auxiliary target and a Context Fabric adapter that retrieves allowed memory context through Memory Policy.

**Architecture:** PR3 keeps durable truth in gbrain and treats memU as auxiliary. The Context Fabric adapter asks Memory Policy for allowed recall actions, wraps outputs as `ContextArtifact`, and degrades gracefully when memU is unavailable.

**Tech Stack:** Rust, existing `MemUClient`, existing `runtime::context::ContextArtifact`, `async-trait`, PR1/PR2 `memory_policy`.

---

## File Structure

- Create: `src-tauri/src/memory_policy/targets/memu.rs`
  - MemU target adapter and unavailable test constructor.
- Modify: `src-tauri/src/memory_policy/targets/mod.rs`
  - Export memU target.
- Create: `src-tauri/src/runtime/context_memory_policy.rs`
  - Context Fabric bridge.
- Modify: `src-tauri/src/runtime/mod.rs`
  - Export context adapter.
- Modify: `src-tauri/src/memory_policy/tests.rs`
  - Add memU degraded receipt tests.
- Create: `src-tauri/src/runtime/context_memory_policy_tests.rs`
  - ContextArtifact conversion tests.

## Task 1: Add MemU Target As Degradable Auxiliary Adapter

**Files:**
- Create: `src-tauri/src/memory_policy/targets/memu.rs`
- Modify: `src-tauri/src/memory_policy/targets/mod.rs`
- Modify: `src-tauri/src/memory_policy/tests.rs`

- [ ] **Step 1: Write memU unavailable test**

Append to `memory_policy/tests.rs`:

```rust
#[tokio::test]
async fn memu_unavailable_returns_degraded_receipt() {
    let target = crate::memory_policy::targets::memu::MemuPolicyTarget::unavailable_for_tests();
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::AuxiliaryRecall));
    let receipt = target.execute(&decision, &decision.actions[0]).await.unwrap();
    assert_eq!(receipt.target, MemoryPolicyTarget::Memu);
    assert_eq!(receipt.status, MemoryPolicyReceiptStatus::Degraded);
}
```

- [ ] **Step 2: Implement memU target**

Create `src-tauri/src/memory_policy/targets/memu.rs`:

```rust
use std::sync::Arc;

use async_trait::async_trait;

use crate::memu::client::MemUClient;
use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReceiptStatus,
};

#[derive(Clone)]
pub struct MemuPolicyTarget {
    client: Option<Arc<MemUClient>>,
}

impl MemuPolicyTarget {
    pub fn new(client: Arc<MemUClient>) -> Self {
        Self { client: Some(client) }
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
```

- [ ] **Step 3: Export module**

In `targets/mod.rs`, add:

```rust
pub mod memu;
```

- [ ] **Step 4: Run memU test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::memu_unavailable_returns_degraded_receipt --lib
```

Expected: PASS.

## Task 2: Add Context Memory Policy Adapter

**Files:**
- Create: `src-tauri/src/runtime/context_memory_policy.rs`
- Modify: `src-tauri/src/runtime/mod.rs`
- Create: `src-tauri/src/runtime/context_memory_policy_tests.rs`

- [ ] **Step 1: Inspect runtime module exports**

Run:

```bash
sed -n '1,120p' src-tauri/src/runtime/mod.rs
```

Expected: see existing runtime module list and a safe place to add `pub mod context_memory_policy;`.

- [ ] **Step 2: Add adapter module export**

In `src-tauri/src/runtime/mod.rs`, add:

```rust
pub mod context_memory_policy;
#[cfg(test)]
mod context_memory_policy_tests;
```

- [ ] **Step 3: Write adapter tests**

Create `src-tauri/src/runtime/context_memory_policy_tests.rs`:

```rust
use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyInput, MemoryPolicySource,
};
use crate::runtime::context::ContextSource;
use crate::runtime::context_memory_policy::memory_receipt_to_context_artifact;

fn input() -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::ContextFabric,
        source_event_id: "ctx-event-1".into(),
        task_id: "task-1".into(),
        intent_id: None,
        content: "project uclaw memory policy".into(),
        requested_class: MemoryKnowledgeClass::LegacyRead,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: Vec::new(),
    }
}

#[test]
fn receipt_becomes_context_artifact_with_memory_source() {
    let decision = classify_memory_policy_input(input());
    let action = &decision.actions[0];
    let receipt = crate::memory_policy::receipts::build_receipt(
        &decision,
        action,
        crate::memory_policy::MemoryPolicyReceiptStatus::Succeeded,
        None,
        Some("memory-policy://receipt/r1".into()),
        Some("memory_graph:legacy_read".into()),
        None,
    );
    let artifact = memory_receipt_to_context_artifact(&receipt, "legacy recall body");
    assert_eq!(artifact.r#ref.source, ContextSource::Memory);
    assert!(artifact.content.contains("legacy recall body"));
    assert_eq!(artifact.citations.len(), 1);
}
```

- [ ] **Step 4: Implement adapter helper**

Create `src-tauri/src/runtime/context_memory_policy.rs`:

```rust
use crate::memory_policy::MemoryPolicyExecutionReceipt;
use crate::runtime::context::{Citation, ContextArtifact, ContextRef, ContextSource};

pub fn memory_receipt_to_context_artifact(
    receipt: &MemoryPolicyExecutionReceipt,
    content: impl Into<String>,
) -> ContextArtifact {
    let target = receipt.target.as_task_event_target();
    ContextArtifact {
        r#ref: ContextRef::new(
            ContextSource::Memory,
            format!("memory-policy/{}/{}", target, receipt.receipt_id),
        )
        .with_label(format!("memory policy {}", target)),
        content: content.into(),
        citations: vec![Citation {
            line: None,
            evidence_ref: receipt
                .artifact_ref
                .clone()
                .unwrap_or_else(|| format!("memory-policy://receipt/{}", receipt.receipt_id)),
        }],
        retrieval_ts: chrono::Utc::now().to_rfc3339(),
    }
}
```

- [ ] **Step 5: Run context adapter test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml runtime::context_memory_policy_tests::receipt_becomes_context_artifact_with_memory_source --lib
```

Expected: PASS.

## Task 3: Wire Executor Test Constructor To Use MemU Target

**Files:**
- Modify: `src-tauri/src/memory_policy/executor.rs`
- Modify: `src-tauri/src/memory_policy/tests.rs`

- [ ] **Step 1: Add constructor replacement**

In test constructors, replace fake memU with:

```rust
Arc::new(crate::memory_policy::targets::memu::MemuPolicyTarget::unavailable_for_tests())
```

- [ ] **Step 2: Run full memory policy tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: PASS.

## Task 4: PR3 Verification And Commit

**Files:**
- All PR3 files listed above.

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
cargo test --manifest-path src-tauri/Cargo.toml runtime::context_memory_policy --lib
```

Expected: PASS.

- [ ] **Step 2: Format and diff check**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git diff --check -- src-tauri/src/memory_policy src-tauri/src/runtime docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr3-memu-context-adapter.md
```

Expected: no diff-check output.

- [ ] **Step 3: GitNexus impact and detect**

Run GitNexus impact before editing existing runtime export symbols. Run GitNexus `detect_changes(scope=staged)` before commit.

Expected: no HIGH/CRITICAL impact without explicit approval.

- [ ] **Step 4: Commit PR3**

Run:

```bash
git add src-tauri/src/memory_policy src-tauri/src/runtime docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr3-memu-context-adapter.md
git commit -m "feat(agent-os): add memu target and context memory policy adapter" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib; cargo test --manifest-path src-tauri/Cargo.toml runtime::context_memory_policy --lib; cargo fmt --manifest-path src-tauri/Cargo.toml; git diff --check; GitNexus detect_changes scope=staged."
```

Expected: commit succeeds.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr3-memu-context-adapter.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch PR3 only after PR2 lands.

**2. Inline Execution** - execute PR3 in this session after PR2 verification.
