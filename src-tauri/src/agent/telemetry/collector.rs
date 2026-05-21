//! `TokenBudgetCollector` — in-memory per-task latest snapshot store.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use crate::agent::token_budget::TokenBudgetSnapshot;

/// Latest-snapshot-per-task store. Each task overwrites its previous
/// snapshot on each turn.
///
/// Cloneable — share via `AppState` across Tauri commands. Internal
/// `RwLock` allows concurrent reads (Tauri command threads) while
/// writes (agent loop) take an exclusive lock briefly.
#[derive(Debug, Clone, Default)]
pub struct TokenBudgetCollector {
    inner: Arc<RwLock<HashMap<String, TokenBudgetSnapshot>>>,
}

impl TokenBudgetCollector {
    pub fn new() -> Self {
        Self::default()
    }

    /// Replace `task_id`'s snapshot with `snapshot`.
    pub fn record(&self, snapshot: TokenBudgetSnapshot) {
        let mut map = self.inner.write().expect("collector lock poisoned");
        map.insert(snapshot.task_id.clone(), snapshot);
    }

    /// Return the latest snapshot for `task_id` (cloned).
    pub fn latest(&self, task_id: &str) -> Option<TokenBudgetSnapshot> {
        let map = self.inner.read().expect("collector lock poisoned");
        map.get(task_id).cloned()
    }

    /// All task ids currently tracked.
    pub fn task_ids(&self) -> Vec<String> {
        let map = self.inner.read().expect("collector lock poisoned");
        let mut ids: Vec<String> = map.keys().cloned().collect();
        ids.sort();
        ids
    }

    /// Number of tasks tracked.
    pub fn len(&self) -> usize {
        self.inner.read().expect("collector lock poisoned").len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.read().expect("collector lock poisoned").is_empty()
    }

    /// Drop the snapshot for a task — call when the task completes
    /// and its UI panel is closed.
    pub fn forget(&self, task_id: &str) -> bool {
        self.inner
            .write()
            .expect("collector lock poisoned")
            .remove(task_id)
            .is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_snapshot(task_id: &str, turn: u32) -> TokenBudgetSnapshot {
        TokenBudgetSnapshot::new(
            task_id,
            turn,
            "anthropic",
            "claude-sonnet-4-5",
            "2026-05-21T12:00:00Z",
        )
    }

    #[test]
    fn empty_collector_returns_none() {
        let c = TokenBudgetCollector::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.latest("t1").is_none());
        assert!(c.task_ids().is_empty());
    }

    #[test]
    fn record_then_latest_roundtrips() {
        let c = TokenBudgetCollector::new();
        c.record(fresh_snapshot("t1", 1));
        let got = c.latest("t1").unwrap();
        assert_eq!(got.task_id, "t1");
        assert_eq!(got.turn, 1);
    }

    #[test]
    fn record_overwrites_prior_turn() {
        let c = TokenBudgetCollector::new();
        c.record(fresh_snapshot("t1", 1));
        c.record(fresh_snapshot("t1", 5));
        let got = c.latest("t1").unwrap();
        assert_eq!(got.turn, 5);
        // Only one entry per task.
        assert_eq!(c.len(), 1);
    }

    #[test]
    fn multiple_tasks_isolated() {
        let c = TokenBudgetCollector::new();
        c.record(fresh_snapshot("t1", 1));
        c.record(fresh_snapshot("t2", 1));
        c.record(fresh_snapshot("t3", 1));
        assert_eq!(c.len(), 3);
        assert_eq!(c.task_ids(), vec!["t1", "t2", "t3"]);
    }

    #[test]
    fn task_ids_sorted() {
        let c = TokenBudgetCollector::new();
        c.record(fresh_snapshot("z", 1));
        c.record(fresh_snapshot("a", 1));
        c.record(fresh_snapshot("m", 1));
        assert_eq!(c.task_ids(), vec!["a", "m", "z"]);
    }

    #[test]
    fn forget_removes_entry() {
        let c = TokenBudgetCollector::new();
        c.record(fresh_snapshot("t1", 1));
        assert!(c.forget("t1"));
        assert!(c.latest("t1").is_none());
        assert!(!c.forget("t1"));
    }

    #[test]
    fn clone_shares_state() {
        let c1 = TokenBudgetCollector::new();
        let c2 = c1.clone();
        c1.record(fresh_snapshot("t1", 1));
        // c2 sees c1's write — same Arc<RwLock> inside.
        assert_eq!(c2.latest("t1").unwrap().task_id, "t1");
    }

    #[tokio::test]
    async fn concurrent_record_safe() {
        let c = TokenBudgetCollector::new();
        let mut handles = Vec::new();
        for i in 0..50 {
            let c = c.clone();
            handles.push(tokio::spawn(async move {
                c.record(fresh_snapshot(&format!("t{i}"), i as u32));
            }));
        }
        for h in handles {
            h.await.unwrap();
        }
        assert_eq!(c.len(), 50);
    }
}
