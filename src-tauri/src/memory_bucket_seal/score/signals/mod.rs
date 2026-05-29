//! Per-signal computations for the score admission gate.
//!
//! Faithful port of `openhuman::memory::tree::score::signals` minus the
//! `entity_density_score(extracted)` path (extract isn't wired in uClaw yet).

pub mod interaction;
pub mod metadata_weight;
pub mod ops;
pub mod source_weight;
pub mod token_count;
pub mod types;
pub mod unique_words;

pub use ops::{combine, combine_cheap_only, compute_cheap};
pub use types::{ScoreSignals, SignalWeights};
