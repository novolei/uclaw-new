//! Low-level subprocess helpers shared by every higher-level git op.
//!
//! All process execution goes through this file so that:
//! - Output is captured with a single uniform error model ([`GitError`]).
//! - `--no-optional-locks` and other safety flags can be inserted in one
//!   place if we ever need them globally.
//! - PATH lookup ([`command_exists`]) is portable and does not depend on
//!   the probed binary supporting `--version`.
//!
//! ## Public API quick-reference
//!
//! Names diverge slightly from the migration plan (§3) for layering
//! clarity; the implementation is a strict superset of the reference
//! library's `git_stdout` / `git_status_ok`:
//!
//! | Plan name              | Implementation name         |
//! | ---------------------- | --------------------------- |
//! | `run_git_stdout`       | [`git_stdout`]              |
//! | `run_git_ok`           | [`git_ok`]                  |
//! | `format_command_failure` | folded into [`super::error::GitError`] (`Display` + `from_output`) |
//! | (new)                  | [`run_stdout`] / [`run_ok`] (program-agnostic, used by `gh`) |
//! | (new)                  | [`git_stdout_no_locks`] (read-only safety) |

use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::error::{GitError, GitResult};

/// The `git` binary name; centralised so test doubles can shadow it later.
pub(crate) const GIT_BIN: &str = "git";

/// The `gh` (GitHub CLI) binary name.
pub(crate) const GH_BIN: &str = "gh";

/// Run `program args...` in `cwd` and return captured stdout as a UTF-8
/// string. Non-zero exit codes become [`GitError::NonZeroExit`].
pub(crate) fn run_stdout(program: &str, args: &[&str], cwd: &Path) -> GitResult<String> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(GitError::from_output(program, args, &output));
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Run `program args...` in `cwd` and discard stdout/stderr **on
/// success**; on failure both streams are captured inside the returned
/// [`GitError::NonZeroExit`].
pub(crate) fn run_ok(program: &str, args: &[&str], cwd: &Path) -> GitResult<()> {
    let output = Command::new(program).args(args).current_dir(cwd).output()?;
    if !output.status.success() {
        return Err(GitError::from_output(program, args, &output));
    }
    Ok(())
}

/// Async sibling of [`run_stdout`] backed by `tokio::process::Command`.
///
/// **Use only for network-bound binaries** (`gh pr create`, `gh issue
/// create`, future `git push`).  Local git operations are
/// sub-millisecond and not worth the round-trip into the tokio
/// scheduler — they should keep using [`run_stdout`] (sync) wrapped
/// in `spawn_blocking` at the IPC layer.
///
/// The point of this helper is to **free the blocking-task pool while
/// gh is waiting on the GitHub API**.  Without it, every concurrent
/// `gh pr create` parks one blocking-pool worker for several seconds,
/// which can starve other tools (file write, search, etc.) under
/// heavy load.
pub(crate) async fn run_stdout_async(
    program: &str,
    args: &[&str],
    cwd: &Path,
) -> GitResult<String> {
    let output = tokio::process::Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .await?;
    if !output.status.success() {
        return Err(GitError::from_output(program, args, &output));
    }
    Ok(String::from_utf8(output.stdout)?)
}

/// Convenience: run `git args...` in `cwd` and return stdout.
pub(crate) fn git_stdout(cwd: &Path, args: &[&str]) -> GitResult<String> {
    run_stdout(GIT_BIN, args, cwd)
}

/// Convenience: run `git args...` in `cwd` and check the exit status.
pub(crate) fn git_ok(cwd: &Path, args: &[&str]) -> GitResult<()> {
    run_ok(GIT_BIN, args, cwd)
}

/// Run `git args...` with `--no-optional-locks` injected so that read-only
/// inspection (status / diff) cannot interfere with concurrent IDE git
/// activity.  Used by the prompt-injection helpers and by `/diff`.
pub(crate) fn git_stdout_no_locks(cwd: &Path, args: &[&str]) -> GitResult<String> {
    let mut all = Vec::with_capacity(args.len() + 1);
    all.push("--no-optional-locks");
    all.extend_from_slice(args);
    git_stdout(cwd, &all)
}

/// Cross-platform check that `name` is reachable on `PATH`.
///
/// We deliberately do **not** invoke the binary (e.g. with `--version`)
/// because some tools either do not implement that flag or have side
/// effects on first launch.  Instead we walk `PATH` ourselves, honouring
/// `PATHEXT` on Windows.
///
/// `name` must be a bare binary name, not a path: any `/` (or platform
/// separator) defeats the lookup and is rejected up-front so a hostile
/// caller cannot use this helper as a `..` traversal primitive.
#[must_use]
pub(crate) fn command_exists(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') || name.contains(std::path::MAIN_SEPARATOR) {
        return false;
    }

    let path_var = match env::var_os("PATH") {
        Some(value) => value,
        None => return false,
    };

    let extensions = path_extensions();

    for dir in env::split_paths(&path_var) {
        if dir.as_os_str().is_empty() {
            continue;
        }
        if extensions.is_empty() {
            if is_executable_file(&dir.join(name)) {
                return true;
            }
        } else {
            for ext in &extensions {
                let candidate: PathBuf = if ext.is_empty() {
                    dir.join(name)
                } else {
                    dir.join(format!("{name}{ext}"))
                };
                if is_executable_file(&candidate) {
                    return true;
                }
            }
        }
    }

    false
}

#[cfg(windows)]
fn path_extensions() -> Vec<String> {
    // .PS1 is intentionally omitted: PowerShell scripts require an
    // explicit execution policy to run, so probing them as if they were
    // ordinary executables would yield false positives.  `git` and `gh`
    // ship as .EXE, which the default below already covers.
    let raw = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".to_string());
    let mut out: Vec<String> = raw
        .split(';')
        .map(str::trim)
        .filter(|piece| !piece.is_empty())
        .map(|piece| {
            if piece.starts_with('.') {
                piece.to_string()
            } else {
                format!(".{piece}")
            }
        })
        .collect();
    // Always also try the bare name in case PATHEXT excludes it.
    out.push(String::new());
    out
}

#[cfg(not(windows))]
fn path_extensions() -> Vec<String> {
    Vec::new()
}

#[cfg(unix)]
fn is_executable_file(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    match std::fs::metadata(path) {
        Ok(meta) => meta.is_file() && meta.permissions().mode() & 0o111 != 0,
        Err(_) => false,
    }
}

#[cfg(not(unix))]
fn is_executable_file(path: &Path) -> bool {
    std::fs::metadata(path)
        .map(|meta| meta.is_file())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_exists_finds_a_real_shell_tool() {
        // `cargo test` runs in a context where one of these very common
        // binaries must exist; otherwise the harness itself could not
        // have spawned us.
        let candidates = if cfg!(windows) {
            vec!["cmd"]
        } else {
            vec!["sh"]
        };
        assert!(
            candidates.iter().any(|c| command_exists(c)),
            "expected at least one of {candidates:?} on PATH"
        );
    }

    #[test]
    fn command_exists_rejects_obvious_garbage() {
        assert!(!command_exists(""));
        assert!(!command_exists("definitely-not-a-binary-xyz-1234567"));
    }
}
