# Plugin Lifecycle (enable/disable + persistence) Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b — subprocess/RPC plugin system end-to-end. This is the first 3b slice after PR #619 (loader boot + MCP spawn + example plugin). The five gap-audit debts are CLOSED (verified 2026-06-04); the open frontier is the plugin ecosystem, and lifecycle (enable/disable + persistence) is its foundational first slice — a prerequisite for install-from-registry / management UI / sandbox.

## Problem

Plugins are discovered (`plugins/discovery.rs` scans `$DATA_DIR/plugins/<id>/plugin.toml`) and registered (`plugins/registration.rs` → `AgentApi.register_tool` + `McpServerConfig`), spawned at boot. But:

1. **No enable/disable state, no persistence.** grep confirms zero plugin enabled/disabled state anywhere; no `plugins` table; `McpServerConfig.enabled` is hardcoded `true` at plugin registration (`registration.rs:130`). A user cannot turn a plugin off.
2. **`AgentApi::register_plugin` + `plugin_index` are dead.** `agent/api/mod.rs:261` `register_plugin(id, PluginRegistrationSet)` exists ("roll back registrations when the plugin shuts down") but is NEVER called. So tools/commands aren't attributed to their plugin — there's no way to filter a plugin's surface out.
3. **No control surface.** Zero plugin Tauri commands, no UI.

## Decision (approved 2026-06-04)

- **Persistence: a V59 `plugins` table** `(id PK, enabled, updated_at)` — queryable, room for future install/version/sandbox metadata. (Not settings-KV.)
- **Gate both contribution seams** so disable removes the plugin's WHOLE surface: (A) its tools/commands don't enter the session registry; (B) its MCP server doesn't connect.
- **Take effect next session / reconnect** (not immediate mid-session kill). All plugins register at boot (incl. disabled) + are filtered at `build_session_registry`/connect time, so a runtime toggle takes effect on the next agent session / MCP reconnect without a restart.
- **Tauri commands only; UI deferred** (`list_plugins`, `set_plugin_enabled`).

## Design

### §1 V59 `plugins` table + DAO
```sql
CREATE TABLE IF NOT EXISTS plugins (
    id         TEXT PRIMARY KEY,
    enabled    INTEGER NOT NULL DEFAULT 1,
    updated_at INTEGER NOT NULL
);
```
Registered in `db/migrations.rs` `run()` after the latest migration (plan reconfirms next-free V — spec assumes V59). A small DAO (home: a new `plugins/state.rs`, or extend an existing plugins module — plan pins):
- `load_enabled_map(conn) -> HashMap<String, bool>` — all rows.
- `set_plugin_enabled(conn, id, enabled, now_ms)` — upsert.
- `ensure_plugin_row(conn, id, now_ms)` — insert default-enabled row if absent (called per discovered plugin so new plugins default ON + appear in the table).

### §2 Populate `plugin_index` (wire the dead `register_plugin`)
In `plugins/registration.rs` (or `lifecycle.rs` where registration completes), after a plugin's tools/commands/renderers are registered into `AgentApi`, call `api.register_plugin(PluginId(manifest.id), set)` with a `PluginRegistrationSet` listing exactly the tool names / command names / renderer types / hook events it contributed (the `PluginRegistrationSummary` already tracks these — map it to a `PluginRegistrationSet`). This is the attribution that §3's filter needs. **All discovered plugins register** (enabled and disabled alike) — filtering happens at build/connect time, not registration.

### §3 Seam A — `build_session_registry` filters disabled plugins
`AgentApi::build_session_registry(&self, ctx)` currently instantiates EVERY tool. Add a filter: build the set of tool names owned by currently-disabled plugins (from `plugin_index` ∩ the enabled-map) and skip them. The enabled-map is read from `app_state` (the live source, updated on toggle) via `ctx` — `ctx.app_state` is already in scope (used to build `McpToolProxy`). Plan pins the exact `SessionContext`/`app_state` path to the enabled-map. Tools with no owning plugin (builtins) are never filtered.

### §4 Seam B — MCP server enabled-state from the table
At boot, when `plugins/registration.rs` builds each plugin's `McpServerConfig`, set `enabled` from the loaded enabled-map (default true) instead of hardcoded `true`. `mcp::connect_all_enabled` already skips `enabled=false` configs, so a disabled plugin's MCP server won't connect. On `set_plugin_enabled`, also call `mcp_manager.set_enabled(plugin_id, enabled)` (exists, `mcp/mod.rs:1420` — mutates config + persists; disable disconnects on next reconnect cycle / enable allows reconnect).

### §5 app_state enabled-state holder
`AppState` gains `plugin_enabled: Arc<RwLock<HashMap<String, bool>>>`, loaded from the V59 table at boot (after discovery, so every discovered plugin has a row via `ensure_plugin_row`). `build_session_registry`'s filter reads it; `set_plugin_enabled` writes the table AND updates this map so the next session reflects the change without restart. Plan pins `AppState` construction + the `SessionContext.app_state` access.

