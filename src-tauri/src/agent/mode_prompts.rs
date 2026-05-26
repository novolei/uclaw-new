//! System prompt composition with Karpathy-flavored behavioral guardrails
//! and per-SafetyMode operating constraints.
//!
//! Composition order (top → bottom = LLM priority increasing):
//!   1. User's global system prompt (from Settings → 通用)
//!   2. <workspace>/uclaw.md (workspace-level project context)
//!   3. [WORKSPACE] cwd block
//!   4. Optional persona voice block (expression/style only)
//!   5. KARPATHY_BASELINE (compile-time, always injected)
//!   6. mode_addition (compile-time, by current SafetyMode)
//!
//! Empty layers are skipped; remaining layers joined with "\n\n---\n\n".

use crate::safety::SafetyMode;
use std::path::{Path, PathBuf};
use tauri::Emitter;

pub const KARPATHY_BASELINE: &str = include_str!("prompts/baseline.md");

/// M2-A wire-up — process-wide cached output of the
/// `baseline_blocks::render_all()` registry. Used by `compose_system_prompt`
/// instead of `KARPATHY_BASELINE.trim()` directly.
///
/// Why both? `KARPATHY_BASELINE` remains the include_str!() of the source
/// `baseline.md` file: (a) the Settings UI in
/// `tauri_commands::list_prompt_sources` renders it raw so the user can
/// see attribution + the source-of-truth Markdown; (b) the byte-equal
/// invariant test in `baseline_blocks` anchors against it.
///
/// The trip-wire test
/// `baseline_blocks::tests::render_all_equals_baseline_md_trimmed_byte_for_byte`
/// ensures `karpathy_baseline()` and `KARPATHY_BASELINE.trim()` produce
/// identical bytes — so flipping the const → fn-call here is a no-op in
/// output but unlocks future per-block opt-in / opt-out (M2-H L3 budget
/// gating).
pub fn karpathy_baseline() -> &'static str {
    use std::sync::OnceLock;
    static CACHED: OnceLock<String> = OnceLock::new();
    CACHED
        .get_or_init(crate::agent::baseline_blocks::render_all)
        .as_str()
}

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

/// Start watching `<workspace_root>/uclaw.md` for changes.
/// When the file is modified, emits a `uclaw-md:changed` event via the Tauri app handle.
/// Returns a `JoinHandle` for the watcher task — drop it to stop watching.
pub fn watch_uclaw_md(
    workspace_root: PathBuf,
    app_handle: tauri::AppHandle,
) -> tokio::task::JoinHandle<()> {
    use notify::{EventKind, RecursiveMode, Watcher};
    tokio::task::spawn_blocking(move || {
        let uclaw_md_path = workspace_root.join("uclaw.md");

        // Only watch if the file already exists; if it's created later,
        // the next compose_system_prompt call will pick it up.
        if !uclaw_md_path.exists() {
            tracing::debug!(
                path = %uclaw_md_path.display(),
                "uclaw.md does not exist, skipping file watch"
            );
            return;
        }

        let watch_dir = uclaw_md_path.parent().unwrap_or(&workspace_root).to_path_buf();
        let watch_path = uclaw_md_path.clone();

        let (tx, rx) = std::sync::mpsc::channel();
        let mut watcher = match notify::recommended_watcher(move |res: Result<notify::Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx.send(event);
            }
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to create uclaw.md file watcher");
                return;
            }
        };

        if let Err(e) = watcher.watch(&watch_dir, RecursiveMode::NonRecursive) {
            tracing::warn!(error = %e, path = %watch_dir.display(), "Failed to watch directory for uclaw.md");
            return;
        }

        tracing::debug!(path = %watch_path.display(), "Watching uclaw.md for changes");

        for event in rx {
            let is_uclaw_md = event.paths.iter().any(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n == "uclaw.md")
                    .unwrap_or(false)
            });

            if !is_uclaw_md {
                continue;
            }

            let is_modify = matches!(
                event.kind,
                EventKind::Modify(_) | EventKind::Create(_)
            );

            if is_modify {
                tracing::info!(path = %watch_path.display(), "uclaw.md changed, emitting event");
                let _ = app_handle.emit("uclaw-md:changed", serde_json::json!({
                    "path": watch_path.display().to_string(),
                }));
            }
        }
    })
}

pub fn compose_system_prompt(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
) -> String {
    compose_with_baseline(user_global_base, workspace_root, mode, karpathy_baseline())
}

