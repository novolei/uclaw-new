# Phase 8F - Provider Execution Boundary

## Summary

Extract the live provider route/guard logic from `BrowserAgentLoop` into a focused provider execution module. The behavior remains local-first: routed actions still execute through the current local Chromium action registry unless a non-local provider is selected, in which case execution fails closed at the provider boundary.

This keeps `agent_loop.rs` thin after Phase 8E and advances ADR Phase 8 from "route signals in the loop" toward a replaceable `BrowserProvider` execution boundary. CLI and MCP execution adapters remain future slices.

## ADR Section 18 Answers

### 1. What user intent does this support?

Users running browser tasks get the same current local browser behavior, but the runtime now has a dedicated boundary for future provider execution and fallback decisions.

### 2. What autonomy level can it run at?

It runs at the same autonomy level as existing browser tasks because no new provider is enabled and no additional browser side effects are introduced.

### 3. What is the canonical truth source?

`BrowserTaskRun` remains canonical for task state. Provider route decisions remain canonical route metadata and are emitted through Phase 8D/8E rollout signals when enabled.

### 4. What TaskEvent entries does it emit?

No new TaskEvent kind or code. It preserves Phase 8E route signals:

- `browser.provider.selected`
- `browser.provider.degraded`
- `browser.provider.rolled_back`

### 5. What context does it read, and how is it cited?

It reads the current `BrowserAction`, session id, and optional identity profile id already present at action execution time. It does not read external content, memory graph, or secrets.

### 6. What capability cards does it add or consume?

It consumes existing provider capability cards and local Chromium provider status. It adds no new cards and promotes no provider.

### 7. What policy hooks can block it?

Existing browser runtime/identity/prompt policies remain unchanged. The provider execution boundary blocks if route selection chooses a non-local provider before that provider has a live executor.

### 8. What world projection does the UI render?

No UI change. Existing task steps and rollout signals continue to be the projection surface.

### 9. What harness cases prove it works?

Focused tests prove:

- local Chromium executor executes routed actions when local is selected;
- provider execution returns a blocked outcome for non-local routes;
- action-to-selection mapping remains stable for click, screenshot state, and evaluate;
- existing browser agent loop, rollout, provider, runtime, and runtime-pack tests still pass.

### 10. What is the rollback or disable path?

Revert this PR. Phase 8E live route signals remain recoverable by reverting back to the direct loop wiring. No persisted state or migration is introduced.

### 11. What does it deliberately not own?

No Playwright CLI/MCP execution switch, no provider promotion, no settings/UI/IPC, no DB migration, no hosted provider, no raw provider tools, and no harness-score ranking.

## Allowed Files

- `src-tauri/src/browser/provider_execution.rs`
- `src-tauri/src/browser/provider_execution_tests.rs`
- `src-tauri/src/browser/agent_loop.rs`
- `src-tauri/src/browser/mod.rs`
- `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`
- `docs/superpowers/plans/2026-05-24-browser-runtime-phase8f-provider-execution-boundary.md`

## Non-Goals

- No CLI/MCP execution adapter wiring into live tasks.
- No runtime pack mutation.
- No UI, IPC, Settings, DB, or migration work.
- No provider default selection promotion.
- No change to browser task decision prompts or action schema.

## Impact Targets

Pre-edit GitNexus impact:

- `run` in `src-tauri/src/browser/agent_loop.rs` - LOW.
- `BrowserAgentLoop` struct in `src-tauri/src/browser/agent_loop.rs` - LOW.
- `provider_selection_request_for_action` - LOW.
- `route_live_browser_action_provider` - LOW.
- `provider_route_blocks_local_action` - LOW.
- `provider_route_blocked_step` - LOW.
- `browser/mod.rs` file target was not resolved by GitNexus; the planned edit is an additive module export only.

## Rollback

Revert the PR commit. The prior Phase 8E direct live route guard is restored by git history; no cleanup command or data migration is required.

## Verification

Expected focused checks:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::agent_loop
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_pack
rustfmt --edition 2021 --check src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs
git diff --check -- src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/browser/mod.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md docs/superpowers/plans/2026-05-24-browser-runtime-phase8f-provider-execution-boundary.md
```

`src-tauri/src/browser/mod.rs` is excluded from the `rustfmt` file check
because rustfmt follows the module tree from `mod.rs` and rewrites unrelated
legacy browser files. Phase 8F keeps `mod.rs` to one additive module export and
uses `git diff --check` to verify whitespace on that export.

Before commit:

```bash
npx gitnexus detect-changes --scope staged
```
