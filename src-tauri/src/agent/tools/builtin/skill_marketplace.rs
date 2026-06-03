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
//!    keyword via the skills.sh registry (`/api/search`, public, no
//!    auth, semantic, ranked by install count). Returns skillId +
//!    name + source (owner/repo) + installs.
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
use serde::Deserialize;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{
    ApprovalRequirement, Tool, ToolError, ToolErrorKind, ToolOutput,
};
use crate::skills::SkillsRegistry;

const USER_AGENT: &str = "uClaw/0.1";
const SEARCH_TIMEOUT_MS: u64 = 10_000;
const SEARCH_API_BASE: &str = "https://skills.sh";
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
        "Search the open agent-skills ecosystem (skills.sh) for community skills matching a query. Semantic search; results are ranked by install count (popularity). Returns candidate `skillId` + `name` + `source` (owner/repo) + `installs`, plus an `installHint`. Use when the user asks \"is there a skill for X\" or \"find a skill that does X\". To install one, pass that result's `installHint.source` and `installHint.skill_id` to skill_install_from_marketplace (which requires user approval)."
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
                    "To install one, call skill_install_from_marketplace with `source` and `skill_id` copied from a result's `installHint`. The install will require user approval."
                },
            }),
            elapsed,
        ))
    }
}

/// One skill row from skills.sh `/api/search`.
#[derive(Debug, Deserialize)]
struct SkillsShSkill {
    #[serde(rename = "skillId", default)]
    skill_id: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    installs: u64,
    #[serde(default)]
    source: String,
}

/// Envelope returned by skills.sh `/api/search`.
#[derive(Debug, Deserialize)]
struct SkillsShSearchResponse {
    #[serde(default)]
    skills: Vec<SkillsShSkill>,
}

/// Deserialize + dedup + shape the skills.sh response. Pure (no
/// network) so it is unit-testable against fixtures.
///
/// skills.sh returns results pre-sorted by `installs` descending and
/// flags cross-repo copies with `isDuplicate`. We dedup by `skillId`
/// keeping the first occurrence (= highest installs), then cap to
/// `limit`. Each result carries an `installHint` with exactly the
/// `source` + `skill_id` that `skill_install_from_marketplace` needs.
fn parse_search_response(body: serde_json::Value, limit: usize) -> Vec<serde_json::Value> {
    let parsed: SkillsShSearchResponse =
        serde_json::from_value(body).unwrap_or(SkillsShSearchResponse { skills: Vec::new() });

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut results: Vec<serde_json::Value> = Vec::new();
    for s in parsed.skills {
        if s.skill_id.is_empty() || s.source.is_empty() {
            continue;
        }
        // Move owned fields out of `s` so `skill_id`/`source` can be used
        // twice below (top-level field + installHint) without a
        // move-after-move; only one clone goes into the dedup set.
        let skill_id = s.skill_id;
        let source = s.source;
        if !seen.insert(skill_id.clone()) {
            continue; // duplicate skillId — first (highest installs) wins
        }
        results.push(json!({
            "skillId": skill_id.clone(),
            "name": s.name,
            "source": source.clone(),
            "installs": s.installs,
            "installHint": { "source": source, "skill_id": skill_id },
        }));
        if results.len() >= limit {
            break;
        }
    }
    results
}

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

    // skills.sh registry search — public, no auth. Semantic search,
    // results pre-sorted by install count descending.
    let url = format!(
        "{}/api/search?q={}&limit={}",
        SEARCH_API_BASE,
        urlencoding::encode(query),
        limit
    );

    let resp = client.get(&url).send().await.map_err(|e| {
        ToolError::kinded(
            ToolErrorKind::NetworkError,
            format!("skills.sh search request failed: {e}"),
        )
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let kind = match status.as_u16() {
            429 => ToolErrorKind::RateLimited,
            503 | 502 | 504 => ToolErrorKind::Unavailable,
            _ => ToolErrorKind::UpstreamError,
        };
        return Err(ToolError::kinded(
            kind,
            format!(
                "skills.sh search returned {status}: {}",
                truncate_for_error(&body, 200),
            ),
        ));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| {
        ToolError::kinded(
            ToolErrorKind::ParseError,
            format!("skills.sh search returned malformed JSON: {e}"),
        )
    })?;

    Ok(parse_search_response(body, limit))
}

fn truncate_for_error(s: &str, n: usize) -> String {
    if s.len() <= n {
        s.to_string()
    } else {
        format!("{}…", &s[..n])
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
        "Install a skill from a public GitHub repo into ~/.uclaw/skills/_marketplace/. Use when the user accepts a skill_marketplace_search suggestion — pass that result's `installHint.source` (owner/repo) and `installHint.skill_id`. You may also pass a fully-qualified `source` = `owner/repo/<path-to-skill-dir>` directly. Requires user approval because it fetches third-party code and persists it across all future sessions."
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
        assert_eq!(out.len(), 53); // 50 ASCII bytes + 3-byte '…' (U+2026)
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

    #[test]
    fn parse_search_response_maps_fields_and_dedups() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{
              "query": "excel",
              "skills": [
                { "id": "a/b/excel-automation", "skillId": "excel-automation",
                  "name": "excel-automation", "installs": 9529, "source": "a/b", "isDuplicate": false },
                { "id": "c/d/excel-automation", "skillId": "excel-automation",
                  "name": "excel-automation", "installs": 12, "source": "c/d", "isDuplicate": true },
                { "id": "e/f/pdf-fill", "skillId": "pdf-fill",
                  "name": "pdf fill", "installs": 40, "source": "e/f" }
              ]
            }"#,
        )
        .unwrap();

        let out = parse_search_response(body, 8);

        // Duplicate skillId collapses to the first (highest-installs) entry.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0]["skillId"], "excel-automation");
        assert_eq!(out[0]["source"], "a/b");
        assert_eq!(out[0]["installs"], 9529);
        // Install hint carries exactly what skill_install_from_marketplace needs.
        assert_eq!(out[0]["installHint"]["source"], "a/b");
        assert_eq!(out[0]["installHint"]["skill_id"], "excel-automation");
        assert_eq!(out[1]["skillId"], "pdf-fill");
    }

    #[test]
    fn parse_search_response_honors_limit_and_empty() {
        let empty: serde_json::Value = serde_json::json!({ "skills": [] });
        assert!(parse_search_response(empty, 8).is_empty());

        let body: serde_json::Value = serde_json::json!({
            "skills": [
                { "skillId": "one", "name": "one", "installs": 3, "source": "o/1" },
                { "skillId": "two", "name": "two", "installs": 2, "source": "o/2" }
            ]
        });
        assert_eq!(parse_search_response(body, 1).len(), 1);
    }

    #[test]
    fn parse_search_response_skips_rows_with_empty_skill_id_or_source() {
        // A malformed/partial API row (empty skillId or empty source) is
        // uninstallable, so it must be dropped — not surfaced as a candidate.
        let body: serde_json::Value = serde_json::json!({
            "skills": [
                { "skillId": "", "name": "no-id", "installs": 5, "source": "o/1" },
                { "skillId": "no-source", "name": "no-source", "installs": 4, "source": "" },
                { "skillId": "good", "name": "good", "installs": 3, "source": "o/2" }
            ]
        });
        let out = parse_search_response(body, 8);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0]["skillId"], "good");
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
