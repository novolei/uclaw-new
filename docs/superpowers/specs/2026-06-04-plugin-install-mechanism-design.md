# Plugin Install Mechanism Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** Pi-convergence Phase 3b plugin system. Lets a user ADD a new plugin (install its files into `$DATA_DIR/plugins/<id>/`) so it's discovered/registered on next boot. After lifecycle (#667), skills (#668), sandbox (#669), management UI (#672), command dispatch (#673/#674). The marketplace BROWSE experience + rich UI v2 is the follow-up slice.

## Problem

Plugins must be manually dropped into `$DATA_DIR/plugins/<id>/`. There's no install flow. Discovery (`PluginDiscovery`) scans that dir; a plugin dir = `plugin.toml` (manifest, `id` must equal the dir name) + the runtime executable. Registration is **boot-only** (`PluginLifecycleOwner::connect_and_register` runs once in `AppState::new`; `AgentApi` is frozen `Arc` afterward), so a freshly-installed plugin's tools/skills/commands activate only after an app restart.

## Decision (approved 2026-06-04)

- **Install source: `git clone` from a URL** (primary) + **local directory import** (secondary). `git` via `std::process::Command::new("git")` — no new Rust dependency (git is present on the user's macOS); local copy via `std::fs` + `walkdir` (already a dep). No archive deps (tar/zip) — those (HTTP tarball) are a future source.
- **Restart-to-activate (v1)**: install writes the files + ensures the DB row; tools/commands/skills activate on next boot (inherent to the boot-frozen `AgentApi`). The UI communicates this ("已安装，重启应用以激活"). Runtime hot-reload is a separate, larger effort (out of scope).
- **Marketplace browse + rich UI = next slice** (UI v2). This slice ships the install MECHANISM + a minimal install affordance in the existing `PluginsSettings`.

## Design

### §1 `plugins/install.rs` (new module)
```rust
pub struct InstalledPlugin { pub id: String, pub display_name: String, pub version: String }

pub enum InstallError { /* AlreadyInstalled(id), GitFailed(String), ManifestMissing, ManifestInvalid(String), Io(String) */ }

/// Clone a git repo (repo root = the plugin: plugin.toml at root) into a staging
/// dir, validate + read the manifest, then promote to plugins/<manifest.id>/.
/// Rejects if plugins/<id> already exists. Returns the installed manifest summary.
pub fn install_from_git(git_url: &str, plugins_root: &Path) -> Result<InstalledPlugin, InstallError>;

/// Validate + recursively copy a local plugin dir (containing plugin.toml) into
/// plugins/<manifest.id>/. Rejects if already installed.
pub fn install_from_local_dir(src: &Path, plugins_root: &Path) -> Result<InstalledPlugin, InstallError>;
```
- **git**: clone to a unique staging dir under `plugins_root` (e.g. `plugins/.staging-<uuid>/`) via `Command::new("git").args(["clone","--depth","1",url,staging])`; on success read `staging/plugin.toml`, parse `PluginManifest`, validate `id` non-empty; if `plugins/<id>` exists → `AlreadyInstalled` (clean up staging); else `std::fs::rename(staging, plugins/<id>)` (promote). On any failure remove staging.
- **local**: validate `src/plugin.toml` exists + parses; read id; reject if `plugins/<id>` exists; recursively copy `src` → `plugins/<id>` (`walkdir` + `std::fs::create_dir_all`/`copy`).
- **validation** shared: parse the manifest (`toml::from_str::<PluginManifest>`), require non-empty `id`. (Discovery later requires `id == dir_name`, which holds because we name the dir after the manifest id.)
- **security note**: installing executes nothing (clone/copy only); the plugin's subprocess runs sandboxed (#669) at boot. Supply-chain trust is the user's (they chose the URL/dir). v1 doesn't fetch the plugin's own deps (node_modules etc.) — plugins must be self-contained.

### §2 Tauri commands (tauri_commands.rs + main.rs)
```rust
#[derive(serde::Serialize)]
pub struct InstalledPluginInfo { pub id: String, pub display_name: String, pub version: String, pub restart_required: bool }

#[tauri::command]
pub async fn install_plugin_from_git(state: State<'_, AppState>, git_url: String) -> Result<InstalledPluginInfo, Error>;

#[tauri::command]
pub async fn install_plugin_from_dir(state: State<'_, AppState>, dir_path: String) -> Result<InstalledPluginInfo, Error>;
```
Each: resolve `plugins_root = state.data_dir.join("plugins")`; call the install fn (map `InstallError` → `Error`); `ensure_plugin_row(&conn, &id, now_ms)` (so it appears enabled in `list_plugins`); return `InstalledPluginInfo { …, restart_required: true }`. Register both in `main.rs` `generate_handler!` (near `list_plugins`).

### §3 Frontend install affordance (PluginsSettings.tsx + tauri-bridge.ts + types.ts)
- `tauri-bridge.ts`: `installPluginFromGit(gitUrl)` + `installPluginFromDir(dirPath)` bindings; `InstalledPluginInfo` type.
- `PluginsSettings.tsx`: add an "安装插件" section above the list — a git-URL text input + an 安装 button (calls `installPluginFromGit`), and a "从文件夹导入" button that opens the Tauri dialog (`@tauri-apps/plugin-dialog` `open({ directory: true })`) then calls `installPluginFromDir`. On success: `toast.success('已安装 ' + info.display_name, { description: '重启应用以激活其工具 / 命令' })` + `refresh()` (the new plugin appears in the list, MCP 未连接 until restart). On error: `toast.error`. Disable the button while installing.

## Data flow

```
user: 安装插件 (git URL or 文件夹) → install_plugin_from_git/dir
  → install.rs: git clone/copy → staging → validate plugin.toml → promote to plugins/<id>/
  → ensure_plugin_row(id) → return { id, display_name, version, restart_required: true }
  → frontend: toast "已安装，重启以激活" + refresh list (new plugin shown, 未连接)
next boot: PluginDiscovery finds plugins/<id>/ → connect_and_register → tools/skills/commands live + MCP connects
```

## Out of scope

Marketplace browse / remote registry index (next slice = UI v2 + marketplace); HTTP tarball source (needs archive deps); runtime hot-reload (boot-frozen AgentApi); installing the plugin's own dependencies (node_modules / pip); uninstall (could be a quick follow — delete dir + DB row); plugin signing/verification; upgrade-in-place (reject-if-exists for v1); a git subdir/monorepo plugin (repo root = plugin for v1).

## Error handling

`AlreadyInstalled` → user-facing "插件 <id> 已安装" (no overwrite). `git clone` non-zero exit / git absent → `GitFailed` with stderr tail (toast). `ManifestMissing`/`ManifestInvalid` → reject + clean up staging (no partial install). Local dir without plugin.toml → reject. All failures clean up the staging dir (no orphan dirs). `ensure_plugin_row` DB error → return Err (install files succeeded but DB row failed — log; the plugin still discovers on boot + defaults enabled, so non-fatal — but surface the error). Staging dir uses a unique name to avoid concurrent-install collisions.

## Testing

1. **install_from_local_dir**: a tempdir src with a valid `plugin.toml` (id="demo") → copied to `plugins/demo/plugin.toml`; returns `{id:"demo",…}`. Reject-if-exists: second call → `AlreadyInstalled`. Missing/invalid plugin.toml → error + no `plugins/demo` created.
2. **manifest validation**: a src with malformed toml → `ManifestInvalid`; empty id → error.
3. **staging cleanup**: a failed install leaves no staging dir (assert plugins_root has no `.staging-*`).
4. **git** (harder — needs git + a repo): unit-test the local path + validation thoroughly; the git path is covered by an E2E soak (clone a tiny local git repo OR the example plugin turned into a git repo) — document. Optionally a test using a `file://` local git repo if git is available in the test env.
5. **frontend**: PluginsSettings renders the install section; clicking 安装 with a URL calls `installPluginFromGit` (mock) + shows the restart toast; `tsc --noEmit` clean.
`cargo build`/clippy + `cargo test --lib plugins` + `cd ui && npx tsc --noEmit` + vitest.

## Scope / files

| File | Change |
|---|---|
| `plugins/install.rs` (new) | `install_from_git` + `install_from_local_dir` + validation + `InstalledPlugin`/`InstallError` + tests |
| `plugins/mod.rs` | `pub mod install;` |
| `tauri_commands.rs` + `main.rs` | `install_plugin_from_git` + `install_plugin_from_dir` + `InstalledPluginInfo` + invoke_handler |
| `ui/.../PluginsSettings.tsx`, `lib/tauri-bridge.ts`, `lib/types.ts` | install section (git URL + 文件夹 dialog) + bindings + type |

## Risk

Med. New install surface (writes to the plugins dir) + a `git` subprocess + frontend. Main risks: (1) **staging/promote atomicity** — clone/copy to a unique staging dir, validate, then `rename` to `plugins/<id>`; clean up staging on ANY failure (no partial/orphan dirs); reject-if-exists prevents clobbering. (2) **id == dir_name invariant** — the dir is named after the manifest id, satisfying discovery's validation. (3) **git availability/cross-platform** — `Command::new("git")`; on macOS (target) git is present; failure → clear `GitFailed` error (not a panic). (4) **restart-to-activate UX** — must be communicated (toast) so users aren't confused why the new plugin's tools aren't live. (5) **two-edit Tauri** — register both commands in main.rs. (6) `@tauri-apps/plugin-dialog` must be available (recon confirms a dialog plugin) — if not, ship git-URL-only + defer the folder picker. (7) supply-chain — installing arbitrary code; mitigated at runtime by the #669 sandbox; install is user-initiated trust. Bisectable: install.rs+tests → Tauri+bridge → frontend → verify+E2E. After this slice, a user can install a plugin from a git URL or a local folder via the Settings UI; it activates on restart — the plugin ecosystem becomes user-extensible (the marketplace browse + rich UI follow in the next slice).
