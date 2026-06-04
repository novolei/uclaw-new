/// The default memory space used by background pipelines that have no per-session space.
pub const DEFAULT_SPACE_ID: &str = "default";

pub mod models;
// Step 2d — pure markdown frontmatter utils relocated from gbrain::browse.
pub mod page_markdown;
// Step 2d — proactive chat-turn extractor relocated from gbrain::chat_extractor.
pub mod chat_extractor;
pub mod store;
pub mod search;
pub mod recall;
pub mod reflection;
pub mod environment;
pub mod auto_classify;
// Memory OS Foundation Phase 1 — per-entity compiled-truth + timeline schema.
pub mod entity_page;
// Memory OS Foundation Phase 2 — zero-LLM reference extractor + link-type inferrer.
pub mod auto_link;
// Memory OS Foundation Phase 3 — AI Wiki synthesis (index.md SQL-only, overview.md LLM-driven).
pub mod wiki_synth;
// Memory OS Foundation Phase 6a — shared LLM adapter for wiki/lint/entity-synth scenarios.
pub mod memory_os_llm;
// Memory OS Foundation Phase 7 — markdown frontmatter + disk-mirror sync state.
pub mod brain_io;
// Memory OS Foundation Phase 7.4 — opt-in fs watcher over the brain dir.
pub mod brain_watcher;
// Memory OS Sprint 1.7 — PROFILE.md managed-block parser/renderer.
pub mod profile_md;
// Memory OS L3 §4.12.1 (RETAINED per ADR 2026-05-20 §8) — Importance-Aware
// Decay: Ebbinghaus forgetting + importance weighting. Writes to V44's
// `memory_importance_scores` table. Algorithm-only in this PR; scheduler
// wiring deferred to a follow-up.
pub mod importance_decay;
// Memory OS L3 §3.2.1 (RETAINED per ADR 2026-05-20 §8) — global
// `timeline_events` write API. Wired into EntityPage create (Q2a) +
// other event sources in follow-up PRs.
pub mod timeline_events;
// Memory OS L3 §3.3 (RETAINED per ADR 2026-05-20 §8) — Temporal
// Query Classifier. V1 keyword + regex detection; future PR adds
// LLM disambiguation fallback.
pub mod temporal_classifier;
// Memory OS L3 §4.12.3 (RETAINED per ADR 2026-05-20 §8) — Spaced
// Repetition (Anki SM-2 ladder) for verified high-importance
// EntityPages. V45 schema + state-machine + tests in this PR;
// scheduler hook + LLM re-check in a follow-up.
pub mod spaced_repetition;
// Memory OS L3 §4.12.4 (RETAINED per ADR 2026-05-20 §8) — Concept
// Drift Detection. V46 schema + Levenshtein-based pure algorithm +
// tests; scheduler + LLM triage in a follow-up.
pub mod drift_detection;
// Memory OS L3 §4.12.5 (RETAINED per ADR 2026-05-20 §8) — Cross-
// Source Triangulation: confidence boost when ≥2 sources agree.
// V47 schema + pure boost formula + DB record/summarize helpers
// in this PR; scheduler + LLM "does source X support claim Y?"
// classifier in a follow-up.
pub mod triangulation;
// Step 3b-3 — native MemoryExtractor (single JSON-mode LLM call,
// replaces memU's `memorize` pipeline). Output feeds reflection layer.
pub mod extractor;
// openhuman-E — directed, weighted tool-transition graph. Replaces
// boolean-undirected co_used edge façade. V58 schema; aggregated by
// proactive job; read by suggest_tool_chain.
pub mod tool_transitions;
// openhuman-G — shared text-dedup primitives (normalize, bigram-Jaccard,
// CJK-aware tokenization). Used by skill_parser (skill dedup) and reflection
// (fact dedup). Pure functions, no DB.
pub mod text_dedup;

// ──────────────────────────────────────────────────────────────────────────
// memory_graph write guard — RETIRED (2026-06-02, Step 3b-3).
//
// Originally a runtime freeze guard (Phase 0.5-T7): per the 2026-05-20
// gbrain-primary-freeze ADR §11.2, the `memory_graph` write surface was frozen
// in favor of gbrain. That policy is superseded — see below.
//
// This module was previously FROZEN
// (2026-05-20 north-star ADR §11.2 + `2026-05-20-gbrain-primary-freeze-l2-cognitive.md`):
// writes were meant to route through gbrain. The **2026-06-01 two-layer terminal
// ADR supersedes that** (`docs/adr/2026-06-01-memory-two-layer-terminal-state.md`,
// "P4 → redefined"): memory_graph is the RETAINED single sanctioned rich-structure
// store, and gbrain is being retired instead. So writes here are legitimate
// (reflection's per-turn population, ProactiveService extraction, etc.).
//
// This guard is now a **no-op**, kept only so its ~23 call sites compile unchanged
// (a later sweep may remove the calls). The companion commit-time check
// (`scripts/git-hooks/checks/check-memory-graph-freeze.sh`) was removed in this
// slice; the per-user `.claude/hooks/check-memory-graph.sh` (gitignored) should be
// removed locally too. The surviving principle — "do not spawn new divergent
// graph/entity stores; route rich data through memory_graph behind the adapter
// seam" — is an architectural norm in that ADR, not a runtime guard.
pub(crate) fn enforce_freeze(_call_site: &'static str) {
    // no-op: the memory_graph freeze was lifted by the 2026-06-01 two-layer ADR.
}

#[cfg(test)]
mod freeze_tests {
    /// The retired guard is a no-op — callable, never panics.
    #[test]
    fn enforce_freeze_is_a_noop() {
        super::enforce_freeze("test::callsite");
    }
}
