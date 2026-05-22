use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::PathBuf;
use tokio::process::Command;
pub use super::protocol::{Repo, Issue, PullRequest};

const ISSUE_STATE_BATCH_SIZE: usize = 25;

const SEARCH_PATHS: &[&str] = &[
    "/opt/homebrew/bin",
    "/usr/local/bin",
    "/usr/bin",
    "/bin",
    "/usr/sbin",
    "/sbin",
];

const HOME_RELATIVE_PATHS: &[&str] = &[".local/bin", ".cargo/bin"];

fn preferred_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = SEARCH_PATHS.iter().map(PathBuf::from).collect();

    if let Some(home) = dirs::home_dir() {
        for rel in HOME_RELATIVE_PATHS {
            dirs.push(home.join(rel));
        }
    }

    dirs
}

/// Build a predictable PATH for subprocesses launched from the app bundle.
///
/// macOS GUI apps started from Finder often miss Homebrew and user-local paths,
/// which breaks scripts like `codex` that use `#!/usr/bin/env node`.
pub fn build_path_env() -> String {
    let mut ordered_paths: Vec<PathBuf> = Vec::new();
    let mut seen = HashSet::new();

    if let Some(current_path) = std::env::var_os("PATH") {
        for path in std::env::split_paths(&current_path) {
            if seen.insert(path.clone()) {
                ordered_paths.push(path);
            }
        }
    }

    for path in preferred_dirs() {
        if seen.insert(path.clone()) {
            ordered_paths.push(path);
        }
    }

    std::env::join_paths(ordered_paths)
        .unwrap_or_default()
        .to_string_lossy()
        .to_string()
}

/// Resolve a binary name to its full path, searching common macOS locations.
/// Falls back to the bare name if not found (letting the OS try).
pub fn resolve(name: &str) -> String {
    for dir in preferred_dirs() {
        let candidate = dir.join(name);
        if candidate.exists() {
            return candidate.to_string_lossy().to_string();
        }
    }

    name.to_string()
}

#[async_trait]
pub trait GitHubGateway: Send + Sync {
    async fn list_repos(&self, filter: Option<String>) -> Result<Vec<Repo>, String>;
    async fn list_issues(
        &self,
        repo: &str,
        state: Option<&str>,
        label: Option<&str>,
    ) -> Result<Vec<Issue>, String>;
    async fn list_open_prs(&self, repo: &str) -> Result<Vec<PullRequest>, String>;
    async fn get_issue_states(
        &self,
        repo: &str,
        issue_numbers: &[u64],
    ) -> Result<HashMap<u64, String>, String>;
    async fn get_issue_state(&self, repo: &str, issue_number: u64) -> Result<String, String>;
    async fn get_issue_detail(&self, repo: &str, number: u64) -> Result<Issue, String>;
    async fn is_pr_merged_for_issue(&self, repo: &str, issue_number: u64) -> Result<bool, String>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GhCliGateway;

#[async_trait]
impl GitHubGateway for GhCliGateway {
    async fn list_repos(&self, filter: Option<String>) -> Result<Vec<Repo>, String> {
        let json_fields = "nameWithOwner,name,owner,description,url,defaultBranchRef,isPrivate";
        let output = run_gh(&["repo", "list", "--limit", "100", "--json", json_fields]).await?;
        let raw: Vec<serde_json::Value> = serde_json::from_str(&output)
            .map_err(|e| format!("Failed to parse repos JSON: {}", e))?;

        let mut repos: Vec<Repo> = raw.iter().map(parse_repo).collect();
        if let Some(filter) = filter {
            let filter = filter.to_lowercase();
            repos.retain(|repo| repo.full_name.to_lowercase().contains(&filter));
        }

        Ok(repos)
    }

    async fn list_issues(
        &self,
        repo: &str,
        state: Option<&str>,
        label: Option<&str>,
    ) -> Result<Vec<Issue>, String> {
        let json_fields = "number,title,body,state,labels,assignees,url,createdAt,updatedAt";
        let state_filter = state.unwrap_or("open");
        let mut args = vec![
            "issue",
            "list",
            "-R",
            repo,
            "--state",
            state_filter,
            "--limit",
            "100",
            "--json",
            json_fields,
        ];

        if let Some(label) = label {
            args.push("--label");
            args.push(label);
        }

        let output = run_gh(&args).await?;
        let raw: Vec<serde_json::Value> = serde_json::from_str(&output)
            .map_err(|e| format!("Failed to parse issues JSON: {}", e))?;

        Ok(raw.iter().map(parse_issue).collect())
    }

