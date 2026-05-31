# PluginRuntime Module TDD Plan

**Goal:** Deepen the plugin seam so callers receive approved MCP configs,
preflight evidence, status, and kill semantics from one Module. Reuse the
existing MCP subprocess/RPC Adapter.

**Spec:** `docs/superpowers/specs/2026-06-01-plugin-runtime-design.md`
**Related design:** `docs/superpowers/specs/2026-05-31-subprocess-rpc-plugin-last-mile-design.md`
**References:** `/Users/ryanliu/Documents/pi_agent_rust/src/extensions.rs`,
`/Users/ryanliu/Documents/pi_agent_rust/src/extension_preflight.rs`,
`/Users/ryanliu/Documents/pi/packages/agent/src/agent.ts`

## Reconciliation With Last-Mile Spec

- The last-mile spec's strategic choice still stands: MCP is the plugin
  subprocess/RPC Adapter.
- Current branch truth differs from the old recon: `AppState::new` already
  invokes `PluginLifecycleOwner::connect_and_register(&mut api)`, so this plan
  keeps that boot seam and adds approved MCP config consumption.
- The old plan wanted `PluginRegistrationSummary.mcp_configs` and
  `PluginLifecycleReport::plugin_mcp_configs()`. This plan keeps those but adds
  preflight/status/kill evidence so the Module is deep rather than a config
  mapper.
- `McpServerConfig` has no working-directory field. Relative executables are
  resolved under `LoadedPlugin.plugin_dir`; `runtime.working_dir` is recorded in
  preflight/status notes but not passed to MCP in this slice.

## Current Code Truth

- `PluginDiscovery::discover()` is pure scan/parse and already returns
  per-plugin results.
- `PluginRegistrar::register()` registers plugin tools via
  `McpToolProxy::for_plugin(...)`, records commands, and currently only records
  `mcp_servers`.
- `PluginLifecycleReport` contains `loaded`, `discovery_errors`, and
  `registration_errors`.
- `AppState::new` creates `mcp_manager` before building `agent_api`; the
  `agent_api` boot block calls `connect_and_register`.
- `McpManager::add_server` is sync and takes `McpServerConfig`; the existing
  async MCP connect path owns spawning.

## GitNexus Impact Targets

Run before production edits:

- `PluginRegistrar::register` (GitNexus returned UNKNOWN: symbol not indexed after `npx gitnexus analyze`)
- `PluginLifecycleOwner::connect_and_register` (GitNexus returned UNKNOWN: symbol not indexed after `npx gitnexus analyze`)
- `PluginLifecycleReport` (GitNexus returned UNKNOWN: symbol not indexed after `npx gitnexus analyze`)
- `AppState::new` (LOW; 0 direct callers, 0 affected processes)
- New preflight/status symbols do not need impact before creation.

If any impact is HIGH or CRITICAL, record the blast radius here before editing.

## Tasks

### Task 1: Preflight/status model

**Files:** `src-tauri/src/plugins/lifecycle.rs` or a new focused
`src-tauri/src/plugins/runtime.rs`; `src-tauri/src/plugins/mod.rs`.

- [x] Add failing tests for:
  - Valid subprocess plugin preflight passes.
  - Missing `run_subprocess` fails.
  - Missing executable fails.
  - Unsupported `runtime.kind` fails.
- [x] Implement:
  - `PluginPreflightVerdict::{Pass, Warn, Fail}`.
  - `PluginPreflightFinding` with severity/category/message.
  - `PluginPreflightReport::from_findings(plugin_id, findings)`.
  - `preflight_plugin(&LoadedPlugin) -> PluginPreflightReport`.
- [x] Verify:
  - `cargo test --lib plugins -- --nocapture`

### Task 2: Registrar builds approved MCP configs

**Files:** `src-tauri/src/plugins/registration.rs`.