### §6 Tauri commands (two-edit rule: define + `invoke_handler!`)
- `list_plugins(state) -> Vec<PluginInfo>`: join discovered manifests (re-run discovery, or cache the boot `PluginLifecycleReport`) with the enabled-map + MCP status. `PluginInfo { id, display_name, version, enabled, mcp_connected }`.
- `set_plugin_enabled(state, id, enabled) -> Result<(), String>`: `set_plugin_enabled(conn, ...)` + update `app_state.plugin_enabled` + `mcp_manager.set_enabled(id, enabled)`. Best-effort on the MCP side (a non-MCP plugin just no-ops there).
Both registered in `main.rs` `invoke_handler!`.

## Data flow (after this slice)

```
boot: discover plugins → ensure_plugin_row(id) each → load_enabled_map → app_state.plugin_enabled
      register ALL plugins into AgentApi (+ register_plugin → plugin_index)
      build each McpServerConfig.enabled from the map → connect_all_enabled skips disabled
agent session: build_session_registry → skip tools owned by disabled plugins (plugin_index ∩ map)
toggle: set_plugin_enabled(id,false) → V59 write + app_state map update + mcp_manager.set_enabled
        → next session: tools gone ; next reconnect: MCP server gone
```

## Out of scope

Immediate mid-session kill/rebuild (next-session/reconnect timing chosen); install-from-registry / marketplace; OS sandbox; management UI (commands only); skills contribution gating beyond what plugin_index covers (skills are currently `skipped` in registration — recorded, not live; when they go live they inherit the same plugin_index filter); per-plugin permission editing.

## Error handling

DAO errors: `load_enabled_map` failure → empty map → everything defaults enabled (fail-open is correct for a not-yet-toggled system; a missing row = enabled). `set_plugin_enabled` DB failure → return Err to the command (user sees it). `build_session_registry` filter is null-safe (no plugin_index entry / no map entry → tool kept). MCP `set_enabled` on a non-MCP plugin → no-op. Migration V59 additive.

## Testing

1. **DAO**: `ensure_plugin_row` inserts default-enabled; `set_plugin_enabled` toggles + `load_enabled_map` reflects it; re-`ensure` doesn't clobber an existing row's enabled.
2. **plugin_index population**: after registration, `AgentApi.plugin_index` has an entry per plugin with its contributed tool names.
3. **Seam A filter**: an AgentApi with plugin P (tool "p_tool") + a disabled-map {P:false} → `build_session_registry` omits "p_tool"; enabled → includes it; a builtin tool is never filtered.
4. **Seam B**: a plugin's `McpServerConfig.enabled` reflects the map (false when disabled).
5. **Tauri**: `set_plugin_enabled` persists + updates the live map; `list_plugins` reports enabled-state.
6. `cargo build`/clippy clean; `cargo test --lib` for `agent::api`, `plugins`, `db::migrations` + broad dependent run.

## Scope / files

| File | Change |
|---|---|
| `db/migrations.rs` | V59 `plugins` table, registered in `run()` |
| `plugins/state.rs` (new) | DAO: `load_enabled_map` / `set_plugin_enabled` / `ensure_plugin_row` |
| `plugins/registration.rs` (+ `lifecycle.rs`) | call `register_plugin` (populate plugin_index); set `McpServerConfig.enabled` from the map |
| `agent/api/mod.rs` | `build_session_registry` filters tools owned by disabled plugins (via plugin_index + map) |
| `app.rs` | `AppState.plugin_enabled` holder; boot wiring (ensure rows + load map + thread to registration/registry) |
| `tauri_commands.rs` + `main.rs` | `list_plugins` + `set_plugin_enabled` (+ invoke_handler! registration) |

## Risk

Med. Touches the agent registry build path (`build_session_registry` — hot, per-session) + boot wiring + a new table + 2 commands. Main risks: (1) **build_session_registry filter correctness** — must skip ONLY disabled-plugin tools, never builtins (null-safe lookup); (2) **enabled-map liveness** — the app_state map must be the single source the filter reads (not a stale snapshot), updated on toggle; (3) the **two-config-struct / two-edit Tauri rule** — register both commands in `invoke_handler!`; (4) **plugin_index attribution** — the `PluginRegistrationSet` must list exactly the contributed names so the filter is precise; (5) migration V-number (plan reconfirms V59). Bisectable: table+DAO → plugin_index+filter → boot wiring+MCP enabled → commands → verify. After this slice, plugins are user-controllable (enable/disable persists across restarts; disable removes the whole surface next session/reconnect), and the dead `register_plugin`/`plugin_index` path is alive — the foundation for install/marketplace/sandbox.