    async fn list_open_prs(&self, repo: &str) -> Result<Vec<PullRequest>, String> {
        let json_fields = "number,title,body,state,headRefName,url,createdAt,updatedAt,author";
        let output = run_gh(&[
            "pr",
            "list",
            "-R",
            repo,
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            json_fields,
        ])
        .await?;

        let raw: Vec<serde_json::Value> = serde_json::from_str(&output)
            .map_err(|e| format!("Failed to parse PRs JSON: {}", e))?;

        Ok(raw.iter().map(parse_pull_request).collect())
    }

    async fn get_issue_states(
        &self,
        repo: &str,
        issue_numbers: &[u64],
    ) -> Result<HashMap<u64, String>, String> {
        let issue_numbers = unique_issue_numbers(issue_numbers);
        if issue_numbers.is_empty() {
            return Ok(HashMap::new());
        }

        let (owner, name) = split_repo_full_name(repo)?;
        let mut states = HashMap::new();

        for chunk in issue_numbers.chunks(ISSUE_STATE_BATCH_SIZE) {
            let query = build_issue_states_query(owner, name, chunk)?;
            let query_arg = format!("query={}", query);
            let output = run_gh(&["api", "graphql", "-f", &query_arg]).await?;
            let response: serde_json::Value = serde_json::from_str(&output)
                .map_err(|e| format!("Failed to parse issue states JSON: {}", e))?;

            if let Some(errors) = response["errors"].as_array() {
                if let Some(message) = errors
                    .iter()
                    .filter_map(|error| error["message"].as_str())
                    .next()
                {
                    return Err(format!("GitHub GraphQL failed: {}", message));
                }
            }

            let repository = response["data"]["repository"]
                .as_object()
                .ok_or_else(|| "GitHub GraphQL response missing repository data".to_string())?;

            for issue_number in chunk {
                let field_name = format!("issue_{}", issue_number);
                if let Some(state) = repository
                    .get(&field_name)
                    .and_then(|issue| issue["state"].as_str())
                {
                    states.insert(*issue_number, state.to_string());
                }
            }
        }

        Ok(states)
    }

    async fn get_issue_state(&self, repo: &str, issue_number: u64) -> Result<String, String> {
        let issue_number = issue_number.to_string();
        let output = run_gh(&[
            "issue",
            "view",
            &issue_number,
            "-R",
            repo,
            "--json",
            "state",
        ])
        .await?;
        let value: serde_json::Value = serde_json::from_str(&output)
            .map_err(|e| format!("Failed to parse issue JSON: {}", e))?;
        Ok(value["state"].as_str().unwrap_or("OPEN").to_string())
    }

    async fn get_issue_detail(&self, repo: &str, number: u64) -> Result<Issue, String> {
        let issue_number = number.to_string();
        let json_fields = "number,title,body,state,labels,assignees,url,createdAt,updatedAt";
        let output = run_gh(&[
            "issue",
            "view",
            &issue_number,
            "-R",
            repo,
            "--json",
            json_fields,
        ])
        .await?;

        let value: serde_json::Value = serde_json::from_str(&output)
            .map_err(|e| format!("Failed to parse issue JSON: {}", e))?;
        Ok(parse_issue(&value))
    }

