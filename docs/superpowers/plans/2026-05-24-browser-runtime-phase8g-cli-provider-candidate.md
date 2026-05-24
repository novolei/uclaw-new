# Phase 8G - CLI/MCP Provider Candidate Route Inputs

## Goal

Advance ADR Phase 8 by letting the live provider execution boundary route with
feature-flagged Playwright CLI/MCP candidate inputs, while preserving the safe
default local Chromium path and avoiding provider promotion.

## ADR Section 18 Questions

1. What user-visible behavior changes?
   - None by default. Browser actions still execute through local Chromium.
     When future runtime policy enables CLI/MCP route inputs, route decisions
     can see those candidates before execution wiring is enabled.
2. What state does it own?
   - A small `BrowserProviderActionRouteOptions` value owned by the provider
     executor for feature flags, optional runtime-pack readiness evidence, and
     disabled provider IDs.
3. What events does it emit?
   - No new event names. Existing provider selected/degraded/rollback intents
     continue to flow through Phase 8D/8E conversion.
4. What context does it read, and how is it cited?
   - Browser action shape, safe runtime feature flags, optional runtime-pack
     status report, and disabled provider IDs. This phase does not expose new
     model-visible context.
5. What capability cards does it add or consume?
   - It consumes the existing local Chromium, Playwright CLI, and Playwright MCP
     provider capability/status contracts.
6. What policy hooks can block it?
   - Feature flags, runtime-pack readiness, disabled provider IDs, and the
     existing non-local fail-closed execution guard.
7. What world projection does the UI render?
   - No UI change. Existing provider route signals remain projection-compatible.
8. What harness cases prove it works?
   - Focused provider execution tests showing default local selection, optional
     CLI candidate selection when local is disabled and CLI is ready, and
     fail-closed execution before local registry fallback.
9. What is the rollback/disable path?
   - Revert this PR. Safe defaults omit CLI/MCP candidates unless explicitly
     enabled in route options.
10. What does it deliberately not own?
   - CLI/MCP action execution in live tasks, provider promotion, Settings/IPC,
     DB migration, hosted providers, raw provider tools, and runtime pack
     mutation.
11. What are the migration and compatibility constraints?
   - No schema migration. Default constructor behavior must remain compatible
     with Phase 8F and preserve local Chromium execution.

## Allowed Files

- `src-tauri/src/browser/provider_execution.rs`
- `src-tauri/src/browser/provider_execution_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8g-cli-provider-candidate.md`

## Non-Goals

- Do not execute Playwright CLI/MCP from `BrowserAgentLoop`.
- Do not promote Playwright CLI/MCP above local Chromium.
- Do not add UI, IPC, Settings, DB, hosted-provider, or raw-tool exposure.
- Do not touch `agentic_loop.rs` or `tauri_commands.rs` in this slice.

## Impact Targets

- `BrowserProviderActionExecutor` in `src-tauri/src/browser/provider_execution.rs`
- `BrowserProviderActionExecutor::new`
- `BrowserProviderActionExecutor::route_action`
- `route_live_browser_action_provider`
- `provider_route_blocks_local_action`

Pre-edit GitNexus impact reported LOW risk for these targets; no HIGH/CRITICAL
risk was observed.

## Rollback

Revert this PR. The Phase 8F provider execution boundary remains recoverable
from git history, and no runtime state or user data migration is involved.

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs
git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8g-cli-provider-candidate.md
GitNexus detect_changes scope=staged
```
