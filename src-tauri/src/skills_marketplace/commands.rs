//! Tauri commands for the skills marketplace. Errors are returned as plain
//! Chinese strings so the UI can show them as-is.

use std::path::{Path, PathBuf};

use rusqlite::{Connection, OptionalExtension as _};

use super::install::{self, flatten_slug, global_root, workspace_root};
use super::{InstallScope, MarketplaceError, MarketplaceProvider, SkillDetail};
use crate::skills::{SkillProvenance, SkillsRegistry};

/// `settings` key holding a provider's API key.
pub(crate) fn api_key_setting(provider: MarketplaceProvider) -> &'static str {
    match provider {
        MarketplaceProvider::SkillsSh => "skills_sh_api_key",
        MarketplaceProvider::Skillsmp => "skillsmp_api_key",
    }
}

/// The stored API key for `provider`, if any (never sent to the frontend).
pub(crate) fn read_api_key(conn: &Connection, provider: MarketplaceProvider) -> Option<String> {
    conn.query_row(
        "SELECT value FROM settings WHERE key = ?1",
        [api_key_setting(provider)],
        |r| r.get::<_, String>(0),
    )
    .optional()
    .ok()
    .flatten()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
}

/// `spaces.path` for a workspace id; error when missing or empty.
pub(crate) fn workspace_path(conn: &Connection, workspace_id: &str) -> Result<PathBuf, MarketplaceError> {
    let path: Option<String> = conn
        .query_row("SELECT path FROM spaces WHERE id = ?1", [workspace_id], |r| r.get(0))
        .optional()
        .map_err(|e| MarketplaceError::Install(e.to_string()))?
        .flatten();
    path.map(|p| p.trim().to_string())
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| MarketplaceError::Invalid("该工作区没有设置文件夹路径，无法安装到工作区".to_string()))
}

/// Every non-empty `spaces.path`.
pub(crate) fn all_workspace_paths(conn: &Connection) -> Vec<PathBuf> {
    let Ok(mut stmt) = conn.prepare("SELECT path FROM spaces WHERE path IS NOT NULL AND TRIM(path) != ''") else {
        return Vec::new();
    };
    stmt.query_map([], |r| r.get::<_, String>(0))
        .map(|rows| rows.filter_map(Result::ok).map(PathBuf::from).collect())
        .unwrap_or_default()
}

/// What to fetch for a UI row without a provider detail endpoint:
/// `(source, skill_id)` for [`super::github::resolve_source`]. A GitHub URL
/// is used as-is; otherwise `owner/repo` plus the id's last segment, since
/// skills.sh ids are not always the real directory path.
pub(crate) fn detail_source(id: &str, source: Option<&str>) -> Result<(String, Option<String>), MarketplaceError> {
    let src = source.map(str::trim).filter(|s| !s.is_empty()).unwrap_or(id);
    if src.starts_with("http") {
        return Ok((src.to_string(), None));
    }
    let (owner, repo, _) = super::github::parse_source(src)
        .ok_or_else(|| MarketplaceError::Invalid(format!("来源应为 owner/repo[/路径]：{src}")))?;
    let skill_id = id.rsplit('/').next().filter(|s| !s.is_empty()).map(str::to_string);
    Ok((format!("{owner}/{repo}"), skill_id))
}

/// Write the skill, register it with the registry, record it in V25.
/// Returns the install slug.
pub(crate) fn install_core(
    detail: &SkillDetail,
    scope: InstallScope,
    ws: Option<&Path>,
    data_dir: &Path,
    conn: &Connection,
    registry: &mut SkillsRegistry,
) -> Result<String, MarketplaceError> {
    let slug = flatten_slug(&detail.id);
    let (root, scan_dir, provenance) = match scope {
        InstallScope::Global => {
            let root = global_root(data_dir);
            let dir = root.join(&slug);
            (root, dir, SkillProvenance::Marketplace)
        }
        InstallScope::Workspace => {
            let ws = ws.ok_or_else(|| MarketplaceError::Invalid("没有可安装的工作区".to_string()))?;
            let root = workspace_root(ws);
            (root.clone(), root, SkillProvenance::Project)
        }
    };
    install::write_skill_files(&root, &slug, detail)?;
    registry.add_scan_dir(scan_dir, provenance);
    registry.discover();
    install::record_install(conn, &slug, &detail.hash)?;
    Ok(slug)
}

