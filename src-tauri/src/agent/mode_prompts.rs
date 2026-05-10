//! System prompt composition with Karpathy-flavored behavioral guardrails
//! and per-SafetyMode operating constraints.
//!
//! Composition order (top → bottom = LLM priority increasing):
//!   1. User's global system prompt (from Settings → 通用)
//!   2. <workspace>/uclaw.md (workspace-level project context)
//!   3. KARPATHY_BASELINE (compile-time, always injected)
//!   4. mode_addition (compile-time, by current SafetyMode)
//!
//! Empty layers are skipped; remaining layers joined with "\n\n---\n\n".

use crate::safety::SafetyMode;
use std::path::Path;

pub const KARPATHY_BASELINE: &str = include_str!("prompts/baseline.md");

const MODE_ASK: &str = include_str!("prompts/mode_ask.md");
const MODE_ACCEPT_EDITS: &str = include_str!("prompts/mode_accept_edits.md");
const MODE_PLAN: &str = include_str!("prompts/mode_plan.md");
const MODE_BYPASS: &str = include_str!("prompts/mode_bypass.md");

pub fn mode_addition(mode: &SafetyMode) -> &'static str {
    match mode {
        SafetyMode::Ask => MODE_ASK,
        SafetyMode::AcceptEdits => MODE_ACCEPT_EDITS,
        SafetyMode::Plan => MODE_PLAN,
        SafetyMode::Supervised => "", // Auto — baseline alone
        SafetyMode::Yolo => MODE_BYPASS,
    }
}

/// Read `<workspace_root>/uclaw.md` if it exists, returning trimmed content
/// (or empty string if missing/unreadable). Reads on every call — files are
/// small and OS file cache handles it. If profiling later shows hot path,
/// add an LRU cache.
fn read_uclaw_md(workspace_root: Option<&Path>) -> String {
    workspace_root
        .map(|root| root.join("uclaw.md"))
        .and_then(|p| std::fs::read_to_string(&p).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

pub fn compose_system_prompt(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
) -> String {
    let workspace_md = read_uclaw_md(workspace_root);
    let mode_part = mode_addition(mode);
    let parts: Vec<&str> = [
        user_global_base.trim(),
        workspace_md.as_str(),
        KARPATHY_BASELINE.trim(),
        mode_part,
    ]
    .iter()
    .copied()
    .filter(|s| !s.is_empty())
    .collect();
    parts.join("\n\n---\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn tmp_workspace_with_uclaw(content: &str) -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("uclaw.md"), content).unwrap();
        dir
    }

    #[test]
    fn compose_includes_baseline_and_mode_for_plan() {
        let out = compose_system_prompt("base", None, &SafetyMode::Plan);
        assert!(out.contains("base"));
        assert!(out.contains("THINK BEFORE CODING"), "baseline missing");
        assert!(out.contains("PLAN MODE"), "plan mode addition missing");
    }

    #[test]
    fn compose_auto_mode_omits_addition() {
        let out = compose_system_prompt("base", None, &SafetyMode::Supervised);
        assert!(out.contains("base"));
        assert!(out.contains("THINK BEFORE CODING"));
        assert!(!out.contains("[ASK PERMISSIONS"));
        assert!(!out.contains("[PLAN MODE"));
        assert!(!out.contains("[BYPASS"));
    }

    #[test]
    fn compose_includes_uclaw_md_when_present() {
        let dir = tmp_workspace_with_uclaw("# project rules\nuse rust 2021");
        let out = compose_system_prompt("base", Some(dir.path()), &SafetyMode::Supervised);
        assert!(out.contains("# project rules"));
        assert!(out.contains("use rust 2021"));
    }

    #[test]
    fn compose_skips_missing_uclaw_md() {
        let dir = TempDir::new().unwrap(); // no uclaw.md inside
        let out = compose_system_prompt("base", Some(dir.path()), &SafetyMode::Supervised);
        // Should be exactly: base + sep + baseline (Auto mode adds no extra)
        let sep_count = out.matches("\n\n---\n\n").count();
        assert_eq!(sep_count, 1, "Expected exactly one separator (base|baseline), got {}", sep_count);
    }

    #[test]
    fn compose_handles_empty_user_base() {
        let out = compose_system_prompt("", None, &SafetyMode::Plan);
        // Should be: baseline + sep + plan (no leading base)
        assert!(!out.starts_with("\n"), "should not start with separator");
        assert!(out.contains("THINK BEFORE CODING"));
        assert!(out.contains("PLAN MODE"));
    }
}
