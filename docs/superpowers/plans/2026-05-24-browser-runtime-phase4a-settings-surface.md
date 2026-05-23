# Browser Runtime Phase 4A - Settings Surface Substrate

## Scope

Phase 4A starts ADR Phase 4 with a reversible, UI-only Browser Runtime settings
surface. It adds a first-class Settings destination, a typed view-model for
runtime-pack status, and focused tests. It does not wire IPC commands or perform
runtime mutations.

## ADR Section 18 Questions

1. Intent: make Browser Runtime state visible in Settings before real prepare,
   repair, cleanup, rollback, or task-time prompts land.
2. Autonomy boundary: the UI can display status and inert action affordances;
   it cannot execute runtime-pack side effects.
3. Truth source: Phase 2 runtime-pack status report remains the intended truth
   source; this slice introduces the frontend adapter only.
4. TaskEvent: no TaskEvents are emitted in Phase 4A.
5. Context: no browser task context or runtime pack filesystem context is
   mutated.
6. Capability: no provider capability is promoted or enabled by this slice.
7. Hooks: no Tauri, shell, or automation hooks are added.
8. Projection: the visible settings model mirrors runtime status, update,
   rollback, developer fallback, and auto-prepare projection fields.
9. Harness: Vitest covers the view-model, Settings nav entry, and readonly
   Browser Runtime settings rendering.
10. Rollback: revert the new settings files plus SettingsTab, SettingsNav,
    SettingsPanel, tracker, and this plan.
11. What this does not own: IPC/deep links, SearchPalette integration,
    task-time prompt, real runtime operations, DB migrations, provider
    selection, and Playwright launch behavior.

## Allowed Files

- `ui/src/atoms/settings-tab.ts`
- `ui/src/components/settings/SettingsNav.tsx`
- `ui/src/components/settings/SettingsPanel.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `ui/src/components/settings/SettingsNav.test.tsx`
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts`
- `ui/src/lib/browser-runtime/browser-runtime-settings.test.ts`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No IPC commands.
- No install, repair, reinstall, cleanup, rollback, or run-doctor side effects.
- No SearchPalette deep link.
- No task checkpointing or `paused_waiting_for_browser_runtime`.
- No DB migration or DMZ files.
- No provider promotion, CLI execution, MCP sidecar, or hosted provider work.

## Impact Targets

- `SettingsPanel` in `ui/src/components/settings/SettingsPanel.tsx`
- `SettingsContent` in `ui/src/components/settings/SettingsPanel.tsx`
- `SettingsNav` in `ui/src/components/settings/SettingsNav.tsx`
- `SettingsTab` in `ui/src/atoms/settings-tab.ts` when indexed; GitNexus did
  not resolve this type alias, so manual scope stays limited to the union.

## Rollback

Revert the Phase 4A commit. The settings dialog falls back to the Phase 3H
state with no Browser Runtime tab, and no runtime side effects need cleanup.

## Verification

- `cd ui && npm test -- --run ui/src/lib/browser-runtime/browser-runtime-settings.test.ts ui/src/components/settings/BrowserRuntimeSettings.test.tsx ui/src/components/settings/SettingsNav.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
