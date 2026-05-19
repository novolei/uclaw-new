# UCLAW Harness Runtime Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the generic UCLAW Harness Runtime Core so agent loop, browser, tools, permissions, hooks, memory, gbrain, skills, tasks, coordinator, and prompts can be evaluated through one common case/episode/trace/artifact/grader model.

**Architecture:** Extend the existing `src-tauri/src/harness` module instead of creating a parallel harness. Keep current `TrajectoryStore` and `ToolBudgetManager` intact, then add generic runtime modules beside them: case, episode, trace, artifacts, graders, adapters, and runtime.

**Tech Stack:** Rust/Tauri v2, Serde, chrono, uuid, tempfile for tests, existing `src-tauri/src/harness/*`.

---

## File Structure

Create:

- `src-tauri/src/harness/case.rs` — harness case, subject, fixture, policy, budget, assertion types.
- `src-tauri/src/harness/episode.rs` — run episode, verdict, score map, lifecycle helpers.
- `src-tauri/src/harness/trace.rs` — typed event enum for model/tool/permission/boundary/memory/checkpoint events.
- `src-tauri/src/harness/artifacts.rs` — artifact metadata and filesystem artifact store.
- `src-tauri/src/harness/graders.rs` — grader specs, result shape, registry, and built-in rule graders.
- `src-tauri/src/harness/adapters/mod.rs` — stable adapter trait and placeholder subject adapter IDs.
- `src-tauri/src/harness/runtime.rs` — in-process runtime that starts episodes, appends events, stores artifacts, and grades.

Modify:

- `src-tauri/src/harness/mod.rs` — export the new modules without breaking `TrajectoryStore` and `ToolBudgetManager`.

---

## Task 1: Core Types and Serialization

- [ ] **Step 1: Add failing serialization tests**

Create tests in `src-tauri/src/harness/case.rs`, `episode.rs`, and `trace.rs` that assert camelCase JSON and all required subjects.

- [ ] **Step 2: Implement core types**

Add `HarnessSubject`, `HarnessCase`, `HarnessEpisode`, `HarnessVerdict`, `HarnessEvent`, `HarnessPolicy`, `HarnessBudget`, and `HarnessAssertion`.

- [ ] **Step 3: Verify**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::case harness::episode harness::trace --lib
```

Expected: all tests pass.

---

## Task 2: Artifact Store

- [ ] **Step 1: Write filesystem artifact test**

Test that a JSON artifact is persisted under a run directory and returns a stable `HarnessArtifact`.

- [ ] **Step 2: Implement `HarnessArtifactStore`**

Use `data_dir/harness-artifacts/{run_id}/{artifact_id}.json` as the default path shape.

- [ ] **Step 3: Verify**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness::artifacts --lib
```

Expected: the artifact test writes and reads JSON successfully.

---

## Task 3: Runtime and Graders

- [ ] **Step 1: Write runtime lifecycle test**

Test start episode -> append tool event -> attach artifact -> run grader -> finish episode.

- [ ] **Step 2: Implement `HarnessRuntime`**

Keep the first implementation in-memory. Do not add SQLite migration yet. The runtime should be suitable for dry-run and unit test use before UI persistence lands.

- [ ] **Step 3: Implement built-in graders**

Add:

- `event_exists`: pass when an event kind exists.
- `verdict_is`: pass when final verdict matches.

- [ ] **Step 4: Verify**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness:: --lib
```

Expected: all harness tests pass.

---

## Task 4: Integration Boundary

- [ ] **Step 1: Export modules from `harness/mod.rs`**

Keep:

```rust
pub use trajectory::TrajectoryStore;
pub use budget::ToolBudgetManager;
```

Add exports for the new runtime types.

- [ ] **Step 2: Run broader compile check**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml harness:: --lib
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: harness tests pass and the app compiles.

---

## Out of Scope

- No UI dashboard yet.
- No SQLite episode migration yet.
- No browser identity integration yet.
- No memory/gbrain adapter implementation yet.
- No automated skill promotion gate yet.

These become follow-up PRs once the runtime core is stable.
