// SPDX-License-Identifier: Apache-2.0
//! Pure policy functions for context compaction (threshold selection).

/// Lower bound for the user-configured compaction threshold (tokens).
pub const MIN_COMPACTION_THRESHOLD: usize = 32_000;
/// Tokens reserved for model output.
pub const OUTPUT_RESERVE: usize = 16_384;
/// Extra safety margin below the model window.
pub const SAFETY_MARGIN: usize = 8_192;

/// Effective compaction threshold = clamp(configured, 32K, window - reserve - margin).
/// The 32K floor only constrains the configured value; a small model window cap wins.
/// `model_window == 0` means unknown: the (floored) configured value is used.
pub fn effective_compaction_threshold(configured: usize, model_window: usize) -> usize {
    let floored = configured.max(MIN_COMPACTION_THRESHOLD);
    if model_window == 0 {
        return floored;
    }
    let cap = model_window.saturating_sub(OUTPUT_RESERVE + SAFETY_MARGIN);
    floored.min(cap)
}

/// What the loop should do with the current request size (transcript mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionDecision {
    /// Under the threshold: send as-is.
    None,
    /// At/over the threshold but below the model's hard limit: compact.
    Compact,
    /// At/over the hard limit (`model_window - OUTPUT_RESERVE`): truncate.
    HardTruncate,
}

/// Absolute-threshold compaction decision.
/// `< threshold` → None; `>= threshold` and `< hard_limit` → Compact;
/// `>= hard_limit` → HardTruncate, where `hard_limit = model_window - OUTPUT_RESERVE`.
/// `model_window == 0` means unknown: there is no hard limit.
pub fn decide_compaction(
    estimated_request_tokens: usize,
    threshold: usize,
    model_window: usize,
) -> CompactionDecision {
    if model_window > 0 && estimated_request_tokens >= model_window.saturating_sub(OUTPUT_RESERVE) {
        return CompactionDecision::HardTruncate;
    }
    if estimated_request_tokens >= threshold {
        CompactionDecision::Compact
    } else {
        CompactionDecision::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_threshold_uses_configured_when_window_large() {
        assert_eq!(effective_compaction_threshold(128_000, 1_000_000), 128_000);
    }

    #[test]
    fn effective_threshold_clamps_to_small_window() {
        let t = effective_compaction_threshold(128_000, 32_768);
        assert_eq!(t, 32_768 - 16_384 - 8_192);
        assert!(t <= 32_768);
    }

    #[test]
    fn effective_threshold_enforces_minimum_on_config() {
        assert_eq!(effective_compaction_threshold(10_000, 1_000_000), 32_000);
    }

    #[test]
    fn effective_threshold_unknown_window_uses_configured() {
        assert_eq!(effective_compaction_threshold(128_000, 0), 128_000);
    }

    #[test]
    fn decision_none_below_threshold() {
        assert_eq!(decide_compaction(127_999, 128_000, 1_000_000), CompactionDecision::None);
        assert_eq!(decide_compaction(0, 128_000, 0), CompactionDecision::None);
    }

    #[test]
    fn decision_compacts_at_threshold() {
        assert_eq!(decide_compaction(128_000, 128_000, 1_000_000), CompactionDecision::Compact);
        assert_eq!(decide_compaction(500_000, 128_000, 1_000_000), CompactionDecision::Compact);
        // Unknown window (0) = no hard ceiling: never HardTruncate.
        assert_eq!(decide_compaction(10_000_000, 128_000, 0), CompactionDecision::Compact);
    }

    #[test]
    fn decision_hard_truncates_past_window() {
        let window = 200_000;
        let hard_limit = window - OUTPUT_RESERVE;
        assert_eq!(decide_compaction(hard_limit - 1, 128_000, window), CompactionDecision::Compact);
        assert_eq!(decide_compaction(hard_limit, 128_000, window), CompactionDecision::HardTruncate);
        assert_eq!(decide_compaction(1_000_000, 128_000, window), CompactionDecision::HardTruncate);
    }

    #[test]
    fn decision_compacts_when_window_shrinks() {
        // The same estimate is fine in a 1M window but must act in a 32K one.
        let estimate = 20_000;
        let big = effective_compaction_threshold(128_000, 1_000_000);
        assert_eq!(decide_compaction(estimate, big, 1_000_000), CompactionDecision::None);
        let small = effective_compaction_threshold(128_000, 32_768);
        assert_eq!(decide_compaction(estimate, small, 32_768), CompactionDecision::HardTruncate);
        assert_eq!(decide_compaction(small, small, 32_768), CompactionDecision::Compact);
    }
}
