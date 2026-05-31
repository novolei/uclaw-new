# Subprocess/RPC Plugin "Last Mile" Design (Slice 1 MVP)

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Strategic baseline:** `docs/adr/2026-05-28-uclaw-pi-lightweight-product-philosophy.md` (third-party code plugins via subprocess/RPC, MCP generalized); gap-audit blind-spot ② (the last remaining open debt after the 2026-05-31 refresh — "pluggability last mile").

## Problem

The philosophy ADR chose **subprocess/RPC** as the third-party plugin mechanism (Rust can't do Pi's jiti runtime `.ts` loading). Recon shows the transport + most of the loader already exist but are **built-but-unwired**:

- **MCP IS the subprocess/RPC mechanism**: `McpServerConfig { command, args, env, allowed_tools }` → `spawn` (mcp.rs:671, stdio JSON-RPC 2.0) → `tools/list` → `create_tool_proxies` registers the subprocess's tools into the agent. `McpManager::add_server` (mcp.rs:2065) adds a server config (sync); the existing async MCP connect spawns it.
- The **plugin loader is built + tested**: `plugins/discovery.rs` (`PluginDiscovery::discover` scans `$DATA_DIR/plugins/<id>/plugin.toml` → `LoadedPlugin`), `plugins/registration.rs` (`PluginRegistrar::register(api, &loaded)`), `plugins/lifecycle.rs` (`PluginLifecycleOwner::connect_and_register(&mut AgentApi)` = end-to-end discover→register), `plugins/uclaw_extension.rs` (the `uclaw` MCP-capability handshake), `AgentApi::register_plugin(id, set)` (api/mod.rs:261).
- **But:** (a) `connect_and_register` is **never called at boot** (grep of app.rs/main.rs empty); (b) `PluginRegistrar::register` for `mcp_servers` only **records the name** — "full McpManager wiring deferred to later tasks" (registration.rs:8/81). So a plugin's declared subprocess MCP server is never spawned.

So the "last mile" is **not a new RPC system** — it's wiring the existing loader into boot + completing the documented-deferred `mcp_servers` → `McpManager` spawn, reusing MCP entirely.

## Goal (Slice 1 MVP)

Make one real plugin work end-to-end: drop a plugin (manifest + a stdio MCP server) into `$DATA_DIR/plugins/` → at boot it is discovered → permission-gated → its MCP server subprocess is spawned via the existing MCP path → its tools register → the agent calls one. Reuses MCP; zero new RPC/spawn code.

## Design

### 1. Boot wiring (sync `connect_and_register` in `AppState::new`)

`McpManager::add_server` is **sync** (adds a config; the existing async MCP connect spawns it), so no async-in-sync-boot problem.

```
AppState::new (sync):
  build AgentApi (builtin_descriptors::register_all runs here)
  build McpManager (app.rs:602)
  let report = PluginLifecycleOwner::new(data_dir.join("plugins"))
                  .connect_and_register(&mut agent_api);   // sync; registers tool descriptors
  for cfg in report.plugin_mcp_configs { let _ = mcp_manager.add_server(cfg); }  // sync add
  // existing async MCP connect (boot) spawns all configured servers, incl. plugin ones
  // → tools/list → create_tool_proxies → plugin subprocess tools register
```

