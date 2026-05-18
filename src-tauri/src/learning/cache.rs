//! Shared facet cache — Sprint 1.5.
//!
//! `FacetCache` is the read-side counterpart of [`FacetStore`]. Sprint
//! 1.4's `LearningScheduler::rebuild_now` writes back to SQLite; this
//! cache holds the same data in memory grouped by class so the prompt
//! section (Sprint 1.6) can render the system prompt without a SQL
//! round-trip on every agent turn.
//!
//! Lifecycle:
//! 1. AppState bootstrap creates an empty cache.
//! 2. ProactiveService (Sprint 1.10) calls [`FacetCache::refresh_from`]
//!    right after every `rebuild_now` so the cache mirrors what the
//!    scheduler just wrote.
//! 3. Prompt section (Sprint 1.6) calls [`FacetCache::active_by_class`]
//!    on every agent turn.
//!
//! Concurrency: `Arc<RwLock<...>>`. Reads are cheap and never block
//! each other; writes happen at most every 30 min (scheduler tick).
//!
//! [`FacetStore`]: super::scheduler::FacetStore

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::RwLock;

use crate::learning::candidate::FacetClass;
use crate::learning::scheduler::FacetStore;
use crate::learning::stability_detector::{FacetSnapshot, FacetState};

// ─── FacetCache ────────────────────────────────────────────────────────

/// In-memory mirror of `user_profile_facets` grouped by class +
/// filtered by state. Optimised for the hot read path
/// (prompt-building on every agent turn).
///
/// Internally `Arc<RwLock<HashMap>>` — clone is cheap (one Arc bump),
/// and the same `Arc<FacetCache>` is shared by AppState + scheduler +
/// prompt section without recursive lock contention.
pub struct FacetCache {
    inner: Arc<RwLock<CacheInner>>,
}

#[derive(Default)]
struct CacheInner {
    /// All snapshots, keyed by class. Within each class the Vec is
    /// sorted by stability DESC at refresh time so callers don't
    /// need to re-sort on every read.
    by_class: HashMap<FacetClass, Vec<FacetSnapshot>>,
    /// Number of facets after the last refresh (across all classes).
    /// Cached for cheap O(1) `len()`.
    total: usize,
    /// Epoch ms of the last successful refresh. Surfaced via UI as
    /// "Profile last rebuilt N min ago".
    last_refreshed_at_ms: i64,
}

