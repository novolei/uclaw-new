# Phase 4X - Settings Action Dry-Run IPC

## Summary

Phase 4X gives Browser Runtime Settings a backend-backed dry-run lane for
prepare, repair, reinstall, cleanup, rollback, and keep-current action
controls. `retry_when_online` stays a local preview until it has a distinct
retry/deferred report contract instead of being represented as a prepare
operation. The backend returns a `BrowserRuntimePackExecutionReport` in
dry-run mode only. This is not a real installer, repairer, cleanup executor,
rollback executor, downloader, archive extractor, or Playwright launcher.

## ADR Section 18 Questions

1. **User intent:** let users inspect what a Browser Runtime action would do
   before real execution exists.
2. **Autonomy level:** L0/L1. User clicks a control; the app returns a dry-run
   plan and performs no runtime mutation.
3. **Canonical truth source:** Rust `runtime_pack` planner and dry-run executor
   produce the canonical `BrowserRuntimePackExecutionReport`.
4. **TaskEvent entries:** none. This phase returns event names in the report but
   emits no TaskEvents.
5. **Context read/citation:** reads only local runtime-pack manifest/probe
   status via the same runtime-pack planning path; no external context is cited.
6. **Capability cards:** consumes existing Browser Runtime / Startup Doctor
   runtime-pack planning capability; adds no provider card and promotes no
   provider.
7. **Policy hooks:** dry-run honors existing planner policy fields such as
   network, confirmation, active-task, and destructive flags; it adds no bypass
   and performs no side effect.
8. **World projection:** Settings renders dry-run summary, event names, and
   action metadata as visible action evidence.
9. **Harness cases:** Rust tests cover missing-pack dry-run serialization and
   no file creation; UI bridge tests cover invoke payload; Settings tests cover
   action click dry-run and explicit preview bypass.
10. **Rollback/disable path:** revert this PR. Settings returns to local action
    previews only.
11. **Does not own:** real prepare/repair/reinstall/cleanup/rollback execution,
    runtime download/extract/delete/promote, provider selection, Browser
    Identity, TaskEvents, task resume UX, shared `getSettings`, DB migrations,
    `tauri_commands.rs`, `agentic_loop.rs`, or other task-loop DMZ files.

## Allowed Files

- `src-tauri/src/browser/runtime_pack_ipc.rs`
- `src-tauri/src/main.rs`
- `ui/src/lib/startup/startup-doctor.ts`
- `ui/src/lib/tauri-bridge.ts`
- `ui/src/lib/tauri-bridge.browser-runtime.test.ts`
- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4x-settings-action-dry-run-ipc.md`

## Non-Goals

- No managed executor runner, downloader, archive extraction, delete, promote,
  rollback restore, or Playwright process launch.
- No provider promotion, TaskEvents, DB migrations, shared Settings
  initialization, `tauri_commands.rs`, `agentic_loop.rs`, or task-loop DMZ
  edits.
- No `getBrowserRuntimeStatus` behavior changes.
- No task-time prompt dispatch or no-browser fallback execution.

## Impact Targets

- `src-tauri/src/browser/runtime_pack_ipc.rs`: GitNexus file impact LOW.
- `src-tauri/src/main.rs::main`: GitNexus impact LOW for command registration.
  This is a DMZ command-registration touch. It is intentionally limited to
  adding one handler entry, covered by `cargo check --bin uclaw`, and called
  out for reviewer verification.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW.
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact HIGH
  after Phase 4V because it is shared by Settings and Startup Splash/root App;
  this phase does not edit that symbol.

## Implementation Plan

1. Add a Tauri command that accepts a `BrowserRuntimePackAction` and returns a
   dry-run `BrowserRuntimePackExecutionReport` from the existing Rust planner.
2. Register the command in `src-tauri/src/main.rs`.
3. Add frontend DTO types for execution reports and a dedicated bridge method.
4. In Settings, call the dry-run bridge for runtime-pack action buttons while
   keeping auto-prepare and run-doctor behaviors separate.
5. Keep `retry_when_online` as a local preview because the backend currently
   maps it to the prepare operation and forces online planning for this dry-run
   command.
6. Preserve explicit `status` preview paths: supplied status props do not call
   the dry-run bridge.
7. Render dry-run summary/event evidence in the existing action preview area.
8. Update tracker and tests.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack_ipc`
- `cd ui && npm test -- --run src/lib/tauri-bridge.browser-runtime.test.ts`
- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `cargo check --manifest-path src-tauri/Cargo.toml --bin uclaw`
- `rustfmt --edition 2021 --check src-tauri/src/browser/runtime_pack_ipc.rs`
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes` before commit.

## Rollback

Revert the Phase 4X commit. The dry-run command and Settings dry-run rendering
disappear; no runtime files, settings, task checkpoints, provider state, or user
data are changed.
