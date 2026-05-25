# Browser Runtime Real State PR2 Plan

Date: 2026-05-25
Branch: `codex/browser-runtime-real-state-pr2-splash-app-state`
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr2-splash-app-state`
Base: `origin/main` after PR #503 merge commit `52808ed1`

## Goal

Make production startup handoff consume the Rust aggregated Browser Runtime
status source created in PR1. `App` should own the startup runtime status
request, keep `StartupSplash` rendering the real Rust supervisor projection,
and defer the AppShell handoff until Rust has returned a browser-runtime status
or a bounded failure fallback is recorded. This PR does not execute runtime
actions, install/delete runtime packs, change provider defaults, or reroute
browser tools.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users should not enter the app shell from a purely UI-timed splash while
     browser runtime truth is still unknown. Startup should reflect Rust-owned
     Browser Runtime state before app run begins.
2. What autonomy level can it run at?
   - L1/L2 read-only startup diagnostics. It calls the existing status IPC and
     renders its result; it does not prepare, repair, download, delete, or
     launch a provider lane.
3. What is the canonical truth source?
   - The PR1 `get_browser_runtime_status` IPC backed by
     `BrowserRuntimeStatusService` in Rust.
4. What TaskEvent entries does it emit?
   - None. Existing status/event-name fields are read for projection only.
5. What context does it read, and how is it cited?
   - It reads the serialized status returned by Rust: runtime-pack fields,
     supervisor state, provider readiness, projection, and event names. It does
     not read cookies, storage, page contents, or user secrets.
6. What capability cards does it add or consume?
   - It consumes existing local Chromium / Playwright provider readiness fields
     in the aggregate status. It adds no card.
7. What policy hooks can block it?
   - GitNexus impact/detect-changes, focused frontend tests, and existing
     startup error fallback behavior. If final GitNexus detect is HIGH because
     `App` is a root orchestration node, request review before merge.
8. What world projection does the UI render?
   - `StartupSplash` renders the Startup Doctor view derived from Rust status;
     later PRs can render the richer `projection` fields directly.
9. What harness cases prove it works?
   - App startup tests prove the root handoff waits for runtime status success
     or failure plus minimum splash visibility. StartupSplash tests prove it
     uses parent-provided Rust status instead of issuing a second status call.
10. What is the rollback or disable path?
   - Revert this PR. The old minimum-visible splash and independent
     `StartupSplash` status read return.
11. What does it deliberately not own?
   - No Settings real execution, runtime-pack install/repair/delete,
     BrowserPanel/screencast routing, browser action supervisor guards,
     provider promotion, DB migration, or TaskEvent persistence.

## Implementation

- Move startup browser-runtime status ownership up to `App`.
- Pass the Rust status, loading state, and failure state into `StartupSplash`.
- Require the root handoff gate to wait for app initialization, minimum splash
  visibility, and runtime status completion.
- Preserve bounded fallback on status IPC failure so app startup does not hang.
- Bound pending status IPC with a startup-only timeout so app startup still
  records a Browser Runtime warning projection if Rust status never returns.
- Keep Settings and task-time runtime status consumers unchanged.

## Verification

- `cd ui && npm test -- --run src/App.test.tsx src/components/startup/StartupSplash.test.tsx`
  - Passed: `2 files / 16 tests`.
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`
  - Passed: `1 file / 2 tests`.
- `cd ui && npm run build`
  - Passed with existing dynamic-import and chunk-size warnings.
- `git diff --check -- ui/src/App.tsx ui/src/App.test.tsx ui/src/components/startup/StartupSplash.tsx ui/src/components/startup/StartupSplash.test.tsx ui/src/lib/startup/startup-doctor.ts docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr2-splash-app-handoff.md`
  - Passed.
- GitNexus `detect_changes(scope=all)`
  - HIGH because root `App` participates in top-level startup/listener/settings/model flows.
  - Changed scope remained limited to startup handoff, Startup Splash projection, startup doctor types, tests, and tracker/plan docs.
  - This PR requires fresh review before merge.
