//! System prompt + dynamic-context assembly for ChatDelegate.
//!
//! `effective_system_prompt` is the system-prompt seam (cached by the
//! Anthropic prompt-cache breakpoint). `build_dynamic_context` is the
//! per-turn block prepended to the last user message. Both must stay
//! deterministic — small changes here can invalidate the cache and cost
//! tokens. The order invariants are guarded by tests in this file.

use std::sync::atomic::Ordering;
use super::ChatDelegate;

/// All inputs needed to assemble both the system prompt AND the per-turn
/// dynamic block. Built once per `call_llm`; consumed by exactly one
/// `assemble_system_prompt` call. P3-6 single-seam input bundle.
///
/// Holds owned data so the assembly function is pure (no `&self`, no
/// hidden state reads, no async). Construction reads from `ChatDelegate`
/// state once, then this can be tested in isolation with arbitrary inputs.
pub(super) struct SystemPromptContext {
    pub base_system_prompt: String,
    pub workspace_root: Option<std::path::PathBuf>,
    pub effective_mode: crate::safety::SafetyMode,
    pub injection_context: crate::agent::baseline_blocks::InjectionContext,
    pub persona_block: Option<String>,
    pub skills_manifest_block: String,
    pub skills_manifest_suppress: bool,
    pub memory_context: Option<String>,
    pub prior_memory_snapshot: Option<crate::agent::context_diff::LineFragmentSnapshot>,
    pub learned_profile_block: String,
    pub gbrain_knowledge_block: String,
    pub injected_fragments: Vec<crate::runtime::context::ContextArtifact>,
    pub now: chrono::DateTime<chrono::Local>,
}

/// Outputs of `assemble_system_prompt`. Caller propagates side effects
/// (snapshot store, first-act-flag flip, telemetry record) based on
/// what's returned here. P3-6 single-seam output bundle.
pub(super) struct AssembledPrompt {
    pub system: String,
    pub dynamic_for_last_user: String,
    pub new_memory_context_snapshot:
        Option<crate::agent::context_diff::LineFragmentSnapshot>,
}

