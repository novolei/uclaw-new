# Phase 6C - Settings Browser Identity Status

## Goal

Render the Phase 6B browser identity IPC contract in Browser Runtime Settings:
load safe identity summaries, show authorization/revocation status, expose
unknown active-task state honestly, and provide a one-click revoke control for
already-authorized identities.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users can inspect and revoke a global uClaw-managed browser identity from
     Settings without seeing raw profile or storage-state internals.
2. What autonomy level can it run at?
   - L1-L3 only. This phase lets the user revoke identity consent; it adds no
     new autonomous browser authority.
3. What is the canonical truth source?
   - Backend browser identity metadata exposed through Phase 6B IPC is the
     status source. Task/run/event truth for active task drain remains future
     scope.
4. What TaskEvent entries does it emit?
   - None in this PR. Future task-drain wiring must emit
     `browser.identity.revoked` and checkpoint affected tasks.
5. What context does it read, and how is it cited?
   - It reads only the safe IPC summary fields: profile id, label,
     origin-pattern, provider/scope/status timestamps, revoked flag, and counts.
     It never reads or renders secret handles or raw auth payloads.
6. What capability cards does it add or consume?
   - It consumes the existing browser identity bridge contract and adds no
     provider capability card.
7. What policy hooks can block it?
   - Revocation is user-triggered. New-action blocking remains enforced by the
     Phase 6A backend profile resolution/load checks. Active-task drain policy
     remains later scope.
8. What world projection does the UI render?
   - Browser Runtime Settings renders identity connection/revocation status,
     last-used time when known, and an explicit unknown active-task state until
     the task-drain phase provides real tracking.
9. What harness cases prove it works?
   - Focused Settings tests cover initial identity status load, revoked profile
     rendering, and one-click revoke calling the dedicated bridge and refreshing
     status.
10. What is the rollback or disable path?
    - Revert this PR. Phase 6B IPC and backend Phase 6A revocation behavior
      remain available.
11. What does it deliberately not own?
    - Connect/import flow, authorization WebView, task drain, paused
      checkpointing, TaskEvent writes, payment confirmation, external Chrome
      attach, provider promotion, DB migration, or Space/Workspace identity
      scoping.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6c-settings-identity-status.md`

## Non-Goals

- No backend changes.
- No Settings connect/import flow.
- No authorization WebView.
- No task drain/checkpoint/TaskEvent writes.
- No payment confirmation wiring.
- No provider selection or Playwright promotion.

## Impact Targets

- `BrowserRuntimeSettings`: GitNexus LOW, direct caller `SettingsContent`, 2
  affected settings processes.

## Implementation Plan

1. Load `listBrowserIdentities` from `BrowserRuntimeSettings` alongside runtime
   status, with generation guards matching the existing status refresh pattern.
2. Render a Browser Identity Settings section with status/counts/profile rows
   and active-task state shown as unknown when the backend returns `null`.
3. Add a one-click revoke button for non-revoked identities that calls
   `revokeBrowserIdentity` and refreshes the identity list.
4. Extend focused Settings tests and update the tracker.

## Rollback

Revert this PR. Identity IPC remains callable by later UI work, and no stored
identity metadata shape changes.

## Verification

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect_changes`
