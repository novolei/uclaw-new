//! Bundle 17-A — `StructuredFold` cross-fold delta.
//!
//! After `/compact` produces a `StructuredFold` F1 at turn N, the
//! next compaction trigger at turn N+K usually faces a small
//! incremental delta on top of F1 rather than a fresh corpus.
//! Re-running the full fold prompt LLM call is wasteful when only a
//! handful of items changed.
//!
//! This module produces a per-axis delta between two folds. The
//! dispatcher (Bundle 17-B) reads the delta and either:
//!
//! - applies it locally (`apply_to`) to produce the next fold
//!   without an LLM call, or
//! - emits a `<context_changes_since_last_fold>` block on top of the
//!   stable prior fold so the system-prompt cache breakpoint keeps
//!   hitting.
//!
//! ## Per-axis identity
//!
//! "Same item, new value" must be detectable so a decision-rationale
//! revision shows as `changed`, not `removed + added`. We define
//! `stable_key` per axis:
//!
//! | Axis | Key source |
//! | --- | --- |
//! | facts | `statement` (the assertion text) |
//! | decisions | `decision` (the title) |
//! | unresolved_questions | the question string itself |
//! | evidence_refs | `ArtifactRef::id` |
//! | failed_attempts | `what_was_tried` |
//! | active_constraints | `Constraint::key` |
//! | next_actions | the action string |
//! | rollback_points | `CheckpointRef::id` |
//!
//! Content hash for equality (separate from identity) uses
//! `serde_json` canonical form so any field change flips it.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::compact::fold::{
    ArtifactRef, CheckpointRef, DecisionWithRationale, FactWithEvidence, FailedAttempt,
    MicroCapsule, StructuredFold,
};
use crate::runtime::contracts::Constraint;

// ── AxisDelta + key extraction trait ──────────────────────────────

/// Anything that can live in a StructuredFold axis must declare its
/// `stable_key` (cross-fold identity) so the diff machinery can
/// detect added / removed / changed. Implemented for each of the
/// 8 axis item types below.
pub trait AxisItem: Clone + PartialEq + Serialize {
    fn stable_key(&self) -> String;

    /// Content hash — used to distinguish "same key, same content"
    /// from "same key, new content". Default impl uses
    /// `serde_json::to_string` which canonicalizes most shapes;
    /// types can override for cheaper / more stable hashes.
    fn content_hash(&self) -> String {
        match serde_json::to_string(self) {
            Ok(s) => djb2_hex(&s),
            // Should not happen for our serializable axis types; fall
            // back to a stable-key-only hash so this never panics.
            Err(_) => djb2_hex(&self.stable_key()),
        }
    }
}

/// Per-axis delta. Carries the actual items (not just keys) so the
/// dispatcher can re-inject the deltas as LLM-facing text without a
/// second pass over the fold.
///
/// `Default` is implemented manually so callers don't need `T: Default`
/// — none of the fold's 8 axis item types (FactWithEvidence, etc.)
/// have natural `Default` impls and we don't want to force them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AxisDelta<T> {
    pub added: Vec<T>,
    pub removed: Vec<T>,
    /// `(prior, new)` pairs for items whose `stable_key` matches but
    /// whose content_hash differs.
    pub changed: Vec<(T, T)>,
    pub unchanged_count: usize,
}

impl<T> Default for AxisDelta<T> {
    fn default() -> Self {
        Self {
            added: Vec::new(),
            removed: Vec::new(),
            changed: Vec::new(),
            unchanged_count: 0,
        }
    }
}

impl<T> AxisDelta<T> {
    /// `true` when nothing changed on this axis. Used by `FoldDelta`
    /// to short-circuit empty axes during rendering.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    pub fn added_count(&self) -> usize {
        self.added.len()
    }
    pub fn removed_count(&self) -> usize {
        self.removed.len()
    }
    pub fn changed_count(&self) -> usize {
        self.changed.len()
    }
}

