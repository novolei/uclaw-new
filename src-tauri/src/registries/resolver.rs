//! M3-T2 — Capability Mesh resolver.
//!
//! Turns a `CapabilityQuery` (M1-T1) into a ranked list of
//! `RegistryEntry` matches across the relevant registry.
//!
//! Scoring (per ADR §"Capability Mesh"):
//!
//! - **+100** if `query.name` exactly matches `entry.id()`
//! - **+50** if `query.kind` matches `entry.kind()`
//! - **+5** per query tag whose key+value matches the entry's tags
//! - Candidates with score 0 are NOT returned (caller passed a query
//!   that doesn't constrain enough to be useful).
//!
//! The resolver is parameterized over the registry's entry type so
//! the same logic serves skills / connectors / tools / models / themes.

use crate::registries::{entry::RegistryEntry, store::Registry};
use crate::runtime::contracts::CapabilityQuery;

/// One resolver match — entry id + computed score. The entry itself
/// is fetched via `registry.lookup(&id)` after resolution; returning
/// only the id keeps the resolver lifetime-free of the registry's
/// borrow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedMatch {
    pub id: String,
    pub score: u32,
}

/// Aggregate result of [`resolve`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolveResult {
    /// Matches sorted by `score` descending, ties broken by `id`
    /// ascending. Score-0 entries are excluded.
    pub matches: Vec<ResolvedMatch>,
    /// Total entries the resolver examined.
    pub considered: usize,
}

impl ResolveResult {
    /// Best match's id, if any.
    pub fn best(&self) -> Option<&str> {
        self.matches.first().map(|m| m.id.as_str())
    }

    /// `true` if no matches scored above zero.
    pub fn is_empty(&self) -> bool {
        self.matches.is_empty()
    }
}

/// Resolve a `CapabilityQuery` against any `Registry<E: RegistryEntry>`.
///
/// Generic over `E` so the resolver works uniformly across the 5
/// registries. Caller picks which registry to query.
pub fn resolve<E: RegistryEntry>(
    registry: &Registry<E>,
    query: &CapabilityQuery,
) -> ResolveResult {
    let mut considered = 0usize;
    let mut scored: Vec<ResolvedMatch> = Vec::new();
    for entry in registry.list() {
        considered += 1;
        let s = score_entry(entry, query);
        if s > 0 {
            scored.push(ResolvedMatch {
                id: entry.id().to_string(),
                score: s,
            });
        }
    }
    // Sort by score desc, ties by id asc.
    scored.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
    ResolveResult {
        matches: scored,
        considered,
    }
}

