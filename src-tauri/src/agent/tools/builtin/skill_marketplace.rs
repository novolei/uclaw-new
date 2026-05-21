//! Bundle 21-D + 21-E — `skill_marketplace_search` and
//! `skill_install_from_marketplace` builtin tools.
//!
//! Together these wire uClaw into the skills.sh / GitHub agent-skill
//! ecosystem without requiring an external `npx skills` CLI on the
//! user's machine. The pair mirrors what `find-skills`
//! (vercel-labs/skills) and `skill-creator` (anthropics/skills) ask
//! their host agent to do:
//!
//! 1. `skill_marketplace_search` — discover candidate skills by
//!    keyword. Queries GitHub Code Search for SKILL.md files
//!    containing the query terms, returns name + path + repo + stars.
//!    (skills.sh has no documented public API; GitHub search is the
//!    canonical fallback. If skills.sh ships an API we wire it later
//!    by extending `query_marketplace`.)
//!
//! 2. `skill_install_from_marketplace` — fetch a specific
//!    `owner/repo/<path-to-skill>` from GitHub raw, validate the
//!    SKILL.md, write it under `~/.uclaw/skills/_marketplace/
//!    <owner>__<repo>__<slug>/`, and trigger registry rescan.
//!    Always requires user approval (network + foreign code +
//!    cross-session persistence).
//!
//! Skill-creator and find-skills SKILL.md files reference `npx
//! skills ...` commands; the bundled-into-uClaw versions point at
//! these two tools instead. See Bundle 21-C.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{
    ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput,
};
use crate::skills::SkillsRegistry;

const USER_AGENT: &str = "uClaw/0.1";
const SEARCH_TIMEOUT_MS: u64 = 10_000;
const INSTALL_TIMEOUT_MS: u64 = 30_000;
const MAX_FILE_BYTES: usize = 512 * 1024; // 512 KB per file. Skills should be small.
const MAX_FILES_PER_SKILL: usize = 32;

// ───────────────────────────────────────────────────────────────────
// Tool 1 — skill_marketplace_search
// ───────────────────────────────────────────────────────────────────

pub struct SkillMarketplaceSearchTool {}

impl SkillMarketplaceSearchTool {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for SkillMarketplaceSearchTool {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Tool for SkillMarketplaceSearchTool {
    fn name(&self) -> &str {
        "skill_marketplace_search"
    }

    fn description(&self) -> &str {
        "Search the open agent-skills ecosystem (skills.sh / GitHub) for skills that match a query. Returns candidate skill names + descriptions + repos + stars + install commands. Use when the user asks \"is there a skill for X\" or \"find a skill that does X\". Pair with skill_install_from_marketplace to actually install a candidate (which requires user approval)."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Free-text query describing what the user wants. E.g. \"lunar calendar conversion\", \"pdf form filling\", \"slack message formatting\"."
                },
                "limit": {
                    "type": "integer",
                    "description": "Max results. Default 8, max 20.",
                    "default": 8,
                    "minimum": 1,
                    "maximum": 20
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Search is read-only network egress. Auto-approve, same
        // tier as the web tool.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let started = Instant::now();
        let query = params
            .get("query")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ToolError::kinded(ToolErrorKind::InvalidInput, "missing required `query`")
            })?
            .trim()
            .to_string();
        if query.is_empty() {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                "`query` must not be empty",
            ));
        }
        let limit = params
            .get("limit")
            .and_then(|v| v.as_u64())
            .map(|n| n.clamp(1, 20) as usize)
            .unwrap_or(8);

        let results = query_marketplace(&query, limit).await?;
        let result_count = results.len();
        let elapsed = started.elapsed().as_millis() as u64;

        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "query": query,
                "limit": limit,
                "resultCount": result_count,
                "results": results,
                "note": if result_count == 0 {
                    "No skills found. Try a different query, or check if a relevant skill already exists locally via skill_search."
                } else {
                    "To install one, call skill_install_from_marketplace with the `source` field set to `<owner>/<repo>/<path>` from a result. The install will require user approval."
                },
            }),
            elapsed,
        ))
    }
}

