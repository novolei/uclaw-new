# Phase 4W - Settings Run Doctor Refresh

## Summary

Phase 4W makes the Browser Runtime Settings `run_doctor` control perform a
real read-only status refresh through the existing `getBrowserRuntimeStatus`
bridge. This is not a runtime doctor executor and does not prepare, repair,
reinstall, cleanup, rollback, or mutate runtime files. It only refreshes the
Settings-visible `BrowserRuntimePackStatusReport`.

## ADR Section 18 Questions

1. **User intent:** let users manually refresh Browser Runtime / Startup Doctor
   status from Settings after the initial read.
2. **Autonomy level:** L0/L1 display-only. The user clicks a button, but the app
   only reads local status and updates Settings UI.
3. **Canonical truth source:** `BrowserRuntimePackStatusReport` returned by the
   dedicated read-only `get_browser_runtime_status` command.
4. **TaskEvent entries:** none. This phase does not emit TaskEvents because it
   is a Settings status refresh, not task execution.
5. **Context read/citation:** reads only local browser runtime-pack status
   through the existing Tauri bridge; no external context or user data is cited.
6. **Capability cards:** consumes existing Browser Runtime / Startup Doctor
   status capability; adds no provider card and promotes no provider.
7. **Policy hooks:** status may reflect policy-gated runtime state, but this
   phase adds no policy hook and bypasses none.
8. **World projection:** Browser Runtime Settings renders refreshed status,
   last-check time, version, action previews, rollback availability, and
   auto-prepare state from the refreshed report.
9. **Harness cases:** focused Settings tests cover mount read, run-doctor
   refresh, explicit `status` bypass, rejected refresh safety, and that only the
   read-only bridge is mocked/called.
10. **Rollback/disable path:** revert this PR; Settings returns to preview-only
    `run_doctor` behavior with no persistent side effects.
11. **Does not own:** runtime action execution, installer/downloader adapters,
    repair/cleanup/rollback mutations, backend command changes, shared
    `getSettings`, Startup Splash changes, TaskEvents, provider selection,
    Browser Identity, DB migrations, or DMZ files.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4w-settings-run-doctor-refresh.md`

## Non-Goals

- No backend IPC or `tauri-bridge.ts` changes.
- No runtime pack mutation or real prepare/repair/reinstall/cleanup/rollback.
- No provider promotion, Playwright worker behavior, TaskEvents, DB migrations,
  shared Settings initialization, or DMZ file edits.
- No retry-from-empty semantics in this slice: if the initial status read fails
  and no report exists, `run_doctor` remains disabled because there is no
  displayed runtime report to refresh yet.

## Impact Targets

- `BrowserRuntimeSettings`: GitNexus impact LOW, with direct Settings
  dependents.
- `actionSummary`: GitNexus impact LOW, with only the existing Browser Runtime
  settings model call chain affected.
- `getBrowserRuntimeStatus`: GitNexus impact HIGH after Phase 4V because it is
  shared by Settings and Startup Splash/root App. Fresh reviewer Carver accepted
  the narrow plan on the condition that the bridge implementation remains
  unchanged and refresh stays local to `BrowserRuntimeSettings`.

## Implementation Plan

1. Factor the existing mount read into a local `refreshLiveStatus` callback.
2. Preserve explicit `status` prop bypass: supplied status props do not trigger
   mount refreshes or click refreshes.
3. Add a stale-result guard for overlapping refreshes and unmounts.
4. Have the `run_doctor` button select the action preview and trigger the local
   read-only refresh only when using live status.
5. Tighten `run_doctor` preview copy so it says status refresh, not backend IPC
   still missing.
6. Add focused component tests for refresh success, explicit status bypass, and
   failed refresh safety.
7. Update the tracker with PR state, reviewer acceptance, verification, and
   next action.

## Verification

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` (expected N/A unless
  Rust files change)
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the Phase 4W commit. The only runtime behavior removed is the Settings
button's read-only refresh; no runtime files, provider state, settings, or task
checkpoints are mutated.
