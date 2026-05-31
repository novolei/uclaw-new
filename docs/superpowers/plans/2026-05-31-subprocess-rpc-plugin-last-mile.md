# Subprocess/RPC Plugin Last-Mile MVP Implementation Plan (Slice 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make one real plugin work end-to-end — a plugin in `$DATA_DIR/plugins/` whose declared MCP server (subprocess) is spawned at boot via the existing MCP path, its tools reaching the agent. Wire the already-built plugin loader into boot + complete the documented-deferred `mcp_servers` → `McpServerConfig` construction. Zero new RPC/transport code (reuses MCP).

**Architecture:** `PluginRegistrar::register` builds an `McpServerConfig` from each plugin's manifest (permission-gated on `run_subprocess`), carried up through `PluginLifecycleReport`. `AppState::new` calls `connect_and_register(&mut agent_api)` (sync — registers tool descriptors, already done) and feeds the returned configs to `mcp_manager.add_server(...)` (sync); the existing async MCP connect spawns them. An example plugin proves the loop.

**Tech Stack:** Rust, existing MCP (`McpManager`/`McpServerConfig`), `plugins/` loader, `plugin_manifest` schema. No new deps. Spec: `docs/superpowers/specs/2026-05-31-subprocess-rpc-plugin-last-mile-design.md`.

---

## Source-of-truth references (verified)

- `mcp.rs`: `McpServerConfig { id: String, name: String, description: String, transport_type: TransportType (default Stdio), command: String, args: Vec<String>, env: HashMap<String,String>, url: Option<String>, enabled: bool, auto_approve: bool, tool_allowlist: Option<Vec<String>> }` (531-558). `McpManager::add_server(&mut self, config) -> Result<(), String>` (2065, **sync**). `prefixed_tool_name(server, tool)`.
- `plugins/registration.rs`: `PluginRegistrar::register(api: &mut AgentApi, loaded: &LoadedPlugin) -> Result<PluginRegistrationSummary, RegistrationError>` (39). **Tools already register** via `McpToolProxy::for_plugin(plugin_id, tool, ctx.app_state.mcp_manager)` (54-73). `mcp_servers` loop (81-83) only `summary.mcp_servers_registered.push(name)` — **this is the gap to complete**.
- `plugins/discovery.rs`: `LoadedPlugin { manifest: PluginManifest, plugin_dir: PathBuf, manifest_path: PathBuf }`. Scans `$DATA_DIR/plugins/<id>/plugin.toml`.
- `plugin_manifest/schema.rs`: `PluginManifest { id, version, display_name, description: Option<String>, author, runtime: PluginRuntimeRequirement, permissions: PluginPermissions, contributes: PluginContribution }`. `PluginRuntimeRequirement { min_uclaw_version, kind: Option<String>, executable: Option<String>, args: Vec<String>, working_dir: Option<String> }`. `PluginPermissions { network, filesystem_read, filesystem_write, memory_read, memory_write, run_subprocess, additional }`. `PluginContribution { mcp_servers, skills, commands, tools, themes }`.
- `plugins/lifecycle.rs`: `PluginLifecycleOwner::new(root)` + `connect_and_register(&self, api: &mut AgentApi) -> PluginLifecycleReport` (25); `PluginLifecycleReport { plugins_root, loaded: Vec<PluginRegistrationSummary>, discovery_errors, registration_errors, .. }`.
- `app.rs`: `AppState::new` builds `mcp_manager = Arc::new(RwLock::new(McpManager::new(&data_dir)))` (602); `builtin_descriptors::register_all(&mut agent_api)` runs at boot before the Arc-seal (recon the exact agent_api var + where it's mutable).

---

## CRITICAL facts

1. **Tools already wired** (McpToolProxy). The ONLY registrar gap is `mcp_servers` → `McpServerConfig`. Don't re-do tool registration.
2. **`McpServerConfig` has no `working_dir`** — resolve `manifest.runtime.executable` to an **absolute path** under `loaded.plugin_dir` when it's relative (so cwd doesn't matter). `working_dir` honoring is best-effort/deferred (note it).
3. **Permission gate**: only build a config when `manifest.permissions.run_subprocess == true`; else skip + warn + record in `permission_skipped`.
4. **`add_server` is sync** — call it from `AppState::new` after `connect_and_register`; the existing async MCP connect spawns. Missing plugins dir → empty report → boot unaffected.
5. **Registrar stays decoupled from `McpManager`** — it produces `McpServerConfig`s (in the summary); `AppState::new` adds them.
6. **Pre-commit hooks** — no `--no-verify`. `$DATA_DIR` via `uclaw_utils_home`, not `dirs::home_dir`.