    async fn is_pr_merged_for_issue(&self, repo: &str, issue_number: u64) -> Result<bool, String> {
        let output = run_gh(&[
            "pr",
            "list",
            "-R",
            repo,
            "--state",
            "all",
            "--limit",
            "50",
            "--json",
            "number,title,body,state",
        ])
        .await?;

        let prs: Vec<serde_json::Value> =
            serde_json::from_str(&output).map_err(|e| format!("Failed to parse PRs: {}", e))?;

        for pr in &prs {
            let body = pr["body"].as_str().unwrap_or("");
            let title = pr["title"].as_str().unwrap_or("");
            let state = pr["state"].as_str().unwrap_or("");

            let references_issue = parse_closes_issue(body) == Some(issue_number)
                || parse_issue_from_title(title) == Some(issue_number);

            if references_issue {
                return Ok(state == "MERGED");
            }
        }

        Err(format!("No PR found referencing issue #{}", issue_number))
    }
}

pub fn cli_gateway() -> GhCliGateway {
    GhCliGateway
}

async fn run_gh(args: &[&str]) -> Result<String, String> {
    let output = Command::new(resolve("gh"))
        .env("PATH", build_path_env())
        .args(args)
        .output()
        .await
        .map_err(|e| {
            format!(
                "Failed to run gh CLI: {}. Make sure gh is installed and authenticated.",
                e
            )
        })?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("gh command failed: {}", stderr.trim()));
    }

    String::from_utf8(output.stdout).map_err(|e| format!("Invalid UTF-8 output: {}", e))
}

fn parse_repo(value: &serde_json::Value) -> Repo {
    let owner = value["owner"]["login"].as_str().unwrap_or("").to_string();
    Repo {
        full_name: value["nameWithOwner"].as_str().unwrap_or("").to_string(),
        name: value["name"].as_str().unwrap_or("").to_string(),
        owner,
        description: value["description"].as_str().map(|s| s.to_string()),
        url: value["url"].as_str().unwrap_or("").to_string(),
        default_branch: value["defaultBranchRef"]
            .as_object()
            .and_then(|branch| branch["name"].as_str())
            .unwrap_or("main")
            .to_string(),
        is_private: value["isPrivate"].as_bool().unwrap_or(false),
    }
}

fn parse_issue(value: &serde_json::Value) -> Issue {
    let labels = value["labels"]
        .as_array()
        .map(|labels| {
            labels
                .iter()
                .filter_map(|label| label["name"].as_str().map(|name| name.to_string()))
                .collect()
        })
        .unwrap_or_default();

    let assignee = value["assignees"]
        .as_array()
        .and_then(|assignees| assignees.first())
        .and_then(|assignee| assignee["login"].as_str())
        .map(|login| login.to_string());

    Issue {
        number: value["number"].as_u64().unwrap_or(0),
        title: value["title"].as_str().unwrap_or("").to_string(),
        body: value["body"].as_str().map(|s| s.to_string()),
        state: value["state"].as_str().unwrap_or("OPEN").to_string(),
        labels,
        assignee,
        url: value["url"].as_str().unwrap_or("").to_string(),
        created_at: value["createdAt"].as_str().unwrap_or("").to_string(),
        updated_at: value["updatedAt"].as_str().unwrap_or("").to_string(),
    }
}

fn parse_pull_request(value: &serde_json::Value) -> PullRequest {
    let body = value["body"].as_str().map(|s| s.to_string());
    let closes_issue = body.as_ref().and_then(|body| parse_closes_issue(body));

    PullRequest {
        number: value["number"].as_u64().unwrap_or(0),
        title: value["title"].as_str().unwrap_or("").to_string(),
        body,
        state: value["state"].as_str().unwrap_or("OPEN").to_string(),
        head_branch: value["headRefName"].as_str().unwrap_or("").to_string(),
        url: value["url"].as_str().unwrap_or("").to_string(),
        created_at: value["createdAt"].as_str().unwrap_or("").to_string(),
        updated_at: value["updatedAt"].as_str().unwrap_or("").to_string(),
        author: value["author"]
            .as_object()
            .and_then(|author| author["login"].as_str())
            .map(|login| login.to_string()),
        closes_issue,
    }
}

fn split_repo_full_name(repo: &str) -> Result<(&str, &str), String> {
    repo.split_once('/')
        .ok_or_else(|| format!("Invalid repo full name: {}", repo))
}

fn build_issue_states_query(
    owner: &str,
    name: &str,
    issue_numbers: &[u64],
) -> Result<String, String> {
    let owner = serde_json::to_string(owner)
        .map_err(|e| format!("Failed to encode GitHub owner for GraphQL: {}", e))?;
    let name = serde_json::to_string(name)
        .map_err(|e| format!("Failed to encode GitHub repo name for GraphQL: {}", e))?;
    let fields = issue_numbers
        .iter()
        .map(|issue_number| {
            format!(
                "issue_{0}: issue(number: {0}) {{ number state }}",
                issue_number
            )
        })
        .collect::<Vec<_>>()
        .join(" ");

    Ok(format!(
        "query {{ repository(owner: {owner}, name: {name}) {{ {fields} }} }}",
    ))
}