pub fn compose_system_prompt_with_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    persona_block: Option<&str>,
) -> String {
    compose_with_baseline_and_persona(
        user_global_base,
        workspace_root,
        mode,
        karpathy_baseline(),
        persona_block,
    )
}

/// B2 — injection-aware variant of [`compose_system_prompt`].
///
/// Identical to [`compose_system_prompt`] in every respect (preserves
/// `user_global_base`, workspace `uclaw.md`, the `[WORKSPACE]` cwd
/// block, and the mode addition) EXCEPT the Karpathy baseline section is
/// rendered via A4's [`baseline_blocks::render_with_context`] so per-turn
/// [`InjectionContext`](crate::agent::baseline_blocks::InjectionContext)
/// state can gate conditional baseline blocks.
///
/// **Cache discipline**: all 10 current production blocks use the
/// default `Always` policy, so for today's registry this is
/// byte-identical to [`compose_system_prompt`] for every
/// `InjectionContext`. The injection channel is wired but inert until a
/// future PR adds a non-`Always` block — at which point turn 1 (with
/// `FirstActTurnOnly`) may differ from turns 2+ by design (spec §8.6),
/// while turns 2+ remain byte-stable against each other.
pub fn compose_system_prompt_with_injection(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    injection_ctx: &crate::agent::baseline_blocks::InjectionContext,
) -> String {
    let baseline = crate::agent::baseline_blocks::render_with_context(injection_ctx);
    compose_with_baseline(user_global_base, workspace_root, mode, &baseline)
}

pub fn compose_system_prompt_with_injection_and_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    injection_ctx: &crate::agent::baseline_blocks::InjectionContext,
    persona_block: Option<&str>,
) -> String {
    let baseline = crate::agent::baseline_blocks::render_with_context(injection_ctx);
    compose_with_baseline_and_persona(
        user_global_base,
        workspace_root,
        mode,
        &baseline,
        persona_block,
    )
}

/// Shared composition body. `baseline` is the already-rendered Karpathy
/// baseline section (from `karpathy_baseline()` or
/// `render_with_context(...)`); everything else is identical between the
/// two public entry points.
fn compose_with_baseline(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    baseline: &str,
) -> String {
    compose_with_baseline_and_persona(user_global_base, workspace_root, mode, baseline, None)
}

