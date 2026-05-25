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

// ────────────────────────────────────────────────────────────────────────
// A4: JIT injection channel — policy enum + context struct
// ────────────────────────────────────────────────────────────────────────

/// When a [`BaselineBlock`] should be included in the rendered system
/// prompt. Evaluated per render call against an [`InjectionContext`].
///
/// Policy hierarchy is **disjunctive** — a block declaring
/// `FirstActTurnOnly` is included iff the context says first ACT turn,
/// regardless of error/pressure state. There is currently no
/// composite/AND policy; add later if a real use case demands it.
///
/// Default for every block is [`InjectionPolicy::Always`], which matches
/// the pre-A4 "always include everything" behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InjectionPolicy {
    /// Default — block always appears. Matches pre-A4 behavior.
    Always,
    /// Appears only on the first user request after the task enters
    /// ACT (or AcceptEdits) mode. Used for verbose tool/protocol specs
    /// that the LLM learns from once and recalls for subsequent turns
    /// via prompt cache.
    FirstActTurnOnly,
    /// Appears only when the previous turn ended with a tool execution
    /// error. Used for recovery hints / structured retry guidance.
    OnErrorRecovery,
    /// Appears only when the token budget for this task is > 75% of
    /// the model context window. Matches Dirac's auto-condense threshold
    /// (research doc §2.1). Exclusive: exactly 0.75 does NOT fire.
    OnContextPressure,
}

impl InjectionPolicy {
    /// Returns `true` if a block with this policy should be included
    /// in a render under the given context.
    ///
    /// `OnContextPressure` threshold is exclusive (`> 0.75`) per spec §8.2.
    pub fn applies(self, ctx: &InjectionContext) -> bool {
        match self {
            Self::Always => true,
            Self::FirstActTurnOnly => ctx.is_first_act_turn,
            Self::OnErrorRecovery => ctx.last_error_kind.is_some(),
            Self::OnContextPressure => ctx.context_pressure_ratio > 0.75,
        }
    }
}

/// Per-render context that [`render_with_context`] consults to decide
/// which non-[`InjectionPolicy::Always`] blocks to include.
///
/// Populated by `compose_system_prompt`'s caller (likely
/// `dispatcher::effective_system_prompt` or `agentic_loop`).
/// During A4's PR, callers don't populate this — it stays as the
/// channel interface for the M2-A finalization PR to plug into.
///
/// Derives `Clone + Debug + Default` — B2 consumes it via clone.
#[derive(Debug, Clone, Default)]
pub struct InjectionContext {
    /// `true` iff this is the first user request after the task entered
    /// ACT (or AcceptEdits) mode. Reset to `false` on subsequent turns
    /// within the same mode. Reset to `true` if user toggles back to
    /// Plan and re-enters ACT.
    pub is_first_act_turn: bool,
    /// `Some(kind)` iff the last tool execution returned a structured
    /// error. `None` on success / first turn / non-tool turns. Used by
    /// `OnErrorRecovery` blocks to surface recovery guidance.
    pub last_error_kind: Option<String>,
    /// Ratio of estimated tokens used / model context window. `0.0..=1.0`.
    /// Used by `OnContextPressure` blocks to gate inclusion.
    pub context_pressure_ratio: f32,
}

impl InjectionContext {
    /// Equivalent to "Always blocks only." All fields at default/zero values.
    /// Useful for callers that don't yet populate the context and for tests.
    pub fn baseline() -> Self {
        Self::default()
    }
}

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

pub struct GoalDrivenExecution;
impl BaselineBlock for GoalDrivenExecution {
    fn id(&self) -> &'static str {
        "guardrail.goal-driven-execution"
    }
    fn title(&self) -> &'static str {
        "Goal-driven execution"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "discipline", "plan-loop"]
    }
    fn render(&self) -> String {
        "4. GOAL-DRIVEN EXECUTION. Transform vague requests into verifiable goals.\n   For multi-step work, state your plan as `1. step → verify: check`.\n   Loop until verify passes; don't stop at \"I think it works\".".into()
    }
}

pub struct NeverFakeProgress;
impl BaselineBlock for NeverFakeProgress {
    fn id(&self) -> &'static str {
        "guardrail.never-fake-progress"
    }
    fn title(&self) -> &'static str {
        "Never fake progress"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "discipline", "plan-loop", "honesty"]
    }
    fn render(&self) -> String {
        "5. NEVER FAKE PROGRESS. Bookkeeping tools (`plan_update`, `plan_write`,\n   `TodoWrite`) ONLY update tracking files — they do NOT execute work.\n   NEVER mark a step `done:true` unless you have already called the\n   tool that actually does the work (`edit`, `write_file`, `bash`, etc.)\n   and verified it succeeded. The user sees the artifacts on disk,\n   not your checkmarks. If the artifact is missing, the step is not done.".into()
    }
}

pub struct NoFileContentAsText;
impl BaselineBlock for NoFileContentAsText {
    fn id(&self) -> &'static str {
        "guardrail.no-file-content-as-text"
    }
    fn title(&self) -> &'static str {
        "Never output file content as text"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "tool-call-shape", "file-io"]
    }
    fn render(&self) -> String {
        "6. NEVER OUTPUT FILE CONTENT AS TEXT. To create or modify a file you\n   MUST call `write_file` or `edit`. Putting code or file content in\n   your reply text does NOT create or modify any file — the user cannot\n   use text output as actual code. Always call the tool, never describe\n   what you would write.".into()
    }
}