/// Remove a marketplace skill from the global root and every workspace.
/// When nothing existed anywhere the V25 row is left alone (it may belong
/// to a Halo `_standalone` install, which shares the table).
pub(crate) fn uninstall_core(
    slug: &str,
    data_dir: &Path,
    workspace_paths: &[PathBuf],
    conn: &Connection,
    registry: &mut SkillsRegistry,
) -> Result<(), MarketplaceError> {
    let mut removed: Vec<PathBuf> = Vec::new();
    let (global_dir, existed) = install::remove_skill_dir(&global_root(data_dir), slug)?;
    if existed {
        registry.remove_scan_dir(&global_dir);
        removed.push(global_dir);
    }
    for ws in workspace_paths {
        let (dir, existed) = install::remove_skill_dir(&workspace_root(ws), slug)?;
        if existed {
            removed.push(dir);
        }
    }
    if removed.is_empty() {
        tracing::warn!(slug, "skills_marketplace: uninstall found no marketplace install; leaving V25 alone");
        return Ok(());
    }
    // Drop exactly the skills that lived in the removed dirs. (`reload()`
    // would also drop plugin skills, which `discover()` never re-adds.)
    let gone: Vec<String> = registry
        .list()
        .iter()
        .filter(|m| removed.iter().any(|d| m.path.starts_with(d)))
        .map(|m| m.name.clone())
        .collect();
    for name in gone {
        registry.unregister(&name);
    }
    install::remove_install_row(conn, slug)
}

// ─── Tauri commands ────────────────────────────────────────────────────

use super::github;
use super::{SkillAudit, SkillSummary, SkillsShClient, SkillsmpClient};
use crate::app::AppState;
use tauri::State;

fn stored_key(state: &AppState, provider: MarketplaceProvider) -> Option<String> {
    state.db.lock().ok().and_then(|c| read_api_key(&c, provider))
}

/// A skill's files: skills.sh's detail API when a key is set, else GitHub.
async fn provider_detail(
    state: &AppState,
    id: &str,
    provider: MarketplaceProvider,
    source: Option<&str>,
) -> Result<SkillDetail, MarketplaceError> {
    if provider == MarketplaceProvider::SkillsSh {
        if let Some(key) = stored_key(state, provider) {
            return SkillsShClient::new(Some(key)).detail(id).await;
        }
    }
    let (src, skill_id) = detail_source(id, source)?;
    let gh = github::resolve_source(&src, skill_id.as_deref()).await?;
    github::fetch_skill_dir(&gh).await
}

#[tauri::command]
pub async fn search_skill_marketplace(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
    provider: Option<MarketplaceProvider>,
) -> Result<Vec<SkillSummary>, String> {
    let provider = provider.unwrap_or_default();
    let key = stored_key(&state, provider);
    let limit = limit.unwrap_or(20);
    let out = match provider {
        MarketplaceProvider::SkillsSh => SkillsShClient::new(key).search(&query, limit).await,
        MarketplaceProvider::Skillsmp => SkillsmpClient::new(key).search(&query, limit).await,
    };
    out.map_err(|e| e.to_string())
}

/// Trending/hot/all-time list. skills.sh only, and it needs a key
/// (`MissingApiKey` otherwise — the UI shows a search hint instead).
#[tauri::command]
pub async fn list_skill_marketplace(
    state: State<'_, AppState>,
    view: Option<String>,
    page: Option<u32>,
    provider: Option<MarketplaceProvider>,
) -> Result<Vec<SkillSummary>, String> {
    let provider = provider.unwrap_or_default();
    if provider == MarketplaceProvider::Skillsmp {
        return Ok(Vec::new());
    }
    SkillsShClient::new(stored_key(&state, provider))
        .list(view.as_deref().unwrap_or("trending"), page.unwrap_or(0), 50)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_skill_marketplace_detail(
    state: State<'_, AppState>,
    id: String,
    provider: Option<MarketplaceProvider>,
    source: Option<String>,
) -> Result<SkillDetail, String> {
    provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref())
        .await
        .map_err(|e| e.to_string())
}