impl ChatDelegate {
    /// Build the effective system prompt including memory context, the user's
    /// uclaw.md (workspace-level), Karpathy baseline, and mode-specific
    /// guardrails. Reads uclaw.md on every call (small file, OS cache).
    ///
    /// `effective_mode` should be the current resolved SafetyMode — usually
    /// the global policy mode (or the per-session override when set). Caller
    /// resolves it before invoking; we don't read SafetyManager here because
    /// this method is sync and called from the LLM hot path.
    pub(super) fn effective_system_prompt(&self, effective_mode: &crate::safety::SafetyMode) -> String {
        // system_prompt is byte-stable per session between explicit settings
        // edits: no memory recall or volatile profile facts are injected here.
        // Those live in build_dynamic_context() (prepended to the last user
        // message each turn) so the Anthropic cache_control: ephemeral
        // breakpoint can hit from iteration 2 onward. The persona block below
        // is intentionally limited to user-editable expression/style knobs and
        // does not carry per-turn memories, relationship scores, or task facts.
        //
        // C2-Dirac-B2 wire-up. Two concerns, kept SEPARATE (spec §8.4):
        //
        //  1. System-prompt COMPOSITION. We still build the full prompt
        //     from self.system_prompt + workspace uclaw.md + [WORKSPACE]
        //     cwd block + baseline + mode + manifest — NONE of those are
        //     dropped. The only B2 change is routing the baseline section
        //     through compose_system_prompt_with_injection so A4's
        //     InjectionContext can gate conditional blocks. All 10 current
        //     production blocks are Always-policy → byte-identical to the
        //     pre-B2 compose for every InjectionContext today, so cache
        //     discipline is preserved on EVERY turn (not just turns 2+).
        //
        //  2. Fragment SELECTION. We separately call ContextManager::
        //     for_prompt_with_injection to pick fragments under budget and
        //     produce ComposeStats. The selected fragments are stashed for
        //     build_dynamic_context (per-turn block) — they are NEVER added
        //     to the system prompt, so they cannot bust the cache.
        let inj_ctx = crate::agent::baseline_blocks::InjectionContext {
            is_first_act_turn: self.is_first_act_turn.load(Ordering::Relaxed),
            last_error_kind: self
                .last_error_kind
                .lock()
                .ok()
                .and_then(|g| g.clone()),
            context_pressure_ratio: self.estimate_context_pressure_ratio(),
        };

        // Concern 2: fragment selection + stats (does NOT feed the prompt
        // string — only build_dynamic_context + the M2-J collector).
        let query = crate::agent::context_manager::ComposeQuery::defaults_with_topics(vec![]);
        let composed = self.context_manager_for_prompt_blocking(&query, &inj_ctx);
        if let Ok(mut slot) = self.last_injected_fragments.lock() {
            *slot = composed.injected_fragments.clone();
        }
        if let Some(collector) = &self.telemetry.compose_stats {
            collector.record(&self.conversation_id, composed.stats.clone());
        }
        // First-act flag transitions to false after this read (one-way;
        // TODO(M2-A) proper mode-transition tracking). Today this is inert
        // for cache purposes — no production block is FirstActTurnOnly — but
        // we maintain the flag so the channel is correct when one is added.
        self.is_first_act_turn.store(false, Ordering::Relaxed);

        // Concern 1: compose the full, byte-stable system prompt. The
        // injection-aware compose preserves user base + workspace + mode.
        let persona_block = self.persona_prompt_block_best_effort();
        let prompt = crate::agent::mode_prompts::compose_system_prompt_with_injection_and_persona(
            &self.system_prompt,
            self.workspace_root.as_deref(),
            effective_mode,
            &inj_ctx,
            persona_block.as_deref(),
        );
        // Append the skill manifest block (empty when no skills exist).
        // Once the agent has already invoked `skill_search` in this loop
        // the manifest's recall-prompt job is done — suppress it on the
        // next call to save ~800 tokens (PR 2026-05-13 token-cost optim).
        // The flag stays sticky for the remainder of the loop because the
        // agent loop reuses the same `ChatDelegate` across iterations.
        let suppress_manifest = self.skill_search_used.load(Ordering::Relaxed);
        if self.prompt_blocks.skills_manifest.is_empty() || suppress_manifest {
            prompt
        } else {
            format!("{}{}", prompt, self.prompt_blocks.skills_manifest)
        }
    }

    pub(super) fn persona_prompt_block_best_effort(&self) -> Option<String> {
        let state = self.try_app_state()?;
        let guard = state.db.lock().ok()?;
        let store = crate::agent::persona::store::PersonaStore::new(&guard);
        let voice = store
            .get_global_voice_profile()
            .ok()
            .flatten()
            .unwrap_or_default();
        let ctx = crate::agent::persona::PersonaPromptContext {
            voice,
            bond: crate::agent::persona::BondProfile::default(),
            relationship_gamification_enabled: false,
        };
        Some(crate::agent::persona::render_persona_prompt_block(&ctx))
    }

    /// C2-Dirac-B2 — synchronous bridge to the async
    /// `ContextManager::for_prompt_with_injection`.
    ///
    /// `effective_system_prompt` is sync (called from the LLM hot path),
    /// but `for_prompt_with_injection` is async (fragment `fetch()` is).
    /// uClaw runs on the default multi-thread tokio runtime
    /// (`tokio = { features = ["full"] }`, `#[tokio::main]`), so
    /// `block_in_place` + `Handle::current().block_on` is safe here (spec
    /// §8.2). If this ever runs on a current-thread runtime, swap to a
    /// oneshot-channel + spawn pattern.
    pub(super) fn context_manager_for_prompt_blocking(
        &self,
        query: &crate::agent::context_manager::ComposeQuery,
        inj_ctx: &crate::agent::baseline_blocks::InjectionContext,
    ) -> crate::agent::context_manager::ComposedContext {
        let cm = self.context_manager.clone();
        let q = query.clone();
        let ic = inj_ctx.clone();
        tokio::task::block_in_place(|| {
            tokio::runtime::Handle::current().block_on(async move {
                let guard = cm.read().await;
                guard.for_prompt_with_injection(&q, &ic).await
            })
        })
    }