impl FacetCache {
    /// New empty cache. AppState bootstrap calls this once; the first
    /// refresh happens after the first ProactiveService tick.
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(CacheInner::default())),
        }
    }

    /// Rebuild the cache from a fresh `load_all()` against the store.
    /// Called by ProactiveService after each `rebuild_now`. Errors
    /// surface to the caller; on Err the cache keeps the previous
    /// state (a flaky DB read must not zero out the prompt).
    pub fn refresh_from(
        &self,
        store: &FacetStore,
        now_ms: i64,
    ) -> Result<usize, crate::error::Error> {
        let snapshots = store.load_all()?;
        self.replace_with(snapshots, now_ms);
        Ok(self.len())
    }

    /// Direct setter — used by tests + by hot-path refresh after
    /// `LearningScheduler::rebuild_now` returns to avoid a second
    /// `load_all()`. (Sprint 1.10 will optimize by passing the
    /// `Vec<FacetTransition>` here directly.)
    pub fn replace_with(&self, snapshots: Vec<FacetSnapshot>, now_ms: i64) {
        let mut grouped: HashMap<FacetClass, Vec<FacetSnapshot>> = HashMap::new();
        let total = snapshots.len();
        for s in snapshots {
            grouped.entry(s.class).or_default().push(s);
        }
        // Sort each class by stability DESC so the prompt section can
        // top-K without re-sorting.
        for vec in grouped.values_mut() {
            vec.sort_by(|a, b| {
                b.stability
                    .partial_cmp(&a.stability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        let mut inner = match self.inner.write() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        inner.by_class = grouped;
        inner.total = total;
        inner.last_refreshed_at_ms = now_ms;
    }

    /// Active-state facets in this class, in stability DESC order.
    /// Hot path — called once per agent turn from the prompt builder.
    /// Returns a cloned Vec so the read lock is dropped immediately.
    pub fn active_by_class(&self, class: FacetClass) -> Vec<FacetSnapshot> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        inner
            .by_class
            .get(&class)
            .map(|v| {
                v.iter()
                    .filter(|s| s.state == FacetState::Active)
                    .cloned()
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Every active facet across all classes, sorted by class then
    /// stability. Used by the "Profile" UI and the manual-list IPC.
    pub fn all_active(&self) -> Vec<FacetSnapshot> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        let mut out: Vec<FacetSnapshot> = inner
            .by_class
            .values()
            .flat_map(|v| v.iter().filter(|s| s.state == FacetState::Active).cloned())
            .collect();
        // Deterministic ordering: class (by enum-discriminant str), then stability DESC.
        out.sort_by(|a, b| {
            a.class
                .as_str()
                .cmp(b.class.as_str())
                .then_with(|| {
                    b.stability
                        .partial_cmp(&a.stability)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        out
    }

    /// All facets regardless of state. Used by IPC list endpoint
    /// (settings UI "see all my facets") + the "dismiss" affordance.
    pub fn all(&self) -> Vec<FacetSnapshot> {
        let inner = match self.inner.read() {
            Ok(g) => g,
            Err(p) => p.into_inner(),
        };
        inner.by_class.values().flatten().cloned().collect()
    }

    /// Total facet count (all states). O(1).
    pub fn len(&self) -> usize {
        self.inner
            .read()
            .map(|g| g.total)
            .unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Last refresh wall-clock (epoch ms). Surfaced via the UI as
    /// "Profile rebuilt N min ago".
    pub fn last_refreshed_at_ms(&self) -> i64 {
        self.inner
            .read()
            .map(|g| g.last_refreshed_at_ms)
            .unwrap_or(0)
    }
}

impl Default for FacetCache {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::stability_detector::{CueWeights, FacetState};

    fn snap(class: FacetClass, name: &str, state: FacetState, stability: f64) -> FacetSnapshot {
        FacetSnapshot {
            facet_id: format!("{}-{}", class.as_str(), name),
            class,
            name: name.into(),
            value: "v".into(),
            state,
            stability,
            cue_weights: CueWeights::default(),
            evidence_count: 1,
            last_seen_ms: 1_700_000_000_000,
        }
    }

    #[test]
    fn empty_cache_returns_empty() {
        let c = FacetCache::new();
        assert!(c.is_empty());
        assert_eq!(c.len(), 0);
        assert!(c.active_by_class(FacetClass::Tooling).is_empty());
        assert!(c.all_active().is_empty());
        assert_eq!(c.last_refreshed_at_ms(), 0);
    }

    #[test]
    fn replace_with_groups_by_class() {
        let c = FacetCache::new();
        c.replace_with(
            vec![
                snap(FacetClass::Tooling, "editor", FacetState::Active, 2.5),
                snap(FacetClass::Tooling, "shell", FacetState::Active, 2.0),
                snap(FacetClass::Style, "verbosity", FacetState::Active, 1.8),
            ],
            1_700_000_000_000,
        );
        assert_eq!(c.len(), 3);
        assert_eq!(c.active_by_class(FacetClass::Tooling).len(), 2);
        assert_eq!(c.active_by_class(FacetClass::Style).len(), 1);
        assert!(c.active_by_class(FacetClass::Identity).is_empty());
    }

    #[test]
    fn active_by_class_filters_out_non_active() {
        let c = FacetCache::new();
        c.replace_with(
            vec![
                snap(FacetClass::Tooling, "editor", FacetState::Active, 2.5),
                snap(FacetClass::Tooling, "shell", FacetState::Provisional, 1.0),
                snap(FacetClass::Tooling, "browser", FacetState::Candidate, 0.5),
                snap(FacetClass::Tooling, "deprecated", FacetState::Forgotten, 0.0),
            ],
            0,
        );
        let active = c.active_by_class(FacetClass::Tooling);
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "editor");
    }

    #[test]
    fn active_by_class_is_sorted_by_stability_desc() {
        let c = FacetCache::new();
        // Insert out of order; cache should sort on replace_with.
        c.replace_with(
            vec![
                snap(FacetClass::Tooling, "low", FacetState::Active, 1.6),
                snap(FacetClass::Tooling, "high", FacetState::Active, 3.2),
                snap(FacetClass::Tooling, "mid", FacetState::Active, 2.1),
            ],
            0,
        );
        let active = c.active_by_class(FacetClass::Tooling);
        assert_eq!(active[0].name, "high");
        assert_eq!(active[1].name, "mid");
        assert_eq!(active[2].name, "low");
    }

    #[test]
    fn all_active_returns_across_classes_with_deterministic_ordering() {
        let c = FacetCache::new();
        c.replace_with(
            vec![
                snap(FacetClass::Tooling, "editor", FacetState::Active, 2.5),
                snap(FacetClass::Style, "verbosity", FacetState::Active, 2.8),
                snap(FacetClass::Identity, "name", FacetState::Active, 3.0),
            ],
            0,
        );
        let all = c.all_active();
        assert_eq!(all.len(), 3);
        // Sort key is (class.as_str(), -stability).
        // Class strings: "identity" < "style" < "tooling" alphabetically.
        assert_eq!(all[0].class, FacetClass::Identity);
        assert_eq!(all[1].class, FacetClass::Style);
        assert_eq!(all[2].class, FacetClass::Tooling);
    }

    #[test]
    fn replace_with_overwrites_previous_state() {
        let c = FacetCache::new();
        c.replace_with(
            vec![snap(FacetClass::Tooling, "editor", FacetState::Active, 2.0)],
            1000,
        );
        c.replace_with(
            vec![snap(FacetClass::Style, "verbosity", FacetState::Active, 1.7)],
            2000,
        );
        assert!(
            c.active_by_class(FacetClass::Tooling).is_empty(),
            "second replace must drop first snapshot"
        );
        assert_eq!(c.active_by_class(FacetClass::Style).len(), 1);
        assert_eq!(c.last_refreshed_at_ms(), 2000);
    }

    #[test]
    fn refresh_from_loads_via_store() {
        // Integration with FacetStore — write a row, refresh cache, read back.
        use crate::learning::scheduler::FacetStore;
        use crate::learning::stability_detector::FacetTransition;
        use rusqlite::Connection;
        use std::sync::Mutex;

        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let store = FacetStore::new(Arc::new(Mutex::new(conn)));
        let mut w = CueWeights::default();
        w.add(crate::learning::candidate::CueFamily::Explicit, 1.0);
        let t = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Tooling,
            name: "editor".into(),
            value: "helix".into(),
            new_state: FacetState::Active,
            new_stability: 2.5,
            new_cue_weights: w,
            new_evidence_count: 3,
            new_last_seen_ms: 1_700_000_000_000,
        };
        store.write_transitions(&[t], 1_700_000_000_000).unwrap();

        let c = FacetCache::new();
        let n = c.refresh_from(&store, 1_700_000_001_000).unwrap();
        assert_eq!(n, 1);
        assert_eq!(c.active_by_class(FacetClass::Tooling).len(), 1);
        assert_eq!(c.last_refreshed_at_ms(), 1_700_000_001_000);
    }

    #[test]
    fn cache_handles_concurrent_reads() {
        // Sanity-check: many concurrent readers don't deadlock.
        use std::thread;
        let c = Arc::new(FacetCache::new());
        c.replace_with(
            vec![snap(FacetClass::Tooling, "editor", FacetState::Active, 2.0)],
            0,
        );
        let mut handles = vec![];
        for _ in 0..8 {
            let cc = c.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let _ = cc.active_by_class(FacetClass::Tooling);
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        // Cache still consistent after the burst.
        assert_eq!(c.active_by_class(FacetClass::Tooling).len(), 1);
    }
}