---

## File Structure

| File | Change |
|---|---|
| `plugins/registration.rs` | `mcp_servers` → `McpServerConfig` (from `manifest.runtime`, abs executable, `tool_allowlist`) + `run_subprocess` gate; `PluginRegistrationSummary` gains `mcp_configs: Vec<McpServerConfig>` + `permission_skipped: Vec<String>` |
| `plugins/lifecycle.rs` | `PluginLifecycleReport` exposes aggregated `plugin_mcp_configs()` (or a field) |
| `app.rs` | `AppState::new`: `connect_and_register(&mut agent_api)` + `for cfg in configs { mcp_manager add_server }` |
| `examples/plugins/hello-uclaw/` | **new** — `plugin.toml` + minimal stdio MCP server + README |
| tests | registrar/lifecycle unit + boot smoke + gated integration |

---

## Tasks

### Task 1: registrar — `mcp_servers` → `McpServerConfig` + permission gate

**Files:** `plugins/registration.rs`.

- [ ] **Step 1: Add summary fields.** In `PluginRegistrationSummary`, add:
```rust
    pub mcp_configs: Vec<crate::mcp::McpServerConfig>,
    pub permission_skipped: Vec<String>,
```
(Check the struct's derives — if it derives `Default`, the new `Vec` fields default fine. If `McpServerConfig` isn't `Default`, that's OK — the Vec is empty by default.)

- [ ] **Step 2: Write failing tests** (registration.rs tests — mirror existing `LoadedPlugin` fixture construction; build a `PluginManifest` with `permissions.run_subprocess` + `runtime.executable`/`args` + `contributes.mcp_servers`/`tools`):
```rust
#[test]
fn register_builds_mcp_config_when_run_subprocess_granted() {
    let loaded = fixture_plugin(/* run_subprocess */ true, Some("server.js"), vec!["--flag".into()],
                                /* mcp_servers */ vec!["hello".into()], /* tools */ vec!["greet".into()]);
    let mut api = AgentApi::new();
    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
    assert_eq!(summary.mcp_configs.len(), 1);
    let cfg = &summary.mcp_configs[0];
    assert_eq!(cfg.id, loaded.manifest.id);
    assert!(cfg.command.ends_with("server.js"));           // resolved (abs under plugin_dir)
    assert!(std::path::Path::new(&cfg.command).is_absolute());
    assert_eq!(cfg.args, vec!["--flag".to_string()]);
    assert_eq!(cfg.tool_allowlist, Some(vec!["greet".to_string()])); // from contributes.tools
    assert!(cfg.enabled);
    assert!(summary.permission_skipped.is_empty());
}
#[test]
fn register_skips_mcp_when_run_subprocess_denied() {
    let loaded = fixture_plugin(false, Some("server.js"), vec![], vec!["hello".into()], vec![]);
    let mut api = AgentApi::new();
    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
    assert!(summary.mcp_configs.is_empty());
    assert_eq!(summary.permission_skipped, vec![loaded.manifest.id.clone()]);
}
#[test]
fn register_skips_mcp_when_no_executable() {
    let loaded = fixture_plugin(true, None, vec![], vec!["hello".into()], vec![]);
    let mut api = AgentApi::new();
    let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
    assert!(summary.mcp_configs.is_empty()); // no executable → can't spawn
}
```
(Write a `fixture_plugin(run_subprocess, executable, args, mcp_servers, tools) -> LoadedPlugin` test helper constructing a full `PluginManifest` + a `plugin_dir` of e.g. `/tmp/plug`. Match the exact `PluginManifest`/`PluginRuntimeRequirement`/`PluginPermissions`/`PluginContribution` field set.)

- [ ] **Step 3: Implement** — replace the `mcp_servers` loop (registration.rs ~81-83) with:
```rust
        // mcp_servers — build a real McpServerConfig per plugin (permission-gated).
        // The plugin's runtime.executable IS the stdio MCP server subprocess.
        if !contrib.mcp_servers.is_empty() {
            let perms = &loaded.manifest.permissions;
            match (&loaded.manifest.runtime.executable, perms.run_subprocess) {
                (Some(exe), true) => {
                    // Resolve a relative executable under the plugin dir (abs → no cwd dependence).
                    let exe_path = std::path::Path::new(exe);
                    let command = if exe_path.is_absolute() {
                        exe.clone()
                    } else {
                        loaded.plugin_dir.join(exe_path).to_string_lossy().to_string()
                    };
                    let tool_allowlist = if contrib.tools.is_empty() {
                        None
                    } else {
                        Some(contrib.tools.clone())
                    };
                    let cfg = crate::mcp::McpServerConfig {
                        id: loaded.manifest.id.clone(),
                        name: loaded.manifest.display_name.clone(),
                        description: loaded.manifest.description.clone().unwrap_or_default(),
                        transport_type: Default::default(), // Stdio
                        command,
                        args: loaded.manifest.runtime.args.clone(),
                        env: std::collections::HashMap::new(),
                        url: None,
                        enabled: true,
                        auto_approve: false,
                        tool_allowlist,
                    };
                    summary.mcp_configs.push(cfg);
                    summary.mcp_servers_registered.push(loaded.manifest.id.clone());
                }
                (Some(_), false) => {
                    tracing::warn!(plugin = %loaded.manifest.id,
                        "plugin declares mcp_servers but lacks run_subprocess permission; skipping spawn");
                    summary.permission_skipped.push(loaded.manifest.id.clone());
                }
                (None, _) => {
                    tracing::warn!(plugin = %loaded.manifest.id,
                        "plugin declares mcp_servers but has no runtime.executable; skipping");
                }
            }
        }
```
(Confirm `McpServerConfig` field names/types against mcp.rs:531 — if `transport_type` has no `Default` or fields differ, adjust. Keep the existing tools loop above untouched.)

- [ ] **Step 4: Run + commit.** `cd src-tauri && cargo test --lib plugins::registration 2>&1 | tail`; `cargo build 2>&1 | grep -E "^error" | head`; `git commit -am "feat(plugins): registrar builds McpServerConfig from manifest, run_subprocess-gated (plugin.1)"`

### Task 2: lifecycle — expose aggregated MCP configs

**Files:** `plugins/lifecycle.rs`.

- [ ] **Step 1: Write failing test** — `connect_and_register` on a temp plugins dir with one written `plugin.toml` fixture (run_subprocess=true, executable, mcp_servers) → the report exposes 1 aggregated mcp config. (Reuse the existing lifecycle test scaffolding at lifecycle.rs:65; write a `plugin.toml` into a `tempfile::tempdir()/<id>/`.)
```rust
#[test]
fn connect_and_register_aggregates_plugin_mcp_configs() {
    let dir = tempfile::tempdir().unwrap();
    let pdir = dir.path().join("hello"); std::fs::create_dir_all(&pdir).unwrap();
    std::fs::write(pdir.join("plugin.toml"), SAMPLE_MANIFEST_TOML).unwrap();
    let mut api = AgentApi::new();
    let report = PluginLifecycleOwner::new(dir.path()).connect_and_register(&mut api);
    assert_eq!(report.plugin_mcp_configs().len(), 1);
}
```
(`SAMPLE_MANIFEST_TOML` = a minimal valid manifest string with `run_subprocess = true`, `runtime.executable`, `contributes.mcp_servers = ["hello"]` — match the TOML shape `PluginManifest` deserializes from; check discovery.rs/schema for the exact field layout.)

- [ ] **Step 2: Implement** — add to `PluginLifecycleReport` a method aggregating configs from each loaded summary:
```rust
impl PluginLifecycleReport {
    /// All MCP server configs contributed by successfully-registered plugins,
    /// to be added to the McpManager by the caller (AppState::new).
    pub fn plugin_mcp_configs(&self) -> Vec<crate::mcp::McpServerConfig> {
        self.loaded.iter().flat_map(|s| s.mcp_configs.clone()).collect()
    }
}
```
(If `PluginLifecycleReport.loaded` holds `PluginRegistrationSummary` — confirm; the `connect_and_register` body pushes `summary` into `report.loaded`.)

- [ ] **Step 3: Run + commit.** `cargo test --lib plugins::lifecycle 2>&1 | tail`; `git commit -am "feat(plugins): PluginLifecycleReport aggregates plugin MCP configs (plugin.2)"`

### Task 3: boot wiring in `AppState::new`

**Files:** `app.rs`.

- [ ] **Step 1: RECON** `AppState::new`: find the `agent_api` variable (the `&mut AgentApi` that `builtin_descriptors::register_all` mutates) + confirm `mcp_manager` (app.rs:602) is mutable/lockable at that point, and `data_dir` is in scope. The plugin wiring must run AFTER `register_all` (so descriptors exist) and produce configs added to `mcp_manager` BEFORE the async MCP connect (so they spawn with the rest).

- [ ] **Step 2: Wire** — after `register_all(&mut agent_api)` and after `mcp_manager` is built:
```rust
    // Plugin loader (subprocess/RPC last mile): discover $DATA_DIR/plugins, register
    // their tool descriptors into the AgentApi, and add their MCP server configs to
    // the manager so the existing MCP connect spawns them. Missing dir → no-op.
    {
        let report = crate::plugins::PluginLifecycleOwner::new(data_dir.join("plugins"))
            .connect_and_register(&mut agent_api);
        for cfg in report.plugin_mcp_configs() {
            if let Err(e) = mcp_manager.add_server(cfg) {
                tracing::warn!(error = %e, "plugin MCP server add_server failed; skipping");
            }
        }
        if !report.discovery_errors.is_empty() || !report.registration_errors.is_empty() {
            tracing::warn!(discovery = ?report.discovery_errors, registration = ?report.registration_errors,
                "plugin loader reported errors");
        }
        tracing::info!(plugins = report.loaded.len(), "plugin loader complete");
    }
```
Adapt: `mcp_manager` is likely `Arc<RwLock<McpManager>>` — `add_server` needs `&mut`, so use `mcp_manager.write().<unwrap/await>` (recon whether it's a sync `std::sync::RwLock` or tokio; the McpManager is built at 602 — match its lock type; `add_server` is sync so a sync write guard works). If `agent_api` is already consumed/Arc-wrapped at this point, move the block earlier (right after `register_all`, before the wrap). FLAG the exact placement you chose.

- [ ] **Step 3: Build.** `cargo build 2>&1 | grep -E "^error" | head` (clean). `cargo test --lib agent::api 2>&1 | tail -3`.

- [ ] **Step 4: Commit.** `git commit -am "feat(app): wire plugin loader into boot — register descriptors + spawn plugin MCP servers (plugin.3)"`

### Task 4: example plugin (the end-to-end deliverable)

**Files:** `examples/plugins/hello-uclaw/{plugin.toml, server.mjs (or server.py), README.md}`.

- [ ] **Step 1: Write `examples/plugins/hello-uclaw/plugin.toml`** — a valid manifest (match the TOML the schema deserializes; confirm field names from schema.rs):
```toml
id = "hello-uclaw"
version = "0.1.0"
display_name = "Hello uClaw"
description = "Example plugin: a stdio MCP server exposing one `hello` tool."

[author]
name = "uClaw examples"

[runtime]
min_uclaw_version = "0.1.0"
kind = "subprocess"
executable = "server.mjs"
args = []

[permissions]
run_subprocess = true

[contributes]
mcp_servers = ["hello-uclaw"]
tools = ["hello"]
```

- [ ] **Step 2: Write the minimal stdio MCP server** `server.mjs` (Node, ~40 lines — JSON-RPC 2.0 over stdin/stdout line-delimited: handle `initialize` → return protocolVersion + capabilities + serverInfo; `tools/list` → one `hello` tool with an input schema `{name: string}`; `tools/call` for `hello` → `{ content: [{ type: "text", text: "Hello, <name>!" }] }`; ignore/ack `notifications/*`). Use only Node built-ins (`readline`/process.stdin). (If Node isn't the target runtime, write `server.py` with stdlib `sys.stdin` + `json` — pick the runtime more likely present; document.)

- [ ] **Step 3: Write `README.md`** — how to use: `cp -r examples/plugins/hello-uclaw "$DATA_DIR/plugins/"` → restart uClaw → the agent gains a `hello` tool (exposed as `mcp__hello-uclaw__hello`). Note the `$DATA_DIR` location (uclaw_home).

- [ ] **Step 4: Commit.** `git add examples/plugins/hello-uclaw && git commit -m "docs(plugins): example hello-uclaw plugin (stdio MCP server) end-to-end (plugin.4)"`

### Task 5: gated integration test + verification

**Files:** `plugins/tests.rs` (or an integration test module).

- [ ] **Step 1: Gated integration test** — if the example server's runtime (`node`/`python3`) is present, point a `PluginDiscovery`/registrar at `examples/plugins/` (or a temp copy), build the config, `McpManager::add_server` + connect, and assert the `hello` tool appears (`tools/list` via the manager). Gate on runtime presence (probe `node --version`; skip + `eprintln!` if absent — CI-safe). If a full live spawn is too heavy for a unit test, at minimum assert the example `plugin.toml` parses + the registrar produces a config with `command` ending in `server.mjs` and `tool_allowlist == Some(["hello"])`.

- [ ] **Step 2: Verification.**
  - `cd src-tauri && cargo test --lib plugins 2>&1 | tail` (registrar + lifecycle + example-manifest tests pass).
  - `cargo build 2>&1 | grep -E "^error"` (clean).
  - `cargo test --lib agent 2>&1 | tail -6` — net green; only the 2 known pre-existing failures.
  - `cargo clippy --lib -- -D warnings 2>&1 | grep -E "plugins/|app\.rs" | head` (clean).
  - `git diff main -- src-tauri/Cargo.toml` (empty).
  - **Boot no-op:** absent `$DATA_DIR/plugins/` → empty report, boot unaffected (the lifecycle missing-dir test + reasoning).
  - **Permission gate:** a manifest without `run_subprocess` → no config + `permission_skipped` (Task 1 test).
- [ ] **Step 3: Commit** (if the integration test is a separate file): `git commit -am "test(plugins): gated end-to-end example-plugin integration (plugin.5)"`

---

## Self-Review

- ✅ **Spec coverage:** registrar mcp config + permission gate (Task 1); lifecycle aggregation (Task 2); boot wiring (Task 3); example plugin (Task 4); gated integration + verification (Task 5). Tools-already-registered noted (not redone). Deferred slices (skills/commands, sandbox, install, UI) untouched.
- ✅ **Placeholder scan:** full code for the registrar config-build + report method + boot block + manifest; the boot placement + lock type + server-script runtime are recon-and-adapt with concrete fallbacks.
- ✅ **Type consistency:** `McpServerConfig { id, name, description, transport_type, command, args, env, url, enabled, auto_approve, tool_allowlist }` (verified mcp.rs:531); subprocess fields on `manifest.runtime` (`executable: Option<String>`); `PluginRegistrationSummary.mcp_configs`/`permission_skipped`; `PluginLifecycleReport::plugin_mcp_configs()`; `mcp_manager.add_server(cfg)` sync.
- ✅ **Risk-scaled:** reuses MCP (zero new RPC); boot wiring (Task 3) is the integration-risk task — additive, no-op on missing dir, isolated block. `add_server` sync avoids async-in-sync-boot. Permission-gated spawn.
- Decisions: executable resolved absolute under plugin_dir (no working_dir field → cwd-independent); registrar decoupled from McpManager (produces configs); tools already wired; OS sandbox + other contributions deferred.
