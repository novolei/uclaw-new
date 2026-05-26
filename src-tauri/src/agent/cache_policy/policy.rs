//! `place_breakpoints` — decide where to put up to 4 cache markers
//! across a request's prompt segments.

use serde::{Deserialize, Serialize};

/// Anthropic per-request limit on `cache_control` markers.
pub const MAX_BREAKPOINTS: usize = 4;

/// Logical segment kinds the policy recognizes. Order in
/// [`CacheSegmentKind::CANONICAL_ORDER`] reflects the natural
/// front-to-back layout of a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CacheSegmentKind {
    /// Static system prompt (M2-A baseline).
    Baseline,
    /// Selected skill metadata (M2-H L3).
    SkillMetadata,
    /// Injected context fragments (M2-B).
    ContextFragments,
    /// Conversation message history up to (but not including) the
    /// current user turn.
    Conversation,
}

impl CacheSegmentKind {
    /// Canonical front-to-back order across the request.
    pub const CANONICAL_ORDER: [CacheSegmentKind; 4] = [
        Self::Baseline,
        Self::SkillMetadata,
        Self::ContextFragments,
        Self::Conversation,
    ];

    /// Default priority weight — higher = more cache-valuable.
    /// Per ADR §M2-I: baseline is most stable, conversation is least.
    pub const fn default_weight(self) -> u32 {
        match self {
            Self::Baseline => 100,
            Self::SkillMetadata => 60,
            Self::ContextFragments => 40,
            Self::Conversation => 20,
        }
    }
}

/// One segment's input to the policy. `tokens` is the segment's
/// estimated token cost (used to drop tiny segments — caching a
/// 30-token segment isn't worth a breakpoint slot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CacheSegment {
    pub kind: CacheSegmentKind,
    pub tokens: usize,
}

impl CacheSegment {
    pub fn new(kind: CacheSegmentKind, tokens: usize) -> Self {
        Self { kind, tokens }
    }
}

/// A placed cache breakpoint — tells the wire-up layer "emit a
/// `cache_control: {ephemeral}` marker at the END of this segment".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CacheBreakpoint {
    pub kind: CacheSegmentKind,
    /// Cumulative token offset from the start of the request at
    /// which the marker is placed. Useful for the M2-J UI ("baseline
    /// ends at token 2400").
    pub at_token: usize,
}

/// Policy state — minimum tokens worth marking + per-kind weight
/// overrides. The default is the ADR §M2-I baseline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachePolicy {
    pub min_segment_tokens: usize,
    pub weights: [(CacheSegmentKind, u32); 4],
}

impl CachePolicy {
    /// ADR §M2-I default: skip segments < 1024 tokens; weights per
    /// `default_weight()`.
    pub fn default_policy() -> Self {
        Self {
            min_segment_tokens: 1024,
            weights: [
                (
                    CacheSegmentKind::Baseline,
                    CacheSegmentKind::Baseline.default_weight(),
                ),
                (
                    CacheSegmentKind::SkillMetadata,
                    CacheSegmentKind::SkillMetadata.default_weight(),
                ),
                (
                    CacheSegmentKind::ContextFragments,
                    CacheSegmentKind::ContextFragments.default_weight(),
                ),
                (
                    CacheSegmentKind::Conversation,
                    CacheSegmentKind::Conversation.default_weight(),
                ),
            ],
        }
    }

    pub fn with_min_tokens(mut self, min: usize) -> Self {
        self.min_segment_tokens = min;
        self
    }

    /// Override one segment's weight. Returns Self for chaining.
    pub fn with_weight(mut self, kind: CacheSegmentKind, weight: u32) -> Self {
        for (k, w) in &mut self.weights {
            if *k == kind {
                *w = weight;
                break;
            }
        }
        self
    }

    pub fn weight_for(&self, kind: CacheSegmentKind) -> u32 {
        self.weights
            .iter()
            .find_map(|(k, w)| if *k == kind { Some(*w) } else { None })
            .unwrap_or(0)
    }
}

impl Default for CachePolicy {
    fn default() -> Self {
        Self::default_policy()
    }
}

/// Outcome of [`place_breakpoints`]. Most callers just want the
/// `breakpoints` vec; `stats` are for the M2-J UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PolicyStats {
    pub total_segments: usize,
    pub eligible_segments: usize,
    pub breakpoints_placed: usize,
    /// Segments skipped because their `tokens < min_segment_tokens`.
    pub skipped_for_size: usize,
    /// Segments eligible but cut by the MAX_BREAKPOINTS cap.
    pub skipped_for_slots: usize,
}