- `connect_and_register` stays **sync** + `&mut AgentApi` (slots next to `register_all`, before the Arc-seal). It registers the plugins' tool descriptors and **returns** the `McpServerConfig`s (it does NOT touch `McpManager` — `AppState::new` adds them). This keeps `PluginRegistrar` decoupled from `McpManager`.
- Plugins dir = `$DATA_DIR/plugins/` (discovery's documented convention). Missing dir → empty report → boot unaffected (existing test covers this).

### 2. `PluginRegistrar` — manifest → `McpServerConfig` + permission gate

Replace registration.rs's "record the name" for `mcp_servers` with config construction:

```
for each plugin declaring mcp_servers:
  PERMISSION GATE: require manifest.permissions.run_subprocess == true
    else → skip + tracing::warn(...) + report.permission_skipped.push(plugin_id)
  build McpServerConfig {
    id: plugin_id,                       // (or the mcp_servers entry)
    command: manifest.executable,        // the plugin subprocess = the MCP server
    args: manifest.args,
    env: manifest env (if any),
    allowed_tools: (if contribution.tools non-empty → that whitelist; else None),
    ...other McpServerConfig fields default,
  }
  // working_dir: resolve manifest.working_dir relative to LoadedPlugin.plugin_dir → absolute
  report.plugin_mcp_configs.push(config)
```

- **Permission is the MVP gate**: `run_subprocess` is the prerequisite to spawn; absent → no spawn (recorded + warned). Full OS-level sandbox (network/fs/memory isolation) is **deferred** (Slice 3) — this slice does declaration-gate + record + log only.
- `executable`/`args`/`working_dir` are existing manifest fields; `contribution.tools` → `allowed_tools` (reuse `McpServerConfig.allowed_tools`).
- `PluginRegistrar` still does NOT touch `McpManager` (produces configs only) — preserves the §1 decoupling. `PluginLifecycleReport` gains `plugin_mcp_configs: Vec<McpServerConfig>` + `permission_skipped: Vec<String>`.

### 3. End-to-end example plugin + tests

**Deliverable plugin** `examples/plugins/hello-uclaw/`:
- `plugin.toml` — manifest declaring `executable` + `args` + `permissions.run_subprocess = true` + `contribution.mcp_servers`/`tools`.
- a minimal stdio MCP server script (~30 lines, JSON-RPC 2.0: `initialize` + `tools/list` exposing one `hello` tool + `tools/call` echoing). Language: whichever is reliably present (a `node`/`python3` script).
- A README snippet: copy to `$DATA_DIR/plugins/` → restart → the agent sees + calls `hello`. Live evidence of the declare→discover→spawn→RPC-register→call loop.

## Data flow

```
$DATA_DIR/plugins/hello-uclaw/plugin.toml  (executable + run_subprocess + mcp_servers/tools)
  → boot: PluginDiscovery.discover() → LoadedPlugin
  → PluginRegistrar.register: permission-gate(run_subprocess) → McpServerConfig (from executable/args/working_dir)
  → report.plugin_mcp_configs → AppState::new: mcp_manager.add_server(cfg)
  → existing async MCP connect → spawn subprocess → initialize (uclaw capability negotiated) → tools/list
  → create_tool_proxies → `hello` registered in the agent tool registry
  → agent calls `hello` → JSON-RPC tools/call → subprocess → result
```

## Error handling

Best-effort, boot-safe: missing plugins dir → empty report (no-op). A malformed manifest → discovery error recorded in the report, other plugins still load. A permission-failed plugin → skipped + warned, not fatal. `add_server` failure → logged, boot continues. A plugin's subprocess failing to spawn is handled by the existing MCP connect error path (the rest of the app is unaffected).

## Testing

1. **Unit (registrar):** a fixture `LoadedPlugin` with `run_subprocess=true` + executable/args/tools → `register` produces a correct `McpServerConfig` (command/args/`allowed_tools`/working_dir resolved to `plugin_dir`); `run_subprocess=false` → no config + `permission_skipped` records the id.
2. **Unit (lifecycle):** `connect_and_register` on a temp plugins dir with a written `plugin.toml` fixture → report contains the plugin + its mcp config; missing dir → empty report (existing test stays green).
3. **Boot smoke:** `AppState::new` with absent/empty `$DATA_DIR/plugins/` does not break boot (no-op).
4. **Integration (gated):** with `node`/`python3` present, run the `examples/plugins/hello-uclaw` server as a real subprocess via the spawn path → `tools/list` → assert `hello` registers. Gate on the runtime's presence (skip otherwise — CI-safe, like the prior python3-gated tests).
5. `cargo test --lib plugins` + `agent::api` net green (only the 2 known pre-existing failures); clippy clean; `Cargo.toml` unchanged.

## Scope / files

| File | Change |
|---|---|
| `plugins/registration.rs` | `mcp_servers` → `McpServerConfig` (from manifest) + `run_subprocess` permission gate + `allowed_tools` whitelist; report fields |
| `plugins/lifecycle.rs` | `PluginLifecycleReport` carries `plugin_mcp_configs` + `permission_skipped` |
| `app.rs` | call `connect_and_register(&mut api)` in `AppState::new`; feed `plugin_mcp_configs` to `mcp_manager.add_server` |
| `examples/plugins/hello-uclaw/` | **new** — `plugin.toml` + minimal stdio MCP server + README |
| tests | registrar/lifecycle unit + boot smoke + gated integration |

**Out of scope (deferred slices):** skills/commands/themes contribution wiring (currently "recorded"); OS-level permission **sandbox** enforcement; install-from-registry/zip/git; enable/disable/reload lifecycle; plugin-management UI.

## Risk

Medium. Reuses MCP's proven config→spawn (zero new RPC/transport code); the new code is the registrar's config construction + one `AppState::new` block + the example plugin. The boot wiring touches `AppState::new` (hot path) but is additive and no-ops when the plugins dir is absent. `PluginRegistrar` stays decoupled from `McpManager`. One branch, bisectable commits (registrar → lifecycle report → boot wiring → example plugin + gated integration test → verify).
