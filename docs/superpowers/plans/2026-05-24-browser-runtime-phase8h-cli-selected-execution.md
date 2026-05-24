# Phase 8H - CLI Selected-Route Execution

## Goal

Advance ADR Phase 8 from CLI/MCP route visibility to a real selected-provider
execution boundary for Playwright CLI, without promoting CLI as the default
provider. Safe defaults still execute local Chromium; CLI execution only happens
when explicit route options select the CLI provider.

## ADR Section 18 Questions

1. What user intent does this support?
   - Browser actions that can be satisfied by the supervised Playwright CLI thin
     lane once policy/runtime route options select that provider.
2. What autonomy level can it run at?
   - Same as existing browser actions. This phase does not raise autonomy; it
     only preserves supervised execution under the existing browser action
     boundary.
3. What is the canonical truth source?
   - `BrowserActionResult`, browser task steps, route decisions, and artifact
     refs remain the uClaw truth. Playwright worker output is normalized into
     that result shape.
4. What TaskEvent entries does it emit?
   - No new event names. Existing provider route selected/degraded/blocked
     signals stay visible through the Phase 8D/8E bridge.
5. What context does it read, and how is it cited?
   - The selected route decision, feature flags, runtime-pack readiness report,
     browser action, and Playwright CLI provider result. Artifact refs are kept
     in the normalized result payload.
6. What capability cards does it add or consume?
   - It consumes the existing Playwright CLI provider card and declarative
     action contract.
7. What policy hooks can block it?
   - Feature flags, disabled provider IDs, runtime-pack readiness, unsupported
     action mapping, and the app-managed worker path guard.
8. What world projection does the UI render?
   - No UI change. Browser task steps continue to show normalized action
     execution; route signals continue to expose provider choice.
9. What harness cases prove it works?
   - Focused provider execution tests prove selected CLI routes execute through
     the managed adapter, preserve artifact refs/output, block unsupported
     selected actions, and keep safe defaults local.
10. What is the rollback or disable path?
   - Revert this PR or leave Playwright CLI route options disabled. Default
     local Chromium execution remains intact.
11. What does it deliberately not own?
   - Provider promotion, UI/IPC/DB, Settings toggles, hosted providers, MCP
     execution, raw script exposure, global npm, user-installed Playwright, or
     changes to provider score algorithms.

## Allowed Files

- `src-tauri/src/browser/provider_execution.rs`
- `src-tauri/src/browser/provider_execution_tests.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8h-cli-selected-execution.md`

## Non-Goals

- Do not make Playwright CLI the default route.
- Do not execute MCP-selected routes in this slice.
- Do not expose raw Playwright tools or scripts.
- Do not add UI, IPC, Settings, DB, or hosted-provider paths.
- Do not introduce global npm or user-installed Playwright as a production
  fallback.

## Impact Targets

- `BrowserProviderActionExecutor`
- `BrowserProviderActionExecutor::execute_routed_with_identity`
- `BrowserProviderActionRouteOptions`
- `BrowserProviderActionExecutionOutcome`
- `provider_route_blocked_step`

Pre-edit GitNexus impact reported LOW risk for these targets; no HIGH/CRITICAL
risk was observed.

## Rollback

Revert this PR. The Phase 8G route input behavior remains recoverable from git
history, and safe-default local Chromium execution stays available.

## Verification

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs
git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8h-cli-selected-execution.md
GitNexus detect_changes scope=staged
```
