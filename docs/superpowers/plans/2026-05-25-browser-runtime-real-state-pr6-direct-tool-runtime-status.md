# Browser Runtime Real-State PR6 - Direct Tool Runtime Status

## Goal

Close the next real-state gap after PR1/PR3/PR5: ordinary direct browser tools
must inspect the shared Rust Browser Runtime status before launching or acting
on a browser context. Where a direct tool already maps to `BrowserAction`, it
should execute through `BrowserProviderActionExecutor` so the same provider
route options used by autonomous browser tasks also govern direct tool calls.

## ADR Section 18 Answers

1. User intent: direct chat, Agent session, and automation browser tools should
   use the same Browser Runtime truth source as Startup Splash and browser task
   runs.
2. Autonomy: L1-L3 direct tool execution and automation runs; no autonomous
   escalation beyond the existing tool call.
3. Canonical truth source: `BrowserRuntimeStatusService::inspect_default()`
   plus `BrowserProviderActionExecutor` route decisions.
4. TaskEvent entries: no new persisted TaskEvent in this slice; provider route
   event intents remain in the existing route decision envelope.
5. Context read: browser runtime-pack status, active context sessions, provider
   readiness, and direct tool parameters.
6. Capability cards: consumes existing browser tool capabilities and
   `BrowserAction` provider execution capabilities; adds no new public tool.
7. Policy hooks: existing tool permission/registration gates, Browser Runtime
   provider routing guards, identity/profile policy where already present, and
   file-upload path checks.
8. World projection: no new UI projection; this makes direct actions consume
   the Rust runtime status that the UI already queries.
9. Harness/tests: focused Rust tests for `browser::tools`,
   `browser::provider_execution`, `browser::runtime_status`, and automation
   tool registry compile coverage.
10. Rollback: revert this PR. Direct tools return to direct
    `BrowserContextManager` calls while PR1/PR3/PR5 remain intact.
11. Deliberately not owned: no provider promotion, no Startup Splash changes,
    no Settings execution, no runtime-pack install/repair/delete, no hosted
    provider, no raw Playwright tool exposure, no DB migration, and no new
    TaskEvent persistence.

## Allowed Files

- `src-tauri/src/browser/tools.rs`
- `src-tauri/src/tauri_commands.rs`
- `src-tauri/src/automation/runtime/tool_registry.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- this plan file

## Implementation Plan

1. Extend ordinary direct browser tool structs and `BrowserRunScriptTool` with
   optional `BrowserRuntimeStatusService`.
2. Add a small helper in `browser/tools.rs` that inspects runtime status,
   converts the runtime-pack report into `BrowserProviderActionRouteOptions`,
   and logs/falls back to default options if status inspection fails.
3. Route direct tools with exact `BrowserAction` equivalents through
   `BrowserProviderActionExecutor`: navigate, click, type, scroll, send_keys,
   evaluate, get_state, list_tabs, switch_tab, close_tab, and upload_file.
4. For direct tools without a matching action/result envelope, status-touch the
   runtime service before the existing `BrowserContextManager::get_or_create`
   call so those paths still consume real Rust runtime state.
5. Pass the shared service into chat and Agent session browser tool registries;
   pass an equivalent service built from the same context manager into
   automation browser tools.
6. Update the tracker with PR6 scope, impact notes, verification, and next
   action.

## Impact Notes

- `npx gitnexus analyze` was refreshed in the PR6 worktree before edits.
- GitNexus pre-edit impact for `src-tauri/src/browser/tools.rs` as a file
  target: LOW, 0 affected processes.
- GitNexus pre-edit impact for `BrowserProviderActionExecutor`: LOW, 0
  affected processes.
- GitNexus pre-edit impact for `BrowserRuntimeStatusService`: LOW, 0 affected
  processes.
- GitNexus pre-edit impact for `AutomationToolRegistryDeps`: LOW, 3 direct
  test callers, 0 affected processes.
- GitNexus pre-edit impact for `AppRuntimeService`: LOW, 0 affected processes.
- GitNexus cannot resolve `src-tauri/src/tauri_commands.rs` because the analyzer
  skips it as the one large file over the 512KB threshold; this PR keeps that
  edit limited to the two direct-browser-tool registration macros.

## Verification Plan

- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status`
- `cargo test --manifest-path src-tauri/Cargo.toml --lib automation::runtime::tool_registry`
- `git diff --check -- <changed-files>`
- `npx gitnexus detect-changes --scope staged --repo /Users/ryanliu/Documents/uclaw-worktrees/browser-runtime-real-state-pr6-direct-tool-runtime-status`
