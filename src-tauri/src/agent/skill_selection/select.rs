//! `select_top_k` — score, rank, and budget-cap skill candidates.

use serde::{Deserialize, Serialize};

/// Default K per ADR §M2-H L3 (5 skills per turn).
pub const DEFAULT_TOP_K: usize = 5;

/// Default per-turn metadata budget per ADR §M2-H L3.
pub const DEFAULT_METADATA_BUDGET_TOKENS: usize = 1500;

/// A single skill that *could* be announced this turn.
///
/// `topics` follows the same kebab-lowercase convention as
/// `BaselineBlock::topics()` (M2-A) and `ContextFragment::topics()`
/// (M2-C). The selector uses topic match as the primary signal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SkillCandidate {
    pub id: String,
    pub title: String,
    pub topics: Vec<String>,
    /// Estimated token cost if this candidate's metadata is emitted.
    pub token_estimate: usize,
    /// Free-form description text. The selector scans the per-turn
    /// `recent_text` for description keywords as a secondary signal.
    pub description: String,
}

/// Per-turn query: what the user / agent is currently working on.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionQuery {
    /// Topics relevant to the current turn — e.g. derived from the
    /// pinned context-fragment set or the latest user message.
    pub topics: Vec<String>,
    /// Recent user/agent text used for description keyword matching.
    /// Pass an empty string to skip keyword matching.
    pub recent_text: String,
}

impl SelectionQuery {
    pub fn from_topics(topics: Vec<String>) -> Self {
        Self {
            topics,
            recent_text: String::new(),
        }
    }
}

/// What the selector did this turn — useful for the M2-J UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SelectionStats {
    pub considered: usize,
    pub selected: usize,
    pub tokens_used: usize,
    /// Number of candidates skipped because adding them would have
    /// exceeded `budget_tokens` even though `k` slots remained.
    pub dropped_for_budget: usize,
}

impl SelectionStats {
    pub fn fits_in_budget(&self, budget: usize) -> bool {
        self.tokens_used <= budget
    }
}

