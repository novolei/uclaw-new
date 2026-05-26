//! Git Integration — blast_radius computation, Capsule generation triggers,
//! and Git rollback on failure for the GEP self-evolution engine.
//!
//! Integrates with the existing `crate::git` module where applicable, but
//! provides GEP-specific Git operations: diff-stat parsing for BlastRadius,
//! and controlled rollback mechanisms.

use std::process::Command;

use anyhow::{Context, Result};
use tracing::{debug, warn};

use super::types::BlastRadius;

/// Rollback mode for recovering from a failed Gene application.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RollbackMode {
    /// `git stash` — preserves changes for inspection.
    Stash,
    /// `git reset --hard HEAD` — discards all uncommitted changes.
    Hard,
}

/// Compute the blast radius from the workspace's uncommitted changes.
///
/// Runs `git diff --stat HEAD` in the given repo path and parses the
/// summary line for file count and total line changes.
///
/// Returns `Ok(None)` if there are no changes (clean working tree).
pub fn compute_blast_radius(repo_path: &str) -> Result<Option<BlastRadius>> {
    let output = Command::new("git")
        .args(["-C", repo_path, "diff", "--stat", "HEAD"])
        .output()
        .context("Failed to run git diff --stat")?;

    if !output.status.success() {
        // Non-zero exit may mean no git repo or no HEAD yet
        debug!("git diff --stat returned non-zero; blast radius unavailable");
        return Ok(None);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(None);
    }

    Ok(parse_diff_stat(&stdout))
}

/// Parse the last line of `git diff --stat` output.
///
/// Expected format: `X file(s) changed, Y insertion(s)(+), Z deletion(s)(-)"
/// or `X file(s) changed, Y insertion(s)(+)` for insert-only diffs.
fn parse_diff_stat(stdout: &str) -> Option<BlastRadius> {
    // The summary is always the last non-empty line
    let summary = stdout.lines().last()?.trim();

    if summary.is_empty() {
        return None;
    }

    let mut files = 0u32;
    let mut insertions = 0u32;
    let mut deletions = 0u32;

    // Split by ", " — the summary format uses comma-space separators
    let parts: Vec<&str> = summary.split(", ").collect();

    for part in &parts {
        let part = part.trim();

        if part.contains("file") && part.contains("changed") {
            // "1 file changed" or "3 files changed"
            if let Some(num_str) = part.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<u32>() {
                    files = n;
                }
            }
        } else if part.contains("insertion") {
            // "5 insertions(+)" or "1 insertion(+)"
            if let Some(num_str) = part.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<u32>() {
                    insertions = n;
                }
            }
        } else if part.contains("deletion") {
            // "5 deletions(-)" or "1 deletion(-)"
            if let Some(num_str) = part.split_whitespace().next() {
                if let Ok(n) = num_str.parse::<u32>() {
                    deletions = n;
                }
            }
        }
    }

    Some(BlastRadius {
        files,
        lines: insertions + deletions,
    })
}

/// Rollback working-tree changes on failure.
///
/// - `Stash`: runs `git stash` to preserve changes for later inspection.
/// - `Hard`: runs `git reset --hard HEAD` to discard all changes.
pub fn rollback_on_failure(repo_path: &str, mode: &RollbackMode) -> Result<()> {
    match mode {
        RollbackMode::Stash => {
            let output = Command::new("git")
                .args(["-C", repo_path, "stash"])
                .output()
                .context("Failed to run git stash")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("git stash failed: {}", stderr);
            } else {
                debug!("git stash executed successfully");
            }
            Ok(())
        }
        RollbackMode::Hard => {
            let output = Command::new("git")
                .args(["-C", repo_path, "reset", "--hard", "HEAD"])
                .output()
                .context("Failed to run git reset --hard")?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                warn!("git reset --hard failed: {}", stderr);
            } else {
                debug!("git reset --hard executed successfully");
            }
            Ok(())
        }
    }
}

/// Returns true if any Gene was matched in the current turn — which means
/// we should generate a Capsule after the agent finishes executing tools.
pub fn should_generate_capsule(gene_match_count: usize) -> bool {
    gene_match_count > 0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_diff_stat_single_file() {
        let stdout =
            " src/main.rs | 10 +++++-----\n 1 file changed, 5 insertions(+), 5 deletions(-)";
        let result = parse_diff_stat(stdout).unwrap();
        assert_eq!(result.files, 1);
        assert_eq!(result.lines, 10); // 5 + 5
    }

    #[test]
    fn test_parse_diff_stat_multi_file() {
        let stdout = " src/a.rs | 3 +++\n src/b.rs | 2 --\n 2 files changed, 3 insertions(+), 2 deletions(-)";
        let result = parse_diff_stat(stdout).unwrap();
        assert_eq!(result.files, 2);
        assert_eq!(result.lines, 5);
    }

    #[test]
    fn test_parse_diff_stat_insertions_only() {
        let stdout = " src/new.rs | 20 ++++++++++++++++++++\n 1 file changed, 20 insertions(+)";
        let result = parse_diff_stat(stdout).unwrap();
        assert_eq!(result.files, 1);
        assert_eq!(result.lines, 20);
    }

    #[test]
    fn test_parse_diff_stat_empty() {
        assert!(parse_diff_stat("").is_none());
        assert!(parse_diff_stat("   \n   ").is_none());
    }

    #[test]
    fn test_should_generate_capsule() {
        assert!(!should_generate_capsule(0));
        assert!(should_generate_capsule(1));
        assert!(should_generate_capsule(5));
    }
}