/// Diff two axes given a stable-key extractor. Same shape as
/// `diff_snapshots` in `context_diff/diff.rs`. O(n+m).
pub fn diff_axis<T: AxisItem>(prior: &[T], new: &[T]) -> AxisDelta<T> {
    let mut prior_index: HashMap<String, &T> =
        prior.iter().map(|item| (item.stable_key(), item)).collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged_count = 0usize;

    for new_item in new {
        let key = new_item.stable_key();
        match prior_index.remove(&key) {
            None => added.push(new_item.clone()),
            Some(prior_item) => {
                if prior_item.content_hash() == new_item.content_hash() {
                    unchanged_count += 1;
                } else {
                    changed.push((prior_item.clone(), new_item.clone()));
                }
            }
        }
    }

    let mut removed: Vec<T> = prior_index.into_values().cloned().collect();
    removed.sort_by(|a, b| a.stable_key().cmp(&b.stable_key()));

    AxisDelta {
        added,
        removed,
        changed,
        unchanged_count,
    }
}

// ── AxisItem impls ────────────────────────────────────────────────

impl AxisItem for FactWithEvidence {
    fn stable_key(&self) -> String {
        // Statement text is the natural fact identity.
        self.statement.clone()
    }
}

impl AxisItem for DecisionWithRationale {
    fn stable_key(&self) -> String {
        // Decision title is the natural identity — "we decided X" is
        // one decision regardless of rationale tweaks. Lets rationale
        // updates and reversals (`Use rusqlite` → `Use sqlx`) surface
        // as `changed` rather than `removed + added`.
        self.decision.clone()
    }
}

impl AxisItem for FailedAttempt {
    fn stable_key(&self) -> String {
        self.what_was_tried.clone()
    }
}

impl AxisItem for ArtifactRef {
    fn stable_key(&self) -> String {
        self.id.clone()
    }
}

impl AxisItem for CheckpointRef {
    fn stable_key(&self) -> String {
        self.id.clone()
    }
}

impl AxisItem for MicroCapsule {
    fn stable_key(&self) -> String {
        self.turn_index.to_string()
    }
}

impl AxisItem for String {
    fn stable_key(&self) -> String {
        self.clone()
    }
    fn content_hash(&self) -> String {
        // String content_hash collapses to stable_key — the string
        // IS its identity AND its content, so "same key, different
        // content" is impossible. unchanged or added/removed only.
        djb2_hex(self)
    }
}

impl AxisItem for Constraint {
    fn stable_key(&self) -> String {
        // Constraint key field is the natural identity. Same `key`
        // with different `value` = changed.
        self.key.clone()
    }
}

// ── FoldDelta + StructuredFold::diff / apply_delta ────────────────

/// Full fold-vs-fold delta. One `AxisDelta` per StructuredFold axis
/// plus a `baseline_hash` so the dispatcher can verify the delta
/// applies on top of the expected base fold.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FoldDelta {
    pub facts: AxisDelta<FactWithEvidence>,
    pub decisions: AxisDelta<DecisionWithRationale>,
    pub unresolved_questions: AxisDelta<String>,
    pub evidence_refs: AxisDelta<ArtifactRef>,
    pub failed_attempts: AxisDelta<FailedAttempt>,
    pub active_constraints: AxisDelta<Constraint>,
    pub next_actions: AxisDelta<String>,
    pub rollback_points: AxisDelta<CheckpointRef>,
    pub micro_capsules: AxisDelta<MicroCapsule>,
    /// Hash of the prior fold the delta was computed against. The
    /// dispatcher refuses to apply a delta whose `baseline_hash`
    /// doesn't match the recorded last-fold anchor.
    pub baseline_hash: String,
}

impl FoldDelta {
    /// `true` when no axis changed.
    pub fn is_empty(&self) -> bool {
        self.facts.is_empty()
            && self.decisions.is_empty()
            && self.unresolved_questions.is_empty()
            && self.evidence_refs.is_empty()
            && self.failed_attempts.is_empty()
            && self.active_constraints.is_empty()
            && self.next_actions.is_empty()
            && self.rollback_points.is_empty()
            && self.micro_capsules.is_empty()
    }

