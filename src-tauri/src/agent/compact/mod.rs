//! M2-G — Structured compact / fold types.
//!
//! When a turn or sub-task ends, the agent often needs to collapse a
//! large trace of intermediate work into a small, structured artifact
//! that can be re-injected into the next turn's context window.
//!
//! Per ADR §"Context Fabric" the canonical compact form is a
//! **StructuredFold** with 8 fields:
//!
//! | Field | Meaning |
//! |---|---|
//! | `facts` | observed truths with evidence pointers |
//! | `decisions` | choices made + rationale |
//! | `unresolved_questions` | open questions to revisit |
//! | `evidence_refs` | pointers to artifacts that support the above |
//! | `failed_attempts` | what was tried + why it didn't work |
//! | `active_constraints` | constraints still binding in next turn |
//! | `next_actions` | planned next steps |
//! | `rollback_points` | checkpoints we can return to |
//!
//! This module ships the **types only** (M2-G pilot) — serialization,
//! roundtrip tests, and the convenience constructors. The fold
//! **producer** (LLM call + parser that turns a turn trace into a
//! StructuredFold) lands in a follow-up PR, as does the wire-up that
//! replaces `runtime::context_tools::ContextToolSet::fold/compare`
//! Storage("M2-G") stubs.
//!
//! Layout:
//!
//! - [`fold`] — `StructuredFold` + 8 component types + serde roundtrip tests

pub mod baseline;
pub mod fold;
pub mod fold_diff;
pub mod render;
pub mod summarize;

pub use baseline::{load_baseline, upsert_baseline};
pub use fold::{
    ArtifactRef, CheckpointRef, DecisionWithRationale, FactWithEvidence, FailedAttempt,
    StructuredFold,
};
pub use fold_diff::{diff_axis, AxisDelta, AxisItem, FoldDelta};
pub use render::render_fold_delta_block;
pub use summarize::{summarize_to_fold, SummarizeError};

/// Bundle 17-B — Result of the `/compact` placeholder-rendering decision.
///
/// Returned by [`decide_placeholder`] so the call site (currently
/// `tauri_commands.rs`) can split on which path was taken without
/// re-deriving the decision from the rendered text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompactPath {
    /// First compact on this session, OR drift ≥ threshold. Placeholder
    /// is `new_fold.to_markdown()` — full re-render.
    FullRewrite,
    /// Prior baseline exists AND drift in `(0, threshold)`. Placeholder
    /// is `prior_fold.to_markdown() + "\n\n" + delta_block`. `drift`
    /// reports the observed `total_drift` so callers can log it.
    DeltaRendered { drift: usize },
}

/// Bundle 17-B — pure decision function. Given a `prior_fold` (loaded
/// from `agent_fold_baselines`) + the freshly-produced `new_fold` +
/// the configured threshold, render the `/compact` placeholder text
/// and report which path was taken.
///
/// This function is the testable core of the wire-up in
/// `tauri_commands.rs::/compact intercept`. The call site adds only
/// DB I/O (load_baseline / upsert_baseline) around it.
///
/// Threshold semantics:
/// - `drift == 0` → no changes since prior fold; full rewrite (the
///   prior fold is still current, no reason to render a delta block
///   that says "nothing changed").
/// - `0 < drift < threshold` → delta path.
/// - `drift >= threshold` → full rewrite.
///
/// First-compact case (`prior_fold == None`) always falls into
/// `FullRewrite`.
///
/// See spec
/// [`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md`](../../../../docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md) §9.3.
pub fn decide_placeholder(
    prior_fold: Option<&StructuredFold>,
    new_fold: &StructuredFold,
    threshold: u32,
) -> (String, CompactPath) {
    let threshold_usize = threshold as usize;
    match prior_fold {
        Some(prior) => {
            let delta = prior.diff(new_fold);
            let drift = delta.total_drift();
            if drift > 0 && drift < threshold_usize {
                if let Some(block) = render_fold_delta_block(&delta) {
                    let mut s = prior.to_markdown();
                    s.push_str("\n\n");
                    s.push_str(&block);
                    return (s, CompactPath::DeltaRendered { drift });
                }
            }
            (new_fold.to_markdown(), CompactPath::FullRewrite)
        }
        None => (new_fold.to_markdown(), CompactPath::FullRewrite),
    }
}

