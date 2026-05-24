# Phase 8E - Live Provider Route Signals

## Summary

Wire Phase 8A-8D provider route decisions into the live browser task action path without changing the selected execution provider. Before each browser action executes through the existing local Chromium action registry, `BrowserAgentLoop` will make a provider-route decision, emit rollout-visible provider route `Signal` events when rollout is enabled, and then continue only when the selected provider is `browser.local_chromium`.

This closes the current Phase 8 dry-run risk: provider routing is no longer only a pure contract or offline event bridge. It is evaluated in the live task loop while preserving local-first execution and leaving CLI/MCP execution adapters for separate PRs.

## ADR Section 18 Answers

### 1. What user intent does this support?

Users who run browser tasks need to know which browser provider is responsible for the current action and whether routing is degraded or blocked. This phase supports observable browser automation without silently changing providers.

### 2. What autonomy level can it run at?

It can run at the same autonomy level as existing browser tasks because it does not add new browser capabilities or side effects. It only selects and records the provider route for actions that were already about to execute.

### 3. What is the canonical truth source?

The browser task run remains canonical for the task lifecycle. Provider route decisions are canonicalized through `BrowserProviderRouter` and rollout-visible `TaskEvent::Signal` entries when rollout is enabled.

### 4. What TaskEvent entries does it emit?

It emits Browser-source `Signal` events using existing Phase 8D codes:

- `browser.provider.selected`
- `browser.provider.degraded`
- `browser.provider.rolled_back`

This phase keeps these as signals, not warnings or tool calls.

### 5. What context does it read, and how is it cited?

It reads the current `BrowserAction` chosen by the decision adapter and maps it to the provider selection request shape. It also reads the local Chromium provider readiness snapshot built in-process from the agent loop/action registry boundary. No external user context, secrets, memory graph writes, or uncited web content are added.

### 6. What capability cards does it add or consume?

It consumes the existing provider capability cards and router from Phase 8A-8C. It does not add or promote a capability card.

### 7. What policy hooks can block it?

Existing browser task policy and identity/runtime-preparation gates still run before action execution. This phase adds a provider-route guard: if the selected provider is not `browser.local_chromium`, the task records a blocked provider route step and stops instead of accidentally dispatching a non-local provider through the local action registry.

### 8. What world projection does the UI render?

No UI rendering changes in this slice. Projection consumers can observe provider route signals through the rollout/event path once enabled. Browser task steps remain visible through the existing task monitor.

### 9. What harness cases prove it works?

Focused tests prove:

- action-to-provider-selection mapping chooses local Chromium for ordinary actions;
- unsupported/non-local route decisions become blocked task steps instead of accidental local execution;
- provider route signal batches share one timestamp;
- existing rollout bridge/provider/runtime regressions still pass.

### 10. What is the rollback or disable path?

Revert this PR. The pure Phase 8A route decision, Phase 8B router, Phase 8C scorecard metadata, and Phase 8D signal bridge remain intact. Runtime rollout writes are still env-gated by `UCLAW_ROLLOUT_ENABLED`.

### 11. What does it deliberately not own?

This phase does not switch execution to Playwright CLI or MCP, promote a provider, add Settings/UI/IPC, persist provider choices, add DB migrations, compute live harness scores, change feature flags, or expose raw provider tools to the model.

## Allowed Files

- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/rollout_bridge.rs`
- `src-tauri/src/browser/rollout_bridge_tests.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8e-live-route-signals.md`

## Non-Goals

- No CLI/MCP provider execution switch.
- No default provider promotion.
- No hosted provider.
- No UI, IPC, Settings, or DB migration.
- No global npm or user-installed Playwright path.
- No raw Playwright/MCP/CDP tool exposure.
- No fixture-count-based runtime scoring.

## Impact Targets

Pre-edit GitNexus impact was run for:

- `run` in `src-tauri/src/browser/agent_loop.rs` - LOW.
- `execute_with_identity` in `src-tauri/src/browser/action_registry.rs` - LOW; used as context only.
- `provider_route_decision_to_events` in `src-tauri/src/browser/rollout_bridge.rs` - LOW.

Additional symbol edits will receive impact before modification if they touch an existing function/class/method.

## Rollback

Revert the PR commit. Because this phase adds no persistent schema and rollout writes remain env-gated, rollback is a normal code revert with no data migration.

## Verification

Expected focused checks:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs
git diff --check -- src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8e-live-route-signals.md
```

Before commit:

```bash
npx gitnexus detect-changes --scope staged
```
