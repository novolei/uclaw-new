# PR-1 Pure Type Crates and Runtime Contracts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract pure Rust type crates for messages, tools, protocol envelopes, and runtime contracts while preserving existing uClaw behavior through compatibility re-exports.

**Architecture:** Create focused workspace crates under `crates/` and move dependency-light data contracts into them. Keep `src-tauri/src/agent/types.rs` and `src-tauri/src/runtime/contracts.rs` as compatibility facades so existing call sites can migrate gradually instead of churn in this PR.

**Tech Stack:** Rust workspace crates, serde, serde_json, existing `src-tauri` library tests, GitNexus impact/detect-changes, uClaw DMZ writer/reviewer protocol for root `Cargo.toml`.

---

## 0. Scope Correction

Canonical PR-1 is from `docs/jcode_comparison/README.md`:

```text
PR-1: extract pure type crates for messages, tools, protocol, and runtime contracts.
```

The earlier draft that made PR-1 about event-spine validation was a numbering
drift. Event-spine validation remains useful, but it moves behind this
foundation as a later acceptance/PR-5 projection-journal supporting task.

## 1. Cross-Document Review Gate

Before implementation, verify these documents agree on PR-1:

- `docs/jcode_comparison/README.md`
- `docs/jcode_comparison/04_backend_reconstruction_blueprint.md`
- `docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md`
- `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- this plan

Required command:

```bash
rg -n "PR-1|Runtime event spine|extract pure|type crates|2026-05-23-pr1" \
  docs/jcode_comparison \
  docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md \
  docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md \
  docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md
```

Expected:

```text
PR-1 references point to pure type crates.
Runtime event spine appears only as deferred/follow-up wording, not as PR-1 title.
```

## 2. Current Code Truth

Existing type locations:

- `src-tauri/src/agent/types.rs`
  - message types: `MessageRole`, `ContentBlock`, `ChatMessage`;
  - tool types: `ToolCall`, `ToolDefinition`;
  - response/loop types: `TokenUsage`, `ResponseMetadata`, `RespondOutput`, `StreamDelta`, `LoopSignal`, `LoopOutcome`, `TextAction`;
  - behavior-only types: `ReasoningContext`, `LoopDelegate`, `AgenticLoopConfig`.
- `src-tauri/src/runtime/contracts.rs`
  - Agent OS runtime contracts: `IntentSpec`, `TaskSpec`, `TaskEvent`, `TaskEventSource`, `TaskVerdict`, `CapabilityQuery`, `ContextRef`, `HookDecision`, `BoundaryRef`, `WorkerId`.
- `Cargo.toml`
  - workspace member list; this is a DMZ file.
- `src-tauri/Cargo.toml`
  - `uclaw_core` dependency list.

PR-1 extracts data contracts only. It does not move `LoopDelegate`,
`ReasoningContext`, provider clients, tool dispatcher logic, database logic, or
Tauri command payloads.

## 3. ADR Section 18 Answers

| Question | Answer |
|---|---|
| 1. What user intent does this support? | Users need uClaw runtime, browser, tools, automation, teams, and frontend surfaces to share stable contracts rather than depend on monolithic backend modules. |
| 2. What autonomy level can it run at? | No autonomy behavior changes. This supports later L0-L6 work by making contracts reusable. |
| 3. What is the canonical truth source? | Runtime truth remains `TaskEvent`; this PR only relocates definitions. |
| 4. What TaskEvent entries does it emit? | None. |
| 5. What context does it read, and how is it cited? | No runtime/user context. It reads local source files and validates serde/compile compatibility. |
| 6. What capability cards does it add or consume? | None. It prepares type boundaries future capability cards can depend on. |
| 7. What policy hooks can block it? | Build/test hooks, SPDX hooks, GitNexus, and DMZ writer/reviewer review for root `Cargo.toml`. |
| 8. What world projection does the UI render? | None. Future projection reducers can depend on the extracted contracts. |
| 9. What harness cases prove it works? | Serde round-trip tests in pure crates plus existing focused `src-tauri` tests. |
| 10. What is the rollback or disable path? | Revert crate additions, dependency additions, and compatibility re-export facades. |
| 11. What does it deliberately not own? | ToolContext, BrowserProvider, event-spine validator, frontend projection, ambient automation, team orchestration, and jcode tool ports. |

## 4. GitNexus And DMZ Gate

Root `Cargo.toml` is a DMZ file. This PR requires writer/reviewer review before
merge.

Before editing symbols, run:

```bash
gitnexus impact ChatMessage --direction upstream
gitnexus impact ToolCall --direction upstream
gitnexus impact ToolDefinition --direction upstream
gitnexus impact TaskEvent --direction upstream
gitnexus impact IntentSpec --direction upstream
gitnexus impact TaskSpec --direction upstream
```

If any result is HIGH or CRITICAL, stop and confirm with Ryan before editing
that symbol.

Before commit:

```bash
gitnexus detect-changes --scope staged
```

Expected affected scope:

```text
pure type crates, compatibility facades, and Cargo dependency wiring only
```

## 5. Files

Create:

- `crates/uclaw-message-types/Cargo.toml`
- `crates/uclaw-message-types/src/lib.rs`
- `crates/uclaw-message-types/src/message_tests.rs`
- `crates/uclaw-tool-types/Cargo.toml`
- `crates/uclaw-tool-types/src/lib.rs`
- `crates/uclaw-tool-types/src/tool_tests.rs`
- `crates/uclaw-runtime-contracts/Cargo.toml`
- `crates/uclaw-runtime-contracts/src/lib.rs`
- `crates/uclaw-runtime-contracts/src/contracts_tests.rs`
- `crates/uclaw-protocol-types/Cargo.toml`
- `crates/uclaw-protocol-types/src/lib.rs`
- `crates/uclaw-protocol-types/src/protocol_tests.rs`

Modify:

- `Cargo.toml`
- `src-tauri/Cargo.toml`
- `src-tauri/src/agent/types.rs`
- `src-tauri/src/runtime/contracts.rs`
- `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