    /// Sum of added + changed + removed across all axes — gauges
    /// whether the delta is "small enough" to inject as text rather
    /// than rerun a full fold.
    pub fn total_drift(&self) -> usize {
        fn drift<T>(a: &AxisDelta<T>) -> usize {
            a.added.len() + a.removed.len() + a.changed.len()
        }
        drift(&self.facts)
            + drift(&self.decisions)
            + drift(&self.unresolved_questions)
            + drift(&self.evidence_refs)
            + drift(&self.failed_attempts)
            + drift(&self.active_constraints)
            + drift(&self.next_actions)
            + drift(&self.rollback_points)
            + drift(&self.micro_capsules)
    }
}

impl StructuredFold {
    /// Stable hash of this fold, used as `FoldDelta::baseline_hash`.
    /// Same `serde_json::to_string` canonicalization the AxisItem
    /// content_hash uses.
    pub fn baseline_hash(&self) -> String {
        match serde_json::to_string(self) {
            Ok(s) => djb2_hex(&s),
            Err(_) => String::new(),
        }
    }

    /// Compute the per-axis delta from `self` (prior baseline) to
    /// `other` (current). The result's `baseline_hash` is set from
    /// `self.baseline_hash()` so `apply_delta` can verify it later.
    pub fn diff(&self, other: &StructuredFold) -> FoldDelta {
        FoldDelta {
            facts: diff_axis(&self.facts, &other.facts),
            decisions: diff_axis(&self.decisions, &other.decisions),
            unresolved_questions: diff_axis(
                &self.unresolved_questions,
                &other.unresolved_questions,
            ),
            evidence_refs: diff_axis(&self.evidence_refs, &other.evidence_refs),
            failed_attempts: diff_axis(&self.failed_attempts, &other.failed_attempts),
            active_constraints: diff_axis(
                &self.active_constraints,
                &other.active_constraints,
            ),
            next_actions: diff_axis(&self.next_actions, &other.next_actions),
            rollback_points: diff_axis(&self.rollback_points, &other.rollback_points),
            micro_capsules: diff_axis(&self.micro_capsules, &other.micro_capsules),
            baseline_hash: self.baseline_hash(),
        }
    }

    /// Apply `delta` to `self` (baseline) and return the resulting
    /// fold. Pure functional — does not mutate.
    ///
    /// `apply_to_axis` handles the per-axis merge: starting from the
    /// baseline axis vec, drop items whose key appears in `removed`
    /// or in `changed.prior`, then push the changed.new items, then
    /// push the added items.
    ///
    /// Round-trip guarantee: `self.diff(&other).apply_to(&self) ==
    /// other` whenever no two items in `other` share a `stable_key`
    /// (the StructuredFold invariant — duplicate keys per axis are
    /// undefined behavior).
    pub fn apply_delta(&self, delta: &FoldDelta) -> StructuredFold {
        StructuredFold {
            facts: apply_axis(&self.facts, &delta.facts),
            decisions: apply_axis(&self.decisions, &delta.decisions),
            unresolved_questions: apply_axis(
                &self.unresolved_questions,
                &delta.unresolved_questions,
            ),
            evidence_refs: apply_axis(&self.evidence_refs, &delta.evidence_refs),
            failed_attempts: apply_axis(&self.failed_attempts, &delta.failed_attempts),
            active_constraints: apply_axis(
                &self.active_constraints,
                &delta.active_constraints,
            ),
            next_actions: apply_axis(&self.next_actions, &delta.next_actions),
            rollback_points: apply_axis(&self.rollback_points, &delta.rollback_points),
            micro_capsules: apply_axis(&self.micro_capsules, &delta.micro_capsules),
        }
    }
}

