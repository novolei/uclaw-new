//! Plan-aware termination heuristic.
//!
//! When the agent loop's `handle_text_response` is about to terminate
//! on a text-only LLM response, we check whether any plan file in
//! `<workspace>/.uclaw/plans/` was recently touched and still has
//! `- [ ]` (undone) steps. If yes, the loop should continue (with a
//! nudge prompt) rather than terminate — the LLM probably stopped
//! mid-plan due to a long thinking pause or token quirk, not because
//! the task is actually done.
//!
//! Time window prevents stale plans from older sessions triggering
//! infinite loops on unrelated user replies.

use std::path::Path;
use std::time::{Duration, SystemTime};

/// Scan `<workspace_root>/.uclaw/plans/` for plan files modified within
/// the last `recent_secs` seconds and count `- [ ]` (undone) steps in
/// the most-recently-modified one.
///
/// Returns `Some(undone_count)` if a recent plan with pending steps is
/// found. Returns `None` if:
///   - workspace_root is None
///   - no plans directory exists
///   - no plan files were modified within the window
///   - the most-recent plan has YAML `status: completed` (explicitly closed)
///   - all steps are marked `- [x]`
pub fn pending_plan_steps(workspace_root: Option<&Path>, recent_secs: u64) -> Option<usize> {
    let root = workspace_root?;
    // Defensive: an empty PathBuf joins into a relative path ("" + ".uclaw/plans"
    // → ".uclaw/plans") which would resolve against the binary's CWD instead of
    // the user's workspace. This was a real bug — see active_workspace_root in
    // tauri_commands.rs for the upstream fix. We also reject it here so any
    // future caller can't silently regress the guard.
    if root.as_os_str().is_empty() {
        return None;
    }
    let plans_dir = root.join(".uclaw").join("plans");
    let entries = std::fs::read_dir(&plans_dir).ok()?;

    let cutoff = SystemTime::now().checked_sub(Duration::from_secs(recent_secs))?;

    // Collect (mtime, path) pairs for .md files modified within the window.
    let mut candidates: Vec<(SystemTime, std::path::PathBuf)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("md") {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        let Ok(modified) = meta.modified() else {
            continue;
        };
        if modified < cutoff {
            continue;
        }
        candidates.push((modified, path));
    }
    if candidates.is_empty() {
        return None;
    }

    // Pick the most recently modified plan.
    candidates.sort_by(|a, b| b.0.cmp(&a.0));
    let path = &candidates[0].1;
    let content = std::fs::read_to_string(path).ok()?;

    // Respect explicit completion in YAML frontmatter.
    if content.contains("status: completed") {
        return None;
    }

    let undone = count_undone_steps(&content);
    if undone == 0 {
        None
    } else {
        Some(undone)
    }
}

/// Read a specific plan file by name and count `- [ ]` (undone) steps,
/// **ignoring mtime**. Used by callers that already know which plan is
/// active for this session (e.g. via scanning message history for
/// plan_write / plan_update tool calls). This is the resume-friendly
/// path: the mtime-based `pending_plan_steps` misses plans that haven't
/// been touched in the last 5 minutes, which silently kills the guard
/// for "user comes back after an hour and types 继续" workflows.
///
/// Filename is treated as a bare basename: anything containing path
/// separators or `..` is rejected. Plan files only ever live in
/// `<root>/.uclaw/plans/`, so a directory-walking input is wrong by
/// construction and gets blocked here.
pub fn pending_plan_steps_in_file(workspace_root: Option<&Path>, filename: &str) -> Option<usize> {
    let root = workspace_root?;
    if root.as_os_str().is_empty() {
        return None;
    }
    // Defense in depth: filename ultimately comes from tool arguments
    // (plan_update.filename or parsed from plan_write result text). Reject
    // anything that could escape the plans directory or denote a non-file.
    if filename.is_empty()
        || filename.contains('/')
        || filename.contains('\\')
        || filename.contains("..")
        || filename == "."
    {
        return None;
    }
    let path = root.join(".uclaw").join("plans").join(filename);
    let content = std::fs::read_to_string(&path).ok()?;
    if content.contains("status: completed") {
        return None;
    }
    let undone = count_undone_steps(&content);
    if undone == 0 {
        None
    } else {
        Some(undone)
    }
}