pub struct ChunkedLargeFiles;
impl BaselineBlock for ChunkedLargeFiles {
    fn id(&self) -> &'static str {
        "guardrail.chunked-large-files"
    }
    fn title(&self) -> &'static str {
        "For large files, write in chunks"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["guardrail", "tool-call-shape", "file-io", "output-budget"]
    }
    fn render(&self) -> String {
        "7. FOR LARGE FILES, WRITE IN CHUNKS. A single tool call can hold roughly\n   250-300 lines before hitting output limits. For larger files: call\n   `write_file` with the first 250-300 lines, then call `write_file`\n   again (or `edit`) for each subsequent section. Never attempt to write\n   an 800-line file in one shot — it will be truncated. Plan how many\n   chunks you need before you start, then execute them one tool call at\n   a time.".into()
    }
}

pub struct ModeChangeSuggestions;
impl BaselineBlock for ModeChangeSuggestions {
    fn id(&self) -> &'static str {
        "section.mode-change-suggestions"
    }
    fn title(&self) -> &'static str {
        "Mode-change suggestions"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["section", "mode", "tool-call-shape"]
    }
    fn render(&self) -> String {
        "## Mode-change suggestions\n\nYou can request a mode change with `request_plan_mode_switch` when the\nuser's request is multi-step build/refactor/design work AND they're\ncurrently in Supervised or Yolo mode. Call it BEFORE other tool calls.\nDon't call it for: bug fixes you already understand, single-file edits,\nread-only questions, or after the user has explicitly said \"just do it\".\nThe tool is fire-and-forget; the agent continues regardless.".into()
    }
}

pub struct WhenToCallAskUser;
impl BaselineBlock for WhenToCallAskUser {
    fn id(&self) -> &'static str {
        "section.when-to-call-ask-user"
    }
    fn title(&self) -> &'static str {
        "When to call ask_user"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["section", "ask-user", "policy"]
    }
    fn render(&self) -> String {
        "## When to call ask_user\n\nCall `ask_user` when you need a decision from the user before continuing:\n- The request has 2+ plausible interpretations and your guess could be\n  wrong by 50%+\n- You're about to do something destructive (delete, force-push, drop\n  table) without an explicit prior OK\n- A critical design choice depends on user preference (library choice,\n  API contract shape, file structure)\n\nDo NOT call ask_user for:\n- Trivial yes/no answerable from project context (CLAUDE.md, code)\n- Clarifying typos or grammar\n- Asking permission for things that are already auto-approved by mode".into()
    }
}

pub struct HeaderBlock;
impl BaselineBlock for HeaderBlock {
    fn id(&self) -> &'static str {
        "header.attribution"
    }
    fn title(&self) -> &'static str {
        "Header + attribution"
    }
    fn topics(&self) -> &'static [&'static str] {
        &["header", "attribution"]
    }
    fn render(&self) -> String {
        // The HTML comment is preserved verbatim so future template
        // operators can update attribution / source URL via this block.
        // The "[Behavioral guardrails — apply to every action]" tag
        // line follows immediately so the rendered output matches the
        // existing baseline.md byte-for-byte.
        "<!-- Behavioral guardrails adapted from Andrej Karpathy's observations on LLM\n     coding pitfalls. Source: https://github.com/forrestchang/andrej-karpathy-skills\n     License: MIT. Editable via Settings → 提示词 → 行为护栏 (read-only preview only). -->\n\n[Behavioral guardrails — apply to every action]".into()
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
            &HeaderBlock,
            &ThinkBeforeCoding,
            &SimplicityFirst,
            &SurgicalChanges,
            &GoalDrivenExecution,
            &NeverFakeProgress,
            &NoFileContentAsText,
            &ChunkedLargeFiles,
            &ModeChangeSuggestions,
            &WhenToCallAskUser,
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
    fn registry_has_ten_blocks_in_canonical_order() {
        let r = registry();
        assert_eq!(r.len(), 10);
        let ids: Vec<&str> = r.iter().map(|b| b.id()).collect();
        assert_eq!(
            ids,
            vec![
                "header.attribution",
                "guardrail.think-before-coding",
                "guardrail.simplicity-first",
                "guardrail.surgical-changes",
                "guardrail.goal-driven-execution",
                "guardrail.never-fake-progress",
                "guardrail.no-file-content-as-text",
                "guardrail.chunked-large-files",
                "section.mode-change-suggestions",
                "section.when-to-call-ask-user",
            ]
        );
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

    /// **Strongest M2-A invariant.** The registry's `render_all()` output
    /// must equal `baseline.md` (trimmed) byte-for-byte. This is the
    /// trip-wire that will guarantee the upcoming follow-up PR (which
    /// swaps `KARPATHY_BASELINE` to call `render_all()`) is a safe
    /// drop-in: if either side drifts, this test fails immediately.
    #[test]
    fn render_all_equals_baseline_md_trimmed_byte_for_byte() {
        let from_registry = render_all();
        let from_file = crate::agent::mode_prompts::KARPATHY_BASELINE.trim();
        if from_registry != from_file {
            // On mismatch, print a short diff hint so the reviewer can
            // see *where* the registry diverges from the file without
            // grokking the full ~1.5KB string blob.
            let common = from_registry
                .chars()
                .zip(from_file.chars())
                .take_while(|(a, b)| a == b)
                .count();
            panic!(
                "render_all() != baseline.md.trim()\n\
                 first {} chars match; divergence starts here:\n\
                 ── registry side ───────\n{}\n\
                 ── baseline file ───────\n{}\n",
                common,
                &from_registry.chars().skip(common).take(120).collect::<String>(),
                &from_file.chars().skip(common).take(120).collect::<String>(),
            );
        }
    }
}
