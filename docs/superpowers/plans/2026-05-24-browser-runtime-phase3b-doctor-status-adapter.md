# Browser Runtime Phase 3B - Startup Doctor Status Adapter

## Scope

Phase 3B advances Startup Doctor state plumbing without touching the root
`App` loading path that produced HIGH staged GitNexus risk in Phase 3A. This
slice adds a typed frontend adapter from the Phase 2 runtime-pack status report
vocabulary into the Startup Doctor view model:

- TypeScript DTOs mirroring the runtime-pack doctor/status fields the UI needs;
- pure check-merging helpers for manifest, runtime-pack path, network, and
  last-known runtime status;
- a view-model helper that composes runtime-pack status with existing startup
  checks;
- focused Vitest coverage for ready, deferred/offline, repair, and blocked
  states;
- tracker updates that record Phase 3A as merged and Phase 3B as current.

## ADR Section 18 Answers

1. **User intent:** startup should explain browser runtime readiness in human
   terms before browser work begins.
2. **Autonomy level:** L0-L1 only. This slice maps status into UI state; it does
   not prepare, repair, download, or mutate runtime packs.
3. **Canonical truth source:** Phase 2 `BrowserRuntimePackStatusReport`
   vocabulary remains backend truth; Phase 3B adds a frontend read model for it.
4. **TaskEvent entries:** consumes `browser.runtime.*` event names when present
   in the report, but emits no TaskEvents.
5. **Context read/cited:** reads ADR Phase 3, the tracker, and
   `src-tauri/src/browser/runtime_pack.rs` status/doctor DTO names.
6. **Capability cards:** no provider capability changes.
7. **Policy hooks:** network, destructive, developer fallback, identity, and
   task-time browser policy hooks remain backend-owned and untouched.
8. **World projection:** prepares the Startup Doctor projection mapping that can
   later be hydrated from backend status IPC or World Projection facts.
9. **Harness cases:** unit tests cover ready status, deferred/offline status,
   repair/reinstall status, and blocked status. App-route screenshots remain
   Phase 3C/3D work after root integration.
10. **Rollback/disable path:** revert the adapter helpers/tests, this plan, and
    the tracker update.
11. **Does not own:** root `App` startup wiring, backend IPC commands, runtime
    execution, downloads, cleanup, rollback, settings UI, DB migrations,
    provider promotion, final artwork, or screenshots.

## Allowed Files

- `ui/src/lib/startup/startup-doctor.ts`
- `ui/src/lib/startup/startup-doctor.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase3b-doctor-status-adapter.md`

## Non-Goals

- No root `App` loading-state swap.
- No `tauri_commands.rs`, `main.tsx`, Settings UI, DB migration, runtime-pack
  executor, or browser provider changes.
- No real network checks, downloads, archive extraction, deletion, or runtime
  mutation.
- No Playwright CLI/MCP launch, provider promotion, or browser identity UX.

## Impact Targets

- `deriveStartupDoctorViewModel` is not modified.
- GitNexus could not resolve the new Phase 3A TypeScript symbols as indexed
  symbols, so this slice relies on final staged detect for graph impact.
- New runtime-pack status adapter symbols are additive.

## First Tests

- `cd ui && npm test -- --run src/lib/startup/startup-doctor.test.ts`
- `git diff --check -- <changed-files>`

The broader phase closeout should also run the browser-runtime Rust regressions:

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`

## Rollback

Revert the startup-doctor adapter/test additions, this plan, and the tracker
update. No persisted user data, runtime pack files, DB rows, or browser profiles
are created by this slice.

## Expected Verification Output

- Startup Doctor adapter Vitest: all focused tests pass.
- Rust browser runtime regressions: existing tests pass with no Rust source
  changes.
- `rustfmt` is not required because no Rust files change.
- `git diff --check` returns no output.
- GitNexus detect-changes reports low risk and no unexpected execution flows.
