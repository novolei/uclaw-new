# Browser Runtime Phase 4E - Task-Time Prompt UI

## Scope

Phase 4E renders the Phase 4D task-time Browser Runtime prompt model as an
additive React component. The component shows prepare-now, defer, and
continue-without-browser choices and reports the selected local action to a
caller callback. It does not wire the prompt into App, task runtime,
SearchPalette, Startup Doctor, IPC, settings persistence, or checkpoint writes.

## ADR Section 18 Questions

1. Intent: give users a clear task-time Browser Runtime decision surface before
   any runtime execution or task checkpoint side effect is wired.
2. Autonomy boundary: the component is a presentation/callback surface only; no
   backend command, TaskEvent, filesystem mutation, download, or process launch
   occurs.
3. Truth source: the Phase 4D prompt view model remains the canonical truth for
   status, copy, actions, future event names, and checkpoint intent.
4. TaskEvent: no TaskEvents are emitted; event names are displayed as future
   metadata for the later wiring slice.
5. Context: no browser task, runtime pack, identity, settings, or filesystem
   context is changed.
6. Capability: no provider lane is promoted; no-browser fallback remains an
   explicit model action supplied by task context.
7. Hooks: no Tauri, shell, SearchPalette, Startup Doctor, settings, or task
   runner hooks are added.
8. Projection: the component exposes future checkpoint/projection intent
   visually without changing World Projection state.
9. Harness: Vitest covers hidden ready state, prepare/defer/no-browser rendering,
   disabled actions, checkpoint messaging, and callback selection.
10. Rollback: revert this PR to remove the standalone prompt component while
    keeping the Phase 4D pure model.
11. What this does not own: IPC, runtime-pack execution, deep links, real
    `paused_waiting_for_browser_runtime` checkpoint writes, DB migrations,
    TaskEvent emission, provider routing, App integration, and Playwright
    behavior.

## Allowed Files

- `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.tsx`
- `ui/src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No App, task runner, SearchPalette, Startup Doctor, or Settings wiring.
- No IPC commands, TaskEvents, settings persistence, or checkpoint writes.
- No install, repair, cleanup, rollback, auto-prepare, or run-doctor execution.
- No DB migration, provider promotion, Playwright launch, or DMZ files.

## Impact Targets

- New additive frontend component/test files only. Existing functions, classes,
  methods, backend modules, DMZ files, and provider contracts are not modified.

## Rollback

Revert the Phase 4E commit. The repo returns to Phase 4D with only the pure
task-time prompt model and no rendered prompt component.

## Verification

- `cd ui && npm test -- --run src/components/browser-runtime/BrowserRuntimeTaskTimePrompt.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
