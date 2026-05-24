# Phase 4U - Browser Runtime Settings Live Status Read

## Summary

Phase 4S added the dedicated read-only `get_browser_runtime_status` command and
Phase 4T clarified the Settings presentation. Phase 4U consumes that read-only
bridge from Browser Runtime Settings so the Settings surface can show real
runtime-pack status without using shared `getSettings` or adding any action
execution.

## ADR Section 18 Questions

1. What user intent does this support?
   Users need the Browser Runtime Settings destination to show current local
   runtime-pack status instead of a permanently static placeholder.
2. What autonomy level can it run at?
   L1 read-only local inspection. It invokes a read-only Tauri command and
   renders the returned status report.
3. What is the canonical truth source?
   Rust-owned `BrowserRuntimePackStatusReport` returned by
   `get_browser_runtime_status`, then mapped through the existing
   `BrowserRuntimeSettingsViewModel`.
4. What TaskEvent entries does it emit?
   None. This phase reads status only.
5. What context does it read, and how is it cited?
   It reads the local runtime-pack status report via the dedicated Browser
   Runtime bridge. No new citation surface is added.
6. What capability cards does it add or consume?
   None.
7. What policy hooks can block it?
   None for the read-only status query. Future prepare/repair actions remain
   policy-gated and out of scope.
8. What world projection does the UI render?
   Browser Runtime Settings renders the live runtime-pack status when no
   explicit preview/test status prop is provided.
9. What harness cases prove it works?
   Focused Settings component tests mock the bridge and prove live read
   consumption, plus default browser-runtime Rust regressions, whitespace, and
   GitNexus checks.
10. What is the rollback or disable path?
   Revert this PR. The change has no persistent side effects.
11. What does it deliberately not own?
   It does not execute runtime-pack actions, persist settings, change
   `getSettings`, wire Startup Doctor, edit root `App`, touch DMZ files, launch
   Playwright, promote providers, or mutate user data.

## Allowed Files

- `ui/src/components/settings/BrowserRuntimeSettings.tsx`
- `ui/src/components/settings/BrowserRuntimeSettings.test.tsx`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase4u-settings-live-status-read.md`

## Non-Goals

- No runtime-pack install, repair, reinstall, cleanup, rollback, downloads,
  deletes, launches, TaskEvents, DB writes, provider promotion, or settings
  writes.
- No root `App`, Startup Doctor, `getSettings`, backend command registration,
  `tauri_commands.rs`, `agentic_loop.rs`, or DB migration changes.
- No Playwright CLI/MCP sidecar or no-browser fallback execution.

## Impact Targets

- `ui/src/components/settings/BrowserRuntimeSettings.tsx::BrowserRuntimeSettings`:
  GitNexus impact LOW; 1 direct dependent (`SettingsContent`), 2 affected
  Settings processes.
- `ui/src/lib/tauri-bridge.ts::getBrowserRuntimeStatus`: GitNexus impact LOW;
  0 direct dependents and 0 affected processes before this phase.

## Rollback

Revert this PR. The bridge call is read-only and leaves no runtime files,
settings, DB rows, task state, artifacts, providers, or browser sessions to
clean up.

## Verification

- `cd ui && npm test -- --run src/components/settings/BrowserRuntimeSettings.test.tsx`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests`
- `rustfmt --edition 2021 --check <changed-rust-files>` (expected N/A; no Rust files)
- `git diff --check -- <changed-files>`
- `git diff --cached --check`
- GitNexus `detect_changes`
