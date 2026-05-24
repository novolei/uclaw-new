# Browser Runtime Phase 4J - Paused Waiting Runtime Contract

## Scope

Phase 4J adds the backend browser-task status contract for
`paused_waiting_for_browser_runtime`. It is a conversion/persistence contract
slice only: no agent-loop wiring, no task-time prompt integration, and no real
checkpoint writes are added.

## ADR Section 18 Answers

1. User intent: preserve task state when browser runtime is unavailable and the
   user defers preparation.
2. Autonomy level: L0/L1 contract and event mapping only.
3. Canonical truth source: browser task runs and TaskEvent conversion remain
   the backend truth for browser task boundaries.
4. TaskEvent entries: maps the new status to a checkpoint plus
   `BoundaryYield` reason that the task is waiting for Browser runtime.
5. Context read/citation: reads only `BrowserTaskRun.status` in the rollout
   conversion helper and task-store status strings.
6. Capability cards: none added or consumed.
7. Policy hooks: no policy hook is invoked.
8. World projection: aligns the persisted status and rollout boundary with the
   existing `BrowserTaskBoundaryStatus::PausedWaitingForRuntime` projection.
9. Harness cases: focused Rust tests cover status string roundtrip and rollout
   conversion for paused-waiting runs.
10. Rollback/disable path: revert this PR; no runtime writes or migrations are
    introduced.
11. Deliberately not owned: agent-loop prompt wiring, IPC, UI integration,
    task-time prompt action dispatch, real checkpoint persistence, provider
    execution, DB migration, and runtime side effects.

## Allowed Files

- `src-tauri/src/browser/session_state.rs`
- `src-tauri/src/browser/task_store.rs`
- `src-tauri/src/browser/rollout_bridge.rs`
- `src-tauri/src/browser/rollout_bridge_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4j-paused-waiting-contract.md`

## Non-Goals

- No changes to `agentic_loop.rs`, `tauri_commands.rs`, migrations, root
  workspace manifests, frontend prompt wiring, or Settings UI.
- No new task runtime caller writes the new status yet.
- No real Browser runtime preparation, downloads, cleanup, rollback, or
  provider promotion.

## Impact Targets

- `BrowserTaskStatus`
- `status_to_str`
- `status_from_str`
- `browser_run_to_events`

Pre-edit GitNexus impact was LOW for the enum and status serializers, and
MEDIUM for `browser_run_to_events` through the expected rollout bridge/test
callers.

## Implementation

- Add `PausedWaitingForBrowserRuntime` to `BrowserTaskStatus`.
- Roundtrip it through task-store status strings.
- Map it in `browser_run_to_events` to a checkpoint ref and a boundary yield
  without a terminal `TaskFinished`.
- Add focused tests for persistence string roundtrip and rollout conversion.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::task_store`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>`
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes` on staged changes.

## Rollback

Revert the Phase 4J PR. The rollback removes the new status variant, status
string mapping, rollout conversion, focused tests, this plan, and the tracker
update.
