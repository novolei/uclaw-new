//! Stateful glue between the pure-logic stability detector and the
//! `user_profile_facets` SQLite table — Sprint 1.4.
//!
//! Two types:
//!
//! - [`FacetStore`] — thin wrapper over `Arc<Mutex<Connection>>` that
//!   reads [`FacetSnapshot`]s and writes [`FacetTransition`]s. Mirrors
//!   the shape of `MemoryGraphStore` so callers don't need to know
//!   about rusqlite internals.
//! - [`LearningScheduler`] — owns a [`FacetStore`], the shared
//!   [`Buffer`] from `learning::candidate::Buffer`, and (later) the
//!   [`FacetCache`] from Sprint 1.5. Exposes one public method
//!   [`rebuild_now`](LearningScheduler::rebuild_now) that the
//!   ProactiveService tick (Sprint 1.10) calls every ~30 min.
//!
//! The scheduler does NOT decide cadence — that's the caller's job.
//! Cadence: ProactiveService tick at 60-tick intervals (default
//! 30s × 60 = 30 min). An event-driven path (60s debounced after
//! "new candidates pushed") will be added in Sprint 1.10.

use std::sync::Arc;
use std::sync::Mutex;

use rusqlite::params;
use rusqlite::Connection;

use crate::learning::candidate::{Buffer, FacetClass};
use crate::learning::stability_detector::{
    CueWeights, FacetSnapshot, FacetState, FacetTransition, RebuildOutcome, StabilityDetector,
};

// ─── FacetStore ────────────────────────────────────────────────────────

/// SQLite wrapper for the `user_profile_facets` table (V39 migration).
/// All operations short-circuit if the conn lock is poisoned so a
/// flaky read can't break the producer side.
pub struct FacetStore {
    pub(crate) conn: Arc<Mutex<Connection>>,
}

impl FacetStore {
    pub fn new(conn: Arc<Mutex<Connection>>) -> Self {
        Self { conn }
    }

    /// Read every row of `user_profile_facets` into a Vec of
    /// [`FacetSnapshot`]s. Used by [`LearningScheduler::rebuild_now`]
    /// to seed the detector with the current state of the table.
    ///
    /// Rows with unknown `class` strings are skipped (defensive
    /// against schema drift); rows with malformed `cue_families_json`
    /// fall back to empty weights.
    pub fn load_all(&self) -> Result<Vec<FacetSnapshot>, crate::error::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT facet_id, class, name, value, state, stability, \
                        cue_families_json, evidence_count, last_seen_at \
                 FROM user_profile_facets",
            )
            .map_err(crate::error::Error::Database)?;
        let rows = stmt
            .query_map([], |row| {
                let class_str: String = row.get(1)?;
                Ok((
                    row.get::<_, String>(0)?,
                    class_str,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, String>(4)?,
                    row.get::<_, f64>(5)?,
                    row.get::<_, String>(6)?,
                    row.get::<_, i64>(7)?,
                    row.get::<_, i64>(8)?,
                ))
            })
            .map_err(crate::error::Error::Database)?;

        let mut out = Vec::new();
        for r in rows.flatten() {
            let (facet_id, class_str, name, value, state_str, stability, cue_json, ev_count, last_seen) = r;
            let class = match parse_facet_class(&class_str) {
                Some(c) => c,
                None => {
                    tracing::warn!(
                        class = %class_str,
                        facet_id = %facet_id,
                        "FacetStore::load_all: unknown class string, skipping"
                    );
                    continue;
                }
            };
            let weights = serde_json::from_str::<serde_json::Value>(&cue_json)
                .ok()
                .as_ref()
                .map(CueWeights::from_json)
                .unwrap_or_default();
            out.push(FacetSnapshot {
                facet_id,
                class,
                name,
                value,
                state: FacetState::from_str(&state_str),
                stability,
                cue_weights: weights,
                evidence_count: ev_count.max(0) as u32,
                last_seen_ms: last_seen,
            });
        }
        Ok(out)
    }

    /// Apply a batch of [`FacetTransition`]s. Uses `INSERT OR
    /// REPLACE` semantics keyed on `facet_id` so new rows get
    /// inserted, existing rows updated. Caller (scheduler) supplies
    /// `now_ms` so the `created_at` / `updated_at` columns match the
    /// rebuild's wall-clock view.
    pub fn write_transitions(
        &self,
        transitions: &[FacetTransition],
        now_ms: i64,
    ) -> Result<(), crate::error::Error> {
        if transitions.is_empty() {
            return Ok(());
        }
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let tx = conn.transaction().map_err(crate::error::Error::Database)?;
        for t in transitions {
            // INSERT OR REPLACE on facet_id PK. We preserve created_at
            // when the row already exists by reading it first; if no
            // row exists, created_at = now_ms.
            let existing_created_at: Option<i64> = tx
                .query_row(
                    "SELECT created_at FROM user_profile_facets WHERE facet_id = ?1",
                    params![t.facet_id],
                    |r| r.get(0),
                )
                .ok();
            let created_at = existing_created_at.unwrap_or(now_ms);
            tx.execute(
                "INSERT OR REPLACE INTO user_profile_facets \
                 (facet_id, class, name, value, state, stability, \
                  cue_families_json, evidence_count, last_seen_at, \
                  created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                params![
                    t.facet_id,
                    t.class.as_str(),
                    t.name,
                    t.value,
                    t.new_state.as_str(),
                    t.new_stability,
                    t.new_cue_weights.to_json().to_string(),
                    t.new_evidence_count as i64,
                    t.new_last_seen_ms,
                    created_at,
                    now_ms,
                ],
            )
            .map_err(crate::error::Error::Database)?;
        }
        tx.commit().map_err(crate::error::Error::Database)?;
        Ok(())
    }
}

