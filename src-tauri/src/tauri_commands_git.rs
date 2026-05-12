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

// ─── Write commands (mutating; emit tracing events) ────────────────────

#[tauri::command]
pub async fn git_init_repo(
    state: State<'_, AppState>,
    cwd: String,
) -> Result<(), String> {
    // Init is permitted in any mount the user can read — they
    // explicitly opted into making the dir a workspace already.
    let path = assert_cwd_in_any_mount(&state, &cwd).await?;
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();
    let outcome = run_blocking(move || crate::git::repo::init_repo(&path).map_err(|e| e.to_string())).await;
    tracing::info!(
        op = "init_repo",
        cwd = %cwd_log,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = match &outcome { Ok(_) => "ok", Err(_) => "err" },
        "git_op:init_repo"
    );
    outcome
}

#[tauri::command]
pub async fn git_checkout_branch(
    state: State<'_, AppState>,
    cwd: String,
    name: String,
) -> Result<(), String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let name_log = name.clone();
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();
    let outcome = run_blocking(move || crate::git::branch::checkout(&path, &name).map_err(|e| e.to_string())).await;
    tracing::info!(
        op = "checkout_branch",
        cwd = %cwd_log,
        branch = %name_log,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = match &outcome { Ok(_) => "ok", Err(_) => "err" },
        "git_op:checkout_branch"
    );
    outcome
}

#[tauri::command]
pub async fn git_create_branch(
    state: State<'_, AppState>,
    cwd: String,
    name: String,
) -> Result<(), String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let name_log = name.clone();
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();
    let outcome = run_blocking(move || crate::git::branch::create_and_checkout(&path, &name).map_err(|e| e.to_string())).await;
    tracing::info!(
        op = "create_branch",
        cwd = %cwd_log,
        branch = %name_log,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = match &outcome { Ok(_) => "ok", Err(_) => "err" },
        "git_op:create_branch"
    );
    outcome
}

#[tauri::command]
pub async fn git_commit(
    state: State<'_, AppState>,
    cwd: String,
    message: String,
) -> Result<CommitOutcome, String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();

    let message_for_blocking = message.clone();
    let outcome: Result<CommitOutcome, String> = run_blocking(move || {
        match crate::git::commit::commit_all_with_message(&path, &message_for_blocking) {
            Ok(()) => Ok(CommitOutcome {
                status: "created".to_string(),
                message: message_for_blocking.trim().to_string(),
            }),
            Err(crate::git::GitError::NoWorkspaceChanges) => Ok(CommitOutcome {
                status: "skipped".to_string(),
                message: "no workspace changes to commit".to_string(),
            }),
            Err(other) => Err(other.to_string()),
        }
    })
    .await;

    tracing::info!(
        op = "commit",
        cwd = %cwd_log,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = match &outcome {
            Ok(o) => o.status.as_str(),
            Err(_) => "err",
        },
        "git_op:commit"
    );
    outcome
}

// ─── Network commands (tokio::process, not blocking pool) ────────────

#[tauri::command]
pub async fn gh_create_pr(
    state: State<'_, AppState>,
    cwd: String,
    title: String,
    body: String,
    base: Option<String>,
) -> Result<CreatePrResponse, String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();

    // Resolve base branch on the blocking pool (one git rev-parse).
    let path_for_default = path.clone();
    let resolved_base: String = match base.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(b) => b.to_string(),
        None => run_blocking(move || {
            crate::git::branch::detect_default_branch(&path_for_default).map_err(|e| e.to_string())
        })
        .await?,
    };

    // Body tempfile must survive the await — keep it alive in a let-binding.
    let body_file =
        crate::git::commit::CommitMessageFile::create(if body.trim().is_empty() {
            "(no body provided)\n"
        } else {
            &body
        })
        .map_err(|e| e.to_string())?;

    let request = crate::git::github::pr::PrCreateRequest {
        title: &title,
        body_file: body_file.path(),
        base: &resolved_base,
    };
    let outcome = crate::git::github::pr::create_async(&path, &request)
        .await
        .map_err(|e| e.to_string())
        .map(|out| CreatePrResponse {
            url: out.url,
            was_existing: out.was_existing,
            base: resolved_base.clone(),
        });

    drop(body_file);  // explicit per the design: drop AFTER the await resolves.

    tracing::info!(
        op = "pr_create",
        cwd = %cwd_log,
        title = %title,
        base = %resolved_base,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = match &outcome {
            Ok(o) if o.was_existing => "existing",
            Ok(_) => "created",
            Err(_) => "err",
        },
        "git_op:pr_create"
    );
    outcome
}

