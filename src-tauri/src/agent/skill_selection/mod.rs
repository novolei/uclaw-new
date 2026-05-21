//! M2-H L3 — Per-turn skill selection (top-K under token budget).
//!
//! uClaw plus connected plugins can ship hundreds of skills. Stuffing
//! every skill's metadata into the system prompt every turn blows the
//! context budget — most turns only need 0–5 skills, so we should
//! pick the relevant subset.
//!
//! L3 ships a deterministic, dependency-free selector:
//!
//! 1. Score each candidate against the per-turn query (topic match +
//!    description keyword hits in the recent-text window).
//! 2. Sort descending by score, ties broken by candidate id (stable).
//! 3. Walk the sorted list, picking entries until either `k` picks
//!    are made or `budget_tokens` would be exceeded.
//!
//! This is intentionally **not semantic retrieval** — that lives in
//! the GEP / memory layer. L3 is the cheap, predictable, on-by-default
//! gate. M2-H L4 (diff updates) and the GEP retriever both build on
//! top of L3's output set.
//!
//! Layout:
//!
//! - [`select`] — `SkillCandidate`, `SelectionQuery`, `select_top_k`,
//!   `SelectionStats`

pub mod select;

pub use select::{
    select_top_k, SelectionQuery, SelectionStats, SkillCandidate, DEFAULT_TOP_K,
    DEFAULT_METADATA_BUDGET_TOKENS,
};
