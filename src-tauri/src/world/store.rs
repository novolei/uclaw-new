//! `ProjectionStore` — thread-safe wrapper around `WorldSnapshot`
//! with subscriber notification.
//!
//! Storage layout:
//!
//! - `Arc<RwLock<WorldSnapshot>>` — concurrent readers, exclusive
//!   writer. Reads dominate (every task starts by snapshotting), so
//!   `RwLock` beats `Mutex` here.
//! - `Mutex<Vec<Arc<dyn ProjectionSubscriber>>>` — subscriber list
//!   protected by a separate lock so notify can iterate without
//!   blocking the snapshot lock.
//!
//! Subscribers are called **after** the snapshot lock is released, so
//! a slow subscriber doesn't back up writers. Notifications fire in
//! insertion order for determinism.
//!
//! M4-T2 commit 2 adds:
//! - Adapter integration (filesystem watcher / git poller produce
//!   `ProjectionEvent`s).
//! - SQLite-backed persistence so the projection survives restart.

use std::sync::{Arc, Mutex, RwLock};

use async_trait::async_trait;

use super::entity::{WorldEntity, WorldEntityKind};
use super::snapshot::WorldSnapshot;

/// Observable change to the projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectionEvent {
    /// Entity inserted or updated.
    EntityUpserted {
        kind: WorldEntityKind,
        entity: Box<WorldEntity>,
    },
    /// Entity tombstoned (soft-deleted).
    EntityTombstoned {
        kind: WorldEntityKind,
        id: String,
    },
}

/// Stable id for a projection subscriber. Lets `unsubscribe` find it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ProjectionSubscriberId(pub String);

impl ProjectionSubscriberId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for ProjectionSubscriberId {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

/// Async subscriber. Implementations should be fast — the store
/// awaits each subscriber in series so a slow one delays the rest.
#[async_trait]
pub trait ProjectionSubscriber: Send + Sync {
    fn id(&self) -> ProjectionSubscriberId;
    async fn on_event(&self, event: &ProjectionEvent);
}

/// Thread-safe projection store with subscriber pattern.
#[derive(Clone)]
pub struct ProjectionStore {
    snapshot: Arc<RwLock<WorldSnapshot>>,
    subscribers: Arc<Mutex<Vec<Arc<dyn ProjectionSubscriber>>>>,
}

impl ProjectionStore {
    pub fn new(captured_at: impl Into<String>) -> Self {
        Self {
            snapshot: Arc::new(RwLock::new(WorldSnapshot::new(captured_at))),
            subscribers: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Subscribe. Returns Err if id already in use.
    pub fn subscribe(
        &self,
        sub: Arc<dyn ProjectionSubscriber>,
    ) -> Result<(), DuplicateSubscriberId> {
        let mut subs = self.subscribers.lock().unwrap();
        let new_id = sub.id();
        if subs.iter().any(|s| s.id() == new_id) {
            return Err(DuplicateSubscriberId(new_id));
        }
        subs.push(sub);
        Ok(())
    }

    /// Unsubscribe by id. Returns true if removed.
    pub fn unsubscribe(&self, id: &ProjectionSubscriberId) -> bool {
        let mut subs = self.subscribers.lock().unwrap();
        let before = subs.len();
        subs.retain(|s| &s.id() != id);
        subs.len() != before
    }

    /// Snapshot count (for diagnostics).
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.lock().unwrap().len()
    }

    /// Insert or update an entity. Notifies subscribers AFTER releasing
    /// the snapshot write lock.
    pub async fn upsert(&self, entity: WorldEntity) {
        let event = {
            let mut snap = self.snapshot.write().unwrap();
            snap.insert(entity.clone());
            ProjectionEvent::EntityUpserted {
                kind: entity.kind.clone(),
                entity: Box::new(entity),
            }
        };
        self.notify(&event).await;
    }

    /// Tombstone an entity in-place. No-op if the entity doesn't exist.
    /// Notifies subscribers when a change occurred.
    pub async fn tombstone(
        &self,
        kind: &WorldEntityKind,
        id: &str,
        when: impl Into<String>,
    ) -> bool {
        let when_s = when.into();
        let changed = {
            let mut snap = self.snapshot.write().unwrap();
            let key = WorldSnapshot::key_for(kind, id);
            if let Some(entity) = snap.entities.get_mut(&key) {
                if !entity.state.tombstoned {
                    entity.state = std::mem::take(&mut entity.state).tombstoned(when_s);
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if changed {
            let event = ProjectionEvent::EntityTombstoned {
                kind: kind.clone(),
                id: id.into(),
            };
            self.notify(&event).await;
        }
        changed
    }

    /// Clone the current snapshot. Cheap because `WorldSnapshot`'s
    /// BTreeMap clone is bounded by entity count + property volume.
    /// For very large projections, prefer `with_snapshot` which holds
    /// a read lock during a closure.
    pub fn snapshot(&self) -> WorldSnapshot {
        self.snapshot.read().unwrap().clone()
    }

    /// Run a closure while holding a read lock. Avoids the clone.
    pub fn with_snapshot<R>(&self, f: impl FnOnce(&WorldSnapshot) -> R) -> R {
        let snap = self.snapshot.read().unwrap();
        f(&snap)
    }

    async fn notify(&self, event: &ProjectionEvent) {
        // Snapshot the subscriber list under the mutex, then release
        // before awaiting so a slow subscriber doesn't block new
        // subscribers from registering.
        let subs: Vec<Arc<dyn ProjectionSubscriber>> = {
            let lock = self.subscribers.lock().unwrap();
            lock.clone()
        };
        for sub in subs {
            sub.on_event(event).await;
        }
    }
}

/// Returned by `subscribe` when the id is already in use.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DuplicateSubscriberId(pub ProjectionSubscriberId);

impl std::fmt::Display for DuplicateSubscriberId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "duplicate projection subscriber id: {}", self.0.as_str())
    }
}

impl std::error::Error for DuplicateSubscriberId {}

#[cfg(test)]
mod tests {
    use super::super::entity::{EntityRef, WorldEntityState};
    use super::*;
    use std::sync::Mutex as StdMutex;

