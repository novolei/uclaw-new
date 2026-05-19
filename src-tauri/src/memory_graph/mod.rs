pub mod models;
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
