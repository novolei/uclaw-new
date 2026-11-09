//! `skill_marketplace_search` and `skill_install_from_marketplace` builtin
//! tools — thin wrappers over `crate::skills_marketplace`, so chat installs
//! and 万花筒 → 技能 → 市场 installs share one path (files + registry + V25).
//!
//! 1. `skill_marketplace_search` — skills.sh (default) or skillsmp search,
//!    sorted by popularity, deduped; each result carries an `installHint`.
//! 2. `skill_install_from_marketplace` — resolve `owner/repo[/path]` (+
//!    `skill_id`) to a GitHub skill dir, fetch it, install globally or into
//!    the session workspace. Always requires user approval.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{
    ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput,
};
use crate::skills::SkillsRegistry;


// ───────────────────────────────────────────────────────────────────
// Tool 1 — skill_marketplace_search
// ───────────────────────────────────────────────────────────────────

pub struct SkillMarketplaceSearchTool {
    skills_sh_key: Option<String>,
    skillsmp_key: Option<String>,
}

impl SkillMarketplaceSearchTool {
    /// Keys come from the `settings` table at registry build time.
    pub fn new(skills_sh_key: Option<String>, skillsmp_key: Option<String>) -> Self {
        Self { skills_sh_key, skillsmp_key }
    }
}

#[async_trait]
impl Tool for SkillMarketplaceSearchTool {
    fn name(&self) -> &str {
        "skill_marketplace_search"
    }

    fn description(&self) -> &str {
        "Search community agent-skill marketplaces for skills matching a query. provider \"skills_sh\" (default, skills.sh, ranked by installs) or \"skillsmp\" (skillsmp.com, ranked by GitHub stars). Returns `name`, `source`, `installs`, `description` and an `installHint`. Use when the user asks \"is there a skill for X\". To install one, pass that result's `installHint` fields to skill_install_from_marketplace (which requires user approval)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Free-text query describing what the user wants. E.g. \"lunar calendar conversion\", \"pdf form filling\"."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results. Default 8, max 20.",
                    "default": 8,
                    "minimum": 1,
                    "maximum": 20
                },
                "provider": {
                    "type": "string",
                    "enum": ["skills_sh", "skillsmp"],
                    "description": "Which marketplace. Default skills_sh.",
                    "default": "skills_sh"
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Read-only network egress, same tier as the web tool.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `query`"))?
            .trim()
            .to_string();
        if query.is_empty() {
            return Err(ToolError::kinded(ToolErrorKind::InvalidInput, "`query` must not be empty"));
        }
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n.clamp(1, 20) as usize)
            .unwrap_or(8);
        let provider: MarketplaceProvider = params
            .get("provider")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        // Over-fetch so dedup still leaves `limit` rows.
        let fetch = (limit * 2) as u32;
        let rows = match provider {
            MarketplaceProvider::SkillsSh => {
                crate::skills_marketplace::SkillsShClient::new(self.skills_sh_key.clone()).search(&query, fetch).await
            }
            MarketplaceProvider::Skillsmp => {
                crate::skills_marketplace::SkillsmpClient::new(self.skillsmp_key.clone()).search(&query, fetch).await
            }
        }
        .map_err(|e| {
            let kind = match e {
                crate::skills_marketplace::MarketplaceError::RateLimited => ToolErrorKind::RateLimited,
                _ => ToolErrorKind::UpstreamError,
            };
            ToolError::kinded(kind, e.to_string())
        })?;

        let results = shape_results(rows, provider, limit);
        let result_count = results.len();
        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "query": query,
                "limit": limit,
                "provider": provider,
                "resultCount": result_count,
                "results": results,
                "note": if result_count == 0 {
                    "No skills found. Try a different query or provider, or check local skills via skill_search."
                } else {
                    "To install one, call skill_install_from_marketplace with the fields of a result's `installHint`. The install will require user approval."
                },
            }),
            started.elapsed().as_millis() as u64,
        ))
    }
}

