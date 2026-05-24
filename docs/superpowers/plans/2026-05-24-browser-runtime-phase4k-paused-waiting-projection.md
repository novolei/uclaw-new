# Phase 4K - Paused-Waiting Task Projection

## Summary

Phase 4J added the backend `paused_waiting_for_browser_runtime` task status and
rollout conversion contract. Phase 4K makes that status visible and typed in the
frontend browser task projection before deeper task-runtime prompt dispatch.

This is intentionally split from real runtime gating because true task-time
prepare/defer wiring touches browser task execution boundaries. Phase 4K is a
small projection compatibility PR; Phase 4L can own prompt dispatch and runtime
pause creation.

## ADR Section 18 Questions

1. What user intent does this support?
   Users who defer Browser runtime preparation should see that a browser task is
   waiting for runtime setup instead of seeing an unknown or generic status.
2. What autonomy level can it run at?
   Read/render only. It observes emitted browser task events and updates local
   UI projection; it does not initiate runtime preparation or browser actions.
3. What is the canonical truth source?
   `BrowserTaskRun.status` emitted by the Rust browser task/event model remains
   canonical. Frontend atoms mirror that status for rendering only.
4. What TaskEvent entries does it emit?
   None. Phase 4J already mapped paused-waiting runs to checkpoint and boundary
   yield events; Phase 4K only renders the status.
5. What context does it read, and how is it cited?
   It reads Tauri `browser:task-run` / `browser:task-step` payloads and the
   existing `browserTaskRunAtom` projection. UI text does not cite external
   context.
6. What capability cards does it add or consume?
   None. It consumes the existing Browser task projection surface.
7. What policy hooks can block it?
   None in this slice. Policy hooks for prepare/repair/runtime execution remain
   in runtime-pack and future task-time wiring phases.
8. What world projection does the UI render?
   The Browser task monitor renders `paused_waiting_for_browser_runtime` as a
   waiting/checkpoint status with clear copy that the task is waiting for
   Browser runtime preparation.
9. What harness cases prove it works?
   Focused Vitest coverage should prove the task monitor renders the
   paused-waiting status and that task event hooks store the new status.
10. What is the rollback or disable path?
   Revert the type additions, monitor copy/style change, tests, tracker update,
   and this plan. Backend status remains harmless if the frontend falls back to
   raw status text.
11. What does it deliberately not own?
   It does not own prompt dispatch, ask-user flows, runtime-pack execution,
   backend IPC, agent/browser loop changes, checkpoint creation, DB migrations,
   provider selection, Playwright execution, or Settings behavior.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4k-paused-waiting-projection.md`
- `ui/src/atoms/browser-atoms.ts`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/components/browser/BrowserTaskMonitor.tsx`
- `ui/src/components/browser/BrowserTaskMonitor.test.tsx`
- `ui/src/hooks/useBrowserTaskEvents.test.tsx`

## Non-Goals

- Do not touch `src-tauri/src/browser/agent_loop.rs` in this phase.
- Do not add or change Tauri IPC commands.
- Do not trigger runtime-pack prepare/repair/reinstall/cleanup/rollback.
- Do not wire prompt dispatch, no-browser fallback execution, or checkpoint
  writes.
- Do not modify root `App`, `AppShell`, `BrowserPanel`, or DMZ files.

## Impact Targets

- `BrowserTaskMonitor` in `ui/src/components/browser/BrowserTaskMonitor.tsx`.
- `BrowserTaskStatus` type aliases in `ui/src/atoms/browser-atoms.ts` and
  `ui/src/lib/tauri-bridge.ts`.

GitNexus impact for `BrowserTaskMonitor` is CRITICAL because it sits under
`BrowserPanel` and affects BrowserViewer, Preview, Tabs, Automation, and
KaleidoscopeShell paths. A fresh reviewer sub-agent must accept the narrow
projection-only change before implementation proceeds.

## Rollback

Revert the Phase 4K PR. No migrations, runtime side effects, persistent config
changes, or backend behavior changes are introduced.

## Verification

- `cd ui && npm test -- --run ui/src/components/browser/BrowserTaskMonitor.test.tsx`
- `cd ui && npm test -- --run ui/src/hooks/useBrowserTaskEvents.test.tsx`
- `cd ui && npm run build`
- Browser runtime default Rust regressions:
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
  - `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
