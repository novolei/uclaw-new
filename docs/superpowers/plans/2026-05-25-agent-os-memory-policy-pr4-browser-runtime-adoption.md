# Agent OS Memory Policy PR4 Browser Runtime Adoption Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `browser::runtime_memory_policy` and switch one narrow browser evidence path to Memory Policy without removing the legacy adapter.

**Architecture:** Browser observations/checkpoints remain evidence by default. The new adapter classifies browser events, executes browser artifact writes through Memory Policy, and only creates gbrain actions when explicit promotion metadata is present.

**Tech Stack:** Rust, existing `BrowserLongTermMemoryEvent`, existing `BrowserLongTermMemoryAdapter`, Memory Policy executor from PR1-PR3.

---

## File Structure

- Create: `src-tauri/src/browser/runtime_memory_policy.rs`
  - Browser event to Memory Policy input conversion.
- Modify: `src-tauri/src/browser/mod.rs`
  - Export runtime memory policy module.
- Modify: `src-tauri/src/browser/memory_adapter.rs`
  - Add one optional narrow path that can use Memory Policy for checkpoint or visual observation evidence.
- Create: `src-tauri/src/browser/runtime_memory_policy_tests.rs`
  - Browser classification and promotion gate tests.

## Task 1: Add Browser Runtime Memory Policy Adapter

**Files:**
- Create: `src-tauri/src/browser/runtime_memory_policy.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Create: `src-tauri/src/browser/runtime_memory_policy_tests.rs`

- [x] **Step 1: Add module exports**

In `src-tauri/src/browser/mod.rs`, add:

```rust
pub mod runtime_memory_policy;
#[cfg(test)]
mod runtime_memory_policy_tests;
```

- [x] **Step 2: Write evidence classification tests**

Create `src-tauri/src/browser/runtime_memory_policy_tests.rs`:

```rust
use crate::browser::runtime_memory_policy::{
    classify_browser_evidence, BrowserMemoryPromotionMetadata,
};
use crate::memory_policy::{MemoryKnowledgeClass, MemoryPolicyActionKind, MemoryPolicySource};

#[test]
fn browser_checkpoint_defaults_to_artifact_evidence() {
    let decision = classify_browser_evidence(
        "event-1",
        "task-1",
        "checkpoint payload",
        None,
    );
    assert_eq!(decision.input.source, MemoryPolicySource::BrowserRuntime);
    assert_eq!(decision.knowledge_class, MemoryKnowledgeClass::EpisodicEvidence);
    assert_eq!(decision.actions[0].kind, MemoryPolicyActionKind::BrowserArtifactWrite);
}