fn score_entry<E: RegistryEntry>(entry: &E, query: &CapabilityQuery) -> u32 {
    let mut score: u32 = 0;
    // name exact match
    if let Some(qname) = &query.name {
        if qname == entry.id() {
            score += 100;
        }
    }
    // kind exact match
    if !query.kind.is_empty() && query.kind == entry.kind() {
        score += 50;
    }
    // tag key+value match
    for (qk, qv) in &query.tags {
        if let Some(ev) = entry.tags().get(qk) {
            if ev == qv {
                score += 5;
            }
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registries::skills::SkillEntry;
    use std::collections::BTreeMap;

    fn capability(kind: &str, name: Option<&str>) -> CapabilityQuery {
        CapabilityQuery {
            kind: kind.into(),
            name: name.map(|s| s.into()),
            tags: BTreeMap::new(),
        }
    }

    fn skill(id: &str, kind: &str) -> SkillEntry {
        SkillEntry {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            description: String::new(),
            token_estimate: 100,
            tags: BTreeMap::new(),
        }
    }

    fn tagged_skill(id: &str, kind: &str, tags: &[(&str, &str)]) -> SkillEntry {
        let mut t = BTreeMap::new();
        for (k, v) in tags {
            t.insert((*k).into(), (*v).into());
        }
        SkillEntry {
            id: id.into(),
            kind: kind.into(),
            title: id.into(),
            description: String::new(),
            token_estimate: 100,
            tags: t,
        }
    }

    fn cap_with_tags(kind: &str, tags: &[(&str, &str)]) -> CapabilityQuery {
        let mut t = BTreeMap::new();
        for (k, v) in tags {
            t.insert((*k).into(), (*v).into());
        }
        CapabilityQuery {
            kind: kind.into(),
            name: None,
            tags: t,
        }
    }

    // ── empty / no-match ────────────────────────────────────────────

    #[test]
    fn empty_registry_yields_empty_result() {
        let r: Registry<SkillEntry> = Registry::new();
        let q = capability("skill", Some("anything"));
        let out = resolve(&r, &q);
        assert!(out.is_empty());
        assert_eq!(out.considered, 0);
    }

    #[test]
    fn query_matching_nothing_returns_empty() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        let q = capability("plugin", Some("nope"));
        let out = resolve(&r, &q);
        assert!(out.is_empty());
        assert_eq!(out.considered, 1);
        assert!(out.best().is_none());
    }

    // ── kind / name scoring ────────────────────────────────────────

    #[test]
    fn name_exact_match_outranks_kind_match() {
        let mut r = Registry::new();
        // entry "a" with matching kind only → +50
        r.register(skill("a", "builtin")).unwrap();
        // entry "target" with matching kind AND matching id → +50 + +100
        r.register(skill("target", "builtin")).unwrap();
        let q = capability("builtin", Some("target"));
        let out = resolve(&r, &q);
        assert_eq!(out.matches.len(), 2);
        assert_eq!(out.best(), Some("target"));
        assert_eq!(out.matches[0].score, 150);
        assert_eq!(out.matches[1].score, 50);
    }

    #[test]
    fn kind_only_match_scores_50() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        let q = capability("builtin", None);
        let out = resolve(&r, &q);
        assert_eq!(out.matches.len(), 1);
        assert_eq!(out.matches[0].score, 50);
    }

    #[test]
    fn empty_kind_with_only_name_match_still_scores() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        let q = capability("", Some("a"));
        let out = resolve(&r, &q);
        assert_eq!(out.matches.len(), 1);
        assert_eq!(out.matches[0].score, 100);
    }

    // ── tag scoring ────────────────────────────────────────────────

    #[test]
    fn tag_match_adds_5_per_matching_pair() {
        let mut r = Registry::new();
        r.register(tagged_skill("a", "builtin", &[("lang", "rust")]))
            .unwrap();
        r.register(tagged_skill(
            "b",
            "builtin",
            &[("lang", "rust"), ("level", "advanced")],
        ))
        .unwrap();
        let q = cap_with_tags("builtin", &[("lang", "rust"), ("level", "advanced")]);
        let out = resolve(&r, &q);
        assert_eq!(out.matches.len(), 2);
        // b has 2 tag matches (+10) + kind (+50) = 60
        // a has 1 tag match (+5) + kind (+50) = 55
        assert_eq!(out.best(), Some("b"));
        assert_eq!(out.matches[0].score, 60);
        assert_eq!(out.matches[1].score, 55);
    }

    #[test]
    fn tag_value_mismatch_scores_zero() {
        let mut r = Registry::new();
        r.register(tagged_skill("a", "builtin", &[("lang", "python")]))
            .unwrap();
        let q = cap_with_tags("plugin", &[("lang", "rust")]);
        let out = resolve(&r, &q);
        // No kind match, no tag value match, no name → score 0 → excluded
        assert!(out.is_empty());
    }

    // ── ordering ───────────────────────────────────────────────────

    #[test]
    fn ties_broken_by_id_ascending() {
        let mut r = Registry::new();
        r.register(skill("zebra", "builtin")).unwrap();
        r.register(skill("alpha", "builtin")).unwrap();
        r.register(skill("mango", "builtin")).unwrap();
        let q = capability("builtin", None);
        let out = resolve(&r, &q);
        let ids: Vec<&str> = out.matches.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, vec!["alpha", "mango", "zebra"]);
    }

    // ── ResolveResult accessors ────────────────────────────────────

    #[test]
    fn best_returns_top_match() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        r.register(skill("hit", "builtin")).unwrap();
        let q = capability("builtin", Some("hit"));
        let out = resolve(&r, &q);
        assert_eq!(out.best(), Some("hit"));
    }

    #[test]
    fn considered_counts_all_entries_even_unmatched() {
        let mut r = Registry::new();
        r.register(skill("a", "builtin")).unwrap();
        r.register(skill("b", "plugin")).unwrap();
        r.register(skill("c", "user")).unwrap();
        let q = capability("plugin", None);
        let out = resolve(&r, &q);
        assert_eq!(out.considered, 3);
        assert_eq!(out.matches.len(), 1);
    }
}
