//! `git status` and `git diff` snapshot helpers.
//!
//! Used by the prompt layer to inject working-tree state and by the
//! `/diff` slash command.  All readers go through `git_stdout_no_locks`
//! so they cannot interfere with concurrent IDE git activity.

use std::path::Path;

use super::command::{git_stdout, git_stdout_no_locks};
use super::error::GitResult;

/// Read `git --no-optional-locks status --short --branch`.
///
/// Returns:
/// - `Ok(Some(text))` when the working tree has any reportable state.
/// - `Ok(None)` when the working tree is clean (status output empty).
///
/// Errors propagate from [`git_stdout_no_locks`]; callers that want
/// "best-effort, swallow on missing repo" behaviour can map them via
/// [`status_or_none`].
pub(crate) fn read_status(cwd: &Path) -> GitResult<Option<String>> {
    let stdout = git_stdout_no_locks(cwd, &["status", "--short", "--branch"])?;
    let trimmed = stdout.trim();
    Ok(if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    })
}

/// Best-effort wrapper used by prompt-context injection.
///
/// Mirrors the legacy `read_git_status` semantics from
/// `runtime/prompt/instruction_files.rs` and from the `/rust` reference
/// library: any failure (missing `git`, not a repo, transient lock)
/// collapses to `None` so prompt rendering never fails because of git.
#[must_use]
pub(crate) fn status_or_none(cwd: &Path) -> Option<String> {
    read_status(cwd).ok().flatten()
}

/// Verbosity level for [`read_diff_with_mode`].
///
/// - `Stat`：`git diff --stat` 风格的"几个文件 +N -N"汇总。MB 量级的
///   重构 / 大跳号补丁也只占几行；适合 chat 默认 + prompt 上下文注入，
///   不会撑爆 token 预算。
/// - `Full`：完整的 unified patch；用户在 Workbench 里点"完整 patch"
///   或显式 `/diff full` 才走这条路径。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiffMode {
    Stat,
    Full,
}

/// Read both staged and unstaged diffs and stitch them into a single
/// human-readable block; matches the legacy prompt snapshot format.
///
/// Defaults to [`DiffMode::Stat`] — see [`read_diff_with_mode`] when
/// callers need the full patch (Workbench "完整 patch" toggle, slash
/// `/diff full`).  Existing call sites picked up `--stat` automatically
/// after the migration; this is intentional because the previous
/// "always full patch" default could blow past the prompt context
/// budget on large refactors and was the root cause of /diff 输出过长
/// follow-up §2.
pub(crate) fn read_diff(cwd: &Path) -> GitResult<Option<String>> {
    read_diff_with_mode(cwd, DiffMode::Stat)
}

/// Verbosity-aware variant of [`read_diff`].
pub(crate) fn read_diff_with_mode(cwd: &Path, mode: DiffMode) -> GitResult<Option<String>> {
    let mut sections: Vec<String> = Vec::new();

    let mut base_args: Vec<&str> = vec!["diff", "--cached"];
    if mode == DiffMode::Stat {
        base_args.push("--stat");
    }
    let staged = git_stdout_no_locks(cwd, &base_args)?;
    if !staged.trim().is_empty() {
        sections.push(format!("Staged changes:\n{}", staged.trim_end()));
    }

    let mut unstaged_args: Vec<&str> = vec!["diff"];
    if mode == DiffMode::Stat {
        unstaged_args.push("--stat");
    }
    let unstaged = git_stdout_no_locks(cwd, &unstaged_args)?;
    if !unstaged.trim().is_empty() {
        sections.push(format!("Unstaged changes:\n{}", unstaged.trim_end()));
    }

    Ok(if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    })
}

/// Best-effort counterpart to [`read_diff`] for prompt context injection.
///
/// Hard-coded to [`DiffMode::Stat`] — see the doc on [`read_diff`] for
/// why: prompt context can never afford the full unified patch.
#[must_use]
pub(crate) fn diff_or_none(cwd: &Path) -> Option<String> {
    read_diff(cwd).ok().flatten()
}

/// Run an arbitrary `git args...` and return the trimmed stdout.
///
/// Thin wrapper kept for the slash layer (e.g. `git diff --stat`) so it
/// does not need to import `command::git_stdout` directly.
pub(crate) fn read_raw(cwd: &Path, args: &[&str]) -> GitResult<String> {
    git_stdout(cwd, args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::test_support::TestRepo;

    #[test]
    fn status_clean_repo_returns_branch_header_only() {
        let repo = TestRepo::new("status-clean");
        let s = read_status(repo.path())
            .expect("status ok")
            .expect("branch header must be present on a clean repo");
        // `--short --branch` always emits `## <branch>` even on a clean
        // tree, so we get exactly the header line and nothing else.
        assert!(
            s.starts_with("## "),
            "expected leading branch advert, got: {s}"
        );
        assert!(
            !s.lines().any(|line| !line.starts_with("##")),
            "clean tree should have no file lines, got: {s}"
        );
    }

    #[test]
    fn status_after_change_includes_file() {
        let repo = TestRepo::new("status-dirty");
        std::fs::write(repo.path().join("new.txt"), "hi").expect("write");
        let s = read_status(repo.path())
            .expect("status ok")
            .expect("non-empty");
        assert!(s.contains("new.txt"), "status should mention new.txt: {s}");
    }

    #[test]
    fn status_or_none_swallows_errors_outside_repo() {
        let dir = tempfile::tempdir().expect("temp dir");
        // Not a git repo; helper must return None, never panic / error.
        assert!(status_or_none(dir.path()).is_none());
    }

    #[test]
    fn diff_returns_some_when_unstaged_changes_exist_in_stat_mode() {
        let repo = TestRepo::new("diff-unstaged");
        std::fs::write(repo.path().join("README.md"), "changed\n").expect("modify");
        let d = read_diff(repo.path()).expect("diff ok").expect("non-empty");
        assert!(d.contains("Unstaged changes:"));
        assert!(d.contains("README.md"));
        // Default mode is now `--stat`, so the body has the file
        // summary line but not the full unified patch.
        assert!(
            !d.contains("diff --git"),
            "default read_diff should be --stat mode, not full patch; got: {d}"
        );
    }

    #[test]
    fn diff_with_mode_full_emits_unified_patch() {
        let repo = TestRepo::new("diff-full-mode");
        std::fs::write(repo.path().join("README.md"), "changed\n").expect("modify");
        let d = read_diff_with_mode(repo.path(), DiffMode::Full)
            .expect("diff ok")
            .expect("non-empty");
        assert!(d.contains("Unstaged changes:"));
        assert!(
            d.contains("diff --git"),
            "Full mode must include the unified patch, got: {d}"
        );
    }

    #[test]
    fn diff_returns_some_when_only_staged_changes_exist() {
        use std::process::Command;

        let repo = TestRepo::new("diff-staged");
        std::fs::write(repo.path().join("staged.txt"), "hello\n").expect("write staged");
        let status = Command::new("git")
            .args(["add", "staged.txt"])
            .current_dir(repo.path())
            .status()
            .expect("git add staged");
        assert!(status.success(), "git add must succeed");

        let d = read_diff(repo.path())
            .expect("diff ok")
            .expect("non-empty staged diff");
        assert!(
            d.contains("Staged changes:"),
            "expected staged section, got: {d}"
        );
        assert!(
            !d.contains("Unstaged changes:"),
            "no unstaged section expected when only staged changes exist, got: {d}"
        );
    }
}