Do not modify:

- `src-tauri/src/agent/agentic_loop.rs`
- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/db/migrations.rs`
- `memory_graph`
- frontend files

## 6. uClaw Rust Hygiene Adapted From jcode

This PR adopts the jcode-style Rust layout shown in the user reference images,
adapted to uClaw conventions:

- keep production modules focused on production definitions;
- put substantial unit tests in sibling `*_tests.rs` files;
- include those tests from the production module with:

```rust
#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
```

- use platform gates when needed:

```rust
#[cfg(all(test, not(windows)))]
#[path = "bash_tests.rs"]
mod tests;
```

- start sibling test files with `use super::*;`;
- do not create god files: target roughly under 400 lines per hand-written Rust
  module, split before 600 lines when practical, and treat 900+ lines as a hard
  split trigger unless the file is generated or a data fixture with an explicit
  comment explaining why it stays whole.

For PR-1 specifically, every new pure type crate must use a sibling test file:

- `uclaw-message-types/src/message_tests.rs`
- `uclaw-tool-types/src/tool_tests.rs`
- `uclaw-runtime-contracts/src/contracts_tests.rs`
- `uclaw-protocol-types/src/protocol_tests.rs`

## 7. Implementation Tasks

### Task 1: Extract `uclaw-message-types`

**Files:**
- Create: `crates/uclaw-message-types/Cargo.toml`
- Create: `crates/uclaw-message-types/src/lib.rs`
- Create: `crates/uclaw-message-types/src/message_tests.rs`
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/agent/types.rs`

- [ ] **Step 1: Create crate manifest**

```toml
[package]
name = "uclaw-message-types"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create crate library**

Move `MessageRole`, `ContentBlock`, `ChatMessage`, `estimate_tokens`,
`estimate_message_tokens`, and private `is_cjk` from
`src-tauri/src/agent/types.rs` into `crates/uclaw-message-types/src/lib.rs`.

At the bottom of `lib.rs`, add only the jcode-style external test module hook:

```rust
#[cfg(test)]
#[path = "message_tests.rs"]
mod tests;
```

- [ ] **Step 3: Create sibling tests**

Create `crates/uclaw-message-types/src/message_tests.rs`:

```rust
use super::*;

#[test]
fn chat_message_wire_shape_preserves_role_and_content_type() {
    let msg = ChatMessage::user("hello");
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["role"], "user");
    assert_eq!(value["content"][0]["type"], "text");
    assert_eq!(value["compacted"], false);
}

