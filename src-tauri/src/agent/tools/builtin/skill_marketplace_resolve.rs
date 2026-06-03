// SPDX-License-Identifier: Apache-2.0
//! Resolve a skills.sh `(owner/repo, skillId)` to the in-repo skill
//! directory (the one containing SKILL.md) via GitHub's anonymous
//! git-trees API. skills.sh `/api/search` returns `source` + `skillId`
//! but not the path inside the repo; this module supplies it.

use std::time::Duration;

use crate::agent::tools::tool::{ToolError, ToolErrorKind};

const USER_AGENT: &str = "uClaw/0.1";
const RESOLVE_TIMEOUT_MS: u64 = 15_000;

/// Outcome of resolving a skill: the directory inside the repo that
/// holds SKILL.md, and the branch it was found on (so the installer
/// fetches from the same ref).
pub struct ResolvedSkill {
    pub dir_path: String,
    pub branch: String,
}

/// Collect blob (file) paths from a GitHub recursive git-trees body.
fn parse_tree_paths(body: &serde_json::Value) -> Vec<String> {
    body.get("tree")
        .and_then(|t| t.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|e| e.get("type").and_then(|v| v.as_str()) == Some("blob"))
                .filter_map(|e| e.get("path").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default()
}

/// Find the directory containing `{skill_id}/SKILL.md`. Matches a
/// top-level `{skill_id}/SKILL.md` or any `**/{skill_id}/SKILL.md`.
/// On multiple matches returns the shallowest (fewest path segments).
/// Returns the directory path (without the trailing `/SKILL.md`).
fn match_skill_dir(paths: &[String], skill_id: &str) -> Option<String> {
    let top = format!("{skill_id}/SKILL.md");
    let nested_suffix = format!("/{skill_id}/SKILL.md");
    let mut best: Option<String> = None;
    for p in paths {
        let is_match = p == &top || p.ends_with(&nested_suffix);
        if !is_match {
            continue;
        }
        let dir = p.strip_suffix("/SKILL.md").unwrap_or(p).to_string();
        let shallower = match &best {
            None => true,
            Some(cur) => dir.matches('/').count() < cur.matches('/').count(),
        };
        if shallower {
            best = Some(dir);
        }
    }
    best
}

/// Resolve `(owner, repo, skill_id)` → the in-repo skill directory and
/// the branch it lives on, using GitHub's anonymous REST + git-trees
/// APIs (no auth; these endpoints allow unauthenticated access, unlike
/// /search/code).
pub async fn resolve_skill_path(
    owner: &str,
    repo: &str,
    skill_id: &str,
) -> Result<ResolvedSkill, ToolError> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_millis(RESOLVE_TIMEOUT_MS))
        .user_agent(USER_AGENT)
        .build()
        .map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::NetworkError,
                format!("failed to build http client: {e}"),
            )
        })?;

    // 1. Determine the default branch (fall back to "main").
    let meta_url = format!("https://api.github.com/repos/{owner}/{repo}");
    let meta_resp = client
        .get(&meta_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::NetworkError,
                format!("github repo metadata request failed: {e}"),
            )
        })?;
    if !meta_resp.status().is_success() {
        let status = meta_resp.status();
        let kind = match status.as_u16() {
            403 | 429 => ToolErrorKind::RateLimited,
            404 => ToolErrorKind::ResourceNotFound,
            _ => ToolErrorKind::UpstreamError,
        };
        return Err(ToolError::kinded(
            kind,
            format!("github repo {owner}/{repo} metadata returned {status}"),
        ));
    }
    let meta: serde_json::Value = meta_resp.json().await.map_err(|e| {
        ToolError::kinded(
            ToolErrorKind::ParseError,
            format!("github repo metadata malformed JSON: {e}"),
        )
    })?;
    let branch = meta
        .get("default_branch")
        .and_then(|v| v.as_str())
        .unwrap_or("main")
        .to_string();

    // 2. Recursive git tree for that branch.
    let tree_url = format!(
        "https://api.github.com/repos/{owner}/{repo}/git/trees/{branch}?recursive=1"
    );
    let tree_resp = client
        .get(&tree_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
        .map_err(|e| {
            ToolError::kinded(
                ToolErrorKind::NetworkError,
                format!("github git-trees request failed: {e}"),
            )
        })?;
    if !tree_resp.status().is_success() {
        let status = tree_resp.status();
        let kind = match status.as_u16() {
            403 | 429 => ToolErrorKind::RateLimited,
            404 => ToolErrorKind::ResourceNotFound,
            _ => ToolErrorKind::UpstreamError,
        };
        return Err(ToolError::kinded(
            kind,
            format!("github git-trees for {owner}/{repo}@{branch} returned {status}"),
        ));
    }
    let tree_body: serde_json::Value = tree_resp.json().await.map_err(|e| {
        ToolError::kinded(
            ToolErrorKind::ParseError,
            format!("github git-trees malformed JSON: {e}"),
        )
    })?;

    let paths = parse_tree_paths(&tree_body);
    match match_skill_dir(&paths, skill_id) {
        Some(dir_path) => Ok(ResolvedSkill { dir_path, branch }),
        None => {
            let truncated = tree_body
                .get("truncated")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let hint = if truncated {
                " (repo tree was truncated by GitHub; the skill may exist deeper)"
            } else {
                ""
            };
            Err(ToolError::kinded(
                ToolErrorKind::ResourceNotFound,
                format!(
                    "no `{skill_id}/SKILL.md` found in {owner}/{repo}@{branch}{hint}. \
                     Try another candidate from skill_marketplace_search."
                ),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_skill_dir_top_level() {
        let paths = vec!["xlsx/SKILL.md".to_string(), "README.md".to_string()];
        assert_eq!(match_skill_dir(&paths, "xlsx"), Some("xlsx".to_string()));
    }

    #[test]
    fn match_skill_dir_nested() {
        let paths = vec![
            "skills/excel-automation/SKILL.md".to_string(),
            "skills/excel-automation/helper.py".to_string(),
        ];
        assert_eq!(
            match_skill_dir(&paths, "excel-automation"),
            Some("skills/excel-automation".to_string())
        );
    }

    #[test]
    fn match_skill_dir_prefers_shallowest() {
        let paths = vec![
            "deep/nested/xlsx/SKILL.md".to_string(),
            "xlsx/SKILL.md".to_string(),
        ];
        assert_eq!(match_skill_dir(&paths, "xlsx"), Some("xlsx".to_string()));
    }

    #[test]
    fn match_skill_dir_none() {
        let paths = vec!["other/SKILL.md".to_string()];
        assert_eq!(match_skill_dir(&paths, "xlsx"), None);
    }

    #[test]
    fn match_skill_dir_ignores_substring_false_match() {
        // "my-xlsx" must NOT match skillId "xlsx".
        let paths = vec!["my-xlsx/SKILL.md".to_string()];
        assert_eq!(match_skill_dir(&paths, "xlsx"), None);
    }

    #[test]
    fn parse_tree_paths_extracts_blob_paths() {
        let body: serde_json::Value = serde_json::from_str(
            r#"{ "tree": [
                 { "path": "skills/xlsx/SKILL.md", "type": "blob" },
                 { "path": "skills/xlsx", "type": "tree" }
               ], "truncated": false }"#,
        )
        .unwrap();
        let paths = parse_tree_paths(&body);
        assert_eq!(paths, vec!["skills/xlsx/SKILL.md".to_string()]);
    }
}
