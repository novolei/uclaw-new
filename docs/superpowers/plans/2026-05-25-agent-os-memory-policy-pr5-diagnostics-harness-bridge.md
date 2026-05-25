# Agent OS Memory Policy PR5 Diagnostics And Harness Bridge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Surface Memory Policy receipts in harness/diagnostic paths and prove gbrain offline, memU unavailable, queued completion, and memory_graph frozen behavior.

**Architecture:** PR5 does not add another memory store. It exposes receipt artifacts and harness score inputs using existing `HarnessEvent::MemoryWrite`, `HarnessEvent::MemoryRecall`, and JSON artifact patterns.

**Tech Stack:** Rust, existing harness runtime/artifacts, existing Memory Policy receipt contract, serde JSON.

---

## File Structure

- Create: `src-tauri/src/harness/adapters/memory_policy.rs`
  - Harness adapter helpers for Memory Policy receipts.
- Modify: `src-tauri/src/harness/adapters/mod.rs`
  - Export adapter.
- Modify: `src-tauri/src/harness/trace.rs`
  - Add memory policy target only if needed; otherwise keep existing `MemoryHarnessTarget`.
- Modify: `src-tauri/src/memory_policy/receipts.rs`
  - Add harness event conversion helper.
- Create: `src-tauri/src/harness/adapters/memory_policy_tests.rs`
  - Tests for receipt artifact and harness conversion.

## Task 1: Add Harness Receipt Artifact Helper

**Files:**
- Create: `src-tauri/src/harness/adapters/memory_policy.rs`
- Modify: `src-tauri/src/harness/adapters/mod.rs`
- Create: `src-tauri/src/harness/adapters/memory_policy_tests.rs`

- [ ] **Step 1: Inspect harness adapter exports**

Run:

```bash
sed -n '1,120p' src-tauri/src/harness/adapters/mod.rs
```

Expected: module export list with a safe insertion point.

- [ ] **Step 2: Add module exports**

In `src-tauri/src/harness/adapters/mod.rs`, add:

```rust
pub mod memory_policy;
#[cfg(test)]
mod memory_policy_tests;
```

- [ ] **Step 3: Write artifact helper test**

Create `src-tauri/src/harness/adapters/memory_policy_tests.rs`:

```rust
use crate::harness::adapters::memory_policy::attach_memory_policy_receipt;
use crate::harness::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
use crate::harness::runtime::HarnessRuntime;
use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyInput, MemoryPolicySource,
};

fn input() -> MemoryPolicyInput {
    MemoryPolicyInput {
        source: MemoryPolicySource::Harness,
        source_event_id: "harness-event-1".into(),
        task_id: "task-1".into(),
        intent_id: None,
        content: "memory policy receipt harness test".into(),
        requested_class: MemoryKnowledgeClass::Forbidden,
        promoted: false,
        redaction_clean: false,
        approval_ref: None,
        harness_case_ids: vec!["memory.policy.freeze".into()],
    }
}

#[test]
fn attaches_memory_policy_receipt_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = HarnessRuntime::new(tmp.path());
    let case = HarnessCase {
        id: "memory.policy.freeze".into(),
        subject: HarnessSubject::Memory,
        title: "memory policy freeze".into(),
        prompt: "verify freeze".into(),
        setup: Vec::new(),
        policy: HarnessPolicy::default(),
        budgets: HarnessBudget::default(),
        assertions: Vec::new(),
        graders: Vec::new(),
    };
    let episode = runtime.start_episode(&case);
    let decision = classify_memory_policy_input(input());
    let receipt = crate::memory_policy::receipts::build_receipt(
        &decision,
        &decision.actions[0],
        crate::memory_policy::MemoryPolicyReceiptStatus::Rejected,
        Some(crate::memory_policy::MemoryPolicyReasonCode::MemoryGraphFrozen),
        Some("memory-policy://rejected/action".into()),
        Some("memory_graph:frozen".into()),
        None,
    );
    let artifact = attach_memory_policy_receipt(&runtime, &episode.run_id, &receipt)
        .unwrap()
        .unwrap();
    assert_eq!(artifact.kind, "memory_policy_receipt");
    let stored = runtime.get_episode(&episode.run_id).unwrap();
    assert_eq!(stored.artifacts.len(), 1);
}
```

- [ ] **Step 4: Implement helper**

Create `src-tauri/src/harness/adapters/memory_policy.rs`:

```rust
use crate::harness::artifacts::{ArtifactStoreError, HarnessArtifact};
use crate::harness::runtime::HarnessRuntime;
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
```

