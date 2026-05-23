# Browser Runtime Phase 4C - Auto-Prepare Control Intent

## Scope

Phase 4C adds an explicit Browser Runtime Settings intent for automatic
preparation control. The UI explains that disabling auto-prepare only affects
startup/background downloads; it does not disable browser automation capability
or task-time preparation prompts.

## ADR Section 18 Questions

1. Intent: make auto-prepare control semantics visible before any settings
   persistence or runtime execution is wired.
2. Autonomy boundary: selecting the control only updates a local preview; no
   backend command, settings write, filesystem mutation, download, process
   launch, or task checkpoint occurs.
3. Truth source: existing Browser Runtime settings input remains the local
   source for auto-prepare state until IPC/settings persistence is designed.
4. TaskEvent: no TaskEvents are emitted; the preview lists future event names
   only as projection planning evidence.
5. Context: no runtime pack, task, identity, profile, or filesystem context is
   changed.
6. Capability: browser automation capability is not disabled or promoted by
   this control.
7. Hooks: no Tauri, shell, SearchPalette, Startup Doctor, or task-time hooks are
   added.
8. Projection: the selected preview records the intended policy boundary but
   does not change World Projection state.
9. Harness: Vitest covers enabled/disabled auto-prepare preview semantics and
   the default unknown state.
10. Rollback: revert this PR to return Phase 4B action previews without the
    auto-prepare control intent.
11. What this does not own: settings persistence, IPC, backend policy prompts,
    SearchPalette/Startup Doctor deep links, task-time prompts, task
    checkpointing, DB migrations, provider routing, and Playwright behavior.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- `ui/src/lib/browser-runtime/browser-runtime-settings.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No settings persistence or IPC commands.
- No install, repair, reinstall, cleanup, rollback, run-doctor, or
  auto-prepare execution.
- No SearchPalette, Startup Doctor, task-time prompt, or error/recovery deep
  links.
- No task checkpointing or `paused_waiting_for_browser_runtime`.
- No DB migration, provider promotion, Playwright launch, or DMZ files.

## Impact Targets

- `BrowserRuntimeSettings` in
  `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `deriveBrowserRuntimeSettingsViewModel`, `deriveActions`, `actionPreview`,
  and `actionSummary` in
  `ui/src/lib/browser-runtime/browser-runtime-settings.ts`

## Rollback

Revert the Phase 4C commit. Browser Runtime Settings returns to the Phase 4B
action preview surface without the auto-prepare control intent.

## Verification

- `cd ui && npm test -- --run src/lib/browser-runtime/browser-runtime-settings.test.ts src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
