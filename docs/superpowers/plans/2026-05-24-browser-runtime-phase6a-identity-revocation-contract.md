# Phase 6A - Browser Identity Revocation Contract

## Goal

Start ADR Phase 6 with a small backend contract: authorized browser identities
can become visibly revoked, revoked identities cannot be resolved for new
actions, and their storage-state secret is removed without deleting metadata.

## ADR Section 18 Questions

1. What user intent does this support?
   - Users can later connect a browser identity and revoke it with clear,
     auditable state instead of hidden profile attachment.
2. What autonomy level can it run at?
   - L1-L3 only. This phase adds revocation safety, not new autonomous browser
     authority.
3. What is the canonical truth source?
   - The browser identity metadata index is the local canonical source for
     profile visibility/status; storage-state secrets remain behind
     `BrowserSecretStore`.
4. What TaskEvent entries does it emit?
   - None in this PR. Future task-drain/revoke wiring must emit the
     user-boundary event.
5. What context does it read, and how is it cited?
   - It reads local identity metadata and secret handles. Raw storage-state
     payloads stay in the secret store and are not exposed in metadata.
6. What capability cards does it add or consume?
   - It consumes the existing browser identity/auth-profile primitive and adds
     no new provider card.
7. What policy hooks can block it?
   - Revoked profiles are blocked at resolve/load time. Future UI/task phases
     still own confirmation prompts and bounded drain policy.
8. What world projection does the UI render?
   - No UI change in this PR. It provides visible revoked metadata for later
     Settings status rows.
9. What harness cases prove it works?
   - Focused identity tests cover revoke preserving metadata, deleting the
     secret, blocking load, and excluding revoked profiles from origin
     resolution.
10. What is the rollback or disable path?
   - Revert this PR. Existing import/list/resolve/delete behavior remains the
     prior fallback.
11. What does it deliberately not own?
   - Settings connect UI, in-app authorization window, Tauri IPC, task drain,
     paused checkpointing, payment confirmation, external Chrome attach, and
     provider promotion.

## Allowed Files

- `src-tauri/src/browser/identity/types.rs`
- `src-tauri/src/browser/identity/profile_store.rs`
- `src-tauri/src/browser/identity/broker.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6a-identity-revocation-contract.md`

## Non-Goals

- No UI, IPC, `tauri_commands.rs`, `agentic_loop.rs`, task drain, task status
  mutation, DB migration, external Chrome real-profile attach, or payment
  confirmation behavior.

## Impact Targets

- `BrowserIdentityStatus`: GitNexus LOW, 0 affected processes.
- `BrowserIdentityProfile`: GitNexus LOW, 0 affected processes.
- `BrowserIdentityProfileStore::import_storage_state`: GitNexus LOW, 2 direct
  test callers, 0 affected processes.
- `BrowserIdentityProfileStore::resolve_for_origin`: GitNexus LOW, 1 direct
  test caller, 0 affected processes.
- `BrowserIdentityProfileStore::load_storage_state`: GitNexus LOW, 0 affected
  processes.

## Implementation Plan

1. Add revoked metadata fields/status to identity profiles in a
   serde-backward-compatible way.
2. Add `revoke_profile` to the profile store and broker.
3. Make revoked profiles visible in `list_profiles` but unavailable for origin
   resolution and storage-state loading.
4. Add focused tests and update the tracker.

## Rollback

Revert this PR. Any future metadata with optional revoked fields remains safely
ignored by older code because those fields are additive.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check src-tauri/src/browser/identity/types.rs src-tauri/src/browser/identity/profile_store.rs src-tauri/src/browser/identity/broker.rs`
- `git diff --check -- src-tauri/src/browser/identity/types.rs src-tauri/src/browser/identity/profile_store.rs src-tauri/src/browser/identity/broker.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase6a-identity-revocation-contract.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-phase6a-identity-revocation-contract`