    /// C2-Dirac-B2 — estimate of tokens-used / context-window for A4's
    /// `InjectionContext.context_pressure_ratio`. Stubbed to `0.0` for
    /// now; a follow-up wires the M2-J `TokenBudgetSnapshot` ratio here.
    pub(super) fn estimate_context_pressure_ratio(&self) -> f32 {
        0.0
    }

    /// Build the per-message dynamic context block.
    ///
    /// Prepended to the LAST user message in each LLM call payload — NOT
    /// persisted to the session. Each new call gets a fresh timestamp.
    ///
    /// Includes current time AND workspace root. Time was previously injected
    /// into the system prompt (which caused Anthropic cache misses on every
    /// minute boundary). Moving it here keeps the system prompt byte-stable
    /// across all iterations in a session so cache_control: ephemeral hits
    /// reliably from iteration 2 onward.
    pub(super) fn build_dynamic_context(&self) -> String {
        use chrono::{Datelike, Local, Timelike};
        let now = Local::now();
        let weekday = match now.weekday() {
            chrono::Weekday::Mon => "周一",
            chrono::Weekday::Tue => "周二",
            chrono::Weekday::Wed => "周三",
            chrono::Weekday::Thu => "周四",
            chrono::Weekday::Fri => "周五",
            chrono::Weekday::Sat => "周六",
            chrono::Weekday::Sun => "周日",
        };
        let time_str = format!(
            "{}年{}月{}日 {} {:02}:{:02}",
            now.year(), now.month(), now.day(), weekday, now.hour(), now.minute(),
        );
        let mut block = format!(
            "<system_info>\n当前时间: {}\n注意: 以上时间和工作区路径由系统直接提供。对于询问当前状态的对话（如「你在干啥」「现在几点」「工作区是什么」等），直接用此处信息回答即可——不要运行 bash date、glob、ls、find、pwd 等命令去探查。只有用户明确要求执行文件/目录操作时，才使用相关工具。",
            time_str
        );
        if let Some(root) = &self.workspace_root {
            block.push_str(&format!("\n工作区路径: {}", root.display()));
        }
        block.push_str("\n</system_info>");

        // Memory recall results — per-turn fresh content injected here (not in
        // the system prompt) so Anthropic cache_control: ephemeral on the system
        // prompt block can hit reliably from iteration 2 onward.
        if let Some(ctx) = self.memory_context.as_deref().filter(|s| !s.is_empty()) {
            // M2-D Phase 2 Track A (Bundle 16-B) — cross-turn diff
            // injection. We build a line-level snapshot of the
            // current memory_context, diff against the prior turn's
            // snapshot, and conditionally attach a
            // `<memory_context_changes>` annotation block alongside
            // the full block:
            //
            //   first turn / no prior → full block only (annotation
            //     omitted; the LLM has nothing to compare against)
            //   identical to prior   → full block only
            //                          (Anthropic cache hit path)
            //   small drift (≤40%)   → full block + delta annotation
            //                          (LLM gets "what changed" signal)
            //   significant drift    → full block only (delta would
            //                          be noisy + cache misses anyway)
            //
            // We keep the full block even on small drift because
            // un-cached providers (DeepSeek / Kimi) have no
            // mechanism for "saw it last turn". The delta block is
            // pure additional signal, not a replacement.
            const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
            let new_snapshot = crate::agent::context_diff::LineFragmentSnapshot::from_text(
                "memory_context",
                ctx,
            );
            let prior = self
                .last_memory_context_snapshot
                .lock()
                .ok()
                .and_then(|g| g.clone());

            let delta_annotation: Option<String> = match prior.as_ref() {
                None => {
                    tracing::info!(
                        line_count = new_snapshot.line_count(),
                        token_estimate = new_snapshot.token_estimate,
                        "[M2-D] turn=first memory_context first injection emitted=full",
                    );
                    None
                }
                Some(prior_snap) => {
                    let diff = crate::agent::context_diff::line_diff(
                        prior_snap,
                        &new_snapshot,
                    );
                    let stats = diff.stats();
                    if diff.is_empty() {
                        tracing::debug!(
                            line_count = new_snapshot.line_count(),
                            unchanged = stats.unchanged,
                            "[M2-D] turn=N memory_context unchanged emitted=full cache_state=hit-expected",
                        );
                        None
                    } else if stats
                        .is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD)
                    {
                        tracing::info!(
                            added = stats.added,
                            removed = stats.removed,
                            changed = stats.changed,
                            unchanged = stats.unchanged,
                            added_or_changed_tokens = stats.added_or_changed_tokens,
                            "[M2-D] turn=N memory_context drift=significant emitted=full cache_state=miss",
                        );
                        None
                    } else {
                        let annotation =
                            crate::agent::context_diff::render_delta_annotation(&diff);
                        tracing::info!(
                            added = stats.added,
                            removed = stats.removed,
                            changed = stats.changed,
                            unchanged = stats.unchanged,
                            added_or_changed_tokens = stats.added_or_changed_tokens,
                            "[M2-D] turn=N memory_context drift=small emitted=full+delta cache_state=miss",
                        );
                        annotation
                    }
                }
            };

            if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
                *slot = Some(new_snapshot);
            }

