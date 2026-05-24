# Browser Runtime Phase 4I - Error Recovery Settings Deep Link

## Scope

Phase 4I adds a structured error/recovery action that opens Browser Runtime
Settings from agent error surfaces. It is a frontend navigation contract only
and does not emit events, write checkpoints, or execute runtime operations.

## ADR Section 18 Answers

1. User intent: help users jump from a browser-runtime error or recovery hint
   directly to the Browser Runtime / Startup Doctor settings destination.
2. Autonomy level: L0/L1 UI navigation only.
3. Canonical truth source: structured recovery actions remain the source for
   error-surface affordances; Browser Runtime Settings remains the destination.
4. TaskEvent entries: none are emitted in this slice.
5. Context read/citation: reads only the rendered SDK error message and its
   structured `_errorActions` metadata.
6. Capability cards: none added or consumed.
7. Policy hooks: no policy hook is invoked.
8. World projection: keeps browser-runtime recovery visible by linking existing
   error surfaces to the shared settings destination.
9. Harness cases: focused SDK message renderer tests cover direct assistant
   error rendering, grouped assistant-turn error rendering, and unchanged
   generic settings behavior.
10. Rollback/disable path: revert this PR to remove the action handler and
    focused tests.
11. Deliberately not owned: backend IPC, TaskEvents, checkpoint writes, App/task
    runtime wiring, settings persistence, runtime action execution, root render
    error redesign, provider promotion, and DB migrations.

## Allowed Files

- `ui/src/components/agent/SDKMessageRenderer.tsx`
- `ui/src/components/agent/SDKMessageRenderer.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4i-error-recovery-deep-link.md`

## Non-Goals

- No backend IPC, TaskEvent emission, checkpoint writes, settings persistence,
  DB migration, provider promotion, or runtime side effects.
- No App, AppShell, SettingsPanel, task runtime, or root error-boundary rewrite.
- No new recovery action emitted from Rust or agent code.

## Impact Targets

- `ErrorMessage` in `ui/src/components/agent/SDKMessageRenderer.tsx`

Pre-edit GitNexus impact reported HIGH because `ErrorMessage` is shared by
direct and grouped agent message rendering. Fresh reviewer acceptance is
required before editing, and final GitNexus detect must not report
HIGH/CRITICAL without another reviewer stop.

## Implementation

- Add `open_browser_runtime_settings` handling to the existing structured
  recovery action switch.
- Set `settingsTabAtom` to `browserRuntime` and open `settingsOpenAtom`.
- Reuse the existing Settings icon for the new structured action.
- Add focused renderer tests for the direct and grouped error paths plus
  unchanged generic settings behavior.

## Verification

- `cd ui && npm test -- --run src/components/agent/SDKMessageRenderer.test.tsx`
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

Revert the Phase 4I PR. The rollback removes the recovery action mapping,
focused tests, this plan, and the tracker update.