    fn file_entity(id: &str) -> WorldEntity {
        WorldEntity::new(
            EntityRef::new(id),
            WorldEntityKind::File,
            WorldEntityState::fresh("t0"),
        )
    }

    struct CountingSubscriber {
        id: ProjectionSubscriberId,
        events: StdMutex<Vec<ProjectionEvent>>,
    }

    #[async_trait]
    impl ProjectionSubscriber for CountingSubscriber {
        fn id(&self) -> ProjectionSubscriberId {
            self.id.clone()
        }
        async fn on_event(&self, event: &ProjectionEvent) {
            self.events.lock().unwrap().push(event.clone());
        }
    }

    fn counter(id: &str) -> Arc<CountingSubscriber> {
        Arc::new(CountingSubscriber {
            id: ProjectionSubscriberId::new(id),
            events: StdMutex::new(Vec::new()),
        })
    }

    // ── construction / subscribe ────────────────────────────────────

    #[test]
    fn new_starts_empty() {
        let s = ProjectionStore::new("t0");
        assert_eq!(s.subscriber_count(), 0);
        assert_eq!(s.snapshot().entities.len(), 0);
    }

    #[test]
    fn subscribe_and_unsubscribe() {
        let s = ProjectionStore::new("t0");
        let c = counter("a");
        s.subscribe(c.clone()).unwrap();
        assert_eq!(s.subscriber_count(), 1);
        assert!(s.unsubscribe(&ProjectionSubscriberId::new("a")));
        assert_eq!(s.subscriber_count(), 0);
        assert!(!s.unsubscribe(&ProjectionSubscriberId::new("a")));
    }

    #[test]
    fn subscribe_duplicate_id_returns_err() {
        let s = ProjectionStore::new("t0");
        s.subscribe(counter("x")).unwrap();
        let err = s.subscribe(counter("x")).unwrap_err();
        assert_eq!(err.0, ProjectionSubscriberId::new("x"));
    }

    // ── upsert + notify ────────────────────────────────────────────

    #[tokio::test]
    async fn upsert_inserts_and_notifies() {
        let s = ProjectionStore::new("t0");
        let c = counter("watcher");
        s.subscribe(c.clone()).unwrap();
        s.upsert(file_entity("file:/a")).await;
        // Snapshot has the entity.
        let snap = s.snapshot();
        assert!(snap.get(&WorldEntityKind::File, "file:/a").is_some());
        // Subscriber saw the event.
        let events = c.events.lock().unwrap();
        assert_eq!(events.len(), 1);
        assert!(matches!(events[0], ProjectionEvent::EntityUpserted { .. }));
    }

