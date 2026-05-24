# Phase 4T - Browser Runtime Settings Status Field Clarity

## Summary

Phase 4S merged the read-only Browser Runtime status IPC boundary. A fresh
reviewer pass on the earlier Phase 4A Settings surface accepted the GitNexus
HIGH blast radius but found that update state and developer fallback state were
rendered as unlabeled trailing values in unrelated rows. Phase 4T fixes only
that Settings presentation ambiguity before live status wiring makes the same
surface more important.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need to scan Browser Runtime Settings and understand update state,
   rollback availability, and developer fallback state without guessing which
   row a trailing value belongs to.
2. What autonomy level can it run at?
   L0/L1 display-only. It changes labels and tests only.
3. What is the canonical truth source?
   The existing `BrowserRuntimeSettingsViewModel`, which derives from the
   Phase 2 runtime-pack status report plus local Settings preview inputs.
4. What TaskEvent entries does it emit?
   None.
5. What context does it read, and how is it cited?
   It reads the already supplied Settings view model props. No new external
   context, filesystem state, IPC, or citation surface is added.
6. What capability cards does it add or consume?
   None.
7. What policy hooks can block it?
   None. This phase has no side effects or runtime action execution.
8. What world projection does the UI render?
   The existing Browser Runtime Settings projection, with update state and
   developer fallback promoted to first-class rows.
9. What harness cases prove it works?
   Focused Vitest coverage for the Browser Runtime Settings component plus the
   default browser-runtime Rust regressions and whitespace/GitNexus checks.
10. What is the rollback or disable path?
   Revert this PR. It changes no persistent data or runtime state.
11. What does it deliberately not own?
   It does not wire live IPC data into Settings, call `getBrowserRuntimeStatus`,
   change `getSettings`, execute runtime-pack actions, persist auto-prepare,
   launch Playwright, promote providers, edit DMZ files, or change backend code.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4t-settings-status-clarity.md`

## Non-Goals

- No IPC, Settings live data, root `App`, shared startup initialization, or
  `getSettings` changes.
- No runtime-pack install, repair, reinstall, cleanup, rollback, downloads,
  deletes, launches, provider promotion, TaskEvents, DB writes, or settings
  writes.
- No Playwright CLI/MCP sidecar or no-browser fallback execution.

## Impact Targets

- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW; 1 direct dependent (`SettingsContent`), 2 affected
  Settings processes.
- `ui/src/lib/browser-runtime/browser-runtime-settings.ts::deriveBrowserRuntimeSettingsViewModel`:
  GitNexus impact LOW; 1 direct dependent (`BrowserRuntimeSettings`), 2
  affected Settings processes. This symbol is observed for context and should
  not need editing in this phase.

## Rollback

Revert this PR. The change is display-only and has no runtime, database,
filesystem, settings, task, artifact, provider, or browser-session side effects.

## Verification

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` (expected N/A; no Rust files)
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes`
