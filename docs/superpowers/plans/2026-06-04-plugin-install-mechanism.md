# Plugin Install Mechanism Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`).

**Goal:** Install a plugin from a git URL (or local folder) into `$DATA_DIR/plugins/<id>/` via the Settings UI; it activates on next boot (restart-to-activate).

**Spec:** `docs/superpowers/specs/2026-06-04-plugin-install-mechanism-design.md`

---

## Pinned facts (verbatim)

- **git subprocess precedent** (`tauri_commands_git.rs:279`): `tokio::process::Command::new("gh").args(&args).output().await … if !output.status.success() { let stderr = String::from_utf8_lossy(&output.stderr)… }`. Use the same for `git`.
- `Cargo.toml`: `walkdir = "2"` (105), `toml = "0.8"` (102), `uuid = { version="1", features=["v4","serde"] }` (94). NO `fs_extra` (write recursive copy inline).
- `PluginManifest` (plugin_manifest/schema.rs:122): `{ id: String, version: String, display_name: String, description: Option<String>, author, runtime, permissions, contributes }`. Parse: `toml::from_str::<PluginManifest>(&body)`.
- `PluginDiscovery::load_manifest` (discovery.rs:90) enforces `manifest.id == dir_name` → the install target dir MUST be named the manifest id.
- `plugins/state.rs`: `pub fn ensure_plugin_row(conn: &Connection, id: &str, now_ms: i64) -> rusqlite::Result<()>` (23).
- `plugins/mod.rs` (12-18): `pub mod discovery; pub mod lifecycle; pub mod registration; pub mod runtime; pub mod sandbox; pub mod state; pub mod uclaw_extension;` — add `pub mod install;`.
- `AppState` (app.rs:157): `pub data_dir: PathBuf`, `pub db: Arc<std::sync::Mutex<rusqlite::Connection>>`.
- `list_plugins` (tauri_commands.rs): `let plugins_root = state.data_dir.join("plugins");` … returns `Result<_, Error>`. `now_ms = chrono::Utc::now().timestamp_millis()`. DB lock: `state.db.lock().map_err(|e| Error::Internal(format!("db lock: {e}")))?`.
- `main.rs` handler (~645): `// Plugins (Pi-3b)\n  uclaw_core::tauri_commands::list_plugins,\n  uclaw_core::tauri_commands::set_plugin_enabled,\n  uclaw_core::tauri_commands::list_commands,` — add the 2 install commands here.
- `error.rs`: `Error::Internal(String)`, `Error::Database(#[from] rusqlite::Error)`, `Error::Io(#[from] std::io::Error)`, `Error::InvalidInput(String)`.
- Tauri casing: Rust `git_url` → JS `invoke('install_plugin_from_git', { gitUrl })`; `dir_path` → `{ dirPath }` (camelCase keys auto-convert).
- Frontend: `@tauri-apps/plugin-dialog` `^2.7.1` available; `import { open as openDialog } from '@tauri-apps/plugin-dialog'`; `await openDialog({ directory: true, multiple: false })` → `string | null`. `Input` from `@/components/ui/input`, `Button` from `@/components/ui/button`. `PluginsSettings.tsx` full structure pinned in the spec recon (imports SettingsSection/Card/Row, Switch, listPlugins/setPluginEnabled, toast).
- **NEW files `plugins/install.rs` need explicit `git add`.**

---

## Task 1: `plugins/install.rs` (install logic + tests)

**Files:** Create `plugins/install.rs`; modify `plugins/mod.rs`

- [ ] **Step 1: `pub mod install;`** in `plugins/mod.rs`.