#[test]
fn tool_result_helper_preserves_error_flag() {
    let msg = ChatMessage::user_tool_result("call-1", "failed", true);
    let value = serde_json::to_value(&msg).unwrap();
    assert_eq!(value["content"][0]["type"], "tool_result");
    assert_eq!(value["content"][0]["tool_use_id"], "call-1");
    assert_eq!(value["content"][0]["is_error"], true);
}

#[test]
fn cjk_estimator_counts_chinese_more_heavily_than_ascii() {
    assert!(estimate_tokens("你好世界") > estimate_tokens("hello"));
}
```

- [ ] **Step 4: Wire workspace and compatibility facade**

Add workspace member to root `Cargo.toml`:

```toml
    # Agent OS v2 pure type crates
    "crates/uclaw-message-types",
```

Add dependency to `src-tauri/Cargo.toml`:

```toml
uclaw-message-types = { path = "../crates/uclaw-message-types" }
```

Add compatibility re-export near the top of `src-tauri/src/agent/types.rs`:

```rust
pub use uclaw_message_types::{
    estimate_message_tokens, estimate_tokens, ChatMessage, ContentBlock, MessageRole,
};
```

- [ ] **Step 5: Verify**

```bash
cargo test -p uclaw-message-types
cd src-tauri && cargo test agent::types channels::dispatcher --lib
```

Expected:

```text
test result: ok
```

### Task 2: Extract `uclaw-tool-types`

**Files:**
- Create: `crates/uclaw-tool-types/Cargo.toml`
- Create: `crates/uclaw-tool-types/src/lib.rs`
- Create: `crates/uclaw-tool-types/src/tool_tests.rs`
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/agent/types.rs`

- [ ] **Step 1: Create crate manifest**

```toml
[package]
name = "uclaw-tool-types"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Create crate library**

Move `ToolCall` and `ToolDefinition` from `src-tauri/src/agent/types.rs` into
`crates/uclaw-tool-types/src/lib.rs`.

At the bottom of `lib.rs`, add only the jcode-style external test module hook:

```rust
#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
```

- [ ] **Step 3: Create sibling tests**

Create `crates/uclaw-tool-types/src/tool_tests.rs`:

```rust
use super::*;
use serde_json::json;

#[test]
fn tool_call_wire_shape_is_stable() {
    let call = ToolCall {
        id: "call-1".into(),
        name: "shell".into(),
        arguments: json!({"cmd": "pwd"}),
    };
    let value = serde_json::to_value(call).unwrap();
    assert_eq!(value["id"], "call-1");
    assert_eq!(value["name"], "shell");
    assert_eq!(value["arguments"]["cmd"], "pwd");
}

#[test]
fn tool_definition_wire_shape_is_stable() {
    let definition = ToolDefinition {
        name: "read_file".into(),
        description: "Read a file".into(),
        parameters: json!({"type": "object"}),
    };
    let value = serde_json::to_value(definition).unwrap();
    assert_eq!(value["name"], "read_file");
    assert_eq!(value["description"], "Read a file");
    assert_eq!(value["parameters"]["type"], "object");
}
```

- [ ] **Step 4: Wire workspace and compatibility facade**

Add workspace member:

```toml
    "crates/uclaw-tool-types",
```

Add dependency to `src-tauri/Cargo.toml`:

```toml
uclaw-tool-types = { path = "../crates/uclaw-tool-types" }
```

Add compatibility re-export:

```rust
pub use uclaw_tool_types::{ToolCall, ToolDefinition};
```

- [ ] **Step 5: Verify**

```bash
cargo test -p uclaw-tool-types
cd src-tauri && cargo test llm::providers::anthropic llm::providers::openai agent::llm_stream --lib
```

Expected:

```text
test result: ok
```

### Task 3: Extract `uclaw-runtime-contracts`

**Files:**
- Create: `crates/uclaw-runtime-contracts/Cargo.toml`
- Create: `crates/uclaw-runtime-contracts/src/lib.rs`
- Create: `crates/uclaw-runtime-contracts/src/contracts_tests.rs`
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`
- Modify: `src-tauri/src/runtime/contracts.rs`

- [ ] **Step 1: Create crate manifest**

