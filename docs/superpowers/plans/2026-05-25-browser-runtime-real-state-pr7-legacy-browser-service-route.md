# Browser Runtime Real-State PR7 - Legacy BrowserService Route

## Goal

Remove the remaining live legacy `BrowserService` runtime path from the app
surface. Backward-compatible `browser_get_state`, `browser_launch`,
`browser_shutdown`, and `browser_take_screenshot` commands should no longer
own a separate chromiumoxide browser/page map. They must use the shared Rust
Browser Runtime status source and `BrowserContextManager` instead.

## ADR Section 18 Answers

1. User intent: legacy/canvas browser affordances should launch and report the
   same supervised local browser runtime as the rest of the app.
2. Autonomy: L0 user-triggered launch/status/screenshot commands only.
3. Canonical truth source: `BrowserRuntimeStatusService` plus the shared
   `BrowserContextManager`.
4. TaskEvent entries: none in this slice; these commands remain
   backward-compatible IPC utilities.
5. Context read: aggregate runtime status, active context sessions, and the
   legacy compatibility context tabs.
6. Capability cards: consumes the existing local browser runtime capability;
   adds no new capability.
7. Policy hooks: existing Tauri IPC boundary and Browser Runtime status checks;
   no new policy surface.
8. World projection: legacy `BrowserState` now reflects the shared runtime
   context instead of a private `BrowserService` page map.
9. Harness/tests: focused compile/unit tests for browser runtime status and the
   command module paths touched by the compatibility commands.
10. Rollback: revert this PR to restore the old `BrowserService` field and
    commands.
11. Deliberately not owned: no Startup Splash/App frontend handoff, no Browser
    Panel UI, no UI IPC command routing, no direct tool routing, no
    runtime-pack install/repair/delete, no provider promotion, no hosted
    provider, and no TaskEvent persistence.

## Allowed Files

- `src-tauri/src/app.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/tauri_commands.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Implementation Plan

1. Add a small legacy compatibility session id in `tauri_commands.rs`.
2. Rework `browser_get_state` to inspect `BrowserRuntimeStatusService` and, if
   the compatibility context is running, read tabs from `BrowserContextManager`.
3. Rework `browser_launch`, `browser_shutdown`, and
   `browser_take_screenshot` to status-touch the shared runtime and operate on
   the compatibility context through `BrowserContextManager`.
4. Remove `AppState.browser_service` and the `BrowserService` implementation
   from `browser/mod.rs` so the old private chromiumoxide runtime is no longer
   live app code.
5. Update the tracker with PR7 scope, impact notes, verification, and next
   action.

## Impact Notes

- `npx gitnexus analyze` was refreshed in the PR7 worktree before edits.
- GitNexus pre-edit impact for `BrowserService` in `src-tauri/src/browser/mod.rs`
  was LOW with 0 affected processes.
- GitNexus pre-edit impact for `AppState` in `src-tauri/src/app.rs` was LOW
  with 0 affected processes.
- GitNexus pre-edit impact for `BrowserRuntimeStatusService` was LOW with 0
  affected processes.
- GitNexus cannot resolve `src-tauri/src/tauri_commands.rs` because the analyzer
  skips it as the one large file over the 512KB threshold; this PR keeps that
  edit to the four legacy compatibility commands and focused helpers.

## Verification Plan

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib tauri_commands::browser_legacy_runtime_tests`
- `git diff --check -- <changed-files>`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr7-legacy-browser-service-route`
