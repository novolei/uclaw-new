# Browser Runtime Phase 4B - Settings Action Intents

## Scope

Phase 4B advances ADR Phase 4 controls by making Browser Runtime settings
actions selectable as local intent previews. The user can inspect prepare,
repair, reinstall, cleanup, rollback, defer, retry, keep-current, and
run-doctor intent metadata, but the UI still performs no runtime side effects.

## ADR Section 18 Questions

1. Intent: make Browser Runtime controls understandable before IPC execution is
   wired.
2. Autonomy boundary: a click selects a local preview only; no backend command,
   filesystem mutation, download, process launch, or task mutation occurs.
3. Truth source: Phase 2 runtime-pack status report and operation-plan event
   names remain the source for action availability and preview copy.
4. TaskEvent: no TaskEvents are emitted; event names are displayed as future
   projection evidence only.
5. Context: no browser task, runtime pack, identity, or filesystem context is
   changed.
6. Capability: no provider lane is promoted or enabled.
7. Hooks: no Tauri, shell, automation, or search hooks are added.
8. Projection: the selected intent preview exposes future event names and
   confirmation/destructive boundaries without changing projection state.
9. Harness: Vitest covers action derivation and no-side-effect UI selection.
10. Rollback: revert this PR to return Phase 4A to readonly disabled action
    affordances.
11. What this does not own: IPC execution, deep links, SearchPalette, Startup
    Doctor links, task-time prompts, task checkpointing, DB migrations,
    provider routing, and Playwright process behavior.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- `ui/src/lib/browser-runtime/browser-runtime-settings.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No IPC commands.
- No install, repair, reinstall, cleanup, rollback, or run-doctor execution.
- No SearchPalette or Startup Doctor deep links.
- No task-time browser-runtime prompt.
- No task checkpointing or `paused_waiting_for_browser_runtime`.
- No DB migration, provider promotion, Playwright launch, or DMZ files.

## Impact Targets

- `BrowserRuntimeSettings` in
  `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `deriveBrowserRuntimeSettingsViewModel` in
  `ui/src/lib/browser-runtime/browser-runtime-settings.ts`

## Rollback

Revert the Phase 4B commit. Browser Runtime Settings returns to the Phase 4A
readonly surface with visible disabled actions and no preview selection state.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
