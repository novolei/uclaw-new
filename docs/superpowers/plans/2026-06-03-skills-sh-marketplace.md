# skills.sh Marketplace Search + Install Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace `skill_marketplace_search`'s dead GitHub code-search backend (always 401s) with the no-auth skills.sh `/api/search` registry, and teach `skill_install_from_marketplace` to install a skills.sh result by `(source=owner/repo, skill_id)` via a new git-trees path-resolution module.

**Architecture:** Search hits `https://skills.sh/api/search` (anonymous, semantic, sorted by installs). A new `skill_marketplace_resolve` module maps a skills.sh `(owner/repo, skillId)` to the in-repo SKILL.md directory using GitHub's anonymous git-trees API. The existing native install pipeline (contents API + raw download → `~/.uclaw/skills/_marketplace/`) is reused unchanged. Local `skill_search` is untouched. No external runtimes, no API keys, no GitHub token.

**Tech Stack:** Rust, `reqwest` (async HTTP, already a dep), `serde`/`serde_json`, `tokio` tests. All new logic factored into pure functions that parse fixture JSON so tests never touch the network.

**Spec:** `docs/superpowers/specs/2026-06-03-skills-sh-marketplace-design.md`

---

## Pre-flight (do once before Task 1)

The GitNexus index is stale (last indexed `168e48b`). Per CLAUDE.md, refresh it and run impact analysis on the symbol being rewritten.

- [ ] **Refresh the code index**

Run: `npx gitnexus analyze`
Expected: completes, reports `skill_marketplace` symbols indexed.

- [ ] **Impact-check the rewrite target**

Run impact analysis on `query_marketplace` (the function being rewritten):
`gitnexus_impact({target: "query_marketplace", direction: "upstream"})`
Expected: only caller is `SkillMarketplaceSearchTool::execute` (same file). If the blast radius is larger than that, STOP and report before editing.

---

## File Structure

| File | Responsibility |
|---|---|
| `src-tauri/src/agent/tools/builtin/skill_marketplace.rs` (modify) | Both tools. Search backend swapped to skills.sh; install gains `skill_id` resolution. |
| `src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs` (**new**) | Pure tree-path matching + the async `resolve_skill_path` git-trees call. One responsibility: `(owner/repo, skillId) → in-repo skill dir + branch`. |
| `src-tauri/src/agent/tools/builtin/mod.rs` (modify) | Declare the new module. |

**Reference facts (verified live):**

skills.sh search — `GET https://skills.sh/api/search?q=<q>&limit=<n>`, no auth, returns:
```json
{ "query": "...", "searchType": "fuzzy",
  "skills": [
    { "id": "claude-office-skills/skills/excel-automation", "skillId": "excel-automation",
      "name": "excel-automation", "installs": 9529, "source": "claude-office-skills/skills" }
  ],
  "count": 1, "duration_ms": 141 }
```
Per-skill fields used: `skillId`, `name`, `installs`, `source` (= `owner/repo`), optional `isDuplicate` (bool).

`ToolErrorKind` variants available: `InvalidInput`, `ResourceNotFound`, `PermissionDenied`, `Timeout`, `NetworkError`, `UpstreamError`, `RateLimited`, `PayloadTooLarge`, `ParseError`, `Unavailable`, `PreconditionFailed`, `Other`. Construct via `ToolError::kinded(kind, msg)`. Output via `ToolOutput::new(json, elapsed_ms)`.

Verification commands (from repo root):
- Build: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
- Test: `cd src-tauri && cargo test --lib skill_marketplace 2>&1 | tail -20`

---

## Task 1: Swap search backend to skills.sh

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/skill_marketplace.rs` — module doc (lines 1-26), search `description()` (line 72-74), `execute` output note (lines 128-142), `query_marketplace` (lines 146-282).
- Test: same file, inline `#[cfg(test)] mod tests`.

The strategy: split the network call from the parsing. `query_marketplace` does HTTP only; a new pure `parse_search_response(&Value, limit)` does deserialize + dedup + shape. Tests drive the pure function with fixtures.

- [ ] **Step 1: Write the failing test for `parse_search_response`**

Add to the `mod tests` block at the bottom of the file:

