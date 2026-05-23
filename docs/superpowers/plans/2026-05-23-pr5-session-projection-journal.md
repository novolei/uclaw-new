# PR-5 Session Projection Journal Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [x]`) syntax for tracking.

**Goal:** Make `TaskEvent` projection replay a first-class, testable backend surface without replacing SQLite conversation/session truth.

**Architecture:** PR-5 adds a derived-only `runtime::projection_journal` module that reads existing `RolloutRecord` JSONL and materializes compact startup stubs plus appendable projection journal entries. It does not change `TaskEvent`, `RolloutWriter`, Tauri startup, SQLite migrations, agent loop control flow, or frontend wiring.

**Tech Stack:** Rust, serde, std filesystem APIs, existing `runtime::rollout::RolloutRecord`, existing `uclaw-runtime-contracts::TaskEvent`.

---

## ADR §18 Answers

1. **Intent:** Give Agent OS v2 a replayable session/task projection layer that frontend and harness work can consume later.
2. **Autonomy:** No autonomy change; this PR observes completed runtime events only.
3. **Truth source:** `TaskEvent` rollout JSONL remains the source for projection replay; SQLite remains conversation/session historical truth and rollout index mirror.
4. **TaskEvent entries:** No new variants; PR-5 consumes `TaskStarted`, `Checkpoint`, `BoundaryYield`, `Warning`, and `TaskFinished`.
5. **Context:** Reads rollout JSONL records with rollout file and sequence provenance.
6. **Capabilities:** None; no tool/provider/browser capability behavior changes.
7. **Hooks:** None; no lifecycle hook wiring in this PR.
8. **Projection:** Produces `SessionProjectionStub` and `TaskProjectionSummary` as derived cache data.
9. **Harness:** Adds deterministic replay tests for completed, waiting, checkpointed, malformed, and round-trip cases.
10. **Rollback:** Remove the projection module/export/tests/docs; any generated projection files are safe to delete.
11. **Does not own:** DB schema, `agent_messages`, `conversations`, gbrain facts, UI reducer, Tauri commands, startup boot path, and rollout writer hot path.

## File Structure

- Create `src-tauri/src/runtime/projection_journal.rs`: derived projection types, reducer, filesystem store.
- Create `src-tauri/src/runtime/projection_journal_tests.rs`: sibling tests; no new inline Rust test bodies.
- Modify `src-tauri/src/runtime/mod.rs`: export `projection_journal`.
- Modify `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`: mark PR-5 in progress and record scope/verification.
- Create this plan file.

## Impact Summary

- `RolloutRecord`: LOW impact; consumed by new reducer, not modified.
- `drive_writer`: LOW impact; not modified.
- `replay_jsonl_into_sqlite`: LOW impact; not modified.
- `TaskEvent`: serialized contract, not modified.
- `runtime/mod.rs`: additive module export only; GitNexus target is ambiguous, so final `detect-changes` must verify affected processes.

## Task 1: Projection Reducer and Stub Types

**Files:**
- Create: `src-tauri/src/runtime/projection_journal.rs`
- Create: `src-tauri/src/runtime/projection_journal_tests.rs`
- Modify: `src-tauri/src/runtime/mod.rs`

- [x] **Step 1: Add failing sibling reducer tests**

Create tests that build `RolloutRecord` values in memory and call `SessionProjectionStub::from_records`.

Required assertions:

```rust
assert_eq!(task.status, TaskProjectionStatus::Completed);
assert_eq!(task.event_count, 3);
assert_eq!(task.last_kind.as_deref(), Some("task_finished"));
```

Also add waiting/checkpoint cases:

```rust
assert_eq!(task.status, TaskProjectionStatus::Waiting);
assert_eq!(task.boundary_reason.as_deref(), Some("waiting for user"));
assert!(!task.is_terminal);
```

