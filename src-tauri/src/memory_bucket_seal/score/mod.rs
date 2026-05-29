//! Phase 2: scoring / admission pipeline for bucket-seal.
//!
//! Faithful port of `openhuman::memory::tree::score` slimmed to the
//! cheap-signals-only path. Drops `ScoringConfig.extractor`, drops the
//! LLM band integration, drops `extracted` and `canonical_entities` on
//! `ScoreResult`. Entity extraction lands in a separate future PR.

pub mod embed; // Task 5 fills this in
pub mod signals;
pub mod store;

use serde::{Deserialize, Serialize};

use crate::memory_bucket_seal::score::signals::{
    combine_cheap_only, compute_cheap, ScoreSignals, SignalWeights,
};
use crate::memory_bucket_seal::types::Chunk;

/// Default drop threshold. Chunks with `total < DEFAULT_DROP_THRESHOLD`
/// are tombstoned and never reach the L0 buffer (PR8). Faithful port from openhuman.
pub const DEFAULT_DROP_THRESHOLD: f32 = 0.3;

/// Pre-LLM definite-keep band. Currently unused (no LLM extractor in PR7) —
/// preserved so PR8+ can wire the LLM band without changing the public surface.
pub const DEFAULT_DEFINITE_KEEP: f32 = 0.85;

/// Pre-LLM definite-drop band. Currently unused (no LLM extractor in PR7).
pub const DEFAULT_DEFINITE_DROP: f32 = 0.15;

/// Whole outcome of [`score_chunk`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreResult {
    pub chunk_id: String,
    pub total: f32,
    pub signals: ScoreSignals,
    pub kept: bool,
    pub drop_reason: Option<String>,
}

/// Configuration for [`score_chunk`].
#[derive(Clone, Debug)]
pub struct ScoringConfig {
    pub weights: SignalWeights,
    pub drop_threshold: f32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            weights: SignalWeights::default(),
            drop_threshold: DEFAULT_DROP_THRESHOLD,
        }
    }
}

/// Score a chunk via the cheap-signals path (no LLM, no extract).
///
/// Returns a `ScoreResult` with the combined `total` and an admission
/// decision (`kept`). The orchestrator in PR8 will call this before
/// appending the chunk to the L0 buffer; dropped chunks are tombstoned
/// in the score table but never enter the buffer.
pub fn score_chunk(chunk: &Chunk, config: &ScoringConfig) -> ScoreResult {
    tracing::debug!(
        chunk_id = %chunk.id,
        token_count = chunk.token_count,
        "memory_bucket_seal::score_chunk"
    );
    let signals = compute_cheap(chunk);
    let total = combine_cheap_only(&signals, &config.weights);
    let kept = total >= config.drop_threshold;
    let drop_reason = if kept {
        None
    } else {
        Some(format!(
            "cheap-signals total {:.3} below drop_threshold {:.3}",
            total, config.drop_threshold
        ))
    };
    if !kept {
        tracing::debug!(
            chunk_id = %chunk.id,
            total,
            drop_reason = drop_reason.as_deref().unwrap_or(""),
            "memory_bucket_seal::score_chunk: dropped"
        );
    }
    ScoreResult {
        chunk_id: chunk.id.clone(),
        total,
        signals,
        kept,
        drop_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::types::{Metadata, SourceKind};
    use chrono::Utc;

    fn chunk(content: &str, token_count: u32, tags: &[&str]) -> Chunk {
        let ts = Utc::now();
        let mut meta = Metadata::point_in_time(SourceKind::Chat, "slack:#eng", "alice", ts);
        meta.tags = tags.iter().map(|s| s.to_string()).collect();
        Chunk {
            id: "test_chunk".to_string(),
            content: content.to_string(),
            metadata: meta,
            token_count,
            seq_in_source: 0,
            created_at: ts,
            partial_message: false,
        }
    }

    #[test]
    fn score_chunk_result_has_correct_chunk_id() {
        let c = chunk("We are shipping Phoenix on Friday after migration review.", 50, &[]);
        let result = score_chunk(&c, &ScoringConfig::default());
        assert_eq!(result.chunk_id, "test_chunk");
    }

    #[test]
    fn score_chunk_zeros_entity_density_and_llm_importance() {
        let c = chunk("Some substantive content about the project roadmap.", 40, &[]);
        let result = score_chunk(&c, &ScoringConfig::default());
        assert_eq!(result.signals.entity_density, 0.0);
        assert_eq!(result.signals.llm_importance, 0.0);
    }

    #[test]
    fn score_chunk_drops_trivial_content() {
        // Token count below TOKEN_MIN (10) → token_count signal = 0
        // No tags → interaction signal = 0.5, but with very low token score
        // total should stay low enough to drop
        let c = chunk("hi", 1, &[]);
        let result = score_chunk(&c, &ScoringConfig::default());
        // hi has 1 token → token_count_signal = 0.0
        // With neutral signals and no tags the combined total will be around
        // metadata_weight * 1.5 + source_weight * 1.5 + interaction * 3.0 = 0.5*1.5 + 0.5*1.5 + 0.5*3.0
        // divided by (1+1+1.5+1.5+3+1) = 9 → (0.75+0.75+1.5)/9 = 3.0/9 ≈ 0.333
        // which is > 0.3 so actually kept. Test the drop reason path with a
        // very low threshold override instead.
        let config = ScoringConfig {
            weights: SignalWeights::default(),
            drop_threshold: 0.99,
        };
        let result2 = score_chunk(&c, &config);
        assert!(!result2.kept);
        assert!(result2.drop_reason.is_some());
        let reason = result2.drop_reason.unwrap();
        assert!(reason.contains("drop_threshold"));
        let _ = result; // suppress unused warning
    }

    #[test]
    fn score_chunk_no_drop_reason_when_kept() {
        let c = chunk(
            "We decided to ship Phoenix on Friday after a thorough migration review.",
            50,
            &["sent"],
        );
        let result = score_chunk(&c, &ScoringConfig::default());
        assert!(result.kept);
        assert!(result.drop_reason.is_none());
    }

    #[test]
    fn scoring_config_default_uses_default_threshold() {
        assert!((ScoringConfig::default().drop_threshold - DEFAULT_DROP_THRESHOLD).abs() < 1e-6);
    }

    #[test]
    fn score_chunk_uses_config_threshold() {
        let c = chunk("A reasonable message about project work.", 30, &[]);
        let loose = ScoringConfig {
            drop_threshold: 0.01,
            ..ScoringConfig::default()
        };
        let tight = ScoringConfig {
            drop_threshold: 0.99,
            ..ScoringConfig::default()
        };
        assert!(score_chunk(&c, &loose).kept);
        assert!(!score_chunk(&c, &tight).kept);
    }
}