```rust
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

    let out = parse_search_response(&body, 8);

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
    assert!(parse_search_response(&empty, 8).is_empty());

    let body: serde_json::Value = serde_json::json!({
        "skills": [
            { "skillId": "one", "name": "one", "installs": 3, "source": "o/1" },
            { "skillId": "two", "name": "two", "installs": 2, "source": "o/2" }
        ]
    });
    assert_eq!(parse_search_response(&body, 1).len(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib parse_search_response 2>&1 | tail -15`
Expected: FAIL to compile — `cannot find function parse_search_response in this scope`.

- [ ] **Step 3: Add serde import and the deserialization structs + pure parser**

At the top of the file, add to the imports (after line 32 `use async_trait::async_trait;`):

```rust
use serde::Deserialize;
```

Add a new const next to the existing consts (after line 43 `const SEARCH_TIMEOUT_MS: u64 = 10_000;`):

```rust
const SEARCH_API_BASE: &str = "https://skills.sh";
```

Add the structs + pure parser immediately above `fn query_marketplace` (i.e. replace the old doc comment at lines 146-149):

```rust
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
fn parse_search_response(body: &serde_json::Value, limit: usize) -> Vec<serde_json::Value> {
    let parsed: SkillsShSearchResponse =
        serde_json::from_value(body.clone()).unwrap_or(SkillsShSearchResponse { skills: Vec::new() });

    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut results: Vec<serde_json::Value> = Vec::new();
    for s in parsed.skills {
        if s.skill_id.is_empty() || s.source.is_empty() {
            continue;
        }
        if !seen.insert(s.skill_id.clone()) {
            continue; // duplicate skillId — first (highest installs) wins
        }
        // Bind locals so `skill_id`/`source` can be used twice (top-level
        // field + installHint) without a move-after-move error.
        let skill_id = s.skill_id;
        let source = s.source;
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
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd src-tauri && cargo test --lib parse_search_response 2>&1 | tail -15`
Expected: PASS (2 tests).

- [ ] **Step 5: Rewrite `query_marketplace` to call skills.sh and delegate to the parser**

Replace the body of `query_marketplace` (the GitHub block, old lines 165-281) so the function reads:

```rust
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

    Ok(parse_search_response(&body, limit))
}
```

- [ ] **Step 6: Update the search tool description and the install-hint note**

Replace `description()` (lines 72-74) with:

```rust
    fn description(&self) -> &str {
        "Search the open agent-skills ecosystem (skills.sh) for community skills matching a query. Semantic search; results are ranked by install count (popularity). Returns candidate `skillId` + `name` + `source` (owner/repo) + `installs`, plus an `installHint`. Use when the user asks \"is there a skill for X\" or \"find a skill that does X\". To install one, pass that result's `installHint.source` and `installHint.skill_id` to skill_install_from_marketplace (which requires user approval)."
    }
```

Replace the `"note"` ternary in `execute` (lines 135-139) with:

```rust
                "note": if result_count == 0 {
                    "No skills found. Try a different query, or check if a relevant skill already exists locally via skill_search."
                } else {
                    "To install one, call skill_install_from_marketplace with `source` and `skill_id` copied from a result's `installHint`. The install will require user approval."
                },
```

- [ ] **Step 7: Update the module doc comment (lines 1-26)**

Replace lines 10-15 (the `skill_marketplace_search` bullet describing GitHub Code Search) with:

```rust
//! 1. `skill_marketplace_search` — discover candidate skills by
//!    keyword via the skills.sh registry (`/api/search`, public, no
//!    auth, semantic, ranked by install count). Returns skillId +
//!    name + source (owner/repo) + installs.
```

- [ ] **Step 8: Build and run the full marketplace test set**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (clean compile).
Run: `cd src-tauri && cargo test --lib skill_marketplace 2>&1 | tail -20`
Expected: all pass (the pre-existing empty/missing-query tests + the 2 new parser tests).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/agent/tools/builtin/skill_marketplace.rs
git commit -m "feat(skills): search via skills.sh /api/search (no auth), drop dead GitHub code-search

Replaces the always-401 GitHub /search/code path with the anonymous
skills.sh registry. Pure parse_search_response (dedup by skillId, ranked
by installs) is fixture-tested. Results carry an installHint(source,
skill_id) for the install tool."
```

---

## Task 2: New `skill_marketplace_resolve` module

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs:16` (declare module)