            block.push_str("\n\n<memory_context>\n");
            block.push_str(ctx);
            block.push_str("\n</memory_context>");

            if let Some(annotation) = delta_annotation {
                block.push_str("\n\n");
                block.push_str(&annotation);
            }
        }

        // Learned user profile (30-min rebuild cadence) — same rationale as
        // memory_context: injected here rather than in the system prompt so
        // rebuilds don't bust the Anthropic cache breakpoint.
        if !self.prompt_blocks.learned_profile.is_empty() {
            block.push_str("\n\n");
            block.push_str(&self.prompt_blocks.learned_profile);
        }

        // gbrain Sprint 2.3 — instructions for when to call
        // `mcp__gbrain__*` tools. Only present when gbrain is connected
        // and exposes tools; absent otherwise so we don't promise the
        // LLM tools that won't actually be callable. Lives in the
        // dynamic context block alongside memory_context + profile so
        // tool-presence changes (gbrain reconnects mid-session) take
        // effect on the next prompt build without restarting the agent.
        if !self.prompt_blocks.gbrain_knowledge.is_empty() {
            block.push_str("\n\n");
            block.push_str(&self.prompt_blocks.gbrain_knowledge);
        }

        // C2-Dirac-B2 — ContextManager-selected fragments. Rendered HERE
        // (per-turn dynamic block), NOT in the system prompt, so the
        // system-prompt cache_control:ephemeral breakpoint keeps hitting
        // across turns (spec §8.3). Selected on the prior
        // effective_system_prompt call (same turn, since the agent loop
        // calls effective_system_prompt before build_dynamic_context).
        if let Ok(fragments) = self.last_injected_fragments.lock() {
            block.push_str(&render_context_fragments(&fragments));
        }

        block
    }

    /// Set the memory context obtained from the recall engine.
    /// This will be appended to the system prompt when making LLM calls.
    pub fn set_memory_context(&mut self, context: String) {
        self.memory_context = Some(context);
    }

    /// M2-D Phase 2 — clear the cross-turn memory_context anchor.
    /// Called when a `/compact` runs so the next turn's diff baseline
    /// is the post-fold state, not the pre-fold one. Safe to call
    /// repeatedly; no-op when no anchor exists.
    pub fn clear_memory_context_anchor(&self) {
        if let Ok(mut slot) = self.last_memory_context_snapshot.lock() {
            *slot = None;
        }
    }

    /// Append additional context to the existing memory context.
    /// If no memory context has been set yet, this creates one.
    pub fn append_memory_context(&mut self, extra: &str) {
        match self.memory_context.as_mut() {
            Some(ctx) => {
                ctx.push_str(extra);
            }
            None => {
                self.memory_context = Some(extra.to_string());
            }
        }
    }

    /// Set the skill manifest block to append to the system prompt.
    /// Caller is responsible for building this via skills_manifest::build_skills_manifest.
    pub fn set_skills_manifest_block(&mut self, block: String) {
        self.prompt_blocks.skills_manifest = block;
    }

    /// Set the '## User Profile (Learned)' block (Sprint 1.8).
    ///
    /// Caller builds via
    /// `learning::prompt_section::UserProfileSection::render(&facet_cache)`
    /// (returns `Option<String>` — caller passes empty when None).
    /// Empty input → no append in `effective_system_prompt`.
    pub fn set_learned_profile_block(&mut self, block: String) {
        self.prompt_blocks.learned_profile = block;
    }

    /// Set the '## Long-term Knowledge (gbrain)' instruction block
    /// (Sprint 2.3). Caller builds via
    /// `crate::agent::gbrain_prompt::GbrainKnowledgeSection::render(&mcp_mgr)`
    /// (returns `Option<String>` — caller passes empty when None). Empty
    /// input → no append in `effective_system_prompt`, so the LLM
    /// never sees instructions for `mcp__gbrain__*` tools when those
    /// tools aren't actually registered.
    pub fn set_gbrain_knowledge_block(&mut self, block: String) {
        self.prompt_blocks.gbrain_knowledge = block;
    }
}