- [ ] **Step 2: create `plugins/install.rs`**
```rust
//! Pi-3b — install a plugin (git clone or local-dir copy) into
//! `$DATA_DIR/plugins/<id>/`. The plugin activates on next boot (registration is
//! boot-only). Files only — nothing is executed here; the subprocess runs
//! sandboxed (#669) at boot.

use std::path::Path;

use crate::plugin_manifest::schema::PluginManifest;

#[derive(Debug, Clone, serde::Serialize)]
pub struct InstalledPlugin {
    pub id: String,
    pub display_name: String,
    pub version: String,
}

#[derive(Debug, thiserror::Error)]
pub enum InstallError {
    #[error("plugin '{0}' is already installed")]
    AlreadyInstalled(String),
    #[error("git clone failed: {0}")]
    GitFailed(String),
    #[error("plugin.toml not found in source")]
    ManifestMissing,
    #[error("invalid plugin.toml: {0}")]
    ManifestInvalid(String),
    #[error("io error: {0}")]
    Io(String),
}

/// Validate a source dir contains a parseable plugin.toml with a safe id.
fn validate_manifest_dir(dir: &Path) -> Result<PluginManifest, InstallError> {
    let manifest_path = dir.join("plugin.toml");
    if !manifest_path.exists() {
        return Err(InstallError::ManifestMissing);
    }
    let body = std::fs::read_to_string(&manifest_path).map_err(|e| InstallError::Io(e.to_string()))?;
    let manifest: PluginManifest =
        toml::from_str(&body).map_err(|e| InstallError::ManifestInvalid(e.to_string()))?;
    let id = manifest.id.trim();
    if id.is_empty() {
        return Err(InstallError::ManifestInvalid("empty id".into()));
    }
    // The id becomes a directory name under plugins/ — reject path traversal.
    if id.contains('/') || id.contains('\\') || id.contains("..") {
        return Err(InstallError::ManifestInvalid(format!("unsafe id: {id}")));
    }
    Ok(manifest)
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in walkdir::WalkDir::new(src) {
        let entry = entry.map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;
        let rel = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dst.join(rel);
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

fn finish(manifest: PluginManifest) -> InstalledPlugin {
    InstalledPlugin { id: manifest.id, display_name: manifest.display_name, version: manifest.version }
}

/// Copy a local plugin dir (containing plugin.toml) into plugins/<id>/.
pub fn install_from_local_dir(src: &Path, plugins_root: &Path) -> Result<InstalledPlugin, InstallError> {
    let manifest = validate_manifest_dir(src)?;
    let target = plugins_root.join(&manifest.id);
    if target.exists() {
        return Err(InstallError::AlreadyInstalled(manifest.id));
    }
    std::fs::create_dir_all(plugins_root).map_err(|e| InstallError::Io(e.to_string()))?;
    copy_dir_recursive(src, &target).map_err(|e| InstallError::Io(e.to_string()))?;
    Ok(finish(manifest))
}

/// git clone (repo root = the plugin) into a staging dir, validate, promote to
/// plugins/<id>/. Cleans up staging on any failure.
pub async fn install_from_git(git_url: &str, plugins_root: &Path) -> Result<InstalledPlugin, InstallError> {
    std::fs::create_dir_all(plugins_root).map_err(|e| InstallError::Io(e.to_string()))?;
    let staging = plugins_root.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    let output = tokio::process::Command::new("git")
        .arg("clone")
        .arg("--depth")
        .arg("1")
        .arg(git_url)
        .arg(&staging)
        .output()
        .await
        .map_err(|e| InstallError::GitFailed(format!("git unavailable: {e}")))?;
    if !output.status.success() {
        let _ = std::fs::remove_dir_all(&staging);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let tail = stderr.lines().rev().find(|l| !l.trim().is_empty()).unwrap_or("clone failed");
        return Err(InstallError::GitFailed(tail.to_string()));
    }
    let manifest = match validate_manifest_dir(&staging) {
        Ok(m) => m,
        Err(e) => { let _ = std::fs::remove_dir_all(&staging); return Err(e); }
    };
    let target = plugins_root.join(&manifest.id);
    if target.exists() {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(InstallError::AlreadyInstalled(manifest.id));
    }
    let _ = std::fs::remove_dir_all(staging.join(".git")); // drop clone history (best-effort)
    std::fs::rename(&staging, &target).map_err(|e| {
        let _ = std::fs::remove_dir_all(&staging);
        InstallError::Io(e.to_string())
    })?;
    Ok(finish(manifest))
}
```

