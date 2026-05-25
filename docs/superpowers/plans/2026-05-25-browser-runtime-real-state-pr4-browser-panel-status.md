# Browser Runtime Real State PR4 Plan

Date: 2026-05-25
Branch: `codex/browser-runtime-real-state-pr4-direct-tool-guard`
Worktree: `/Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr4-direct-tool-guard`
Base: `origin/main` at PR #506 merge commit `685d15ad`

## Goal

Make the in-app Browser Panel consume the same Rust aggregate Browser Runtime
status source that Startup and Settings use. The panel status bar should render
the real `BrowserRuntimeStatusService` supervisor state instead of only a local
frontend "ready/loading" label.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users inspecting or interacting with the embedded browser should see the
     Rust-owned Browser Runtime supervisor state in the browser surface itself.
2. What autonomy level can it run at?
   - L0/L1 UI projection only. It reads runtime status and does not execute or
     mutate runtime-pack operations.
3. What is the canonical truth source?
   - `get_browser_runtime_status`, backed by `BrowserRuntimeStatusService`.
4. What TaskEvent entries does it emit?
   - None. This PR is UI read/projection only.
5. What context does it read, and how is it cited?
   - Browser runtime supervisor state, active context count, provider readiness,
     and runtime-pack status from the Rust aggregate status DTO.
6. What capability cards does it add or consume?
   - It consumes the existing local Chromium / Playwright provider readiness
     summary exposed by the aggregate status service.
7. What policy hooks can block it?
   - Existing IPC error handling, Browser Runtime Settings recovery flow, and
     the CRITICAL GitNexus UI review gate for BrowserPanel/BrowserStatusBar.
8. What world projection does the UI render?
   - The Browser Panel status bar renders supervisor runtime state, doctor
     status, active context count, and runtime-status failures.
9. What harness cases prove it works?
   - Focused BrowserPanel/BrowserStatusBar tests verify status fetch,
     supervisor label rendering, and graceful fallback on status errors.
10. What is the rollback or disable path?
   - Revert this PR. Browser Panel returns to its previous local ready/loading
     status label.
11. What does it deliberately not own?
   - No Startup Splash/App handoff, no Settings execution, no direct browser
     command execution guard, no runtime-pack install/repair/delete, no provider
     default promotion, and no TaskEvent persistence.

## Impact

- GitNexus pre-edit impact for `BrowserPanel` reported CRITICAL:
  4 direct callers, 2 affected processes (`KaleidoscopeShell`, `AppShell`).
- GitNexus pre-edit impact for `BrowserStatusBar` reported CRITICAL:
  1 direct caller (`BrowserPanel`), 1 affected process (`KaleidoscopeShell`).
- This PR is allowed to proceed under the active goal, but must remain additive
  and requires fresh-review acceptance before merge.

## Implementation

- Extend the frontend runtime status type to include optional aggregate
  supervisor/provider/projection fields returned by Rust.
- Have `BrowserPanel` fetch `getBrowserRuntimeStatus()` when mounted and after
  initial navigation creates a live tab.
- Pass loading/error/report state into `BrowserStatusBar`.
- Render a compact runtime status chip from the Rust supervisor state, with
  fallback to the existing local ready/loading label if the report is absent.
- Update focused tests.

## Verification

- `cd ui && npm test -- --run src/components/browser/BrowserPanel.test.tsx src/components/browser/BrowserStatusBar.test.tsx`
- `cd ui && npm run build`
- `git diff --check -- ui/src/components/browser/BrowserPanel.tsx ui/src/components/browser/BrowserPanel.test.tsx ui/src/components/browser/BrowserStatusBar.tsx ui/src/components/browser/BrowserStatusBar.test.tsx ui/src/lib/startup/startup-doctor.ts docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr4-browser-panel-status.md`
- GitNexus `detect_changes`