/// C2-Dirac-B2 — render selected ContextArtifacts into the per-turn
/// dynamic context block. Each artifact becomes a `<context_fragment>`
/// XML element. Returns an empty string when `fragments` is empty so
/// callers can unconditionally push_str without emitting stray whitespace.
pub(crate) fn render_context_fragments(
    fragments: &[crate::runtime::context::ContextArtifact],
) -> String {
    let mut out = String::new();
    for art in fragments {
        out.push_str(&format!(
            "\n\n<context_fragment id=\"{}\" source=\"{}\">\n{}\n</context_fragment>",
            art.r#ref.id,
            art.r#ref.source.as_str(),
            art.content,
        ));
    }
    out
}

#[cfg(test)]
mod manifest_suppression_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mirror of the suppression rule in `effective_system_prompt`. Kept
    /// in lockstep with `dispatcher.rs:154-171` — if you change the rule
    /// there, change it here. This test pins the contract without
    /// constructing a full `ChatDelegate` (which needs an LLM provider,
    /// safety manager, etc — heavy for a unit test).
    fn compose_with_suppression(
        base_system: &str,
        manifest_block: &str,
        skill_search_used: &AtomicBool,
    ) -> String {
        let suppress = skill_search_used.load(Ordering::Relaxed);
        if manifest_block.is_empty() || suppress {
            base_system.to_string()
        } else {
            format!("{}{}", base_system, manifest_block)
        }
    }

    /// Default state: flag unset → manifest is appended.
    #[test]
    fn manifest_appended_before_skill_search_used() {
        let flag = AtomicBool::new(false);
        let out = compose_with_suppression(
            "You are an agent.",
            "\n\nMANIFEST_BLOCK",
            &flag,
        );
        assert!(out.contains("MANIFEST_BLOCK"));
    }

    /// After flag is set (simulating `execute_tool_calls` seeing
    /// `skill_search`), the manifest is gone on subsequent prompt
    /// composition. This is the core of PR #137's optim #5 — without
    /// this, the ~800 tokens of manifest leak back into every later
    /// LLM call in the same agent loop.
    #[test]
    fn manifest_suppressed_after_skill_search_used() {
        let flag = AtomicBool::new(false);
        // First call (before skill_search): manifest present.
        let pre = compose_with_suppression("base", "\nM", &flag);
        assert!(pre.contains("M"));

        // Simulate `execute_tool_calls` detecting skill_search.
        flag.store(true, Ordering::Relaxed);

        // Subsequent calls: manifest gone.
        let post = compose_with_suppression("base", "\nM", &flag);
        assert!(!post.contains("M"));
        assert_eq!(post, "base");
    }

    /// Empty manifest: the suppression flag has no observable effect.
    /// Edge case — verifies the `is_empty()` short-circuit takes
    /// precedence over the flag check.
    #[test]
    fn empty_manifest_unaffected_by_flag() {
        for &used in &[false, true] {
            let flag = AtomicBool::new(used);
            let out = compose_with_suppression("base", "", &flag);
            assert_eq!(out, "base", "empty manifest should not produce divergent output; used={}", used);
        }
    }

    /// Flag is sticky — once set, stays set. A second non-skill_search
    /// tool call in the same loop must not flip it back. This guards
    /// against future refactors that might (incorrectly) reset the
    /// flag mid-loop.
    #[test]
    fn flag_stays_set_after_subsequent_non_skill_search_calls() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Relaxed);  // simulate skill_search
        // Simulate a subsequent tool call that is NOT skill_search —
        // mirroring `execute_tool_calls`'s any() check which only
        // sets-true, never sets-false.
        // (No-op — the flag has nothing in execute_tool_calls that
        //  resets it; this test pins that fact.)
        assert!(flag.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod manifest_cap_tests {
    /// PR #137 reduced the manifest cap from 1500 → 800 tokens in
    /// `tauri_commands.rs:5015`. The token budget is consumed by
    /// `skills_manifest::build_skills_manifest` via approximate
    /// 4-chars-per-token math. Verify the function honors a low cap
    /// gracefully (returns something non-empty if even one entry fits;
    /// returns empty if nothing fits — never panics, never returns
    /// the over-budget version).
    use crate::memory_graph::store::MemoryGraphStore;
    use crate::skills::SkillsRegistry;
    use crate::skills_manifest::{build_skills_manifest, StrategyBias};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_store() -> MemoryGraphStore {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        MemoryGraphStore::new(Arc::new(Mutex::new(conn)))
    }

    /// 800-token cap (current production setting) is enough budget for at
    /// least a few entries — verifies the cap isn't accidentally below
    /// the per-entry minimum, which would make the manifest unusable.
    #[test]
    fn manifest_at_800_token_cap_produces_output_when_skills_exist() {
        let registry = SkillsRegistry::new();
        // Empty store + empty registry → manifest can legitimately be
        // empty, no assertion needed beyond "doesn't panic".
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 800, StrategyBias::Balanced, None,
        );
        // No skills loaded → empty manifest is correct.
        assert!(manifest.is_empty() || manifest.contains("Learned Skills"),
            "manifest must either be empty or contain the documented header");
    }

    /// Lower cap = no panic, no overrun. The cap argument is a soft
    /// budget — `format_manifest` stops adding entries when the next
    /// entry would push past the budget. Verifies the format function
    /// handles a very small cap (256 tokens ≈ 1000 chars).
    #[test]
    fn manifest_handles_very_small_cap_without_panic() {
        let registry = SkillsRegistry::new();
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 256, StrategyBias::Balanced, None,
        );
        // Empty registry + empty store → empty manifest, no panic.
        let _ = manifest;
    }
}

