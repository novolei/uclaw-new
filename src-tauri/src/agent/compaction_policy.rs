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
}
