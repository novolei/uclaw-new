//! Filtering rules for directory walks.

use std::collections::HashSet;
use std::sync::LazyLock;

/// Directory basenames the walker always skips. Mirrors Proma's set plus
/// uClaw-specific additions (`pyembed`, `static`, `.uclaw`).
static SKIP_DIRS: LazyLock<HashSet<&'static str>> = LazyLock::new(|| {
    [
        "node_modules", ".git", "dist", ".next", "__pycache__", ".venv",
        "build", ".cache", "target", ".DS_Store", ".idea", ".vscode",
        ".turbo", "coverage", "pyembed", "static", ".uclaw",
    ]
    .into_iter()
    .collect()
});

/// True if the walker should skip an entry with this basename + kind.
pub fn should_ignore(name: &str, is_dir: bool) -> bool {
    if name.starts_with('.') && name != ".gitignore" && name != ".env" {
        return true;
    }
    if is_dir && SKIP_DIRS.contains(name) {
        return true;
    }
    false
}