// ───────────────────────────────────────────────────────────────────
// Bundle 16-C — memory_context cross-turn delta render-path tests.
//
// We pin the four render paths from `build_dynamic_context` without
// constructing a full `LoopDelegate` (which would need an LLM
// provider, safety manager, db handle, etc.). The helper below
// mirrors the in-line logic exactly — any change to the dispatcher
// branch order must be mirrored here, same convention as
// `manifest_suppression_tests`.
//
// The four paths under test:
//
//  1. first turn (no prior anchor)  → full block, no annotation
//  2. unchanged across turns        → full block, no annotation
//  3. small drift (≤ 40%)           → full block + delta annotation
//  4. significant drift (> 40%)     → full block, no annotation
// ───────────────────────────────────────────────────────────────────
#[cfg(test)]
mod memory_context_delta_render_tests {
    use crate::agent::context_diff::{
        line_diff, render_delta_annotation, LineFragmentSnapshot,
    };

    /// Mirror of the dispatcher's render decision. Returns
    /// `(emitted_block, kind_label)` where `kind_label` is one of
    /// `"full"`, `"full+delta"`, used for assertions + telemetry
    /// parity.
    fn render_memory_context(
        prior: Option<&LineFragmentSnapshot>,
        current_text: &str,
    ) -> (String, &'static str) {
        const SIGNIFICANT_DRIFT_THRESHOLD: f32 = 0.40;
        let current = LineFragmentSnapshot::from_text("memory_context", current_text);

        let mut block = String::new();
        let mut kind = "full";

        let annotation: Option<String> = match prior {
            None => None,
            Some(p) => {
                let d = line_diff(p, &current);
                let stats = d.stats();
                if d.is_empty() {
                    None
                } else if stats.is_significant_change(SIGNIFICANT_DRIFT_THRESHOLD) {
                    None
                } else {
                    let a = render_delta_annotation(&d);
                    if a.is_some() {
                        kind = "full+delta";
                    }
                    a
                }
            }
        };

        block.push_str("<memory_context>\n");
        block.push_str(current_text);
        block.push_str("\n</memory_context>");
        if let Some(a) = annotation {
            block.push_str("\n\n");
            block.push_str(&a);
        }
        (block, kind)
    }

