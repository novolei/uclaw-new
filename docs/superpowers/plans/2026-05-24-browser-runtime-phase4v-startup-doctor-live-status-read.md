# Phase 4V - Startup Doctor Live Status Read

## Summary

Phase 4V wires the existing Startup Splash / Startup Doctor surface to the
dedicated read-only Browser Runtime status bridge added in Phase 4S. This is a
read-only consumption slice: Startup Doctor may render runtime-pack readiness,
missing-pack, repair-needed, offline, or degraded status from the backend, but
it must not execute prepare, repair, reinstall, cleanup, rollback, provider
promotion, or task checkpoint side effects.

## ADR Section 18 Questions

1. **User intent:** help users understand at launch whether Browser automation
   is ready, deferred, degraded, or needs attention.
2. **Autonomy level:** L0/L1 display-only. It reads local status and renders UI;
   it performs no browser action or runtime mutation.
3. **Canonical truth source:** `BrowserRuntimePackStatusReport` from the
   dedicated read-only `get_browser_runtime_status` command, mapped through
   `deriveStartupDoctorViewModelFromRuntimePackStatus`.
4. **TaskEvent entries:** none in this phase. TaskEvent emission remains future
   work because this slice is UI read-only.
5. **Context read/citation:** reads only local runtime-pack status through the
   Tauri bridge and existing Startup Doctor check definitions; no external
   context or user data is cited.
6. **Capability cards:** consumes the existing browser runtime status and
   Startup Doctor capability surfaces; adds no provider card and promotes no
   provider.
7. **Policy hooks:** status reads may reflect policy-gated runtime state, but
   this phase introduces no new policy hook and cannot bypass existing gates.
8. **World projection:** Startup Splash renders runtime status as doctor checks,
   progress, recovery guidance, and the existing Browser Runtime Settings deep
   link when runtime checks need attention.
9. **Harness cases:** component tests prove live status reads update Startup
   Doctor checks, explicit preview view models bypass live reads, and read
   failures leave the default startup model intact. Default browser-runtime Rust
   regressions remain in the PR verification loop.
10. **Rollback/disable path:** revert this plan, the Startup Splash live-read
    code/tests, and the tracker update. Startup Splash falls back to the static
    default doctor model.
11. **Does not own:** action execution, runtime install/repair/cleanup/rollback,
    Settings action IPC, task-time prompt dispatch, TaskEvents, provider
    selection, Browser Identity, DB migrations, or DMZ files.

## Allowed Files

- `ui/src/components/startup/StartupSplash.tsx`
- `ui/src/components/startup/StartupSplash.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4v-startup-doctor-live-status-read.md`

## Non-Goals

- No backend command changes.
- No `tauri_commands.rs`, `agentic_loop.rs`, DB migration, workspace
  `Cargo.toml`, `BEHAVIOR.md`, `CLAUDE.md`, or provider promotion edits.
- No runtime prepare/repair/reinstall/cleanup/rollback execution.
- No Settings action wiring, task checkpoint mutation, or TaskEvent emission.
- No Playwright CLI/MCP worker behavior.

## Impact Targets

- `StartupSplash`: GitNexus impact must be checked before edits because it is
  used by root `App` and the Startup Splash preview.
- `deriveStartupDoctorViewModelFromRuntimePackStatus`: checked as the canonical
  mapping boundary for runtime-pack reports into Startup Doctor checks.
- `getBrowserRuntimeStatus`: checked as the read-only bridge consumed by this
  phase.

## Implementation Plan

1. Import `getBrowserRuntimeStatus` and
   `deriveStartupDoctorViewModelFromRuntimePackStatus` into `StartupSplash`.
2. Preserve explicit `viewModel` behavior for previews/tests: when a caller
   supplies `viewModel`, do not issue the live read.
3. When `viewModel` is absent, issue one read-only status request on mount and
   map the report through the existing Startup Doctor runtime-pack adapter.
4. If the read fails or resolves after unmount, leave the existing default
   startup model unchanged.
5. Update tests to mock the bridge, prove the live read path, and prove the
   explicit preview bypass.
6. Update the tracker to mark Phase 4U merged and Phase 4V as the active PR.

## Verification

- `cd ui && npm test -- --run src/components/startup/StartupSplash.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` (expected N/A unless
  Rust files change)
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the Phase 4V commit. Since all behavior is frontend read-only and
guarded behind the absence of an explicit `viewModel` prop, rollback restores
the static Startup Doctor default without runtime pack mutation or data loss.
