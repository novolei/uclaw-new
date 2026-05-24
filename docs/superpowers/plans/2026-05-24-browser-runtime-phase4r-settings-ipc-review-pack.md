# Phase 4R - Settings IPC Review Pack

## Summary

Phase 4Q finished the pure frontend dispatch-effect model for task-time
prepare/defer/no-browser choices. The remaining Phase 4 UX gap is real
Settings and Startup Doctor data/control wiring: Browser Runtime settings need
to read runtime-pack status from Rust and eventually execute prepare, repair,
reinstall, cleanup, rollback, and run-doctor actions through policy-gated
runtime-pack boundaries.

Phase 4R is deliberately docs-only. It records the writer/reviewer plan before
any IPC implementation because the backend command surface lives in the DMZ
`tauri_commands.rs`, and the shared frontend bridge `getSettings` has HIGH
GitNexus impact.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need Browser Runtime settings to show real runtime-pack state and
   eventually run repair/preparation controls without understanding CLI tooling.
2. What autonomy level can it run at?
   Docs/reviewer planning only. Future status reads are L1 local inspection;
   future prepare/repair/cleanup/rollback actions require explicit policy gates
   and confirmation where Phase 2 plans require it.
3. What is the canonical truth source?
   The canonical truth remains Rust-owned runtime-pack status reports,
   operation plans, execution reports, TaskEvents, and browser task checkpoints.
   Settings is a projection/control surface, not provider truth.
4. What TaskEvent entries does it emit?
   None in this docs slice. Future IPC wiring may surface existing runtime
   doctor/action event names, but must not invent ad hoc event names outside
   the browser runtime contracts.
5. What context does it read, and how is it cited?
   This slice reads the Browser Runtime ADR, tracker, current Settings and
   startup-doctor frontend models, `tauri-bridge` IPC patterns, and GitNexus
   impact output. Future UI-visible status must cite runtime-pack status report
   fields and artifact ids when execution reports are produced.
6. What capability cards does it add or consume?
   None in this slice. Future implementation consumes the existing runtime-pack
   and provider capability surfaces; it must not promote Playwright providers.
7. What policy hooks can block it?
   DMZ writer/reviewer review, runtime-pack policy gates, metered/restricted
   network confirmation, destructive cleanup/rollback confirmation, active-task
   protection, and developer-fallback policy can block future implementation.
8. What world projection does the UI render?
   No UI change in this slice. Future implementation should keep Browser
   Runtime settings, Startup Doctor, task-time prompts, and paused-waiting task
   projection consistent from one status/report source.
9. What harness cases prove it works?
   This docs slice is proved by GitNexus detect and diff checks. Future writer
   slices need focused tauri bridge/settings/startup tests plus the default
   browser runtime Rust regressions.
10. What is the rollback or disable path?
   Revert this docs PR. Future IPC slices must remain single-PR reversible and
   keep runtime mutations behind existing feature/policy gates.
11. What does it deliberately not own?
   It does not add Tauri commands, edit `tauri_commands.rs`, execute runtime
   pack actions, persist settings, launch Playwright, change provider
   selection, edit root `App`, change DB migrations, or wire task-loop side
   effects.

## Allowed Files

- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4r-settings-ipc-review-pack.md`

## Non-Goals

- Do not edit `tauri_commands.rs`, `agentic_loop.rs`, root `App`, DB
  migrations, `BEHAVIOR.md`, `CLAUDE.md`, or workspace `Cargo.toml`.
- Do not add frontend `invoke(...)` calls, Tauri command names, runtime-pack
  execution, settings persistence, or provider promotion in this phase.
- Do not execute downloads, filesystem cleanup, rollback, Playwright launch, or
  no-browser fallback behavior.

## Impact Targets

- `ui/src/lib/tauri-bridge.ts::getSettings`: GitNexus impact HIGH, 6 direct
  callers, 2 affected processes (`App`, `GeneralSettings`), and modules Atoms,
  Settings, and Hooks. Future writer work should avoid changing this shared
  settings initializer unless a fresh reviewer accepts the blast radius.
- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW, 1 direct caller, 2 affected processes
  (`SettingsPanel`, `SettingsContent`).
- `ui/src/components/settings/SettingsPanel.tsx::SettingsPanel`: GitNexus
  impact LOW, 0 direct callers, 0 affected processes.
- `src-tauri/src/tauri_commands.rs::get_settings` was not resolved by the
  current GitNexus index, but `tauri_commands.rs` is a repo DMZ file. Any
  backend IPC command addition or command-list registration must therefore use
  the writer/reviewer plan below before implementation.

## Writer Plan For Next Implementation

The next implementation PR may proceed only under one of these scopes:

- Preferred read-only slice: add a narrowly named Browser Runtime status IPC
  command in `tauri_commands.rs` that returns the existing Phase 2 runtime-pack
  status report without executing plans, mutating files, changing settings, or
  launching providers. Add a frontend bridge function that calls only this new
  command and keep `getSettings` unchanged.
- Alternate frontend-only slice: add a small `BrowserRuntimeSettings` adapter
  prop/loader boundary that can later consume a status report, with no Tauri
  command and no `getSettings` changes.

Any writer slice that edits `getSettings`, root `App`, existing startup
initialization, runtime-pack executor side effects, or broad Tauri command
registration must spawn a fresh reviewer sub-agent before implementation or
merge.

## Reviewer Checklist

- Confirm the writer does not change shared app startup/settings initialization
  unless explicitly accepted.
- Confirm the backend command is read-only unless the phase is explicitly about
  an execution action and includes policy/confirmation gates.
- Confirm runtime-pack actions still use Rust-owned planner/executor contracts.
- Confirm no production path requires global npm or user-installed Playwright.
- Confirm rollback is a single PR revert with no persistent user-data changes.

## Rollback

Revert this docs-only PR. It changes no runtime state, user settings, browser
profiles, DB schema, or provider selection.

## Verification

- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes`
- Default browser-runtime Rust regressions remain recommended for this program;
  this docs-only phase does not change Rust or UI code.
