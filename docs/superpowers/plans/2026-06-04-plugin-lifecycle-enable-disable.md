# Plugin Lifecycle (enable/disable + persistence) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** User-controllable plugin enable/disable with persistence. Disable removes the plugin's whole surface (tools/commands + MCP server) next session/reconnect. Wire the dead `register_plugin`/`plugin_index`.

**Spec:** `docs/superpowers/specs/2026-06-04-plugin-lifecycle-enable-disable-design.md`

---

## Pinned facts (verbatim — do not re-derive)

- **Next migration = V59.** V58 (tool_transitions) registered in `db/migrations.rs` `run()` via `for stmt in SQL_V58.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) { if let Err(e) = conn.execute(stmt, []) { tracing::warn!("V58 stmt skipped: {} :: {}", e, stmt); } }`. Mirror for V59. The `Ok(())` + "migrations complete" log is right after V58 — insert V59 before it.
- **`SessionContext`** (`agent/api/session_context.rs`): `{ session_id, workspace, model, app_handle, llm, app_state: &'a crate::app::AppState, tool_config }`. So `ctx.app_state` is `&AppState` — full read access.
- **`build_session_registry`** (`agent/api/mod.rs:85`) is **SYNC**:
  ```rust
  pub fn build_session_registry(&self, ctx: &SessionContext<'_>) -> ToolRegistry {
      let mut registry = ToolRegistry::new();
      for descriptor in self.tools.values() {
          let instance = (descriptor.builder)(ctx);
          registry.register_boxed(instance);
      }
      registry
  }
  ```
  → reading the enabled-map here MUST use a **`std::sync::RwLock`** (a `tokio::sync::RwLock::blocking_read()` panics if this is ever called from an async context). Use `std::sync::RwLock<HashMap<String,bool>>` for `plugin_enabled`.
- **`AgentApi.plugin_index: HashMap<PluginId, PluginRegistrationSet>`**; `PluginRegistrationSet { tools: Vec<String>, providers, commands: Vec<String>, renderers, hook_events }`. `register_plugin(&mut self, id: PluginId, set)` (mod.rs:261, `pub(crate)`) inserts into plugin_index. `PluginId::new(s)`.
- **Plugin tool names are PREFIXED.** `registration.rs` registers `ToolDescriptor{ name: crate::mcp::prefixed_tool_name(&plugin_id, &tool_name), ... }`. But `summary.tools_registered` holds the RAW `tool_name`. **For the filter to match `descriptor.name`, `PluginRegistrationSet.tools` MUST hold the PREFIXED names** — recompute `prefixed_tool_name(plugin_id, raw)` when building the set, do NOT use raw `summary.tools_registered`.
- **`PluginRegistrar::register(api: &mut AgentApi, loaded: &LoadedPlugin) -> Result<PluginRegistrationSummary, _>`** (`plugins/registration.rs:49`); `PluginRegistrationSummary { plugin_id, tools_registered, commands_registered, mcp_servers_registered, mcp_configs: Vec<McpServerConfig>, ... }`. `McpServerConfig { id, name, ..., enabled: bool, ... }` built with `enabled: true` hardcoded (~line 130).
- **Boot site** (`app.rs:~916-1004`): inside `let agent_api = { let mut api = AgentApi::new(); register builtins; let report = crate::plugins::PluginLifecycleOwner::new(plugins_root).connect_and_register(&mut api); ... add report.plugin_mcp_configs() to mcp_manager via blocking_write; ... Arc::new(api) }`. `connect_and_register(&mut api) -> PluginLifecycleReport { loaded: Vec<PluginRegistrationSummary>, ... }`. The `db: Arc<std::sync::Mutex<rusqlite::Connection>>` is available in `AppState::new` scope. AppState is built via a big struct literal `Ok(Self { ... agent_api, ... })`.
- **`AppState`** uses `std::sync::Mutex` for `db`; `tokio::sync::RwLock` for some configs. For `plugin_enabled` use `Arc<std::sync::RwLock<HashMap<String,bool>>>` (see SYNC constraint above).
- **MCP**: `McpManager::set_enabled(&mut self, id, enabled) -> bool` (mod.rs:1420, mutates + save_config); `connect_all_enabled` skips `enabled=false`; `list_enabled_ids`. `SharedMcpManager = Arc<tokio::sync::RwLock<McpManager>>`. Server status: `mcp_manager.read().await` → `.servers.get(id)` → `McpServerState { status: McpServerStatus, config }`. Tauri cmd async → `state.mcp_manager.read().await` / `.write().await`.
- **Discovery for list_plugins**: boot report NOT persisted → `list_plugins` re-runs `PluginDiscovery::new(data_dir.join("plugins")).discover()` → `Vec<Result<LoadedPlugin,_>>`; `LoadedPlugin{ manifest: PluginManifest{ id, version, display_name, ... }, plugin_dir, manifest_path }`.
- **Tauri pattern**: `#[tauri::command] pub async fn foo(state: State<'_, AppState>, ...) -> Result<T, Error>`. Register in `main.rs` `tauri::generate_handler![ ... uclaw_core::tauri_commands::NAME, ... ]` (near the MCP command block).
- **Tests**: `agent/api/tests.rs` builds `AgentApi::new()` + `register_tool(make_test_descriptor("x"))` + `register_plugin(id, set)`; `make_test_descriptor` available. DB tests: `rusqlite::Connection::open_in_memory()` + `super::run(&conn)`. plugins tests use `tempfile::tempdir()` + write `plugin.toml`.
- **NEW file `plugins/state.rs` needs explicit `git add`** (prior slice lost a new file via `-am`).

