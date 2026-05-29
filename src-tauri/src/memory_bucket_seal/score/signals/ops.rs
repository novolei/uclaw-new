//! Cross-signal helpers: signal computation entry point and the two
//! weighted-combine variants (full and cheap-only) used by `score_chunk`.

use super::{interaction, metadata_weight, source_weight, token_count, unique_words};
use super::{ScoreSignals, SignalWeights};
use crate::memory_bucket_seal::types::Chunk;

/// Compute the cheap (no-LLM, no-extract) signal bundle for a chunk.
///
/// `entity_density` and `llm_importance` default to 0.0 — they're populated
/// when entity extraction and LLM rating are wired in (post-PR7).
pub fn compute_cheap(chunk: &Chunk) -> ScoreSignals {
    ScoreSignals {
        token_count: token_count::score(chunk.token_count),
        unique_words: unique_words::score(&chunk.content),
        metadata_weight: metadata_weight::score(&chunk.metadata),
        source_weight: source_weight::score(&chunk.metadata),
        interaction: interaction::score(&chunk.metadata),
        entity_density: 0.0,
        llm_importance: 0.0,
    }
}

/// Weighted sum of signals, normalised to `[0.0, 1.0]`.
///
/// When `w.llm_importance == 0.0` (the default) the LLM signal contributes
/// nothing to either the numerator or the denominator — output is identical
/// to pre-LLM Phase 2.
pub fn combine(signals: &ScoreSignals, w: &SignalWeights) -> f32 {
    let total_weight = w.token_count
        + w.unique_words
        + w.metadata_weight
        + w.source_weight
        + w.interaction
        + w.entity_density
        + w.llm_importance;
    if total_weight <= 0.0 {
        return 0.0;
    }
    let weighted = signals.token_count * w.token_count
        + signals.unique_words * w.unique_words
        + signals.metadata_weight * w.metadata_weight
        + signals.source_weight * w.source_weight
        + signals.interaction * w.interaction
        + signals.entity_density * w.entity_density
        + signals.llm_importance * w.llm_importance;
    (weighted / total_weight).clamp(0.0, 1.0)
}

/// Weighted sum **excluding the `llm_importance` signal**.
///
/// Used by the short-circuit logic in `score_chunk`: if the deterministic
/// (cheap-signals-only) total is already firmly above or below the
/// admission band, we skip the LLM call entirely. The LLM signal only
/// participates in the *final* `combine` once it's been computed.
pub fn combine_cheap_only(signals: &ScoreSignals, w: &SignalWeights) -> f32 {
    let total_weight = w.token_count
        + w.unique_words
        + w.metadata_weight
        + w.source_weight
        + w.interaction
        + w.entity_density;
    if total_weight <= 0.0 {
        return 0.0;
    }
    let weighted = signals.token_count * w.token_count
        + signals.unique_words * w.unique_words
        + signals.metadata_weight * w.metadata_weight
        + signals.source_weight * w.source_weight
        + signals.interaction * w.interaction
        + signals.entity_density * w.entity_density;
    (weighted / total_weight).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::types::{Metadata, SourceKind};
    use chrono::Utc;

    fn chunk_with_tags(tags: &[&str], content: &str, token_count: u32) -> Chunk {
        let ts = Utc::now();
        let mut meta = Metadata::point_in_time(SourceKind::Email, "x", "owner", ts);
        meta.tags = tags.iter().map(|s| s.to_string()).collect();
        Chunk {
            id: "test".to_string(),
            content: content.to_string(),
            metadata: meta,
            token_count,
            seq_in_source: 0,
            created_at: ts,
            partial_message: false,
        }
    }

    #[test]
    fn combine_all_zeros_is_zero() {
        let s = ScoreSignals::default();
        assert!(combine(&s, &SignalWeights::default()) < 0.01);
    }

    #[test]
    fn combine_all_ones_is_one() {
        let s = ScoreSignals {
            token_count: 1.0,
            unique_words: 1.0,
            metadata_weight: 1.0,
            source_weight: 1.0,
            interaction: 1.0,
            entity_density: 1.0,
            llm_importance: 0.0, // default weight is 0 → contribution is zero
        };
        assert!((combine(&s, &SignalWeights::default()) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn weights_influence_total() {
        let s = ScoreSignals {
            token_count: 0.0,
            unique_words: 0.0,
            metadata_weight: 0.0,
            source_weight: 0.0,
            interaction: 1.0,
            entity_density: 0.0,
            llm_importance: 0.0,
        };
        let total = combine(&s, &SignalWeights::default());
        // interaction weight 3.0 / total weight 9.0 = 1/3
        assert!((total - (3.0 / 9.0)).abs() < 1e-6);
    }

    #[test]
    fn compute_cheap_zeros_entity_and_llm() {
        let chunk = chunk_with_tags(&["reply"], "Some substantive text about Phoenix.", 50);
        let s = compute_cheap(&chunk);
        assert_eq!(s.entity_density, 0.0);
        assert_eq!(s.llm_importance, 0.0);
        // Other signals should be non-zero
        assert!(s.interaction > 0.0);
        assert!(s.metadata_weight > 0.0);
        assert!(s.source_weight > 0.0);
    }
}
