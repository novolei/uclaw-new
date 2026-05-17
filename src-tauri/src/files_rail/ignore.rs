//! Filtering rules for directory walks.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Directory basenames the walker always skips. Mirrors Proma's set plus
/// uClaw-specific additions (`pyembed`, `static`).
///
/// `.uclaw` is deliberately NOT in this set: the agent writes plan files,
/// memory snapshots and other user-visible artifacts under
/// `<workspace>/.uclaw/`, so users must be able to browse there from the
/// Files panel. PR #67 already unhid it for agent glob/grep tools — this
/// keeps the Files panel aligned with that decision.
static SKIP_DIRS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "node_modules", ".git", "dist", ".next", "__pycache__", ".venv",
        "build", ".cache", "target", ".DS_Store", ".idea", ".vscode",
        ".turbo", "coverage", "pyembed", "static",
    ]
    .into_iter()
    .collect()
});

/// Basenames that bypass the "hide all dotfiles" rule. `.uclaw` is on
/// this list because it holds agent-written plan / memory artifacts the
/// user needs to see.
const DOTFILE_ALLOWLIST: &[&str] = &[".gitignore", ".env", ".uclaw"];

/// True if the walker should skip an entry with this basename + kind.
pub fn should_ignore(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') && !DOTFILE_ALLOWLIST.contains(&name) {
        return true;
    }
    if is_dir && SKIP_DIRS.contains(name) {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uclaw_directory_is_visible() {
        // Regression: 2026-05-18 gomoku session — agent wrote
        // plans/2026-05-17_191424-网页五子棋小游戏开发计划.md under .uclaw/
        // but the Files panel hid the whole .uclaw directory, so users
        // couldn't see what the agent produced.
        assert!(!should_ignore(".uclaw", true));
    }

    #[test]
    fn other_dotdirs_still_hidden() {
        assert!(should_ignore(".git", true));
        assert!(should_ignore(".cache", true));
        assert!(should_ignore(".venv", true));
        assert!(should_ignore(".vscode", true));
        assert!(should_ignore(".turbo", true));
    }

    #[test]
    fn allowlisted_dotfiles_remain_visible() {
        // The pre-existing carve-outs for .gitignore / .env must still
        // pass after the .uclaw addition.
        assert!(!should_ignore(".gitignore", false));
        assert!(!should_ignore(".env", false));
    }

    #[test]
    fn skip_dirs_still_filtered() {
        assert!(should_ignore("node_modules", true));
        assert!(should_ignore("target", true));
        assert!(should_ignore("pyembed", true));
        assert!(should_ignore("static", true));
    }

    #[test]
    fn ordinary_files_pass_through() {
        assert!(!should_ignore("README.md", false));
        assert!(!should_ignore("src", true));
    }
}
