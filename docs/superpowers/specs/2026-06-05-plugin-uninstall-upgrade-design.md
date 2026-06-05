# Plugin Uninstall + Upgrade Design (Slice 1 of the 4-feature batch)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b plugin lifecycle completion. Adds uninstall + upgrade(=reinstall-from-source) to the now-complete install/manage/sandbox/contribute plugin system.

## Problem

A user can install plugins (git / folder / catalog / agent-authored) but can't remove or update them from the UI — only by deleting files manually. There's also no record of WHERE a plugin came from, so we can't re-fetch it to upgrade.

## Decision (approved 2026-06-05)

- **uninstall_plugin(id)**: remove `plugins/<id>/` dir + delete the V59 `plugins` row + remove its MCP server from the manager (disconnect). Takes effect immediately for the MCP side; tools/skills/commands fully gone next restart (registration is boot-time).
- **upgrade_plugin(id) = reinstall from remembered source**: record an install `source` per plugin (new V60 `source` column); upgrade reads it and re-runs the matching install (catalog→`install_plugin_from_catalog`, git→`install_from_git`, local→`install_from_local_dir`); `agent`/unknown source → error "no remembered source to upgrade from".
- **Frontend**: 更新 + 卸载 buttons in the PluginDetailDrawer (mirror McpDetailDrawer's action row), with a confirm on uninstall + list refresh.

## Design

### §1 V60 migration + source DAO (db/migrations.rs + plugins/state.rs)
- `SQL_V60 = "ALTER TABLE plugins ADD COLUMN source TEXT;"` registered in `run()` after V59 (same `for stmt in SQL_V60.split(';')...` swallow-on-error pattern — re-run warns "duplicate column" harmlessly).
- `plugins/state.rs`:
  - `set_plugin_source(conn, id, source: &str)` — `UPDATE plugins SET source=?2 WHERE id=?1` (or upsert).
  - `get_plugin_source(conn, id) -> Option<String>`.
  - `delete_plugin_row(conn, id)` — `DELETE FROM plugins WHERE id=?1`.

### §2 Record source at install (tauri_commands.rs + agent tool)
After each install's `ensure_*_row`, record the source (one combined helper `ensure_installed_row_with_source(state, id, source)` that locks db once: ensure_plugin_row + set_plugin_source):
- `install_plugin_from_git` → `source = format!("git:{git_url}")`.
- `install_plugin_from_catalog` → `source = format!("catalog:{slug}")`.
- `install_plugin_from_dir` → `source = format!("local:{dir_path}")`.
- `install_plugin` agent tool → `source = "agent"` (uses its own db handle).

### §3 uninstall_plugin + upgrade_plugin (tauri_commands.rs + main.rs)
```rust
#[tauri::command]
pub async fn uninstall_plugin(state: State<'_, AppState>, id: String) -> Result<(), Error> {
    let dir = state.data_dir.join("plugins").join(&id);
    if !dir.exists() { return Err(Error::NotFound(format!("plugin '{id}' not installed"))); }
    state.mcp_manager.write().await.remove_server(&id);          // disconnect + drop config
    std::fs::remove_dir_all(&dir).map_err(|e| Error::Internal(e.to_string()))?;
    { let conn = state.db.lock().map_err(|e| Error::Internal(format!("db lock: {e}")))?;
      let _ = crate::plugins::state::delete_plugin_row(&conn, &id); }
    // also drop from the live plugin_enabled map
    if let Ok(mut m) = state.plugin_enabled.write() { m.remove(&id); }
    Ok(())
}

#[tauri::command]
pub async fn upgrade_plugin(state: State<'_, AppState>, id: String) -> Result<InstalledPluginInfo, Error> {
    let source = { let conn = state.db.lock()...; crate::plugins::state::get_plugin_source(&conn, &id) }
        .ok_or_else(|| Error::InvalidInput(format!("no remembered source for '{id}' — reinstall manually")))?;
    // uninstall (dir + mcp; keep going if dir absent)
    let dir = state.data_dir.join("plugins").join(&id);
    state.mcp_manager.write().await.remove_server(&id);
    let _ = std::fs::remove_dir_all(&dir);
    // reinstall from source (does NOT delete the DB row — upgrade keeps enabled state)
    let plugins_root = state.data_dir.join("plugins");
    let p = if let Some(slug) = source.strip_prefix("catalog:") {
        // rebuild from catalog (reuse the catalog→manifest→staging→install path; factor a helper)
        install_catalog_slug(&state, slug).await?
    } else if let Some(url) = source.strip_prefix("git:") {
        crate::plugins::install::install_from_git(url, &plugins_root).await.map_err(|e| Error::InvalidInput(e.to_string()))?
    } else if let Some(path) = source.strip_prefix("local:") {
        crate::plugins::install::install_from_local_dir(std::path::Path::new(path), &plugins_root).map_err(|e| Error::InvalidInput(e.to_string()))?
    } else {
        return Err(Error::InvalidInput(format!("source '{source}' cannot be auto-upgraded")));
    };
    // re-record source + ensure row (preserve enabled — don't delete the row in upgrade)
    ensure_installed_row_with_source(&state, &p.id, &source)?;
    Ok(InstalledPluginInfo { id: p.id, display_name: p.display_name, version: p.version, restart_required: true })
}
```
(Plan pins: factor `install_catalog_slug(state, slug) -> Result<InstalledPlugin>` shared by `install_plugin_from_catalog` + upgrade. upgrade preserves the DB row's enabled state — it does NOT delete_plugin_row, just rm-dir + reinstall + re-set source.) Register both in main.rs.

### §4 Frontend (PluginDetailDrawer + PluginsSettings + bridge)
- bridge: `uninstallPlugin(id): Promise<void>`, `upgradePlugin(id): Promise<InstalledPluginInfo>`.
- `PluginDetailDrawer`: add `onUninstall?`/`onUpgrade?` props + a button row after the enable toggle: 更新 (outline) + 卸载 (destructive). (Mirror McpDetailDrawer's row.)
- `PluginsSettings`: `onUninstall` (confirm → `uninstallPlugin` → toast + close drawer + refresh) + `onUpgrade` (`upgradePlugin` → toast "已更新，重启生效" → refresh); pass both to the drawer.

## Data flow

```
install (any path) → ensure_installed_row_with_source(id, "catalog:fetch" | "git:<url>" | "local:<path>" | "agent")
uninstall: drawer 卸载 → confirm → uninstall_plugin(id) → mcp remove_server + rm dir + delete row + drop map → refresh
upgrade:   drawer 更新 → upgrade_plugin(id) → read source → mcp remove + rm dir → reinstall from source → re-set source → "restart"
```

## Out of scope

Version-diff upgrade (just reinstall-latest from source); upgrading agent-authored plugins (no remembered fetchable source — error); per-file backup/rollback on a failed upgrade (best-effort: a failed reinstall leaves the plugin removed — documented; user reinstalls); confirm-dialog richness (a simple window.confirm v1); uninstalling a plugin mid-session removing its already-loaded tools (next restart). Env-config / remote registry / sandbox v2 = the other 3 slices.

## Error handling

uninstall: dir absent → NotFound; rm/db errors → Internal (best-effort: mcp removed first so it stops even if rm fails). upgrade: no source → InvalidInput; reinstall failure → InvalidInput (the old dir is already removed — the plugin is gone; the error tells the user to reinstall; acceptable v1, documented). V60 ALTER re-run → "duplicate column" swallowed by the migration warn pattern. mcp remove_server on a non-MCP plugin → no-op (returns None). plugin_enabled map drop is best-effort.

## Testing

1. **state DAO**: `set_plugin_source` then `get_plugin_source` round-trips; `delete_plugin_row` removes it; on a fresh V60 schema.
2. **V60**: a migrated DB has the `source` column (ALTER applied).
3. **uninstall** (unit, where feasible): after install_from_local_dir + uninstall_plugin logic (extract a pure `uninstall_plugin_files(plugins_root, id)` + db delete) → dir gone + row gone.
4. **upgrade source-routing**: a pure `parse_source("catalog:fetch") -> Catalog("fetch")` / `git:` / `local:` / unknown→Err helper, unit-tested.
5. **frontend**: drawer shows 更新/卸载; clicking 卸载 (mock confirm=true) calls uninstallPlugin + refreshes; tsc clean.
`cargo build`/clippy + `cargo test --lib plugins db::migrations` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `db/migrations.rs` | V60 `ALTER TABLE plugins ADD COLUMN source TEXT` |
| `plugins/state.rs` | `set_plugin_source` / `get_plugin_source` / `delete_plugin_row` + tests |
| `tauri_commands.rs` + `main.rs` | `uninstall_plugin` + `upgrade_plugin` + `ensure_installed_row_with_source` + `install_catalog_slug` helper + source recording in 3 install cmds |
| `agent/tools/builtin/install_plugin.rs` | record `source="agent"` |
| `ui/.../PluginDetailDrawer.tsx`, `PluginsSettings.tsx`, `lib/tauri-bridge.ts` | 更新/卸载 buttons + handlers + bindings |

## Risk

Med. Touches the DB (V60 migration), the install commands (source recording), MCP manager (remove), filesystem (rm), + frontend. Main risks: (1) **migration V-number** — V60 is next free (V59 = plugins table); ALTER is idempotent-via-swallow on re-run. (2) **upgrade atomicity** — reinstall removes the old dir first, so a failed reinstall leaves the plugin gone (documented; not a silent corruption — the error is surfaced + the DB row preserved so list still shows it as "installed but files missing" → re-upgrade/reinstall recovers). (3) **mcp remove_server** is sync on `&mut McpManager` → `mcp_manager.write().await.remove_server(&id)`. (4) **enabled-state preservation on upgrade** — upgrade must NOT delete_plugin_row (keeps enabled + source); only uninstall deletes the row. (5) **source recording** must hit all install paths (3 cmds + agent tool) or upgrade won't work for that source. (6) uninstall drops the live plugin_enabled map entry so list_plugins is consistent pre-restart. Bisectable: migration+DAO → commands+source → frontend → verify. After this slice, plugins are fully lifecycle-managed (install ↔ uninstall ↔ upgrade) from the UI.