/// Run the actual search against GitHub Code Search. We look for
/// SKILL.md files mentioning the query terms across all public
/// repos. This is the highest-recall path without depending on
/// skills.sh having a public API.
async fn query_marketplace(
    query: &str,
    limit: usize,
) -> Result<Vec<serde_json::Value>, ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(SEARCH_TIMEOUT_MS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::NetworkError,
                format!("failed to build http client: {e}"),
            )
        })?;

    // GitHub code search query: filename:SKILL.md + the user's
    // query terms. Public, no auth required for low-rate access
    // (60 req/hr per IP); we don't hammer it.
    let gh_query = format!("{} filename:SKILL.md", query);
    let url = format!(
        "https://api.github.com/search/code?q={}&per_page={}",
        urlencoding::encode(&gh_query),
        limit
    );

    let resp = client
        .get(&url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::NetworkError,
                format!("github search request failed: {e}"),
            )
        })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let kind = if status.as_u16() == 429 {
            ToolErrorKind::RateLimited
        } else if status.as_u16() == 403 {
            // GitHub returns 403 for auth-required code search
            // without a token. The error message should tell the
            // LLM not to retry blindly.
            ToolErrorKind::PermissionDenied
        } else {
            ToolErrorKind::UpstreamError
        };
        return Err(ToolError::kinded(
            kind,
            format!(
                "github search returned {status}: {}. Note: GitHub code search \
                 may require authentication for unauthenticated rate-limited \
                 use. Skill discovery via uClaw works best when the user \
                 already has a target skill in mind.",
                truncate_for_error(&body, 200),
            ),
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        ToolError::kinded(
            ToolErrorKind::ParseError,
            format!("github search returned malformed JSON: {e}"),
        )
    })?;

    let empty_items: Vec<serde_json::Value> = Vec::new();
    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .unwrap_or(&empty_items);

    let mut results: Vec<serde_json::Value> = Vec::new();
    for item in items.iter().take(limit) {
        let path = item
            .get("path")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let repo_full = item
            .get("repository")
            .and_then(|r| r.get("full_name"))
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();
        let stars = item
            .get("repository")
            .and_then(|r| r.get("stargazers_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let html_url = item
            .get("html_url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string();

        // Derive the skill slug from the path: typical layout
        // `skill-name/SKILL.md` or `skills/skill-name/SKILL.md`.
        let slug = path
            .rsplit_once('/')
            .and_then(|(parent, _file)| parent.rsplit_once('/').map(|(_, last)| last.to_string()))
            .or_else(|| {
                path.rsplit_once('/').map(|(parent, _)| parent.to_string())
            })
            .unwrap_or_else(|| path.clone());

        // Compose the install source string. Caller passes this to
        // skill_install_from_marketplace.
        let install_source = if path.ends_with("/SKILL.md") {
            let dir = path.strip_suffix("/SKILL.md").unwrap_or(&path);
            format!("{}/{}", repo_full, dir)
        } else {
            format!("{}/{}", repo_full, path)
        };

        results.push(json!({
            "slug": slug,
            "repo": repo_full,
            "path": path,
            "stars": stars,
            "htmlUrl": html_url,
            "installSource": install_source,
            "installCommand": format!(
                "Call skill_install_from_marketplace with source=\"{}\"",
                install_source
            ),
        }));
    }
    Ok(results)
}

fn truncate_for_error(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
    }
}

// ───────────────────────────────────────────────────────────────────
// Tool 2 — skill_install_from_marketplace
// ───────────────────────────────────────────────────────────────────

pub struct SkillInstallFromMarketplaceTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub data_dir: PathBuf,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
}

impl<R: tauri::Runtime> SkillInstallFromMarketplaceTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        data_dir: PathBuf,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
    ) -> Self {
        Self {
            registry,
            data_dir,
            app_handle,
            conversation_id,
        }
    }
}

#[async_trait]
impl<R: tauri::Runtime> Tool for SkillInstallFromMarketplaceTool<R> {
    fn name(&self) -> &str {
        "skill_install_from_marketplace"
    }