- [ ] **Step 3: tests** (in install.rs)
```rust
#[cfg(test)]
mod tests {
    use super::*;
    fn write_plugin(dir: &Path, id: &str) {
        std::fs::create_dir_all(dir).unwrap();
        std::fs::write(dir.join("plugin.toml"), format!(
            "id = \"{id}\"\nversion = \"0.1.0\"\ndisplay_name = \"Demo\"\n\n[author]\nname = \"t\"\n\n[runtime]\nmin_uclaw_version = \"0.1.0\"\n"
        )).unwrap();
        std::fs::write(dir.join("server.mjs"), "// demo").unwrap();
    }
    #[test]
    fn local_install_copies_and_returns_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        write_plugin(&src, "demo");
        let root = tmp.path().join("plugins");
        let info = install_from_local_dir(&src, &root).unwrap();
        assert_eq!(info.id, "demo");
        assert!(root.join("demo/plugin.toml").exists());
        assert!(root.join("demo/server.mjs").exists());
    }
    #[test]
    fn local_install_rejects_already_installed() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src"); write_plugin(&src, "demo");
        let root = tmp.path().join("plugins");
        install_from_local_dir(&src, &root).unwrap();
        let err = install_from_local_dir(&src, &root).unwrap_err();
        assert!(matches!(err, InstallError::AlreadyInstalled(_)));
    }
    #[test]
    fn local_install_rejects_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src"); std::fs::create_dir_all(&src).unwrap();
        let root = tmp.path().join("plugins");
        assert!(matches!(install_from_local_dir(&src, &root).unwrap_err(), InstallError::ManifestMissing));
        assert!(!root.join("demo").exists());
    }
    #[test]
    fn rejects_unsafe_id() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("plugin.toml"), "id = \"../evil\"\nversion=\"0.1.0\"\ndisplay_name=\"x\"\n[author]\nname=\"t\"\n[runtime]\nmin_uclaw_version=\"0.1.0\"\n").unwrap();
        let root = tmp.path().join("plugins");
        assert!(matches!(install_from_local_dir(&src, &root).unwrap_err(), InstallError::ManifestInvalid(_)));
    }
    #[tokio::test]
    async fn git_install_from_local_file_repo() {
        // Skip if git is unavailable.
        if std::process::Command::new("git").arg("--version").output().is_err() { return; }
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        write_plugin(&repo, "gitdemo");
        for args in [vec!["init","-q"], vec!["add","-A"], vec!["-c","user.email=t@t","-c","user.name=t","commit","-qm","x"]] {
            let ok = std::process::Command::new("git").current_dir(&repo).args(&args).output().unwrap().status.success();
            if !ok { return; } // env without git identity → skip
        }
        let root = tmp.path().join("plugins");
        let url = format!("file://{}", repo.display());
        let info = install_from_git(&url, &root).await.unwrap();
        assert_eq!(info.id, "gitdemo");
        assert!(root.join("gitdemo/plugin.toml").exists());
        assert!(!root.join("gitdemo/.git").exists()); // history dropped
        // no leftover staging dirs
        let staging = std::fs::read_dir(&root).unwrap().filter_map(|e| e.ok()).filter(|e| e.file_name().to_string_lossy().starts_with(".staging-")).count();
        assert_eq!(staging, 0);
    }
}
```

- [ ] **Step 4: build + test + commit**
`cd src-tauri && cargo test --lib plugins::install 2>&1 | tail` → green. `cargo build 2>&1 | grep -E "^error"` → empty.
```bash
git add src-tauri/src/plugins/install.rs src-tauri/src/plugins/mod.rs
git commit -m "feat(plugins): install_from_git + install_from_local_dir (staging/promote, path-safe id)"
```
Verify `git show HEAD --stat` lists `plugins/install.rs`.

---

## Task 2: Tauri commands + bridge bindings

**Files:** Modify `tauri_commands.rs`, `main.rs`, `ui/src/lib/tauri-bridge.ts`, `ui/src/lib/types.ts`