- [x] Write failing tests:
  - `register_builds_mcp_config_when_preflight_passes`.
  - `register_skips_mcp_when_run_subprocess_denied`.
  - `register_skips_mcp_when_no_executable`.
  - `register_skips_mcp_when_runtime_kind_unsupported`.
- [x] Implement:
  - `PluginRegistrationSummary.mcp_configs: Vec<McpServerConfig>`.
  - `PluginRegistrationSummary.permission_skipped: Vec<String>`.
  - `PluginRegistrationSummary.preflight: Option<PluginPreflightReport>`.
  - Config construction from manifest runtime fields:
    - `id = manifest.id`
    - `name = manifest.display_name`
    - `description = manifest.description.unwrap_or_default()`
    - `transport_type = TransportType::Stdio`
    - `command = absolute executable path`
    - `args = manifest.runtime.args`
    - `enabled = true`
    - `auto_approve = false`
    - `tool_allowlist = Some(contributes.tools)` when non-empty.
- [x] Preserve existing tool descriptor registration.
- [x] Verify:
  - `cargo test --lib plugins::tests -- --nocapture`

### Task 3: Lifecycle report aggregation and kill semantics

**Files:** `src-tauri/src/plugins/lifecycle.rs`, possible new
`src-tauri/src/plugins/runtime.rs`.

- [x] Write failing tests:
  - Lifecycle aggregates one approved MCP config from a temp plugin dir.
  - Missing plugins dir stays an empty success report.
  - Killed plugin status prevents config contribution.
- [x] Implement:
  - `PluginRuntimeState` or owner-local killed set.
  - `PluginTrustState::{Pending, Acknowledged, Trusted, Killed}`.
  - `PluginRuntimeStatus` with plugin id, trust state, status label, and reason.
  - `PluginLifecycleReport::plugin_mcp_configs()`.
  - `PluginLifecycleOwner::with_killed_plugins(root, ids)` test constructor or
    equivalent non-persistent kill input for this slice.
- [x] Verify:
  - `cargo test --lib plugins -- --nocapture`

### Task 4: Boot consumes approved MCP configs

**Files:** `src-tauri/src/app.rs`.

- [x] Run GitNexus impact for `AppState::new` before editing.
- [x] Add approved plugin configs to `mcp_manager` after
  `connect_and_register(&mut api)` and before `Arc::new(api)` seals the
  `AgentApi`.
- [x] Log preflight/status evidence without inspecting manifest fields.
- [x] Keep boot no-op when `$DATA_DIR/plugins` is absent.
- [x] Verify:
  - `cargo test --lib plugins -- --nocapture`
  - `cargo build --lib`

### Task 5: Review and commit

- [x] Run placeholder scan:

```bash
rg -n "TODO|TBD|implement later|similar to|appropriate" \
  docs/superpowers/specs/2026-06-01-plugin-runtime-design.md \
  docs/superpowers/plans/2026-06-01-plugin-runtime.md | rg -v "rg -n"
```

- [x] Run `git diff --check`.
- [x] Run GitNexus `detect_changes(scope: "staged")` before commit.
- [x] Commit with verification command and expected output in the body.

## Execution Evidence

- RED: `cargo test --lib plugins -- --nocapture` failed before implementation with missing `PluginPreflightReport`, `PluginPreflightVerdict`, `PluginRuntimeStatusKind`, `PluginTrustState`, `mcp_configs`, `permission_skipped`, `preflight`, and `plugin_mcp_configs`.
- GREEN: `cargo test --lib plugins -- --nocapture` passed: 20 tests, 19 passed, 1 ignored.
- BUILD: `cargo build --lib` passed with existing warnings.
- DIFF: `git diff --check` passed.

## Review Notes

- The Module Interface is the report plus approved MCP configs; callers do not
  branch on manifest internals.
- The Adapter remains MCP; no new transport or process supervisor is introduced.
- Preflight is deliberately small but machine-readable and blocks unsafe launch.
- Kill semantics are boot-local in this slice and designed for later persistent
  trust storage.