#[tauri::command]
pub async fn gh_create_issue(
    state: State<'_, AppState>,
    cwd: String,
    title: String,
    body: String,
) -> Result<String, String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();

    let body_file =
        crate::git::commit::CommitMessageFile::create(if body.trim().is_empty() {
            "(no body provided)\n"
        } else {
            &body
        })
        .map_err(|e| e.to_string())?;

    let body_path = body_file.path().to_string_lossy().into_owned();
    let args = vec![
        "issue".to_string(),
        "create".to_string(),
        "--title".to_string(),
        title.clone(),
        "--body-file".to_string(),
        body_path,
    ];
    let output = tokio::process::Command::new("gh")
        .args(&args)
        .current_dir(&path)
        .output()
        .await
        .map_err(|e| format!("gh issue create: {e}"))?;
    drop(body_file);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let err = format!("gh issue create failed: {stderr}");
        tracing::info!(
            op = "issue_create",
            cwd = %cwd_log,
            title = %title,
            duration_ms = started.elapsed().as_millis() as u64,
            outcome = "err",
            "git_op:issue_create"
        );
        return Err(err);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let url = stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(ToOwned::to_owned)
        .ok_or_else(|| format!("could not extract issue URL from gh stdout: {stdout}"))?;

    tracing::info!(
        op = "issue_create",
        cwd = %cwd_log,
        title = %title,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = "created",
        "git_op:issue_create"
    );
    Ok(url)
}

#[tauri::command]
pub async fn git_commit_push_pr(
    state: State<'_, AppState>,
    cwd: String,
    title: String,
    body: String,
    branch_hint: Option<String>,
) -> Result<String, String> {
    let path = assert_cwd_in_editable_mounts(&state, &cwd).await?;
    let cwd_log = cwd.clone();
    let started = std::time::Instant::now();

    let path_for_default = path.clone();
    let default_base = run_blocking(move || {
        crate::git::branch::detect_default_branch(&path_for_default).map_err(|e| e.to_string())
    })
    .await?;

    let path_for_branch = path.clone();
    let default_base_for_branch = default_base.clone();
    let branch_hint_owned = branch_hint.clone();
    let title_for_branch = title.clone();
    let current_branch: String = run_blocking(move || {
        let current = crate::git::branch::current_branch(&path_for_branch).map_err(|e| e.to_string())?;
        if current != default_base_for_branch {
            return Ok::<String, String>(current);
        }
        let hint = branch_hint_owned.as_deref().unwrap_or(&title_for_branch);
        let new_name = crate::git::branch::build_branch_name(hint);
        crate::git::branch::create_and_checkout(&path_for_branch, &new_name).map_err(|e| e.to_string())?;
        Ok::<String, String>(new_name)
    })
    .await?;

    let path_for_commit = path.clone();
    let commit_message = title.clone();
    let commit_outcome = run_blocking(move || {
        match crate::git::commit::commit_all_with_message(&path_for_commit, &commit_message) {
            Ok(()) => Ok::<&'static str, String>("created"),
            Err(crate::git::GitError::NoWorkspaceChanges) => Ok("skipped"),
            Err(other) => Err(other.to_string()),
        }
    })
    .await?;

    crate::git::github::pr::push_branch_set_upstream_async(&path, &current_branch)
        .await
        .map_err(|e| e.to_string())?;

    let body_file =
        crate::git::commit::CommitMessageFile::create(if body.trim().is_empty() {
            "(no body provided)\n"
        } else {
            &body
        })
        .map_err(|e| e.to_string())?;

    let request = crate::git::github::pr::PrCreateRequest {
        title: &title,
        body_file: body_file.path(),
        base: &default_base,
    };
    let pr_outcome = crate::git::github::pr::create_async(&path, &request)
        .await
        .map_err(|e| e.to_string())?;

    drop(body_file);

    let human = format!(
        "{} → branch `{}` → PR {}{}",
        match commit_outcome {
            "created" => "Committed",
            _ => "Working tree clean (no new commit)",
        },
        current_branch,
        if pr_outcome.was_existing { "(existing) " } else { "" },
        pr_outcome.url
    );

    tracing::info!(
        op = "commit_push_pr",
        cwd = %cwd_log,
        title = %title,
        base = %default_base,
        branch = %current_branch,
        duration_ms = started.elapsed().as_millis() as u64,
        outcome = if pr_outcome.was_existing { "existing" } else { "created" },
        "git_op:commit_push_pr"
    );
    Ok(human)
}