---

## Task 1: V59 `plugins` table + DAO

**Files:** Modify `db/migrations.rs`; Create `plugins/state.rs`; Modify `plugins/mod.rs` (add `pub mod state;`)

- [ ] **Step 1: V59 migration**

In `db/migrations.rs`, after `SQL_V58`:
```rust
const SQL_V59: &str = "\
CREATE TABLE IF NOT EXISTS plugins (\
    id         TEXT PRIMARY KEY,\
    enabled    INTEGER NOT NULL DEFAULT 1,\
    updated_at INTEGER NOT NULL\
);\
";
```
In `run()`, after the V58 block + before `tracing::info!("Database migrations complete")`:
```rust
    tracing::debug!("Running migration V59: plugins (enable/disable state)");
    for stmt in SQL_V59.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V59 stmt skipped: {} :: {}", e, stmt);
        }
    }
```

- [ ] **Step 2: Create `plugins/state.rs` DAO + add `pub mod state;` to `plugins/mod.rs`**

```rust
//! openhuman/Pi-3b — plugin enable/disable persistence (V59 `plugins` table).

use rusqlite::{params, Connection};
use std::collections::HashMap;

/// Load all plugin enabled-states. Missing/empty → empty map (a plugin with
/// no row is treated as enabled by callers — fail-open).
pub fn load_enabled_map(conn: &Connection) -> HashMap<String, bool> {
    let mut map = HashMap::new();
    if let Ok(mut stmt) = conn.prepare("SELECT id, enabled FROM plugins") {
        if let Ok(rows) = stmt.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)? != 0))
        }) {
            for row in rows.flatten() {
                map.insert(row.0, row.1);
            }
        }
    }
    map
}

/// Insert a default-enabled row for a plugin if it has none (idempotent;
/// never clobbers an existing row's enabled-state).
pub fn ensure_plugin_row(conn: &Connection, id: &str, now_ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plugins (id, enabled, updated_at) VALUES (?1, 1, ?2)
         ON CONFLICT(id) DO NOTHING",
        params![id, now_ms],
    )?;
    Ok(())
}

/// Set a plugin's enabled-state (upsert).
pub fn set_plugin_enabled(conn: &Connection, id: &str, enabled: bool, now_ms: i64) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plugins (id, enabled, updated_at) VALUES (?1, ?2, ?3)
         ON CONFLICT(id) DO UPDATE SET enabled = ?2, updated_at = ?3",
        params![id, enabled as i64, now_ms],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn conn() -> Connection {
        let c = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&c).unwrap();
        c
    }
    #[test]
    fn ensure_defaults_enabled_and_does_not_clobber() {
        let c = conn();
        ensure_plugin_row(&c, "p", 1).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&true));
        set_plugin_enabled(&c, "p", false, 2).unwrap();
        ensure_plugin_row(&c, "p", 3).unwrap(); // must NOT re-enable
        assert_eq!(load_enabled_map(&c).get("p"), Some(&false));
    }
    #[test]
    fn set_toggles_and_load_reflects() {
        let c = conn();
        set_plugin_enabled(&c, "p", false, 1).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&false));
        set_plugin_enabled(&c, "p", true, 2).unwrap();
        assert_eq!(load_enabled_map(&c).get("p"), Some(&true));
    }
}
```

