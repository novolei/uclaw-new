# Browser Runtime Phase 4F - Settings Deep Link

## Scope

Phase 4F wires the first real Browser Runtime settings deep link from
SearchPalette into the existing Settings dialog. SearchPalette settings entries
carry an explicit `SettingsTab`, and AppShell opens SettingsDialog directly on
that tab. This slice focuses on SearchPalette only.

## ADR Section 18 Questions

1. Intent: make the Browser Runtime Settings destination reachable from the
   command palette before backend runtime actions are wired.
2. Autonomy boundary: this is user navigation only; no runtime preparation,
   IPC, TaskEvent, checkpoint, download, or filesystem side effect occurs.
3. Truth source: `settingsTabAtom` and `settingsOpenAtom` remain the canonical
   frontend truth for the Settings dialog destination and open state.
4. TaskEvent: no TaskEvents are emitted; deep-link navigation is a local UI
   action.
5. Context: no browser task, runtime pack, identity, settings persistence, or
   filesystem context is changed.
6. Capability: no provider lane is promoted and no BrowserProvider behavior is
   changed.
7. Hooks: only the existing SearchPalette selection handler is used; no Tauri,
   startup, task-runner, or backend hooks are added.
8. Projection: no World Projection state changes; this only opens the existing
   Settings projection surface.
9. Harness: Vitest covers SearchPalette rendering and Browser Runtime settings
   payload selection; UI build verifies AppShell type wiring.
10. Rollback: revert this PR to return SearchPalette settings shortcuts to the
    previous non-deep-linked behavior.
11. What this does not own: Startup Doctor deep links, task-time prompt deep
    links, error/recovery surface links, IPC commands, TaskEvents, checkpoint
    writes, DB migrations, provider routing, and runtime side effects.

## Allowed Files

- `ui/src/components/search/SearchPalette.tsx`
- `ui/src/components/search/SearchPalette.test.tsx`
- `ui/src/components/app-shell/AppShell.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Non-goals

- No Startup Doctor, task-time prompt, error/recovery surface, or Settings tab
  UI changes beyond opening the existing tab.
- No IPC commands, TaskEvents, settings persistence, or checkpoint writes.
- No install, repair, cleanup, rollback, auto-prepare, or run-doctor execution.
- No DB migration, provider promotion, Playwright launch, or DMZ files.

## Impact Targets

- `SearchPalette`: GitNexus impact LOW; direct caller `AppShell`, affected
  process labels `App` and `AppShell`.
- `SETTINGS_ITEMS`: GitNexus impact LOW; no affected processes.
- `SettingsItem`: GitNexus impact LOW; AppShell import/file relationship only.
- `handleSearchResultSelect`: GitNexus impact LOW; no affected processes.

## Rollback

Revert the Phase 4F commit. SearchPalette continues to show settings shortcuts,
but Browser Runtime Settings is not directly opened from the palette.

## Verification

- `cd ui && npm test -- --run src/components/search/SearchPalette.test.tsx`
- `cd ui && npm run build`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` is N/A if no Rust
  files change.
- `git diff --check -- <changed-files>`
- GitNexus `detect_changes`
