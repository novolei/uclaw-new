# Browser Runtime Settings Real Evidence

Date: 2026-05-25
Branch: `codex/browser-runtime-settings-real-evidence`

## Goal

Make the Settings browser runtime surface and browser preview path reflect real Rust Browser Runtime Supervisor state:

- Split Supervisor/provider readiness from the Playwright runtime pack status so a missing pack does not look like the whole browser runtime is static or fake.
- Make Settings actions clearly run backend dry-run previews, not implied managed side effects.
- Prevent the pseudo tab id `new` from leaking from navigation loading events into screencast/reload/screenshot paths.
- Add a dev-only debug bridge for runtime inspection instead of enabling global Tauri APIs.

## Impact

GitNexus impact was run before edits:

- `deriveBrowserRuntimeSettingsViewModel`: LOW, one direct test caller.
- `BrowserRuntimeSettings`: LOW, one direct test file.
- `BrowserContext.navigate`: LOW after disambiguating to `src-tauri/src/browser/context.rs`.
- `useBrowserScreencast`: CRITICAL because it is shared by Agent/Preview/Kaleidoscope browser UI flows. Scope is intentionally limited to rejecting non-real tab ids before starting or stopping screencast.

The GitNexus index reported it was built from the primary checkout and may be stale for this sibling worktree; this plan uses the reported blast radius as a conservative gate and verifies with focused tests. `npx gitnexus detect-changes --repo uclaw-new` also reproduced the known repo-targeting issue by reporting primary-checkout `AGENTS.md` / `CLAUDE.md` symbols instead of this worktree's diff, so it is recorded as noisy rather than used as release evidence.

## Implementation

1. Settings evidence
   - Add Supervisor/provider view-model fields derived from `supervisor` and `providerReadiness`.
   - Render a separate `运行时 Supervisor` section before the Playwright pack section.
   - Do not auto-select an action preview on load.
   - Label dry-run buttons as previews and display backend dry-run artifacts/events only after a click.
   - Show pending/error states for dry-run requests.

2. Pseudo tab id boundary
   - Backend loading nav-state for `browser_navigate(tab_id="new")` must never emit `tabId: "new"`.
   - Frontend nav-state and screencast consumers must ignore `new`/empty tab ids and keep the last real tab id if present.
   - Browser address controls must only reload/back/forward real tab ids while still allowing navigation to open a fresh tab.

3. Dev debugging
   - Install `window.__UCLAW_DEBUG__` only in Vite dev mode.
   - Expose `getBrowserRuntimeStatus()` and `dryRunBrowserRuntimeAction(action)` through the existing typed bridge.
   - Keep `window.__TAURI__` disabled because `withGlobalTauri` is intentionally off.

## ADR 18 Answers

1. User promise: Settings and browser preview show real Browser Runtime Supervisor/runtime-pack state.
2. Runtime owner: Rust `BrowserRuntimeStatusService` remains the source of truth.
3. State model: Supervisor/provider readiness is displayed separately from runtime-pack readiness.
4. Storage: No new storage.
5. IPC: No managed mutation IPC added; Settings uses existing read-only status and dry-run IPC.
6. Failure mode: Dry-run errors are visible in Settings and do not clear the last runtime status.
7. Security: No global Tauri API exposure; debug bridge is dev-only.
8. Migration: No schema migration.
9. Tests: Focused frontend tests for Settings and screencast guard; Rust unit test for loading tab-id helper.
10. Rollout: Narrow UI/runtime-boundary PR.
11. Exit criteria: Focused tests pass, build passes where feasible, and GitNexus detect-changes is run before commit.

## Verification

- `npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx src/hooks/useBrowserScreencast.test.tsx src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/browser/BrowserPanel.test.tsx` passed.
- `npm run build` passed with existing chunk-size/dynamic-import warnings.
- `cargo test --manifest-path src-tauri/Cargo.toml --lib loading_nav_state_never_emits_new_as_tab_id` passed after linking local ignored runtime assets into the worktree.
- `git diff --check` passed.
- `npx gitnexus detect-changes --repo uclaw-new` ran, but reported the stale primary-checkout `AGENTS.md` / `CLAUDE.md` noise noted above.