- [ ] **Step 3: Run + commit**

`cd src-tauri && cargo test --lib plugins::state db::migrations 2>&1 | tail` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/plugins/state.rs src-tauri/src/plugins/mod.rs
git commit -m "feat(plugins): V59 plugins table + enabled-state DAO (load/ensure/set)"
```
Verify `git show HEAD --stat` lists `plugins/state.rs` (new).

---

## Task 2: `AppState.plugin_enabled` + `build_session_registry` filter + populate `plugin_index`

**Files:** Modify `app.rs` (AppState field), `agent/api/mod.rs` (filter), `plugins/registration.rs` (register_plugin + prefixed names)

- [ ] **Step 1: Add `plugin_enabled` to `AppState`**

In `app.rs` `AppState` struct, add (near other Arc fields):
```rust
/// Pi-3b — plugin enable/disable state (id → enabled), loaded from V59 at
/// boot. std RwLock because build_session_registry (sync, possibly async ctx)
/// reads it. Toggled live by set_plugin_enabled so the next session reflects it.
pub plugin_enabled: std::sync::Arc<std::sync::RwLock<std::collections::HashMap<String, bool>>>,
```
In the `Ok(Self { ... })` literal, initialize `plugin_enabled` (Task 3 fills it with real data; for now default empty so it compiles — Task 3 replaces the init):
```rust
plugin_enabled: std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
```

- [ ] **Step 2: `build_session_registry` filters disabled-plugin tools**

In `agent/api/mod.rs`, replace `build_session_registry`:
```rust
pub fn build_session_registry(&self, ctx: &SessionContext<'_>) -> crate::agent::tools::tool::ToolRegistry {
    // Pi-3b — tools owned by a currently-disabled plugin are skipped.
    let disabled_tools: std::collections::HashSet<String> = {
        let enabled = ctx.app_state.plugin_enabled.read().ok();
        match enabled {
            Some(map) => self
                .plugin_index
                .iter()
                .filter(|(pid, _)| matches!(map.get(pid.as_str()), Some(false)))
                .flat_map(|(_, set)| set.tools.iter().cloned())
                .collect(),
            None => std::collections::HashSet::new(), // lock poisoned → fail-open
        }
    };
    let mut registry = crate::agent::tools::tool::ToolRegistry::new();
    for descriptor in self.tools.values() {
        if disabled_tools.contains(&descriptor.name) {
            continue;
        }
        let instance = (descriptor.builder)(ctx);
        registry.register_boxed(instance);
    }
    registry
}
```
(`PluginId::as_str()` exists. Builtins have no plugin_index entry → never in `disabled_tools` → never filtered.)

- [ ] **Step 3: Populate `plugin_index` with PREFIXED tool names in registration**

In `plugins/registration.rs` `PluginRegistrar::register`, after the tools/commands/mcp loops build `summary`, before `Ok(summary)`, register the attribution into AgentApi using PREFIXED tool names (matching the descriptors):
```rust
    // Pi-3b — attribute this plugin's contributions so build_session_registry
    // can filter them when the plugin is disabled. Tool names MUST be the
    // PREFIXED names (matching the registered ToolDescriptor.name).
    let mut set = crate::agent::api::plugin::PluginRegistrationSet::default();
    for raw in &contrib.tools {
        set.tools.push(crate::mcp::prefixed_tool_name(&loaded.manifest.id, raw));
    }
    set.commands = summary.commands_registered.clone();
    api.register_plugin(
        crate::agent::api::plugin::PluginId::new(loaded.manifest.id.clone()),
        set,
    );
