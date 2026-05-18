//! Symphony stall detection.
//!
//! A node is considered "alive" while its `Heartbeat` registry has an entry
//! more recent than `stall_timeout_ms`. The per-node executor (`node_run`)
//! calls `Heartbeat::touch(node_id)` on every LLM-streaming partial-text
//! callback (via the `SymphonyHeartbeatSink: StreamingHandle` impl) and once
//! more on `on_usage`. Stall checking runs on the `RunActor` tick.
//!
//! Two reasons stalls exist as a Symphony-level concern (separate from the
//! LLM-layer `STREAM_STALL_TIMEOUT = 45s` in `llm/stream_error.rs`):
//! 1. A wedged tool call may not surface at the LLM layer (the LLM has
//!    already completed; the tool is what's stuck).
//! 2. A crashed agent loop task leaves no LLM stream open; only the absence
//!    of heartbeats detects it.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::Utc;

/// Thread-safe registry of "last seen alive" timestamps keyed by
/// `(run_id, node_id)`. Cheap clone (`Arc`-wrapped at the call site).
#[derive(Debug, Default)]
pub struct Heartbeat {
    /// (run_id, node_id) → last_ms
    inner: Mutex<HashMap<(String, String), i64>>,
}

impl Heartbeat {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a heartbeat at "now" for the given node.
    pub fn touch(&self, run_id: &str, node_id: &str) {
        self.touch_at(run_id, node_id, Utc::now().timestamp_millis());
    }

    /// Record a heartbeat at an explicit timestamp (useful for tests +
    /// recovery, where we restore last_heartbeat from DB).
    pub fn touch_at(&self, run_id: &str, node_id: &str, ms: i64) {
        let mut g = self.inner.lock().unwrap();
        g.insert((run_id.to_string(), node_id.to_string()), ms);
    }

    /// Drop a node's entry (e.g. when it reaches a terminal state).
    pub fn forget(&self, run_id: &str, node_id: &str) {
        let mut g = self.inner.lock().unwrap();
        g.remove(&(run_id.to_string(), node_id.to_string()));
    }

    /// Last seen timestamp for a node, or `None` if not tracked.
    pub fn last_seen(&self, run_id: &str, node_id: &str) -> Option<i64> {
        let g = self.inner.lock().unwrap();
        g.get(&(run_id.to_string(), node_id.to_string())).copied()
    }

    /// Snapshot of (run_id, node_id) pairs whose last heartbeat is older
    /// than `now_ms - threshold_ms`. Caller decides what to do — typically
    /// the `RunActor` will transition each to `Stalled` and queue a retry.
    pub fn check_stalls(&self, now_ms: i64, threshold_ms: u64) -> Vec<(String, String)> {
        let g = self.inner.lock().unwrap();
        let cutoff = now_ms.saturating_sub(threshold_ms as i64);
        g.iter()
            .filter(|(_, &t)| t < cutoff)
            .map(|((r, n), _)| (r.clone(), n.clone()))
            .collect()
    }

    /// How many nodes are currently being tracked (debug / metrics).
    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn touch_and_last_seen_roundtrip() {
        let hb = Heartbeat::new();
        hb.touch_at("r", "n", 1000);
        assert_eq!(hb.last_seen("r", "n"), Some(1000));
        hb.touch_at("r", "n", 2000);
        assert_eq!(hb.last_seen("r", "n"), Some(2000));
    }

    #[test]
    fn forget_removes_entry() {
        let hb = Heartbeat::new();
        hb.touch_at("r", "n", 1000);
        hb.forget("r", "n");
        assert!(hb.last_seen("r", "n").is_none());
        assert!(hb.is_empty());
    }

    #[test]
    fn check_stalls_returns_overdue_only() {
        let hb = Heartbeat::new();
        hb.touch_at("r", "alive", 9_500);
        hb.touch_at("r", "stalled", 1_000);
        // Threshold = 5000ms, now = 10_000. cutoff = 5_000 → 'stalled' (1000) is overdue.
        let mut got = hb.check_stalls(10_000, 5_000);
        got.sort();
        assert_eq!(got, vec![("r".into(), "stalled".into())]);
    }

    #[test]
    fn empty_when_nothing_touched() {
        let hb = Heartbeat::new();
        assert_eq!(hb.check_stalls(10_000, 5_000), Vec::<(String, String)>::new());
        assert_eq!(hb.len(), 0);
    }

    #[test]
    fn handles_threshold_at_exact_boundary() {
        let hb = Heartbeat::new();
        hb.touch_at("r", "edge", 5_000);
        // now=10_000 threshold=5_000 → cutoff=5_000; entry at exactly 5_000
        // is NOT < cutoff, so it survives.
        assert!(hb.check_stalls(10_000, 5_000).is_empty());
        hb.touch_at("r", "edge", 4_999);
        assert_eq!(hb.check_stalls(10_000, 5_000).len(), 1);
    }
}