/// Count `- [ ]` checkbox lines (anywhere in the content). Tolerates
/// leading whitespace + lowercase `x`/`X` for done state.
fn count_undone_steps(content: &str) -> usize {
    content
        .lines()
        .filter(|line| {
            let t = line.trim_start();
            t.starts_with("- [ ]") || t.starts_with("* [ ]")
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::SystemTime;
    use tempfile::TempDir;

    fn workspace_with_plan(filename: &str, content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        let plans_dir = dir.path().join(".uclaw").join("plans");
        fs::create_dir_all(&plans_dir).unwrap();
        fs::write(plans_dir.join(filename), content).unwrap();
        dir
    }

    #[test]
    fn returns_none_for_missing_workspace() {
        assert_eq!(pending_plan_steps(None, 300), None);
    }

    #[test]
    fn returns_none_for_empty_pathbuf() {
        // Regression: spaces.path was empty string in the user's DB, which
        // produced PathBuf::from("") and made read_dir resolve a bogus
        // relative path. The guard MUST treat empty PathBuf as None.
        let empty = std::path::PathBuf::from("");
        assert_eq!(pending_plan_steps(Some(empty.as_path()), 300), None);
    }

    #[test]
    fn returns_none_for_no_plans_dir() {
        let dir = TempDir::new().unwrap();
        assert_eq!(pending_plan_steps(Some(dir.path()), 300), None);
    }

    #[test]
    fn returns_count_when_recent_plan_has_undone_steps() {
        let content = "---\nstatus: in_progress\n---\n## Steps\n- [x] 1. done\n- [ ] 2. undone\n- [ ] 3. undone\n";
        let dir = workspace_with_plan("plan.md", content);
        assert_eq!(pending_plan_steps(Some(dir.path()), 300), Some(2));
    }

    #[test]
    fn returns_none_when_all_steps_done() {
        let content = "---\nstatus: in_progress\n---\n## Steps\n- [x] 1. a\n- [x] 2. b\n";
        let dir = workspace_with_plan("plan.md", content);
        assert_eq!(pending_plan_steps(Some(dir.path()), 300), None);
    }

    #[test]
    fn returns_none_when_status_completed() {
        // Even if some boxes look undone, explicit completion wins.
        let content = "---\nstatus: completed\n---\n## Steps\n- [ ] leftover\n";
        let dir = workspace_with_plan("plan.md", content);
        assert_eq!(pending_plan_steps(Some(dir.path()), 300), None);
    }

    #[test]
    fn returns_none_for_old_plans() {
        let content = "---\nstatus: in_progress\n---\n## Steps\n- [ ] 1. undone\n";
        let dir = workspace_with_plan("plan.md", content);
        // 0-second window means "must be modified in the future" — no plan satisfies.
        assert_eq!(pending_plan_steps(Some(dir.path()), 0), None);
    }

    #[test]
    fn picks_most_recent_when_multiple_plans_exist() {
        let dir = TempDir::new().unwrap();
        let plans_dir = dir.path().join(".uclaw").join("plans");
        fs::create_dir_all(&plans_dir).unwrap();
        // Older: all done
        fs::write(
            plans_dir.join("old.md"),
            "---\nstatus: in_progress\n---\n## Steps\n- [x] done\n",
        )
        .unwrap();
        // Sleep 50ms so newer file has visibly later mtime
        std::thread::sleep(std::time::Duration::from_millis(50));
        // Newer: 3 undone — should win
        fs::write(
            plans_dir.join("new.md"),
            "---\nstatus: in_progress\n---\n## Steps\n- [ ] a\n- [ ] b\n- [ ] c\n",
        )
        .unwrap();
        assert_eq!(pending_plan_steps(Some(dir.path()), 300), Some(3));
    }

    #[test]
    fn count_undone_steps_handles_indentation_and_asterisk() {
        let content = "  - [ ] indented\n* [ ] asterisk-style\n  - [x] done\n";
        assert_eq!(count_undone_steps(content), 2);
    }

    // ── pending_plan_steps_in_file (session-history-driven path) ─────
    // Regression: 2026-05-18 04:46 五子棋 resume — user came back >1h after
    // last plan_update so mtime fallback returned None and the guard never
    // engaged. New entry point bypasses mtime and reads a specific file by
    // name, intended for callers that already know which plan is active
    // (e.g. via scanning message history for plan_write/plan_update calls).

    #[test]
    fn in_file_returns_count_regardless_of_mtime() {
        // The whole point: don't care about mtime, just count undone steps.
        let content = "---\nstatus: in_progress\n---\n## Steps\n- [ ] 1. a\n- [ ] 2. b\n";
        let dir = workspace_with_plan("any-age.md", content);
        // Even with 0-second window (which kills the mtime-based fn), the
        // explicit-file variant must still return the count.
        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "any-age.md"),
            Some(2)
        );
    }

    #[test]
    fn in_file_returns_none_for_completed_status() {
        let content = "---\nstatus: completed\n---\n## Steps\n- [ ] still here\n";
        let dir = workspace_with_plan("done.md", content);
        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "done.md"),
            None
        );
    }

    #[test]
    fn in_file_returns_none_when_all_done() {
        let content = "---\nstatus: in_progress\n---\n## Steps\n- [x] a\n- [x] b\n";
        let dir = workspace_with_plan("alldone.md", content);
        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "alldone.md"),
            None
        );
    }

    #[test]
    fn in_file_returns_none_when_file_missing() {
        let dir = TempDir::new().unwrap();
        fs::create_dir_all(dir.path().join(".uclaw").join("plans")).unwrap();
        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "missing.md"),
            None
        );
    }

    #[test]
    fn in_file_rejects_path_traversal() {
        // Defense: filename came from message history which is influenced by
        // tool arguments. Reject anything that could escape .uclaw/plans/.
        let dir = workspace_with_plan(
            "plan.md",
            "---\nstatus: in_progress\n---\n## Steps\n- [ ] x\n",
        );
        // Plant a fake plan-like file OUTSIDE plans/ to prove we don't read it.
        fs::write(
            dir.path().join("escaped.md"),
            "---\nstatus: in_progress\n---\n## Steps\n- [ ] should not be read\n",
        )
        .unwrap();

        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "../escaped.md"),
            None
        );
        assert_eq!(
            pending_plan_steps_in_file(Some(dir.path()), "subdir/plan.md"),
            None
        );
        assert_eq!(pending_plan_steps_in_file(Some(dir.path()), "."), None);
    }

    #[test]
    fn in_file_returns_none_for_missing_workspace() {
        assert_eq!(pending_plan_steps_in_file(None, "any.md"), None);
    }

    #[test]
    fn in_file_returns_none_for_empty_pathbuf() {
        let empty = std::path::PathBuf::from("");
        assert_eq!(
            pending_plan_steps_in_file(Some(empty.as_path()), "any.md"),
            None
        );
    }
}