- [ ] **Step 1: Tauri commands** (tauri_commands.rs, near `list_plugins`)
```rust
#[derive(serde::Serialize)]
pub struct InstalledPluginInfo {
    pub id: String,
    pub display_name: String,
    pub version: String,
    pub restart_required: bool,
}

fn ensure_installed_row(state: &AppState, id: &str) -> Result<(), Error> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("db lock: {e}")))?;
    crate::plugins::state::ensure_plugin_row(&conn, id, now_ms).map_err(Error::Database)
}

#[tauri::command]
pub async fn install_plugin_from_git(state: State<'_, AppState>, git_url: String) -> Result<InstalledPluginInfo, Error> {
    let plugins_root = state.data_dir.join("plugins");
    let p = crate::plugins::install::install_from_git(&git_url, &plugins_root)
        .await
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    ensure_installed_row(&state, &p.id)?;
    Ok(InstalledPluginInfo { id: p.id, display_name: p.display_name, version: p.version, restart_required: true })
}

#[tauri::command]
pub async fn install_plugin_from_dir(state: State<'_, AppState>, dir_path: String) -> Result<InstalledPluginInfo, Error> {
    let plugins_root = state.data_dir.join("plugins");
    let p = crate::plugins::install::install_from_local_dir(std::path::Path::new(&dir_path), &plugins_root)
        .map_err(|e| Error::InvalidInput(e.to_string()))?;
    ensure_installed_row(&state, &p.id)?;
    Ok(InstalledPluginInfo { id: p.id, display_name: p.display_name, version: p.version, restart_required: true })
}
```

- [ ] **Step 2: register in main.rs** — after `uclaw_core::tauri_commands::list_commands,`:
```rust
            uclaw_core::tauri_commands::install_plugin_from_git,
            uclaw_core::tauri_commands::install_plugin_from_dir,
```

- [ ] **Step 3: frontend bindings** — `ui/src/lib/types.ts`:
```ts
export interface InstalledPluginInfo {
  id: string
  display_name: string
  version: string
  restart_required: boolean
}
```
`ui/src/lib/tauri-bridge.ts` (near listPlugins; add `InstalledPluginInfo` to the types import):
```ts
export const installPluginFromGit = (gitUrl: string): Promise<InstalledPluginInfo> =>
  invoke('install_plugin_from_git', { gitUrl })
export const installPluginFromDir = (dirPath: string): Promise<InstalledPluginInfo> =>
  invoke('install_plugin_from_dir', { dirPath })
```

- [ ] **Step 4: build + commit**
`cargo build 2>&1 | grep -E "^error"` empty; `cargo test --lib plugins 2>&1 | tail -3` green; `cd ui && npx tsc --noEmit 2>&1 | grep -iE "tauri-bridge|types.ts|Installed" | head` (no new).
```bash
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs ui/src/lib/tauri-bridge.ts ui/src/lib/types.ts
git commit -m "feat(plugins): install_plugin_from_git/dir Tauri commands + bridge bindings"
```

---

## Task 3: Frontend install affordance (PluginsSettings)

**Files:** Modify `ui/src/components/settings/PluginsSettings.tsx`

