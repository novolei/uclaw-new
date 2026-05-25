# Browser Runtime Real State PR3 Plan

Date: 2026-05-25
Branch: `codex/browser-runtime-real-state-pr3-task-runtime-status`
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr3-task-runtime-status`
Base: `origin/main` at `35187bf3` after PR #505

## Goal

Make task-time autonomous Browser agent execution consume the Rust aggregate
Browser Runtime status source created in PR1. `BrowserAgentLoop` should route
browser actions with a fresh runtime-pack snapshot from
`BrowserRuntimeStatusService` instead of the default empty provider options.
This PR does not depend on PR #504 and does not change the frontend Startup
Splash handoff.

## ADR Section 18 Questions

1. What user intent does this support?
   - Browser tasks should run against real Rust-owned Browser Runtime readiness
     rather than a default route model that has no runtime-pack state.
2. What autonomy level can it run at?
   - L2 task-time execution routing. It reads runtime status and uses existing
     provider route policy; it does not install/delete runtime packs or promote
     providers.
3. What is the canonical truth source?
   - `BrowserRuntimeStatusService::inspect_default()` and its
     `BrowserRuntimeStatusReport.runtime_pack` snapshot.
4. What TaskEvent entries does it emit?
   - No new TaskEvent persistence. Existing provider route evidence emission
     remains unchanged.
5. What context does it read, and how is it cited?
   - Runtime-pack readiness, provider readiness inputs, and active browser
     session state from the Rust aggregate status service. It does not read page
     content, cookies, storage, or user secrets.
6. What capability cards does it add or consume?
   - It consumes existing local Chromium / Playwright provider readiness and
     runtime-pack capability metadata. It adds no card.
7. What policy hooks can block it?
   - Existing provider route policy, runtime-pack readiness, feature flags,
     GitNexus impact/detect-changes, and focused Rust tests.
8. What world projection does the UI render?
   - No UI projection change in this PR. The runtime snapshot comes from the
     same aggregate status that exposes projection fields for UI consumers.
9. What harness cases prove it works?
   - Focused Rust tests prove `BrowserAgentLoop` injects a runtime-pack snapshot
     into provider routing and falls back to default route options when status
     inspection fails.
10. What is the rollback or disable path?
   - Revert this PR. Browser task routing returns to default provider options.
11. What does it deliberately not own?
   - No Settings real execution, no direct browser tool supervisor guard, no
     runtime-pack install/repair/delete, no provider default promotion, no
     Startup Splash handoff, and no TaskEvent persistence.

## Implementation

- Add optional `BrowserRuntimeStatusService` dependency to `BrowserAgentLoop`.
- Pass the service from `BrowserTaskTool`, `BrowserTaskResumeTool`, and
  `RetryWithBrowserAgentTool` registrations.
- Before routing a task-time browser action, inspect the Rust aggregate status
  and build `BrowserProviderActionRouteOptions` with the real runtime-pack
  report.
- Preserve bounded fallback: if status inspection fails, route with default
  provider options and log a warning rather than failing unrelated local
  Chromium task execution.
- Add focused Rust tests around provider route options injection.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
- `rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs`
- `git diff --check -- src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/tools.rs src-tauri/src/tauri_commands.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr3-task-runtime-status.md`
- `npx gitnexus analyze` in this worktree, then GitNexus `detect_changes(scope=all)`

Note: `src-tauri/src/tauri_commands.rs` is not rustfmt-clean in the current
main-branch baseline and GitNexus skips it as the one large file over the
512KB analyzer threshold. PR3 keeps its edit there to the two browser tool
registration injection sites and verifies whitespace with `git diff --check`.