/// Parse a fully-qualified `owner/repo/<path>` install source. Returns
/// `(owner, repo, skill_path)` when there are ≥3 segments, else `None`
/// (a bare `owner/repo` needs `skill_id` resolution instead).
fn parse_install_source(source: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = source.split('/').collect();
    // Reject <3 segments (a bare owner/repo) and any empty segment
    // (a trailing/double slash like "owner/repo/" or "a//b"), which
    // would otherwise yield an empty skill_path that silently hits the
    // GitHub root listing instead of a clear input error.
    if parts.len() < 3 || parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    Some((parts[0].to_string(), parts[1].to_string(), parts[2..].join("/")))
}

/// Resolve any accepted install source into `(owner, repo, skill_path,
/// branch)`. Handles both the fully-qualified `owner/repo/<path>` form
/// and the skills.sh `owner/repo` + `skill_id` form (which resolves the
/// path + branch via git-trees).
async fn install_resolve_source(
    source: &str,
    skill_id: Option<&str>,
    git_ref: &str,
) -> Result<(String, String, String, String), ToolError> {
    if let Some((owner, repo, skill_path)) = parse_install_source(source) {
        return Ok((owner, repo, skill_path, git_ref.to_string()));
    }
    // Bare owner/repo — needs skill_id. Reject anything that isn't
    // exactly two non-empty segments ("owner/", "/repo", "owner" …).
    let parts: Vec<&str> = source.split('/').collect();
    if parts.len() != 2 || parts[0].is_empty() || parts[1].is_empty() {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!(
                "source {source:?} must be `owner/repo/<skill-dir>` or `owner/repo` with a `skill_id`"
            ),
        ));
    }
    let sid = skill_id.filter(|s| !s.is_empty()).ok_or_else(|| {
        ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!(
                "source {source:?} is `owner/repo` — pass `skill_id` (from a \
                 skill_marketplace_search result's installHint) so the path can be resolved"
            ),
        )
    })?;
    let resolved = crate::agent::tools::builtin::skill_marketplace_resolve::resolve_skill_path(
        parts[0], parts[1], sid,
    )
    .await?;
    Ok((
        parts[0].to_string(),
        parts[1].to_string(),
        resolved.dir_path,
        resolved.branch,
    ))
}

// ───────────────────────────────────────────────────────────────────
// Tool 2 — skill_install_from_marketplace
// ───────────────────────────────────────────────────────────────────

pub struct SkillInstallFromMarketplaceTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub data_dir: PathBuf,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// The session's workspace, for `scope = "workspace"`.
    pub workspace: Option<PathBuf>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
}

impl<R: tauri::Runtime> SkillInstallFromMarketplaceTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        data_dir: PathBuf,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        workspace: Option<PathBuf>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
    ) -> Self {
        Self { registry, data_dir, db, workspace, app_handle, conversation_id }
    }
}

#[async_trait]
impl<R: tauri::Runtime> Tool for SkillInstallFromMarketplaceTool<R> {
    fn name(&self) -> &str {
        "skill_install_from_marketplace"
    }

