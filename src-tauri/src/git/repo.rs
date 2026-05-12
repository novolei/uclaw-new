//! Repository discovery: locate the top-level of the git working tree.
//!
//! Replaces the legacy `find_git_root` from
//! `/rust/crates/claw-cli/src/main.rs` and the brittle
//! `parse_git_status_metadata` (which extracted the branch by string-
//! parsing `## branch...nothing to commit`).  We never parse `git
//! status` for branch names anymore — branch lookup goes through
//! [`super::branch::current_branch`] which runs `git branch
//! --show-current` directly.

use std::path::{Path, PathBuf};

use super::command::{git_ok, git_stdout};
use super::error::{GitError, GitResult};

/// Resolve the absolute path to the top of the git working tree that
/// contains `cwd`.
///
/// Returns [`GitError::NotARepository`] if `git rev-parse` succeeds but
/// produces an empty path (defensive — should not happen in practice).
pub(crate) fn find_git_root(cwd: &Path) -> GitResult<PathBuf> {
    let raw = git_stdout(cwd, &["rev-parse", "--show-toplevel"])?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Err(GitError::NotARepository)
    } else {
        Ok(PathBuf::from(trimmed))
    }
}

/// Returns `true` if `cwd` (or any ancestor) is inside a git working
/// tree.  Does **not** raise on missing `git`; callers that need to
/// distinguish "no git binary" from "not a repo" should use
/// [`find_git_root`] directly.
#[must_use]
pub(crate) fn is_inside_repo(cwd: &Path) -> bool {
    find_git_root(cwd).is_ok()
}

/// Run `git init` in `cwd`.
///
/// Idempotent: re-running on an existing repo is a no-op (git itself
/// just prints "Reinitialized existing Git repository").  Caller is
/// expected to refresh `is_inside_repo` afterwards so the UI can
/// re-enable git affordances.
pub(crate) fn init_repo(cwd: &Path) -> GitResult<()> {
    git_ok(cwd, &["init"])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_support::TestRepo;

    #[test]
    fn find_git_root_returns_repo_root() {
        let repo = TestRepo::new("repo-root");
        let root = find_git_root(repo.path()).expect("find root");
        assert_eq!(
            root.canonicalize().expect("canon root"),
            repo.path().canonicalize().expect("canon repo")
        );
    }

    #[test]
    fn find_git_root_works_from_subdir() {
        let repo = TestRepo::new("repo-subdir");
        let sub = repo.path().join("nested/dir");
        std::fs::create_dir_all(&sub).expect("mkdir nested");
        let root = find_git_root(&sub).expect("find root from subdir");
        assert_eq!(
            root.canonicalize().expect("canon root"),
            repo.path().canonicalize().expect("canon repo")
        );
    }

    #[test]
    fn is_inside_repo_false_outside() {
        let dir = tempfile::tempdir().expect("temp dir");
        assert!(!is_inside_repo(dir.path()));
    }

    #[test]
    fn is_inside_repo_true_inside() {
        let repo = TestRepo::new("inside-true");
        assert!(is_inside_repo(repo.path()));
        let nested = repo.path().join("a/b/c");
        std::fs::create_dir_all(&nested).expect("mkdir nested");
        assert!(is_inside_repo(&nested));
    }
}