- [x] **Step 2: Run tests and verify they fail before implementation**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::projection_journal`

Expected: compile failure because `projection_journal` does not exist.

- [x] **Step 3: Implement derived projection types**

Implement:

```rust
pub const PROJECTION_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskProjectionStatus {
    Running,
    Waiting,
    Checkpointed,
    Completed,
    Cancelled,
    Failed,
    BudgetExhausted,
}
```

Add `TaskProjectionSummary` with compact fields only:

- `task_id`
- `intent_id`
- `source`
- `first_ts`
- `last_ts`
- `last_kind`
- `status`
- `is_terminal`
- `event_count`
- `last_sequence`
- `checkpoint_ref`
- `boundary_reason`
- `warning_count`
- `source_rollout_file`

Add `SessionProjectionStub`:

- `schema_version`
- `generated_at`
- `source_rollout_file`
- `last_sequence`
- `malformed_line_count`
- `tasks`

- [x] **Step 4: Implement reducer semantics**

`SessionProjectionStub::from_records(records, generated_at)` should:

- group by `event.task_id()`;
- preserve max `sequence`;
- set `intent_id` from `TaskStarted` or `RolloutRecord.intent_id`;
- map `TaskFinished` verdicts to terminal statuses;
- map `BoundaryYield` to `Waiting` and `is_terminal = false`;
- map `Checkpoint` to `Checkpointed` and keep the latest checkpoint ref;
- keep `last_kind`, `last_ts`, `event_count`, and `warning_count`.
- treat `last_sequence` as the projection/file-order sequence, not the
  per-task `RolloutRecord.sequence` counter.

- [x] **Step 5: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::projection_journal`

Expected: all projection reducer tests pass.

## Task 2: Projection Journal Files and Corruption-Tolerant Startup Stub

**Files:**
- Modify: `src-tauri/src/runtime/projection_journal.rs`
- Modify: `src-tauri/src/runtime/projection_journal_tests.rs`

- [x] **Step 1: Add filesystem tests**

Add tests for:

- stub round-trip JSON;
- generated stub JSON does not contain heavy event payload keys such as `tokenUsage`;
- malformed rollout JSONL line increments `malformed_line_count` without panicking;
- invalid UTF-8 rollout JSONL bytes increment `malformed_line_count` without
  aborting projection replay;
- journal entry round-trip.

- [x] **Step 2: Implement filesystem helpers**

Add:

```rust
pub struct ProjectionJournalStore {
    root_dir: PathBuf,
}
```

With methods:

- `new(root_dir)`
- `stub_path_for_rollout(rollout_file)`
- `journal_path_for_rollout(rollout_file)`
- `write_stub(&self, stub)`
- `read_stub(&self, rollout_file)`
- `append_journal_entries(&self, rollout_file, entries)`
- `read_journal_entries_lossy(&self, rollout_file)`
- `build_stub_from_rollout_jsonl(rollout_file, generated_at)`

Use path-injected directories only. Do not call `uclaw_home()` in PR-5.

- [x] **Step 3: Implement compact journal entries**

Add `ProjectionJournalEntry` with:

- `sequence`
- `task_id`
- `ts`
- `kind`
- `source`
- `status`
- `is_terminal`
- `checkpoint_ref`
- `boundary_reason`

Derive entries from rollout records without copying the full `TaskEvent` payload.

- [x] **Step 4: Run focused tests**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::projection_journal`

Expected: reducer and filesystem tests pass.

## Task 3: Status Ledger and Verification

**Files:**
- Modify: `docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md`
- Modify: `docs/superpowers/plans/2026-05-23-pr5-session-projection-journal.md`

- [x] **Step 1: Update status ledger**

Mark PR-5 in progress on branch `codex/agent-os-jcode-pr5-projection-journal`, stacked on PR #402 until PR-4 merges.

- [x] **Step 2: Run final verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::projection_journal
cargo test --manifest-path src-tauri/Cargo.toml --lib runtime::rollout
cargo test -p uclaw-runtime-contracts
git diff --check -- docs/superpowers/plans/2026-05-23-pr5-session-projection-journal.md docs/superpowers/AGENT_OS_JCODE_UPGRADE_STATUS.md src-tauri/src/runtime/mod.rs src-tauri/src/runtime/projection_journal.rs src-tauri/src/runtime/projection_journal_tests.rs
npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/agent-os-jcode-pr5-projection-journal
```

Expected: all focused tests pass, diff check passes, GitNexus reports no unexpected high-risk affected processes.

## Self-Review

- Spec coverage: reducer, projection journal entries, startup stub, corruption tolerance, status ledger, and verification are covered.
- Placeholder scan: no TBD/TODO items; each task names exact files and commands.
- Type consistency: uses existing `RolloutRecord`, `TaskEvent`, `TaskEventSource`, and `TaskVerdict` without changing their wire shape.
