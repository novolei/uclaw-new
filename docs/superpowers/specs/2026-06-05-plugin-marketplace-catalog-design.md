# Plugin Marketplace Catalog Design (Slice B)

**Date:** 2026-06-05
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b. A bundled curated catalog of community MCP servers the user can browse + one-click install (as managed plugins). After install (#675), UI v2 (#676), authoring (#678). Slice B of two.

## Problem

uClaw plugins are MCP subprocesses, and the community has 20k+ MCP servers — but a user can only add one by knowing its git URL / scaffolding it. There's no in-app "browse known-good servers + install" experience. We want a curated marketplace: a vetted list shipped with uClaw, browsable in Settings, one-click → generates a `plugins/<slug>/` and installs it (restart to activate).

## Decision (approved 2026-06-05)

- **Bundled static curated JSON catalog** (`include_str!`, ~6-8 vetted official MCP reference servers). No remote/registry dependency in v1.
- **Catalog install generates a plugin.toml** (executable = the server command, e.g. `npx`/`uvx`; args; permissions from the entry; `contributes.mcp_servers=[slug]`, `contributes.tools=[]`) → installs as a managed plugin (lifecycle/UI/detail/sandbox all apply). Leaving `contributes.tools` empty means `tool_allowlist=None` → ALL the server's real tools (from its `tools/list` at connect) are exposed — no need to curate tool names.
- **PATH-command executables**: catalog servers run via `npx`/`uvx` (PATH commands, not files in the plugin dir). registration's command resolution is extended to treat a bare command (no path separator) as a PATH lookup.
- **Sandbox relaxed per declared permissions**: npx servers need network (fetch) + fs_write (npm cache); the catalog entry declares them → the #669 sandbox honors them. Documented tradeoff (npx servers are lightly-sandboxed: env-scrub + cwd + rlimits still apply, FS/network open per declared perms).
- **v1 = zero-config servers work immediately**; entries needing env (API keys) or extra args carry a `setup_note` shown to the user (env injection is a documented v2 follow — the floor env-scrub would strip API keys; for now the user edits the generated plugin.toml).

## Design

### §1 PATH-command executable support (plugins/registration.rs + runtime.rs)
- `registration.rs` command-build: currently `is_absolute() ? executable : plugin_dir.join(exe)`. Add a middle case: **bare command** (`!is_absolute && no '/' separator`, e.g. `npx`) → use `executable` as-is (PATH lookup). So: `if absolute → executable; else if contains('/') → plugin_dir.join(exe); else → executable (PATH)`.
- `runtime.rs` preflight (the "executable does not exist yet" check): skip the existence warning for a bare-command executable (it's a PATH command, resolved at spawn).

### §2 Bundled catalog (`plugins/catalog.rs` new + `plugins/catalog.json`)
```rust
#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct CatalogEntry {
    pub slug: String,            // = the plugin id installed
    pub name: String,            // display_name
    pub description: String,
    pub category: String,        // "office" | "coding" | "web" | "utility"
    pub command: String,         // "npx" | "uvx"
    pub args: Vec<String>,       // e.g. ["-y", "@modelcontextprotocol/server-memory"]
    #[serde(default)] pub permissions: CatalogPermissions, // network/fs_read/fs_write/run_subprocess
    #[serde(default)] pub env_hints: Vec<EnvHint>,         // { name, description } — informational
    #[serde(default)] pub setup_note: Option<String>,      // shown if extra config needed
    #[serde(default)] pub homepage: Option<String>,
}
pub fn builtin_catalog() -> Vec<CatalogEntry> { serde_json::from_str(include_str!("catalog.json")).unwrap_or_default() }
```
`catalog.json` — ~6-8 entries (official `@modelcontextprotocol/server-*` / `mcp-server-*`): memory, sequential-thinking, time, fetch, everything (zero-config) + github, filesystem, git (with `setup_note` for env/args). Each with accurate command/args + minimal permissions.

### §3 Tauri commands (tauri_commands.rs + main.rs)
- `list_catalog() -> Vec<CatalogEntry>` — returns `builtin_catalog()`, marking which are already installed (a `installed: bool` on a thin DTO, or the frontend cross-refs `list_plugins`). Plan picks: return `CatalogEntry` + the frontend dims already-installed (cross-ref by slug against listPlugins). Simpler: return the entries; frontend cross-refs.
- `install_plugin_from_catalog(slug) -> InstalledPluginInfo`:
  - find the entry; build a `PluginManifest` (id=slug, version="0.0.0" or entry, display_name=name, description, author{name:"marketplace"}, runtime{kind:"subprocess", executable:command, args}, permissions from entry, contributes{mcp_servers:[slug], tools:[]});
  - write to a temp dir `<plugins_root>/.catalog-staging-<uuid>/<slug>/plugin.toml` (via `toml::to_string(&manifest)`), then `install_from_local_dir(staging/<slug>, plugins_root)` (reuse the validated install path) + `ensure_plugin_row`; clean staging;
  - return `InstalledPluginInfo { id, display_name, version, restart_required: true }`.
- Register both in main.rs.

### §4 Frontend (PluginsSettings.tsx + tauri-bridge.ts + types.ts)
- types: `CatalogEntry { slug, name, description, category, command, args, permissions, env_hints?, setup_note?, homepage? }`.
- bridge: `listCatalog()` + `installPluginFromCatalog(slug)`.
- PluginsSettings: add a **"插件市场"** SettingsSection (after install, before the installed grid) — a card grid of catalog entries (name, description, category badge, an 安装 button); already-installed entries (cross-ref by slug) show 已安装 (disabled). Install → `installPluginFromCatalog(slug)` → toast (with `setup_note` if present: "已安装，重启激活；注意：<setup_note>") → refresh.

## Data flow

```
open 插件 tab → listCatalog() + listPlugins() → render 市场 cards (dim installed)
click 安装(memory) → install_plugin_from_catalog("memory")
  → build PluginManifest{ executable:"npx", args:["-y","@modelcontextprotocol/server-memory"], mcp_servers:["memory"], tools:[] }
  → toml::to_string → staging → install_from_local_dir → plugins/memory/ + DB row
  → toast "已安装 Memory，重启激活" → refresh
next boot: discovery → register (executable "npx" = PATH command) → spawn (sandbox: perms-relaxed) → tools/list → all tools exposed
```

## Out of scope

Remote/registry catalog (v1 bundled-static); env/API-key injection UI (env-needing servers carry a setup_note; user edits plugin.toml — v2); per-entry arg configuration UI (e.g. filesystem root path — setup_note for v1); uninstall; auto-restart; curating tool names (real tools come from tools/list); WASM. The catalog is small + curated (not the full 20k — that's the remote-registry v2).

## Error handling

`install_plugin_from_catalog`: unknown slug → `Error::NotFound`. Already installed (install_from_local_dir → AlreadyInstalled) → surfaced as a user error ("已安装"). toml serialize / staging / copy failure → clean staging + Err. Bare-command executable that isn't on PATH (npx/uvx missing) → the plugin installs but fails to connect at boot (preflight no longer warns for PATH commands; the MCP connect error shows in the plugin detail drawer's preflight/status) — documented (user needs Node/npx or uv/uvx). Sandbox: an npx server without declared network/fs perms would be blocked → catalog entries declare the perms they need.

## Testing

1. **PATH-command resolution**: registration builds `command == "npx"` (not `plugin_dir/npx`) for a manifest with `executable="npx"`; still joins plugin_dir for a relative file `executable="server.mjs"`; absolute stays absolute. Unit-test the resolution helper (extract it pure).
2. **catalog parse**: `builtin_catalog()` parses the bundled JSON → ≥6 entries, each with non-empty slug/command.
3. **install_from_catalog**: build manifest from an entry → toml round-trips → install lands `plugins/<slug>/plugin.toml` with executable=command + contributes.mcp_servers=[slug] + empty tools; unknown slug → NotFound. (Extract a pure `manifest_from_catalog(entry) -> PluginManifest` + test it; the Tauri command is a thin wrapper.)
4. **frontend**: PluginsSettings renders catalog cards (mock listCatalog) + install calls installPluginFromCatalog; tsc clean.
`cargo build`/clippy + `cargo test --lib plugins` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `plugins/registration.rs` | PATH-command executable resolution (extract `resolve_command` helper) |
| `plugins/runtime.rs` | preflight: skip "not exist" warning for bare-command executable |
| `plugins/catalog.rs` (new) + `plugins/catalog.json` (new) | `CatalogEntry` + `builtin_catalog()` + curated entries |
| `plugins/mod.rs` | `pub mod catalog;` |
| `tauri_commands.rs` + `main.rs` | `list_catalog` + `install_plugin_from_catalog` (+ `manifest_from_catalog` helper) |
| `ui/lib/types.ts` + `lib/tauri-bridge.ts` | `CatalogEntry` + `listCatalog`/`installPluginFromCatalog` |
| `ui/.../settings/PluginsSettings.tsx` | "插件市场" browse section (cards + 安装 + installed-dim) |

## Risk

Med. Backend (catalog + install + the registration PATH fix) + frontend browse. Main risks: (1) **PATH-command resolution** — the registration change must NOT break existing relative-file plugins (hello-uclaw `server.mjs` → still plugin_dir-joined); unit-test all 3 cases (absolute / relative-with-sep / bare-command). (2) **npx/uvx availability** — catalog servers need Node/uv installed; missing → connect fails at boot (shown in detail drawer); documented. (3) **sandbox vs npx** — entries declare network+fs perms so the sandbox allows npm-cache writes + network; without them the server is blocked → curate perms per entry. (4) **env-needing servers** — github etc. need API keys the floor env-scrub strips; v1 flags via setup_note (no injection); zero-config servers work immediately. (5) **catalog accuracy** — package names/commands must be correct (official `@modelcontextprotocol/server-*` are stable); a wrong name installs a plugin that fails to connect (visible in the drawer). (6) reuse `install_from_local_dir` (validated path) via a staging dir — don't hand-roll. Bisectable: PATH-fix+test → catalog+install backend → frontend → verify. After this slice, a user browses a curated marketplace in Settings and one-click-installs community MCP servers as managed, sandboxed plugins — closing the "discover + install community plugins" loop (the full 20k remote registry is a v2).
