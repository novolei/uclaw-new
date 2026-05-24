# Phase 5B-Preflight B - Settings Live Runtime Path Mapping

## Summary

Expose the already-serialized Browser runtime path fields in the frontend live
status contract and Browser Runtime Settings surface. This closes the Phase 4/5
visibility drift found by the dry-run audit before Playwright CLI child-worker
execution begins.

## ADR 18 Questions

1. **User intent:** users need to verify which uClaw-managed runtime root and
   current pack the app is reading before trusting prepare/repair/provider
   behavior.
2. **Autonomy level:** L0/L1 only. This is read-only display and type mapping.
3. **Canonical truth:** Rust `BrowserRuntimePackStatusReport` returned by
   `get_browser_runtime_status`; Settings renders a projection of that report.
4. **TaskEvents:** none. This phase only displays existing status fields.
5. **Context read/cited:** the frontend reads the dedicated read-only Tauri
   bridge result and cites it visually as Settings rows.
6. **Capability cards:** consumes existing Browser Runtime / Playwright CLI
   setup readiness context; adds no provider capability.
7. **Policy hooks:** no new policy hooks. Existing runtime action controls stay
   dry-run or read-only.
8. **World projection:** Browser Runtime Settings renders runtime root/current
   pack path from live status instead of placeholder/manual preview-only data.
9. **Harness cases:** focused Settings, browser-runtime view-model, Tauri bridge,
   and Startup Doctor adapter tests prove path fields are accepted and displayed.
10. **Rollback/disable:** revert this PR; it only changes frontend type/view
    mapping, tests, and tracker/plan docs.
11. **Non-ownership:** no runtime pack execution, no Playwright child worker, no
    provider promotion, no Settings persistence, no backend IPC changes, no
    `agentic_loop.rs`, and no `tauri_commands.rs`.

## Allowed Files

- `ui/src/lib/startup/startup-doctor.ts`
- `ui/src/lib/startup/startup-doctor.test.ts`
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- `ui/src/lib/browser-runtime/browser-runtime-settings.test.ts`
- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `ui/src/lib/tauri-bridge.browser-runtime.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase5b-preflight-settings-paths.md`

## Non-Goals

- Do not edit backend IPC or Tauri command registration.
- Do not execute prepare/repair/reinstall/cleanup/rollback for real.
- Do not spawn Node, Playwright, or a browser worker.
- Do not promote `browser.playwright_cli` or change provider selection.
- Do not change persisted Settings schema.

## Impact Targets

- `StartupRuntimePackStatusReport` impact is GitNexus CRITICAL because the
  shared TypeScript interface has broad import fan-out. The actual change is an
  additive optional/required field alignment with the Rust JSON that already
  exists. Focused tests must cover bridge, Startup Doctor adapter, and Settings.
- `deriveBrowserRuntimeSettingsViewModel` impact is HIGH because it feeds
  Browser Runtime Settings and SettingsPanel flows. Keep behavior additive:
  live `report.currentPackDir` should win, then legacy `runtimePackPath`, then
  placeholder.
- `BrowserRuntimeSettings` impact is LOW. Only render the view-model path labels.

## Implementation Steps

1. Add `runtimeRoot` and `currentPackDir` to the frontend runtime-pack status
   report type and fixtures.
2. Teach the Settings view-model to derive display rows from live report paths.
3. Render runtime root/current pack path in Settings without adding new action
   behavior.
4. Update focused tests for live bridge data and explicit preview compatibility.
5. Update the tracker with PR #454 merge state, Phase 5B-preflight B progress,
   impact notes, verification, and next action.

## Verification

- `cd ui && npm test -- --run ui/src/lib/browser-runtime/browser-runtime-settings.test.ts ui/src/components/settings/BrowserRuntimeSettings.test.tsx ui/src/lib/tauri-bridge.browser-runtime.test.ts ui/src/lib/startup/startup-doctor.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect-changes`

## Rollback

Revert this PR. The runtime backend contract remains unchanged because the Rust
status report already serialized the path fields before this phase.
