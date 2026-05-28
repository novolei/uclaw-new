//! `TokenBudgetSnapshot` — per-turn token accounting payload for the UI.

use serde::{Deserialize, Serialize};

/// One row of the "where did the tokens go" breakdown.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextSegmentBreakdown {
    pub baseline: u64,
    pub skill_metadata: u64,
    pub context_fragments: u64,
    pub conversation: u64,
    pub current_user_message: u64,
}

impl ContextSegmentBreakdown {
    pub fn total(&self) -> u64 {
        self.baseline
            + self.skill_metadata
            + self.context_fragments
            + self.conversation
            + self.current_user_message
    }
}

/// Aggregated savings from the 7-layer Token Defense system (M2-H).
/// Each field counts tokens that **would have been** sent without
/// the defense — i.e. they're savings, not actual usage.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefenseSavings {
    /// L1 TruncationPolicy: bytes dropped from tool outputs.
    pub l1_truncated_bytes: u64,
    /// L2 ToolExposure: tools hidden this turn.
    pub l2_tools_hidden: u32,
    /// L2 normalize_tool_schema: schema bytes removed.
    pub l2_schema_bytes_removed: u64,
    /// L5 image_policy: image blocks stripped (combined Anthropic+OpenAI).
    pub l5_images_stripped: u32,
    /// L6 call_audit: orphan tool calls synthesized.
    pub l6_orphans_synthesized: u32,
    /// L7 compression: current CompressionState string for display.
    /// Empty = not compressed yet.
    pub l7_compression_state: String,
}

/// Compact summary of M2-I cache breakpoints for the UI.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakpointSummary {
    /// Number of cache_control markers placed this turn (0-4).
    pub placed: u32,
    /// Token offsets where markers were placed (ascending order).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub at_tokens: Vec<u64>,
}

/// The flat payload the UI subscribes to. Built once per turn.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenBudgetSnapshot {
    /// Owning task / session id.
    pub task_id: String,
    /// RFC 3339 timestamp at which the snapshot was assembled.
    pub captured_at: String,
    /// Turn number within the task (1-indexed).
    pub turn: u32,
    /// Provider + model that processed this turn (for cache hit rate
    /// attribution).
    pub provider: String,
    pub model: String,
    /// Per-segment input breakdown.
    pub input_breakdown: ContextSegmentBreakdown,
    /// Actual usage as reported by the provider (post-call).
    /// Defaults to zeros until PostLlmCall fires.
    pub provider_input_tokens: u64,
    pub provider_output_tokens: u64,
    pub provider_cached_tokens: u64,
    pub provider_reasoning_tokens: u64,
    /// Per-call cost in micro-USD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_usd_micros: Option<u64>,
    /// Aggregated L1-L7 defense savings.
    pub defense_savings: DefenseSavings,
    /// M2-I cache breakpoint summary.
    pub cache_breakpoints: BreakpointSummary,
    /// Per-turn budget cap (provider context window). 0 = unknown.
    pub turn_budget: u64,
}

impl TokenBudgetSnapshot {
    /// Create a fresh snapshot with task/turn metadata. Other fields
    /// default to zero — callers populate them as L-layers run.
    pub fn new(
        task_id: impl Into<String>,
        turn: u32,
        provider: impl Into<String>,
        model: impl Into<String>,
        captured_at: impl Into<String>,
    ) -> Self {
        Self {
            task_id: task_id.into(),
            captured_at: captured_at.into(),
            turn,
            provider: provider.into(),
            model: model.into(),
            ..Default::default()
        }
    }

    /// Total input tokens computed from the breakdown.
    pub fn breakdown_total(&self) -> u64 {
        self.input_breakdown.total()
    }

    /// `true` if provider has reported usage. Used by the UI to
    /// distinguish "pre-call estimate" from "post-call actual".
    pub fn has_provider_usage(&self) -> bool {
        self.provider_input_tokens > 0 || self.provider_output_tokens > 0
    }

    /// `true` if the cache provided any benefit this turn.
    pub fn cache_was_used(&self) -> bool {
        self.provider_cached_tokens > 0
    }

    /// Cache hit ratio in [0.0, 1.0]. `None` when provider hasn't
    /// reported usage yet.
    pub fn cache_hit_ratio(&self) -> Option<f64> {
        if !self.has_provider_usage() {
            return None;
        }
        if self.provider_input_tokens == 0 {
            return Some(0.0);
        }
        Some(self.provider_cached_tokens as f64 / self.provider_input_tokens as f64)
    }

