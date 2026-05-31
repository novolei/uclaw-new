# Effect-Typed Tool Scheduling Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans or superpowers:subagent-driven-development to execute this plan task-by-task.

**Goal:** Borrow Pi Rust `ToolEffects` into uClaw so tool scheduling is derived
from effect compatibility while preserving the existing dispatcher behavior.

**Architecture:** Add a deeper tool-effect Interface first, then move
dispatcher batching behind one planning helper. `ToolConcurrency` stays as a
compatibility adapter during migration.

**Tech Stack:** Rust, Tokio `JoinSet`, existing `Tool` trait, existing
`ToolDispatcher`, GitNexus, focused Rust tests.

## Required GitNexus Checks

Before editing production code, run impact analysis on:

- `Tool`
- `ToolConcurrency`
- `dispatch_inner`
- `emit_tool_start`
- `ReadFileTool::concurrency`
- `GetFileSkeletonTool::concurrency`
- `BashTool::concurrency`

Record risk level, direct callers, and affected processes in this plan's
execution notes. HIGH or CRITICAL results must be reported before edits.

## Task 1: Add failing `ToolEffects` tests

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs`

- [x] **Step 1: Add unit tests for effects**

Add tests proving:

```rust
assert!(ToolEffects::read().parallel_safe());
assert!(!ToolEffects::write().parallel_safe());
assert!(!ToolEffects::process().parallel_safe());
assert!(ToolEffects::read().compatible_with(ToolEffects::network()));
```

- [x] **Step 2: Verify RED**

Run:

```bash
cd src-tauri && cargo test --lib agent::tools::tool::effects_tests -- --nocapture
```

Expected: compile failure because `ToolEffects` does not exist in uClaw yet.

## Task 2: Implement `ToolEffects`

**Files:**
- Modify: `src-tauri/src/agent/tools/tool.rs`

- [x] **Step 1: Add `ToolEffects`**

Copy the Pi Rust shape into uClaw with these public methods:

- `read`
- `write`
- `append`
- `network`
- `process`
- `union`
- `reads`
- `writes`
- `appends`
- `networks`
- `processes`
- `labels`
- `parallel_safe`
- `compatible_with`

- [x] **Step 2: Add `Tool::effects()`**

Default to `ToolEffects::write()` so undeclared tools serialize fail-closed.

- [x] **Step 3: Derive default `Tool::concurrency()` from effects**

Keep the existing `ToolConcurrency` enum, but make the default implementation:

```rust
if self.effects().parallel_safe() {
    ToolConcurrency::Parallel
} else {
    ToolConcurrency::Sequential
}
```

- [x] **Step 4: Classify initial builtins**

`ReadFileTool` and `GetFileSkeletonTool` return `ToolEffects::read()`.
`BashTool` returns `ToolEffects::process()`. Leave other tools on the
fail-closed default unless already proven read-only.

- [x] **Step 5: Verify GREEN**

Run:

```bash
cd src-tauri && cargo test --lib agent::tools::tool -- --nocapture
```

Expected: tool trait and effects tests pass.

## Task 3: Add failing batch-plan tests

**Files:**
- Modify: `src-tauri/src/agent/tool_dispatch/mod.rs`

- [x] **Step 1: Add planner tests**

Add tests proving:

- read/read calls produce one concurrent batch with two call IDs.
- read/write/read produces three batches.
- read/process/read produces three batches.
- unknown/read produces two batches, with unknown as a barrier.

- [x] **Step 2: Verify RED**

Run:

```bash
cd src-tauri && cargo test --lib agent::tool_dispatch::tests::tool_effect_batch_plans -- --nocapture
```

Expected: compile failure because the batch planner does not exist yet.

## Task 4: Implement batch planning

**Files:**
- Modify: `src-tauri/src/agent/tool_dispatch/mod.rs`

- [x] **Step 1: Add planner types**

Add private test-visible types:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ToolBatchKind { Concurrent, Barrier }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatchPlan { pub batches: Vec<ToolBatch> }

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ToolBatch { pub kind: ToolBatchKind, pub call_ids: Vec<String>, pub effects: Vec<Vec<&'static str>> }
```

- [x] **Step 2: Add `plan_tool_batches`**

Given `&ToolRegistry` and `&[ToolCall]`, resolve each call's effects. Unknown
tools use `ToolEffects::write()`. Consecutive compatible calls share a
`Concurrent` batch. Barrier calls drain the current batch and form their own
`Barrier` batch.

- [x] **Step 3: Use the planner in `dispatch_inner`**

Preserve output ordering. For each concurrent batch, spawn all calls and collect
their outcomes. For each barrier batch, run the single call inline after the
previous concurrent set is drained.

- [x] **Step 4: Add trace evidence**

Emit one debug trace with batch count and labels before execution. This is
evidence only; it must not change event payloads.

- [x] **Step 5: Verify GREEN**

Run:

```bash
cd src-tauri && cargo test --lib agent::tool_dispatch::tests::tool_effect_batch_plans -- --nocapture
cd src-tauri && cargo test --lib agent::tool_dispatch -- --nocapture
```

Expected: planner tests and existing dispatcher tests pass.

## Task 5: Focused verification and review

- [x] **Step 1: Run focused tests**

Run:

```bash
cd src-tauri && cargo test --lib agent::tools::tool -- --nocapture
cd src-tauri && cargo test --lib agent::tool_dispatch -- --nocapture
git diff --check
```

- [x] **Step 2: Run search verification**

Run:

```bash
rg -n "ToolEffects|fn effects|fn concurrency\\(" src-tauri/src/agent/tools src-tauri/src/agent/tool_dispatch
```

Expected: `ToolEffects` is the new scheduling Interface and old
`concurrency()` overrides are limited to compatibility cases.

- [x] **Step 3: Run GitNexus detect-changes**

Run staged GitNexus detect-changes and confirm the changed scope is tool
trait/dispatcher scheduling.

- [x] **Step 4: Code review**

Review against this spec. Critical or Important findings must be fixed before
the commit.

- [x] **Step 5: Commit**

Commit with the focused verification commands and expected pass counts in the
body.