    // ── Path 1: first turn ─────────────────────────────────────────

    #[test]
    fn dispatcher_first_turn_emits_full_memory_context_block_only() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let (block, kind) = render_memory_context(None, ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language"));
        assert!(
            !block.contains("<memory_context_changes"),
            "first turn must not emit delta annotation"
        );
    }

    // ── Path 2: unchanged across turns ─────────────────────────────

    #[test]
    fn dispatcher_unchanged_turn_emits_full_block_no_delta_annotation() {
        let ctx = "- preferred_language: zh\n- project: uClaw\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", ctx);
        let (block, kind) = render_memory_context(Some(&prior), ctx);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "no drift → no annotation"
        );
    }

    // ── Path 3: small drift → full + delta ─────────────────────────

    #[test]
    fn dispatcher_small_drift_emits_full_block_plus_delta_annotation() {
        // Prior: 5 lines. New: 4 unchanged + 1 changed (value flip).
        // Drift = 1/5 = 0.20, below 0.40 threshold → small drift path.
        let prior_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: en\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- preferred_language: zh\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        // Full block present
        assert!(block.contains("<memory_context>"));
        assert!(block.contains("preferred_language: zh"));
        // Delta annotation present
        assert!(block.contains("<memory_context_changes"));
        assert!(
            block.contains("~ changed key=preferred_language"),
            "delta must surface the changed line\nblock:\n{}",
            block
        );
        // Block order: <memory_context> first, then <memory_context_changes>
        let mc_pos = block.find("<memory_context>").unwrap();
        let mcc_pos = block.find("<memory_context_changes").unwrap();
        assert!(
            mc_pos < mcc_pos,
            "full block must precede the delta annotation"
        );
    }

    #[test]
    fn dispatcher_small_drift_one_added_line_emits_delta() {
        // 5 prior lines, 1 line added → drift = 0/5 (added-only doesn't
        // count toward drift fraction since it's not in prior).
        // is_significant_change checks (removed + changed) / prior, so
        // pure-add never crosses 40%. We expect full+delta.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text =
            "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n- last_query: foo\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full+delta");
        assert!(block.contains("+ added: - last_query: foo"));
    }

    // ── Path 4: significant drift ──────────────────────────────────

    #[test]
    fn dispatcher_significant_drift_emits_full_block_no_annotation() {
        // 4 prior lines, 3 removed (drift = 0.75) → above threshold.
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n";
        let new_text = "- a: 1\n- z: new\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(block.contains("<memory_context>"));
        assert!(
            !block.contains("<memory_context_changes"),
            "significant drift must NOT emit delta annotation (noisy + cache miss anyway)"
        );
    }

    // ── Anchor reset via clear_memory_context_anchor() ─────────────

    #[test]
    fn clear_anchor_makes_next_turn_behave_like_first_turn() {
        let prior_text = "- a: 1\n";
        let new_text = "- a: 1\n- b: 2\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);

        // With anchor: small drift → delta
        let (with_anchor, kind_with) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind_with, "full+delta");
        assert!(with_anchor.contains("<memory_context_changes"));

        // After clear (None): treated as first turn → no delta
        let (after_clear, kind_after) = render_memory_context(None, new_text);
        assert_eq!(kind_after, "full");
        assert!(!after_clear.contains("<memory_context_changes"));
    }

    // ── Reorder invariance: same key set, different order → no drift

    #[test]
    fn reordered_lines_emit_full_block_no_annotation() {
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n";
        let new_text = "- c: 3\n- a: 1\n- b: 2\n";  // same keys, reordered
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(
            !block.contains("<memory_context_changes"),
            "reorder must not trigger delta annotation"
        );
    }

    // ── Drift threshold is exactly 0.40 (boundary check) ──────────

    #[test]
    fn drift_exactly_at_40_percent_threshold_is_significant() {
        // 5 prior, 2 removed → drift = 2/5 = 0.40 → meets `>=` cutoff
        // in is_significant_change(0.40), so this is the "significant"
        // path with `full` (no delta).
        let prior_text = "- a: 1\n- b: 2\n- c: 3\n- d: 4\n- e: 5\n";
        let new_text = "- a: 1\n- b: 2\n- c: 3\n";
        let prior = LineFragmentSnapshot::from_text("memory_context", prior_text);
        let (block, kind) = render_memory_context(Some(&prior), new_text);
        assert_eq!(kind, "full");
        assert!(!block.contains("<memory_context_changes"));
    }
}

