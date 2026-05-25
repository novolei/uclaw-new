# Agent OS Memory Policy PR2 Gbrain And Artifact Targets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace PR1 fake gbrain and browser artifact targets with bounded real adapters while preserving explicit receipts.

**Architecture:** PR2 keeps the executor contract from PR1 and wires two real side-effect paths: approved durable writes through existing gbrain helpers and evidence receipt artifacts through a small artifact writer. All writes continue to pass hook gates and emit receipts.

**Tech Stack:** Rust, existing `gbrain::browse::put_page`, existing `SharedMcpManager`, filesystem JSON artifacts, `serde_json`, `tokio::time::timeout`.

---

## File Structure

- Create: `src-tauri/src/memory_policy/targets/gbrain.rs`
  - Bounded gbrain write adapter.
- Create: `src-tauri/src/memory_policy/targets/browser_artifact.rs`
  - JSON receipt artifact adapter.
- Modify: `src-tauri/src/memory_policy/targets/mod.rs`
  - Export real target modules.
- Modify: `src-tauri/src/memory_policy/executor.rs`
  - Add constructor accepting real gbrain/artifact adapters.
- Modify: `src-tauri/src/memory_policy/tests.rs`
  - Add fake-MCP-free unit tests for target formatting and artifact writes.

## Task 1: Add Gbrain Target Formatting Tests

**Files:**
- Modify: `src-tauri/src/memory_policy/tests.rs`
- Create: `src-tauri/src/memory_policy/targets/gbrain.rs`

- [x] **Step 1: Write failing formatting test**

Append:

```rust
#[test]
fn gbrain_target_formats_slug_and_markdown() {
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let action = &decision.actions[0];
    let request = crate::memory_policy::targets::gbrain::build_gbrain_write_request(&decision, action);
    assert!(request.slug.starts_with("memory-policy/task-1/"));
    assert!(request.content.contains("type: memory_policy_receipt"));
    assert!(request.content.contains("Ryan prefers gbrain as durable memory."));
}
```

- [x] **Step 2: Run test to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::gbrain_target_formats_slug_and_markdown --lib
```

Expected: FAIL because `targets::gbrain` is missing.

- [x] **Step 3: Implement request formatting**

Create `src-tauri/src/memory_policy/targets/gbrain.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::memory_policy::types::{MemoryPolicyAction, MemoryPolicyDecision};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GbrainPolicyWriteRequest {
    pub slug: String,
    pub content: String,
}

pub fn build_gbrain_write_request(
    decision: &MemoryPolicyDecision,
    action: &MemoryPolicyAction,
) -> GbrainPolicyWriteRequest {
    let slug = format!(
        "memory-policy/{}/{}",
        sanitize_slug_segment(&decision.input.task_id),
        sanitize_slug_segment(&action.action_id)
    );
    let content = format!(
        "---\ntitle: \"Memory policy write {}\"\ntype: memory_policy_receipt\ntags:\n  - memory_policy\n  - {}\ntask_id: {}\nsource_event_id: {}\n---\n\n# Memory Policy Durable Knowledge\n\n{}\n",
        yaml_escape(&action.action_id),
        action.target.as_task_event_target(),
        decision.input.task_id,
        decision.input.source_event_id,
        decision.input.content,
    );
    GbrainPolicyWriteRequest { slug, content }
}

fn sanitize_slug_segment(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if ch == '-' || ch == '_' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() { "unknown".into() } else { trimmed }
}

fn yaml_escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}
```

- [x] **Step 4: Export module**

In `src-tauri/src/memory_policy/targets/mod.rs`, add:

```rust
pub mod gbrain;
```

- [x] **Step 5: Run formatting test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::gbrain_target_formats_slug_and_markdown --lib
```

Expected: PASS.

## Task 2: Add Browser Artifact Target

**Files:**
- Create: `src-tauri/src/memory_policy/targets/browser_artifact.rs`
- Modify: `src-tauri/src/memory_policy/targets/mod.rs`
- Modify: `src-tauri/src/memory_policy/tests.rs`

- [x] **Step 1: Write failing artifact test**

Append:

```rust
#[tokio::test]
async fn browser_artifact_target_writes_receipt_json() {
    let tmp = tempfile::tempdir().unwrap();
    let target = crate::memory_policy::targets::browser_artifact::BrowserArtifactPolicyTarget::new(tmp.path());
    let mut event = input(MemoryKnowledgeClass::EpisodicEvidence);
    event.source = MemoryPolicySource::BrowserRuntime;
    let decision = classify_memory_policy_input(event);
    let receipt = target.execute(&decision, &decision.actions[0]).await.unwrap();
    assert_eq!(receipt.status, MemoryPolicyReceiptStatus::Succeeded);
    let artifact_ref = receipt.artifact_ref.unwrap();
    assert!(artifact_ref.starts_with("file://"));
    assert!(std::fs::read_to_string(artifact_ref.trim_start_matches("file://")).unwrap().contains("sourceEventId"));
}
```

- [x] **Step 2: Implement target**

Create `src-tauri/src/memory_policy/targets/browser_artifact.rs`:

```rust
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
```

- [x] **Step 3: Export artifact module**

In `targets/mod.rs`, add:

```rust
pub mod browser_artifact;
```

