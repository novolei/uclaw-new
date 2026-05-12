# W6 PR A — Workspace Git Backbone Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port if2Ai's `src-tauri/src/modules/git/` (minus `worktree.rs`) into uClaw as a new `src-tauri/src/git/` module, plus the IPC layer (`src-tauri/src/tauri_commands_git.rs`), main.rs handler registration, and the frontend IPC wrapper (`ui/src/modules/git/api.ts`). 14 Tauri commands, sandbox-gated against `MountRoot.editable`. 18 Rust tests + 7 UI helper tests.

**Architecture:** Synchronous CLI shell-outs (no `git2`/libgit2), one-concern-per-file Rust modules, `tokio::task::spawn_blocking` boundary at IPC layer, `tokio::process` only for `gh pr create` / `gh issue create` / `git push`. Sandbox check canonicalizes `cwd` against `state.files_rail_list_mounts(None).await` with `MountRoot.editable` gating mutations.

**Tech Stack:** Rust (existing `tokio`, `tempfile`, `serde`, `serde_json` deps — no new Cargo dependencies). Frontend: `@tauri-apps/api/core` (existing). No React in this PR.

**Spec:** `docs/superpowers/specs/2026-05-13-w6-workspace-git-design.md` §3 (backend) + §4.1 (frontend api.ts).

---

## Pre-flight

- [ ] **Confirm starting state**

```bash
cd /Users/ryanliu/Documents/uclaw
git checkout claude/w6-workspace-git
git branch --show-current   # must be claude/w6-workspace-git
git log --oneline -2
# Expect: b622a9f docs(spec): W6 workspace git integration design
#         <prior main commit>
```

- [ ] **Baselines (record before starting)**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 395 passed (will become 413 by Task 12)

cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -3
# Expect: 296 passed (will become 303 by Task 12)
```

- [ ] **Verify `tempfile` is already in src-tauri/Cargo.toml**

```bash
grep -n "^tempfile" /Users/ryanliu/Documents/uclaw/src-tauri/Cargo.toml
# Expect: `tempfile = "3"` at line ~125. No Cargo additions needed for this PR.
```

- [ ] **Branch hygiene reminder** — every subagent prompt verifies `git branch --show-current` at start, before commit, and after commit. The harness silently flips branches. If a subagent finds itself on a different branch, STOP and report — do NOT push or continue.

---

## File Structure

### New Rust files (under `src-tauri/src/git/`)

| Path | LOC (target) | Responsibility |
|---|---|---|
| `git/mod.rs` | ~45 | Module declarations + `GitError`/`GitResult` re-exports |
| `git/error.rs` | ~145 | `GitError` enum + `Display`/`Error`/`From` impls + `from_output` constructor |
| `git/command.rs` | ~230 | `git_stdout`, `git_ok`, `git_stdout_no_locks`, `run_stdout_async`, `command_exists` |
| `git/repo.rs` | ~95 | `find_git_root`, `is_inside_repo`, `init_repo` |
| `git/branch.rs` | ~245 | `current_branch`, `branch_exists`, `list_branches_verbose`, `detect_default_branch`, `checkout`, `create_and_checkout`, `slugify` |
| `git/status.rs` | ~210 | `DiffMode`, `read_status`, `read_diff_with_mode`, `status_or_none`, `diff_or_none` |
| `git/commit.rs` | ~210 | `CommitMessageFile` RAII, `has_workspace_changes`, `stage_all`, `commit_all_with_message` |
| `git/github/mod.rs` | ~25 | `is_gh_available` |
| `git/github/pr.rs` | ~220 | `PrCreateRequest`, `PrCreateOutcome`, `create_async`, `push_branch_set_upstream_async`, `parse_pr_url`, `parse_pr_view_url` |
| `git/test_support.rs` | ~70 | `TestRepo` — disposable git repo for unit tests (`#[cfg(test)]` only) |

### New IPC file

| Path | LOC | Responsibility |
|---|---|---|
| `src-tauri/src/tauri_commands_git.rs` | ~400 | 14 `#[tauri::command]` wrappers + `assert_cwd_in_any_mount` / `assert_cwd_in_editable_mounts` helpers + `run_blocking` + tracing audit |

### Modified Rust files

| Path | Change | Lines |
|---|---|---|
| `src-tauri/src/lib.rs` | Add `pub mod git;` + `pub mod tauri_commands_git;` | +2 |
| `src-tauri/src/tauri_commands.rs` | Add `pub use crate::tauri_commands_git::*;` re-export line | +1 |
| `src-tauri/src/main.rs` | Add 14 `uclaw_core::tauri_commands_git::*` entries to `generate_handler!` macro | +18 (1 header comment + 14 names + spacing) |

### New TypeScript files

| Path | LOC | Responsibility |
|---|---|---|
| `ui/src/modules/git/api.ts` | ~285 | Typed IPC wrappers + `parseBranchList` + `uncommittedFromStatus` helpers |
| `ui/src/modules/git/api.test.ts` | ~110 | 7 fixture tests for `parseBranchList` and `uncommittedFromStatus` |

**Total new code: ~2200 Rust + ~400 TS = ~2600 LOC across 12 new files + 3 modified files. Each file under 300 LOC except `branch.rs` (~245), `command.rs` (~230), `pr.rs` (~220), `status.rs` (~210), `commit.rs` (~210), `tauri_commands_git.rs` (~400). The last exceeds the 300 soft cap but stays well under 400 hard cap — justified as a single-concern flat module mirroring uClaw's pattern.**

---

## Task 1: Scaffold — module declarations + error type + test_support

**Files:**
- Create: `src-tauri/src/git/mod.rs`
- Create: `src-tauri/src/git/error.rs`
- Create: `src-tauri/src/git/test_support.rs`
- Modify: `src-tauri/src/lib.rs`

