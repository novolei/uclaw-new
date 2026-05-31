# PluginRuntime Module Design

**Date:** 2026-06-01
**Status:** Child spec, implemented
**Parent spec:** `docs/superpowers/specs/2026-05-31-pi-modernization-six-modules-design.md`
**Related design:** `docs/superpowers/specs/2026-05-31-subprocess-rpc-plugin-last-mile-design.md`
**Pi references:** `/Users/ryanliu/Documents/pi_agent_rust/src/extensions.rs`, `/Users/ryanliu/Documents/pi_agent_rust/src/extension_preflight.rs`, `/Users/ryanliu/Documents/pi/packages/agent/src/agent.ts`

## Problem

The current plugin seam is still shallow. `PluginDiscovery` parses
`$DATA_DIR/plugins/<id>/plugin.toml`, `PluginRegistrar` registers declared
tools as `McpToolProxy` descriptors, and `PluginLifecycleOwner` reports
discovery and registration errors. That Interface is nearly as complex as its
Implementation because runtime decisions still leak to callers:

- `mcp_servers` are accounted for but do not produce owned MCP configs.
- Permission and compatibility checks are not represented as preflight
  findings.
- Runtime state has no stable vocabulary for loaded, skipped, failed, or
  killed plugins.
- There is no kill switch or audit trail at the plugin seam.

The last-mile subprocess/RPC spec correctly identifies MCP as the Adapter for
plugin subprocesses. This child spec deepens the same seam into a
`PluginRuntime` Module that owns preflight, manifest-to-MCP config
construction, lifecycle status, and kill semantics while still reusing
`McpManager` for transport.

## Goal

Make `PluginRuntime` the Deep Module at the plugin seam:

```text
AppState boot
  -> PluginRuntime::load(...)
       -> discovery
       -> preflight
       -> manifest contribution routing
       -> MCP config contribution
       -> status ledger
  -> AppState adds approved MCP configs to McpManager
```

Callers should not need to know which manifest fields imply a spawn, why a
plugin was skipped, or whether a plugin has been killed. They should receive a
small report and a list of approved `McpServerConfig` values.

## Current uClaw Truth

- `src-tauri/src/plugins/discovery.rs` is a pure scan-and-parse Module for
  plugin manifests.
- `src-tauri/src/plugins/registration.rs` routes tool contributions into
  `AgentApi`, records commands/skills/themes, and only records `mcp_servers`.
- `src-tauri/src/plugins/lifecycle.rs` owns discover-then-register but its
  report is accounting-only.
- `src-tauri/src/app.rs` already calls `PluginLifecycleOwner::new(...).connect_and_register(&mut api)`
  during `AgentApi` boot and logs the report.
- `src-tauri/src/mcp.rs` already has the subprocess/RPC Adapter:
  `McpServerConfig`, `McpManager::add_server`, `connect_server_shared`, stdio
  JSON-RPC, tool listing, health, reconnect, and audit.

## Pi Reference Truth

Pi Rust treats extensions as a runtime, not a manifest parser:

- `extension_preflight.rs` returns `PreflightReport` with a schema, verdict,
  confidence, risk banner, findings, and severity/category summaries.
- `extensions.rs` models `ExtensionPolicyMode`, `PolicyDecision`, trust states
  (`Pending`, `Acknowledged`, `Trusted`, `Killed`), kill-switch audit entries,
  and runtime-owned snapshots.
- Runtime handles expose shutdown, tool discovery, and dispatch through a small
  runtime Interface.

Pi TypeScript reinforces the same shape at the agent level: active runs own an
abort controller and runtime-owned state is cleared centrally rather than by
distributed callsites.

## uClaw Adaptation

Borrow the Interface shape and lifecycle vocabulary, not Pi's QuickJS/native
extension host. uClaw's plugin Adapter remains MCP subprocess/RPC.

`PluginRuntime` will be implemented in the existing `plugins/` area:

- `PluginPreflightReport` with a uClaw schema, verdict, findings, and summary.
- `PluginRuntimeStatus` and `PluginTrustState` for loaded/skipped/failed/killed
  plugins.
- `PluginRuntimeReport` or an expanded `PluginLifecycleReport` that exposes
  approved MCP configs and status evidence.
- Registrar logic that builds `McpServerConfig` values only after the preflight
  and permission gate pass.
- Kill semantics that skip config contribution and leave a machine-readable
  audit/status entry.

## Interface

The first implementation slice keeps the public call shape close to existing
code:

```rust
PluginLifecycleOwner::new(plugins_root)
    .connect_and_register(&mut api) -> PluginLifecycleReport
```

The report becomes the stable Interface:

- `loaded: Vec<PluginRegistrationSummary>`
- `discovery_errors: Vec<String>`
- `registration_errors: Vec<String>`
- `preflight_reports: Vec<PluginPreflightReport>`
- `runtime_statuses: Vec<PluginRuntimeStatus>`
- `plugin_mcp_configs() -> Vec<McpServerConfig>`

`AppState` consumes only `plugin_mcp_configs()` and report fields for logging.
It does not inspect manifest internals.

## Preflight Rules

The initial uClaw preflight is intentionally narrower than Pi's full analyzer:

- If `contributes.mcp_servers` is non-empty and `runtime.executable` is
  missing, verdict `Fail`.
- If `contributes.mcp_servers` is non-empty and
  `permissions.run_subprocess == false`, verdict `Fail`.
- If `runtime.kind` is present and is not `"subprocess"`, verdict `Fail`.
- If `runtime.executable` is relative, resolve it under `LoadedPlugin.plugin_dir`
  before constructing an MCP config.
- If the resolved executable does not exist, verdict `Warn` for the first slice:
  config can still be produced for test fixtures and developer setups, but the
  report records the launch risk before `McpManager` spawn.

This gives locality now while leaving room for later policy checks against
network/filesystem/memory permissions.

## Acceptance Evidence

- Tests prove a plugin with `run_subprocess=true`, `runtime.kind="subprocess"`,
  `runtime.executable`, `contributes.mcp_servers`, and tools produces one
  approved `McpServerConfig`.
- Tests prove missing `run_subprocess`, missing executable, or unsupported
  runtime kind create preflight failure and no MCP config.
- Tests prove a killed plugin creates a killed/skipped runtime status and no MCP
  config.
- Tests prove lifecycle aggregation exposes approved configs without callers
  inspecting manifest internals.
- `AppState` boot adds approved plugin MCP configs to `McpManager` through
  `add_server`, and missing plugins dir remains a no-op.
- Focused Rust tests pass for `plugins` and relevant `mcp` config helpers.

## Non-Goals

- Do not add a new plugin transport.
- Do not embed Pi's QuickJS/native runtime host.
- Do not implement install-from-registry, enable/disable UI, hot reload, or
  zip/git packaging in this slice.
- Do not enforce OS sandboxing beyond manifest preflight and MCP config gating.
- Do not move plugin trust state into persistent storage yet; the first slice is
  boot-local and report-backed.

## Rollback

Revert the PluginRuntime commits. Since approved subprocesses are still added
through existing `McpManager::add_server`, rollback restores the earlier
accounting-only lifecycle and does not alter the MCP transport or config file
format beyond plugin-added configs created during a boot.