```toml
[package]
name = "uclaw-runtime-contracts"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
```

- [ ] **Step 2: Move runtime contracts**

Move public type definitions from `src-tauri/src/runtime/contracts.rs` into
`crates/uclaw-runtime-contracts/src/lib.rs`. Move any existing tests into
`crates/uclaw-runtime-contracts/src/contracts_tests.rs`.

At the bottom of `lib.rs`, add only the jcode-style external test module hook:

```rust
#[cfg(test)]
#[path = "contracts_tests.rs"]
mod tests;
```

Replace old module content with:

```rust
//! Compatibility facade for Agent OS v2 runtime contracts.
//!
//! Canonical definitions live in `uclaw-runtime-contracts`.

pub use uclaw_runtime_contracts::*;
```

- [ ] **Step 3: Wire workspace**

Add workspace member:

```toml
    "crates/uclaw-runtime-contracts",
```

Add dependency to `src-tauri/Cargo.toml`:

```toml
uclaw-runtime-contracts = { path = "../crates/uclaw-runtime-contracts" }
```

- [ ] **Step 4: Verify**

```bash
cargo test -p uclaw-runtime-contracts
cd src-tauri && cargo test runtime::contracts runtime::rollout agent::regular_task browser::rollout_bridge automation::rollout_bridge harness::case --lib
```

Expected:

```text
test result: ok
```

### Task 4: Add `uclaw-protocol-types`

**Files:**
- Create: `crates/uclaw-protocol-types/Cargo.toml`
- Create: `crates/uclaw-protocol-types/src/lib.rs`
- Create: `crates/uclaw-protocol-types/src/protocol_tests.rs`
- Modify: `Cargo.toml`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Create crate manifest**

```toml
[package]
name = "uclaw-protocol-types"
version = "0.1.0"
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
rust-version.workspace = true

[dependencies]
serde = { version = "1", features = ["derive"] }
serde_json = "1"
uclaw-message-types = { path = "../uclaw-message-types" }
uclaw-runtime-contracts = { path = "../uclaw-runtime-contracts" }
uclaw-tool-types = { path = "../uclaw-tool-types" }
```

- [ ] **Step 2: Create protocol envelope crate**

```rust
//! Shared protocol envelope types for uClaw IPC, runtime traces,
//! and future provider/plugin boundaries.

use serde::{Deserialize, Serialize};

pub use uclaw_message_types::{ChatMessage, ContentBlock, MessageRole};
pub use uclaw_runtime_contracts::{
    AutonomyLevel, CapabilityQuery, ContextRef, IntentOrigin, IntentSpec, RiskClass, TaskEvent,
    TaskEventSource, TaskSpec, TaskVerdict,
};
pub use uclaw_tool_types::{ToolCall, ToolDefinition};

pub const UCLAW_PROTOCOL_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProtocolDomain {
    Agent,
    Browser,
    Automation,
    Tool,
    Harness,
    World,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolEnvelope<T> {
    pub version: u32,
    pub domain: ProtocolDomain,
    pub payload: T,
}

impl<T> ProtocolEnvelope<T> {
    pub fn new(domain: ProtocolDomain, payload: T) -> Self {
        Self {
            version: UCLAW_PROTOCOL_VERSION,
            domain,
            payload,
        }
    }
}
```

At the bottom of `lib.rs`, add only the jcode-style external test module hook:

```rust
#[cfg(test)]
#[path = "protocol_tests.rs"]
mod tests;
```

- [ ] **Step 3: Create sibling tests**

Create `crates/uclaw-protocol-types/src/protocol_tests.rs`:

```rust
use super::*;

#[test]
fn protocol_envelope_wire_shape_is_camel_case() {
    let envelope = ProtocolEnvelope::new(ProtocolDomain::Agent, "payload");
    let value = serde_json::to_value(envelope).unwrap();
    assert_eq!(value["version"], UCLAW_PROTOCOL_VERSION);
    assert_eq!(value["domain"], "agent");
    assert_eq!(value["payload"], "payload");
}

#[test]
fn protocol_crate_reexports_runtime_message_and_tool_types() {
    let _message = ChatMessage::user("hello");
    let _event = TaskEvent::Warning {
        ts: "2026-05-23T00:00:00Z".into(),
        source: TaskEventSource::AgentLoop,
        task_id: "task-1".into(),
        code: "example".into(),
        message: "hello".into(),
    };
    let _tool = ToolCall {
        id: "call-1".into(),
        name: "shell".into(),
        arguments: serde_json::json!({"cmd": "pwd"}),
    };
}
```

