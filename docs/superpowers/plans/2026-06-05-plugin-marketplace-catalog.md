# Plugin Marketplace Catalog Implementation Plan (Slice B)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** A bundled curated catalog of community MCP servers, browsable in Settings, one-click installable as managed plugins.

**Spec:** `docs/superpowers/specs/2026-06-05-plugin-marketplace-catalog-design.md`

---

## Pinned facts (verbatim)

- **registration.rs command-build** (mcp_servers block, ~line 130, per Phase-2 recon): `else if let Some(executable) = &loaded.manifest.runtime.executable { let exe_path = std::path::Path::new(executable); let command = if exe_path.is_absolute() { executable.clone() } else { loaded.plugin_dir.join(exe_path).to_string_lossy().to_string() }; … }`. Add the bare-command (PATH) middle case.
- **runtime.rs preflight** (runtime.rs:120-132): warns "runtime.executable does not exist yet" when `loaded.plugin_dir.join(exe_path)` doesn't exist. Skip this warning for a bare command.
- **install** (`plugins/install.rs`): `install_from_local_dir(src: &Path, plugins_root: &Path) -> Result<InstalledPlugin{id,display_name,version}, InstallError>`. `ensure_plugin_row(conn, id, now_ms)` (state.rs). `plugins_root = data_dir.join("plugins")`.
- `PluginManifest` (plugin_manifest/schema.rs) is `Serialize` → `toml::to_string(&manifest)` works. Fields: id, version, display_name, description: Option, author: PluginAuthor{name,email?,url?}, runtime: PluginRuntimeRequirement{min_uclaw_version, kind?, executable?, args, working_dir?}, permissions: PluginPermissions{network,filesystem_read,filesystem_write,memory_read,memory_write,run_subprocess,additional}, contributes: PluginContribution{mcp_servers,skills,commands,tools,themes}.
- `include_str!` precedent: `agent/mode_prompts.rs:18` `include_str!("prompts/baseline.md")`. So `include_str!("catalog.json")` next to catalog.rs.
- `list_plugins`/`install_plugin_from_dir` (tauri_commands.rs) + `InstalledPluginInfo { id, display_name, version, restart_required }` (Slice install). `Error::NotFound(String)`. `state.data_dir`, `state.db`.
- `main.rs` handler block `// Plugins (Pi-3b)` (~953) — add the 2 catalog commands.
- Frontend: `PluginsSettings.tsx` (install section + card grid + drawer — from UI v2). `listPlugins`/`PluginInfo` in tauri-bridge/types. `Input`/`Button`/`SettingsSection`/`SettingsCard` available. `Badge` from `@/components/ui/badge`.
- **NEW files**: `plugins/catalog.rs` + `plugins/catalog.json` need explicit `git add`.

---

## Task 1: PATH-command executable resolution

**Files:** Modify `plugins/registration.rs`, `plugins/runtime.rs`

- [ ] **Step 1: extract + extend `resolve_command`** — in `registration.rs`, add a pure helper + use it in the mcp_servers block:
```rust
/// Resolve a manifest `runtime.executable` to a spawnable command string.
/// - absolute path → as-is
/// - contains a path separator (e.g. "./server.mjs", "bin/x") → joined to plugin_dir
/// - bare command (e.g. "npx", "uvx", "python") → as-is (PATH lookup at spawn)
pub(crate) fn resolve_command(executable: &str, plugin_dir: &std::path::Path) -> String {
    let p = std::path::Path::new(executable);
    if p.is_absolute() {
        executable.to_string()
    } else if executable.contains('/') || executable.contains('\\') {
        plugin_dir.join(p).to_string_lossy().to_string()
    } else {
        executable.to_string() // bare command → PATH
    }
}
```
Replace the inline `let command = if exe_path.is_absolute() {…} else {…}` with `let command = resolve_command(executable, &loaded.plugin_dir);`.

