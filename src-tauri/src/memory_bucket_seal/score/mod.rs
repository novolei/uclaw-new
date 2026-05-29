//! Phase 2: scoring / admission pipeline for bucket-seal.
//!
//! Faithful port of `openhuman::memory::tree::score` slimmed to the
//! cheap-signals-only path. Drops `ScoringConfig.extractor`, drops the
//! LLM band integration, drops `extracted` and `canonical_entities` on
//! `ScoreResult`. Entity extraction lands in a separate future PR.

pub mod signals;
pub mod store;
// pub mod embed;  -- Task 5
