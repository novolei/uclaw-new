# Browser Runtime Real State PR5 - UI Command Runtime Touch

## Goal

Make Browser UI IPC commands consult the real Rust Browser Runtime status before
browser execution, and route the browser UI actions that already have product
level `BrowserAction` equivalents through `BrowserProviderActionExecutor`.

This closes the next gap after PR #506: task-time agent routing uses aggregate
runtime status, but app UI browser commands still call `BrowserContextManager`
directly.

## ADR Section 18 Answers

1. User intent: direct Browser Panel actions such as navigate, switch tab,
   screenshot, DOM read, click, reload, screencast, and login completion.
2. Autonomy level: user-directed UI commands only; no new autonomous behavior.
3. Canonical truth source: Rust `BrowserRuntimeStatusService`, composed from the
   runtime pack, supervisor, provider readiness, and live context manager state.
4. TaskEvent entries: none added in this slice; this is command routing/status
   plumbing before new event emission.
5. Context read/citation: reads live runtime status in-process and logs command
   name, supervisor state, doctor status, active context count, and pack
   readiness.
6. Capability cards: consumes existing Local Chromium / Playwright CLI / MCP
   provider readiness through `BrowserProviderActionExecutor`.
7. Policy hooks: no new policy hook; provider route blocking remains respected
   for routed `BrowserAction` commands.
8. World projection: no new UI projection; PR #507 separately projects Browser
   Panel runtime status.
9. Harness cases: Rust unit/compile coverage for provider execution and browser
   command helpers, plus runtime status/provider focused tests.
10. Rollback path: revert this plan, tracker row, and the `tauri_commands.rs`
    helper/call-site changes.
11. Non-ownership: does not implement new provider actions for coordinate
    clicks, reload/back/forward, screencast, screenshots, login probing, or
    legacy `BrowserService` commands.

## Allowed Files

- `src-tauri/src/tauri_commands.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr5-ui-command-runtime-touch.md`

## Non-goals

- No Splash/App changes; PR #504 owns that gate.
- No Browser Panel frontend changes; PR #507 owns that gate.
- No Playwright provider promotion or feature flag changes.
- No broad legacy `BrowserService` rewrite.
- No new migrations.

## Impact Notes

- GitNexus cannot resolve individual `tauri_commands.rs` Browser UI IPC
  functions because the file is skipped as a large file during analysis.
- Indexed symbols used by this slice reported LOW impact:
  `BrowserProviderActionExecutor`, `BrowserRuntimeStatusService`,
  `BrowserProviderActionRouteOptions`, and `BrowserActionResult`.
- The slice keeps behavior narrow: only `browser_ui_navigate` and
  `browser_ui_switch_tab` move to the provider executor because they have direct
  `BrowserAction` equivalents and existing command return shapes can be
  preserved.

## Verification

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib tauri_commands`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib tauri_commands::browser_ui_runtime_command_tests`
- `rustfmt --edition 2021 --check src-tauri/src/tauri_commands.rs` is a known
  large-file caveat: it currently wants to reformat thousands of unrelated
  pre-existing lines, so this PR uses diff whitespace checks plus compile tests
  instead of whole-file rustfmt.
- `git diff --check -- src-tauri/src/tauri_commands.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-25-browser-runtime-real-state-pr5-ui-command-runtime-touch.md`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr5-ui-command-runtime-touch`