- [ ] **Step 4: Wire workspace**

Add workspace member:

```toml
    "crates/uclaw-protocol-types",
```

Add dependency to `src-tauri/Cargo.toml`:

```toml
uclaw-protocol-types = { path = "../crates/uclaw-protocol-types" }
```

- [ ] **Step 5: Verify**

```bash
cargo test -p uclaw-protocol-types
```

Expected:

```text
test result: ok
```

### Task 5: Update Close-Loop Status Ledger

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`

- [ ] **Step 1: Update PR-1 row**

```markdown
| PR-1 | Pure type crates for messages/tools/protocol/runtime contracts | In progress | Codex | Execute `docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md` in the isolated worktree. |
```

- [ ] **Step 2: Append decision log row**

```markdown
| 2026-05-23 | Corrected PR-1 numbering drift: PR-1 is pure type crate extraction, not event spine validation. | `docs/jcode_comparison/README.md` listed PR-1 as type extraction. | Event spine validation moves behind the type-crate foundation. |
| 2026-05-23 | Adopted jcode-style Rust test/module hygiene for uClaw PR-1. | User reference screenshots show sibling `*_tests.rs` modules loaded via `#[path = "..."] mod tests;`. | PR-1 crates must use sibling test files and avoid god files through focused module boundaries. |
```

- [ ] **Step 3: Update PR-1 progress**

```markdown
## PR-1 Progress

- Plan: `docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md`
- Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr1-plan`
- Branch: `codex/agent-os-jcode-pr1-plan`
- Scope: extract `uclaw-message-types`, `uclaw-tool-types`, `uclaw-runtime-contracts`, and `uclaw-protocol-types`.
- Rust hygiene: sibling `*_tests.rs` files only; no substantial inline test module blocks in production modules.
- DMZ files: root `Cargo.toml` touched; writer/reviewer required before merge.
- Migration: none planned.
- Rollback: revert crate additions, dependency additions, and compatibility re-export facades.
```

### Task 6: Final Superpowers Review

**Files:**
- All docs listed in Section 1.

- [ ] **Step 1: Verify PR numbering consistency**

Run the Section 1 cross-document command.

Expected:

```text
PR-1 references point to pure type crates.
Runtime event spine appears only as deferred/follow-up wording, not as PR-1 title.
```

- [ ] **Step 2: Verify plan hygiene**

```bash
rg -n "T[B]D|T[O]DO|FI[X]ME|placehold[e]r|\\?\\?|implement late[r]|fill in detail[s]|Similar t[o]|appropriate error handlin[g]" \
  docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md \
  docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md
```

Expected:

```text
<no output>
```

- [ ] **Step 3: Verify no inline Rust test module examples remain**

```bash
rg -n "mod tests [\\{]" \
  docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md
```

Expected:

```text
<no output>
```

- [ ] **Step 4: Verify markdown whitespace**

```bash
git diff --check -- \
  docs/superpowers/plans/2026-05-23-pr1-pure-type-crates-runtime-contracts.md \
  docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md \
  docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md
```

Expected:

```text
<no output>
```

## 8. Final Verification

After implementation:

```bash
cargo test -p uclaw-message-types
cargo test -p uclaw-tool-types
cargo test -p uclaw-runtime-contracts
cargo test -p uclaw-protocol-types
cd src-tauri
cargo test agent::types agent::regular_task agent::llm_stream runtime::rollout browser::rollout_bridge automation::rollout_bridge harness::case --lib
cargo check
```

Expected:

```text
test result: ok
Finished `dev` profile
```

## 9. Rollback

Rollback is a normal git revert. Manual rollback removes the four crates,
removes their workspace/dependency entries, and restores local definitions in
`src-tauri/src/agent/types.rs` and `src-tauri/src/runtime/contracts.rs`.

## 10. Out Of Scope

This PR does not change runtime behavior, event emission, ToolContext,
BrowserProvider, automation scheduling, team orchestration, frontend projection,
or migrations.
