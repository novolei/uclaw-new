//! Commit primitives: detect dirty state, stage everything, write a
//! commit message tempfile, and run `git commit --file <tmp>`.
//!
//! Implements the §5 fix for "tempfile leak in the reference library":
//! [`CommitMessageFile`] is RAII — its `Drop` impl removes the file
//! even if `commit_with_message_file` panics or returns early.

use std::path::{Path, PathBuf};

use tempfile::Builder;

use super::command::{git_ok, git_stdout};
use super::error::{GitError, GitResult};

/// Returns `true` if `git status --short` reports any tracked-or-
/// untracked change in the working tree.
pub(crate) fn has_workspace_changes(cwd: &Path) -> GitResult<bool> {
    let stdout = git_stdout(cwd, &["status", "--short"])?;
    Ok(!stdout.trim().is_empty())
}

/// `git add -A`.
pub(crate) fn stage_all(cwd: &Path) -> GitResult<()> {
    git_ok(cwd, &["add", "-A"])
}

/// Run `git commit --file <path>`.
pub(crate) fn commit_with_message_file(cwd: &Path, message_file: &Path) -> GitResult<()> {
    let path_str = message_file.to_string_lossy().into_owned();
    git_ok(cwd, &["commit", "--file", path_str.as_str()])
}

/// RAII wrapper around a temporary commit-message file.
///
/// The reference library wrote the message via `std::env::temp_dir()`
/// and never deleted the file; this wrapper guarantees cleanup on
/// `Drop` (success **and** failure paths) by leaning on
/// [`tempfile::NamedTempFile`].
#[derive(Debug)]
pub(crate) struct CommitMessageFile {
    inner: tempfile::NamedTempFile,
}

impl CommitMessageFile {
    /// Allocate a new tempfile under the OS temp dir and write
    /// `message` (a leading/trailing newline is preserved as-is so the
    /// commit body matches what the caller supplied).
    pub(crate) fn create(message: &str) -> GitResult<Self> {
        let trimmed = message.trim();
        if trimmed.is_empty() {
            return Err(GitError::EmptyCommitMessage);
        }
        let mut file = Builder::new()
            .prefix("uclaw-commit-")
            .suffix(".txt")
            .tempfile()?;
        // Use std::io::Write directly; tempfile::NamedTempFile derefs
        // to a File that supports it.
        std::io::Write::write_all(&mut file, message.as_bytes())?;
        Ok(Self { inner: file })
    }

    /// Path to the underlying tempfile (lifetime of the message file).
    pub(crate) fn path(&self) -> &Path {
        self.inner.path()
    }

    /// Take ownership of the tempfile path **without** deleting it.
    ///
    /// Most callers should just rely on the RAII `Drop`; this is only
    /// useful for diagnostic flows that want to surface the file path
    /// to the user after the commit succeeded.
    #[allow(dead_code)]
    pub(crate) fn keep(self) -> GitResult<PathBuf> {
        let (file, path) = self.inner.keep().map_err(|err| {
            // tempfile::PersistError wraps both the underlying io::Error
            // and the original NamedTempFile; we surface the io part.
            GitError::Io(err.error)
        })?;
        drop(file);
        Ok(path)
    }
}

/// Convenience: stage everything and create a commit with `message`.
///
/// This is the smallest "do the right thing" entry-point used by the
/// `/commit` slash command.  Errors of interest:
///
/// - [`GitError::EmptyCommitMessage`] — `message` is whitespace-only.
/// - [`GitError::NoWorkspaceChanges`] — clean tree; the slash layer
///   can render this as an idempotent "skipped" outcome.
/// - [`GitError::NonZeroExit`] — `git add` / `git commit` itself failed.
pub(crate) fn commit_all_with_message(cwd: &Path, message: &str) -> GitResult<()> {
    if message.trim().is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }
    if !has_workspace_changes(cwd)? {
        return Err(GitError::NoWorkspaceChanges);
    }
    stage_all(cwd)?;
    let message_file = CommitMessageFile::create(message)?;
    commit_with_message_file(cwd, message_file.path())?;
    // `message_file` drops here, deleting the tempfile.
    Ok(())
}

/// Variant of [`commit_all_with_message`] that surrenders the
/// commit-message tempfile to the caller (preserved on disk) so it can
/// be referenced in diagnostic/UX output.  Use sparingly: ownership
/// transfers to the caller, including cleanup responsibility.
#[allow(dead_code)]
pub(crate) fn commit_all_with_message_keep_path(cwd: &Path, message: &str) -> GitResult<PathBuf> {
    if message.trim().is_empty() {
        return Err(GitError::EmptyCommitMessage);
    }
    if !has_workspace_changes(cwd)? {
        return Err(GitError::NoWorkspaceChanges);
    }
    stage_all(cwd)?;
    let message_file = CommitMessageFile::create(message)?;
    commit_with_message_file(cwd, message_file.path())?;
    message_file.keep()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_support::TestRepo;

    #[test]
    fn has_workspace_changes_reports_dirty_after_write() {
        let repo = TestRepo::new("commit-dirty");
        assert!(!has_workspace_changes(repo.path()).expect("clean check"));
        std::fs::write(repo.path().join("new.txt"), "x").expect("write");
        assert!(has_workspace_changes(repo.path()).expect("dirty check"));
    }

    #[test]
    fn commit_message_file_is_deleted_on_drop() {
        let path_held;
        {
            let f = CommitMessageFile::create("hello world").expect("create file");
            path_held = f.path().to_path_buf();
            assert!(path_held.exists(), "file must exist while alive");
        }
        assert!(
            !path_held.exists(),
            "tempfile must be removed on drop, still at: {}",
            path_held.display()
        );
    }

    #[test]
    fn commit_message_file_rejects_empty_input() {
        let err = CommitMessageFile::create("   \n\t  ").expect_err("must reject");
        assert!(matches!(err, GitError::EmptyCommitMessage));
    }

    #[test]
    fn commit_all_with_message_creates_commit_for_dirty_repo() {
        let repo = TestRepo::new("commit-go");
        std::fs::write(repo.path().join("change.txt"), "data\n").expect("write change");

        commit_all_with_message(repo.path(), "feat: add change\n").expect("commit ok");

        // Verify the commit actually landed by inspecting the latest
        // log subject.
        let subject = std::process::Command::new("git")
            .args(["log", "-1", "--pretty=%s"])
            .current_dir(repo.path())
            .output()
            .expect("git log");
        assert!(subject.status.success());
        let subject_str = String::from_utf8(subject.stdout).expect("utf8");
        assert!(
            subject_str.trim() == "feat: add change",
            "expected our commit subject, got: {subject_str:?}"
        );
    }

    #[test]
    fn commit_all_with_message_errors_when_clean() {
        let repo = TestRepo::new("commit-clean");
        let err = commit_all_with_message(repo.path(), "noop").expect_err("must fail clean");
        assert!(
            matches!(err, GitError::NoWorkspaceChanges),
            "expected NoWorkspaceChanges, got {err:?}"
        );
    }

    #[test]
    fn has_workspace_changes_reports_dirty_for_staged_only() {
        let repo = TestRepo::new("commit-staged");
        std::fs::write(repo.path().join("staged.txt"), "x\n").expect("write");
        // Stage but do not commit.
        let status = std::process::Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo.path())
            .status()
            .expect("git add staged");
        assert!(status.success(), "git add must succeed");
        assert!(
            has_workspace_changes(repo.path()).expect("dirty check"),
            "staged-only changes must register as dirty"
        );
    }
}
