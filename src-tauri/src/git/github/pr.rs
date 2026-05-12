//! `gh pr` wrappers + URL parsing.
//!
//! Two extraction helpers cover the two output shapes seen in the
//! reference library:
//! - [`parse_pr_url`] for `gh pr create` (plain stdout containing the
//!   PR URL on its own line).
//! - [`parse_pr_view_url`] for `gh pr view --json url` (JSON object).

use std::path::Path;
use std::process::Command;

use super::super::command::{run_stdout_async, GH_BIN, GIT_BIN};
use super::super::error::{GitError, GitResult};
use super::is_gh_available;

/// Inputs for `gh pr create`.
#[derive(Debug, Clone)]
pub(crate) struct PrCreateRequest<'a> {
    pub(crate) title: &'a str,
    pub(crate) body_file: &'a Path,
    pub(crate) base: &'a str,
}

/// Successful create output: the PR URL.
#[derive(Debug, Clone)]
pub(crate) struct PrCreateOutcome {
    pub(crate) url: String,
    /// `true` if the URL came from `gh pr view` (i.e. the create call
    /// failed because a PR already exists for the branch).
    pub(crate) was_existing: bool,
}

/// Run `gh pr create` (and, on failure, `gh pr view --json url` to
/// fetch the URL of an already-open PR for the same branch).
pub(crate) fn create(cwd: &Path, request: &PrCreateRequest<'_>) -> GitResult<PrCreateOutcome> {
    if !is_gh_available() {
        return Err(GitError::MissingBinary(GH_BIN));
    }

    let body_path = request.body_file.to_string_lossy().into_owned();
    let create_args: [&str; 8] = [
        "pr",
        "create",
        "--title",
        request.title,
        "--body-file",
        body_path.as_str(),
        "--base",
        request.base,
    ];

    let output = Command::new(GH_BIN)
        .args(create_args)
        .current_dir(cwd)
        .output()?;
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        return parse_pr_url(stdout.as_ref())
            .map(|url| PrCreateOutcome {
                url,
                was_existing: false,
            })
            .ok_or_else(|| {
                GitError::Parse(format!(
                    "could not extract PR URL from `gh pr create` stdout: {stdout}"
                ))
            });
    }

    // Create failed; assume "PR already exists" and try to look it up.
    let view = Command::new(GH_BIN)
        .args(["pr", "view", "--json", "url"])
        .current_dir(cwd)
        .output()?;
    if !view.status.success() {
        // Surface the original failure (more informative) rather than
        // the lookup failure.
        return Err(GitError::from_output(GH_BIN, &create_args, &output));
    }
    let view_stdout = String::from_utf8_lossy(&view.stdout);
    parse_pr_view_url(view_stdout.as_ref())
        .map(|url| PrCreateOutcome {
            url,
            was_existing: true,
        })
        .ok_or_else(|| {
            GitError::Parse(format!(
                "could not extract PR URL from `gh pr view --json url` output: {view_stdout}"
            ))
        })
}

/// Parse `gh pr create` stdout for the first line that looks like an
/// HTTP(S) URL.  Returns the trimmed URL or `None` if absent.
#[must_use]
pub(crate) fn parse_pr_url(stdout: &str) -> Option<String> {
    stdout
        .lines()
        .map(str::trim)
        .find(|line| line.starts_with("http://") || line.starts_with("https://"))
        .map(ToOwned::to_owned)
}

/// Parse `gh pr view --json url` JSON for `.url`.
#[must_use]
pub(crate) fn parse_pr_view_url(stdout: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()?
        .get("url")?
        .as_str()
        .map(ToOwned::to_owned)
}