fn unique_issue_numbers(issue_numbers: &[u64]) -> Vec<u64> {
    let mut seen = BTreeSet::new();
    issue_numbers
        .iter()
        .copied()
        .filter(|issue_number| *issue_number > 0 && seen.insert(*issue_number))
        .collect()
}

/// Parse blocker references from issue body text.
/// Looks for patterns like "blocked by #X", "depends on #X", "requires #X".
pub fn parse_blockers(text: &str) -> Vec<u64> {
    let text_lower = text.to_lowercase();
    let mut blockers = Vec::new();
    let patterns = [
        "blocked by #",
        "depends on #",
        "requires #",
        "waiting on #",
        "waiting for #",
        "after #",
    ];

    for pattern in &patterns {
        let mut search_from = 0;
        while let Some(pos) = text_lower[search_from..].find(pattern) {
            let abs_pos = search_from + pos + pattern.len();
            let after = &text_lower[abs_pos..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                if n > 0 && !blockers.contains(&n) {
                    blockers.push(n);
                }
            }
            search_from = abs_pos;
        }
    }

    blockers
}

/// Parse "Closes #123" or "Fixes #123" from PR body
fn parse_closes_issue(body: &str) -> Option<u64> {
    let body_lower = body.to_lowercase();
    for keyword in &[
        "closes #",
        "fixes #",
        "resolves #",
        "close #",
        "fix #",
        "resolve #",
    ] {
        if let Some(pos) = body_lower.find(keyword) {
            let after = &body_lower[pos + keyword.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return Some(n);
            }
        }
    }
    None
}

/// Parse issue number from PR title like "Fix #14: ..."
pub fn parse_issue_from_title(title: &str) -> Option<u64> {
    let title_lower = title.to_lowercase();
    for keyword in &[
        "fix #",
        "fixes #",
        "closes #",
        "resolve #",
        "resolves #",
        "close #",
        "feat #",
        "issue #",
    ] {
        if let Some(pos) = title_lower.find(keyword) {
            let after = &title_lower[pos + keyword.len()..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                return Some(n);
            }
        }
    }
    // Try pattern "#123" anywhere
    for (index, ch) in title.char_indices() {
        if ch == '#' {
            let after = &title[index + 1..];
            let num_str: String = after.chars().take_while(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = num_str.parse::<u64>() {
                if n > 0 {
                    return Some(n);
                }
            }
        }
    }
    None
}

/// Fetch the current state of an issue (e.g. "OPEN", "CLOSED").
/// Returns the state string or an error if the check fails.
pub async fn get_issue_state(repo: &str, issue_number: u64) -> Result<String, String> {
    cli_gateway().get_issue_state(repo, issue_number).await
}

/// Check if a PR associated with a given issue number is actually merged.
/// Returns Ok(true) if merged, Ok(false) if still open/closed-not-merged, Err on failure.
pub async fn is_pr_merged_for_issue(repo: &str, issue_number: u64) -> Result<bool, String> {
    cli_gateway()
        .is_pr_merged_for_issue(repo, issue_number)
        .await
}

pub async fn get_issue_detail(repo: String, issue_number: u64) -> Result<Issue, String> {
    cli_gateway().get_issue_detail(&repo, issue_number).await
}

pub async fn list_issues(
    repo: &str,
    state: Option<&str>,
    label: Option<&str>,
) -> Result<Vec<Issue>, String> {
    cli_gateway().list_issues(repo, state, label).await
}

pub async fn list_open_prs(repo: String) -> Result<Vec<PullRequest>, String> {
    cli_gateway().list_open_prs(&repo).await
}

pub async fn get_issue_states(
    repo: &str,
    issue_numbers: &[u64],
) -> Result<HashMap<u64, String>, String> {
    cli_gateway().get_issue_states(repo, issue_numbers).await
}

pub async fn list_repos(filter: Option<String>) -> Result<Vec<Repo>, String> {
    cli_gateway().list_repos(filter).await
}
