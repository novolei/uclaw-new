# Remote MCP Registry Browse Design (Slice 3 of the 4-feature batch)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b. Browse + install from the official MCP registry (registry.modelcontextprotocol.io) — extends the bundled catalog (Slice B #679) to the live ecosystem. Slice 3 of 4.

## Problem

The bundled catalog has ~8 curated servers. The official registry lists thousands. Users should be able to search the live registry in-app and one-click-install a server as a managed plugin — reusing the catalog install machinery + the env-config (Slice 2) for keys.

## Registry API (recon)

`GET https://registry.modelcontextprotocol.io/v0/servers?limit=100[&cursor=<nextCursor>]` →
```json
{ "servers": [ { "server": { "name": "io.github.foo/bar", "title": "Bar", "description": "...", "version": "1.0.0",
                              "remotes": [...], "packages": [ { "registryType": "npm"|"pypi", "identifier": "@scope/pkg",
                                "version": "latest", "transport": {"type":"stdio"},
                                "packageArguments": [{"name","description","isRequired","format","type"}],
                                "environmentVariables": [{"name","description","isRequired","isSecret"}] } ] },
                 "_meta": {...} } ],
  "metadata": { "nextCursor": "...", "count": 100 } }
```
- **No server-side `?search=`** → fetch a page, filter client-side.
- Two server flavors: **`remotes`** (HTTP URL — skip in v1, doesn't fit the subprocess plugin model) and **`packages`** (npm→`npx`, pypi→`uvx` stdio — these map to our plugin model).

## Decision (approved 2026-06-05)

- **`search_registry(query)`**: fetch one page (limit=100), keep only servers with a **stdio package** (npm/pypi), map each to a `RegistryEntry`, client-side filter by `query` (name/title/description substring). v1 = first 100 (logged "showing first 100"); pagination/load-more = future.
- **`install_plugin_from_registry(entry)`**: build a `PluginManifest` (executable=npx|uvx, args from identifier@version, permissions=network+fs_write+run_subprocess, contributes.mcp_servers=[id], tools=[]) → install via a shared `install_staged_manifest` helper (factored from `install_catalog_slug`) → record `source="registry:<name>"`. A server with required `packageArguments`/`environmentVariables` carries a `setup_note` (user adds args/keys via the env-config drawer + plugin.toml).
- **id sanitization**: registry names contain `/`/`.` (rejected by install.rs) → `sanitize_plugin_id("io.github.foo/bar") = "io-github-foo-bar"` (non-`[A-Za-z0-9_-]`→`-`, collapse, trim).
- **UI**: a "在线市场" search box + results grid in PluginsSettings (below the bundled 插件市场).

## Design

### §1 `plugins/registry.rs` (new)
- `RegistryEntry { id, name, title, description, command, args: Vec<String>, env_hints: Vec<catalog::EnvHint>, setup_note: Option<String>, homepage: Option<String> }` (Serialize/Deserialize — crosses to TS).
- `sanitize_plugin_id(name: &str) -> String` — map non-`[A-Za-z0-9_-]` to `-`, collapse repeats, trim `-`, lowercase; empty→fallback. (+ unit tests incl. the `io.github.foo/bar` case + that the result passes install.rs's id check: no `/`,`\`,`..`.)
- `async fn fetch_registry(query: Option<&str>) -> Result<Vec<RegistryEntry>, String>` — reqwest::Client::builder().timeout(8s).user_agent(...).build() → GET the URL → `resp.json::<serde_json::Value>()` → navigate `servers[].server`: pick the first package with `transport.type=="stdio"` + registryType npm|pypi; build command (npm→"npx" args ["-y", "<identifier>@<version>"]; pypi→"uvx" args ["<identifier>"]); env_hints from `environmentVariables[]` (name+description); setup_note if any `environmentVariables[].isRequired` or `packageArguments[].isRequired` ("此服务需要配置环境变量/参数：…；安装后在详情里设置"); skip servers with no stdio package. Filter by query substring (lowercased) over name/title/description. Defensive parse (skip malformed entries). Map network/decode errors → `Err(String)`.
- `manifest_from_registry(entry: &RegistryEntry) -> PluginManifest` — mirror `manifest_from_catalog`; id=entry.id, executable=entry.command, args=entry.args, permissions network+fs_write+run_subprocess=true, mcp_servers=[id], tools=[].
- `pub mod registry;` in plugins/mod.rs.

### §2 Shared install helper (tauri_commands.rs)
Factor `install_catalog_slug`'s staging tail into:
```rust
async fn install_staged_manifest(state: &AppState, manifest: &PluginManifest) -> Result<InstalledPlugin, Error> {
    let toml_str = toml::to_string(manifest).map_err(|e| Error::Internal(format!("toml: {e}")))?;
    let plugins_root = state.data_dir.join("plugins");
    let staging = plugins_root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let staged = staging.join(&manifest.id);          // dir name MUST equal manifest.id
    std::fs::create_dir_all(&staged).map_err(|e| Error::Internal(e.to_string()))?;
    std::fs::write(staged.join("plugin.toml"), toml_str).map_err(|e| Error::Internal(e.to_string()))?;
    let res = crate::plugins::install::install_from_local_dir(&staged, &plugins_root).map_err(|e| Error::InvalidInput(e.to_string()));
    let _ = std::fs::remove_dir_all(&staging);
    res
}
```
Rewrite `install_catalog_slug` to use it (`let m = manifest_from_catalog(&entry); install_staged_manifest(&state, &m).await`).

### §3 Tauri commands (tauri_commands.rs + main.rs)
```rust
#[tauri::command] pub async fn search_registry(query: Option<String>) -> Result<Vec<RegistryEntry>, Error>
   // crate::plugins::registry::fetch_registry(query.as_deref()).await.map_err(Error::Internal)
#[tauri::command] pub async fn install_plugin_from_registry(state, entry: crate::plugins::registry::RegistryEntry) -> Result<InstalledPluginInfo, Error>
   // let m = manifest_from_registry(&entry); let p = install_staged_manifest(&state, &m).await?;
   // ensure_installed_row_with_source(&state, &p.id, &format!("registry:{}", entry.name))?; Ok(InstalledPluginInfo{...})
```
Register both in main.rs Plugins block.

### §4 Frontend (PluginsSettings + bridge + types)
- types: `RegistryEntry { id, name, title, description, command, args, env_hints?, setup_note?, homepage? }`.
- bridge: `searchRegistry(query?: string) -> RegistryEntry[]`, `installPluginFromRegistry(entry) -> InstalledPluginInfo`.
- PluginsSettings: a "在线市场" SettingsSection (below 插件市场): a search Input + 搜索 Button (+ Enter) → `searchRegistry` → results grid (cards: title, description, a "源" badge, 安装 button [已安装 if installedSlugs.has(entry.id)]). Install → `doInstall(() => installPluginFromRegistry(entry))` + `setup_note` toast. Loading/empty states ("搜索中…", "无结果", initial "搜索社区 MCP 服务器…").

## Data flow

```
在线市场 search "github" → search_registry("github") → fetch registry page → filter stdio packages + substring → RegistryEntry[]
click 安装 → install_plugin_from_registry(entry) → manifest_from_registry → install_staged_manifest → plugins/<id>/ + source="registry:<name>"
setup_note present → toast "需配置：…" → user sets env in detail drawer (Slice 2) → restart → connects
```

## Out of scope

Pagination/load-more (first 100 v1); server-side search (client-side filter); remote/HTTP servers (subprocess plugins only v1 — remotes skipped; could be added via the Integrations MCP path later); auto-upgrade of registry plugins (`registry:` source → upgrade errors cleanly, reinstall manually); version pinning UI; trust/signature verification (registry is the official source; npx fetches at runtime). Sandbox v2 = Slice 4.

## Error handling

`search_registry`: network/timeout/decode failure → `Error::Internal("registry: …")` (frontend toasts "搜索失败"). Empty/malformed entries skipped (not fatal). `install_plugin_from_registry`: id-sanitize → always path-safe; already-installed → `install_from_local_dir` AlreadyInstalled → `Error::InvalidInput` (frontend "已安装"). npx/uvx missing on the host → installs but fails to connect at boot (visible in detail drawer status) — documented. Registry down → search errors; no offline cache v1.

## Testing

1. **sanitize_plugin_id**: `"io.github.foo/bar"` → `"io-github-foo-bar"` (no `/`,`.`,`..`); collapses repeats; trims; passes install.rs's id rules. Unit test.
2. **registry parse** (pure): given a sample registry JSON `serde_json::Value` (npm package + pypi package + a remotes-only server), `parse_servers(value, None)` → 2 entries (remotes-only skipped), npm→command "npx" args ["-y","@x/y@1.0.0"], pypi→"uvx". Extract `parse_servers(value, query)` pure (no network) + test it. (The async `fetch_registry` is the thin network wrapper.)
3. **manifest_from_registry**: entry → PluginManifest with executable=command, mcp_servers=[id], tools empty; toml round-trips.
4. **frontend**: search renders results from a mocked searchRegistry; install calls installPluginFromRegistry; tsc clean.
`cargo build`/clippy + `cargo test --lib plugins` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `plugins/registry.rs` (new) | `RegistryEntry`, `sanitize_plugin_id`, `parse_servers` (pure) + `fetch_registry` (async), `manifest_from_registry` + tests |
| `plugins/mod.rs` | `pub mod registry;` |
| `tauri_commands.rs` + `main.rs` | `search_registry` + `install_plugin_from_registry` + `install_staged_manifest` (factor from install_catalog_slug) |
| `ui/lib/types.ts` + `lib/tauri-bridge.ts` | `RegistryEntry` + `searchRegistry`/`installPluginFromRegistry` |
| `ui/.../settings/PluginsSettings.tsx` | "在线市场" search section |

## Risk

Med-high (network integration + external API shape + new module). Main risks: (1) **registry JSON shape** — parse defensively via `serde_json::Value` navigation (not strict structs) so missing/extra fields don't break it; skip malformed entries; the `parse_servers` pure function is unit-tested against a fixed sample. (2) **id sanitization** — registry names contain `/` (install.rs rejects); `sanitize_plugin_id` must produce a path-safe id; staging inner dir = `staging.join(&manifest.id)`. (3) **no server-side search** — fetch 100 + client filter; log "first 100" (don't pretend full coverage). (4) **registry source not auto-upgradable** — clean `InvalidInput` from upgrade (documented; reinstall). (5) **reqwest per-call** (no shared client) — mirror skill_marketplace.rs (timeout + user-agent). (6) **remotes skipped** — only stdio-package servers installable v1; note in UI/empty-state. (7) env/arg-required servers carry setup_note → reuse Slice 2 env-config. Bisectable: registry module+sanitize+parse (tested) → commands+shared-helper → frontend → verify. After this slice, users search the live MCP ecosystem in-app and install any stdio server as a managed, sandboxed, env-configurable plugin.