fn parse_facet_class(s: &str) -> Option<FacetClass> {
    match s {
        "style" => Some(FacetClass::Style),
        "identity" => Some(FacetClass::Identity),
        "tooling" => Some(FacetClass::Tooling),
        "veto" => Some(FacetClass::Veto),
        "goal" => Some(FacetClass::Goal),
        "channel" => Some(FacetClass::Channel),
        _ => None,
    }
}

// ─── LearningScheduler ─────────────────────────────────────────────────

/// Owns the producer buffer + the [`FacetStore`]. The ProactiveService
/// tick (Sprint 1.10) holds an `Arc<LearningScheduler>` and calls
/// [`rebuild_now`](Self::rebuild_now) every 60 ticks.
///
/// In Sprint 1.5 a `FacetCache` Arc will be added so the prompt
/// section can read active facets without a SQL round-trip.
pub struct LearningScheduler {
    store: Arc<FacetStore>,
    buffer: Arc<Buffer>,
}

impl LearningScheduler {
    pub fn new(store: Arc<FacetStore>, buffer: Arc<Buffer>) -> Self {
        Self { store, buffer }
    }

    /// Borrow the underlying FacetStore handle. ProactiveService uses
    /// this to refresh FacetCache after `rebuild_now` without
    /// constructing a parallel store.
    pub fn store_handle(&self) -> Arc<FacetStore> {
        self.store.clone()
    }

    /// Borrow the underlying producer buffer. Used by IPC paths
    /// (Sprint 1.10) that want to push a candidate synchronously
    /// from a tauri command handler.
    pub fn buffer_handle(&self) -> Arc<Buffer> {
        self.buffer.clone()
    }