/// Async sibling of [`create`] that uses `tokio::process::Command`
/// directly instead of going through `spawn_blocking`.
///
/// This is the **right call site for the IPC layer** because
/// `gh pr create` typically waits several seconds on the GitHub API —
/// running it on a tokio task lets the runtime keep handling other
/// requests instead of parking a blocking-pool worker for the whole
/// network round-trip.  Local-only git ops (status/diff/branch) keep
/// using the sync helpers since they're sub-millisecond and not worth
/// the scheduler hop.
pub(crate) async fn create_async(
    cwd: &Path,
    request: &PrCreateRequest<'_>,
) -> GitResult<PrCreateOutcome> {
    if !is_gh_available() {
        return Err(GitError::MissingBinary(GH_BIN));
    }

    let body_path = request.body_file.to_string_lossy().into_owned();
    let create_args: [&str; 8] = [
        "pr",
        "create",
        "--title",
        request.title,
        "--body-file",
        body_path.as_str(),
        "--base",
        request.base,
    ];

    match run_stdout_async(GH_BIN, &create_args, cwd).await {
        Ok(stdout) => parse_pr_url(&stdout)
            .map(|url| PrCreateOutcome {
                url,
                was_existing: false,
            })
            .ok_or_else(|| {
                GitError::Parse(format!(
                    "could not extract PR URL from `gh pr create` stdout: {stdout}"
                ))
            }),
        Err(create_err) => {
            // Create failed; assume "PR already exists" and look it up.
            // Surface the original create-error if the lookup also fails
            // (more informative than the lookup error alone).
            match run_stdout_async(GH_BIN, &["pr", "view", "--json", "url"], cwd).await {
                Ok(view_stdout) => parse_pr_view_url(&view_stdout)
                    .map(|url| PrCreateOutcome {
                        url,
                        was_existing: true,
                    })
                    .ok_or_else(|| {
                        GitError::Parse(format!(
                            "could not extract PR URL from `gh pr view --json url` output: {view_stdout}"
                        ))
                    }),
                Err(_) => Err(create_err),
            }
        }
    }
}

/// Push the current branch to `origin`, setting upstream if absent.
///
/// Lives next to `gh pr create` because `/commit-push-pr` always pairs
/// the two; isolating this avoids dragging git's push semantics into
/// the higher-level slash layer.
///
/// **Limitation**: the remote name is hard-coded to `"origin"`, mirroring
/// the reference implementation (`/rust/crates/commands/src/lib.rs:999`).
/// If multi-remote support is ever needed, take the remote name as a
/// parameter and thread it through the slash-command surface.
pub(crate) fn push_branch_set_upstream(cwd: &Path, branch: &str) -> GitResult<()> {
    let status = Command::new(GIT_BIN)
        .args(["push", "--set-upstream", "origin", branch])
        .current_dir(cwd)
        .output()?;
    if !status.status.success() {
        return Err(GitError::from_output(
            GIT_BIN,
            &["push", "--set-upstream", "origin", branch],
            &status,
        ));
    }
    Ok(())
}

/// Async sibling of [`push_branch_set_upstream`] backed by tokio.
///
/// Same rationale as [`create_async`] — `git push` is a network round-
/// trip; running it through tokio frees the blocking-task pool while we
/// wait on the remote.
pub(crate) async fn push_branch_set_upstream_async(cwd: &Path, branch: &str) -> GitResult<()> {
    let args = ["push", "--set-upstream", "origin", branch];
    let output = tokio::process::Command::new(GIT_BIN)
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        return Err(GitError::from_output(GIT_BIN, &args, &output));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_url_picks_first_http_line() {
        let stdout =
            "Creating pull request for feat/x → main\nhttps://github.com/o/r/pull/42\nMore info";
        assert_eq!(
            parse_pr_url(stdout).as_deref(),
            Some("https://github.com/o/r/pull/42")
        );
    }

    #[test]
    fn parse_pr_url_returns_none_when_absent() {
        assert_eq!(parse_pr_url("nothing useful here"), None);
        assert_eq!(parse_pr_url(""), None);
    }

    #[test]
    fn parse_pr_view_url_extracts_url_field() {
        let stdout = r#"{"url":"https://github.com/o/r/pull/7"}"#;
        assert_eq!(
            parse_pr_view_url(stdout).as_deref(),
            Some("https://github.com/o/r/pull/7")
        );
    }

    #[test]
    fn parse_pr_view_url_returns_none_for_invalid_json() {
        assert_eq!(parse_pr_view_url("not json"), None);
        // Missing url field.
        assert_eq!(parse_pr_view_url(r#"{"other":"value"}"#), None);
    }
}
