//! `ContextManager` — per-turn context composition.

use std::sync::Arc;

use crate::agent::baseline_blocks;
use crate::runtime::context::{ContextArtifact, ContextFragment};

/// Per-turn input describing what the agent is working on. Used by
/// `for_prompt` to decide which fragments to inject and how to score
/// them.
#[derive(Debug, Clone, Default)]
pub struct ComposeQuery {
    /// Topics relevant to the current turn (kebab-lowercase, matching
    /// `BaselineBlock::topics()` / `ContextFragment::topics()`).
    pub topics: Vec<String>,
    /// Maximum total token budget the manager may use across selected
    /// fragments. `0` disables fragment injection (baseline still
    /// rendered).
    pub fragment_token_budget: usize,
    /// Maximum number of fragments to inject. `usize::MAX` for no cap.
    pub max_fragments: usize,
}

impl ComposeQuery {
    /// Default per-turn quota: 4 fragments / 8K tokens. Matches the
    /// budget ranges suggested in ADR §"Context Fabric".
    pub fn defaults_with_topics(topics: Vec<String>) -> Self {
        Self {
            topics,
            fragment_token_budget: 8192,
            max_fragments: 4,
        }
    }
}

/// Per-fragment selection observation — what passed budget, what was
/// rejected. Returned for the M2-J UI.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ComposeStats {
    /// Number of fragments in the manager's available set when
    /// `for_prompt` was called.
    pub fragments_available: usize,
    /// Fragments actually selected for injection.
    pub fragments_selected: usize,
    /// Fragments skipped because of `max_fragments` cap.
    pub fragments_dropped_for_count: usize,
    /// Fragments skipped because of `fragment_token_budget` cap.
    pub fragments_dropped_for_budget: usize,
    /// Total `token_estimate()` summed over selected fragments.
    pub fragment_tokens_used: usize,
}

/// Composed context ready to feed into the LLM call. The agent loop
/// concatenates `system_prompt` with whatever conversation history it
/// maintains, and injects `injected_fragments` as additional context
/// blocks (provider-specific format is the dispatcher's job).
#[derive(Debug, Clone)]
pub struct ComposedContext {
    /// The full baseline system prompt (from `baseline_blocks::registry`).
    pub system_prompt: String,
    /// Fragments selected for this turn, in score-descending order.
    pub injected_fragments: Vec<ContextArtifact>,
    /// What the manager did.
    pub stats: ComposeStats,
}

/// Central per-session context orchestrator. Built once at session
/// start, mutated as fragments are added/removed, and called by the
/// agent loop on every turn via [`for_prompt`].
///
/// Thread-safety: `ContextManager` holds `Arc<dyn ContextFragment>`
/// so fragments can be shared across threads. The manager itself is
/// not internally locked — wrap in `Mutex`/`RwLock` at the call site
/// if multiple threads will mutate the fragment set.
pub struct ContextManager {
    /// Dynamic fragments available to this session.
    fragments: Vec<Arc<dyn ContextFragment>>,
}

impl ContextManager {
    /// Empty manager — no fragments registered. Baseline blocks come
    /// from the static `baseline_blocks::registry`, so even an empty
    /// manager still produces the full baseline system prompt.
    pub fn new() -> Self {
        Self {
            fragments: Vec::new(),
        }
    }

    /// Register a fragment as available for this session.
    pub fn add_fragment(&mut self, fragment: Arc<dyn ContextFragment>) {
        self.fragments.push(fragment);
    }

    /// Bulk-add convenience.
    pub fn add_fragments<I>(&mut self, fragments: I)
    where
        I: IntoIterator<Item = Arc<dyn ContextFragment>>,
    {
        self.fragments.extend(fragments);
    }

    /// Number of fragments currently registered.
    pub fn fragment_count(&self) -> usize {
        self.fragments.len()
    }

    /// Return the static baseline system prompt (M2-A registry). This
    /// is identical across turns until baseline_blocks::registry()
    /// changes (currently never; M2-B follow-up adds UCLAW.md hot
    /// reload).
    pub fn baseline_system_prompt(&self) -> String {
        baseline_blocks::render_all()
    }

