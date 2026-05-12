//! Branch helpers: current / exists / default detection, plus the
//! slug + branch-name builder used by `/commit-push-pr`.
//!
//! Implements the §5 fix list:
//! - `detect_default_branch` adds a `git config --get init.defaultBranch`
//!   fallback before giving up and returning the current branch.
//! - `slugify` collapses repeated separators and trims edges, like the
//!   reference library, but is unit-tested here.

use std::env;
use std::path::Path;

use super::command::{git_ok, git_stdout};
use super::error::{GitError, GitResult};

/// Return the name of the currently checked-out branch.
///
/// - Returns [`GitError::NoBranch`] when the repository is valid but
///   no branch is checked out (e.g. detached HEAD: `git branch
///   --show-current` exits 0 with empty stdout).
/// - Returns [`GitError::NonZeroExit`] when `git` itself rejects the
///   call (not a repository at all — bubble up to caller's preferred
///   "is this a repo" check).
pub(crate) fn current_branch(cwd: &Path) -> GitResult<String> {
    let raw = git_stdout(cwd, &["branch", "--show-current"])?;
    let branch = raw.trim();
    if branch.is_empty() {
        Err(GitError::NoBranch)
    } else {
        Ok(branch.to_string())
    }
}

/// Returns `true` if a local branch with this name exists.
///
/// Implemented via `git show-ref --verify --quiet refs/heads/<name>`;
/// any non-zero exit (including "branch absent") collapses to `false`.
#[must_use]
pub(crate) fn branch_exists(cwd: &Path, name: &str) -> bool {
    git_ok(
        cwd,
        &[
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{name}"),
        ],
    )
    .is_ok()
}

/// List local branches one-per-line with the verbose format
/// (`* main 1abc2d3 subject`).  Used by `/branch list`.
pub(crate) fn list_branches_verbose(cwd: &Path) -> GitResult<String> {
    git_stdout(cwd, &["branch", "--list", "--verbose"])
}

/// Detect the repository's default branch.
///
/// Resolution order (longest possible chain first):
/// 1. `git symbolic-ref refs/remotes/origin/HEAD` (works once `origin`
///    is configured).
/// 2. `main`, then `master` if either exists locally.
/// 3. `git config --get init.defaultBranch` (gives a sensible answer
///    on freshly-init'd repos with no `origin` and no `main` yet).
/// 4. Falls back to [`current_branch`].
pub(crate) fn detect_default_branch(cwd: &Path) -> GitResult<String> {
    if let Ok(reference) = git_stdout(cwd, &["symbolic-ref", "refs/remotes/origin/HEAD"]) {
        if let Some(branch) = reference
            .trim()
            .rsplit('/')
            .next()
            .filter(|value| !value.is_empty())
        {
            return Ok(branch.to_string());
        }
    }

    for candidate in ["main", "master"] {
        if branch_exists(cwd, candidate) {
            return Ok(candidate.to_string());
        }
    }

    if let Ok(config_value) = git_stdout(cwd, &["config", "--get", "init.defaultBranch"]) {
        let trimmed = config_value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.to_string());
        }
    }

    current_branch(cwd)
}

/// Switch the working tree to an existing branch (`git checkout <name>`).
///
/// Fails with [`GitError::MissingRequired`] when `name` is empty.  Other
/// failures (dirty tree blocking checkout, branch doesn't exist, …) are
/// surfaced as [`GitError::NonZeroExit`] with the underlying `git`
/// stderr — caller is expected to render that verbatim to the user.
pub(crate) fn checkout(cwd: &Path, name: &str) -> GitResult<()> {
    if name.trim().is_empty() {
        return Err(GitError::MissingRequired("branch name"));
    }
    git_ok(cwd, &["checkout", name])
}

/// Create a new branch at HEAD and check it out (`git checkout -b <name>`).
///
/// Fails with [`GitError::MissingRequired`] when `name` is empty, with
/// [`GitError::NonZeroExit`] when `git` rejects the call (most commonly
/// "branch already exists" — caller can detect via the rendered error
/// or by pre-flighting [`branch_exists`]).
pub(crate) fn create_and_checkout(cwd: &Path, name: &str) -> GitResult<()> {
    if name.trim().is_empty() {
        return Err(GitError::MissingRequired("branch name"));
    }
    git_ok(cwd, &["checkout", "-b", name])
}

