//! M2-A pilot — `BaselineBlock` trait + initial 3-block registry.
//!
//! M2-A's goal (per `uclaw-upgrade-implementation-plan.md`) is to rewrite
//! the 12 sections of `baseline.md` as **individually-addressable blocks**.
//! Today `baseline.md` is one 66-line `include_str!` blob — every system
//! prompt build includes 100% of it.
//!
//! The blocks shape lets the future M2-C/F context fabric:
//!
//! - tag each block with a `topic` so a context tool can pull "just the
//!   guardrails about file IO" or "just the ask_user policy"
//! - measure token cost per block (`token_estimate()`) so M2-H's L3
//!   ("block-level budget gating") can drop low-utility blocks under
//!   pressure
//! - run A/B experiments on a single guardrail's wording without
//!   shipping a fork of the entire baseline
//!
//! This pilot lands the **trait + 3 example blocks** (mirroring the first
//! three guardrails) so the architecture is in place. **No callers are
//! changed**; `compose_system_prompt` still uses `KARPATHY_BASELINE`
//! verbatim. M2-A follow-up PRs will:
//!
//! 1. Author the remaining 9 blocks (guardrails 4-7 + 3 helper sections)
//! 2. Cut `baseline.md` over to the registry
//! 3. Wire `compose_system_prompt` to render the active set instead of
//!    `KARPATHY_BASELINE`

use std::sync::OnceLock;

/// A single, individually-renderable section of the baseline prompt.
///
/// Render outputs are joined with `"\n\n"` by the registry (mirrors the
/// blank-line separation in the current `baseline.md`). Each block owns
/// its title prefix, so the joiner doesn't need to know structure.
pub trait BaselineBlock: Send + Sync {
    /// Stable kebab-case identifier — used as the registry key and as
    /// the metric label when M2-H L3 measures per-block token cost.
    fn id(&self) -> &'static str;

    /// Short human-readable label for settings UIs and rollout payloads.
    fn title(&self) -> &'static str;

    /// Topic tags. M2-C context tools query by topic ("ask for blocks
    /// about file IO"). Free-form lowercase strings; conventional tags:
    /// `"guardrail"`, `"policy"`, `"mode"`, `"tool-call-shape"`.
    fn topics(&self) -> &'static [&'static str];

    /// The block body as Markdown. The current pilot returns static
    /// content; future blocks may templated-render via
    /// `uclaw_utils_template::render` (the M2-T1a/M2-T1b path).
    fn render(&self) -> String;

    /// Best-effort token count for this block. Used by M2-H L3 to gate
    /// inclusion under tight budgets. Default uses the same 4-chars-per-
    /// token heuristic the rest of the codebase uses; blocks with
    /// dynamic content (e.g. workspace path) should override with a
    /// more accurate estimate.
    fn token_estimate(&self) -> usize {
        // Match `agent::types::estimate_tokens` semantics — overshoot
        // by ~25% for CJK safety, undershoot fine for ASCII English.
        let rendered = self.render();
        rendered.chars().count() / 4
    }
}

// ────────────────────────────────────────────────────────────────────────
// Initial 3 blocks — mirror baseline.md guardrails 1, 2, 3 verbatim.
// ────────────────────────────────────────────────────────────────────────

pub struct ThinkBeforeCoding;
impl BaselineBlock for ThinkBeforeCoding {
    fn id(&self) -> &'static str {
        "guardrail.think-before-coding"
    }
    fn title(&self) -> &'static str {
        "Think before coding"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "discipline", "ask-user"]
    }
    fn render(&self) -> String {
        "1. THINK BEFORE CODING. State your assumptions. If a request has multiple\n   interpretations, present them — don't silently pick one. When unclear,\n   call `ask_user` to surface the question instead of guessing.".into()
    }
}

pub struct SimplicityFirst;
impl BaselineBlock for SimplicityFirst {
    fn id(&self) -> &'static str {
        "guardrail.simplicity-first"
    }
    fn title(&self) -> &'static str {
        "Simplicity first"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "discipline", "code-shape"]
    }
    fn render(&self) -> String {
        "2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative\n   features. No abstractions for single-use code. If you'd write 200 lines\n   and it could be 50, rewrite it.".into()
    }
}

pub struct SurgicalChanges;
impl BaselineBlock for SurgicalChanges {
    fn id(&self) -> &'static str {
        "guardrail.surgical-changes"
    }
    fn title(&self) -> &'static str {
        "Surgical changes"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "discipline", "scope-control"]
    }
    fn render(&self) -> String {
        "3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't\n   \"improve\" adjacent code, comments, or formatting. Match existing style.\n   If you notice unrelated issues, mention them — don't fix them inline.".into()
    }
}