fn compose_with_baseline_and_persona(
    user_global_base: &str,
    workspace_root: Option<&Path>,
    mode: &SafetyMode,
    baseline: &str,
    persona_block: Option<&str>,
) -> String {
    let workspace_md = read_uclaw_md(workspace_root);
    let mode_part = mode_addition(mode);
    let persona = persona_block.unwrap_or_default().trim();
    // Tell the agent its real cwd so it doesn't hallucinate paths when the
    // user asks "where am I" / "what's my workspace". Without this, the
    // agent has no signal about the actual filesystem location and either
    // makes one up or asks to run `pwd`. The shell, read_file, glob, etc.
    // tools are all already pinned to this directory at registration time.
    //
    // NOTE: do NOT include phrases like "run pwd to verify" here — they
    // implicitly permit the model to probe the workspace with shell tools
    // in response to conversational status questions ("你在干啥" etc.),
    // triggering spurious glob/ls/date calls the user never requested.
    // M2-T1a — workspace path block rendered via uclaw_utils_template
    // (Phase 0.5 ported crate). One placeholder ({{cwd}}); fallback to
    // empty string is preserved via `unwrap_or_default()` after we map
    // the Option<&Path>. Render failure also degrades to empty so a
    // template typo can't crash the agent loop's system prompt build.
    const WORKSPACE_PATH_TEMPLATE: &str = "[WORKSPACE]\nYour current working directory is: {{cwd}}\nAll relative paths in shell, file, and glob tools resolve from this directory. When the user asks where files live or what the cwd is, answer directly with this path. Do NOT call shell commands (pwd, ls, glob, find, etc.) to probe or verify the workspace unless the user explicitly requests a file or directory operation.";
    let workspace_path_block = workspace_root
        .map(|p| {
            let cwd = p.display().to_string();
            uclaw_utils_template::render(WORKSPACE_PATH_TEMPLATE, [("cwd", cwd.as_str())])
                .unwrap_or_else(|e| {
                    tracing::warn!(
                        "M2-T1a: workspace-path template render failed: {e};                          falling back to empty string"
                    );
                    String::new()
                })
        })
        .unwrap_or_default();
    let parts: Vec<&str> = [
        user_global_base.trim(),
        workspace_md.as_str(),
        workspace_path_block.as_str(),
        persona,
        // M2-A wire-up — registry-rendered baseline (output identical to
        // KARPATHY_BASELINE.trim() by invariant test, see baseline_blocks).
        // B2: passed in by the caller so compose_system_prompt_with_injection
        // can substitute the injection-aware render without duplicating the
        // rest of the composition.
        baseline,
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

    use crate::agent::baseline_blocks::InjectionContext;

    #[test]
    fn compose_with_injection_baseline_matches_plain_compose() {
        // B2 cache-discipline invariant: with InjectionContext::baseline()
        // the injection-aware compose must be byte-identical to the
        // pre-B2 compose_system_prompt path. This proves we did NOT drop
        // user_global_base / workspace_md / workspace_path_block and did
        // NOT alter the baseline rendering for the current registry.
        for mode in [
            SafetyMode::Plan,
            SafetyMode::Supervised,
            SafetyMode::Ask,
            SafetyMode::AcceptEdits,
            SafetyMode::Yolo,
        ] {
            let plain = compose_system_prompt("base prompt", None, &mode);
            let injected = compose_system_prompt_with_injection(
                "base prompt",
                None,
                &mode,
                &InjectionContext::baseline(),
            );
            assert_eq!(plain, injected, "mode {mode:?} must be byte-identical");
        }
    }

    #[test]
    fn compose_with_injection_is_byte_stable_across_inj_variants() {
        // All 10 production blocks are Always-policy → flipping injection
        // state must NOT change the composed prompt bytes. Guards the
        // "turns 2+ byte-stable" requirement on the live hot path.
        let first = InjectionContext {
            is_first_act_turn: true,
            last_error_kind: Some("Timeout".into()),
            context_pressure_ratio: 0.95,
        };
        let later = InjectionContext::baseline();
        let a = compose_system_prompt_with_injection(
            "base", None, &SafetyMode::Supervised, &first,
        );
        let b = compose_system_prompt_with_injection(
            "base", None, &SafetyMode::Supervised, &later,
        );
        assert_eq!(a, b, "production registry must not vary by injection");
    }

    #[test]
    fn compose_with_injection_preserves_user_base_and_workspace() {
        let dir = tmp_workspace_with_uclaw("# project rules\nuse rust 2021");
        let out = compose_system_prompt_with_injection(
            "MY_USER_BASE",
            Some(dir.path()),
            &SafetyMode::Plan,
            &InjectionContext::baseline(),
        );
        assert!(out.contains("MY_USER_BASE"), "user base dropped");
        assert!(out.contains("# project rules"), "uclaw.md dropped");
        assert!(out.contains("[WORKSPACE]"), "workspace path block dropped");
        assert!(out.contains(&dir.path().display().to_string()), "cwd dropped");
        assert!(out.contains("THINK BEFORE CODING"), "baseline dropped");
        assert!(out.contains("PLAN MODE"), "mode addition dropped");
    }

    #[test]
    fn persona_block_is_below_workspace_and_above_baseline() {
        let dir = tmp_workspace_with_uclaw("# project rules\nuse rust 2021");
        let out = compose_system_prompt_with_persona(
            "base",
            Some(dir.path()),
            &SafetyMode::Supervised,
            Some("[PERSONA]\nSpeak with warmth."),
        );

        let workspace = out.find("[WORKSPACE]").unwrap();
        let persona = out.find("[PERSONA]").unwrap();
        let baseline = out.find("THINK BEFORE CODING").unwrap();
        assert!(workspace < persona, "persona must follow workspace");
        assert!(persona < baseline, "persona must precede baseline");
    }

    #[test]
    fn empty_persona_block_matches_plain_compose() {
        let plain = compose_system_prompt("base", None, &SafetyMode::Plan);
        let with_empty =
            compose_system_prompt_with_persona("base", None, &SafetyMode::Plan, Some("  \n"));
        assert_eq!(plain, with_empty);
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
        // No uclaw.md, so parts are: base + [WORKSPACE] + baseline (Auto mode adds no extra)
        // → exactly 2 separators between the 3 parts.
        let sep_count = out.matches("\n\n---\n\n").count();
        assert_eq!(sep_count, 2, "Expected base|workspace|baseline, got {} separators", sep_count);
        assert!(out.contains("[WORKSPACE]"), "workspace block missing");
        assert!(out.contains(&dir.path().display().to_string()), "workspace path missing");
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