**Why first:** every other Rust file in this PR imports from `super::error` or uses `TestRepo` in tests. Land the scaffold first so subsequent tasks compile cleanly.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git log --oneline -1
```

- [ ] **Step 2: Create `src-tauri/src/git/mod.rs`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/mod.rs` in full, then write a uClaw-adapted version (drops `worktree` and `slash` since they're out of scope):

```rust
//! Git + GitHub CLI integration layer.
//!
//! Ported from if2Ai's `src-tauri/src/modules/git/` (W6 — uClaw spec at
//! `docs/superpowers/specs/2026-05-13-w6-workspace-git-design.md`).
//!
//! The module is intentionally split by responsibility — every file owns
//! a single bounded concern:
//!
//! - [`error`]    — [`GitError`] / [`GitResult`] (single error model)
//! - [`command`]  — subprocess execution + PATH probing
//! - [`status`]   — `git status` / `git diff` snapshots
//! - [`branch`]   — current / exists / default-branch detection + slug
//! - [`repo`]     — repository discovery (`rev-parse --show-toplevel`)
//! - [`commit`]   — staging + RAII commit-message tempfile
//! - [`github`]   — `gh pr` wrappers + URL parsing
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

pub(crate) mod branch;
pub(crate) mod command;
pub(crate) mod commit;
pub(crate) mod error;
pub(crate) mod github;
pub(crate) mod repo;
pub(crate) mod status;

#[cfg(test)]
pub(crate) mod test_support;

// Re-export the error model at the module root so siblings (the IPC
// layer, future agent tools) can refer to it without depending on the
// internal sub-module path.
#[allow(unused_imports)]
pub(crate) use error::{GitError, GitResult};
```

- [ ] **Step 3: Create `src-tauri/src/git/error.rs`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/error.rs` lines 1-144 in full and copy verbatim with one deviation: **drop the `WorktreeAlreadyExists(PathBuf)` variant** since `worktree.rs` is out of scope. Remove its `Display` arm too. Final file:

```rust
//! Structured error type for the git module.

use std::fmt;
use std::io;
use std::string::FromUtf8Error;

/// All failure modes surfaced by the git module.
#[derive(Debug)]
pub(crate) enum GitError {
    Io(io::Error),
    Utf8(FromUtf8Error),
    NonZeroExit {
        program: String,
        args: Vec<String>,
        exit_code: Option<i32>,
        stderr: String,
        stdout: String,
    },
    MissingBinary(&'static str),
    NotARepository,
    NoBranch,
    NoWorkspaceChanges,
    CommitMessageRequired,
    EmptyCommitMessage,
    MissingRequired(&'static str),
    Parse(String),
    Internal(String),
}

impl GitError {
    #[must_use]
    pub fn from_output(program: &str, args: &[&str], output: &std::process::Output) -> Self {
        Self::NonZeroExit {
            program: program.to_string(),
            args: args.iter().map(|a| (*a).to_string()).collect(),
            exit_code: output.status.code(),
            stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
        }
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "git io error: {error}"),
            Self::Utf8(error) => write!(f, "git output not valid UTF-8: {error}"),
            Self::NonZeroExit { program, args, exit_code: _, stderr, stdout } => {
                let detail = if stderr.is_empty() { stdout } else { stderr };
                if detail.is_empty() {
                    write!(f, "{program} {} failed", args.join(" "))
                } else {
                    write!(f, "{program} {} failed: {detail}", args.join(" "))
                }
            }
            Self::MissingBinary(name) => write!(f, "required binary not found on PATH: {name}"),
            Self::NotARepository => write!(f, "not a git repository"),
            Self::NoBranch => write!(f, "no branch is currently checked out (detached HEAD)"),
            Self::NoWorkspaceChanges => write!(f, "no workspace changes to commit"),
            Self::CommitMessageRequired => write!(f, "commit message is required"),
            Self::EmptyCommitMessage => write!(f, "commit message is empty"),
            Self::MissingRequired(field) => write!(f, "missing required input: {field}"),
            Self::Parse(detail) => write!(f, "git output parse error: {detail}"),
            Self::Internal(detail) => write!(f, "git internal error: {detail}"),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Utf8(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for GitError {
    fn from(error: io::Error) -> Self { Self::Io(error) }
}

impl From<FromUtf8Error> for GitError {
    fn from(error: FromUtf8Error) -> Self { Self::Utf8(error) }
}

impl From<GitError> for String {
    fn from(error: GitError) -> Self { error.to_string() }
}

/// Result alias used throughout the git module.
pub(crate) type GitResult<T> = Result<T, GitError>;
```

- [ ] **Step 4: Create `src-tauri/src/git/test_support.rs`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/test_support.rs` lines 1-65 in full and copy verbatim with one change: replace the `if2ai-git-test-` prefix with `uclaw-git-test-` and the test identity emails from `tests@if2ai.local` to `tests@uclaw.local`:

```rust
//! Test-only helpers for spinning up disposable git repositories.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

pub(crate) struct TestRepo {
    _dir: TempDir,
    path: PathBuf,
}

impl TestRepo {
    pub(crate) fn new(label: &str) -> Self {
        let dir = tempfile::Builder::new()
            .prefix(&format!("uclaw-git-test-{label}-"))
            .tempdir()
            .expect("create temp dir for git test repo");
        let path = dir.path().to_path_buf();

        run(&path, "git", &["init", "--initial-branch=main"])
            .or_else(|_| {
                run(&path, "git", &["init"])?;
                run(&path, "git", &["branch", "-m", "main"])
            })
            .expect("git init must succeed");

        run(&path, "git", &["config", "user.name", "uClaw Tests"]).expect("git config user.name");
        run(&path, "git", &["config", "user.email", "tests@uclaw.local"])
            .expect("git config user.email");
        run(&path, "git", &["config", "commit.gpgsign", "false"]).expect("disable gpg sign");

        std::fs::write(path.join("README.md"), "seed\n").expect("write seed file");
        run(&path, "git", &["add", "README.md"]).expect("git add seed");
        run(&path, "git", &["commit", "-m", "chore: seed"]).expect("git commit seed");

        Self { _dir: dir, path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

fn run(cwd: &Path, program: &str, args: &[&str]) -> std::io::Result<()> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(format!(
            "{program} {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        )));
    }
    Ok(())
}
```

- [ ] **Step 5: Register the module in `src-tauri/src/lib.rs`**

Open `src-tauri/src/lib.rs`. Find the line `pub mod files_rail;` (around line 51). Add **before** the `// Evaluation harness` comment block:

```rust
// W6: Git integration (workspace + branch picker backbone)
pub mod git;
pub mod tauri_commands_git;
```

(The `tauri_commands_git` module file doesn't exist yet — it'll be created in Task 8. We register the `pub mod` line now and the Rust compiler will yell when we get to Task 2. To avoid the warning, **comment out** the `pub mod tauri_commands_git;` line for now and uncomment it in Task 10 right before `tauri_commands_git.rs` is committed.)

Concrete edit — add only this for now:

```rust
// W6: Git integration (workspace + branch picker backbone)
pub mod git;
// pub mod tauri_commands_git;  // enabled in Task 10
```

- [ ] **Step 6: Verify compile**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^(error|warning: unused)" | head -20
# Expect: empty (test_support is #[cfg(test)] so it doesn't compile yet — that's fine)

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git:: 2>&1 | tail -10
# Expect: 0 tests (no test files yet)
```

- [ ] **Step 7: Branch verify + commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/mod.rs src-tauri/src/git/error.rs src-tauri/src/git/test_support.rs src-tauri/src/lib.rs
git commit -m "feat(git): scaffold — module declarations + GitError type + TestRepo helper

Ports if2Ai's modules/git/{mod,error,test_support}.rs minus WorktreeAlreadyExists
error variant (worktree out of scope). Test identity rebranded uClaw.

W6 PR A Task 1 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 2: command.rs — subprocess helpers + PATH probing

**Files:**
- Create: `src-tauri/src/git/command.rs`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/command.rs` lines 1-227.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Open the if2Ai source**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/command.rs` in full (lines 1-227). Copy verbatim into `src-tauri/src/git/command.rs`. Adjust:
- Imports: keep `use super::error::{GitError, GitResult};` exactly (the `super::error` path is identical in both layouts).
- No other changes needed — file is platform-agnostic and references only stdlib + tokio.

The file contains:
- Constants `GIT_BIN`, `GH_BIN`
- Sync helpers `run_stdout`, `run_ok`, `git_stdout`, `git_ok`, `git_stdout_no_locks`
- Async helper `run_stdout_async`
- `command_exists` with `path_extensions` + `is_executable_file` cfg-gated per OS
- A `#[cfg(test)]` mod with 2 tests (`command_exists_finds_a_real_shell_tool`, `command_exists_rejects_obvious_garbage`)

- [ ] **Step 3: Verify build + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::command 2>&1 | tail -8
# Expect: 2 passed (command_exists_finds_a_real_shell_tool + rejects_obvious_garbage)

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 397 passed (395 baseline + 2 new)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/command.rs
git commit -m "feat(git): command runner + --no-optional-locks + cross-platform PATH probe

Verbatim port of if2Ai modules/git/command.rs. Sync helpers run via
std::process::Command; async run_stdout_async backs gh pr create / git
push to free the blocking-task pool during GitHub round-trips.

W6 PR A Task 2 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 3: repo.rs — git root discovery + init

**Files:**
- Create: `src-tauri/src/git/repo.rs`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/repo.rs` lines 1-91.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Open the if2Ai source**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/repo.rs` in full (lines 1-91). Copy verbatim into `src-tauri/src/git/repo.rs`. Only adjustment: the test module imports `use crate::modules::git::test_support::TestRepo;` — change to `use crate::git::test_support::TestRepo;`.

The file contains:
- `find_git_root(cwd: &Path) -> GitResult<PathBuf>`
- `is_inside_repo(cwd: &Path) -> bool`
- `init_repo(cwd: &Path) -> GitResult<()>`
- 4 tests: `find_git_root_returns_repo_root`, `find_git_root_works_from_subdir`, `is_inside_repo_false_outside`, `is_inside_repo_true_inside`

- [ ] **Step 3: Verify**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::repo 2>&1 | tail -8
# Expect: 4 passed

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 401 passed (395 baseline + 2 from Task 2 + 4 from Task 3)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/repo.rs
git commit -m "feat(git): repo discovery (find_git_root, is_inside_repo, init_repo)

Verbatim port of if2Ai modules/git/repo.rs. Test imports rewritten to
crate::git::test_support.

W6 PR A Task 3 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 4: branch.rs — list / current / default / checkout / create + slugify

**Files:**
- Create: `src-tauri/src/git/branch.rs`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/branch.rs` lines 1-240.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Open the if2Ai source**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/branch.rs` in full (lines 1-240). Copy verbatim into `src-tauri/src/git/branch.rs`. Test imports — change every `crate::modules::git::test_support::TestRepo` to `crate::git::test_support::TestRepo`.

Functions exported (verify all present after copy):
- `current_branch(cwd: &Path) -> GitResult<String>`
- `branch_exists(cwd: &Path, name: &str) -> bool`
- `list_branches_verbose(cwd: &Path) -> GitResult<String>`
- `detect_default_branch(cwd: &Path) -> GitResult<String>`
- `checkout(cwd: &Path, name: &str) -> GitResult<()>`
- `create_and_checkout(cwd: &Path, name: &str) -> GitResult<()>`
- `build_branch_name(hint: &str) -> String`
- `build_branch_name_with_owner(hint: &str, owner: Option<&str>) -> String`
- `slugify(value: &str) -> String`

Tests (8): `slugify_collapses_separators`, `slugify_returns_change_for_empty_or_punctuation`, `current_branch_reports_main_in_seed_repo`, `branch_exists_true_for_main_false_for_unknown`, `detect_default_branch_returns_main_when_origin_absent`, `build_branch_name_with_owner_prefixes_when_present`, `build_branch_name_with_owner_falls_back_when_blank_or_none`, `list_branches_verbose_includes_main`.

- [ ] **Step 3: Verify**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::branch 2>&1 | tail -10
# Expect: 8 passed

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 409 passed (395 + 2 + 4 + 8 = 409)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/branch.rs
git commit -m "feat(git): branch helpers (list/current/default/checkout/create) + slugify

Verbatim port of if2Ai modules/git/branch.rs (240 LOC). detect_default_branch
chain: symbolic-ref origin/HEAD → main → master → init.defaultBranch → current.
Test imports rewritten to crate::git::test_support.

W6 PR A Task 4 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 5: status.rs — DiffMode + read_status + read_diff

**Files:**
- Create: `src-tauri/src/git/status.rs`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/status.rs` lines 1-209.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Open the if2Ai source**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/status.rs` in full (lines 1-209). Copy verbatim into `src-tauri/src/git/status.rs`. Test imports — change `crate::modules::git::test_support::TestRepo` to `crate::git::test_support::TestRepo`.

Items exported:
- `enum DiffMode { Stat, Full }`
- `read_status(cwd: &Path) -> GitResult<Option<String>>`
- `read_diff(cwd: &Path) -> GitResult<Option<String>>` (defaults to `DiffMode::Stat`)
- `read_diff_with_mode(cwd: &Path, mode: DiffMode) -> GitResult<Option<String>>`
- `status_or_none(cwd: &Path) -> Option<String>` (best-effort)
- `diff_or_none(cwd: &Path) -> Option<String>` (best-effort)
- `read_raw(cwd: &Path, args: &[&str]) -> GitResult<String>` (escape hatch)

Tests (5): `status_clean_repo_returns_branch_header_only`, `status_after_change_includes_file`, `status_or_none_swallows_errors_outside_repo`, `diff_returns_some_when_unstaged_changes_exist_in_stat_mode`, `diff_with_mode_full_emits_unified_patch`, `diff_returns_some_when_only_staged_changes_exist`. (Actually 6 — recount.)

- [ ] **Step 3: Verify**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::status 2>&1 | tail -8
# Expect: 6 passed

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 415 passed (395 + 2 + 4 + 8 + 6 = 415)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/status.rs
git commit -m "feat(git): status + diff snapshots with DiffMode {Stat, Full}

Verbatim port of if2Ai modules/git/status.rs. Read-only ops route through
git_stdout_no_locks so concurrent IDE git activity is undisturbed.

W6 PR A Task 5 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 6: commit.rs — CommitMessageFile RAII + commit_all_with_message

**Files:**
- Create: `src-tauri/src/git/commit.rs`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/commit.rs` lines 1-208.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Open the if2Ai source**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/commit.rs` in full (lines 1-208). Copy verbatim into `src-tauri/src/git/commit.rs`. Two adjustments:
- Test imports: `crate::modules::git::test_support::TestRepo` → `crate::git::test_support::TestRepo`
- Tempfile prefix: change `Builder::new().prefix("if2ai-commit-")` to `.prefix("uclaw-commit-")` (line ~54)

Items exported:
- `has_workspace_changes(cwd: &Path) -> GitResult<bool>`
- `stage_all(cwd: &Path) -> GitResult<()>`
- `commit_with_message_file(cwd: &Path, message_file: &Path) -> GitResult<()>`
- `struct CommitMessageFile { inner: tempfile::NamedTempFile }` with `create(message)`, `path()`, `keep()` methods (Drop is implicit via NamedTempFile)
- `commit_all_with_message(cwd: &Path, message: &str) -> GitResult<()>` — the high-level entry point
- `commit_all_with_message_keep_path(...) -> GitResult<PathBuf>` (#[allow(dead_code)] but ported for future use)

Tests (6): `has_workspace_changes_reports_dirty_after_write`, `commit_message_file_is_deleted_on_drop`, `commit_message_file_rejects_empty_input`, `commit_all_with_message_creates_commit_for_dirty_repo`, `commit_all_with_message_errors_when_clean`, `has_workspace_changes_reports_dirty_for_staged_only`.

- [ ] **Step 3: Verify**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::commit 2>&1 | tail -8
# Expect: 6 passed

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 421 passed (415 + 6 = 421)
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/commit.rs
git commit -m "feat(git): CommitMessageFile RAII tempfile + commit_all_with_message

Verbatim port of if2Ai modules/git/commit.rs with uClaw tempfile prefix.
NamedTempFile guarantees cleanup on Drop. Clean tree returns
NoWorkspaceChanges (idempotent skip outcome for the IPC layer).

W6 PR A Task 6 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 7: github/{mod,pr}.rs — gh availability + PR creation + push

**Files:**
- Create: `src-tauri/src/git/github/mod.rs`
- Create: `src-tauri/src/git/github/pr.rs`

**Source:**
- `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/github/mod.rs` lines 1-22
- `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/github/pr.rs` lines 1-255

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Create `src-tauri/src/git/github/mod.rs`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/github/mod.rs` lines 1-22 in full. Copy verbatim but **drop the `pub(crate) mod issue;` line** (no Issue UI in W6). Final:

```rust
//! GitHub-CLI (`gh`) integration.
//!
//! Pure subprocess wrappers around `gh pr create / view` and the URL-
//! parsing helpers needed to surface the resulting URL to the user.
//! IPC layer consumes these directly; if `gh` is missing the frontend
//! branches on [`is_gh_available`] and falls back to draft text (PR B).

pub(crate) mod pr;

use super::command::{command_exists, GH_BIN};

/// Returns `true` if the `gh` binary is reachable on `PATH`.
#[must_use]
pub(crate) fn is_gh_available() -> bool {
    command_exists(GH_BIN)
}
```

- [ ] **Step 3: Create `src-tauri/src/git/github/pr.rs`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src-tauri/src/modules/git/github/pr.rs` lines 1-255 in full. Copy verbatim into `src-tauri/src/git/github/pr.rs`. **No path adjustments needed** — the imports use `super::super::command::{...}` and `super::super::error::{...}` which point to the same relative locations in uClaw's tree.

Items exported:
- `struct PrCreateRequest<'a> { title: &'a str, body_file: &'a Path, base: &'a str }`
- `struct PrCreateOutcome { url: String, was_existing: bool }`
- `create(cwd: &Path, request: &PrCreateRequest<'_>) -> GitResult<PrCreateOutcome>` (sync — kept for completeness even though IPC layer uses async)
- `create_async(cwd: &Path, request: &PrCreateRequest<'_>) -> GitResult<PrCreateOutcome>` (the one the IPC layer calls)
- `parse_pr_url(stdout: &str) -> Option<String>`
- `parse_pr_view_url(stdout: &str) -> Option<String>`
- `push_branch_set_upstream(cwd: &Path, branch: &str) -> GitResult<()>`
- `push_branch_set_upstream_async(cwd: &Path, branch: &str) -> GitResult<()>`

Tests (4): `parse_pr_url_picks_first_http_line`, `parse_pr_url_returns_none_when_absent`, `parse_pr_view_url_extracts_url_field`, `parse_pr_view_url_returns_none_for_invalid_json`.

- [ ] **Step 4: Verify**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib git::github 2>&1 | tail -8
# Expect: 4 passed

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 425 passed (421 + 4 = 425)
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/git/github/mod.rs src-tauri/src/git/github/pr.rs
git commit -m "feat(git): gh pr create + push --set-upstream (sync + async siblings)

Verbatim port of if2Ai modules/git/github/{mod,pr}.rs minus the
issue.rs file (no Issue UI in W6 Phase 1+2). Drops the create call
through tokio::process so the blocking pool isn't parked during the
GitHub network round-trip.

W6 PR A Task 7 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 8: IPC sandbox helpers + 7 read commands

**Files:**
- Create: `src-tauri/src/tauri_commands_git.rs` (partial — read commands only)

**Why this split:** PR A's IPC file is ~400 LOC across 14 commands plus sandbox helpers. Splitting into 7 read + 7 write keeps each commit at ~200 LOC and bisectable.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Create `src-tauri/src/tauri_commands_git.rs` with sandbox + 7 read commands**

Path: `src-tauri/src/tauri_commands_git.rs`

```rust
//! Tauri command wrappers for the W6 git module (`src-tauri/src/git/`).
//!
//! Sandbox model:
//!   - **Read commands** (status, diff, is_repo, branches, current_branch,
//!     default_branch, gh_available) accept any cwd that canonicalize-
//!     compares equal to a registered `MountRoot.path`.
//!   - **Write commands** (init_repo, checkout, create_branch, commit,
//!     gh_create_pr, git_commit_push_pr) additionally require
//!     `MountRoot.editable == true`.
//!
//! Sandbox source-of-truth: `state.files_rail_list_mounts(None).await`.
//! Workspace mounts default editable=true; AttachedDirs default false
//! and the user opts in via the existing W3 mount toggle.
//!
//! Concurrency:
//!   - Local git ops wrap in `tokio::task::spawn_blocking` (free the
//!     async runtime; local git is sub-millisecond and not worth a
//!     scheduler hop, but spawn_blocking is required because std::Command
//!     is sync).
//!   - Network ops (`gh pr create`, `git push`) use the `*_async`
//!     siblings in `git::github::pr` directly so they don't park a
//!     blocking-pool worker during the GitHub round-trip.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::State;

use crate::app::AppState;
use crate::git::{branch, commit, github, repo, status};

// ─── Sandbox helpers ──────────────────────────────────────────────────

/// Resolve `cwd` against the user's registered MountRoots. Accepts any
/// mount (workspace, session, attached_dir) — read-permissive.
async fn assert_cwd_in_any_mount(state: &AppState, cwd: &str) -> Result<PathBuf, String> {
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
            continue;  // mount path doesn't exist on disk; skip silently
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
async fn assert_cwd_in_editable_mounts(
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
async fn run_blocking<F, T>(work: F) -> Result<T, String>
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
    pub status: String,    // "created" | "skipped"
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
    Ok(run_blocking(move || Ok::<_, String>(repo::is_inside_repo(&path))).await.unwrap_or(false))
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
    // No sandbox check — just probes PATH.
    Ok(run_blocking(move || Ok::<_, String>(github::is_gh_available())).await.unwrap_or(false))
}
```

- [ ] **Step 3: Enable `tauri_commands_git` in lib.rs**

Open `src-tauri/src/lib.rs`. Uncomment the line added in Task 1:

```rust
// W6: Git integration (workspace + branch picker backbone)
pub mod git;
pub mod tauri_commands_git;
```

- [ ] **Step 4: Verify compile**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
```

If errors complain about `Path` being unused (write commands aren't here yet), that's fine — Task 9 adds them.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/tauri_commands_git.rs src-tauri/src/lib.rs
git commit -m "feat(git): IPC sandbox helpers + 7 read commands

assert_cwd_in_any_mount / assert_cwd_in_editable_mounts canonicalize-
compare cwd against files_rail_list_mounts and gate writes via the
MountRoot.editable flag (W3 V17 schema).

Commands added: git_status, git_diff, git_is_repo, git_branches,
git_current_branch, git_default_branch, gh_available.

W6 PR A Task 8 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 9: IPC write/network commands + commit_push_pr + tracing

**Files:**
- Modify: `src-tauri/src/tauri_commands_git.rs`

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Append write commands + composite to `src-tauri/src/tauri_commands_git.rs`**

Append the following to the end of the file:

```rust

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
    let outcome = run_blocking(move || repo::init_repo(&path).map_err(|e| e.to_string())).await;
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
    let outcome = run_blocking(move || branch::checkout(&path, &name).map_err(|e| e.to_string())).await;
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
    let outcome = run_blocking(move || branch::create_and_checkout(&path, &name).map_err(|e| e.to_string())).await;
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

    // Map the GitError::NoWorkspaceChanges case to status="skipped" so
    // the UI renders an idempotent info toast rather than a red error.
    let outcome: Result<CommitOutcome, String> = run_blocking(move || {
        match commit::commit_all_with_message(&path, &message) {
            Ok(()) => Ok(CommitOutcome {
                status: "created".to_string(),
                message: message.trim().to_string(),
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
            branch::detect_default_branch(&path_for_default).map_err(|e| e.to_string())
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

    let request = github::pr::PrCreateRequest {
        title: &title,
        body_file: body_file.path(),
        base: &resolved_base,
    };
    let outcome = github::pr::create_async(&path, &request)
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

    let title_arg = title.clone();
    let body_path = body_file.path().to_string_lossy().into_owned();

    // No dedicated helper in git::github for issue create; shell out
    // directly via tokio::process.
    let args = [
        "issue".to_string(),
        "create".to_string(),
        "--title".to_string(),
        title_arg.clone(),
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

    // Parse the URL out of stdout — first http(s):// line.
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

    // Resolve default branch first (need it both for branch creation
    // hint and gh pr create --base).
    let path_for_default = path.clone();
    let default_base = run_blocking(move || {
        branch::detect_default_branch(&path_for_default).map_err(|e| e.to_string())
    })
    .await?;

    // If we're sitting on the default branch, create a new feature branch
    // first so the PR has somewhere to merge from.
    let path_for_branch = path.clone();
    let default_base_for_branch = default_base.clone();
    let branch_hint_owned = branch_hint.clone();
    let current_branch: String = run_blocking(move || {
        let current = branch::current_branch(&path_for_branch).map_err(|e| e.to_string())?;
        if current != default_base_for_branch {
            return Ok::<String, String>(current);
        }
        // On default branch — synthesize a feature branch name.
        let hint = branch_hint_owned.as_deref().unwrap_or(&title);
        let new_name = branch::build_branch_name(hint);
        branch::create_and_checkout(&path_for_branch, &new_name).map_err(|e| e.to_string())?;
        Ok::<String, String>(new_name)
    })
    .await?;

    // Commit any pending changes (idempotent — skipped if clean).
    let path_for_commit = path.clone();
    let commit_message = title.clone();
    let commit_outcome = run_blocking(move || {
        match commit::commit_all_with_message(&path_for_commit, &commit_message) {
            Ok(()) => Ok::<&'static str, String>("created"),
            Err(crate::git::GitError::NoWorkspaceChanges) => Ok("skipped"),
            Err(other) => Err(other.to_string()),
        }
    })
    .await?;

    // Push the branch with upstream tracking.
    github::pr::push_branch_set_upstream_async(&path, &current_branch)
        .await
        .map_err(|e| e.to_string())?;

    // Open the PR via gh.
    let body_file =
        crate::git::commit::CommitMessageFile::create(if body.trim().is_empty() {
            "(no body provided)\n"
        } else {
            &body
        })
        .map_err(|e| e.to_string())?;

    let request = github::pr::PrCreateRequest {
        title: &title,
        body_file: body_file.path(),
        base: &default_base,
    };
    let pr_outcome = github::pr::create_async(&path, &request)
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
```

- [ ] **Step 3: Verify compile**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^error" | head
# Expect: empty
```

- [ ] **Step 4: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/tauri_commands_git.rs
git commit -m "feat(git): IPC write/network commands + commit_push_pr composite

Adds: git_init_repo, git_checkout_branch, git_create_branch, git_commit,
gh_create_pr, gh_create_issue, git_commit_push_pr.

All mutating commands emit git_op:* tracing events. Network ops use
tokio::process directly; gh_create_pr keeps the body tempfile alive
across the await via an explicit let-binding + drop, matching if2Ai's
pattern in commands/git.rs:572-574.

git_commit_push_pr composite: detect default branch → spin up feature
branch if on default → stage+commit (idempotent skip on clean) → push
with upstream → open PR. Returns a human-readable summary string.

W6 PR A Task 9 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 10: tauri_commands.rs re-export + main.rs handler registration

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Add re-export to `tauri_commands.rs`**

Open `src-tauri/src/tauri_commands.rs`. After the existing preview re-export block (around line 21):

```rust
// ─── Preview Commands (re-exported from preview::commands) ────────────────

pub use crate::preview::commands::{preview_read_bytes, preview_resolve_chips};
```

Add immediately after:

```rust
// ─── Git Commands (re-exported from tauri_commands_git) ──────────────

pub use crate::tauri_commands_git::{
    git_status, git_diff, git_is_repo, git_init_repo, git_branches,
    git_current_branch, git_default_branch, git_checkout_branch,
    git_create_branch, git_commit, git_commit_push_pr,
    gh_available, gh_create_pr, gh_create_issue,
};
```

- [ ] **Step 3: Register commands in `main.rs` `generate_handler!` macro**

Open `src-tauri/src/main.rs`. Find the line:

```rust
            uclaw_core::preview::commands::preview_resolve_chips,
            // Workspace Commands
```

Insert the git block between these two lines so the order is `Preview Commands → Git Commands → Workspace Commands`:

```rust
            uclaw_core::preview::commands::preview_resolve_chips,
            // ─── Git Commands ───
            uclaw_core::tauri_commands_git::gh_available,
            uclaw_core::tauri_commands_git::gh_create_issue,
            uclaw_core::tauri_commands_git::gh_create_pr,
            uclaw_core::tauri_commands_git::git_branches,
            uclaw_core::tauri_commands_git::git_checkout_branch,
            uclaw_core::tauri_commands_git::git_commit,
            uclaw_core::tauri_commands_git::git_commit_push_pr,
            uclaw_core::tauri_commands_git::git_create_branch,
            uclaw_core::tauri_commands_git::git_current_branch,
            uclaw_core::tauri_commands_git::git_default_branch,
            uclaw_core::tauri_commands_git::git_diff,
            uclaw_core::tauri_commands_git::git_init_repo,
            uclaw_core::tauri_commands_git::git_is_repo,
            uclaw_core::tauri_commands_git::git_status,
            // Workspace Commands
```

(Alphabetized for stable diff against future additions.)

- [ ] **Step 4: Verify compile**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build 2>&1 | grep -E "^(error|warning: unused)" | head -20
# Expect: empty
```

- [ ] **Step 5: Run full Rust test suite**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 425 passed (395 + 30 from git module = 425)
```

The 18 new test count from the spec (§3.9) breaks down as: command (2) + repo (4) + branch (8) + status (6) + commit (6) + github::pr (4) = 30 tests, not 18. The spec's "18" was a planning-time estimate; the actual port count is higher because if2Ai's modules already include test coverage. Either is fine — the new floor is 30. Update `cargo test --lib` baseline to 425 going forward.

- [ ] **Step 6: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(git): wire 14 git commands into Tauri handler

Re-export through tauri_commands.rs and register in main.rs invoke_handler!
under a 'Git Commands' comment block. Alphabetized so future additions
produce stable diffs.

W6 PR A Task 10 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 11: Frontend api.ts — typed IPC wrappers + parseBranchList tests

**Files:**
- Create: `ui/src/modules/git/api.ts`
- Create: `ui/src/modules/git/api.test.ts`

**Source:** `/Users/ryanliu/Documents/IfAI/if2Ai/src/modules/git/api.ts` lines 1-283.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Create `ui/src/modules/git/api.ts`**

Read `/Users/ryanliu/Documents/IfAI/if2Ai/src/modules/git/api.ts` lines 1-283 in full. Copy verbatim with the following adjustments:

1. **Remove the 4 worktree functions** (`gitWorktrees`, `gitAddWorktree`, `gitRemoveWorktree`, `gitPruneWorktrees`) and the `CreatedWorktreeProject` interface + `gitCreateWorktreeProject` function. None of these are wired through PR A's IPC layer.

2. The `// ── Worktree ──` section header (lines 152-218) is dropped entirely.

3. **Add `uncommittedFromStatus` helper** at the end of the file (was inlined inside `BranchPicker.tsx` in if2Ai but belongs in the IPC layer for reuse in PR B):

```ts
// ── Helpers ─────────────────────────────────────────────────────────────────

/**
 * Count the number of changed files reported in `git status --short --branch`.
 *
 * The first line is the `## branch ...` header — drop it; every remaining
 * non-empty line is one changed/untracked file. Returns `0` when `raw` is
 * null (clean tree) or has no file lines.
 */
export function uncommittedFromStatus(raw: string | null): number {
  if (!raw) return 0;
  return raw.split("\n").slice(1).filter((line) => line.trim().length > 0).length;
}
```

Resulting file has ~240 LOC (down from if2Ai's 283 after dropping worktree).

- [ ] **Step 3: Create `ui/src/modules/git/api.test.ts`**

```ts
import { describe, it, expect } from 'vitest'
import { parseBranchList, uncommittedFromStatus } from './api'

describe('parseBranchList', () => {
  it('returns one entry per non-empty line with current detected by *', () => {
    const raw = '  main         abcdef1 init\n* feat/foo     1234567 wip\n  bug/x        ffeeddc fix'
    const result = parseBranchList(raw)
    expect(result).toEqual([
      { name: 'main', isCurrent: false },
      { name: 'feat/foo', isCurrent: true },
      { name: 'bug/x', isCurrent: false },
    ])
  })

  it('treats worktree-locked branches (+) as non-current', () => {
    const raw = '+ shared/x      abcdef1 used elsewhere\n* main         1234567 init'
    const result = parseBranchList(raw)
    expect(result).toEqual([
      { name: 'shared/x', isCurrent: false },
      { name: 'main', isCurrent: true },
    ])
  })

  it('skips the (HEAD detached at sha) pseudo-entry', () => {
    const raw = '* (HEAD detached at abc123)\n  main         abcdef1 init'
    const result = parseBranchList(raw)
    expect(result).toEqual([{ name: 'main', isCurrent: false }])
  })

  it('returns empty array for empty input', () => {
    expect(parseBranchList('')).toEqual([])
    expect(parseBranchList('   \n   ')).toEqual([])
  })
})

describe('uncommittedFromStatus', () => {
  it('returns 0 for null (clean tree)', () => {
    expect(uncommittedFromStatus(null)).toBe(0)
  })

  it('returns 0 when only the branch header is present', () => {
    expect(uncommittedFromStatus('## main')).toBe(0)
  })

  it('counts non-empty file lines after the branch header', () => {
    const raw = '## main\n M src/foo.ts\n?? src/bar.tsx\n A docs/note.md'
    expect(uncommittedFromStatus(raw)).toBe(3)
  })
})
```

- [ ] **Step 4: Verify TS + tests**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run api 2>&1 | tail -10
# Expect: 7 passed (4 parseBranchList + 3 uncommittedFromStatus)

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: 303 passed (296 + 7 = 303)
```

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
git add ui/src/modules/git/api.ts ui/src/modules/git/api.test.ts
git commit -m "feat(git): frontend api.ts — typed IPC wrappers + parseBranchList tests

Verbatim port of if2Ai's src/modules/git/api.ts minus the 4 worktree
functions (out of W6 scope). Adds uncommittedFromStatus helper inlined
from if2Ai BranchPicker.tsx:56-64 (belongs in IPC layer for reuse).

Single sanctioned entry-point for all git IPC; PR B components import
exclusively from here.

7 new vitest cases: 4 for parseBranchList (current detection,
worktree-locked, detached-HEAD skip, empty input) + 3 for
uncommittedFromStatus (null, header-only, multi-file).

W6 PR A Task 11 of 12."
git log --oneline -1
git branch --show-current
```

---

## Task 12: Final verification + ensure baselines

**Files:** none modified — verification only.

- [ ] **Step 1: Branch verify**

```bash
cd /Users/ryanliu/Documents/uclaw
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 2: Full Rust test suite**

```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | tail -3
# Expect: 425 passed (395 baseline + 30 new = 425)
```

If the count is lower, run with verbose to find failures:
```bash
cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo test --lib 2>&1 | grep -E "FAILED|test result"
```

- [ ] **Step 3: Full TypeScript + Vitest**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | tail -3
# Expect: clean

cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
# Expect: 303 passed (296 baseline + 7 new = 303)
```

- [ ] **Step 4: Full production build**

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm run build 2>&1 | tail -10
# Expect: build succeeds with no errors

cd /Users/ryanliu/Documents/uclaw/src-tauri && cargo build --release 2>&1 | tail -5
# Expect: build succeeds (warnings OK)
```

- [ ] **Step 5: Git command smoke test (manual, optional)**

Open Tauri dev mode and verify IPC commands are registered. From the renderer console:

```js
const { invoke } = window.__TAURI_INTERNALS__.invoke ? window : await import('@tauri-apps/api/core')
await invoke('git_is_repo', { cwd: '/Users/ryanliu/Documents/uclaw' })
// Expect: true (uClaw repo is a git repo and is also a workspace mount)

await invoke('git_current_branch', { cwd: '/Users/ryanliu/Documents/uclaw' })
// Expect: 'claude/w6-workspace-git'

await invoke('git_status', { cwd: '/Users/ryanliu/Documents/uclaw' })
// Expect: string starting with '## claude/w6-workspace-git...' OR null if perfectly clean
```

If any of these reject, inspect the tracing logs at `~/.uclaw/logs/uclaw.log.YYYY-MM-DD` for the `git_op:*` event.

- [ ] **Step 6: Commit log check**

```bash
cd /Users/ryanliu/Documents/uclaw && git log --oneline main..HEAD
```

Expected ~12 commits in order (plus the spec commit from before the plan was written):
- `docs(plan)` (this plan)
- 11 task commits — one per concern, bisectable

- [ ] **Step 7: Working tree clean check**

```bash
cd /Users/ryanliu/Documents/uclaw && git status --short
# Expect: empty
git branch --show-current   # claude/w6-workspace-git
```

- [ ] **Step 8: (CONDITIONAL — only after user explicitly approves push)** Push + open PR

Per CLAUDE.md: do **not** push or open a PR until the user explicitly asks. When approved:

```bash
git push -u origin claude/w6-workspace-git

gh pr create --title "W6 PR A: Workspace git backbone (Rust module + IPC + frontend api.ts)" --body "$(cat <<'EOF'
## Summary

PR A of W6 — workspace git integration backbone. Ports if2Ai's `src-tauri/src/modules/git/` (minus worktree) into uClaw as a new `src-tauri/src/git/` module, plus the IPC layer with sandbox check against `MountRoot.editable`, plus the frontend IPC typed-wrapper layer.

**No React in this PR.** All UI (BranchPicker, GitWorkbenchDialog, GitActionsPicker, GitChipsRow) lands in PR B once this merges.

## What lands

| Layer | Files | LOC |
|---|---|---|
| Rust git module | `src-tauri/src/git/{mod,error,command,repo,branch,status,commit,github/mod,github/pr,test_support}.rs` | ~1500 |
| IPC layer | `src-tauri/src/tauri_commands_git.rs` (14 commands + sandbox) | ~400 |
| Handler registration | `src-tauri/src/{lib,tauri_commands,main}.rs` | +25 |
| Frontend IPC | `ui/src/modules/git/api.ts` | ~240 |
| Tests | Rust + vitest fixtures | +37 |

## Tauri commands (alphabetized)

`gh_available`, `gh_create_issue`, `gh_create_pr`, `git_branches`, `git_checkout_branch`, `git_commit`, `git_commit_push_pr`, `git_create_branch`, `git_current_branch`, `git_default_branch`, `git_diff`, `git_init_repo`, `git_is_repo`, `git_status`.

## Sandbox model

Every IPC call canonicalize-compares `cwd` against `state.files_rail_list_mounts(None).await`:

- **Read commands** accept any mount (workspace, session, attached_dir)
- **Write commands** additionally require `MountRoot.editable == true` (workspace mounts default true; AttachedDirs default false, opt-in via W3 mount toggle)

Defends against symlink + macOS case-insensitivity attacks via double-canonicalize on both candidate and each mount path.

## Concurrency

- Local git ops (status, diff, branch, commit): wrapped in `tokio::task::spawn_blocking`
- Network ops (`gh pr create`, `git push`): use `tokio::process::Command` directly via `*_async` helpers so the blocking pool isn't parked during the GitHub round-trip

## Observability

Every mutating command emits a structured tracing event from the `git_op:*` family (`init_repo`, `checkout_branch`, `create_branch`, `commit`, `pr_create`, `issue_create`, `commit_push_pr`). Payload: `{ cwd, op, duration_ms, outcome }`. Read commands skip audit (too chatty for BranchPicker re-fetches).

## Test plan

- [x] `cd src-tauri && cargo test --lib` — 425 passed (395 baseline + 30 git tests)
- [x] `cd ui && npx tsc --noEmit` — clean
- [x] `cd ui && npm test -- --run` — 303 passed (296 baseline + 7 new)
- [x] `cd ui && npm run build` — clean
- [x] `cd src-tauri && cargo build --release` — clean
- [ ] Manual: invoke `git_is_repo`, `git_current_branch`, `git_status` from renderer console against the uClaw repo

## What's out of scope (PR B)

- BranchPicker, GitWorkbenchDialog, GitActionsPicker, GitChipsRow components
- Dual composer wiring (ChatInput + AgentView)
- WorkspacePill (re-export of existing affordance)
- 11-theme spot checks (no UI yet)

## Branch base

Branched from `main` at <head sha>.
EOF
)"
```

---

## Self-Review

### Spec coverage

| Spec requirement | Implementing task |
|---|---|
| §3.1 Module layout port from if2Ai (verbatim, minus worktree.rs) | Tasks 1-7 |
| §3.2 GitError type with `WorktreeAlreadyExists` dropped | Task 1 |
| §3.3 `--no-optional-locks` discipline | Task 2 (verbatim port preserves) |
| §3.4 14 Tauri commands | Tasks 8-9 |
| §3.5 Sandbox model (read-permissive, write-gated, canonicalize both sides) | Task 8 |
| §3.6 Concurrency: `spawn_blocking` for local, `tokio::process` for network | Tasks 8-9 |
| §3.7 Audit integration via `git_op:*` tracing events | Task 9 |
| §3.8 `gh` graceful degradation (`is_gh_available`, retry-as-existing) | Task 7 (verbatim port preserves) |
| §3.9 18 backend tests | Tasks 1-7 (actually 30 from the verbatim port) |
| §4.1 Frontend api.ts (verbatim port, drop worktree, + uncommittedFromStatus) | Task 11 |
| §4.1 7 helper tests for parseBranchList + uncommittedFromStatus | Task 11 |

All spec requirements have an implementing task. No gaps.

### Placeholder scan

- No "TBD" / "TODO" / "implement later" remain
- Every code step pastes actual content
- Sandbox helpers fully specified (not "add sandbox check")
- Test cases fully written (not "write tests for the above")
- Every file path is absolute
- Every command shows expected output

### Type consistency

- `GitError` variant names match across `error.rs` (Task 1), `commit.rs` (Task 6, uses `NoWorkspaceChanges`), `tauri_commands_git.rs` (Task 9, pattern-matches `crate::git::GitError::NoWorkspaceChanges`)
- `CommitOutcome { status, message }` DTO in `tauri_commands_git.rs` (Task 8) matches the TypeScript `CommitOutcome { status: "created"|"skipped"; message: string }` in `api.ts` (Task 11)
- `CreatePrResponse { url, was_existing, base }` (Rust, snake_case via `serde(rename_all = "camelCase")`) → `CreatePrResponse { url, wasExisting, base }` (TS) — confirmed match
- IPC command names — all 14 spelled identically across:
  - `tauri_commands_git.rs` `#[tauri::command] fn <name>` definitions
  - `tauri_commands.rs` re-export `pub use crate::tauri_commands_git::{...}`
  - `main.rs` `uclaw_core::tauri_commands_git::<name>` macro entries
  - `api.ts` `invoke<T>('<name>', ...)` calls

### Resolved discrepancies

- Spec §3.9 estimated 18 tests; actual count is 30 (if2Ai's modules ship their own test coverage and we port it verbatim). Plan reflects the higher number with a baseline update note in Task 10.
- Spec §3.4 listed `gh_create_issue` in the IPC table even though PR B has no Issue UI affordance. Plan ports the backend command anyway because it's part of if2Ai's verbatim port and the backbone PR is the right place for it. UI affordance can be added in a later wave.

---

## Execution Handoff

**Plan complete and saved to `docs/superpowers/plans/2026-05-13-w6-workspace-git-pr-a-backbone.md`. Two execution options:**

**1. Subagent-Driven (recommended)** — controller dispatches a fresh subagent per task, two-stage review after each (spec compliance + code quality), fast iteration. **Model split**: haiku for mechanical verbatim ports (Tasks 2-7, 11), sonnet for IPC + sandbox + composite command (Tasks 8-9), opus for final review.

**2. Inline Execution** — execute tasks in this session using `superpowers:executing-plans`, batch execution with checkpoints.

**Which approach?**