    /// Fraction of the turn budget consumed by INPUT tokens.
    /// `None` when `turn_budget == 0`.
    pub fn budget_usage_fraction(&self) -> Option<f64> {
        if self.turn_budget == 0 {
            return None;
        }
        Some(self.provider_input_tokens as f64 / self.turn_budget as f64)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh() -> TokenBudgetSnapshot {
        TokenBudgetSnapshot::new(
            "t1",
            1,
            "anthropic",
            "claude-sonnet-4-5",
            "2026-05-21T12:00:00Z",
        )
    }

    // ── ContextSegmentBreakdown ─────────────────────────────────────

    #[test]
    fn breakdown_total_sums_five_segments() {
        let b = ContextSegmentBreakdown {
            baseline: 100,
            skill_metadata: 200,
            context_fragments: 300,
            conversation: 400,
            current_user_message: 500,
        };
        assert_eq!(b.total(), 1500);
    }

    #[test]
    fn breakdown_default_zero_total() {
        assert_eq!(ContextSegmentBreakdown::default().total(), 0);
    }

    // ── TokenBudgetSnapshot construction ────────────────────────────

    #[test]
    fn new_sets_metadata_zeros_rest() {
        let s = fresh();
        assert_eq!(s.task_id, "t1");
        assert_eq!(s.turn, 1);
        assert_eq!(s.provider, "anthropic");
        assert_eq!(s.model, "claude-sonnet-4-5");
        assert_eq!(s.captured_at, "2026-05-21T12:00:00Z");
        assert_eq!(s.breakdown_total(), 0);
        assert!(!s.has_provider_usage());
        assert!(!s.cache_was_used());
        assert!(s.cache_hit_ratio().is_none());
        assert!(s.budget_usage_fraction().is_none());
    }

    // ── has_provider_usage / cache_was_used ─────────────────────────

    #[test]
    fn has_provider_usage_true_when_input_or_output_present() {
        let mut s = fresh();
        s.provider_input_tokens = 100;
        assert!(s.has_provider_usage());
        let mut s = fresh();
        s.provider_output_tokens = 50;
        assert!(s.has_provider_usage());
    }

    #[test]
    fn cache_was_used_only_when_cached_tokens_positive() {
        let mut s = fresh();
        assert!(!s.cache_was_used());
        s.provider_cached_tokens = 10;
        assert!(s.cache_was_used());
    }

    // ── cache_hit_ratio ────────────────────────────────────────────

    #[test]
    fn cache_hit_ratio_none_when_no_usage() {
        let s = fresh();
        assert!(s.cache_hit_ratio().is_none());
    }

    #[test]
    fn cache_hit_ratio_computes_fraction() {
        let mut s = fresh();
        s.provider_input_tokens = 1000;
        s.provider_cached_tokens = 250;
        let r = s.cache_hit_ratio().unwrap();
        assert!((r - 0.25).abs() < 1e-9);
    }

    #[test]
    fn cache_hit_ratio_returns_zero_when_input_zero_but_output_present() {
        let mut s = fresh();
        s.provider_output_tokens = 100;
        let r = s.cache_hit_ratio().unwrap();
        assert_eq!(r, 0.0);
    }

    // ── budget_usage_fraction ─────────────────────────────────────

    #[test]
    fn budget_usage_fraction_requires_turn_budget() {
        let mut s = fresh();
        s.provider_input_tokens = 1000;
        assert!(s.budget_usage_fraction().is_none());
        s.turn_budget = 8000;
        assert!((s.budget_usage_fraction().unwrap() - 0.125).abs() < 1e-9);
    }

    // ── DefenseSavings + BreakpointSummary defaults ───────────────

    #[test]
    fn defense_savings_default_all_zero_empty_state() {
        let d = DefenseSavings::default();
        assert_eq!(d.l1_truncated_bytes, 0);
        assert_eq!(d.l2_tools_hidden, 0);
        assert_eq!(d.l5_images_stripped, 0);
        assert!(d.l7_compression_state.is_empty());
    }

    #[test]
    fn breakpoint_summary_default_zero_empty() {
        let b = BreakpointSummary::default();
        assert_eq!(b.placed, 0);
        assert!(b.at_tokens.is_empty());
    }

    // ── serde round-trip ───────────────────────────────────────────

    #[test]
    fn snapshot_serde_camel_case_roundtrip() {
        let mut s = fresh();
        s.input_breakdown.baseline = 1500;
        s.input_breakdown.skill_metadata = 800;
        s.provider_input_tokens = 5000;
        s.provider_cached_tokens = 3000;
        s.turn_budget = 200_000;
        s.defense_savings.l1_truncated_bytes = 12_345;
        s.defense_savings.l6_orphans_synthesized = 1;
        s.defense_savings.l7_compression_state = "structured_folded".into();
        s.cache_breakpoints.placed = 3;
        s.cache_breakpoints.at_tokens = vec![1500, 2300, 3500];

        let json = serde_json::to_string(&s).unwrap();
        // Spot-check camelCase keys.
        assert!(json.contains("\"taskId\":"));
        assert!(json.contains("\"providerInputTokens\":5000"));
        assert!(json.contains("\"providerCachedTokens\":3000"));
        assert!(json.contains("\"l1TruncatedBytes\":12345"));
        assert!(json.contains("\"l7CompressionState\":\"structured_folded\""));
        assert!(json.contains("\"cacheBreakpoints\":"));
        // Roundtrip preserves equality.
        let back: TokenBudgetSnapshot = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn cost_usd_micros_skipped_when_none() {
        let s = fresh();
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("costUsdMicros"));
    }

    #[test]
    fn cost_usd_micros_included_when_some() {
        let mut s = fresh();
        s.cost_usd_micros = Some(12_345);
        let json = serde_json::to_string(&s).unwrap();
        assert!(json.contains("\"costUsdMicros\":12345"));
    }
}
