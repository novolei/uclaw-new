//! `CompressionState` enum + `Compactor` state machine.

use serde::{Deserialize, Serialize};

/// Fraction of total token budget at which the compactor flips from
/// `None` to `Approaching`. Per ADR §M2-H L7 spec (75%).
pub const DEFAULT_APPROACHING_THRESHOLD: f32 = 0.75;

/// Fraction at which compaction MUST occur next turn (90%). Above
/// this the agent shouldn't take another LLM round without folding.
pub const DEFAULT_COMPACT_THRESHOLD: f32 = 0.90;

/// Current compaction state of the conversation.
///
/// State is per-session and persisted alongside the agent task so
/// recovery from a crash / restart returns to the same compaction
/// regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompressionState {
    /// No compaction in effect — turns run straight against the full
    /// transcript.
    None,
    /// Token usage has crossed `approaching_threshold`. Next turn the
    /// compactor MUST flip to a compacted state before the LLM call.
    Approaching,
    /// Legacy single-string summary (pre-M2-G compress_context_if_needed).
    /// Kept for backwards compatibility with sessions started before
    /// M2-G; new sessions never enter this state.
    LegacyCompacted,
    /// 8-field `StructuredFold` (M2-G) is the active baseline. The
    /// LLM sees the fold instead of the full transcript.
    StructuredFolded,
    /// Active baseline is a `StructuredFold` PLUS turn-by-turn diffs
    /// (M2-D). Used when re-folding would be expensive and the recent
    /// turn deltas are small.
    DiffInjected,
}

/// Why a transition request was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    /// The combination `(from, to)` is not in the legal-transitions
    /// table. Carries the requested edge for diagnostic logging.
    IllegalTransition {
        from: CompressionState,
        to: CompressionState,
    },
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::IllegalTransition { from, to } => {
                write!(f, "illegal compression transition: {from:?} -> {to:?}")
            }
        }
    }
}

impl std::error::Error for TransitionError {}

/// Hint at *why* the compactor advanced state. Surfaces to
/// observability so the M2-J UI can show "compacted at 78% budget"
/// vs. "compacted by user command".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactorTrigger {
    /// Crossed `approaching_threshold` automatically.
    ApproachingThreshold,
    /// Crossed `compact_threshold` automatically.
    CompactThreshold,
    /// User pressed "compact now" / agent decided structurally.
    Manual,
}

/// Per-session state machine. Holds the current state + the
/// threshold config the session was started with.
#[derive(Debug, Clone, PartialEq)]
pub struct Compactor {
    state: CompressionState,
    approaching_threshold: f32,
    compact_threshold: f32,
}

impl Compactor {
    pub fn new() -> Self {
        Self {
            state: CompressionState::None,
            approaching_threshold: DEFAULT_APPROACHING_THRESHOLD,
            compact_threshold: DEFAULT_COMPACT_THRESHOLD,
        }
    }

    pub fn with_thresholds(approaching: f32, compact: f32) -> Self {
        assert!(
            approaching < compact,
            "approaching threshold must be < compact threshold"
        );
        Self {
            state: CompressionState::None,
            approaching_threshold: approaching,
            compact_threshold: compact,
        }
    }

    pub fn state(&self) -> CompressionState {
        self.state
    }

    pub fn approaching_threshold(&self) -> f32 {
        self.approaching_threshold
    }

    pub fn compact_threshold(&self) -> f32 {
        self.compact_threshold
    }

    /// Request a transition. Returns `Ok(())` on success and
    /// `Err(IllegalTransition)` if the edge isn't allowed.
    pub fn transition_to(&mut self, to: CompressionState) -> Result<(), TransitionError> {
        if !is_legal_transition(self.state, to) {
            return Err(TransitionError::IllegalTransition {
                from: self.state,
                to,
            });
        }
        self.state = to;
        Ok(())
    }

    /// Step the state machine based on a token-usage fraction in
    /// [0.0, 1.0]. Returns an optional trigger describing why the
    /// state changed (or `None` if no change). Internal transitions
    /// always use legal edges.
    pub fn observe_token_fraction(&mut self, fraction: f32) -> Option<CompactorTrigger> {
        // From `None`:
        //   fraction >= compact     → caller MUST compact; we hint
        //                              via CompactThreshold but don't
        //                              choose Legacy vs Structured —
        //                              that's a session-level config.
        //   fraction >= approaching → step to Approaching.
        match self.state {
            CompressionState::None => {
                if fraction >= self.compact_threshold {
                    // Still step to Approaching — the caller will
                    // immediately transition_to a compacted variant.
                    self.state = CompressionState::Approaching;
                    Some(CompactorTrigger::CompactThreshold)
                } else if fraction >= self.approaching_threshold {
                    self.state = CompressionState::Approaching;
                    Some(CompactorTrigger::ApproachingThreshold)
                } else {
                    None
                }
            }
            // From `Approaching` or any compacted state, fraction
            // observations don't directly advance state — the caller
            // does so via transition_to once they've actually folded.
            _ => None,
        }
    }
}

