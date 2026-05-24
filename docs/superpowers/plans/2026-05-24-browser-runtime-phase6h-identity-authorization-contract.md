# Browser Runtime Phase 6H - Identity Authorization Contract

## Phase Goal

Backfill the ADR Phase 6 authorization gap with a generic browser identity
authorization completion contract. The existing automation login path already
captures browser/WebView storage state and imports it into the browser identity
broker, but that behavior is trapped in a spec-specific lane inside
`tauri_commands.rs`. This phase exposes the same capability as a reusable
browser identity IPC/bridge contract and keeps `tauri_commands.rs` as a thin
compatibility shim.

Phase 6H is named `6H` because an older unused `phase6g-payment-confirmation`
worktree/branch exists at the Phase 6F base. This PR does not reuse or delete
that worktree.

## ADR 11 Questions

1. User intent:
   Let a user consent to a browser login/authorization flow once, save the
   resulting uClaw-managed identity, and reuse/revoke it through the existing
   browser identity model instead of repeating per-domain prompts.

2. Autonomy level:
   L0/L1 only. The user must initiate or complete the authorization surface.
   Captured identity state may later be consumed by browser tasks under their
   approved autonomy level; payment, posting, and account mutations still need
   policy/user confirmation.

3. Canonical truth source:
   Browser identity metadata plus storage-state secret material remain canonical
   for identity. Task/run/event state remains canonical for automation. WebView
   cookies and provider internals are capture inputs only, not product truth.

4. TaskEvent entries:
   This phase does not add new TaskEvent emission. It returns typed authorization
   completion reports and preserves the existing automation login completion
   event for compatibility. Future Settings UI can emit/cite
   `browser.identity.authorized` once it consumes this contract.

5. Context read and citation:
   Reads a managed browser tab or Tauri WebView cookie/storage snapshot,
   requested label, URL/origin, and scope. It returns profile ids and summary
   metadata only; raw cookies/storage values are never returned to the UI or
   model. Future model-visible use should cite the identity profile id and
   browser artifact/profile boundary event.

6. Capability cards:
   Consumes the local browser provider identity/profile capability from the
   existing BrowserProvider surface. It adds no new provider card and does not
   promote CLI/MCP/hosted providers.

7. Policy hooks:
   Blocks invalid URLs, missing labels, empty storage state, unauthenticated
   cookie snapshots, revoked profiles, and future policy gates around profile
   use, payment, credentials, posting, file upload/download, and hosted data
   egress.

8. World projection:
   No new UI surface in this phase. The bridge exposes enough typed data for a
   later Settings connect/recovery UI to render authorized identity, origin,
   status, profile id, and completion state.

9. Harness cases:
   Focused unit tests cover URL-to-origin derivation, same-site login cookie
   detection, import of captured storage state into identity summaries without
   secret leakage, missing-auth rejection, and existing automation login
   compatibility. Frontend bridge tests cover the generic IPC calls.

10. Rollback or disable path:
   Revert this PR. Existing list/revoke identity IPC and automation login
   behavior remain recoverable because compatibility wrappers preserve their
   public command names. No migration or persistent schema change is introduced.

11. Deliberately not owned:
   No Settings connect UI, no payment confirmation UI, no provider promotion,
   no DB migration, no hosted provider, no external Chrome attach, no raw secret
   display, no global npm/manual Playwright path, and no broad rewrite of
   `agentic_loop.rs` or `tauri_commands.rs`.

## Allowed Files

- `src-tauri/src/browser/identity_authorization.rs`
- `src-tauri/src/browser/identity_ipc.rs`
- `src-tauri/src/browser/mod.rs`
- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/main.rs`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/lib/tauri-bridge.browser-identity.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase6h-identity-authorization-contract.md`

## Non-Goals

- Do not build the Settings connect/recovery UI.
- Do not add TaskEvent emission or new task checkpoint states.
- Do not promote any provider or change default provider policy.
- Do not add migrations, persistent locator/recipe writes, or hosted-provider
  credentials.
- Do not add browser business logic to `tauri_commands.rs`; keep it as a shim
  for existing automation commands.

## Impact Targets

- `BrowserIdentityProfileSummary` in `browser/identity_ipc.rs`.
- Existing automation login completion helpers in `tauri_commands.rs`; GitNexus
  may not resolve these symbols because of the large file, so document UNKNOWN
  as a high-attention note and keep edits narrow.
- `listBrowserIdentities` / identity bridge area in `ui/src/lib/tauri-bridge.ts`.
- `main.rs` Tauri command registration; additive command registration only.

## Rollback

Revert this PR. No migration or runtime pack mutation is involved. Existing
Phase 6 list/revoke identity IPC, Phase 6F resume decisions, Phase 8 provider
routing, and Phase 10 hosted-provider harness coverage remain intact.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_authorization`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::identity_ipc`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-identity.test.ts`
- `rustfmt --edition 2021 --check <changed-rust-files>`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
