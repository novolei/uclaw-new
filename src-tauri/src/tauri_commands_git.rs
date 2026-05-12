//! Tauri command wrappers for the W6 git module (`src-tauri/src/git/`).
//!
//! Sandbox model:
//!   - **Read commands** (status, diff, is_repo, branches, current_branch,
//!     default_branch, gh_available) accept any cwd that canonicalize-
//!     compares equal to a registered `MountRoot.path`.
//!   - **Write commands** (init_repo, checkout, create_branch, commit,
//!     gh_create_pr, gh_create_issue, git_commit_push_pr) additionally
//!     require `MountRoot.editable == true`. Added in Task 9.
//!
//! Sandbox source-of-truth: `state.files_rail_list_mounts(None).await`.
//! Workspace mounts default editable=true; AttachedDirs default false
//! and the user opts in via the existing W3 mount toggle.
//!
//! Concurrency:
//!   - Local git ops wrap in `tokio::task::spawn_blocking` (required
//!     because std::Command is sync).
//!   - Network ops (`gh pr create`, `git push`) use the `*_async`
//!     siblings in `git::github::pr` directly so they don't park a
//!     blocking-pool worker during the GitHub round-trip.

use std::path::PathBuf;

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::git::{branch, github, repo, status};

// ─── Sandbox helpers ──────────────────────────────────────────────────

/// Resolve `cwd` against the user's registered MountRoots. Accepts any
/// mount (workspace, session, attached_dir) — read-permissive.
pub(crate) async fn assert_cwd_in_any_mount(
    state: &AppState,
    cwd: &str,
) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(cwd);
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|e| format!("invalid cwd '{cwd}': {e}"))?;

    let mounts = state
        .files_rail_list_mounts(None)
        .await
        .map_err(|e| format!("failed to list mounts: {e}"))?;

    for mount in mounts {
        let Ok(canonical_mount) = mount.path.canonicalize() else {
            continue;
        };
        if canonical_candidate == canonical_mount
            || canonical_candidate.starts_with(&canonical_mount)
        {
            return Ok(canonical_candidate);
        }
    }
    Err(format!(
        "cwd '{cwd}' is not inside any registered workspace or attached directory"
    ))
}

/// Resolve `cwd` against MountRoots AND require the matching mount to
/// be `editable`. Write-gated.
pub(crate) async fn assert_cwd_in_editable_mounts(
    state: &AppState,
    cwd: &str,
) -> Result<PathBuf, String> {
    let candidate = PathBuf::from(cwd);
    let canonical_candidate = candidate
        .canonicalize()
        .map_err(|e| format!("invalid cwd '{cwd}': {e}"))?;

    let mounts = state
        .files_rail_list_mounts(None)
        .await
        .map_err(|e| format!("failed to list mounts: {e}"))?;

    for mount in mounts {
        let Ok(canonical_mount) = mount.path.canonicalize() else {
            continue;
        };
        if canonical_candidate == canonical_mount
            || canonical_candidate.starts_with(&canonical_mount)
        {
            if mount.editable {
                return Ok(canonical_candidate);
            } else {
                return Err(format!(
                    "cwd '{cwd}' is inside mount '{}' which is read-only; \
                     enable write access via the files-rail mount toggle to proceed",
                    mount.label
                ));
            }
        }
    }
    Err(format!(
        "cwd '{cwd}' is not inside any registered editable mount"
    ))
}

/// Wrap a sync git op in spawn_blocking so it doesn't park the async
/// runtime. Errors collapse to String via GitError's Display impl.
pub(crate) async fn run_blocking<F, T>(work: F) -> Result<T, String>
where
    F: FnOnce() -> Result<T, String> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| err.to_string())?
}

// ─── Response DTOs ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitOutcome {
    pub status: String,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePrResponse {
    pub url: String,
    pub was_existing: bool,
    pub base: String,
}

// ─── Read commands ────────────────────────────────────────────────────

#[tauri::command]
pub async fn git_status(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<Option<String>, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    run_blocking(move || status::read_status(&path).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn git_diff(
    state: State<'_, AppState>,
    cwd: String,
    full: Option<bool>,
) -> Result<Option<String>, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    let mode = if full.unwrap_or(false) {
        status::DiffMode::Full
    } else {
        status::DiffMode::Stat
    };
    run_blocking(move || status::read_diff_with_mode(&path, mode).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn git_is_repo(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<bool, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    Ok(run_blocking(move || Ok::<_, String>(repo::is_inside_repo(&path)))
        .await
        .unwrap_or(false))
}

#[tauri::command]
pub async fn git_branches(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<String, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    run_blocking(move || branch::list_branches_verbose(&path).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn git_current_branch(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<String, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    run_blocking(move || branch::current_branch(&path).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn git_default_branch(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<String, String> {
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    run_blocking(move || branch::detect_default_branch(&path).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn gh_available() -> Result<bool, String> {
    Ok(run_blocking(move || Ok::<_, String>(github::is_gh_available()))
        .await
        .unwrap_or(false))
}