/// Decide where to place up to [`MAX_BREAKPOINTS`] cache markers.
///
/// Segments are presented in **canonical front-to-back order**
/// (caller may interleave segments any way; the policy operates on
/// the list it receives).
///
/// Algorithm:
///
/// 1. Filter out segments below `policy.min_segment_tokens`.
/// 2. Sort the remainder by `(weight desc, canonical-order asc)`.
/// 3. Pick the top `MAX_BREAKPOINTS` segments.
/// 4. Place a breakpoint at the END of each picked segment, with the
///    `at_token` cursor advancing as we walk the input.
///
/// The returned `breakpoints` are sorted by `at_token` ascending
/// (front-to-back along the request) so the wire-up layer can emit
/// them in stream order.
pub fn place_breakpoints(
    segments: &[CacheSegment],
    policy: &CachePolicy,
) -> (Vec<CacheBreakpoint>, PolicyStats) {
    let mut stats = PolicyStats {
        total_segments: segments.len(),
        ..Default::default()
    };

    // Build a list of (input_index, segment) for eligible entries.
    let mut eligible: Vec<(usize, &CacheSegment)> = segments
        .iter()
        .enumerate()
        .filter(|(_, s)| {
            if s.tokens < policy.min_segment_tokens {
                false
            } else {
                true
            }
        })
        .collect();
    stats.eligible_segments = eligible.len();
    stats.skipped_for_size = segments.len() - eligible.len();

    // Rank: weight desc, ties broken by canonical-front-to-back
    // (i.e. input index ascending).
    eligible.sort_by(|a, b| {
        policy
            .weight_for(b.1.kind)
            .cmp(&policy.weight_for(a.1.kind))
            .then_with(|| a.0.cmp(&b.0))
    });

    // Cap at MAX_BREAKPOINTS.
    if eligible.len() > MAX_BREAKPOINTS {
        stats.skipped_for_slots = eligible.len() - MAX_BREAKPOINTS;
        eligible.truncate(MAX_BREAKPOINTS);
    }

    // Pick set is the indices we keep; we still want a forward walk
    // for the at_token cursor.
    let mut keep_indices: std::collections::HashSet<usize> =
        eligible.iter().map(|(i, _)| *i).collect();

    let mut breakpoints = Vec::with_capacity(keep_indices.len());
    let mut cursor: usize = 0;
    for (i, seg) in segments.iter().enumerate() {
        cursor += seg.tokens;
        if keep_indices.remove(&i) {
            breakpoints.push(CacheBreakpoint {
                kind: seg.kind,
                at_token: cursor,
            });
        }
    }
    stats.breakpoints_placed = breakpoints.len();
    (breakpoints, stats)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(k: CacheSegmentKind, tokens: usize) -> CacheSegment {
        CacheSegment::new(k, tokens)
    }

    // ── CacheSegmentKind ────────────────────────────────────────────

    #[test]
    fn canonical_order_has_all_four_kinds() {
        let kinds = CacheSegmentKind::CANONICAL_ORDER;
        assert_eq!(kinds.len(), 4);
        let mut s = kinds.to_vec();
        s.sort_by_key(|k| format!("{k:?}"));
        s.dedup();
        assert_eq!(s.len(), 4);
    }

    #[test]
    fn weights_monotonic_in_canonical_order() {
        // Baseline > SkillMetadata > ContextFragments > Conversation.
        let order = CacheSegmentKind::CANONICAL_ORDER;
        for w in order.windows(2) {
            assert!(
                w[0].default_weight() > w[1].default_weight(),
                "{:?} should outweigh {:?}",
                w[0],
                w[1]
            );
        }
    }

    #[test]
    fn segment_kind_serde_snake_case() {
        let v = serde_json::to_value(CacheSegmentKind::ContextFragments).unwrap();
        assert_eq!(v, serde_json::json!("context_fragments"));
    }

    // ── CachePolicy ─────────────────────────────────────────────────

    #[test]
    fn default_policy_min_tokens_1024() {
        let p = CachePolicy::default_policy();
        assert_eq!(p.min_segment_tokens, 1024);
        // Default weights match the per-kind helper.
        for k in CacheSegmentKind::CANONICAL_ORDER {
            assert_eq!(p.weight_for(k), k.default_weight());
        }
    }

    #[test]
    fn with_weight_overrides() {
        let p = CachePolicy::default_policy().with_weight(CacheSegmentKind::Conversation, 999);
        assert_eq!(p.weight_for(CacheSegmentKind::Conversation), 999);
        // Other weights unaffected.
        assert_eq!(p.weight_for(CacheSegmentKind::Baseline), 100);
    }

    #[test]
    fn with_min_tokens_overrides() {
        let p = CachePolicy::default_policy().with_min_tokens(2048);
        assert_eq!(p.min_segment_tokens, 2048);
    }

    // ── place_breakpoints ──────────────────────────────────────────

    #[test]
    fn places_zero_when_all_below_min() {
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 500),
            seg(CacheSegmentKind::Conversation, 500),
        ];
        let (bps, stats) = place_breakpoints(&segments, &CachePolicy::default());
        assert!(bps.is_empty());
        assert_eq!(stats.skipped_for_size, 2);
    }

    #[test]
    fn places_breakpoints_for_all_eligible_within_cap() {
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 3000),
            seg(CacheSegmentKind::SkillMetadata, 2000),
            seg(CacheSegmentKind::ContextFragments, 4000),
            seg(CacheSegmentKind::Conversation, 5000),
        ];
        let (bps, stats) = place_breakpoints(&segments, &CachePolicy::default());
        // All 4 fit under MAX_BREAKPOINTS = 4.
        assert_eq!(bps.len(), 4);
        assert_eq!(stats.skipped_for_slots, 0);
        // at_token cumulative: 3000, 5000, 9000, 14000.
        assert_eq!(bps[0].at_token, 3000);
        assert_eq!(bps[1].at_token, 5000);
        assert_eq!(bps[2].at_token, 9000);
        assert_eq!(bps[3].at_token, 14000);
    }

    #[test]
    fn breakpoints_returned_in_at_token_order() {
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 2000),
            seg(CacheSegmentKind::SkillMetadata, 1500),
            seg(CacheSegmentKind::ContextFragments, 1200),
            seg(CacheSegmentKind::Conversation, 1100),
        ];
        let (bps, _) = place_breakpoints(&segments, &CachePolicy::default());
        let cursors: Vec<usize> = bps.iter().map(|b| b.at_token).collect();
        let mut sorted = cursors.clone();
        sorted.sort();
        assert_eq!(
            cursors, sorted,
            "breakpoints must be in front-to-back order"
        );
    }

    #[test]
    fn drops_low_weight_segments_when_over_cap() {
        // 5 eligible segments — Baseline appears twice via two slices
        // (e.g. baseline + secondary baseline). Conversation should
        // be dropped first.
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 2000),
            seg(CacheSegmentKind::SkillMetadata, 2000),
            seg(CacheSegmentKind::ContextFragments, 2000),
            seg(CacheSegmentKind::Conversation, 2000),
            seg(CacheSegmentKind::Conversation, 2000),
        ];
        let (bps, stats) = place_breakpoints(&segments, &CachePolicy::default());
        assert_eq!(bps.len(), MAX_BREAKPOINTS);
        assert_eq!(stats.skipped_for_slots, 1);
        // The DROPPED segment should be the second Conversation
        // (lowest weight, latest in order). So among kept breakpoints,
        // count Conversation entries — should be exactly 1.
        let convo_count = bps
            .iter()
            .filter(|b| b.kind == CacheSegmentKind::Conversation)
            .count();
        assert_eq!(convo_count, 1);
    }

    #[test]
    fn skips_undersized_then_caps() {
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 500), // skipped (too small)
            seg(CacheSegmentKind::SkillMetadata, 2000),
            seg(CacheSegmentKind::ContextFragments, 2000),
            seg(CacheSegmentKind::Conversation, 2000),
            seg(CacheSegmentKind::Conversation, 2000),
        ];
        let (bps, stats) = place_breakpoints(&segments, &CachePolicy::default());
        assert_eq!(stats.skipped_for_size, 1);
        // 4 eligible, fits in 4 slots.
        assert_eq!(bps.len(), 4);
        assert_eq!(stats.skipped_for_slots, 0);
    }

    #[test]
    fn stats_record_totals() {
        let segments = vec![
            seg(CacheSegmentKind::Baseline, 2000),
            seg(CacheSegmentKind::SkillMetadata, 500), // skip
            seg(CacheSegmentKind::Conversation, 2000),
        ];
        let (bps, stats) = place_breakpoints(&segments, &CachePolicy::default());
        assert_eq!(stats.total_segments, 3);
        assert_eq!(stats.eligible_segments, 2);
        assert_eq!(stats.skipped_for_size, 1);
        assert_eq!(stats.breakpoints_placed, 2);
        assert_eq!(bps.len(), 2);
    }

    #[test]
    fn breakpoint_serde_camel_case() {
        let b = CacheBreakpoint {
            kind: CacheSegmentKind::Baseline,
            at_token: 1500,
        };
        let v = serde_json::to_value(&b).unwrap();
        assert_eq!(v["kind"], "baseline");
        assert_eq!(v["atToken"], 1500);
    }
}
