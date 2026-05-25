# Browser Runtime Control Center PR4 Execution Promotion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make real browser task routing consume Control Center provider config and probe gates so active route, not hard-coded safe defaults, decides CLI-first/MCP-second/Local-Chromium-fallback execution.

**Architecture:** Extend `BrowserRuntimeStatusService` to read config-backed feature flags and active route. Pass route options into `BrowserRuntimeActionExecutor` and `BrowserProviderActionExecutor`. CLI can execute through the existing provider adapter when active; MCP remains guarded and blocked unless the existing MCP action adapter is wired with the same wrapped-action boundary. Every route decision records selected and skipped providers in existing route artifacts/events.

**Tech Stack:** Rust async browser runtime, existing provider router/executor, BrowserRuntimeStatusService, serde tests, focused Rust tests.

---

## File Structure

| Path | Responsibility |
| --- | --- |
| `src-tauri/src/browser/runtime_status.rs` | Add config-aware service inspection used by task execution. |
| `src-tauri/src/browser/runtime_execution.rs` | Consume Control Center active route and provider config. |
| `src-tauri/src/browser/provider_execution.rs` | Respect desired priority/probe gates in provider route options and route artifacts. |
| `src-tauri/src/browser/provider_execution_tests.rs` | Cover CLI-selected execution and fallback when probes fail. |
| `src-tauri/src/browser/tools.rs` | Pass config-aware runtime status service into direct tools without changing public tool schemas. |
| `src-tauri/src/browser/agent_loop.rs` | Preserve thin orchestration while using config-aware executor construction. |
| `src-tauri/src/tauri_commands.rs` | Touch only shim sites that construct browser tool/executor state. |
| `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md` | Track route promotion completion and risk gates. |

## Boundaries

- This PR changes real browser execution routing and needs fresh review before merge.
- This PR does not expose raw MCP tools.
- This PR does not remove Local Chromium fallback.
- This PR does not widen browser action schemas.
- This PR does not add new UI beyond route evidence already exposed by earlier PRs.

## ADR 18 Answers

1. Intent: browser actions use the provider order the user selected once probes prove a provider is routable.
2. Autonomy: task-time browser action execution through supervised provider lanes.
3. Truth source: Control Center active route derived by Rust from config, runtime-pack status, and probe cache.
4. TaskEvent: selected/skipped provider route evidence is emitted through existing provider route artifacts/events.
5. Context: reads provider config, probe cache, runtime-pack status, and current browser action request.
6. Capability: promotes CLI first, MCP second, Local Chromium fallback.
7. Hooks: probe gates, provider route decision, GitNexus impact, focused Rust tests, fresh reviewer.
8. Projection: task artifacts show selected provider and skipped providers.
9. Harness: provider execution tests and browser task smoke cover CLI selected, probe failed fallback, Local Chromium fallback.
10. Rollback: disable CLI/MCP in provider config or revert PR; Local Chromium remains fallback.
11. Non-ownership: no raw MCP tool exposure, no MCP settings redesign, no unrelated agent-loop refactor.

### Task 1: Make Runtime Status Service Config-Aware

**Files:**
- Modify: `src-tauri/src/browser/runtime_status.rs`
- Modify: `src-tauri/src/browser/runtime_control_center.rs`

- [ ] **Step 1: Write service inspection test**

Add:

```rust
#[test]
fn config_aware_status_enables_cli_feature_flag_in_provider_readiness() {
    let temp_dir = tempfile::tempdir().expect("temp dir");
    let runtime_pack = fixture_runtime_pack_status(temp_dir.path(), true);
    let mut config = BrowserRuntimeProviderConfig::default();
    config.playwright_cli_enabled = true;

    let report = compose_browser_runtime_status_with_config(runtime_pack, Vec::new(), config);

    assert!(report.control_center.feature_flags.playwright_cli);
    assert_eq!(
        report.provider_readiness.playwright_cli.setup_checks[0].id,
        "playwright_cli_feature_flag"
    );
    assert_eq!(
        report.provider_readiness.playwright_cli.setup_checks[1].id,
        "runtime_pack_ready"
    );
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status::tests::config_aware_status_enables_cli_feature_flag_in_provider_readiness`

Expected: FAIL if runtime status still uses `safe_defaults()` internally.

- [ ] **Step 3: Implement config-aware service method**