- [ ] **Step 2: preflight — skip not-exist warning for bare command** (runtime.rs ~120-132): wrap the existence check so a bare command (no separator, not absolute) is NOT warned:
```rust
            if let Some(executable) = &manifest.runtime.executable {
                let exe_path = std::path::Path::new(executable);
                let is_bare = !exe_path.is_absolute() && !executable.contains('/') && !executable.contains('\\');
                if !is_bare {
                    let resolved = if exe_path.is_absolute() { exe_path.to_path_buf() } else { loaded.plugin_dir.join(exe_path) };
                    if !resolved.exists() {
                        findings.push(/* the existing Warning finding */);
                    }
                }
            }
```
(Keep the existing finding construction; just gate it behind `!is_bare`.)

- [ ] **Step 3: tests** (registration.rs tests):
```rust
#[test]
fn resolve_command_handles_absolute_relative_and_bare() {
    use std::path::Path;
    let dir = Path::new("/plugins/x");
    assert_eq!(resolve_command("/usr/bin/node", dir), "/usr/bin/node"); // absolute
    assert_eq!(resolve_command("server.mjs", dir), "/plugins/x/server.mjs"); // bare-file? NO sep → PATH? 
    // NOTE: "server.mjs" has no '/' → treated as PATH command. But existing plugins use "server.mjs"
    // as a FILE in the plugin dir. DECISION: a name ending in a known script ext OR containing '.'
    // is a file; truly-bare (npx) is PATH. See Step-4 note — adjust the rule.
}
```
**CRITICAL DECISION (resolve in implementation):** the example plugin uses `executable = "server.mjs"` (a FILE in the plugin dir, no separator). A naive "no '/' → PATH" rule would break it (→ PATH lookup of "server.mjs" fails). Fix the rule: **a bare command = no separator AND no file extension** (npx/uvx/python have no '.'; server.mjs/server.py have '.'). So: `is_bare = !absolute && !contains(sep) && !file_name_has_extension`. OR simpler + explicit: bare = no separator AND the joined `plugin_dir/<exe>` does NOT exist (then treat as PATH). The existence-probe rule is robust: `if absolute → exe; else { let joined = plugin_dir.join(exe); if joined.exists() || exe.contains(sep) → joined; else → exe (PATH) }`. **Use the existence-probe rule** — it keeps `server.mjs` (exists in dir) joined, and `npx` (doesn't exist in dir) as PATH. Update `resolve_command` accordingly + test:
```rust
pub(crate) fn resolve_command(executable: &str, plugin_dir: &std::path::Path) -> String {
    let p = std::path::Path::new(executable);
    if p.is_absolute() { return executable.to_string(); }
    let joined = plugin_dir.join(p);
    if joined.exists() || executable.contains('/') || executable.contains('\\') {
        joined.to_string_lossy().to_string()
    } else {
        executable.to_string() // not a file in the plugin dir + no separator → PATH command
    }
}
```
Test: a temp plugin_dir with a `server.mjs` file → `resolve_command("server.mjs", dir)` == `dir/server.mjs`; `resolve_command("npx", dir)` (no such file) == `"npx"`; absolute stays.

- [ ] **Step 4: build + test + commit**
`cd src-tauri && cargo test --lib resolve_command plugins::registration 2>&1 | tail` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/plugins/registration.rs src-tauri/src/plugins/runtime.rs
git commit -m "feat(plugins): resolve_command supports PATH-command executables (npx/uvx) for catalog plugins"
```

---

## Task 2: Catalog + install_plugin_from_catalog backend

**Files:** Create `plugins/catalog.rs` + `plugins/catalog.json`; modify `plugins/mod.rs`, `tauri_commands.rs`, `main.rs`

- [ ] **Step 1: `plugins/catalog.json`** (curated, official servers)
```json
[
  { "slug": "sequential-thinking", "name": "Sequential Thinking", "description": "Structured step-by-step reasoning scratchpad for the agent.", "category": "coding", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-sequential-thinking"], "permissions": { "run_subprocess": true }, "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "fetch", "name": "Fetch", "description": "Fetch a URL and return clean markdown.", "category": "web", "command": "uvx", "args": ["mcp-server-fetch"], "permissions": { "run_subprocess": true, "network": true, "filesystem_write": true }, "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "time", "name": "Time", "description": "Current time + timezone conversions.", "category": "utility", "command": "uvx", "args": ["mcp-server-time"], "permissions": { "run_subprocess": true }, "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "everything", "name": "Everything (demo)", "description": "Reference server exercising all MCP features — useful to test the plugin pipeline.", "category": "utility", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-everything"], "permissions": { "run_subprocess": true }, "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "memory", "name": "Knowledge Graph Memory", "description": "Local knowledge-graph memory (separate from uClaw's built-in memory).", "category": "utility", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-memory"], "permissions": { "run_subprocess": true, "filesystem_write": true }, "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "filesystem", "name": "Filesystem", "description": "Read/write files under an allowed root.", "category": "coding", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem"], "permissions": { "run_subprocess": true, "filesystem_read": true, "filesystem_write": true }, "setup_note": "Edit plugins/filesystem/plugin.toml to append your allowed root path(s) to runtime.args.", "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "git", "name": "Git", "description": "Inspect + operate on a git repository.", "category": "coding", "command": "uvx", "args": ["mcp-server-git"], "permissions": { "run_subprocess": true, "filesystem_read": true, "filesystem_write": true }, "setup_note": "Edit plugins/git/plugin.toml to add --repository <path> to runtime.args.", "homepage": "https://github.com/modelcontextprotocol/servers" },
  { "slug": "github", "name": "GitHub", "description": "Read issues/PRs, search code, trigger workflows.", "category": "coding", "command": "npx", "args": ["-y", "@modelcontextprotocol/server-github"], "permissions": { "run_subprocess": true, "network": true, "filesystem_write": true }, "env_hints": [{ "name": "GITHUB_PERSONAL_ACCESS_TOKEN", "description": "A GitHub PAT with repo scope." }], "setup_note": "Needs GITHUB_PERSONAL_ACCESS_TOKEN — env injection is not yet wired (v2); set it in your shell env that launches uClaw, or wait for env config.", "homepage": "https://github.com/modelcontextprotocol/servers" }
]
```

- [ ] **Step 2: `plugins/catalog.rs`** + `pub mod catalog;` in mod.rs
```rust
//! Pi-3b — bundled curated catalog of community MCP servers (the marketplace).
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CatalogPermissions {
    #[serde(default)] pub network: bool,
    #[serde(default)] pub filesystem_read: bool,
    #[serde(default)] pub filesystem_write: bool,
    #[serde(default)] pub run_subprocess: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvHint { pub name: String, pub description: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub slug: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub command: String,
    pub args: Vec<String>,
    #[serde(default)] pub permissions: CatalogPermissions,
    #[serde(default, skip_serializing_if = "Vec::is_empty")] pub env_hints: Vec<EnvHint>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub setup_note: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")] pub homepage: Option<String>,
}

pub fn builtin_catalog() -> Vec<CatalogEntry> {
    serde_json::from_str(include_str!("catalog.json")).unwrap_or_default()
}

/// Build a PluginManifest from a catalog entry. contributes.tools left empty →
/// tool_allowlist=None at registration → ALL the server's real tools (tools/list)
/// are exposed; no need to curate tool names.
pub fn manifest_from_catalog(e: &CatalogEntry) -> crate::plugin_manifest::schema::PluginManifest {
    use crate::plugin_manifest::schema::*;
    PluginManifest {
        id: e.slug.clone(),
        version: "0.0.0".to_string(),
        display_name: e.name.clone(),
        description: Some(e.description.clone()),
        author: PluginAuthor { name: "marketplace".into(), email: None, url: e.homepage.clone() },
        runtime: PluginRuntimeRequirement {
            min_uclaw_version: "0.1.0".into(),
            kind: Some("subprocess".into()),
            executable: Some(e.command.clone()),
            args: e.args.clone(),
            working_dir: None,
        },
        permissions: PluginPermissions {
            network: e.permissions.network,
            filesystem_read: e.permissions.filesystem_read,
            filesystem_write: e.permissions.filesystem_write,
            memory_read: false, memory_write: false,
            run_subprocess: e.permissions.run_subprocess,
            additional: vec![],
        },
        contributes: PluginContribution {
            mcp_servers: vec![e.slug.clone()],
            skills: vec![], commands: vec![], tools: vec![], themes: vec![],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn catalog_parses_and_has_entries() {
        let c = builtin_catalog();
        assert!(c.len() >= 6);
        for e in &c { assert!(!e.slug.is_empty() && !e.command.is_empty()); }
    }
    #[test] fn manifest_from_catalog_shapes_plugin() {
        let e = &builtin_catalog()[0];
        let m = manifest_from_catalog(e);
        assert_eq!(m.id, e.slug);
        assert_eq!(m.runtime.executable.as_deref(), Some(e.command.as_str()));
        assert_eq!(m.contributes.mcp_servers, vec![e.slug.clone()]);
        assert!(m.contributes.tools.is_empty());
        // toml round-trips
        let toml_str = toml::to_string(&m).unwrap();
        assert!(toml_str.contains(&format!("id = \"{}\"", e.slug)));
    }
}
```

- [ ] **Step 3: Tauri commands** (tauri_commands.rs)
```rust
#[tauri::command]
pub async fn list_catalog(_state: State<'_, AppState>) -> Result<Vec<crate::plugins::catalog::CatalogEntry>, Error> {
    Ok(crate::plugins::catalog::builtin_catalog())
}

#[tauri::command]
pub async fn install_plugin_from_catalog(state: State<'_, AppState>, slug: String) -> Result<InstalledPluginInfo, Error> {
    let entry = crate::plugins::catalog::builtin_catalog().into_iter().find(|e| e.slug == slug)
        .ok_or_else(|| Error::NotFound(format!("catalog entry '{slug}' not found")))?;
    let manifest = crate::plugins::catalog::manifest_from_catalog(&entry);
    let toml_str = toml::to_string(&manifest).map_err(|e| Error::Internal(format!("toml: {e}")))?;
    let plugins_root = state.data_dir.join("plugins");
    // stage <plugins_root>/.catalog-staging-<uuid>/<slug>/plugin.toml then install_from_local_dir
    let staging = plugins_root.join(format!(".catalog-staging-{}", uuid::Uuid::new_v4()));
    let staged_plugin = staging.join(&slug);
    std::fs::create_dir_all(&staged_plugin).map_err(|e| Error::Internal(e.to_string()))?;
    std::fs::write(staged_plugin.join("plugin.toml"), toml_str).map_err(|e| Error::Internal(e.to_string()))?;
    let res = crate::plugins::install::install_from_local_dir(&staged_plugin, &plugins_root)
        .map_err(|e| Error::InvalidInput(e.to_string()));
    let _ = std::fs::remove_dir_all(&staging); // clean staging regardless
    let p = res?;
    let now_ms = chrono::Utc::now().timestamp_millis();
    if let Ok(conn) = state.db.lock() { let _ = crate::plugins::state::ensure_plugin_row(&conn, &p.id, now_ms); }
    Ok(InstalledPluginInfo { id: p.id, display_name: p.display_name, version: p.version, restart_required: true })
}
```
- [ ] **Step 4: register in main.rs** (after install commands): `list_catalog`, `install_plugin_from_catalog`.
- [ ] **Step 5: build + test + commit**
`cargo test --lib catalog plugins 2>&1 | tail` green; `cargo build 2>&1 | grep -E "^error"` empty.
```bash
git add src-tauri/src/plugins/catalog.rs src-tauri/src/plugins/catalog.json src-tauri/src/plugins/mod.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(plugins): bundled marketplace catalog + install_plugin_from_catalog (generate plugin.toml from curated MCP servers)"
```

---

## Task 3: Frontend marketplace browse

**Files:** `ui/lib/types.ts`, `ui/lib/tauri-bridge.ts`, `ui/.../settings/PluginsSettings.tsx`

- [ ] **Step 1: types** (types.ts)
```ts
export interface CatalogEntry {
  slug: string; name: string; description: string; category: string
  command: string; args: string[]
  permissions: { network?: boolean; filesystem_read?: boolean; filesystem_write?: boolean; run_subprocess?: boolean }
  env_hints?: { name: string; description: string }[]
  setup_note?: string; homepage?: string
}
```
- [ ] **Step 2: bindings** (tauri-bridge.ts): add `CatalogEntry` to types import; `export const listCatalog = (): Promise<CatalogEntry[]> => invoke('list_catalog')`; `export const installPluginFromCatalog = (slug: string): Promise<InstalledPluginInfo> => invoke('install_plugin_from_catalog', { slug })`.
- [ ] **Step 3: PluginsSettings — 插件市场 section** (between 安装插件 and the installed grid):
  - state: `const [catalog, setCatalog] = useState<CatalogEntry[]|null>(null)`; in `refresh` (or a parallel effect) `setCatalog(await listCatalog())`.
  - `const installedSlugs = new Set((plugins ?? []).map(p => p.id))`.
  - render a `SettingsSection title="插件市场" description="一键安装社区精选的 MCP 插件（重启后激活）"` → a `grid grid-cols-2 gap-3` of catalog cards: each = name + category Badge + description + an 安装 Button (disabled + "已安装" if `installedSlugs.has(e.slug)`; if `e.setup_note`, show a small "需配置" hint). Install handler:
```tsx
  const onInstallCatalog = (e: CatalogEntry) => doInstall(async () => {
    const info = await installPluginFromCatalog(e.slug)
    if (e.setup_note) toast.info('需要额外配置', { description: e.setup_note })
    return info
  })
```
  (`doInstall` from Slice install already shows "已安装…重启激活" + refresh.)
- [ ] **Step 4: tsc + test + commit**
`cd ui && npx tsc --noEmit 2>&1 | grep -iE "PluginsSettings|tauri-bridge|types\.ts|Catalog" | head` → no new. `npm test -- --run PluginsSettings 2>&1 | tail` → green (update the test if it now needs a `listCatalog` mock — add `invoke('list_catalog')` → `[]` to the mock so the component renders).
```bash
git add ui/src/lib/types.ts ui/src/lib/tauri-bridge.ts ui/src/components/settings/PluginsSettings.tsx
git commit -m "feat(ui): plugin marketplace browse section (listCatalog + one-click install)"
```

---

## Task 4: Whole-slice verify + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean; `cargo test --lib plugins catalog` green.
- [ ] **Step 2**: `cd ui && npx tsc --noEmit` (no new) + `npm test -- --run` (no new failures).
- [ ] **Step 3**: grep gates — `resolve_command` 3-case; `pub mod catalog`; `list_catalog`+`install_plugin_from_catalog` in main.rs; catalog.json ≥6 entries; frontend 市场 section.
- [ ] **Step 4 (optional soak)**: confirm `manifest_from_catalog` → toml → install round-trips (covered by the unit test).
- [ ] **Step 5**: PR with `## Commits (bisectable)` table. Note: PATH-command executables (npx/uvx); catalog = generate plugin.toml (tools empty→expose all discovered); sandbox relaxed per declared perms; env-needing servers carry setup_note (env injection = v2); remote 20k registry = v2.
- [ ] **Step 6**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: marketplace catalog shipped — plugin system end-to-end complete: discover/install(git/folder/catalog/agent-authored)/enable-disable/sandbox/contribute tools+skills+commands/inspect; remaining = remote registry, env config, sandbox v2).

---

## Self-Review

**Spec coverage:** §1 PATH-fix → T1; §2 catalog → T2; §3 commands → T2; §4 frontend → T3. ✓
**Placeholder scan:** the resolve_command rule has a flagged DECISION (existence-probe rule, resolved in T1 Step 3) — concrete, not a TODO. ✓
**Type consistency:** `CatalogEntry`/`CatalogPermissions`/`EnvHint` (Rust serde snake) ↔ TS interface; `manifest_from_catalog` → PluginManifest (tools empty → tool_allowlist None → expose all); `install_plugin_from_catalog → InstalledPluginInfo` (reuses Slice-install type); reuses `install_from_local_dir` via staging. ✓
**Critical: resolve_command must NOT break existing relative-file plugins** — existence-probe rule keeps `server.mjs` (exists) joined, `npx` (absent) as PATH; unit-tested all cases. ✓
**Sandbox/env honesty:** entries declare perms (sandbox relaxes accordingly); env-needing flagged via setup_note (no injection v1). ✓
**New-file safety:** T2+T4 verify catalog.rs + catalog.json in `git show --stat`. ✓
