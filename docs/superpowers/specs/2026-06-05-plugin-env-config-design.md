# Per-Plugin Env Config Design (Slice 2 of the 4-feature batch)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b. Lets a user set env vars (API keys etc.) for a plugin's MCP subprocess so env-needing catalog servers (github → `GITHUB_PERSONAL_ACCESS_TOKEN`) actually work. Slice 2 of 4 (uninstall/upgrade #680 · env-config · remote registry · sandbox v2).

## Problem

Catalog/community MCP servers like `github` need API keys via env. uClaw can install them but provides no way to set their env, and the #669 sandbox `env_clear()`s the subprocess — so even host env wouldn't reach them. The marketplace `setup_note` tells users a key is needed but there's no UI to set it.

## Key finding (recon): the sandbox already supports this

`StdioTransport::spawn` calls `apply_floor(cmd, policy, env)` with `McpServerConfig.env` as `extra_env`. `apply_floor` does `env_clear()` then re-inserts the allowlist **merged with `extra_env` (extra_env wins)** (sandbox.rs:83-87). So **any value placed in `McpServerConfig.env` survives the scrub on all paths** (non-sandbox / Unix floor / macOS seatbelt — seatbelt only rewraps command+args, not env). The pathway is dormant today (`config.env` is always empty). **This slice activates it — NO sandbox changes needed.**

## Decision (approved 2026-06-05)

- **Per-plugin env stored in DB** (new V61 `plugin_env` table: `plugin_id, key, value`, PK(plugin_id,key)). Secrets in the DB (not plaintext config files), lifecycle-consistent with the plugins table.
- **Injected at boot** into `McpServerConfig.env` in `app.rs` Phase 3 (the existing owned-Vec mutation loop where `cfg.enabled` is already applied) → survives the sandbox scrub via the existing `extra_env` path.
- **UI**: an "环境变量" key=value editor in the PluginDetailDrawer (list existing + add/remove rows). Changes take effect next restart (env is read at boot/spawn).
- **Cleanup**: `uninstall_plugin` deletes the plugin's env rows.

## Design