```
(`contrib` = `&loaded.manifest.contributes`, already in scope. `register_plugin` is `pub(crate)` — registration.rs is in-crate, OK. If `register_plugin` visibility blocks the call, widen to `pub` — note it.)

- [ ] **Step 4: Tests**

In `agent/api/tests.rs`, add a Seam-A filter test. It needs a `SessionContext` with an `app_state` carrying `plugin_enabled`. If building a full `AppState` in a unit test is too heavy, the cleanest is to make the filter testable WITHOUT a full AppState — but since `build_session_registry` takes `&SessionContext` which needs `&AppState`, check how existing build_session_registry tests construct a SessionContext (grep `build_session_registry` in tests). If there's a test AppState builder, reuse it + set `plugin_enabled`. If NOT, add a focused unit test on the filter LOGIC by extracting the `disabled_tools` computation into a small `pub(crate) fn disabled_tool_names(plugin_index, enabled_map) -> HashSet<String>` helper and test that directly (then build_session_registry calls it). PREFER the helper-extraction (clean + testable without AppState). Report which you did.
Test: plugin_index {P:[tool "mcp__P__t"]} + map {P:false} → disabled set contains "mcp__P__t"; map {P:true} → empty; a builtin name never appears.

- [ ] **Step 5: Build + commit**

`cargo build 2>&1 | grep -E "^error"` → empty. `cargo test --lib agent::api 2>&1 | tail` → green.
```bash
git add src-tauri/src/app.rs src-tauri/src/agent/api/mod.rs src-tauri/src/plugins/registration.rs
# + agent/api/tests.rs if separate, + any helper file
git commit -m "feat(plugins): build_session_registry filters disabled-plugin tools + populate plugin_index (prefixed names)"
```

---

## Task 3: Boot wiring — ensure rows, load map, set MCP enabled, store in AppState

**Files:** Modify `app.rs`; `plugins/lifecycle.rs` + `plugins/registration.rs` (thread the enabled-map)

- [ ] **Step 1: Thread an enabled-map into registration so `McpServerConfig.enabled` honors it**

Extend the boot plugin flow so each plugin's `McpServerConfig.enabled` = map lookup (default true). Two viable wirings — pick the lower-churn one after reading `connect_and_register`:
- **(a)** Add an `enabled_map: &HashMap<String,bool>` param to `connect_and_register` (and to `PluginRegistrar::register`), and set `enabled: enabled_map.get(&loaded.manifest.id).copied().unwrap_or(true)` in the `McpServerConfig` literal (replace hardcoded `true`).
- **(b)** Leave registration as-is, and in `app.rs` after `connect_and_register` returns, mutate `report.plugin_mcp_configs()`'s enabled before `add_server` — but `plugin_mcp_configs()` likely returns owned/cloned configs; if you can adjust them before `add_server`, do it there.
Prefer (a) if `connect_and_register`/`register` are easy to thread; else (b). Report which.

- [ ] **Step 2: Boot sequence in `app.rs` (inside/around the `agent_api` block)**

Before/within the plugin registration, using the `db` conn (lock it briefly) + discovery:
1. Run discovery (or reuse the report's loaded plugin ids) to get plugin ids.
2. For each discovered plugin id: `crate::plugins::state::ensure_plugin_row(&conn, id, now_ms)`.
3. `let enabled_map = crate::plugins::state::load_enabled_map(&conn);`
4. Pass `&enabled_map` into the registration flow (Step 1).
5. Store the map into AppState: replace the Task-2 empty init with `plugin_enabled: std::sync::Arc::new(std::sync::RwLock::new(enabled_map)),` (or set it after construction if the map is computed after the struct literal — thread a variable).
`now_ms = chrono::Utc::now().timestamp_millis()`. Lock the `db` mutex with `db.lock()` (it's `Arc<std::sync::Mutex<Connection>>`; AppState::new is sync so a blocking lock is fine). Drop the conn lock before any `.await`.

- [ ] **Step 3: Build + a boot smoke test**

`cargo build 2>&1 | grep -E "^error"` → empty. If there's an AppState/boot integration test, run it. Add (if feasible) a test that a disabled plugin's McpServerConfig comes out `enabled=false` when the map says so (can test `PluginRegistrar::register` with an enabled_map directly, mirroring the lifecycle test fixture).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/app.rs src-tauri/src/plugins/lifecycle.rs src-tauri/src/plugins/registration.rs
git commit -m "feat(plugins): boot wiring — ensure rows + load enabled-map + gate MCP config enabled + store in AppState"
```

---

## Task 4: Tauri commands `list_plugins` + `set_plugin_enabled`

**Files:** Modify `tauri_commands.rs`, `main.rs`

- [ ] **Step 1: `list_plugins`**

