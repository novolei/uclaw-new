# Phase 4L - Task Runtime Pause Gate

## Summary

Phase 4J added the backend `paused_waiting_for_browser_runtime` status and
Phase 4K taught the frontend projection to render it. Phase 4L adds the first
runtime task gate: an explicit task-time `defer` decision can pause a
`browser_task` before browser automation starts, persist a checkpoint, and
return a `PausedWaitingForBrowserRuntime` run.

This slice deliberately avoids prompt dispatch and Settings IPC. It proves the
checkpoint/status behavior behind an explicit request field so later phases can
wire the real prompt without changing the pause contract.

## ADR Section 18 Questions

1. What user intent does this support?
   Users can defer Browser runtime preparation at task time without losing the
   task. The backend records the task as paused and resumable instead of
   launching a browser when runtime preparation is not ready.
2. What autonomy level can it run at?
   Low autonomy. The pause only happens when the caller explicitly passes
   `runtime_preparation_decision: "defer"` to `browser_task`.
3. What is the canonical truth source?
   `BrowserTaskRun.status` and the browser task store checkpoint are canonical.
   The request field is an input decision, not durable truth.
4. What TaskEvent entries does it emit?
   Phase 4J already maps paused-waiting runs to `Checkpoint` and
   `BoundaryYield` through rollout conversion. Phase 4L creates the underlying
   paused run/checkpoint; it does not add new event types.
5. What context does it read, and how is it cited?
   It reads only the `browser_task` request payload and existing task-store
   resume/checkpoint context. No external context or uncited web data is read.
6. What capability cards does it add or consume?
   None. It consumes the existing browser task capability and status contract.
7. What policy hooks can block it?
   The gate is policy-compatible because it is inert by default and only pauses.
   Runtime prepare/repair/install policy prompts remain in runtime-pack
   execution phases.
8. What world projection does the UI render?
   Existing Phase 4K UI projection renders the paused-waiting status. This
   phase changes the backend state that feeds that projection.
9. What harness cases prove it works?
   Focused Rust tests prove the runtime decision defaults to `ready`, `defer`
   creates a user-intervention pause step, and tool parsing accepts only
   `ready` or `defer`.
10. What is the rollback or disable path?
   Revert this PR. Without the request field, browser tasks use the prior ready
   behavior and do not enter the paused-waiting gate.
11. What does it deliberately not own?
   It does not own prompt UI dispatch, Tauri IPC, Settings persistence,
   runtime-pack execution, no-browser fallback execution, provider promotion,
   Playwright process launch, DB migrations, or DMZ file edits.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4l-task-runtime-pause-gate.md`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/tools.rs`
- `src-tauri/src/harness/adapters/browser.rs`

## Non-Goals

- Do not edit `tauri_commands.rs`, root `App`, `BEHAVIOR.md`, `CLAUDE.md`,
  `db/migrations.rs`, or workspace `Cargo.toml`.
- Do not add task-time prompt dispatch or ask-user UI wiring.
- Do not add Settings IPC, persistence, or runtime-pack execution.
- Do not launch Playwright, download runtime packs, delete files, or promote
  providers.
- Do not implement no-browser fallback execution.

## Impact Targets

- `BrowserTaskRequest` in `src-tauri/src/browser/agent_loop.rs`.
- `BrowserAgentLoop.run` in `src-tauri/src/browser/agent_loop.rs`.
- `BrowserTaskTool.parameters_schema` in `src-tauri/src/browser/tools.rs`.
- `BrowserTaskTool.execute` in `src-tauri/src/browser/tools.rs`.

GitNexus impact for all four existing symbols reported LOW risk with 0 direct
callers, 0 affected processes, and 0 affected modules before edits.

## Rollback

Revert the Phase 4L PR. The change has no migrations, persistent settings,
runtime downloads, provider promotion, or browser process side effects. Existing
tasks continue to default to `ready`.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs src-tauri/src/harness/adapters/browser.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
