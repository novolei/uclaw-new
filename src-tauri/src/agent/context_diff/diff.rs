//! Snapshot + diff types and `diff_snapshots` function.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::agent::compact::ArtifactRef;

/// One fragment's identity + content-hash + token estimate. Captured
/// per turn so the next turn's diff can detect added / removed /
/// changed fragments without holding their full content.
///
/// `content_hash` is opaque — typically a hex SHA-256 of the
/// fragment's rendered body, but any stable string the caller
/// produces works (e.g. "{retrieval_ts}-{len}"). The diff only cares
/// about string inequality.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentSnapshot {
    pub fragment_ref: ArtifactRef,
    pub content_hash: String,
    pub token_estimate: usize,
}

impl FragmentSnapshot {
    pub fn new(
        fragment_ref: ArtifactRef,
        content_hash: impl Into<String>,
        token_estimate: usize,
    ) -> Self {
        Self {
            fragment_ref,
            content_hash: content_hash.into(),
            token_estimate,
        }
    }
}

/// One changed fragment — its ref, the prior content hash, the new
/// content hash. Sufficient for "this fragment was updated, re-fetch
/// it" semantics; the actual new content is fetched at injection
/// time by the dispatcher.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedFragment {
    pub fragment_ref: ArtifactRef,
    pub prior_hash: String,
    pub new_hash: String,
    pub new_token_estimate: usize,
}

/// Aggregate stats returned alongside the diff. The dispatcher uses
/// `is_significant_change()` to decide between sending the full fold
/// (for catastrophic context changes) and sending just the diff.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiffStats {
    pub added: usize,
    pub removed: usize,
    pub changed: usize,
    pub unchanged: usize,
    /// Sum of `token_estimate` across `added` and `changed`. The
    /// transmit cost of the diff is roughly this number.
    pub added_or_changed_tokens: usize,
}

impl DiffStats {
    pub fn total_prior(&self) -> usize {
        self.removed + self.changed + self.unchanged
    }

    pub fn total_new(&self) -> usize {
        self.added + self.changed + self.unchanged
    }

    /// `true` when more than `threshold_fraction` of prior fragments
    /// were either removed or changed — sign that the full fold is
    /// cheaper than the diff in token count.
    pub fn is_significant_change(&self, threshold_fraction: f32) -> bool {
        let prior = self.total_prior();
        if prior == 0 {
            return false;
        }
        let drift = self.removed + self.changed;
        (drift as f32) / (prior as f32) >= threshold_fraction
    }
}

/// The diff between two fragment-snapshot lists. `added`, `removed`,
/// and `changed` carry the actual ref+hash trios; the dispatcher
/// uses them to plan re-fetches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextDiff {
    pub added: Vec<FragmentSnapshot>,
    pub removed: Vec<FragmentSnapshot>,
    pub changed: Vec<ChangedFragment>,
    /// Number of fragments present in both lists with identical hash.
    pub unchanged_count: usize,
}

impl ContextDiff {
    /// `true` when nothing changed — caller may skip re-injection.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }

    /// Build a `DiffStats` summary from this diff.
    pub fn stats(&self) -> DiffStats {
        let added_or_changed_tokens: usize = self
            .added
            .iter()
            .map(|s| s.token_estimate)
            .chain(self.changed.iter().map(|c| c.new_token_estimate))
            .sum();
        DiffStats {
            added: self.added.len(),
            removed: self.removed.len(),
            changed: self.changed.len(),
            unchanged: self.unchanged_count,
            added_or_changed_tokens,
        }
    }
}