    /// One rebuild pass. Drains the buffer, reads current snapshots,
    /// runs the detector, writes transitions back.
    ///
    /// Returns the [`RebuildOutcome`] for telemetry. Errors during DB
    /// read/write surface to the caller — ProactiveService logs and
    /// continues (a flaky rebuild does not crash the tick loop).
    pub fn rebuild_now(&self, now_ms: i64) -> Result<RebuildOutcome, crate::error::Error> {
        let candidates = self.buffer.drain();
        let snapshots = self.store.load_all()?;
        let (transitions, outcome) =
            StabilityDetector::rebuild(snapshots, candidates, now_ms);
        self.store.write_transitions(&transitions, now_ms)?;
        Ok(outcome)
    }

    /// Test/IPC entry point that doesn't drain the buffer (allows
    /// "preview what the rebuild would produce" without consuming).
    /// Returns transitions + outcome; **does NOT write to DB**.
    #[allow(dead_code)]
    pub fn dry_run(&self, now_ms: i64) -> Result<(Vec<FacetTransition>, RebuildOutcome), crate::error::Error> {
        // Snapshot then peek-but-don't-drain. We achieve this by
        // draining + immediately re-pushing — Buffer doesn't have a
        // peek API to keep its surface small.
        let candidates = self.buffer.drain();
        let snapshots = self.store.load_all()?;
        // Clone candidates so we can re-push originals.
        let candidates_for_run = candidates.clone();
        for c in candidates {
            self.buffer.push(c);
        }
        Ok(StabilityDetector::rebuild(snapshots, candidates_for_run, now_ms))
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::learning::candidate::{CueFamily, EvidenceRef, LearningCandidate};

    fn fresh_store() -> Arc<FacetStore> {
        let conn = Connection::open_in_memory().unwrap();
        // Run migrations through V39.
        crate::db::migrations::run(&conn).unwrap();
        Arc::new(FacetStore::new(Arc::new(Mutex::new(conn))))
    }

    fn cand(class: FacetClass, name: &str, value: &str, cue: CueFamily) -> LearningCandidate {
        let mut c = LearningCandidate::new(
            class,
            name,
            value,
            cue,
            EvidenceRef::Manual { note: "t".into() },
        );
        // Pin to a deterministic timestamp for test reproducibility.
        c.observed_at_ms = 1_700_000_000_000;
        c
    }

    // ─── FacetStore ───────────────────────────────────────────────

    #[test]
    fn facet_store_load_all_returns_empty_on_fresh_db() {
        let store = fresh_store();
        let snaps = store.load_all().unwrap();
        assert!(snaps.is_empty());
    }

    #[test]
    fn facet_store_write_then_load_round_trips() {
        let store = fresh_store();
        let now = 1_700_000_000_000i64;
        let mut weights = CueWeights::default();
        weights.add(CueFamily::Explicit, 1.0);
        let t = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Tooling,
            name: "editor".into(),
            value: "helix".into(),
            new_state: FacetState::Active,
            new_stability: 1.8,
            new_cue_weights: weights.clone(),
            new_evidence_count: 3,
            new_last_seen_ms: now - 1000,
        };
        store.write_transitions(&[t.clone()], now).unwrap();
        let snaps = store.load_all().unwrap();
        assert_eq!(snaps.len(), 1);
        let s = &snaps[0];
        assert_eq!(s.facet_id, "f1");
        assert_eq!(s.class, FacetClass::Tooling);
        assert_eq!(s.name, "editor");
        assert_eq!(s.value, "helix");
        assert_eq!(s.state, FacetState::Active);
        assert!((s.stability - 1.8).abs() < 1e-9);
        assert_eq!(s.evidence_count, 3);
        assert_eq!(s.last_seen_ms, now - 1000);
        assert!((s.cue_weights.explicit - 1.0).abs() < 1e-9);
    }

