# Phase 6D - Identity Active-Task Drain Tracker

## Goal

Make browser identity revocation visible to running browser tasks: report active
identity-backed tasks in identity IPC, mark revocation with a bounded drain
deadline, and let `BrowserAgentLoop` checkpoint affected tasks at safe action
boundaries instead of continuing with a revoked identity.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users can revoke a browser identity and trust uClaw to stop new identity
     use while letting current browser work reach a safe checkpoint.
2. What autonomy level can it run at?
   - L1-L3 only. Revocation is user-triggered; autonomous browser work only
     observes the revocation and checkpoints.
3. What is the canonical truth source?
   - Backend identity metadata remains the grant truth. A process-local
     identity task registry is the live active-task truth for the current app
     process. Browser task store/checkpoints remain canonical persisted task
     state.
4. What TaskEvent entries does it emit?
   - This PR emits browser task steps/checkpoints and long-term browser memory
     final state. It does not add DB `TaskEvent` rows; a later UI choice phase
     can map the checkpoint to the global TaskEvent ledger.
5. What context does it read, and how is it cited?
   - It reads active `BrowserTaskRun` metadata, profile id, session id, run id,
     task title, status, and drain deadline. It does not read or expose
     `secret_handle`, cookies, storage state, screenshots, or raw page text.
6. What capability cards does it add or consume?
   - It consumes the existing browser identity and browser task contracts; no
     provider capability card is added.
7. What policy hooks can block it?
   - Revoked profiles already block new profile resolve/load. This slice adds
     a runtime drain gate in `BrowserAgentLoop`; future user-choice policy owns
     isolated-profile fallback, reauthorize, or end-task actions.
8. What world projection does the UI render?
   - Identity IPC reports `activeTaskCount` and safe active-task summaries so
     Settings can display live work instead of `null`.
9. What harness cases prove it works?
   - Focused unit tests cover active-task registration, revocation drain
     deadlines, IPC counts/summaries, and checkpoint-step construction for
     revoked identity tasks.
10. What is the rollback or disable path?
    - Revert this PR. Identity revoke returns to Phase 6B behavior: secret
      deletion and new-action blocking without active-task drain reporting.
11. What does it deliberately not own?
    - Authorization WebView, Settings connect/import flow, global TaskEvent DB
      emission, user choice UI after checkpoint, isolated-profile fallback,
      reauthorize flow, payment confirmation, provider promotion, DB migration,
      or external Chrome profile attach.

## Allowed Files

- `src-tauri/src/browser/identity_tasks.rs`
- `src-tauri/src/browser/identity_ipc.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/tools.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/app.rs`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/lib/tauri-bridge.browser-identity.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6d-identity-drain-tracker.md`

## Non-Goals

- No `agentic_loop.rs` or `tauri_commands.rs` edits unless implementation
  proves a real integration gap. Avoiding those files must not leave behavior
  in a dry-run lane.
- No schema migration.
- No auth WebView or connect/import UI.
- No provider promotion or Playwright MCP work.
- No raw auth material in IPC or UI types.
- No global TaskEvent ledger write until the follow-up user-choice phase.

## Impact Targets

- `AppState`: add one shared registry field initialized at app startup.
- `BrowserAgentLoop`: register identity-backed runs and checkpoint after
  revocation drain expires.
- `BrowserTaskTool` / `BrowserTaskResumeTool`: pass the shared registry into
  the loop.
- `list_browser_identities` / `revoke_browser_identity`: include active-task
  counts and drain summaries.
- `BrowserRuntimeSettings` should not need UI edits in this slice; existing
  `activeTaskCount` rendering should start showing a number through bridge
  types.

## Implementation Plan

1. Add `BrowserIdentityTaskRegistry` with active-task summaries, drain
   deadlines, revocation state, and focused tests.
2. Wire `AppState` to own one registry and pass it into browser task tools.
3. Wire `BrowserAgentLoop` to register identity-backed tasks, update status,
   and checkpoint when a revocation drain deadline has expired at an action
   boundary.
4. Extend identity IPC helper functions and bridge types to return
   `activeTaskCount` and safe `activeTasks`.
5. Update the tracker and run focused Rust/TS verification plus GitNexus
   staged detect.

## Rollback

Revert this PR. Running tasks will no longer observe live identity revocation
state, but Phase 6A/6B revoked-profile blocking and Settings revoke remain
available.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_tasks`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
- `rustfmt --edition 2021 --check <changed-rust-files>`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect_changes`
