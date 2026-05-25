//! `ComposeStatsCollector` — in-memory latest-`ComposeStats`-per-conversation store.
//!
//! M2-J UI feed for the M2-B `ContextManager` wire-up (C2-Dirac-B2).
//! Mirrors the `agent::telemetry::TokenBudgetCollector` shape: a
//! cloneable handle around an `Arc<RwLock<HashMap<..>>>` shared via
//! `AppState`. The `ChatDelegate` writes the latest stats each turn via
//! `record`; the `get_compose_stats` Tauri command reads via `latest`.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::agent::context_manager::ComposeStats;

/// Latest-`ComposeStats`-per-conversation store. Each conversation
/// overwrites its previous stats on each `effective_system_prompt` call.
///
/// Cloneable — share via `AppState` across Tauri commands. Internal
/// `RwLock` allows concurrent reads (Tauri command threads) while the
/// single agent-loop writer takes a brief exclusive lock.
#[derive(Debug, Clone, Default)]
pub struct ComposeStatsCollector {
    inner: Arc<RwLock<HashMap<String, ComposeStats>>>,
}

impl ComposeStatsCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace `conversation_id`'s stats with `stats`.
    pub fn record(&self, conversation_id: &str, stats: ComposeStats) {
        let mut map = self.inner.write().expect("compose-stats lock poisoned");
        map.insert(conversation_id.to_string(), stats);
    }

    /// Return the latest stats for `conversation_id` (cloned).
    pub fn latest(&self, conversation_id: &str) -> Option<ComposeStats> {
        let map = self.inner.read().expect("compose-stats lock poisoned");
        map.get(conversation_id).cloned()
    }

    /// Number of conversations tracked.
    pub fn len(&self) -> usize {
        self.inner.read().expect("compose-stats lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner
            .read()
            .expect("compose-stats lock poisoned")
            .is_empty()
    }

    /// Drop the stats for a conversation — call when the session closes.
    pub fn forget(&self, conversation_id: &str) -> bool {
        self.inner
            .write()
            .expect("compose-stats lock poisoned")
            .remove(conversation_id)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats(available: usize, selected: usize) -> ComposeStats {
        ComposeStats {
            fragments_available: available,
            fragments_selected: selected,
            ..Default::default()
        }
    }

    #[test]
    fn empty_collector_returns_none() {
        let c = ComposeStatsCollector::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.latest("conv-1").is_none());
    }

    #[test]
    fn record_then_latest_roundtrips() {
        let c = ComposeStatsCollector::new();
        c.record("conv-1", stats(3, 2));
        let got = c.latest("conv-1").unwrap();
        assert_eq!(got.fragments_available, 3);
        assert_eq!(got.fragments_selected, 2);
    }

    #[test]
    fn record_overwrites_prior_turn() {
        let c = ComposeStatsCollector::new();
        c.record("conv-1", stats(3, 1));
        c.record("conv-1", stats(5, 4));
        let got = c.latest("conv-1").unwrap();
        assert_eq!(got.fragments_available, 5);
        assert_eq!(got.fragments_selected, 4);
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn conversations_isolated() {
        let c = ComposeStatsCollector::new();
        c.record("a", stats(1, 1));
        c.record("b", stats(2, 2));
        assert_eq!(c.len(), 2);
        assert_eq!(c.latest("a").unwrap().fragments_available, 1);
        assert_eq!(c.latest("b").unwrap().fragments_available, 2);
    }

    #[test]
    fn clone_shares_state() {
        let c1 = ComposeStatsCollector::new();
        let c2 = c1.clone();
        c1.record("conv-1", stats(7, 0));
        assert_eq!(c2.latest("conv-1").unwrap().fragments_available, 7);
    }

    #[test]
    fn forget_removes_entry() {
        let c = ComposeStatsCollector::new();
        c.record("conv-1", stats(1, 1));
        assert!(c.forget("conv-1"));
        assert!(c.latest("conv-1").is_none());
        assert!(!c.forget("conv-1"));
    }
}