    fn description(&self) -> &str {
        "Install a skill from a public GitHub repo into ~/.uclaw/skills/_marketplace/. Use when the user accepts a suggestion from skill_marketplace_search (or names a specific skill). The source string is `owner/repo/<path-to-skill-dir>` (the directory CONTAINING the SKILL.md, NOT the SKILL.md path itself). The install requires user approval because it fetches third-party code and persists it across all future sessions."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "source": {
                    "type": "string",
                    "description": "GitHub source: `owner/repo/<path-to-skill-dir>`. Examples: \"anthropics/skills/skill-creator\", \"vercel-labs/skills/find-skills\", \"obra/superpowers/brainstorming\"."
                },
                "ref": {
                    "type": "string",
                    "description": "Git ref (branch/tag/commit) to install from. Default \"main\".",
                    "default": "main"
                },
                "force": {
                    "type": "boolean",
                    "description": "If true, overwrites existing installation. Default false — refuses to clobber.",
                    "default": false
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

        // Parse `owner/repo/<path>` — at minimum 3 segments.
        let parts: Vec<&str> = source.split('/').collect();
        if parts.len() < 3 {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "source {source:?} must be in form `owner/repo/<skill-dir-path>` \
                     (e.g. \"anthropics/skills/skill-creator\")"
                ),
            ));
        }
        let owner = parts[0];
        let repo = parts[1];
        let skill_path = parts[2..].join("/");

        // Slug for local install dir. Format mirrors marketplace
        // recovery in AppState::new: `_marketplace/<owner>__<slug>`.
        // We append the path tail too so two skills from the same
        // repo don't collide.
        let path_tail = skill_path.replace('/', "__");
        let install_slug = format!("{owner}__{repo}__{path_tail}");
        let install_dir = self
            .data_dir
            .join("skills")
            .join("_marketplace")
            .join(&install_slug);

        if install_dir.exists() && !force {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "skill already installed at {}. Pass force=true to reinstall.",
                    install_dir.display()
                ),
            ));
        }

        let client = reqwest::Client::builder()
            .timeout(Duration::from_millis(INSTALL_TIMEOUT_MS))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("failed to build http client: {e}"),
                )
            })?;

        // List files in the skill dir via GitHub contents API.
        let list_url = format!(
            "https://api.github.com/repos/{}/{}/contents/{}?ref={}",
            owner,
            repo,
            urlencoding::encode(&skill_path),
            urlencoding::encode(&git_ref),
        );
        let resp = client
            .get(&list_url)
            .header("Accept", "application/vnd.github+json")
            .send()
            .await
            .map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("github contents request failed: {e}"),
                )
            })?;
        if !resp.status().is_success() {
            return Err(ToolError::kinded(
                ToolErrorKind::UpstreamError,
                format!(
                    "github contents API returned {}: source={source} ref={git_ref}",
                    resp.status()
                ),
            ));
        }
        let listing: serde_json::Value = resp.json().await.map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::ParseError,
                format!("github contents API returned malformed JSON: {e}"),
            )
        })?;

        // contents API returns a single object for files, an array
        // for directories. We expect the user to point at a dir.
        let entries: Vec<serde_json::Value> = match listing {
            serde_json::Value::Array(arr) => arr,
            other => {
                return Err(ToolError::kinded(
                    ToolErrorKind::InvalidInput,
                    format!(
                        "source {source:?} points at a file, not a directory. Pass \
                         the path to the skill DIRECTORY (the one containing SKILL.md). \
                         Got: {}",
                        truncate_for_error(&other.to_string(), 200)
                    ),
                ));
            }
        };

        if entries.len() > MAX_FILES_PER_SKILL {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "refusing to install skill with {} files (cap is {}). \
                     This is likely a misnamed source pointing at a large dir.",
                    entries.len(),
                    MAX_FILES_PER_SKILL,
                ),
            ));
        }

        // Verify the listing contains a SKILL.md.
        let has_skill_md = entries.iter().any(|e| {
            e.get("name").and_then(|v| v.as_str()) == Some("SKILL.md")
                && e.get("type").and_then(|v| v.as_str()) == Some("file")
        });
        if !has_skill_md {
            return Err(ToolError::kinded(
                ToolErrorKind::InvalidInput,
                format!(
                    "source {source:?} does not contain a SKILL.md. \
                     Listed entries: {:?}",
                    entries
                        .iter()
                        .filter_map(|e| e.get("name").and_then(|v| v.as_str()))
                        .collect::<Vec<_>>(),
                ),
            ));
        }

        // Fresh start: if force=true and dir exists, blow it away
        // before write. Bounded to within our own data dir.
        if install_dir.exists() && force {
            let _ = std::fs::remove_dir_all(&install_dir);
        }
        std::fs::create_dir_all(&install_dir).map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::Other,
                format!("failed to create {}: {e}", install_dir.display()),
            )
        })?;

        let mut written_files: Vec<String> = Vec::new();
        let mut skipped: Vec<String> = Vec::new();
        for entry in &entries {
            let entry_name = entry
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let entry_type = entry
                .get("type")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            // Reject path traversal in entry names — defense in
            // depth; GitHub shouldn't return these but we don't
            // trust the upstream blindly.
            if entry_name.contains("..") || entry_name.contains('/') {
                skipped.push(format!("{entry_name} (suspicious name)"));
                continue;
            }
            // Files only. Subdirectories aren't recursively
            // installed in this first cut — most skills are
            // shallow. Subdirs can be a follow-up.
            if entry_type != "file" {
                skipped.push(format!("{entry_name} (type={entry_type}, only files supported in v1)"));
                continue;
            }
            let download_url = entry
                .get("download_url")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            if download_url.is_empty() {
                skipped.push(format!("{entry_name} (no download_url)"));
                continue;
            }
            // Fetch each file individually.
            let file_resp = client
                .get(download_url)
                .send()
                .await
                .map_err(|e| {
                    ToolError::kinded(
                        ToolErrorKind::NetworkError,
                        format!("failed to fetch {entry_name}: {e}"),
                    )
                })?;
            if !file_resp.status().is_success() {
                return Err(ToolError::kinded(
                    ToolErrorKind::UpstreamError,
                    format!(
                        "fetching {entry_name} returned {}",
                        file_resp.status()
                    ),
                ));
            }
            let bytes = file_resp.bytes().await.map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::NetworkError,
                    format!("failed to read {entry_name}: {e}"),
                )
            })?;
            if bytes.len() > MAX_FILE_BYTES {
                return Err(ToolError::kinded(
                    ToolErrorKind::PayloadTooLarge,
                    format!(
                        "{entry_name} is {} bytes (cap is {})",
                        bytes.len(),
                        MAX_FILE_BYTES
                    ),
                ));
            }
            let dest = install_dir.join(entry_name);
            std::fs::write(&dest, &bytes).map_err(|e| {
                ToolError::kinded(
                    ToolErrorKind::Other,
                    format!("failed to write {}: {e}", dest.display()),
                )
            })?;
            written_files.push(entry_name.to_string());
        }

        // Register the new install dir + rescan.
        let discovered = {
            let mut reg = self.registry.write().await;
            reg.add_scan_dir(
                install_dir.clone(),
                crate::skills::SkillProvenance::Marketplace,
            );
            reg.discover().len()
        };

        let _ = self.app_handle.emit(
            "agent:skill-installed",
            json!({
                "source": source,
                "ref": git_ref,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "filesSkipped": skipped,
                "registryTotal": discovered,
                "conversationId": self.conversation_id,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }),
        );

        tracing::info!(
            source = %source,
            ref_ = %git_ref,
            install_path = %install_dir.display(),
            files_written = written_files.len(),
            files_skipped = skipped.len(),
            registry_total = discovered,
            "[Bundle 21-D] installed skill from marketplace"
        );

        let elapsed = started.elapsed().as_millis() as u64;
        Ok(ToolOutput::new(
            json!({
                "ok": true,
                "source": source,
                "ref": git_ref,
                "installPath": install_dir.display().to_string(),
                "filesWritten": written_files,
                "filesSkipped": skipped,
                "registryReloaded": true,
                "registryTotal": discovered,
                "message": format!(
                    "Installed {source:?} → {} ({} files, {} skipped). Registry \
                     reloaded — skill is immediately available to skill_search.",
                    install_dir.display(),
                    written_files.len(),
                    skipped.len(),
                ),
            }),
            elapsed,
        ))
    }
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 21-D / 21-E
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_for_error_short() {
        assert_eq!(truncate_for_error("hi", 100), "hi");
    }

    #[test]
    fn truncate_for_error_long() {
        let out = truncate_for_error(&"a".repeat(500), 50);
        assert_eq!(out.len(), 51); // 50 + '…'
        assert!(out.ends_with('…'));
    }

    #[tokio::test]
    async fn skill_marketplace_search_rejects_empty_query() {
        let tool = SkillMarketplaceSearchTool::new();
        let err = tool
            .execute(json!({ "query": "" }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("empty"));
    }

    #[tokio::test]
    async fn skill_marketplace_search_rejects_missing_query() {
        let tool = SkillMarketplaceSearchTool::new();
        let err = tool.execute(json!({})).await.unwrap_err();
        assert!(format!("{err}").contains("query"));
    }
}