- [ ] **Step 5: Run harness adapter test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory_policy_tests::attaches_memory_policy_receipt_artifact --lib
```

Expected: PASS.

## Task 2: Add Receipt To HarnessEvent Conversion

**Files:**
- Modify: `src-tauri/src/memory_policy/receipts.rs`
- Modify: `src-tauri/src/harness/adapters/memory_policy_tests.rs`

- [ ] **Step 1: Add conversion test**

Append:

```rust
#[test]
fn memory_graph_frozen_receipt_maps_to_harness_memory_write() {
    let decision = classify_memory_policy_input(input());
    let receipt = crate::memory_policy::receipts::build_receipt(
        &decision,
        &decision.actions[0],
        crate::memory_policy::MemoryPolicyReceiptStatus::Rejected,
        Some(crate::memory_policy::MemoryPolicyReasonCode::MemoryGraphFrozen),
        Some("memory-policy://rejected/action".into()),
        Some("memory_graph:frozen".into()),
        None,
    );
    let event = crate::memory_policy::receipts::receipt_to_harness_event(&receipt);
    assert_eq!(event.kind(), "memory_write");
}
```

- [ ] **Step 2: Implement conversion**

In `src-tauri/src/memory_policy/receipts.rs`, add:

```rust
pub fn receipt_to_harness_event(
    receipt: &MemoryPolicyExecutionReceipt,
) -> crate::harness::trace::HarnessEvent {
    let target = match receipt.target {
        crate::memory_policy::MemoryPolicyTarget::Gbrain => {
            crate::harness::trace::MemoryHarnessTarget::Gbrain
        }
        _ => crate::harness::trace::MemoryHarnessTarget::MemorySystem,
    };
    crate::harness::trace::HarnessEvent::MemoryWrite {
        ts: receipt.created_at.clone(),
        target,
        artifact_ref: receipt_artifact_ref(receipt),
    }
}
```

- [ ] **Step 3: Run conversion test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory_policy_tests::memory_graph_frozen_receipt_maps_to_harness_memory_write --lib
```

Expected: PASS.

## Task 3: Add Harness Cases For Failure Modes

**Files:**
- Create: `src-tauri/src/harness/cases/memory/memory-policy-freeze.json`
- Create: `src-tauri/src/harness/cases/memory/memory-policy-degraded.json`
- Modify: `src-tauri/src/harness/adapters/memory.rs`

- [ ] **Step 1: Add freeze case JSON**

Create `src-tauri/src/harness/cases/memory/memory-policy-freeze.json`:

```json
{
  "id": "memory.policy.freeze",
  "title": "Memory Policy rejects memory_graph writes",
  "target": "gbrain",
  "prompt": "Attempt a memory_graph write through Memory Policy and verify it is rejected.",
  "require_write_receipt": true,
  "expected_terms": ["memory_graph_frozen"]
}
```

- [ ] **Step 2: Add degraded case JSON**

Create `src-tauri/src/harness/cases/memory/memory-policy-degraded.json`:

```json
{
  "id": "memory.policy.degraded",
  "title": "Memory Policy records degraded auxiliary memory",
  "target": "memu",
  "prompt": "Run a memU auxiliary write with memU unavailable and verify the degraded receipt.",
  "require_write_receipt": true,
  "expected_terms": ["degraded"]
}
```

- [ ] **Step 3: Ensure case loader includes files**

Run:

```bash
rg -n "cases/memory|load_builtin_cases|include_str!" src-tauri/src/harness/adapters/memory.rs
```

Expected: see the existing builtin case list. Add the two new `include_str!` entries beside other memory cases.

- [ ] **Step 4: Run builtin cases test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory::tests::loads_builtin_memory_gbrain_eval_cases --lib
```

Expected: PASS and builtins include the new case ids.

## Task 4: PR5 Verification And Commit

**Files:**
- All PR5 files listed above.

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory_policy --lib
cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory::tests::loads_builtin_memory_gbrain_eval_cases --lib
```

Expected: PASS.

- [ ] **Step 2: Format and diff check**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git diff --check -- src-tauri/src/harness src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr5-diagnostics-harness-bridge.md
```

Expected: no diff-check output.

- [ ] **Step 3: GitNexus impact and detect**

Run GitNexus impact before editing existing `receipt_to_task_event` or harness adapter symbols. Run GitNexus `detect_changes(scope=staged)` before commit.

Expected: no HIGH/CRITICAL impact without explicit approval.

- [ ] **Step 4: Commit PR5**

Run:

```bash
git add src-tauri/src/harness src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr5-diagnostics-harness-bridge.md
git commit -m "feat(harness): surface memory policy receipts" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib; cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory_policy --lib; cargo test --manifest-path src-tauri/Cargo.toml harness::adapters::memory::tests::loads_builtin_memory_gbrain_eval_cases --lib; cargo fmt --manifest-path src-tauri/Cargo.toml; git diff --check; GitNexus detect_changes scope=staged."
```

Expected: commit succeeds.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr5-diagnostics-harness-bridge.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch PR5 after PR1-PR4 land.

**2. Inline Execution** - execute PR5 in this session after diagnostics scope review.