- [ ] **Step 1: imports** — add `Input` (`@/components/ui/input`), `Button` (`@/components/ui/button`), `open as openDialog` (`@tauri-apps/plugin-dialog`), `installPluginFromGit`/`installPluginFromDir` (tauri-bridge), `useState` already imported.
- [ ] **Step 2: state + handlers** in the component:
```tsx
  const [gitUrl, setGitUrl] = useState('')
  const [installing, setInstalling] = useState(false)

  const doInstall = async (fn: () => Promise<{ display_name: string }>) => {
    setInstalling(true)
    try {
      const info = await fn()
      toast.success(`已安装 ${info.display_name}`, { description: '重启应用以激活其工具 / 命令' })
      setGitUrl('')
      await refresh()
    } catch (e) {
      toast.error('安装失败', { description: String(e) })
    } finally {
      setInstalling(false)
    }
  }

  const onInstallGit = () => {
    const url = gitUrl.trim()
    if (!url) return
    void doInstall(() => installPluginFromGit(url))
  }
  const onInstallDir = async () => {
    const dir = await openDialog({ directory: true, multiple: false })
    if (!dir || typeof dir !== 'string') return
    void doInstall(() => installPluginFromDir(dir))
  }
```
- [ ] **Step 3: install UI** — add an install `SettingsSection` (or a block) ABOVE the existing list section:
```tsx
      <SettingsSection title="安装插件" description="从 git 仓库或本地文件夹安装（重启后激活）">
        <SettingsCard>
          <div className="flex items-center gap-2 px-4 py-3.5">
            <Input
              value={gitUrl}
              onChange={(e) => setGitUrl(e.target.value)}
              placeholder="git 仓库 URL，例如 https://github.com/user/plugin.git"
              disabled={installing}
              onKeyDown={(e) => { if (e.key === 'Enter') onInstallGit() }}
            />
            <Button onClick={onInstallGit} disabled={installing || !gitUrl.trim()}>安装</Button>
            <Button variant="outline" onClick={onInstallDir} disabled={installing}>从文件夹导入</Button>
          </div>
        </SettingsCard>
      </SettingsSection>
```
(Wrap the two SettingsSection blocks in a fragment `<>…</>` since the component currently returns a single SettingsSection.)
- [ ] **Step 4: tsc + test + commit**
`cd ui && npx tsc --noEmit 2>&1 | grep -iE "PluginsSettings" | head` → no new. `cd ui && npm test -- --run PluginsSettings 2>&1 | tail` → green (existing tests still pass; if a test asserts the single-section structure, update it).
```bash
git add ui/src/components/settings/PluginsSettings.tsx
git commit -m "feat(ui): install plugin from git URL / local folder in PluginsSettings (restart-to-activate)"
```

---

## Task 4: Whole-slice verify + E2E + ship

- [ ] **Step 1**: `cargo build` + `cargo clippy --lib` clean; `cargo test --lib plugins` green (incl. install tests).
- [ ] **Step 2**: `cd ui && npx tsc --noEmit` (no new) + `npm test -- --run` (no new failures).
- [ ] **Step 3: E2E soak (document)** — make a temp git repo from the `examples/plugins/hello-uclaw` dir (`git init` + commit), `install_from_git("file://…")` into a temp plugins root, assert `plugins/hello-uclaw/plugin.toml` exists + no `.git` + no staging leftovers. (The git unit test in T1 already covers this with a synthetic plugin; reuse/confirm.) Report result.
- [ ] **Step 4**: grep gates — `pub mod install` in plugins/mod.rs; both install commands in main.rs handler; path-traversal guard on id present; staging cleanup on every error branch.
- [ ] **Step 5**: PR with `## Commits (bisectable)` table. Note: git-clone + local-dir sources; restart-to-activate (boot-frozen AgentApi); path-safe id; staging/promote atomicity; sandbox (#669) mitigates runtime supply-chain; marketplace browse + UI v2 = next slice.
- [ ] **Step 6**: rebase onto latest origin/main, rebase-merge, sync main, cleanup, reindex, update memory ([[project-pi-lightweight-vs-agent-os]]: plugin install shipped; next = marketplace browse + UI v2, sandbox v2).

---

## Self-Review

**Spec coverage:** §1 install.rs → T1; §2 Tauri → T2; §3 frontend → T3. ✓
**Placeholder scan:** the git test "skip if git/identity unavailable" + the PluginsSettings "update test if it asserts single-section" are flagged fallbacks, not TODOs. ✓
**Type consistency:** `InstalledPlugin{id,display_name,version}` (Rust) → `InstalledPluginInfo{…,restart_required}` (Tauri) → TS `InstalledPluginInfo`; install fns return `Result<_, InstallError>` mapped to `Error::InvalidInput`; bridge `{ gitUrl }`/`{ dirPath }` (camelCase auto-convert). ✓
**Security:** path-traversal guard on manifest id (id → dir name); install copies/clones only (no exec); staging cleaned on all failure branches; reject-if-exists (no clobber). ✓
**Restart-to-activate:** communicated via the success toast. ✓
**New-file safety:** T1+T4 verify `plugins/install.rs` in `git show --stat`. ✓