/// Compute the diff between a prior snapshot list and a new one.
///
/// Algorithm:
///
/// 1. Index `prior` by `fragment_ref` (HashMap<ArtifactRef, &FragmentSnapshot>).
/// 2. Walk `new`. For each entry, look up in the prior index:
///    - missing → added
///    - present + same hash → unchanged
///    - present + different hash → changed
/// 3. Whatever's left in the prior index → removed.
///
/// `ArtifactRef` is `Hash + Eq` (#331), so the lookup is O(1) and the
/// overall algorithm is O(n + m). Order of returned `added` /
/// `removed` / `changed` mirrors input ordering for determinism.
pub fn diff_snapshots(prior: &[FragmentSnapshot], new: &[FragmentSnapshot]) -> ContextDiff {
    let mut prior_index: HashMap<&ArtifactRef, &FragmentSnapshot> =
        prior.iter().map(|s| (&s.fragment_ref, s)).collect();

    let mut added = Vec::new();
    let mut changed = Vec::new();
    let mut unchanged_count = 0usize;

    for new_snap in new {
        match prior_index.remove(&new_snap.fragment_ref) {
            None => added.push(new_snap.clone()),
            Some(prior_snap) if prior_snap.content_hash == new_snap.content_hash => {
                unchanged_count += 1;
            }
            Some(prior_snap) => {
                changed.push(ChangedFragment {
                    fragment_ref: new_snap.fragment_ref.clone(),
                    prior_hash: prior_snap.content_hash.clone(),
                    new_hash: new_snap.content_hash.clone(),
                    new_token_estimate: new_snap.token_estimate,
                });
            }
        }
    }

    // Anything left in the index was in prior but not in new.
    let mut removed: Vec<FragmentSnapshot> = prior_index.into_values().cloned().collect();
    // Sort removed by id for deterministic output (HashMap drain is
    // not order-stable).
    removed.sort_by(|a, b| a.fragment_ref.id.cmp(&b.fragment_ref.id));

    ContextDiff {
        added,
        removed,
        changed,
        unchanged_count,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(id: &str, hash: &str, tokens: usize) -> FragmentSnapshot {
        FragmentSnapshot::new(ArtifactRef::new(id), hash, tokens)
    }

    // ── empty cases ────────────────────────────────────────────────

    #[test]
    fn diff_empty_lists_is_empty() {
        let d = diff_snapshots(&[], &[]);
        assert!(d.is_empty());
        assert_eq!(d.unchanged_count, 0);
    }

    #[test]
    fn diff_only_new_means_all_added() {
        let new = vec![snap("a", "h1", 100), snap("b", "h2", 200)];
        let d = diff_snapshots(&[], &new);
        assert_eq!(d.added.len(), 2);
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
        assert_eq!(d.unchanged_count, 0);
    }

    #[test]
    fn diff_only_prior_means_all_removed() {
        let prior = vec![snap("a", "h1", 100), snap("b", "h2", 200)];
        let d = diff_snapshots(&prior, &[]);
        assert_eq!(d.removed.len(), 2);
        assert!(d.added.is_empty());
        assert!(d.changed.is_empty());
    }

    // ── unchanged / changed detection ───────────────────────────────

    #[test]
    fn identical_snapshots_all_unchanged() {
        let s = vec![snap("a", "h1", 100), snap("b", "h2", 200)];
        let d = diff_snapshots(&s, &s);
        assert!(d.is_empty());
        assert_eq!(d.unchanged_count, 2);
    }

    #[test]
    fn detects_hash_change_as_changed_not_added_or_removed() {
        let prior = vec![snap("a", "h1", 100)];
        let new = vec![snap("a", "h2", 120)]; // same id, new hash
        let d = diff_snapshots(&prior, &new);
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());
        assert_eq!(d.changed.len(), 1);
        let c = &d.changed[0];
        assert_eq!(c.prior_hash, "h1");
        assert_eq!(c.new_hash, "h2");
        assert_eq!(c.new_token_estimate, 120);
    }

    // ── mixed ──────────────────────────────────────────────────────

    #[test]
    fn mixed_added_removed_changed_unchanged() {
        let prior = vec![
            snap("a", "h1", 100), // unchanged
            snap("b", "h2", 200), // changed
            snap("c", "h3", 300), // removed
        ];
        let new = vec![
            snap("a", "h1", 100),    // unchanged
            snap("b", "h2-v2", 220), // changed
            snap("d", "h4", 150),    // added
        ];
        let d = diff_snapshots(&prior, &new);
        assert_eq!(d.added.len(), 1);
        assert_eq!(d.added[0].fragment_ref.id, "d");
        assert_eq!(d.removed.len(), 1);
        assert_eq!(d.removed[0].fragment_ref.id, "c");
        assert_eq!(d.changed.len(), 1);
        assert_eq!(d.changed[0].fragment_ref.id, "b");
        assert_eq!(d.unchanged_count, 1);
        assert!(!d.is_empty());
    }

    // ── deterministic ordering ────────────────────────────────────

    #[test]
    fn removed_is_id_sorted() {
        let prior = vec![snap("z", "h", 1), snap("a", "h", 1), snap("m", "h", 1)];
        let d = diff_snapshots(&prior, &[]);
        let ids: Vec<&str> = d
            .removed
            .iter()
            .map(|s| s.fragment_ref.id.as_str())
            .collect();
        assert_eq!(ids, vec!["a", "m", "z"]);
    }

    #[test]
    fn added_preserves_input_order() {
        let new = vec![snap("z", "h", 1), snap("a", "h", 1), snap("m", "h", 1)];
        let d = diff_snapshots(&[], &new);
        let ids: Vec<&str> = d.added.iter().map(|s| s.fragment_ref.id.as_str()).collect();
        assert_eq!(ids, vec!["z", "a", "m"]);
    }

    // ── stats ──────────────────────────────────────────────────────

    #[test]
    fn stats_count_correct() {
        let prior = vec![snap("a", "h1", 100), snap("b", "h2", 200)];
        let new = vec![snap("a", "h1-v2", 120), snap("c", "h3", 300)];
        let d = diff_snapshots(&prior, &new);
        let s = d.stats();
        assert_eq!(s.added, 1);
        assert_eq!(s.removed, 1);
        assert_eq!(s.changed, 1);
        assert_eq!(s.unchanged, 0);
        // 1 added (300) + 1 changed (120) = 420 tokens
        assert_eq!(s.added_or_changed_tokens, 420);
        assert_eq!(s.total_prior(), 2);
        assert_eq!(s.total_new(), 2);
    }

    #[test]
    fn is_significant_change_threshold() {
        let prior = vec![
            snap("a", "h", 1),
            snap("b", "h", 1),
            snap("c", "h", 1),
            snap("d", "h", 1),
        ];
        // 2 removed, 0 changed → drift = 2 / 4 = 0.5.
        let new = vec![snap("a", "h", 1), snap("b", "h", 1)];
        let d = diff_snapshots(&prior, &new);
        let s = d.stats();
        assert!(s.is_significant_change(0.5));
        assert!(s.is_significant_change(0.4));
        assert!(!s.is_significant_change(0.6));
    }

    #[test]
    fn is_significant_change_returns_false_for_empty_prior() {
        let d = diff_snapshots(&[], &[snap("a", "h", 1)]);
        let s = d.stats();
        assert!(!s.is_significant_change(0.0));
    }

    // ── serde ─────────────────────────────────────────────────────

    #[test]
    fn diff_serde_camel_case_roundtrip() {
        let prior = vec![snap("a", "h1", 100), snap("b", "h2", 200)];
        let new = vec![snap("a", "h1-v2", 120), snap("c", "h3", 300)];
        let d = diff_snapshots(&prior, &new);
        let json = serde_json::to_string(&d).unwrap();
        assert!(json.contains("\"unchangedCount\":"));
        assert!(json.contains("\"fragmentRef\":"));
        assert!(json.contains("\"newTokenEstimate\":"));
        let back: ContextDiff = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