In `tauri_commands.rs`:
```rust
#[derive(serde::Serialize)]
pub struct PluginInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub enabled: bool,
    pub mcp_connected: bool,
}

#[tauri::command]
pub async fn list_plugins(state: State<'_, AppState>) -> Result<Vec<PluginInfo>, Error> {
    let plugins_root = state.data_dir.join("plugins");
    let discovered = crate::plugins::PluginDiscovery::new(plugins_root).discover().unwrap_or_default();
    let enabled_map = { state.plugin_enabled.read().map(|m| m.clone()).unwrap_or_default() };
    let mgr = state.mcp_manager.read().await;
    let mut out = Vec::new();
    for r in discovered {
        if let Ok(loaded) = r {
            let id = loaded.manifest.id.clone();
            let mcp_connected = mgr.servers.get(&id).map(|s| /* status connected? */ s.is_connected()).unwrap_or(false);
            out.push(PluginInfo {
                enabled: enabled_map.get(&id).copied().unwrap_or(true),
                mcp_connected,
                id,
                display_name: loaded.manifest.display_name,
                version: loaded.manifest.version,
            });
        }
    }
    Ok(out)
}
```
(Confirm the McpServerState connected check — find the actual field/method, e.g. `matches!(s.status, McpServerStatus::Connected)` or an `is_connected()`. If `servers` isn't pub-accessible from the command, use an existing `list_*`/status method on McpManager. Report what you used.)

- [ ] **Step 2: `set_plugin_enabled`**

```rust
#[tauri::command]
pub async fn set_plugin_enabled(state: State<'_, AppState>, id: String, enabled: bool) -> Result<(), Error> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    {
        let conn = state.db.lock().map_err(|e| Error::Internal(format!("db lock: {e}")))?;
        crate::plugins::state::set_plugin_enabled(&conn, &id, enabled, now_ms)
            .map_err(Error::Database)?;
    }
    if let Ok(mut map) = state.plugin_enabled.write() {
        map.insert(id.clone(), enabled);
    }
    {
        let mut mgr = state.mcp_manager.write().await;
        mgr.set_enabled(&id, enabled); // no-op (false) if not an MCP-backed plugin
    }
    Ok(())
}
```
(Match the actual `Error` variants — find how other commands construct `Error::Internal`/`Error::Database` or use `.to_string()` → `Result<_, String>`. Mirror the nearest command's error style.)

- [ ] **Step 3: Register in `main.rs` invoke_handler!**

Add near the MCP command block in `tauri::generate_handler![...]`:
```rust
            uclaw_core::tauri_commands::list_plugins,
            uclaw_core::tauri_commands::set_plugin_enabled,
```

- [ ] **Step 4: Build + commit**

`cargo build 2>&1 | grep -E "^error"` → empty (the two-edit rule: missing main.rs entry compiles but fails at runtime — verify BOTH edits done).
```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(plugins): list_plugins + set_plugin_enabled Tauri commands (+ invoke_handler registration)"
```

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean (no new warnings in plugins/agent::api/migrations/tauri_commands).
- [ ] **Step 2**: tests — `plugins::state`, `agent::api`, `db::migrations`, `plugins` + broad dependent run. Green.
- [ ] **Step 3**: grep gates — `register_plugin` now CALLED in registration (was dead); both commands in `main.rs` generate_handler!; `McpServerConfig` `enabled` no longer hardcoded `true` for plugins.
- [ ] **Step 4**: `npx gitnexus analyze`.
- [ ] **Step 5**: PR with `## Commits (bisectable)` table. Note two-edit Tauri rule honored; std RwLock rationale; prefixed-name attribution. **Verify `git show <commit> --stat` includes `plugins/state.rs`.**
- [ ] **Step 6**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory ([[project-pi-lightweight-vs-agent-os]] or a new plugin-system memory: lifecycle slice shipped; next 3b = skills/commands contribution / install / sandbox / UI).

---

## Self-Review

**Spec coverage:** §1 table+DAO → T1; §2 plugin_index → T2; §3 Seam A filter → T2; §4 Seam B MCP enabled → T3; §5 app_state holder → T2(field)+T3(load); §6 commands → T4. ✓
**Placeholder scan:** the "(a) vs (b) threading", "helper-extraction vs full AppState test", "McpServerState connected check", "Error variant style" are flagged decisions with concrete fallbacks + report-back, not TODOs. ✓
**Type consistency:** `plugin_enabled: Arc<std::sync::RwLock<HashMap<String,bool>>>` used in app.rs field + build_session_registry read + set_plugin_enabled write; `PluginRegistrationSet.tools` = prefixed names matching descriptor.name (the filter's correctness hinge). ✓
**SYNC/async:** std::sync::RwLock for plugin_enabled (build_session_registry is sync, may run in async ctx → no tokio blocking_read). ✓
**New-file safety:** T1 + T5 verify `plugins/state.rs` in `git show --stat`. ✓