#[cfg(test)]
mod decide_tests {
    use super::*;
    use crate::agent::compact::fold::{DecisionWithRationale, FactWithEvidence};

    fn fold_with_n_facts(n: usize) -> StructuredFold {
        StructuredFold::default().with_facts(
            (0..n)
                .map(|i| FactWithEvidence {
                    statement: format!("fact-{i}"),
                    evidence: vec![],
                    confidence: None,
                })
                .collect(),
        )
    }

    #[test]
    fn first_compact_takes_full_rewrite() {
        let new = fold_with_n_facts(3);
        let (text, path) = decide_placeholder(None, &new, 5);
        assert_eq!(path, CompactPath::FullRewrite);
        assert!(
            !text.contains("<context_changes_since_last_fold"),
            "FullRewrite must not include delta block"
        );
        assert!(text.contains("fact-0"));
    }

    #[test]
    fn small_drift_takes_delta_path() {
        // prior has 3 facts; new adds 1 fact → drift = 1 < threshold 5.
        let prior = fold_with_n_facts(3);
        let new = fold_with_n_facts(4);
        let (text, path) = decide_placeholder(Some(&prior), &new, 5);
        assert_eq!(path, CompactPath::DeltaRendered { drift: 1 });
        assert!(
            text.contains("<context_changes_since_last_fold"),
            "DeltaRendered must include the changes wrapper"
        );
        // The first 3 facts must appear via prior.to_markdown() stable prefix.
        assert!(text.contains("fact-0"));
        assert!(text.contains("fact-1"));
        assert!(text.contains("fact-2"));
        // The fourth fact must appear inside the delta block, prefixed with [facts].
        assert!(text.contains("+ [facts] fact-3"));
    }

    #[test]
    fn drift_at_threshold_falls_through_to_full() {
        // prior has 0 facts; new has 5 facts (5 added). Threshold = 5 →
        // drift >= threshold → full rewrite.
        let prior = fold_with_n_facts(0);
        let new = fold_with_n_facts(5);
        let (text, path) = decide_placeholder(Some(&prior), &new, 5);
        assert_eq!(path, CompactPath::FullRewrite);
        assert!(!text.contains("<context_changes_since_last_fold"));
    }

    #[test]
    fn drift_above_threshold_full_rewrite() {
        let prior = fold_with_n_facts(0);
        let new = fold_with_n_facts(20);
        let (_text, path) = decide_placeholder(Some(&prior), &new, 5);
        assert_eq!(path, CompactPath::FullRewrite);
    }

    #[test]
    fn zero_drift_full_rewrite_not_delta() {
        // prior == new → drift = 0. Spec §9.3: don't emit a "nothing
        // changed" delta block; just full-rewrite (idempotent).
        let prior = fold_with_n_facts(2);
        let new = fold_with_n_facts(2);
        let (text, path) = decide_placeholder(Some(&prior), &new, 5);
        assert_eq!(path, CompactPath::FullRewrite);
        assert!(!text.contains("<context_changes_since_last_fold"));
    }

    #[test]
    fn changed_axis_routes_to_delta() {
        // Mutate a single decision's rationale — that's a `changed` axis
        // entry, contributing 1 to drift. Threshold 5 → delta path.
        let prior = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Use V52 table".into(),
            rationale: "before".into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }]);
        let new = StructuredFold::default().with_decisions(vec![DecisionWithRationale {
            decision: "Use V52 table".into(),
            rationale: "after".into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }]);
        let (text, path) = decide_placeholder(Some(&prior), &new, 5);
        assert_eq!(path, CompactPath::DeltaRendered { drift: 1 });
        assert!(text.contains("~ [decisions] Use V52 table"));
        assert!(text.contains("prior rationale: before"));
        assert!(text.contains("now   rationale: after"));
    }

    #[test]
    fn delta_path_preserves_prior_fold_byte_prefix() {
        // The whole point of the delta path is that the prior fold's
        // markdown is byte-stable between two consecutive compacts so
        // the prompt-cache breakpoint hits a stable prefix.
        let prior = fold_with_n_facts(3);
        let new = fold_with_n_facts(4);
        let (text, _) = decide_placeholder(Some(&prior), &new, 5);

        let prior_md = prior.to_markdown();
        assert!(
            text.starts_with(&prior_md),
            "delta placeholder must start with prior.to_markdown() byte-for-byte"
        );
    }
}