    #[test]
    fn facet_store_write_transition_updates_existing() {
        // First write creates the row; second with same facet_id updates.
        let store = fresh_store();
        let now = 1_700_000_000_000i64;
        let mut w = CueWeights::default();
        w.add(CueFamily::Behavioral, 0.7);
        let t1 = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Style,
            name: "verbosity".into(),
            value: "terse".into(),
            new_state: FacetState::Candidate,
            new_stability: 0.5,
            new_cue_weights: w.clone(),
            new_evidence_count: 1,
            new_last_seen_ms: now,
        };
        store.write_transitions(&[t1], now).unwrap();
        let mut w2 = w.clone();
        w2.add(CueFamily::Explicit, 1.0);
        let t2 = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Style,
            name: "verbosity".into(),
            value: "terse".into(),
            new_state: FacetState::Active,
            new_stability: 2.1,
            new_cue_weights: w2,
            new_evidence_count: 3,
            new_last_seen_ms: now + 1000,
        };
        store.write_transitions(&[t2], now + 1000).unwrap();
        let snaps = store.load_all().unwrap();
        assert_eq!(snaps.len(), 1, "same facet_id must update, not duplicate");
        assert_eq!(snaps[0].state, FacetState::Active);
        assert_eq!(snaps[0].evidence_count, 3);
    }

    #[test]
    fn facet_store_empty_transitions_is_noop() {
        let store = fresh_store();
        store.write_transitions(&[], 0).unwrap();
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn facet_store_skips_rows_with_unknown_class() {
        // Manually insert a malformed row, then load_all should skip it.
        let store = fresh_store();
        {
            let conn = store.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO user_profile_facets \
                 (facet_id, class, name, value, state, stability, \
                  cue_families_json, evidence_count, last_seen_at, \
                  created_at, updated_at) \
                 VALUES ('bad', 'mystery_class', 'x', 'y', 'active', 1.0, \
                         '{}', 1, 0, 0, 0)",
                [],
            )
            .unwrap();
        }
        let snaps = store.load_all().unwrap();
        assert!(snaps.is_empty(), "unknown class string must be skipped, not parsed as default");
    }

    #[test]
    fn facet_store_preserves_created_at_on_update() {
        // created_at should NOT change when the same facet_id is rewritten.
        let store = fresh_store();
        let initial_now = 1_700_000_000_000i64;
        let later_now = initial_now + 100_000;
        let mut w = CueWeights::default();
        w.add(CueFamily::Explicit, 1.0);
        let t = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Identity,
            name: "name".into(),
            value: "Alice".into(),
            new_state: FacetState::Active,
            new_stability: 2.0,
            new_cue_weights: w.clone(),
            new_evidence_count: 2,
            new_last_seen_ms: initial_now,
        };
        store.write_transitions(&[t.clone()], initial_now).unwrap();
        // Write again at later_now.
        store.write_transitions(&[t], later_now).unwrap();
        // created_at should be initial_now, updated_at should be later_now.
        let conn = store.conn.lock().unwrap();
        let (created_at, updated_at): (i64, i64) = conn
            .query_row(
                "SELECT created_at, updated_at FROM user_profile_facets WHERE facet_id = 'f1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(created_at, initial_now);
        assert_eq!(updated_at, later_now);
    }

    // ─── LearningScheduler ────────────────────────────────────────

    #[test]
    fn scheduler_rebuild_now_empty_inputs_is_noop() {
        let store = fresh_store();
        let buf = Arc::new(Buffer::new(10));
        let sched = LearningScheduler::new(store.clone(), buf);
        let outcome = sched.rebuild_now(1_700_000_000_000).unwrap();
        assert_eq!(outcome.total, 0);
        assert!(store.load_all().unwrap().is_empty());
    }

    #[test]
    fn scheduler_rebuild_now_consumes_buffer_and_writes_rows() {
        let store = fresh_store();
        let buf = Arc::new(Buffer::new(10));
        buf.push(cand(FacetClass::Tooling, "editor", "helix", CueFamily::Explicit));
        buf.push(cand(FacetClass::Identity, "name", "Alice", CueFamily::Explicit));
        let sched = LearningScheduler::new(store.clone(), buf.clone());
        let outcome = sched.rebuild_now(1_700_000_000_000).unwrap();
        assert_eq!(outcome.total, 2);
        assert_eq!(outcome.promoted_to_active, 2, "explicit cues both promote to active");
        assert!(buf.is_empty(), "rebuild drains the buffer");
        assert_eq!(store.load_all().unwrap().len(), 2);
    }

    #[test]
    fn scheduler_rebuild_now_merges_with_existing_facets() {
        // Pre-seed a facet via direct store write, then push a new candidate
        // and rebuild — should aggregate not duplicate.
        let store = fresh_store();
        let buf = Arc::new(Buffer::new(10));
        let now = 1_700_000_000_000i64;
        let mut w = CueWeights::default();
        w.add(CueFamily::Behavioral, 0.7);
        let t = FacetTransition {
            facet_id: "f1".into(),
            class: FacetClass::Style,
            name: "verbosity".into(),
            value: "terse".into(),
            new_state: FacetState::Candidate,
            new_stability: 0.5,
            new_cue_weights: w,
            new_evidence_count: 1,
            new_last_seen_ms: now - 1000,
        };
        store.write_transitions(&[t], now - 1000).unwrap();
        buf.push(cand(FacetClass::Style, "verbosity", "terse", CueFamily::Explicit));
        let sched = LearningScheduler::new(store.clone(), buf);
        let outcome = sched.rebuild_now(now).unwrap();
        assert_eq!(outcome.total, 1, "same key combines, not duplicates");
        let snaps = store.load_all().unwrap();
        assert_eq!(snaps.len(), 1);
        assert_eq!(snaps[0].evidence_count, 2, "1 existing + 1 new");
        assert_eq!(snaps[0].state, FacetState::Active, "explicit boost promotes");
    }

    #[test]
    fn scheduler_dry_run_does_not_drain_buffer_or_write() {
        let store = fresh_store();
        let buf = Arc::new(Buffer::new(10));
        buf.push(cand(FacetClass::Tooling, "editor", "helix", CueFamily::Explicit));
        let sched = LearningScheduler::new(store.clone(), buf.clone());
        let (transitions, outcome) = sched.dry_run(1_700_000_000_000).unwrap();
        assert_eq!(transitions.len(), 1);
        assert_eq!(outcome.promoted_to_active, 1);
        // Buffer NOT drained.
        assert_eq!(buf.len(), 1, "dry_run must not drain the buffer");
        // DB NOT written.
        assert!(store.load_all().unwrap().is_empty(), "dry_run must not write");
    }

    #[test]
    fn scheduler_concurrent_pushes_during_rebuild_are_handled() {
        // Push from a producer thread while rebuild runs. The
        // rebuild only drains what was in the buffer at drain-time;
        // the late pushes survive in the buffer for the next rebuild.
        use std::thread;
        let store = fresh_store();
        let buf = Arc::new(Buffer::new(100));
        // Seed the buffer with 5 candidates.
        for i in 0..5 {
            buf.push(cand(FacetClass::Tooling, &format!("t{}", i), "v", CueFamily::Explicit));
        }
        let sched = LearningScheduler::new(store.clone(), buf.clone());
        let buf_clone = buf.clone();
        let producer = thread::spawn(move || {
            // Late pushes after drain — they should land in buffer.
            for i in 5..10 {
                buf_clone.push(cand(FacetClass::Tooling, &format!("t{}", i), "v", CueFamily::Explicit));
            }
        });
        let _ = sched.rebuild_now(1_700_000_000_000);
        producer.join().unwrap();
        // We can't guarantee how the race resolves, but the post-state
        // must be self-consistent: buffer.len() + DB facets >= 5,
        // and total facets is bounded by 10.
        let facets_in_db = store.load_all().unwrap().len();
        let still_buffered = buf.len();
        assert!(
            facets_in_db + still_buffered >= 5 && facets_in_db + still_buffered <= 10,
            "post-state: db={}, buffer={}, sum must be in [5, 10]",
            facets_in_db,
            still_buffered
        );
    }
}