    /// Heart of the M2-B contract. Compose the per-turn LLM payload:
    ///
    /// 1. Render baseline blocks (M2-A) into the system prompt.
    /// 2. Score each registered fragment against `query.topics`.
    /// 3. Select fragments under the `max_fragments` + `fragment_token_budget`
    ///    caps, in descending score order.
    /// 4. Fetch each selected fragment's content.
    ///
    /// Note: this is `async` because [`ContextFragment::fetch`] is.
    /// The implementation issues fetches **sequentially** today —
    /// concurrent fetch via `futures::join_all` is a M2-B optimization
    /// follow-up (correctness wins out over speed in the pilot).
    pub async fn for_prompt(&self, query: &ComposeQuery) -> ComposedContext {
        let mut stats = ComposeStats {
            fragments_available: self.fragments.len(),
            ..Default::default()
        };

        let system_prompt = self.baseline_system_prompt();

        // Early-out: budget or count is zero → baseline only.
        if query.max_fragments == 0 || query.fragment_token_budget == 0 {
            return ComposedContext {
                system_prompt,
                injected_fragments: Vec::new(),
                stats,
            };
        }

        // Score by topic-overlap count, ties broken by ref id (stable).
        let mut scored: Vec<(i64, &Arc<dyn ContextFragment>)> = self
            .fragments
            .iter()
            .map(|f| (score_fragment(f.as_ref(), &query.topics), f))
            .collect();
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0).then_with(|| a.1.ref_().id.cmp(&b.1.ref_().id))
        });

        // Walk in ranked order under both caps.
        let mut selected_refs: Vec<&Arc<dyn ContextFragment>> = Vec::new();
        for (_, frag) in scored.into_iter() {
            if selected_refs.len() >= query.max_fragments {
                stats.fragments_dropped_for_count += 1;
                continue;
            }
            let est = frag.token_estimate();
            if stats.fragment_tokens_used + est > query.fragment_token_budget {
                stats.fragments_dropped_for_budget += 1;
                continue;
            }
            stats.fragment_tokens_used += est;
            selected_refs.push(frag);
        }

        // Fetch sequentially. Errors are swallowed (logged once we wire
        // tracing) — a missing fragment shouldn't abort the whole turn.
        let mut injected = Vec::with_capacity(selected_refs.len());
        for frag in selected_refs {
            match frag.fetch().await {
                Ok(art) => injected.push(art),
                Err(_e) => {
                    // M2-B follow-up: tracing::warn!(error = ?_e, "fragment fetch failed");
                }
            }
        }
        stats.fragments_selected = injected.len();

        ComposedContext {
            system_prompt,
            injected_fragments: injected,
            stats,
        }
    }
}

impl Default for ContextManager {
    fn default() -> Self {
        Self::new()
    }
}

// ── internal ───────────────────────────────────────────────────────