/// Build a branch name of the form `<owner>/<slug>` where `<owner>`
/// comes from `$SAFEUSER`, then `$USER`; falls back to the bare slug.
///
/// Wraps [`build_branch_name_with_owner`] with the env-derived owner so
/// production code stays a single call while tests can pass the owner
/// in directly and avoid touching process-wide environment variables.
#[must_use]
pub(crate) fn build_branch_name(hint: &str) -> String {
    let owner = env::var("SAFEUSER")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .or_else(|| env::var("USER").ok().filter(|v| !v.trim().is_empty()));
    build_branch_name_with_owner(hint, owner.as_deref())
}

/// Pure variant of [`build_branch_name`] — formats `<owner>/<slug>` if
/// `owner` is `Some(non-empty)`, else returns the bare slug.  Exposed
/// at module-private scope so tests do not need to mutate environment
/// variables (see review item S2).
#[must_use]
pub(crate) fn build_branch_name_with_owner(hint: &str, owner: Option<&str>) -> String {
    let slug = slugify(hint);
    match owner.map(str::trim).filter(|v| !v.is_empty()) {
        Some(owner) => format!("{owner}/{slug}"),
        None => slug,
    }
}

/// Lower-case slug; non-alphanumeric collapses to a single dash.
///
/// Returns `"change"` if the input contains no alphanumerics (so callers
/// always get a valid branch component).
#[must_use]
pub(crate) fn slugify(value: &str) -> String {
    let mut slug = String::with_capacity(value.len());
    let mut last_was_dash = false;
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            last_was_dash = false;
        } else if !last_was_dash {
            slug.push('-');
            last_was_dash = true;
        }
    }
    let trimmed = slug.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "change".to_string()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_collapses_separators() {
        assert_eq!(slugify("Add Foo Bar!!"), "add-foo-bar");
        assert_eq!(slugify("---weird___ID---"), "weird-id");
    }

    #[test]
    fn slugify_returns_change_for_empty_or_punctuation() {
        assert_eq!(slugify(""), "change");
        assert_eq!(slugify("!!!"), "change");
    }

    #[test]
    fn current_branch_reports_main_in_seed_repo() {
        let repo = crate::git::test_support::TestRepo::new("current");
        let b = current_branch(repo.path()).expect("current branch");
        assert_eq!(b, "main");
    }

    #[test]
    fn branch_exists_true_for_main_false_for_unknown() {
        let repo = crate::git::test_support::TestRepo::new("exists");
        assert!(branch_exists(repo.path(), "main"));
        assert!(!branch_exists(repo.path(), "definitely-not-a-branch-xyz"));
    }

    #[test]
    fn detect_default_branch_returns_main_when_origin_absent() {
        let repo = crate::git::test_support::TestRepo::new("default");
        let d = detect_default_branch(repo.path()).expect("default");
        assert_eq!(d, "main");
    }

    #[test]
    fn build_branch_name_with_owner_prefixes_when_present() {
        assert_eq!(
            build_branch_name_with_owner("Add Foo Bar", Some("alice")),
            "alice/add-foo-bar"
        );
    }

    #[test]
    fn build_branch_name_with_owner_falls_back_when_blank_or_none() {
        assert_eq!(
            build_branch_name_with_owner("Add Foo Bar", None),
            "add-foo-bar"
        );
        assert_eq!(
            build_branch_name_with_owner("Add Foo Bar", Some("   ")),
            "add-foo-bar"
        );
    }

    #[test]
    fn list_branches_verbose_includes_main() {
        let repo = crate::git::test_support::TestRepo::new("list-branches");
        let listing = list_branches_verbose(repo.path()).expect("list branches");
        assert!(
            listing.contains("main"),
            "verbose branch listing should include `main`, got: {listing}"
        );
    }
}