This module turns a skills.sh `(owner/repo, skillId)` into the directory that holds that skill's `SKILL.md`, plus the branch it found it on. Two pure helpers (`parse_tree_paths`, `match_skill_dir`) are fixture-tested; one async `resolve_skill_path` makes the two anonymous GitHub calls.

- [ ] **Step 1: Declare the module**

In `src-tauri/src/agent/tools/builtin/mod.rs`, add directly below line 16 (`pub mod skill_marketplace;`):

```rust
pub mod skill_marketplace_resolve;
```

- [ ] **Step 2: Write the failing tests for the pure helpers**

Create `src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs` with ONLY the test module first (so it fails to compile against missing fns):

```rust
// SPDX-License-Identifier: Apache-2.0
//! Resolve a skills.sh `(owner/repo, skillId)` to the in-repo skill
//! directory (the one containing SKILL.md) via GitHub's anonymous
//! git-trees API. skills.sh `/api/search` returns `source` + `skillId`
//! but not the path inside the repo; this module supplies it.

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
```

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test --lib skill_marketplace_resolve 2>&1 | tail -15`
Expected: FAIL to compile — `cannot find function match_skill_dir` / `parse_tree_paths`.

- [ ] **Step 4: Implement the pure helpers + the resolved struct**

Insert above the `#[cfg(test)]` block:

```rust
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
```

- [ ] **Step 5: Run the pure-helper tests to verify they pass**

Run: `cd src-tauri && cargo test --lib skill_marketplace_resolve 2>&1 | tail -15`
Expected: PASS (6 tests).

- [ ] **Step 6: Implement the async `resolve_skill_path`**

Append (above the `#[cfg(test)]` block):

```rust
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
```

- [ ] **Step 7: Build and re-run the module tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib skill_marketplace_resolve 2>&1 | tail -15`
Expected: PASS (6 tests).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/agent/tools/builtin/skill_marketplace_resolve.rs src-tauri/src/agent/tools/builtin/mod.rs
git commit -m "feat(skills): skill_marketplace_resolve — (owner/repo, skillId) -> in-repo SKILL.md dir

Anonymous GitHub default-branch + recursive git-trees lookup. Pure
match_skill_dir/parse_tree_paths are fixture-tested (top-level, nested,
shallowest-wins, substring-safe, no-match). git-trees needs no auth,
unlike /search/code."
```

---

## Task 3: Wire install to accept `(source=owner/repo, skill_id)`

**Files:**
- Modify: `src-tauri/src/agent/tools/builtin/skill_marketplace.rs` — install `description()` (lines 325-327), `parameters_schema()` (lines 329-350), and the parsing block in `execute` (lines 361-394, plus the `git_ref` usages at lines 431-435 and 452).

Behavior: if `source` is exactly `owner/repo` (2 segments), require `skill_id`, resolve the path + branch via Task 2; otherwise keep the existing `owner/repo/<path>` parsing. The resolved branch overrides `ref` only on the 2-segment path.

- [ ] **Step 1: Write the failing test for the 2-segment-needs-skill_id guard**

Add to the `mod tests` block (this path is reachable without network — it errors before any HTTP):

```rust
#[tokio::test]
async fn install_two_segment_source_requires_skill_id() {
    let err = super::install_resolve_source("owner/repo", &None, "main")
        .await
        .unwrap_err();
    assert!(format!("{err}").contains("skill_id"));
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cd src-tauri && cargo test --lib install_ 2>&1 | tail -15`
Expected: FAIL to compile — `cannot find function parse_install_source` / `install_resolve_source`.

- [ ] **Step 3: Add the two helper functions**

Add these free functions in `skill_marketplace.rs`, directly below `fn truncate_for_error` (after line 290):

```rust
/// Parse a fully-qualified `owner/repo/<path>` install source. Returns
/// `(owner, repo, skill_path)` when there are ≥3 segments, else `None`
/// (a bare `owner/repo` needs `skill_id` resolution instead).
fn parse_install_source(source: &str) -> Option<(String, String, String)> {
    let parts: Vec<&str> = source.split('/').collect();
    if parts.len() < 3 {
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
    skill_id: &Option<String>,
    git_ref: &str,
) -> Result<(String, String, String, String), ToolError> {
    if let Some((owner, repo, skill_path)) = parse_install_source(source) {
        return Ok((owner, repo, skill_path, git_ref.to_string()));
    }
    // Bare owner/repo — needs skill_id.
    let parts: Vec<&str> = source.split('/').collect();
    if parts.len() != 2 {
        return Err(ToolError::kinded(
            ToolErrorKind::InvalidInput,
            format!(
                "source {source:?} must be `owner/repo/<skill-dir>` or `owner/repo` with a `skill_id`"
            ),
        ));
    }
    let sid = skill_id.as_deref().filter(|s| !s.is_empty()).ok_or_else(|| {
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --lib install_ 2>&1 | tail -15`
Expected: PASS (4 tests).