    fn description(&self) -> &str {
        "Install a skill from a public GitHub repo, globally (~/.uclaw/skills/_marketplace/, default) or into this session's workspace (scope=\"workspace\" → <workspace>/.uclaw/skills/). Use when the user accepts a skill_marketplace_search suggestion — pass that result's `installHint.source` (owner/repo) and `installHint.skill_id`. You may also pass a fully-qualified `source` = `owner/repo/<path-to-skill-dir>` directly. Requires user approval because it fetches third-party code and persists it across all future sessions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "Either `owner/repo` (paired with `skill_id`, the normal case from skill_marketplace_search — copy the result's installHint) OR a fully-qualified `owner/repo/<path-to-skill-dir>`. Examples: \"claude-office-skills/skills\" + skill_id \"excel-automation\"; or \"anthropics/skills/skill-creator\"."
                },
                "skill_id": {
                    "type": "string",
                    "description": "The skillId from a skill_marketplace_search result's installHint. Required when `source` is a bare `owner/repo`; ignored when `source` already includes the path."
                },
                "ref": {
                    "type": "string",
                    "description": "Git ref (branch/tag/commit). Only used for the fully-qualified form; the `owner/repo`+skill_id form auto-detects the repo's default branch. Default \"main\".",
                    "default": "main"
                },
                "force": {
                    "type": "boolean",
                    "description": "If true, overwrites existing installation. Default false — refuses to clobber.",
                    "default": false
                },
                "scope": {
                    "type": "string",
                    "enum": ["global", "workspace"],
                    "description": "global (default): available in every session. workspace: copied into this session's workspace folder.",
                    "default": "global"
                }
            },
            "required": ["source"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Network fetch + third-party code + cross-session
        // persistence. Always ask the user.
        ApprovalRequirement::UnlessAutoApproved
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();

        let source = params
            .get("source")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `source`")
            })?
            .trim()
            .to_string();

        let git_ref = params
            .get("ref")
            .and_then(|v| v.as_str())
            .unwrap_or("main")
            .to_string();

        let force = params
            .get("force")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let skill_id = params
            .get("skill_id")
            .and_then(|v| v.as_str())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());

        // Resolve into (owner, repo, skill_path, branch). The branch
        // may differ from `git_ref` when resolved from `owner/repo` +
        // skill_id (git-trees reports the repo's default branch).
        let (owner, repo, skill_path, git_ref) =
            install_resolve_source(&source, skill_id.as_deref(), &git_ref).await?;

        let scope = match params.get("scope").and_then(|v| v.as_str()) {
            Some("workspace") => InstallScope::Workspace,
            _ => InstallScope::Global,
        };

        let gh_ref = crate::skills_marketplace::github::GithubSkillRef {
            owner,
            repo,
            dir_path: skill_path,
            git_ref: git_ref.clone(),
        };
        let detail = crate::skills_marketplace::github::fetch_skill_dir(&gh_ref)
            .await
            .map_err(|e| ToolError::kinded(ToolErrorKind::UpstreamError, format!("source {source:?}: {e}")))?;

        // One install path with the 万花筒 marketplace: files + registry + V25.
        let install_dir = install_detail(
            &detail,
            scope,
            self.workspace.as_deref(),
            &self.data_dir,
            &self.db,
            &self.registry,
            force,
        )
        .await?;
        let written_files: Vec<String> = detail.files.iter().map(|f| f.path.clone()).collect();
        let registry_total = self.registry.read().await.list().len();

        let _ = self.app_handle.emit(
            "agent:skill-installed",
            json!({
                "source": source,
                "ref": git_ref,
                "scope": scope,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "registryTotal": registry_total,
                "conversationId": self.conversation_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );

        tracing::info!(
            source = %source,
            ref_ = %git_ref,
            install_path = %install_dir.display(),
            files_written = written_files.len(),
            "installed skill from marketplace"
        );

        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "source": source,
                "ref": git_ref,
                "scope": scope,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "registryReloaded": true,
                "registryTotal": registry_total,
                "message": format!(
                    "Installed {source:?} → {} ({} files). The skill is available now.",
                    install_dir.display(),
                    written_files.len(),
                ),
            }),
            started.elapsed().as_millis() as u64,
        ))
    }
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 21-D / 21-E
// ───────────────────────────────────────────────────────────────────

use crate::skills_marketplace::{InstallScope, MarketplaceProvider, SkillSummary};

/// Sort by installs (desc, stable), drop rows without a slug, dedup by
/// slug (most-installed wins), cap to `limit`, shape for the model.
fn shape_results(rows: Vec<SkillSummary>, provider: MarketplaceProvider, limit: usize) -> Vec<serde_json::Value> {
    let mut rows: Vec<SkillSummary> = rows.into_iter().filter(|r| !r.slug.is_empty()).collect();
    rows.sort_by(|a, b| b.installs.cmp(&a.installs));
    let mut seen = std::collections::HashSet::new();
    rows.iter()
        .filter(|r| seen.insert(r.slug.clone()))
        .take(limit)
        .map(|r| to_result_json(r, provider))
        .collect()
}

fn to_result_json(row: &SkillSummary, provider: MarketplaceProvider) -> serde_json::Value {
    // skills.sh ids aren't always the real path: install resolves
    // owner/repo + skill_id through git-trees. skillsmp gives a GitHub URL.
    let install_hint = match provider {
        MarketplaceProvider::SkillsSh => json!({ "source": row.source, "skill_id": row.slug }),
        MarketplaceProvider::Skillsmp => json!({ "source": row.install_url }),
    };
    json!({
        "id": row.id,
        "name": row.name,
        "source": row.source,
        "installs": row.installs,
        "installUrl": row.install_url,
        "description": row.description,
        "installHint": install_hint,
    })
}