/// Select up to `k` of `candidates`, ranked by relevance to `query`,
/// constrained by both `k` and `budget_tokens`.
///
/// Returns the selected subset (already sorted by score descending,
/// stable on id) plus stats.
///
/// Scoring (deterministic, dependency-free):
///
/// - **+10** for each topic match (candidate topic ∈ query topics)
/// - **+1** for each whitespace-tokenized lowercased word in
///   `query.recent_text` that appears in the candidate's description
///   (also lowercased). Each token counts at most once, even if it
///   appears multiple times in the description.
///
/// Candidates scoring 0 are still included (with lowest priority) so
/// the selector is fail-soft — when no relevance signal exists at
/// all, the agent still gets *some* skills up to the budget.
pub fn select_top_k(
    candidates: Vec<SkillCandidate>,
    query: &SelectionQuery,
    k: usize,
    budget_tokens: usize,
) -> (Vec<SkillCandidate>, SelectionStats) {
    let mut stats = SelectionStats {
        considered: candidates.len(),
        ..Default::default()
    };

    if k == 0 || budget_tokens == 0 {
        return (Vec::new(), stats);
    }

    // Pre-tokenize recent_text once.
    let query_tokens: std::collections::HashSet<String> = query
        .recent_text
        .split_whitespace()
        .map(|t| t.to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .collect();

    // Score every candidate.
    let mut scored: Vec<(i64, SkillCandidate)> = candidates
        .into_iter()
        .map(|c| (score_candidate(&c, query, &query_tokens), c))
        .collect();

    // Sort: descending score, ties broken by ascending id (stable).
    scored.sort_by(|a, b| {
        b.0.cmp(&a.0).then_with(|| a.1.id.cmp(&b.1.id))
    });

    // Walk in ranked order, picking under both K and token budget.
    let mut picked: Vec<SkillCandidate> = Vec::with_capacity(k.min(stats.considered));
    for (_, cand) in scored.into_iter() {
        if picked.len() >= k {
            break;
        }
        if stats.tokens_used + cand.token_estimate > budget_tokens {
            stats.dropped_for_budget += 1;
            continue;
        }
        stats.tokens_used += cand.token_estimate;
        picked.push(cand);
    }
    stats.selected = picked.len();
    (picked, stats)
}

fn score_candidate(
    c: &SkillCandidate,
    query: &SelectionQuery,
    query_tokens: &std::collections::HashSet<String>,
) -> i64 {
    let mut score: i64 = 0;
    // +10 per topic match.
    for t in &c.topics {
        if query.topics.iter().any(|qt| qt == t) {
            score += 10;
        }
    }
    // +1 per unique query token found in candidate description.
    let desc_lower = c.description.to_ascii_lowercase();
    for qt in query_tokens {
        if desc_lower.contains(qt) {
            score += 1;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(id: &str, topics: &[&str], tokens: usize, desc: &str) -> SkillCandidate {
        SkillCandidate {
            id: id.into(),
            title: id.into(),
            topics: topics.iter().map(|s| s.to_string()).collect(),
            token_estimate: tokens,
            description: desc.into(),
        }
    }

    // ── empty inputs ────────────────────────────────────────────────

    #[test]
    fn empty_candidates_returns_empty() {
        let q = SelectionQuery::from_topics(vec!["x".into()]);
        let (out, stats) = select_top_k(vec![], &q, DEFAULT_TOP_K, DEFAULT_METADATA_BUDGET_TOKENS);
        assert!(out.is_empty());
        assert_eq!(stats.considered, 0);
        assert_eq!(stats.selected, 0);
    }

    #[test]
    fn k_zero_returns_empty() {
        let q = SelectionQuery::default();
        let (out, _) = select_top_k(vec![cand("a", &["x"], 100, "")], &q, 0, 1000);
        assert!(out.is_empty());
    }

    #[test]
    fn budget_zero_returns_empty() {
        let q = SelectionQuery::default();
        let (out, _) = select_top_k(vec![cand("a", &["x"], 100, "")], &q, 5, 0);
        assert!(out.is_empty());
    }

    // ── topic match ────────────────────────────────────────────────

    #[test]
    fn topic_match_outranks_no_match() {
        let q = SelectionQuery::from_topics(vec!["browser".into()]);
        let cands = vec![
            cand("no-match", &["unrelated"], 100, ""),
            cand("match", &["browser"], 100, ""),
        ];
        let (out, _) = select_top_k(cands, &q, 5, 1000);
        // Both included (fail-soft), but match comes first.
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].id, "match");
        assert_eq!(out[1].id, "no-match");
    }

    #[test]
    fn multiple_topic_matches_accumulate() {
        let q = SelectionQuery::from_topics(vec!["a".into(), "b".into(), "c".into()]);
        let cands = vec![
            cand("one-match", &["a", "z"], 100, ""),
            cand("triple-match", &["a", "b", "c"], 100, ""),
            cand("double-match", &["a", "b"], 100, ""),
        ];
        let (out, _) = select_top_k(cands, &q, 5, 1000);
        assert_eq!(out[0].id, "triple-match");
        assert_eq!(out[1].id, "double-match");
        assert_eq!(out[2].id, "one-match");
    }

    // ── description keyword scoring ────────────────────────────────

    #[test]
    fn description_keyword_match_breaks_tie_among_topicless() {
        let q = SelectionQuery {
            topics: vec![],
            recent_text: "deploy nginx config".into(),
        };
        let cands = vec![
            cand("a", &[], 100, "unrelated text"),
            cand("b", &[], 100, "for deploying nginx to prod"), // 2 matches
        ];
        let (out, _) = select_top_k(cands, &q, 5, 1000);
        assert_eq!(out[0].id, "b");
    }

    #[test]
    fn description_match_case_insensitive() {
        let q = SelectionQuery {
            topics: vec![],
            recent_text: "RUST async".into(),
        };
        let c = cand("a", &[], 100, "Rust Async runtime helpers");
        let (out, _) = select_top_k(vec![c], &q, 5, 1000);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn topic_match_outranks_pure_description_match() {
        // +10 per topic vs +1 per description keyword — topic wins
        // even if description has 9 matches.
        let q = SelectionQuery {
            topics: vec!["x".into()],
            recent_text: "alpha beta gamma delta epsilon zeta eta theta iota".into(),
        };
        let cands = vec![
            cand(
                "desc-heavy",
                &[],
                100,
                "alpha beta gamma delta epsilon zeta eta theta iota",
            ),
            cand("topic-match", &["x"], 100, ""),
        ];
        let (out, _) = select_top_k(cands, &q, 5, 1000);
        // 1 topic match = 10 > 9 description matches.
        assert_eq!(out[0].id, "topic-match");
    }

    // ── k cap ──────────────────────────────────────────────────────

    #[test]
    fn picks_at_most_k_candidates() {
        let q = SelectionQuery::from_topics(vec!["x".into()]);
        let cands: Vec<_> = (0..10)
            .map(|i| cand(&format!("c{i:02}"), &["x"], 50, ""))
            .collect();
        let (out, stats) = select_top_k(cands, &q, 3, 10_000);
        assert_eq!(out.len(), 3);
        assert_eq!(stats.selected, 3);
    }

    // ── budget cap ─────────────────────────────────────────────────

    #[test]
    fn respects_token_budget_even_with_remaining_k_slots() {
        let q = SelectionQuery::from_topics(vec!["x".into()]);
        // Each costs 400 tokens; budget is 1000 → picks 2, drops rest.
        let cands: Vec<_> = (0..5)
            .map(|i| cand(&format!("c{i}"), &["x"], 400, ""))
            .collect();
        let (out, stats) = select_top_k(cands, &q, 10, 1000);
        assert_eq!(stats.selected, 2);
        assert_eq!(out.len(), 2);
        assert!(stats.fits_in_budget(1000));
        assert_eq!(stats.tokens_used, 800);
        assert_eq!(stats.dropped_for_budget, 3);
    }

    #[test]
    fn skips_oversized_candidate_but_keeps_smaller_ones_after() {
        // High-priority but oversized: skipped. Low-priority but
        // affordable: included.
        let q = SelectionQuery::from_topics(vec!["high".into()]);
        let cands = vec![
            cand("huge-priority", &["high"], 5000, ""),
            cand("small-low-priority", &[], 200, ""),
            cand("medium-low-priority", &[], 300, ""),
        ];
        let (out, stats) = select_top_k(cands, &q, 5, 1000);
        // huge-priority skipped (5000 > 1000 budget).
        // small + medium fit (200 + 300 = 500).
        assert_eq!(out.len(), 2);
        assert!(out.iter().all(|c| c.id != "huge-priority"));
        assert_eq!(stats.dropped_for_budget, 1);
    }

    // ── tie breaking ───────────────────────────────────────────────

    #[test]
    fn ties_broken_by_ascending_id() {
        let q = SelectionQuery::from_topics(vec!["x".into()]);
        // All score equally — order should follow id ascending.
        let cands = vec![
            cand("c", &["x"], 100, ""),
            cand("a", &["x"], 100, ""),
            cand("b", &["x"], 100, ""),
        ];
        let (out, _) = select_top_k(cands, &q, 5, 1000);
        assert_eq!(out.iter().map(|c| &c.id).collect::<Vec<_>>(), vec!["a", "b", "c"]);
    }

    // ── stats accuracy ────────────────────────────────────────────

    #[test]
    fn stats_count_considered_independent_of_selected() {
        let q = SelectionQuery::from_topics(vec!["x".into()]);
        let cands = vec![
            cand("a", &["x"], 100, ""),
            cand("b", &["x"], 100, ""),
            cand("c", &["x"], 100, ""),
        ];
        let (_, stats) = select_top_k(cands, &q, 1, 1000);
        assert_eq!(stats.considered, 3);
        assert_eq!(stats.selected, 1);
        // 2 dropped because k limit, not budget.
        assert_eq!(stats.dropped_for_budget, 0);
    }

    // ── serde roundtrip ────────────────────────────────────────────

    #[test]
    fn skill_candidate_serde_roundtrip() {
        let c = cand("rust-async", &["rust", "async"], 250, "Helpers for tokio");
        let json = serde_json::to_string(&c).unwrap();
        let back: SkillCandidate = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