Add to `BrowserRuntimeStatusService`:

```rust
pub async fn inspect_with_provider_config(
    &self,
    provider_config: crate::browser::runtime_control_center::BrowserRuntimeProviderConfig,
) -> Result<BrowserRuntimeStatusReport, Error> {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_uclaw_home(&manifest)?;
    let runtime_pack = inspect_runtime_pack_status(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
        BrowserRuntimePackStatusRequest {
            trigger: BrowserRuntimePackPlanTrigger::Settings,
            network_state: BrowserRuntimePackNetworkState::Online,
            auto_prepare_enabled: true,
            user_confirmed: false,
        },
    );
    let active_context_sessions = self.context_manager.list_active_sessions().await;
    Ok(compose_browser_runtime_status_with_config(
        runtime_pack,
        active_context_sessions,
        provider_config,
    ))
}
```

Keep `inspect_default()` as the compatibility path using default config.

- [ ] **Step 4: Run status tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/browser/runtime_status.rs src-tauri/src/browser/runtime_control_center.rs
git commit -m "feat(browser-runtime): inspect status with provider config" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center (expected PASS)"
```

### Task 2: Feed Active Route into Provider Execution

**Files:**
- Modify: `src-tauri/src/browser/runtime_execution.rs`
- Modify: `src-tauri/src/browser/provider_execution.rs`
- Modify: `src-tauri/src/browser/provider_execution_tests.rs`

- [ ] **Step 1: Write route execution tests**

Add:

```rust
#[test]
fn route_options_include_control_center_active_route() {
    let status = status_with_active_route("browser.playwright_cli");
    let options = route_options_from_runtime_status(status);

    assert_eq!(options.active_provider_id.as_deref(), Some("browser.playwright_cli"));
    assert!(options.disabled_provider_ids.iter().all(|id| id != "browser.playwright_cli"));
}

#[test]
fn failed_cli_probe_keeps_local_chromium_active_for_execution() {
    let status = status_with_cli_enabled_failed_probe();
    let options = route_options_from_runtime_status(status);

    assert_eq!(options.active_provider_id.as_deref(), Some("browser.local_chromium"));
    assert!(options.skipped_provider_reasons.iter().any(|item| {
        item.provider_id == "browser.playwright_cli" && item.reason == "probe_failed"
    }));
}
```

- [ ] **Step 2: Run failing tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests
```

Expected: FAIL because route options do not contain active provider evidence.

- [ ] **Step 3: Extend route options**

In `provider_execution.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProviderSkippedReason {
    pub provider_id: String,
    pub reason: String,
}

pub struct BrowserProviderActionRouteOptions {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub runtime_report: Option<BrowserRuntimePackStatusReport>,
    pub disabled_provider_ids: Vec<String>,
    pub active_provider_id: Option<String>,
    pub skipped_provider_reasons: Vec<BrowserProviderSkippedReason>,
}
```

Add builder:

```rust
pub fn with_active_control_center_route(
    mut self,
    active_provider_id: impl Into<String>,
    skipped: Vec<BrowserProviderSkippedReason>,
) -> Self {
    self.active_provider_id = Some(active_provider_id.into());
    self.skipped_provider_reasons = skipped;
    self
}
```

In `runtime_execution.rs`, update `route_options_from_runtime_status`:

```rust
let skipped = status
    .control_center
    .provider_lanes
    .iter()
    .filter_map(|lane| {
        lane.fallback_reason.as_ref().map(|reason| BrowserProviderSkippedReason {
            provider_id: lane.provider_id.clone(),
            reason: reason.clone(),
        })
    })
    .collect();

BrowserProviderActionRouteOptions::default()
    .with_runtime_report(status.runtime_pack)
    .with_active_control_center_route(status.control_center.active_provider_route.provider_id, skipped)
```

- [ ] **Step 4: Respect active provider in routing**

In `route_live_browser_action_provider_with_options`, force the selection request to prefer `options.active_provider_id` when it is present. If active route is Local Chromium, keep existing fallback behavior. If active route is CLI, allow CLI route only for actions supported by `playwright_cli_action_for_browser_action`; unsupported actions block with a clear message and do not silently bypass the active route.