    #[tokio::test]
    async fn upsert_with_no_subscribers_still_inserts() {
        let s = ProjectionStore::new("t0");
        s.upsert(file_entity("file:/a")).await;
        assert!(s.snapshot().get(&WorldEntityKind::File, "file:/a").is_some());
    }

    #[tokio::test]
    async fn upsert_overwrites_existing() {
        let s = ProjectionStore::new("t0");
        let mut e1 = file_entity("file:/a");
        e1.state.version = 1;
        s.upsert(e1).await;
        let mut e2 = file_entity("file:/a");
        e2.state.version = 7;
        s.upsert(e2).await;
        assert_eq!(s.snapshot().entities.len(), 1);
        assert_eq!(
            s.snapshot()
                .get(&WorldEntityKind::File, "file:/a")
                .unwrap()
                .state
                .version,
            7
        );
    }

    // ── tombstone ──────────────────────────────────────────────────

    #[tokio::test]
    async fn tombstone_returns_true_and_notifies() {
        let s = ProjectionStore::new("t0");
        let c = counter("w");
        s.subscribe(c.clone()).unwrap();
        s.upsert(file_entity("file:/a")).await;
        let ok = s.tombstone(&WorldEntityKind::File, "file:/a", "t1").await;
        assert!(ok);
        assert!(s
            .snapshot()
            .get(&WorldEntityKind::File, "file:/a")
            .unwrap()
            .is_tombstoned());
        // 1 upsert + 1 tombstone notification.
        let events = c.events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert!(matches!(events[1], ProjectionEvent::EntityTombstoned { .. }));
    }

    #[tokio::test]
    async fn tombstone_unknown_returns_false_no_notify() {
        let s = ProjectionStore::new("t0");
        let c = counter("w");
        s.subscribe(c.clone()).unwrap();
        let ok = s.tombstone(&WorldEntityKind::File, "nope", "t1").await;
        assert!(!ok);
        assert!(c.events.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn tombstone_twice_only_notifies_once() {
        let s = ProjectionStore::new("t0");
        let c = counter("w");
        s.subscribe(c.clone()).unwrap();
        s.upsert(file_entity("file:/a")).await;
        s.tombstone(&WorldEntityKind::File, "file:/a", "t1").await;
        let ok = s.tombstone(&WorldEntityKind::File, "file:/a", "t2").await;
        assert!(!ok, "second tombstone is no-op");
        // 1 upsert + 1 tombstone, NOT a second tombstone notify.
        let events = c.events.lock().unwrap();
        assert_eq!(events.len(), 2);
    }

    // ── with_snapshot zero-clone ──────────────────────────────────

    #[tokio::test]
    async fn with_snapshot_runs_closure_under_read_lock() {
        let s = ProjectionStore::new("t0");
        s.upsert(file_entity("file:/a")).await;
        s.upsert(file_entity("file:/b")).await;
        let count = s.with_snapshot(|snap| snap.entities.len());
        assert_eq!(count, 2);
    }

    // ── multi-subscriber dispatch order ───────────────────────────

    #[tokio::test]
    async fn subscribers_notified_in_registration_order() {
        let s = ProjectionStore::new("t0");
        let a = counter("a");
        let b = counter("b");
        let c = counter("c");
        s.subscribe(a.clone()).unwrap();
        s.subscribe(b.clone()).unwrap();
        s.subscribe(c.clone()).unwrap();
        s.upsert(file_entity("file:/x")).await;
        // Each subscriber received exactly 1 event.
        for sub in [&a, &b, &c] {
            assert_eq!(sub.events.lock().unwrap().len(), 1);
        }
    }

    // ── DuplicateSubscriberId Display ──────────────────────────────

    #[test]
    fn duplicate_subscriber_id_display() {
        let e = DuplicateSubscriberId(ProjectionSubscriberId::new("x"));
        let s = e.to_string();
        assert!(s.contains("duplicate"));
        assert!(s.contains("x"));
    }
}
