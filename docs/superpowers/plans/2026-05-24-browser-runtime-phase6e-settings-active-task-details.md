# Phase 6E - Settings Active-Task Details

## Goal

Render the live browser-identity active task summaries introduced by Phase 6D in
Browser Runtime Settings, so Settings does not collapse real revocation/drain
state into only a count.

## ADR Section 18 Questions

1. User intent: understand which browser tasks are currently using an authorized
   uClaw-managed identity before deciding whether to revoke it.
2. Autonomy level: UI-only observation, L0/L1. It performs no browser actions.
3. Canonical truth source: `list_browser_identities` and the
   `BrowserIdentityActiveTaskSummary` values from the Rust identity task
   registry.
4. TaskEvents: none in this slice. Phase 6D owns checkpoint state; later slices
   may add user-boundary TaskEvents.
5. Context read/citation: Settings reads the identity status bridge response.
   No model-visible browser observation is created.
6. Capability cards: consumes the existing browser identity/support contract;
   adds no provider card.
7. Policy hooks: no new policy hook. Existing revoke policy and task checkpoint
   behavior remain owned by backend phases.
8. World projection: Settings renders active identity task run id, session id,
   status, drain deadline, and task summary.
9. Harness cases: focused Vitest coverage for active-task display and revoked
   drain metadata; existing Rust browser identity/runtime tests remain
   regressions.
10. Rollback/disable: revert this PR; Settings returns to active-task count-only
    display while backend registry/IPC remains intact.
11. Non-ownership: no authorization WebView, profile import, reauthorization
    flow, isolated-profile fallback, provider promotion, DB migration,
    TaskEvent emission, or raw auth material display.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6e-settings-active-task-details.md`
- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`

## Impact Targets

- `BrowserRuntimeSettings` in
  `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `identityActiveTaskLabel` in
  `ui/src/components/settings/BrowserRuntimeSettings.tsx`

Pre-edit GitNexus impact for both targets is LOW.

## Implementation Plan

1. Update the tracker with Phase 6D merge status and Phase 6E scope.
2. Add compact active-task rows/cards under the browser identity section.
3. Show task, status, run/session ids, and drain deadline when present.
4. Add focused Settings tests for active tasks and draining revoked tasks.
5. Run focused UI tests, default browser-runtime regressions, rustfmt where
   applicable, `git diff --check`, and GitNexus detect.

## Rollback

Revert this PR. The backend `activeTasks` contract from Phase 6D remains
available, but Settings stops rendering per-task details.

## Verification

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check`
- GitNexus `detect_changes`