- [x] **Step 4: Run artifact test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::browser_artifact_target_writes_receipt_json --lib
```

Expected: PASS.

## Task 3: Add Real Gbrain Target With Bounded Result Shape

**Files:**
- Modify: `src-tauri/src/memory_policy/targets/gbrain.rs`
- Modify: `src-tauri/src/memory_policy/tests.rs`

- [x] **Step 1: Add unavailable test**

Append:

```rust
#[tokio::test]
async fn gbrain_unavailable_returns_deferred_receipt() {
    let target = crate::memory_policy::targets::gbrain::GbrainPolicyTarget::unavailable_for_tests();
    let decision = classify_memory_policy_input(input(MemoryKnowledgeClass::DurableKnowledge));
    let receipt = target.execute(&decision, &decision.actions[0]).await.unwrap();
    assert_eq!(receipt.status, MemoryPolicyReceiptStatus::Deferred);
    assert_eq!(receipt.reason_code, Some(MemoryPolicyReasonCode::GbrainUnavailable));
}
```

- [x] **Step 2: Implement target wrapper**

Extend `gbrain.rs`:

```rust
use async_trait::async_trait;

use crate::mcp::SharedMcpManager;
use crate::memory_policy::receipts::build_receipt;
use crate::memory_policy::targets::{MemoryPolicyTargetAdapter, MemoryPolicyTargetError};
use crate::memory_policy::types::{
    MemoryPolicyAction, MemoryPolicyDecision, MemoryPolicyExecutionReceipt,
    MemoryPolicyReasonCode, MemoryPolicyReceiptStatus,
};

#[derive(Clone)]
pub struct GbrainPolicyTarget {
    mcp: Option<SharedMcpManager>,
}

impl GbrainPolicyTarget {
    pub fn new(mcp: SharedMcpManager) -> Self {
        Self { mcp: Some(mcp) }
    }

    pub fn unavailable_for_tests() -> Self {
        Self { mcp: None }
    }
}

#[async_trait]
impl MemoryPolicyTargetAdapter for GbrainPolicyTarget {
    async fn execute(
        &self,
        decision: &MemoryPolicyDecision,
        action: &MemoryPolicyAction,
    ) -> Result<MemoryPolicyExecutionReceipt, MemoryPolicyTargetError> {
        let Some(mcp) = self.mcp.as_ref() else {
            return Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                None,
            ));
        };
        let request = build_gbrain_write_request(decision, action);
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            crate::gbrain::browse::put_page(mcp, &request.slug, &request.content),
        )
        .await;
        match result {
            Ok(Ok(page)) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Succeeded,
                None,
                Some(format!("gbrain://{}", page.slug)),
                Some(page.slug),
                None,
            )),
            Ok(Err(err)) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some(err.to_command_string()),
            )),
            Err(_) => Ok(build_receipt(
                decision,
                action,
                MemoryPolicyReceiptStatus::Deferred,
                Some(MemoryPolicyReasonCode::GbrainUnavailable),
                Some(format!("memory-policy://deferred/{}", action.action_id)),
                None,
                Some("gbrain write timed out after 5s".into()),
            )),
        }
    }
}
```

- [x] **Step 3: Run gbrain tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::gbrain_unavailable_returns_deferred_receipt --lib
cargo test --manifest-path src-tauri/Cargo.toml memory_policy::tests::gbrain_target_formats_slug_and_markdown --lib
```

Expected: PASS.

## Task 4: Wire Executor Constructor For Real Targets

**Files:**
- Modify: `src-tauri/src/memory_policy/executor.rs`

- [x] **Step 1: Add constructor**

Add:

```rust
pub fn with_real_gbrain_and_artifacts(
    hook_bus: HookBus,
    gbrain_mcp: crate::mcp::SharedMcpManager,
    artifact_root: impl AsRef<std::path::Path>,
    memu: Arc<dyn MemoryPolicyTargetAdapter>,
) -> Self {
    Self::new(
        hook_bus,
        Arc::new(crate::memory_policy::targets::gbrain::GbrainPolicyTarget::new(gbrain_mcp)),
        memu,
        Arc::new(crate::memory_policy::targets::browser_artifact::BrowserArtifactPolicyTarget::new(artifact_root)),
    )
}
```

- [x] **Step 2: Run full memory_policy tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
```

Expected: PASS.

## Task 5: PR2 Verification And Commit

**Files:**
- All PR2 files listed above.

- [x] **Step 1: Format and check**

Run:

```bash
cargo fmt --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib
git diff --check -- src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr2-gbrain-artifact-targets.md
```

Expected: PASS and no diff-check output.

- [x] **Step 2: Run GitNexus impact and detect changes**

Before modifying existing `MemoryPolicyExecutor` symbols, run GitNexus impact on `MemoryPolicyExecutor`. Before commit, run GitNexus `detect_changes(scope=staged)`.

Expected: no HIGH/CRITICAL impact without user approval; staged changes limited to memory_policy and the plan.

- [x] **Step 3: Commit PR2**

Run:

```bash
git add src-tauri/src/memory_policy docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr2-gbrain-artifact-targets.md
git commit -m "feat(agent-os): wire memory policy gbrain artifact targets" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml memory_policy --lib; cargo fmt --manifest-path src-tauri/Cargo.toml; git diff --check; GitNexus detect_changes scope=staged."
```

Expected: commit succeeds.

## Execution Handoff

Plan complete and saved to `docs/superpowers/plans/2026-05-25-agent-os-memory-policy-pr2-gbrain-artifact-targets.md`. Two execution options:

**1. Subagent-Driven (recommended)** - dispatch a fresh PR2 subagent after PR1 lands.

**2. Inline Execution** - execute PR2 in this session after PR1 verification.