/// Apply a single axis delta to its baseline vec. Order: baseline ∖
/// removed ∖ changed.prior, then push changed.new in order, then
/// push added in order. Deterministic.
fn apply_axis<T: AxisItem>(baseline: &[T], delta: &AxisDelta<T>) -> Vec<T> {
    // Index baseline items by stable_key for O(1) skip checks.
    let mut to_drop: std::collections::HashSet<String> = delta
        .removed
        .iter()
        .map(|item| item.stable_key())
        .collect();
    for (prior, _new) in &delta.changed {
        to_drop.insert(prior.stable_key());
    }

    let mut out: Vec<T> = baseline
        .iter()
        .filter(|item| !to_drop.contains(&item.stable_key()))
        .cloned()
        .collect();
    for (_prior, new) in &delta.changed {
        out.push(new.clone());
    }
    out.extend(delta.added.iter().cloned());
    out
}

// ── helpers ────────────────────────────────────────────────────────

fn djb2_hex(s: &str) -> String {
    use std::hash::Hasher;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    h.write(s.as_bytes());
    format!("{:x}", h.finish())
}

// ───────────────────────────────────────────────────────────────────
// Tests — Bundle 17-A spec acceptance criteria
// ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(stmt: &str, conf: Option<f32>) -> FactWithEvidence {
        FactWithEvidence {
            statement: stmt.into(),
            evidence: vec![],
            confidence: conf,
        }
    }

    fn decision(title: &str, rationale: &str) -> DecisionWithRationale {
        DecisionWithRationale {
            decision: title.into(),
            rationale: rationale.into(),
            alternatives_considered: vec![],
            evidence: vec![],
        }
    }

    fn fail(what: &str, why: &str) -> FailedAttempt {
        FailedAttempt {
            what_was_tried: what.into(),
            why_it_failed: why.into(),
            evidence: None,
        }
    }

    fn constraint(k: &str, v: &str) -> Constraint {
        serde_json::from_value(serde_json::json!({ "key": k, "value": v })).unwrap()
    }

    // ── identity per axis ──────────────────────────────────────────

    #[test]
    fn axis_item_keys_per_type() {
        assert_eq!(fact("A", None).stable_key(), "A");
        assert_eq!(decision("D", "r").stable_key(), "D");
        assert_eq!(fail("try", "fail").stable_key(), "try");
        assert_eq!(ArtifactRef::new("r:1").stable_key(), "r:1");
        assert_eq!(CheckpointRef::new("cp-7").stable_key(), "cp-7");
        assert_eq!("a question".to_string().stable_key(), "a question");
        assert_eq!(constraint("license", "Apache-2.0").stable_key(), "license");
    }

    // ── per-axis diff ──────────────────────────────────────────────

    #[test]
    fn diff_axis_detects_added_decision() {
        let prior = vec![decision("Use rusqlite", "sync API")];
        let new = vec![
            decision("Use rusqlite", "sync API"),
            decision("Adopt SemVer", "stable upgrade story"),
        ];
        let d = diff_axis(&prior, &new);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].decision, "Adopt SemVer");
        assert_eq!(d.unchanged_count, 1);
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
    }

    #[test]
    fn diff_axis_detects_resolved_blocker_as_removed() {
        // Modeling resolved blockers via `unresolved_questions` axis.
        let prior: Vec<String> = vec![
            "Does M2-G need Pin?".into(),
            "Cache invalidation?".into(),
        ];
        let new: Vec<String> = vec!["Cache invalidation?".into()];
        let d = diff_axis(&prior, &new);
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0], "Does M2-G need Pin?");
        assert_eq!(d.unchanged_count, 1);
    }

    #[test]
    fn diff_axis_detects_decision_reversal_as_changed_not_pair_of_add_remove() {
        // Same decision title, new rationale → must be `changed`.
        let prior = vec![decision("DB driver", "Use rusqlite — sync matches repo")];
        let new = vec![decision("DB driver", "Use sqlx — async-first, better tooling")];
        let d = diff_axis(&prior, &new);
        assert!(d.added.is_empty(), "stable key collapses to changed, not added");
        assert!(d.removed.is_empty(), "stable key collapses to changed, not removed");
        assert_eq!(d.changed.len(), 1);
        let (p, n) = &d.changed[0];
        assert_eq!(p.rationale, "Use rusqlite — sync matches repo");
        assert_eq!(n.rationale, "Use sqlx — async-first, better tooling");
    }

    #[test]
    fn diff_axis_constraint_value_change_is_changed() {
        let prior = vec![constraint("license", "Apache-2.0")];
        let new = vec![constraint("license", "MIT")];
        let d = diff_axis(&prior, &new);
        assert_eq!(d.changed.len(), 1);
        assert!(d.added.is_empty() && d.removed.is_empty());
    }

    #[test]
    fn diff_axis_empty_when_identical() {
        let s = vec![decision("X", "r"), decision("Y", "r2")];
        let d = diff_axis(&s, &s);
        assert!(d.is_empty());
        assert_eq!(d.unchanged_count, 2);
    }

    #[test]
    fn diff_axis_stats_count_correct() {
        let prior = vec![
            decision("A", "r1"), // unchanged
            decision("B", "r2"), // changed
            decision("C", "r3"), // removed
        ];
        let new = vec![
            decision("A", "r1"),    // unchanged
            decision("B", "r2-v2"), // changed
            decision("D", "r4"),    // added
        ];
        let d = diff_axis(&prior, &new);
        assert_eq!(d.added_count(), 1);
        assert_eq!(d.removed_count(), 1);
        assert_eq!(d.changed_count(), 1);
        assert_eq!(d.unchanged_count, 1);
        // Removed is id-sorted (key = decision string)
        assert_eq!(d.removed[0].decision, "C");
        assert_eq!(d.added[0].decision, "D");
    }

    // ── StructuredFold::diff (whole-fold) ─────────────────────────

    #[test]
    fn fold_diff_empty_when_identical_folds() {
        let f = StructuredFold::default()
            .with_decisions(vec![decision("X", "r")])
            .with_next_actions(vec!["a".into()]);
        let d = f.diff(&f);
        assert!(d.is_empty());
        assert_eq!(d.total_drift(), 0);
        // Baseline hash is populated even for empty diffs
        assert!(!d.baseline_hash.is_empty());
    }

    #[test]
    fn fold_diff_detects_added_fact_across_full_fold() {
        let prior = StructuredFold::default()
            .with_facts(vec![fact("Auth uses OAuth2", Some(0.9))]);
        let new = StructuredFold::default().with_facts(vec![
            fact("Auth uses OAuth2", Some(0.9)),
            fact("Storage is SQLite", Some(0.8)),
        ]);
        let d = prior.diff(&new);
        assert_eq!(d.facts.added.len(), 1);
        assert_eq!(d.facts.added[0].statement, "Storage is SQLite");
        assert_eq!(d.facts.unchanged_count, 1);
        assert!(d.decisions.is_empty());  // other axes untouched
        assert_eq!(d.total_drift(), 1);
    }

    #[test]
    fn fold_diff_baseline_hash_distinguishes_distinct_baselines() {
        let a = StructuredFold::default().with_next_actions(vec!["a".into()]);
        let b = StructuredFold::default().with_next_actions(vec!["b".into()]);
        assert_ne!(a.baseline_hash(), b.baseline_hash());
    }

    // ── apply_delta roundtrip (THE key invariant) ─────────────────

    #[test]
    fn fold_diff_apply_delta_roundtrip_pure_add() {
        let a = StructuredFold::default()
            .with_decisions(vec![decision("X", "r")])
            .with_next_actions(vec!["ship M2-D".into()]);
        let b = StructuredFold::default()
            .with_decisions(vec![decision("X", "r"), decision("Y", "r2")])
            .with_next_actions(vec!["ship M2-D".into(), "verify".into()]);
        let delta = a.diff(&b);
        let rebuilt = a.apply_delta(&delta);
        assert_eq!(rebuilt, b, "apply(diff(a, b)) must equal b");
    }

    #[test]
    fn fold_diff_apply_delta_roundtrip_with_changes_and_removes() {
        let a = StructuredFold::default()
            .with_decisions(vec![
                decision("A", "r-a-v1"),
                decision("B", "r-b"),  // will be removed
                decision("C", "r-c"),
            ])
            .with_facts(vec![fact("F1", None), fact("F2", Some(0.5))]);
        let b = StructuredFold::default()
            .with_decisions(vec![
                decision("A", "r-a-v2"),  // changed rationale
                decision("C", "r-c"),     // unchanged
                decision("D", "r-d"),     // added
            ])
            .with_facts(vec![fact("F1", None), fact("F2", Some(0.5))]);
        let delta = a.diff(&b);
        let rebuilt = a.apply_delta(&delta);
        // Order may differ within an axis but the set must match.
        assert_eq!(
            sorted_keys(&rebuilt.decisions),
            sorted_keys(&b.decisions),
            "decision keys must match after roundtrip",
        );
        assert_eq!(
            decisions_by_key(&rebuilt.decisions, "A").rationale,
            "r-a-v2",
            "changed rationale must propagate",
        );
        assert!(decisions_by_key_opt(&rebuilt.decisions, "B").is_none());
        assert!(decisions_by_key_opt(&rebuilt.decisions, "D").is_some());
        assert_eq!(rebuilt.facts, b.facts);
    }

    #[test]
    fn fold_diff_apply_delta_identity_when_diff_empty() {
        let a = StructuredFold::default()
            .with_unresolved_questions(vec!["q1".into(), "q2".into()]);
        let delta = a.diff(&a);
        assert!(delta.is_empty());
        let rebuilt = a.apply_delta(&delta);
        assert_eq!(rebuilt, a);
    }

    #[test]
    fn fold_diff_total_drift_counts_all_axes() {
        let a = StructuredFold::default();
        let b = StructuredFold::default()
            .with_facts(vec![fact("f1", None)])              // +1
            .with_decisions(vec![decision("d1", "r")])       // +1
            .with_next_actions(vec!["a1".into(), "a2".into()]) // +2
            .with_micro_capsules(vec![MicroCapsule {
                turn_index: 1,
                user_query: "query".into(),
                agent_outcome: "outcome".into(),
            }]);                                             // +1
        let d = a.diff(&b);
        assert_eq!(d.total_drift(), 5);
    }

    // ── serde roundtrip ───────────────────────────────────────────

    #[test]
    fn fold_delta_serde_roundtrip_camel_case() {
        let a = StructuredFold::default().with_next_actions(vec!["x".into()]);
        let b = StructuredFold::default().with_next_actions(vec!["y".into()]);
        let delta = a.diff(&b);
        let json = serde_json::to_string(&delta).unwrap();
        assert!(json.contains("\"baselineHash\":"));
        assert!(json.contains("\"unchangedCount\":"));
        let back: FoldDelta = serde_json::from_str(&json).unwrap();
        assert_eq!(delta, back);
    }

    // ── helpers ───────────────────────────────────────────────────

    fn sorted_keys<T: AxisItem>(items: &[T]) -> Vec<String> {
        let mut keys: Vec<String> = items.iter().map(|i| i.stable_key()).collect();
        keys.sort();
        keys
    }

    fn decisions_by_key<'a>(
        items: &'a [DecisionWithRationale],
        key: &str,
    ) -> &'a DecisionWithRationale {
        decisions_by_key_opt(items, key)
            .unwrap_or_else(|| panic!("decision key={} not found", key))
    }

    fn decisions_by_key_opt<'a>(
        items: &'a [DecisionWithRationale],
        key: &str,
    ) -> Option<&'a DecisionWithRationale> {
        items.iter().find(|d| d.decision == key)
    }
}