- [ ] **Step 5: Run execution tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/runtime_execution.rs src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/provider_execution_tests.rs
git commit -m "feat(browser-runtime): route execution from control center" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests (expected PASS)"
```

### Task 3: Pass Config-Aware Runtime Status into Task-Time Tools

**Files:**
- Modify: `src-tauri/src/browser/tools.rs`
- Modify: `src-tauri/src/browser/agent_loop.rs`
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: Run GitNexus impact**

Run:

```bash
npx gitnexus impact BrowserRuntimeActionExecutor --direction upstream
npx gitnexus impact BrowserRuntimeStatusService --direction upstream
```

Expected: record risk level and direct callers in the PR notes. Stop and ask for explicit user approval if HIGH/CRITICAL appears and has not already been authorized.

- [ ] **Step 2: Write thin-orchestration tests**

Add focused tests to existing browser tool/runtime execution test files:

```rust
#[tokio::test]
async fn direct_browser_tool_uses_config_backed_runtime_status_when_available() {
    let route_options = direct_browser_tool_route_options_from_status(status_with_active_route("browser.playwright_cli"));

    assert_eq!(route_options.active_provider_id.as_deref(), Some("browser.playwright_cli"));
}
```

- [ ] **Step 3: Run failing tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
```

Expected: FAIL until helpers consume config-aware status.

- [ ] **Step 4: Implement config-aware helper**

When an `AppState` is available, read:

```rust
let provider_config = state.settings.read().await.browser_runtime_provider_config.clone();
let runtime_status = state
    .browser_runtime_status_service
    .inspect_with_provider_config(provider_config)
    .await?;
```

Keep IPC functions thin: construct config-aware runtime status or executor and delegate to existing modules. Do not add browser routing business logic in `tauri_commands.rs`.

- [ ] **Step 5: Run Rust tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/tools.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(browser-runtime): use config-aware routing in browser tools" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests (expected PASS)"
```

### Task 4: Route Evidence and Tracker Update

**Files:**
- Modify: `src-tauri/src/browser/rollout_bridge.rs`
- Modify: `src-tauri/src/browser/rollout_bridge_tests.rs`
- Modify: `docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`

- [ ] **Step 1: Write route evidence test**

Add:

```rust
#[test]
fn route_artifact_includes_skipped_provider_reasons() {
    let artifact = route_artifact_from_decision_and_skips(
        decision_for_selected_provider("browser.local_chromium"),
        vec![("browser.playwright_cli", "probe_failed")],
    );

    assert!(artifact.summary.contains("browser.local_chromium"));
    assert!(artifact.summary.contains("browser.playwright_cli"));
    assert!(artifact.summary.contains("probe_failed"));
}
```

- [ ] **Step 2: Run failing test**

Run: `cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge::tests`

Expected: FAIL until skipped-provider evidence is serialized.

- [ ] **Step 3: Add skipped-provider evidence**

Extend route artifact construction to include:

```json
{
  "selectedProviderId": "browser.local_chromium",
  "skippedProviders": [
    { "providerId": "browser.playwright_cli", "reason": "probe_failed" }
  ]
}
```

- [ ] **Step 4: Update tracker**

In `BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md`, add one row stating PR4 promotes execution routing to consume Control Center active route with CLI/MCP probe gates and Local Chromium fallback.

- [ ] **Step 5: Run verification**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests
git diff --check -- src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
```

Expected: PASS and no whitespace errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md
git commit -m "feat(browser-runtime): record control center route evidence" -m "Verification: cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge::tests; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution; cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests; git diff --check -- src-tauri/src/browser/rollout_bridge.rs src-tauri/src/browser/rollout_bridge_tests.rs docs/superpowers/BROWSER_RUNTIME_SUPERVISOR_UPGRADE_STATUS.md (expected PASS)"
```

## Final Verification

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_status
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_control_center
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::runtime_execution
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::rollout_bridge::tests
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::tools
rustfmt --edition 2021 --check src-tauri/src/browser/runtime_status.rs src-tauri/src/browser/runtime_control_center.rs src-tauri/src/browser/runtime_execution.rs src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/tools.rs src-tauri/src/browser/agent_loop.rs src-tauri/src/tauri_commands.rs
npx gitnexus detect-changes
git diff --check
```

Expected:

- Rust tests PASS.
- Rustfmt exits 0.
- GitNexus detect shows expected browser-runtime/provider execution symbols only.
- `git diff --check` exits 0.
- Fresh reviewer is requested before merge because this PR changes real browser execution routing.
