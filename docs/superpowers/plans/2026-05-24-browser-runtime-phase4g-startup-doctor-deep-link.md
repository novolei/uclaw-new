# Browser Runtime Phase 4G - Startup Doctor Settings Deep Link

## Scope

Phase 4G adds the first Startup Doctor -> Browser Runtime Settings deep-link
affordance. It is intentionally component-scoped: `StartupSplash` exposes an
optional callback and renders a settings action only when a browser-runtime
doctor check needs attention.

## ADR Section 18 Answers

1. User intent: help users recover browser runtime setup from Startup Doctor
   without understanding runtime-pack internals.
2. Autonomy level: L0/L1 UI navigation only; no browser automation or runtime
   preparation is executed.
3. Canonical truth source: the Startup Doctor view model remains the source for
   whether a runtime check needs attention; Settings remains the destination.
4. TaskEvent entries: none in this slice. TaskEvents remain for later IPC/task
   runtime phases.
5. Context read/citation: reads only the supplied `StartupDoctorViewModel`.
   There are no model-visible observations or artifact citations.
6. Capability cards: none added or consumed.
7. Policy hooks: no policy hook is invoked because this is local UI navigation
   affordance only.
8. World projection: keeps startup/runtime preparation visible by connecting
   the Startup Doctor attention state to the existing Browser Runtime settings
   destination.
9. Harness cases: focused StartupSplash tests cover button visibility and
   callback behavior; existing startup tests cover unchanged first-frame and
   diagnostics behavior.
10. Rollback/disable path: revert this PR to remove the optional callback and
    settings button; no persisted state or backend migration is involved.
11. Deliberately not owned: root `App` wiring, Settings persistence, IPC,
    runtime actions, TaskEvents, task checkpoints, error-surface deep links,
    task-time prompt wiring, provider promotion, and DB migrations.

## Allowed Files

- `ui/src/components/startup/StartupSplash.tsx`
- `ui/src/components/startup/StartupSplash.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4g-startup-doctor-deep-link.md`

## Non-Goals

- No root `App` changes.
- No SettingsPanel/AppShell changes.
- No backend IPC, runtime operation execution, TaskEvent emission, or task
  checkpoint writes.
- No Startup Doctor status source changes.
- No Browser Runtime settings persistence.

## Impact Targets

- `StartupSplash`
- `StartupCheckRow`
- `startupRecoverySurface`

Pre-edit GitNexus impact for all three targets must be LOW or this phase stops.

## Implementation

- Add an optional `onOpenBrowserRuntimeSettings` prop to `StartupSplash`.
- Detect failed or warning browser-runtime doctor checks by check id.
- Render a compact settings action in the recovery surface only when that
  callback exists and a browser-runtime check needs attention.
- Add focused tests proving the callback is called and that the action is hidden
  for non-browser-runtime attention.

## Verification

- `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx`
- `cd ui && npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable
  unless Rust files change.
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes` on staged changes.

## Rollback

Revert the Phase 4G PR. The rollback removes the optional prop, the Startup
Doctor settings action, the focused tests, this plan, and the tracker update.