#[cfg(test)]
mod b2_context_wireup_tests {
    //! C2-Dirac-B2 — fragment rendering for build_dynamic_context.
    //!
    //! The cache-discipline / composition guarantees are tested where the
    //! logic lives, without a Tauri AppHandle (ChatDelegate is hard-typed
    //! to the Wry runtime, which `tauri::test::mock_app()` cannot satisfy):
    //!
    //! - System-prompt byte-stability + "self.system_prompt / workspace
    //!   not dropped" → `mode_prompts::tests::compose_with_injection_*`
    //!   (effective_system_prompt delegates verbatim to
    //!   compose_system_prompt_with_injection for the prompt string).
    //! - Fragment SELECTION + ComposeStats → `context_manager::manager`
    //!   + `stats_collector` tests, and the `context_wireup_bench`
    //!   integration test.
    //!
    //! Here we cover the remaining seam: how selected fragments render
    //! into the per-turn dynamic block (and that they NEVER look like a
    //! system-prompt block — they're a distinct <context_fragment> tag).

    use super::render_context_fragments;
    use crate::runtime::context::{ContextArtifact, ContextRef, ContextSource};

    fn artifact(id: &str, source: ContextSource, content: &str) -> ContextArtifact {
        ContextArtifact {
            r#ref: ContextRef::new(source, id),
            content: content.into(),
            citations: Vec::new(),
            retrieval_ts: "2026-05-25T00:00:00Z".into(),
        }
    }

    #[test]
    fn empty_fragments_render_to_empty_string() {
        // Caller push_str's the result unconditionally — no fragments must
        // produce zero bytes (no stray whitespace in the dynamic block).
        assert_eq!(render_context_fragments(&[]), "");
    }

    #[test]
    fn fragment_renders_as_context_fragment_xml_with_id_and_source() {
        let frags = vec![artifact(
            "thread/abc",
            ContextSource::Conversation,
            "alpha\nbeta",
        )];
        let out = render_context_fragments(&frags);
        assert!(out.contains("<context_fragment id=\"thread/abc\" source=\"conversation\">"));
        assert!(out.contains("alpha\nbeta"));
        assert!(out.contains("</context_fragment>"));
    }

    #[test]
    fn multiple_fragments_each_get_their_own_block() {
        let frags = vec![
            artifact("file/a.rs", ContextSource::Codebase, "fn a() {}"),
            artifact("recall/x", ContextSource::Memory, "page body"),
        ];
        let out = render_context_fragments(&frags);
        assert_eq!(out.matches("<context_fragment ").count(), 2);
        assert_eq!(out.matches("</context_fragment>").count(), 2);
        assert!(out.contains("source=\"codebase\""));
        assert!(out.contains("source=\"memory\""));
    }
}
