# Browser Runtime Phase 4D - Task-Time Prompt Model

## Scope

Phase 4D adds a pure frontend model for the task-time Browser Runtime
preparation prompt. It derives prepare-now, defer, and continue-without-browser
choices from the Phase 2 runtime-pack status report and task fallback context,
but it does not render UI, write checkpoints, call IPC, or mutate runtime state.

## ADR Section 18 Questions

1. Intent: define task-time prompt semantics before any runtime execution or
   task checkpoint side effect is wired.
2. Autonomy boundary: the model returns choices only; no backend command,
   TaskEvent, settings write, checkpoint write, filesystem mutation, download,
   or process launch occurs.
3. Truth source: Phase 2 runtime-pack status report plus explicit task fallback
   input determine prompt state.
4. TaskEvent: no TaskEvents are emitted; future event names are listed as
   preview metadata only.
5. Context: no browser task, runtime pack, identity, or filesystem context is
   changed.
6. Capability: no provider lane is promoted; no-browser fallback remains an
   explicit task capability input.
7. Hooks: no Tauri, shell, SearchPalette, Startup Doctor, settings, or task
   runner hooks are added.
8. Projection: the model exposes future projection/checkpoint intent metadata
   without changing World Projection state.
9. Harness: Vitest covers ready, prepare-now, defer/checkpoint, no-browser
   fallback, and blocked runtime states.
10. Rollback: revert this PR to remove the pure prompt model and return Phase
    4C as the latest Settings-only surface.
11. What this does not own: UI rendering, IPC, runtime-pack execution, deep
    links, real `paused_waiting_for_browser_runtime` checkpoint writes, DB
    migrations, provider routing, and Playwright behavior.

## Allowed Files

- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.ts`
- `ui/src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No UI component or Settings/SearchPalette/Startup Doctor wiring.
- No IPC commands, TaskEvents, or checkpoint writes.
- No install, repair, cleanup, rollback, auto-prepare, or run-doctor execution.
- No DB migration, provider promotion, Playwright launch, or DMZ files.

## Impact Targets

- New additive frontend model symbols only. No existing function/class/method is
  modified in this slice.

## Rollback

Revert the Phase 4D commit. The repo returns to Phase 4C with Settings
auto-prepare preview semantics and no task-time prompt model.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-task-prompt.test.ts`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
