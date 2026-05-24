# Phase 6F - Identity Boundary Actions

## Goal

Add an explicit browser-task resume contract for revoked or missing identity
boundaries: continue with an isolated profile, resume with explicit
reauthorization, or end the task.

## ADR Section 18 Questions

1. User intent: recover a checkpointed browser task after its authorized
   identity was revoked or is unavailable.
2. Autonomy level: L1/L2 resume control. The agent may continue only after the
   user-visible boundary has an explicit decision encoded in the tool call.
3. Canonical truth source: browser task checkpoints, `BrowserAgentLoop`, and the
   `browser_task_resume` tool contract.
4. TaskEvents: this slice records browser task steps/checkpoints only; fuller
   TaskEvent projection remains a later Phase 6 harness/UI slice.
5. Context read/citation: no new model-visible browser observation is created
   before the identity boundary decision is resolved.
6. Capability cards: no provider promotion or new provider card.
7. Policy hooks: implicit revoked-identity resume remains blocked; explicit
   `isolated_profile`, `reauthorize`, or `end_task` decisions are recorded as
   boundary actions.
8. World projection: browser task run output exposes the boundary action step so
   UI/projection consumers can render the recovery choice.
9. Harness cases: focused Rust tests cover default blocking, isolated-profile
   opt-in, reauthorize validation, and end-task step shape.
10. Rollback/disable: revert this PR; revoked-identity resume returns to the
    Phase 6D behavior where implicit resume is blocked until explicit auth is
    supplied.
11. Non-ownership: no authorization WebView, Settings connect/import flow,
    payment confirmation, hosted provider, provider promotion, DB migration, or
    frontend recovery UI in this slice.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6f-identity-boundary-actions.md`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/tools.rs`
- `src-tauri/src/harness/adapters/browser.rs`

## Impact Targets

- `BrowserTaskRequest` in `src-tauri/src/browser/agent_loop.rs`
- `BrowserAgentLoop::run` in `src-tauri/src/browser/agent_loop.rs`
- `BrowserTaskResumeTool::parameters_schema` in `src-tauri/src/browser/tools.rs`
- `BrowserTaskResumeTool::execute` in `src-tauri/src/browser/tools.rs`
- `BrowserTaskTool::parameters_schema` in `src-tauri/src/browser/tools.rs`
- `BrowserTaskTool::execute` in `src-tauri/src/browser/tools.rs`
- `BrowserParityCase::to_task_request` in
  `src-tauri/src/harness/adapters/browser.rs`

Pre-edit GitNexus impact for all resolved targets is LOW. The local helper
`identity_revocation_resume_blocked_step` was not resolved by the index; it is
only called inside `BrowserAgentLoop::run`.

## Implementation Plan

1. Add `BrowserIdentityResumeDecision` with safe default `RequireAuth`.
2. Keep implicit revoked-identity checkpoint resume blocked.
3. Allow explicit `isolated_profile` resume to drop checkpoint auth metadata.
4. Require explicit replacement auth for `reauthorize`.
5. Allow explicit `end_task` to stop the run with a boundary step and checkpoint.
6. Expose `identity_resume_decision` on `browser_task_resume`.
7. Add focused parser and step-shape tests plus default browser-runtime
   regressions.

## Rollback

Revert this PR. No schema, provider, profile-store, or runtime-pack data is
changed.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs`
- `git diff --check`
- GitNexus `detect_changes`