### §1 V61 migration + env DAO (db/migrations.rs + plugins/state.rs)
- `SQL_V61 = "CREATE TABLE IF NOT EXISTS plugin_env (plugin_id TEXT NOT NULL, key TEXT NOT NULL, value TEXT NOT NULL, PRIMARY KEY(plugin_id, key));"` registered after V60 with the same `for stmt in SQL_V61.split(';')...` block (before the final `tracing::info!("Database migrations complete")`).
- `plugins/state.rs` (HashMap already imported):
  - `set_plugin_env(conn, plugin_id, key, value)` — upsert (`INSERT ... ON CONFLICT(plugin_id,key) DO UPDATE SET value=?3`).
  - `get_plugin_env(conn, plugin_id) -> HashMap<String,String>` — `SELECT key,value FROM plugin_env WHERE plugin_id=?1` (mirror `load_enabled_map`'s flatten loop).
  - `delete_plugin_env(conn, plugin_id, key)` — `DELETE ... WHERE plugin_id=?1 AND key=?2`.
  - `delete_all_plugin_env(conn, plugin_id)` — `DELETE ... WHERE plugin_id=?1` (uninstall cleanup).

### §2 Boot injection (app.rs Phase 3, ~lines 998-1015)
In the existing `for cfg in &mut plugin_mcp { cfg.enabled = ...; }` loop, also set `cfg.env`:
```rust
// load env once (reuse the Phase-2 db conn or open one)
let plugin_env_map: HashMap<String, HashMap<String,String>> = { let conn = db.lock()...; /* per id: get_plugin_env */ };
for cfg in &mut plugin_mcp {
    cfg.enabled = *plugin_enabled_map.get(&cfg.id).unwrap_or(&true);
    cfg.env = crate::plugins::state::get_plugin_env(&conn, &cfg.id); // (conn in scope)
}
```
(Plan pins the exact conn handling — simplest: in Phase 3 open one `db.lock()` and call `get_plugin_env(&conn, &cfg.id)` per cfg. `registration.rs` stays untouched — `env: HashMap::new()` there is the default, overwritten here.)

### §3 Tauri commands (tauri_commands.rs + main.rs)
```rust
#[tauri::command] pub async fn get_plugin_env(state, id: String) -> Result<HashMap<String,String>, Error>  // db.lock → get_plugin_env
#[tauri::command] pub async fn set_plugin_env(state, id: String, key: String, value: String) -> Result<(), Error>  // validate key non-empty → set_plugin_env
#[tauri::command] pub async fn delete_plugin_env(state, id: String, key: String) -> Result<(), Error>
```
Register all 3 in main.rs `// Plugins (Pi-3b)` block. Add `delete_all_plugin_env(&conn, &id)` to `uninstall_plugin` (same db.lock block).

### §4 Frontend (PluginDetailDrawer + bridge + test)
- bridge: `getPluginEnv(id) -> Record<string,string>`, `setPluginEnv(id, key, value)`, `deletePluginEnv(id, key)`.
- `PluginDetailDrawer`: fetch `getPluginEnv(plugin.id)` in the existing detail useEffect → `env` state. Render an "环境变量" section (after 权限/沙箱, before 预检): existing rows = key (read-only label or Input) + value Input + 删除 Button; an add-row (2 Inputs + 添加 Button) → `setPluginEnv` then re-fetch. A "重启生效" hint. Direct bridge calls (matches the getPluginDetail precedent).
- test: add a `get_plugin_env` → `{}` stub to PluginsSettings.test.tsx's invoke mock.

## Data flow

```
detail drawer opens → getPluginEnv(id) → render env rows
user adds GITHUB_PERSONAL_ACCESS_TOKEN=ghp_xxx → setPluginEnv(id,key,value) → DB plugin_env
restart → app.rs Phase 3: cfg.env = get_plugin_env(conn, id) → McpServerConfig.env
spawn → apply_floor: env_clear() then re-insert allowlist+extra_env(=cfg.env) → the key reaches the github MCP subprocess ✓
uninstall → delete_all_plugin_env
```

## Out of scope

Env-hint-driven forms (generic key=value editor v1; the catalog `setup_note` tells users what's needed); secret masking/encryption-at-rest (DB plaintext v1 — same as other uClaw secrets); per-session/runtime env override (boot-time only); hot-reload without restart (env read at spawn); validating that a key is actually used by the server. Remote registry / sandbox v2 = the other slices.

## Error handling

`set_plugin_env`: empty key → `Error::InvalidInput`. DB errors → `Error::Database`/`Internal`. `get_plugin_env` on an unknown plugin → empty map (not an error). Boot injection: a db-lock failure → log + cfg.env stays empty (plugin spawns without the env → connect may fail, visible in the detail drawer status — non-fatal to boot). uninstall env-cleanup is best-effort (`let _ =`).

## Testing

1. **env DAO**: `set_plugin_env` then `get_plugin_env` returns the map; upsert overwrites; `delete_plugin_env` removes one key; `delete_all_plugin_env` clears all — on a fresh V61 schema.
2. **V61**: a migrated DB has the `plugin_env` table.
3. **frontend**: drawer renders env rows from a mocked getPluginEnv; adding a row calls setPluginEnv; tsc clean.
`cargo build`/clippy + `cargo test --lib plugins db::migrations` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `db/migrations.rs` | V61 `plugin_env` table |
| `plugins/state.rs` | `set/get/delete_plugin_env` + `delete_all_plugin_env` + tests |
| `app.rs` | Phase 3: inject `cfg.env = get_plugin_env(conn, id)` |
| `tauri_commands.rs` + `main.rs` | `get/set/delete_plugin_env` commands + `delete_all_plugin_env` in uninstall |
| `plugins/sandbox.rs` | (doc-only) update the stale "config.env is empty" comment |
| `ui/.../PluginDetailDrawer.tsx`, `lib/tauri-bridge.ts`, `PluginsSettings.test.tsx` | env editor + bindings + test mock |

## Risk

Low-med. The crux (sandbox) is already solved — this slice activates a dormant, documented pathway with NO sandbox-logic changes. Main risks: (1) **migration V61** is next free (V60 = source column). (2) **boot injection point** — must set `cfg.env` in the SAME Phase-3 owned-Vec loop that sets `cfg.enabled`, BEFORE `add_server` (registration.rs default `env: HashMap::new()` is overwritten). (3) **conn handling in Phase 3** — open one db.lock in Phase 3 + `get_plugin_env(&conn, &cfg.id)` per cfg (avoid lock-per-iteration churn / re-entrancy with Phase 2's lock — Phase 2's lock is dropped before Phase 3). (4) **uninstall cleanup** reuses the existing db.lock block. (5) secrets in DB plaintext — acceptable v1 (consistent with other uClaw secret storage); flagged. Bisectable: migration+DAO → boot-inject+commands+uninstall → frontend → verify. After this slice, env-needing catalog servers (github, etc.) are fully usable: install from marketplace → set the key in the drawer → restart → it connects.
