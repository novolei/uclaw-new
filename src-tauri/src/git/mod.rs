//! Git + GitHub CLI integration layer.
//!
//! Ported from if2Ai's `src-tauri/src/modules/git/` (W6 — uClaw spec at
//! `docs/superpowers/specs/2026-05-13-w6-workspace-git-design.md`).
//!
//! The module is intentionally split by responsibility — every file owns
//! a single bounded concern:
//!
//! - [`error`]    — [`GitError`] / [`GitResult`] (single error model)
//! - `command`    — subprocess execution + PATH probing (Task 2)
//! - `status`     — `git status` / `git diff` snapshots (Task 5)
//! - `branch`     — current / exists / default-branch detection + slug (Task 4)
//! - `repo`       — repository discovery (`rev-parse --show-toplevel`) (Task 3)
//! - `commit`     — staging + RAII commit-message tempfile (Task 6)
//! - `github`     — `gh pr` wrappers + URL parsing (Task 7)
//!
//! All operations are synchronous (CLI spawning); the Tauri command layer
//! (`src-tauri/src/tauri_commands_git.rs`) is responsible for wrapping
//! them in `tokio::task::spawn_blocking`. Network ops (`gh pr create`,
//! `git push`) use the `*_async` siblings in [`github::pr`] so they don't
//! park a blocking-pool worker for the GitHub round-trip.
//!
//! Differences from if2Ai's port source:
//! - `worktree.rs` is dropped (out of W6 Phase 1+2 scope).
//! - `slash.rs` is dropped (uClaw uses skills/tools, not slash commands).
//! - `IssueRequest` / `issue.rs` is dropped (no UI affordance in PR B).

pub(crate) mod error;
// Subsequent tasks add their own `pub(crate) mod <name>;` lines below
// so each commit's build stays green and bisectable.

#[cfg(test)]
pub(crate) mod test_support;

// Re-export the error model at the module root so siblings (the IPC
// layer, future agent tools) can refer to it without depending on the
// internal sub-module path.
#[allow(unused_imports)]
pub(crate) use error::{GitError, GitResult};
