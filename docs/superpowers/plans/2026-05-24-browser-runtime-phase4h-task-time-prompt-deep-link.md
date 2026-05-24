# Browser Runtime Phase 4H - Task-Time Prompt Settings Deep Link

## Scope

Phase 4H adds a component-scoped Browser Runtime Settings deep-link affordance
to the task-time runtime prompt. It does not wire the prompt into agent task
runtime yet and does not execute runtime actions.

## ADR Section 18 Answers

1. User intent: help users inspect runtime state while deciding whether to
   prepare now, defer, or continue without browser support.
2. Autonomy level: L0/L1 UI navigation only.
3. Canonical truth source: the prompt view model remains the source for
   task-time choices; Browser Runtime Settings remains the destination.
4. TaskEvent entries: none are emitted in this slice; prompt event names remain
   preview-only.
5. Context read/citation: reads only the supplied prompt view model.
6. Capability cards: none added or consumed.
7. Policy hooks: no policy hook is invoked.
8. World projection: keeps task-time runtime readiness visible by linking the
   prompt to the existing Browser Runtime settings destination.
9. Harness cases: focused prompt tests cover button visibility and callback
   behavior; existing prompt tests cover prepare/defer/no-browser actions.
10. Rollback/disable path: revert this PR to remove the optional callback and
    settings action.
11. Deliberately not owned: root App/task runtime wiring, IPC, TaskEvents,
    checkpoint writes, settings persistence, runtime action execution, error
    recovery links, provider promotion, and DB migrations.

## Allowed Files

- `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.tsx`
- `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4h-task-time-prompt-deep-link.md`

## Non-Goals

- No AppShell, App, SettingsPanel, or task runtime wiring.
- No backend IPC, TaskEvent emission, checkpoint writes, settings persistence,
  DB migration, or runtime side effects.
- No error/recovery-surface links.

## Impact Targets

- `BrowserRuntimeTaskTimePrompt`
- `eventPreview`

Pre-edit GitNexus impact must be LOW or this phase stops.

## Implementation

- Add an optional `onOpenBrowserRuntimeSettings` prop.
- Render a compact Browser Runtime Settings button in the prompt footer only
  when the callback is supplied.
- Add focused tests for callback behavior and hidden-by-default behavior.

## Verification

- `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- `cd ui && npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is not applicable
  unless Rust files change.
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes` on staged changes.

## Rollback

Revert the Phase 4H PR. The rollback removes the optional prompt settings
callback, focused tests, this plan, and the tracker update.