#[test]
fn promoted_browser_knowledge_adds_gbrain_write() {
    let decision = classify_browser_evidence(
        "event-2",
        "task-1",
        "stable selector: button[type=submit]",
        Some(BrowserMemoryPromotionMetadata {
            redaction_clean: true,
            approval_ref: Some("approval-1".into()),
            harness_case_ids: vec!["browser.login.replay".into()],
        }),
    );
    let kinds: Vec<_> = decision.actions.iter().map(|action| action.kind).collect();
    assert!(kinds.contains(&MemoryPolicyActionKind::BrowserArtifactWrite));
    assert!(kinds.contains(&MemoryPolicyActionKind::GbrainWrite));
}
```

- [x] **Step 3: Implement adapter classification**

Create `src-tauri/src/browser/runtime_memory_policy.rs`:

```rust
use crate::memory_policy::{
    classify_memory_policy_input, MemoryKnowledgeClass, MemoryPolicyAction,
    MemoryPolicyActionKind, MemoryPolicyDecision, MemoryPolicyExecutionMode, MemoryPolicyInput,
    MemoryPolicySource,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserMemoryPromotionMetadata {
    pub redaction_clean: bool,
    pub approval_ref: Option<String>,
    pub harness_case_ids: Vec<String>,
}

pub fn classify_browser_evidence(
    source_event_id: impl Into<String>,
    task_id: impl Into<String>,
    content: impl Into<String>,
    promotion: Option<BrowserMemoryPromotionMetadata>,
) -> MemoryPolicyDecision {
    let source_event_id = source_event_id.into();
    let task_id = task_id.into();
    let content = content.into();
    let mut decision = classify_memory_policy_input(MemoryPolicyInput {
        source: MemoryPolicySource::BrowserRuntime,
        source_event_id,
        task_id,
        intent_id: None,
        content,
        requested_class: MemoryKnowledgeClass::EpisodicEvidence,
        promoted: promotion.is_some(),
        redaction_clean: promotion.as_ref().map(|p| p.redaction_clean).unwrap_or(false),
        approval_ref: promotion.as_ref().and_then(|p| p.approval_ref.clone()),
        harness_case_ids: promotion
            .as_ref()
            .map(|p| p.harness_case_ids.clone())
            .unwrap_or_default(),
    });

    if let Some(promotion) = promotion {
        if promotion.redaction_clean
            && (promotion.approval_ref.is_some() || !promotion.harness_case_ids.is_empty())
        {
            let gbrain_action = MemoryPolicyAction {
                action_id: format!("{}-promote-gbrain", decision.actions[0].action_id),
                kind: MemoryPolicyActionKind::GbrainWrite,
                target: MemoryPolicyActionKind::GbrainWrite.target(),
                execution_mode: MemoryPolicyExecutionMode::BoundedAwait,
                topic: "browser_promoted_knowledge".into(),
                size_bytes: decision.input.content.len(),
                idempotency_key: format!("{}:gbrain:promoted", decision.input.source_event_id),
            };
            decision.actions.push(gbrain_action);
        }
    }
    decision
}
```

- [x] **Step 4: Run adapter tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy_tests --lib
```

Expected: PASS.

## Task 2: Switch One Narrow Browser Evidence Path

**Files:**
- Modify: `src-tauri/src/browser/memory_adapter.rs`

- [x] **Step 1: Run GitNexus impact**

Before editing `BrowserLongTermMemoryAdapter`, run GitNexus impact for `BrowserLongTermMemoryAdapter` upstream.

Expected: if risk is HIGH/CRITICAL, stop and ask for review before editing.

- [x] **Step 2: Add policy classification helper without removing legacy writes**

In `BrowserLongTermMemoryAdapter::record_checkpoint`, before `self.record(...)`, add a local classification call and trace:

```rust
let policy_decision = crate::browser::runtime_memory_policy::classify_browser_evidence(
    format!("{}:checkpoint:{}", run.run_id, step_index),
    run.run_id.clone(),
    serde_json::to_string(&payload).unwrap_or_else(|_| "{}".into()),
    None,
);
tracing::debug!(
    run_id = %run.run_id,
    action_count = policy_decision.actions.len(),
    "browser checkpoint classified by memory policy"
);
```

This PR does not execute the policy path from production code yet. It proves classification on the narrow checkpoint path while preserving legacy behavior.

- [x] **Step 3: Add regression test for no gbrain auto-promotion**

In `runtime_memory_policy_tests.rs`, add:

```rust
#[test]
fn unpromoted_browser_payload_never_adds_gbrain_action() {
    let decision = classify_browser_evidence(
        "event-3",
        "task-1",
        "{\"screenshotRef\":\"browser://shot\"}",
        None,
    );
    assert!(!decision
        .actions
        .iter()
        .any(|action| action.kind == MemoryPolicyActionKind::GbrainWrite));
}
```

- [x] **Step 4: Run browser tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy_tests --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::memory_adapter --lib
```

Expected: PASS. If the second filter matches no tests, run `cargo test --manifest-path src-tauri/Cargo.toml memory_adapter --lib` and record the exact result in the commit body.

## Task 3: PR4 Verification And Commit

**Files:**
- All PR4 files listed above.

- [x] **Step 1: Run focused test set**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy_tests --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: PASS.

- [x] **Step 2: Format and diff check**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
git diff --check -- src-tauri/src/browser src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr4-browser-runtime-adoption.md
```

Expected: no diff-check output.

- [x] **Step 3: GitNexus detect**

Run GitNexus `detect_changes(scope=staged)`.

Expected: changed flow should be limited to browser memory classification and new browser adapter tests. HIGH/CRITICAL requires review.

- [x] **Step 4: Commit PR4**

Run:

```bash
git add src-tauri/src/browser src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr4-browser-runtime-adoption.md
git commit -m "feat(browser): classify runtime memory through policy spine" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_memory_policy_tests --lib; cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib; cargo fmt --manifest-path src-tauri/Cargo.toml; git diff --check; GitNexus detect_changes scope=staged."
```

Expected: commit succeeds.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr4-browser-runtime-adoption.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch PR4 after PR1-PR3 land.

**2. Inline Execution** - execute PR4 in this session with browser-focused checkpoint review.