- [ ] **Step 5: Rewrite the `execute` parsing block to use the helper**

In `SkillInstallFromMarketplaceTool::execute`, replace the block from line 370 (`let git_ref = ...`) through line 394 (`let skill_path = parts[2..].join("/");`) with:

```rust
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
            install_resolve_source(&source, &skill_id, &git_ref).await?;
```

Note: this single replacement spans the original lines 370-394 inclusive — i.e. it subsumes the old `git_ref` (370-374), `force` (376-379), and `owner`/`repo`/`skill_path` parsing (381-394) blocks, re-declaring `git_ref` and `force` exactly once. There is no duplication to clean up afterward. `owner`/`repo` are now `String` (were `&str`); the later `format!` calls at lines 431-435 and 452 accept `&String` unchanged, and `git_ref` there is now the resolved branch.

- [ ] **Step 6: Add `skill_id` to the install schema and update the description**

Replace `parameters_schema()` (lines 329-350) with:

```rust
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
```

Replace `description()` (lines 325-327) with:

```rust
    fn description(&self) -> &str {
        "Install a skill from a public GitHub repo into ~/.uclaw/skills/_marketplace/. Use when the user accepts a skill_marketplace_search suggestion — pass that result's `installHint.source` (owner/repo) and `installHint.skill_id`. You may also pass a fully-qualified `source` = `owner/repo/<path-to-skill-dir>` directly. Requires user approval because it fetches third-party code and persists it across all future sessions."
    }
```

- [ ] **Step 7: Build and run the full marketplace + resolve test set**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib skill_marketplace 2>&1 | tail -25`
Expected: all pass.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/agent/tools/builtin/skill_marketplace.rs
git commit -m "feat(skills): install accepts (source=owner/repo, skill_id) via git-trees resolve

skill_marketplace_search results install directly: their installHint
(source, skill_id) resolves to the in-repo SKILL.md dir + default branch,
then the existing contents+raw fetch pipeline runs unchanged. The
fully-qualified owner/repo/<path> form still works for directly-named skills."
```

---

## Task 4: End-to-end verification

**Files:** none (verification only).

- [ ] **Step 1: Full backend build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.

- [ ] **Step 2: Full marketplace test suite**

Run: `cd src-tauri && cargo test --lib skill_marketplace 2>&1 | tail -25`
Expected: all green (parser dedup/limit, resolve match/parse helpers, install source parsing, pre-existing query-validation tests).

- [ ] **Step 3: Confirm changes are scoped as expected**

Run: `gitnexus_detect_changes()`
Expected: only `query_marketplace`, `parse_search_response`, the new `skill_marketplace_resolve` symbols, and the install tool's `execute`/schema/description are reported. If anything else shows up, investigate before proceeding.

- [ ] **Step 4: Manual end-to-end smoke (real network, in the running app)**

Launch the app, then in Agent mode prompt: "帮我搜索做 Excel 的 skill". Expect `skill_marketplace_search` to return candidates (e.g. `excel-automation`, installs > 0) with no 401. Pick one and confirm `skill_install_from_marketplace` (with the installHint's source + skill_id) prompts for approval, writes to `~/.uclaw/skills/_marketplace/…/SKILL.md`, and that a subsequent `skill_search` hits it.

Record the result (pass/fail + any error text) in the PR body. If step 4 can't run in this environment, note that it's deferred to manual QA rather than marking it done.

---

## Definition of done

- skills.sh search returns candidates with no 401 (the original bug is gone).
- Install resolves a skills.sh `(source, skill_id)` to files in `~/.uclaw/skills/_marketplace/` and re-registers them.
- Local `skill_search` is byte-for-byte unchanged.
- No GitHub token, no `npx`, no new external runtime introduced.
- `cargo build` clean; all `skill_marketplace*` unit tests pass.
