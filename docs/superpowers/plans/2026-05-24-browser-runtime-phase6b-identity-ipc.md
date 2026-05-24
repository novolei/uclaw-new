# Phase 6B - Browser Identity IPC Contract

## Goal

Expose the Phase 6A browser identity revocation contract through a narrow IPC
surface that Settings can consume later: list identity status as safe metadata
and revoke one identity without exposing storage-state secret handles.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users will be able to see and revoke a consented browser identity from
     Settings instead of relying on hidden profile state.
2. What autonomy level can it run at?
   - L1-L3 only. This phase exposes visibility and revocation controls, not
     new autonomous browser authority.
3. What is the canonical truth source?
   - The browser identity metadata index remains the local truth for visible
     profile status. Raw storage-state secrets remain behind
     `BrowserSecretStore`.
4. What TaskEvent entries does it emit?
   - None in this PR. It returns command reports only. Future task-drain
     wiring must emit the `browser.identity.revoked` user-boundary event.
5. What context does it read, and how is it cited?
   - It reads local browser identity metadata. IPC responses cite no model
     context and deliberately omit secret handles and raw auth payloads.
6. What capability cards does it add or consume?
   - It consumes the existing browser identity/auth-profile primitive and adds
     no provider card.
7. What policy hooks can block it?
   - Revoked identities remain blocked by the Phase 6A resolve/load checks.
     Future Settings connect and active-task drain slices own confirmation and
     policy prompts.
8. What world projection does the UI render?
   - No Settings UI is rendered in this PR. The bridge contract carries status,
     last-used time, revoked time, and an explicit unknown active-task count for
     later Settings rows.
9. What harness cases prove it works?
   - Focused Rust IPC tests cover safe summary serialization, count derivation,
     and revoke reports. Focused Vitest bridge tests cover command names and
     camelCase payloads.
10. What is the rollback or disable path?
    - Revert this PR. Phase 6A backend identity metadata and revocation
      behavior remain available to internal callers.
11. What does it deliberately not own?
    - Settings UI, connect flow, in-app authorization WebView, task drain,
      paused checkpointing, payment confirmation, external Chrome attach,
      provider promotion, and raw storage-state import UI.

## Allowed Files

- `src-tauri/src/browser/identity_ipc.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/main.rs`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/lib/tauri-bridge.browser-identity.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6b-identity-ipc.md`

## Non-Goals

- No Settings component changes.
- No authorization WebView or connect flow.
- No task drain, task status mutation, checkpoint writes, or TaskEvent writes.
- No DB migration.
- No provider selection or Playwright provider promotion.
- No raw storage-state, cookie, bearer token, or secret-handle exposure over
  IPC.

## Impact Targets

- `src-tauri/src/main.rs::main`: GitNexus LOW, 0 direct callers, 0 affected
  processes before adding command registrations.
- New IPC helper symbols are additive and covered by focused tests.

## Implementation Plan

1. Add safe identity summary/report DTOs and pure broker-backed list/revoke
   helpers.
2. Add Tauri commands for listing and revoking browser identities.
3. Register the commands in the main invoke handler and export the module.
4. Add frontend bridge types/calls and focused bridge tests.
5. Update the tracker with Phase 6A merge facts and Phase 6B progress.

## Rollback

Revert this PR. The identity metadata index remains compatible because this PR
adds no stored fields or migrations.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
- `rustfmt --edition 2021 --check src-tauri/src/browser/identity_ipc.rs src-tauri/src/browser/mod.rs src-tauri/src/main.rs`
- `git diff --check -- <changed-files>`
- GitNexus staged `detect_changes`