fn score_fragment(frag: &dyn ContextFragment, query_topics: &[String]) -> i64 {
    let mut score: i64 = 0;
    for t in frag.topics() {
        if query_topics.iter().any(|qt| qt == *t) {
            score += 10;
        }
    }
    score
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::context::{
        ContextSource, ConversationHistoryFragment, MemoryRecallFragment, WorkspaceFileFragment,
    };

    fn convo(id: &str) -> Arc<dyn ContextFragment> {
        Arc::new(ConversationHistoryFragment {
            thread_id: id.into(),
            turns: vec!["a".into(), "b".into()],
        })
    }

    fn mem(query: &str) -> Arc<dyn ContextFragment> {
        Arc::new(MemoryRecallFragment {
            query: query.into(),
            mock_hits: vec![("page".into(), "snippet".into())],
        })
    }

    fn file(rel: &str) -> Arc<dyn ContextFragment> {
        Arc::new(WorkspaceFileFragment {
            workspace_rel_path: rel.into(),
            max_bytes: Some(0),
        })
    }

    // ── construction / mutation ─────────────────────────────────────

    #[test]
    fn new_starts_empty() {
        let m = ContextManager::new();
        assert_eq!(m.fragment_count(), 0);
    }

    #[test]
    fn add_fragment_grows_set() {
        let mut m = ContextManager::new();
        m.add_fragment(convo("t1"));
        m.add_fragment(mem("rust"));
        assert_eq!(m.fragment_count(), 2);
    }

    #[test]
    fn add_fragments_bulk() {
        let mut m = ContextManager::new();
        m.add_fragments([convo("t1"), mem("rust"), file("a.rs")]);
        assert_eq!(m.fragment_count(), 3);
    }

    // ── baseline_system_prompt ──────────────────────────────────────

    #[test]
    fn baseline_system_prompt_is_non_empty_and_stable() {
        let m = ContextManager::new();
        let p1 = m.baseline_system_prompt();
        let p2 = m.baseline_system_prompt();
        assert!(!p1.is_empty());
        assert_eq!(p1, p2, "baseline must be stable across calls");
    }

    // ── for_prompt: budget / count caps ─────────────────────────────

    #[tokio::test]
    async fn for_prompt_returns_baseline_only_when_budget_zero() {
        let mut m = ContextManager::new();
        m.add_fragment(convo("t1"));
        let q = ComposeQuery {
            topics: vec!["conversation".into()],
            fragment_token_budget: 0,
            max_fragments: 10,
        };
        let out = m.for_prompt(&q).await;
        assert!(out.injected_fragments.is_empty());
        assert!(!out.system_prompt.is_empty());
        assert_eq!(out.stats.fragments_available, 1);
        assert_eq!(out.stats.fragments_selected, 0);
    }

    #[tokio::test]
    async fn for_prompt_returns_baseline_only_when_max_fragments_zero() {
        let mut m = ContextManager::new();
        m.add_fragment(convo("t1"));
        let q = ComposeQuery {
            topics: vec!["conversation".into()],
            fragment_token_budget: 99999,
            max_fragments: 0,
        };
        let out = m.for_prompt(&q).await;
        assert!(out.injected_fragments.is_empty());
    }

    // ── for_prompt: topic scoring ───────────────────────────────────

    #[tokio::test]
    async fn for_prompt_prioritizes_topic_match() {
        let mut m = ContextManager::new();
        m.add_fragment(mem("rust"));         // topics: ["memory", "recall"]
        m.add_fragment(convo("t1"));         // topics: ["conversation", "history"]
        let q = ComposeQuery::defaults_with_topics(vec!["conversation".into()]);
        let out = m.for_prompt(&q).await;
        // Both fragments fit budget — conversation should come first.
        assert!(out.injected_fragments.len() >= 1);
        assert_eq!(out.injected_fragments[0].r#ref.source, ContextSource::Conversation);
    }

    // ── for_prompt: max_fragments cap ───────────────────────────────

    #[tokio::test]
    async fn for_prompt_respects_max_fragments_cap() {
        let mut m = ContextManager::new();
        m.add_fragment(convo("t1"));
        m.add_fragment(convo("t2"));
        m.add_fragment(convo("t3"));
        let q = ComposeQuery {
            topics: vec!["conversation".into()],
            fragment_token_budget: 99999,
            max_fragments: 2,
        };
        let out = m.for_prompt(&q).await;
        assert_eq!(out.stats.fragments_selected, 2);
        assert_eq!(out.stats.fragments_dropped_for_count, 1);
    }

    // ── for_prompt: stable ordering on tied scores ──────────────────

    #[tokio::test]
    async fn ties_broken_by_ref_id_ascending() {
        let mut m = ContextManager::new();
        m.add_fragment(convo("z-thread"));
        m.add_fragment(convo("a-thread"));
        m.add_fragment(convo("m-thread"));
        let q = ComposeQuery::defaults_with_topics(vec!["conversation".into()]);
        let out = m.for_prompt(&q).await;
        let ids: Vec<&str> = out
            .injected_fragments
            .iter()
            .map(|a| a.r#ref.id.as_str())
            .collect();
        // ref.id is just the thread_id with a "conversation:" prefix
        // from ConversationHistoryFragment::ref_(). Either way they
        // sort alphabetically.
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
    }

    // ── for_prompt: empty manager still returns baseline ────────────

    #[tokio::test]
    async fn for_prompt_with_empty_manager_returns_baseline_only() {
        let m = ContextManager::new();
        let q = ComposeQuery::defaults_with_topics(vec!["anything".into()]);
        let out = m.for_prompt(&q).await;
        assert!(out.injected_fragments.is_empty());
        assert_eq!(out.stats.fragments_available, 0);
        assert!(!out.system_prompt.is_empty());
    }

    // ── ComposeQuery::defaults_with_topics ──────────────────────────

    #[test]
    fn defaults_with_topics_sets_8k_4_caps() {
        let q = ComposeQuery::defaults_with_topics(vec!["x".into()]);
        assert_eq!(q.fragment_token_budget, 8192);
        assert_eq!(q.max_fragments, 4);
        assert_eq!(q.topics, vec!["x"]);
    }

    // ── ComposeStats ────────────────────────────────────────────────

    #[tokio::test]
    async fn stats_count_available_independent_of_selected() {
        let mut m = ContextManager::new();
        m.add_fragments([convo("t1"), convo("t2"), mem("r")]);
        let q = ComposeQuery {
            topics: vec!["conversation".into()],
            fragment_token_budget: 99999,
            max_fragments: 1,
        };
        let out = m.for_prompt(&q).await;
        assert_eq!(out.stats.fragments_available, 3);
        assert_eq!(out.stats.fragments_selected, 1);
        // 2 dropped because of count cap (the non-conversation memory
        // fragment + the second convo).
        assert_eq!(out.stats.fragments_dropped_for_count, 2);
    }
}