/// Install a fetched skill (shared by the tool): refuses to overwrite an
/// existing install unless `force`; returns the install dir.
async fn install_detail(
    detail: &crate::skills_marketplace::SkillDetail,
    scope: InstallScope,
    workspace: Option<&std::path::Path>,
    data_dir: &std::path::Path,
    db: &std::sync::Mutex<rusqlite::Connection>,
    registry: &RwLock<SkillsRegistry>,
    force: bool,
) -> Result<PathBuf, ToolError> {
    use crate::skills_marketplace::install::{flatten_slug, global_root, workspace_root};
    let root = match scope {
        InstallScope::Global => global_root(data_dir),
        InstallScope::Workspace => workspace_root(workspace.ok_or_else(|| {
            ToolError::kinded(ToolErrorKind::InvalidInput, "scope=workspace but this session has no workspace")
        })?),
    };
    let dir = root.join(flatten_slug(&detail.id));
    if dir.exists() && !force {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!("skill already installed at {}. Pass force=true to reinstall.", dir.display()),
        ));
    }
    let mut reg = registry.write().await;
    let conn = db.lock().map_err(|e| ToolError::kinded(ToolErrorKind::Other, e.to_string()))?;
    crate::skills_marketplace::commands::install_core(detail, scope, workspace, data_dir, &conn, &mut reg)
        .map_err(|e| ToolError::kinded(ToolErrorKind::Other, e.to_string()))?;
    Ok(dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn skill_marketplace_search_rejects_empty_query() {
        let tool = SkillMarketplaceSearchTool::new(None, None);
        let err = tool
            .execute(json!({ "query": "" }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("empty"));
    }

    #[tokio::test]
    async fn skill_marketplace_search_rejects_missing_query() {
        let tool = SkillMarketplaceSearchTool::new(None, None);
        let err = tool.execute(json!({})).await.unwrap_err();
        assert!(format!("{err}").contains("query"));
    }

    fn row(id: &str, slug: &str, source: &str, installs: u64, install_url: &str) -> SkillSummary {
        SkillSummary {
            id: id.into(),
            slug: slug.into(),
            name: slug.into(),
            source: source.into(),
            installs,
            source_type: "github".into(),
            install_url: install_url.into(),
            url: String::new(),
            description: String::new(),
        }
    }

    #[test]
    fn shape_results_sorts_dedups_and_caps() {
        let rows = vec![
            row("c/d/excel", "excel", "c/d", 12, "c/d/excel"),
            row("e/f/pdf", "pdf", "e/f", 40, "e/f/pdf"),
            row("a/b/excel", "excel", "a/b", 9529, "a/b/excel"),
            row("g/h/zip", "zip", "g/h", 40, "g/h/zip"),
            row("x/y/", "", "x/y", 99999, ""),
        ];
        let out = shape_results(rows, MarketplaceProvider::SkillsSh, 3);
        let ids: Vec<&str> = out.iter().map(|v| v["id"].as_str().unwrap()).collect();
        assert_eq!(ids, vec!["a/b/excel", "e/f/pdf", "g/h/zip"], "desc by installs, stable, deduped, empty slug dropped, capped");
    }

    #[test]
    fn to_result_json_maps_fields() {
        let v = to_result_json(&row("a/b/excel", "excel", "a/b", 7, "a/b/excel"), MarketplaceProvider::SkillsSh);
        assert_eq!(v["name"], "excel");
        assert_eq!(v["installs"], 7);
        assert_eq!(v["installUrl"], "a/b/excel");
        assert_eq!(v["installHint"]["source"], "a/b");
        assert_eq!(v["installHint"]["skill_id"], "excel");
        let url = "https://github.com/o/r/tree/main/s";
        let m = to_result_json(&row("smp-id", "smp-id", "author", 3, url), MarketplaceProvider::Skillsmp);
        assert_eq!(m["installHint"]["source"], url);
        assert!(m["installHint"].get("skill_id").is_none());
    }

    fn install_env() -> (tempfile::TempDir, std::sync::Mutex<rusqlite::Connection>, RwLock<SkillsRegistry>) {
        let conn = crate::skills_marketplace::install::tests::v25_conn();
        (tempfile::TempDir::new().unwrap(), std::sync::Mutex::new(conn), RwLock::new(SkillsRegistry::new()))
    }

    fn skill_detail() -> crate::skills_marketplace::SkillDetail {
        crate::skills_marketplace::install::tests::detail_with(&[("SKILL.md", "---\nname: demo\ndescription: d\n---\nb")])
    }

    #[tokio::test]
    async fn install_detail_records_v25_row() {
        let (tmp, db, reg) = install_env();
        let dir = install_detail(&skill_detail(), InstallScope::Global, None, tmp.path(), &db, &reg, false).await.unwrap();
        assert!(dir.join("SKILL.md").is_file());
        let conn = db.lock().unwrap();
        assert_eq!(crate::skills_marketplace::install::read_install_version(&conn, "o__r__s").as_deref(), Some("h1"));
        drop(conn);
        assert_eq!(reg.read().await.list().len(), 1);
    }

    #[tokio::test]
    async fn install_detail_refuses_existing_without_force() {
        let (tmp, db, reg) = install_env();
        install_detail(&skill_detail(), InstallScope::Global, None, tmp.path(), &db, &reg, false).await.unwrap();
        let err = install_detail(&skill_detail(), InstallScope::Global, None, tmp.path(), &db, &reg, false).await.unwrap_err();
        assert!(format!("{err}").contains("force"));
        install_detail(&skill_detail(), InstallScope::Global, None, tmp.path(), &db, &reg, true).await.unwrap();
    }

    #[tokio::test]
    async fn install_detail_workspace_needs_workspace() {
        let (tmp, db, reg) = install_env();
        assert!(install_detail(&skill_detail(), InstallScope::Workspace, None, tmp.path(), &db, &reg, false).await.is_err());
        let ws = tmp.path().join("ws");
        let dir = install_detail(&skill_detail(), InstallScope::Workspace, Some(&ws), tmp.path(), &db, &reg, false).await.unwrap();
        assert!(dir.starts_with(ws.join(".uclaw").join("skills")));
    }

    #[tokio::test]
    async fn install_two_segment_source_requires_skill_id() {
        let err = super::install_resolve_source("owner/repo", None, "main")
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("skill_id"));
    }

    #[tokio::test]
    async fn install_resolve_three_segment_skips_network() {
        // A fully-qualified source returns immediately from
        // parse_install_source without ever awaiting resolve_skill_path,
        // and passes the caller's git_ref through unchanged.
        let (owner, repo, path, branch) =
            super::install_resolve_source("a/b/some/skill", None, "main")
                .await
                .unwrap();
        assert_eq!((owner.as_str(), repo.as_str()), ("a", "b"));
        assert_eq!(path, "some/skill");
        assert_eq!(branch, "main"); // git_ref passed through, not resolved
    }

    #[test]
    fn install_three_segment_source_parses_directly() {
        // The 3-segment form needs no resolution / network.
        let parsed = super::parse_install_source("anthropics/skills/skill-creator");
        assert_eq!(parsed, Some(("anthropics".into(), "skills".into(), "skill-creator".into())));
    }

    #[test]
    fn install_three_segment_with_nested_path() {
        let parsed = super::parse_install_source("a/b/skills/deep/leaf");
        assert_eq!(parsed, Some(("a".into(), "b".into(), "skills/deep/leaf".into())));
    }

    #[test]
    fn install_two_segment_source_has_no_path() {
        assert_eq!(super::parse_install_source("owner/repo"), None);
    }

    #[test]
    fn parse_install_source_rejects_empty_segments() {
        // Trailing/double slashes are malformed, not a valid skill_path.
        assert_eq!(super::parse_install_source("owner/repo/"), None);
        assert_eq!(super::parse_install_source("a//b"), None);
    }

    #[tokio::test]
    async fn install_resolve_rejects_trailing_slash_source() {
        // "owner/repo/" must error cleanly (it is neither a valid
        // 3-segment path nor a bare owner/repo) rather than silently
        // hitting the GitHub root listing.
        let err = super::install_resolve_source("owner/repo/", Some("x"), "main")
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("owner/repo"));
    }
}