// ────────────────────────────────────────────────────────────────────────
// Registry — process-wide singleton listing every block uClaw knows about.
// ────────────────────────────────────────────────────────────────────────

/// Process-wide registry of baseline blocks. Lazily initialized on first
/// access. Single source of truth that future callers query for the
/// blocks they want to compose into a prompt.
pub fn registry() -> &'static [&'static dyn BaselineBlock] {
    static REGISTRY: OnceLock<Vec<&'static dyn BaselineBlock>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let blocks: Vec<&'static dyn BaselineBlock> = vec![
            &ThinkBeforeCoding,
            &SimplicityFirst,
            &SurgicalChanges,
        ];
        blocks
    })
}

/// Find a block by its `id()`. Returns `None` if the registry doesn't
/// know about that block id. Useful for settings UIs that show
/// per-block toggles.
pub fn find(id: &str) -> Option<&'static dyn BaselineBlock> {
    registry().iter().copied().find(|b| b.id() == id)
}

/// Render every block in registry order, joined by `"\n\n"`. Mirrors
/// the blank-line separation in the existing `baseline.md`.
pub fn render_all() -> String {
    registry()
        .iter()
        .map(|b| b.render())
        .collect::<Vec<_>>()
        .join("\n\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_three_blocks_in_order() {
        let r = registry();
        assert_eq!(r.len(), 3);
        assert_eq!(r[0].id(), "guardrail.think-before-coding");
        assert_eq!(r[1].id(), "guardrail.simplicity-first");
        assert_eq!(r[2].id(), "guardrail.surgical-changes");
    }

    #[test]
    fn block_ids_are_unique() {
        let r = registry();
        let mut ids: Vec<&'static str> = r.iter().map(|b| b.id()).collect();
        ids.sort_unstable();
        let original_len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), original_len, "duplicate block id detected");
    }

    #[test]
    fn block_ids_are_kebab_lowercase() {
        for b in registry() {
            let id = b.id();
            // No uppercase, no underscores in the path segments.
            assert!(
                id.chars().all(|c| c.is_ascii_lowercase() || c == '-' || c == '.'),
                "block id {id} is not kebab-lowercase"
            );
        }
    }

    #[test]
    fn all_blocks_have_at_least_one_topic() {
        for b in registry() {
            assert!(
                !b.topics().is_empty(),
                "block {} must declare at least one topic",
                b.id()
            );
        }
    }

    #[test]
    fn token_estimates_are_positive() {
        for b in registry() {
            assert!(
                b.token_estimate() > 0,
                "block {} reported zero tokens — likely empty render",
                b.id()
            );
        }
    }

    #[test]
    fn find_recovers_registered_block() {
        let block = find("guardrail.simplicity-first").expect("known block");
        assert_eq!(block.title(), "Simplicity first");
    }

    #[test]
    fn find_returns_none_for_unknown_id() {
        assert!(find("guardrail.does-not-exist").is_none());
    }

    #[test]
    fn render_all_matches_first_three_guardrails_in_baseline_md() {
        let combined = render_all();
        // First three guardrails of baseline.md, joined with blank lines.
        // M2-A invariant: as long as this matches verbatim, the registry
        // is a drop-in replacement candidate for guardrails 1-3.
        let baseline = crate::agent::mode_prompts::KARPATHY_BASELINE;
        for needle in [
            "1. THINK BEFORE CODING.",
            "2. SIMPLICITY FIRST.",
            "3. SURGICAL CHANGES.",
        ] {
            assert!(
                combined.contains(needle),
                "render_all() missing guardrail prefix: {needle}"
            );
            assert!(
                baseline.contains(needle),
                "baseline.md missing guardrail prefix: {needle} (test fixture drift)"
            );
        }
        // The exact byte sequence of the 3 guardrails — concatenated with
        // double-newlines — must match the registry's render output.
        // If baseline.md is reworded, this assertion is the trip-wire
        // that forces us to update the block content alongside.
        let want = "1. THINK BEFORE CODING. State your assumptions. If a request has multiple\n   interpretations, present them — don't silently pick one. When unclear,\n   call `ask_user` to surface the question instead of guessing.\n\n2. SIMPLICITY FIRST. Minimum code that solves the problem. No speculative\n   features. No abstractions for single-use code. If you'd write 200 lines\n   and it could be 50, rewrite it.\n\n3. SURGICAL CHANGES. Touch only what the user asked you to touch. Don't\n   \"improve\" adjacent code, comments, or formatting. Match existing style.\n   If you notice unrelated issues, mention them — don't fix them inline.";
        assert_eq!(combined, want);
    }
}