/// skills.sh audit; empty when the provider has none or no key is set.
#[tauri::command]
pub async fn get_skill_marketplace_audit(
    state: State<'_, AppState>,
    id: String,
    provider: Option<MarketplaceProvider>,
) -> Result<SkillAudit, String> {
    let provider = provider.unwrap_or_default();
    match (provider, stored_key(&state, provider)) {
        (MarketplaceProvider::SkillsSh, Some(key)) => {
            SkillsShClient::new(Some(key)).audit(&id).await.map_err(|e| e.to_string())
        }
        _ => Ok(SkillAudit::default()),
    }
}

/// True when the skill is installed and its upstream hash changed.
#[tauri::command]
pub async fn check_skill_marketplace_update(
    state: State<'_, AppState>,
    id: String,
    provider: Option<MarketplaceProvider>,
    source: Option<String>,
) -> Result<bool, String> {
    let detail = provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let slug = flatten_slug(&detail.id);
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    Ok(install::read_install_version(&conn, &slug).is_some() && install::needs_update(&conn, &slug, &detail.hash))
}

/// Install globally or into a workspace; returns the install slug.
#[tauri::command]
pub async fn install_skill_from_marketplace(
    state: State<'_, AppState>,
    id: String,
    scope: InstallScope,
    workspace_id: Option<String>,
    provider: Option<MarketplaceProvider>,
    source: Option<String>,
) -> Result<String, String> {
    let ws = match scope {
        InstallScope::Global => None,
        InstallScope::Workspace => {
            let wid = workspace_id.ok_or_else(|| "没有选中工作区".to_string())?;
            let conn = state.db.lock().map_err(|e| e.to_string())?;
            Some(workspace_path(&conn, &wid).map_err(|e| e.to_string())?)
        }
    };
    let detail = provider_detail(&state, &id, provider.unwrap_or_default(), source.as_deref())
        .await
        .map_err(|e| e.to_string())?;
    let mut registry = state.skills_registry.write().await;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    install_core(&detail, scope, ws.as_deref(), &state.data_dir, &conn, &mut registry).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn uninstall_skill_from_marketplace(state: State<'_, AppState>, slug: String) -> Result<(), String> {
    let mut registry = state.skills_registry.write().await;
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    let workspaces = all_workspace_paths(&conn);
    uninstall_core(&slug, &state.data_dir, &workspaces, &conn, &mut registry).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::skills_marketplace::install::tests::{detail_with, v25_conn};
    use tempfile::TempDir;

    const SKILL_MD: &str = "---\nname: demo-skill\ndescription: a demo\n---\nbody";

    fn conn() -> Connection {
        let c = v25_conn();
        c.execute_batch(
            "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE spaces (id TEXT PRIMARY KEY, path TEXT);",
        )
        .unwrap();
        c
    }

    fn names(reg: &SkillsRegistry) -> Vec<String> {
        reg.list().iter().map(|m| m.name.clone()).collect()
    }

    fn v25_count(c: &Connection) -> i64 {
        c.query_row("SELECT COUNT(*) FROM marketplace_standalone_installs", [], |r| r.get(0)).unwrap()
    }

    #[test]
    fn read_api_key_none_when_unset() {
        let c = conn();
        assert_eq!(read_api_key(&c, MarketplaceProvider::SkillsSh), None);
        c.execute("INSERT INTO settings VALUES ('skillsmp_api_key', 'k1')", []).unwrap();
        assert_eq!(read_api_key(&c, MarketplaceProvider::Skillsmp).as_deref(), Some("k1"));
        assert_eq!(read_api_key(&c, MarketplaceProvider::SkillsSh), None);
    }

    #[test]
    fn workspace_path_errors_when_empty() {
        let c = conn();
        c.execute("INSERT INTO spaces VALUES ('w-empty', ''), ('w-ok', '/tmp/ws')", []).unwrap();
        assert!(workspace_path(&c, "w-empty").is_err());
        assert!(workspace_path(&c, "missing").is_err());
        assert_eq!(workspace_path(&c, "w-ok").unwrap(), PathBuf::from("/tmp/ws"));
        assert_eq!(all_workspace_paths(&c), vec![PathBuf::from("/tmp/ws")]);
    }

    #[test]
    fn detail_source_resolves_rows() {
        assert_eq!(
            detail_source("o/r/excel", Some("o/r")).unwrap(),
            ("o/r".to_string(), Some("excel".to_string()))
        );
        assert_eq!(
            detail_source("o/r/excel", None).unwrap(),
            ("o/r".to_string(), Some("excel".to_string())),
            "skills.sh id alone"
        );
        let url = "https://github.com/o/r/tree/main/s";
        assert_eq!(detail_source("x", Some(url)).unwrap(), (url.to_string(), None));
        assert!(detail_source("bad", None).is_err());
    }

    #[test]
    fn install_global_registers_and_records() {
        let tmp = TempDir::new().unwrap();
        let c = conn();
        let mut reg = SkillsRegistry::new();
        let slug = install_core(&detail_with(&[("SKILL.md", SKILL_MD)]), InstallScope::Global, None, tmp.path(), &c, &mut reg).unwrap();
        assert_eq!(slug, "o__r__s");
        assert!(global_root(tmp.path()).join("o__r__s/SKILL.md").is_file());
        assert_eq!(names(&reg), vec!["demo-skill"]);
        assert_eq!(install::read_install_version(&c, "o__r__s").as_deref(), Some("h1"));
    }

    #[test]
    fn install_workspace_writes_copy_and_registers_project_dir() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let c = conn();
        let mut reg = SkillsRegistry::new();
        install_core(&detail_with(&[("SKILL.md", SKILL_MD)]), InstallScope::Workspace, Some(&ws), tmp.path(), &c, &mut reg).unwrap();
        assert!(workspace_root(&ws).join("o__r__s/SKILL.md").is_file());
        assert!(!global_root(tmp.path()).exists(), "workspace install writes nothing globally");
        assert_eq!(names(&reg), vec!["demo-skill"]);
    }

    #[test]
    fn install_workspace_without_path_writes_nothing() {
        let tmp = TempDir::new().unwrap();
        let c = conn();
        let mut reg = SkillsRegistry::new();
        let r = install_core(&detail_with(&[("SKILL.md", SKILL_MD)]), InstallScope::Workspace, None, tmp.path(), &c, &mut reg);
        assert!(r.is_err());
        assert!(!global_root(tmp.path()).exists());
        assert_eq!(v25_count(&c), 0);
    }

    #[test]
    fn uninstall_removes_files_row_and_unregisters() {
        let tmp = TempDir::new().unwrap();
        let ws = tmp.path().join("ws");
        let c = conn();
        let mut reg = SkillsRegistry::new();
        let d = detail_with(&[("SKILL.md", SKILL_MD)]);
        install_core(&d, InstallScope::Global, None, tmp.path(), &c, &mut reg).unwrap();
        install_core(&d, InstallScope::Workspace, Some(&ws), tmp.path(), &c, &mut reg).unwrap();
        uninstall_core("o__r__s", tmp.path(), &[ws.clone()], &c, &mut reg).unwrap();
        assert!(!global_root(tmp.path()).join("o__r__s").exists());
        assert!(!workspace_root(&ws).join("o__r__s").exists());
        assert_eq!(v25_count(&c), 0);
        assert!(names(&reg).is_empty(), "skill dropped from the registry");
    }

    #[test]
    fn uninstall_keeps_other_skills_registered() {
        let tmp = TempDir::new().unwrap();
        let c = conn();
        let mut reg = SkillsRegistry::new();
        let other = std::fs::create_dir_all(tmp.path().join("user/keep")).map(|_| tmp.path().join("user")).unwrap();
        std::fs::write(other.join("keep/SKILL.md"), "---\nname: keep-me\ndescription: k\n---\nb").unwrap();
        reg.add_scan_dir(other, SkillProvenance::User);
        install_core(&detail_with(&[("SKILL.md", SKILL_MD)]), InstallScope::Global, None, tmp.path(), &c, &mut reg).unwrap();
        uninstall_core("o__r__s", tmp.path(), &[], &c, &mut reg).unwrap();
        assert_eq!(names(&reg), vec!["keep-me"]);
    }

    #[test]
    fn uninstall_unknown_slug_leaves_standalone_row() {
        let tmp = TempDir::new().unwrap();
        let c = conn();
        c.execute("INSERT INTO marketplace_standalone_installs VALUES ('halo-skill', 'skill', '1', 0, NULL)", []).unwrap();
        let mut reg = SkillsRegistry::new();
        uninstall_core("halo-skill", tmp.path(), &[], &c, &mut reg).unwrap();
        assert_eq!(v25_count(&c), 1, "Halo _standalone row untouched");
    }
}