impl Default for Compactor {
    fn default() -> Self {
        Self::new()
    }
}

// ── transition table ───────────────────────────────────────────────

/// Legal-transition predicate. Single source of truth.
///
/// Exhaustive `(from, to)` table (25 entries for 5 states × 5 states).
/// Critical invariants are called out in the comments next to the
/// rejecting rows.
pub fn is_legal_transition(from: CompressionState, to: CompressionState) -> bool {
    use CompressionState::*;
    match (from, to) {
        // ── From None ──────────────────────────────────────────────
        (None, None) => true,
        (None, Approaching) => true,
        (None, LegacyCompacted) => true,
        (None, StructuredFolded) => true,
        // No fold baseline yet → diff is impossible.
        (None, DiffInjected) => false,

        // ── From Approaching ───────────────────────────────────────
        (Approaching, None) => true,
        (Approaching, Approaching) => true,
        (Approaching, LegacyCompacted) => true,
        (Approaching, StructuredFolded) => true,
        // No fold baseline yet → diff is impossible.
        (Approaching, DiffInjected) => false,

        // ── From LegacyCompacted ───────────────────────────────────
        (LegacyCompacted, None) => true,
        (LegacyCompacted, Approaching) => true,
        (LegacyCompacted, LegacyCompacted) => true,
        // Modernization: rebuild as a fresh structured fold.
        (LegacyCompacted, StructuredFolded) => true,
        // ADR §M2-H L7 CRITICAL INVARIANT: diff layer needs a
        // StructuredFold baseline to diff against. Going legacy →
        // diff would corrupt the next-turn context.
        (LegacyCompacted, DiffInjected) => false,

        // ── From StructuredFolded ──────────────────────────────────
        (StructuredFolded, None) => true,
        (StructuredFolded, Approaching) => true,
        // Regression away from structured baseline is forbidden —
        // would discard the fold DiffInjected relies on.
        (StructuredFolded, LegacyCompacted) => false,
        (StructuredFolded, StructuredFolded) => true,
        (StructuredFolded, DiffInjected) => true,

        // ── From DiffInjected ──────────────────────────────────────
        (DiffInjected, None) => true,
        (DiffInjected, Approaching) => true,
        // Regression away from structured baseline.
        (DiffInjected, LegacyCompacted) => false,
        // Re-fold to re-anchor when diff overhead grows.
        (DiffInjected, StructuredFolded) => true,
        (DiffInjected, DiffInjected) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Compactor: construction ─────────────────────────────────────

    #[test]
    fn new_starts_in_none_with_default_thresholds() {
        let c = Compactor::new();
        assert_eq!(c.state(), CompressionState::None);
        assert!((c.approaching_threshold() - DEFAULT_APPROACHING_THRESHOLD).abs() < 1e-6);
        assert!((c.compact_threshold() - DEFAULT_COMPACT_THRESHOLD).abs() < 1e-6);
    }

    #[test]
    fn with_thresholds_honors_overrides() {
        let c = Compactor::with_thresholds(0.5, 0.8);
        assert!((c.approaching_threshold() - 0.5).abs() < 1e-6);
    }

    #[test]
    #[should_panic(expected = "approaching threshold must be < compact threshold")]
    fn with_thresholds_panics_on_invalid_ordering() {
        // Approaching >= compact is incoherent.
        let _ = Compactor::with_thresholds(0.9, 0.5);
    }

    // ── observe_token_fraction ──────────────────────────────────────

    #[test]
    fn observe_below_approaching_returns_none() {
        let mut c = Compactor::new();
        assert!(c.observe_token_fraction(0.5).is_none());
        assert_eq!(c.state(), CompressionState::None);
    }

    #[test]
    fn observe_crossing_approaching_threshold_flips() {
        let mut c = Compactor::new();
        let trigger = c.observe_token_fraction(0.76).unwrap();
        assert_eq!(trigger, CompactorTrigger::ApproachingThreshold);
        assert_eq!(c.state(), CompressionState::Approaching);
    }

    #[test]
    fn observe_crossing_compact_threshold_flips_with_compact_trigger() {
        let mut c = Compactor::new();
        let trigger = c.observe_token_fraction(0.95).unwrap();
        assert_eq!(trigger, CompactorTrigger::CompactThreshold);
        assert_eq!(c.state(), CompressionState::Approaching);
    }

    #[test]
    fn observe_does_not_advance_from_already_compacted_states() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::None).unwrap(); // identity
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        let trigger = c.observe_token_fraction(0.99);
        assert!(trigger.is_none());
        // Still in StructuredFolded.
        assert_eq!(c.state(), CompressionState::StructuredFolded);
    }

    // ── transition_to: legal edges ──────────────────────────────────

    #[test]
    fn legal_path_none_to_structured_folded_via_approaching() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::Approaching).unwrap();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        assert_eq!(c.state(), CompressionState::StructuredFolded);
    }

    #[test]
    fn legal_path_structured_to_diff_and_back() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        c.transition_to(CompressionState::DiffInjected).unwrap();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        c.transition_to(CompressionState::DiffInjected).unwrap();
        assert_eq!(c.state(), CompressionState::DiffInjected);
    }

    #[test]
    fn legal_reset_to_none_from_any_state() {
        // For each terminal state, reach it via a legal path then
        // reset to None. None → DiffInjected isn't legal so we route
        // via Approaching → StructuredFolded → DiffInjected.
        let paths: &[&[CompressionState]] = &[
            &[CompressionState::Approaching],
            &[CompressionState::LegacyCompacted],
            &[CompressionState::StructuredFolded],
            &[
                CompressionState::StructuredFolded,
                CompressionState::DiffInjected,
            ],
        ];
        for path in paths {
            let mut c = Compactor::new();
            for step in *path {
                c.transition_to(*step).unwrap();
            }
            c.transition_to(CompressionState::None).unwrap();
            assert_eq!(c.state(), CompressionState::None);
        }
    }

    #[test]
    fn legal_legacy_to_structured_modernization() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::LegacyCompacted).unwrap();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        assert_eq!(c.state(), CompressionState::StructuredFolded);
    }

    #[test]
    fn identity_transition_is_legal() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::None).unwrap();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        assert_eq!(c.state(), CompressionState::StructuredFolded);
    }

    // ── transition_to: illegal edges ────────────────────────────────

    #[test]
    fn illegal_legacy_to_diff_is_rejected() {
        // The critical ADR §M2-H L7 invariant: never skip
        // StructuredFolded between Legacy and Diff.
        let mut c = Compactor::new();
        c.transition_to(CompressionState::LegacyCompacted).unwrap();
        let err = c.transition_to(CompressionState::DiffInjected).unwrap_err();
        match err {
            TransitionError::IllegalTransition { from, to } => {
                assert_eq!(from, CompressionState::LegacyCompacted);
                assert_eq!(to, CompressionState::DiffInjected);
            }
        }
        // State unchanged.
        assert_eq!(c.state(), CompressionState::LegacyCompacted);
    }

    #[test]
    fn illegal_structured_to_legacy_is_rejected() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        let err = c
            .transition_to(CompressionState::LegacyCompacted)
            .unwrap_err();
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
        assert_eq!(c.state(), CompressionState::StructuredFolded);
    }

    #[test]
    fn illegal_approaching_to_diff_is_rejected() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::Approaching).unwrap();
        let err = c.transition_to(CompressionState::DiffInjected).unwrap_err();
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
    }

    #[test]
    fn illegal_diff_to_legacy_is_rejected() {
        let mut c = Compactor::new();
        c.transition_to(CompressionState::StructuredFolded).unwrap();
        c.transition_to(CompressionState::DiffInjected).unwrap();
        let err = c
            .transition_to(CompressionState::LegacyCompacted)
            .unwrap_err();
        assert!(matches!(err, TransitionError::IllegalTransition { .. }));
    }

    // ── serde ──────────────────────────────────────────────────────

    #[test]
    fn state_serde_is_snake_case() {
        let v = serde_json::to_value(CompressionState::StructuredFolded).unwrap();
        assert_eq!(v, serde_json::json!("structured_folded"));
        let back: CompressionState = serde_json::from_value(v).unwrap();
        assert_eq!(back, CompressionState::StructuredFolded);

        let v = serde_json::to_value(CompressionState::DiffInjected).unwrap();
        assert_eq!(v, serde_json::json!("diff_injected"));
    }

    // ── transition_to: error Display ────────────────────────────────

    #[test]
    fn error_display_includes_states() {
        let err = TransitionError::IllegalTransition {
            from: CompressionState::LegacyCompacted,
            to: CompressionState::DiffInjected,
        };
        let s = err.to_string();
        assert!(s.contains("LegacyCompacted"));
        assert!(s.contains("DiffInjected"));
    }
}
