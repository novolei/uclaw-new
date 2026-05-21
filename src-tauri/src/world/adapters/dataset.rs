//! Dataset adapter — database tables / spreadsheets / CSV files.
//!
//! Each dataset becomes one `WorldEntity` of kind `Dataset`. State
//! carries source (postgres / mysql / sqlite / sheets / csv / parquet),
//! schema name, table name, row_count, column_count, owner.
//! Per-row tracking is out of scope — datasets are typically too large
//! to project at row granularity; we observe table-level metadata
//! only.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Inbound dataset change event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DatasetEvent {
    /// Dataset created or first observed.
    DatasetObserved {
        dataset_id: String,
        source: String, // "postgres", "sheets", "csv", ...
        schema: String,
        table: String,
        row_count: u64,
        column_count: u32,
        owner: String,
        observed_at: String,
    },
    /// Schema or row counts changed.
    DatasetUpdated {
        dataset_id: String,
        source: String,
        schema: String,
        table: String,
        row_count: u64,
        column_count: u32,
        owner: String,
        observed_at: String,
    },
    /// Dataset dropped / file removed.
    DatasetDeleted {
        dataset_id: String,
        deleted_at: String,
    },
}

/// Build a `WorldEntity` for one dataset.
pub fn dataset_to_entity(
    dataset_id: &str,
    source: &str,
    schema: &str,
    table: &str,
    row_count: u64,
    column_count: u32,
    owner: &str,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("dataset:{source}:{dataset_id}");
    let state = WorldEntityState::fresh(observed_at)
        .with_property("dataset_id", json!(dataset_id))
        .with_property("source", json!(source))
        .with_property("schema", json!(schema))
        .with_property("table", json!(table))
        .with_property("row_count", json!(row_count))
        .with_property("column_count", json!(column_count))
        .with_property("owner", json!(owner));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::Dataset, state)
}

pub struct DatasetAdapter {
    store: ProjectionStore,
}

impl DatasetAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    pub async fn handle(&self, event: DatasetEvent) -> bool {
        match event {
            DatasetEvent::DatasetObserved {
                dataset_id,
                source,
                schema,
                table,
                row_count,
                column_count,
                owner,
                observed_at,
            }
            | DatasetEvent::DatasetUpdated {
                dataset_id,
                source,
                schema,
                table,
                row_count,
                column_count,
                owner,
                observed_at,
            } => {
                let entity = dataset_to_entity(
                    &dataset_id,
                    &source,
                    &schema,
                    &table,
                    row_count,
                    column_count,
                    &owner,
                    &observed_at,
                );
                self.store.upsert(entity).await;
                true
            }
            DatasetEvent::DatasetDeleted {
                dataset_id,
                deleted_at,
            } => {
                let snap = self.store.snapshot();
                for source in &["postgres", "mysql", "sqlite", "sheets", "csv", "parquet", "other"]
                {
                    let key = format!("dataset:{source}:{dataset_id}");
                    if snap.get(&WorldEntityKind::Dataset, &key).is_some() {
                        return self
                            .store
                            .tombstone(&WorldEntityKind::Dataset, &key, &deleted_at)
                            .await;
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn observed(id: &str, source: &str) -> DatasetEvent {
        DatasetEvent::DatasetObserved {
            dataset_id: id.into(),
            source: source.into(),
            schema: "public".into(),
            table: "users".into(),
            row_count: 1000,
            column_count: 5,
            owner: "admin".into(),
            observed_at: "t0".into(),
        }
    }

    // ── entity factory ─────────────────────────────────────────────

    #[test]
    fn entity_id_uses_dataset_source_namespace() {
        let e = dataset_to_entity(
            "d1", "postgres", "public", "users", 100, 3, "alice", "t",
        );
        assert_eq!(e.r#ref.id, "dataset:postgres:d1");
        assert_eq!(e.kind, WorldEntityKind::Dataset);
        assert_eq!(e.state.properties.get("row_count"), Some(&json!(100)));
        assert_eq!(e.state.properties.get("column_count"), Some(&json!(3)));
    }

    // ── serde ──────────────────────────────────────────────────────

    #[test]
    fn event_serde_tag_snake_case() {
        let v = serde_json::to_value(observed("d1", "postgres")).unwrap();
        assert_eq!(v["kind"], "dataset_observed");
    }

    #[test]
    fn event_roundtrips_deleted() {
        let e = DatasetEvent::DatasetDeleted {
            dataset_id: "d1".into(),
            deleted_at: "t".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: DatasetEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    // ── adapter: observe ──────────────────────────────────────────

    #[tokio::test]
    async fn observe_inserts_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = DatasetAdapter::new(store.clone());
        adapter.handle(observed("d1", "postgres")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Dataset, "dataset:postgres:d1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("row_count"), Some(&json!(1000)));
    }

    // ── adapter: update ───────────────────────────────────────────

    #[tokio::test]
    async fn update_replaces_row_count() {
        let store = ProjectionStore::new("t0");
        let adapter = DatasetAdapter::new(store.clone());
        adapter.handle(observed("d1", "postgres")).await;
        adapter
            .handle(DatasetEvent::DatasetUpdated {
                dataset_id: "d1".into(),
                source: "postgres".into(),
                schema: "public".into(),
                table: "users".into(),
                row_count: 5000,
                column_count: 5,
                owner: "admin".into(),
                observed_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Dataset, "dataset:postgres:d1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("row_count"), Some(&json!(5000)));
    }

    // ── adapter: delete ───────────────────────────────────────────

    #[tokio::test]
    async fn delete_finds_dataset_regardless_of_source() {
        let store = ProjectionStore::new("t0");
        let adapter = DatasetAdapter::new(store.clone());
        adapter.handle(observed("d1", "sheets")).await;
        let changed = adapter
            .handle(DatasetEvent::DatasetDeleted {
                dataset_id: "d1".into(),
                deleted_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Dataset, "dataset:sheets:d1")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }

    #[tokio::test]
    async fn delete_unknown_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = DatasetAdapter::new(store.clone());
        let changed = adapter
            .handle(DatasetEvent::DatasetDeleted {
                dataset_id: "ghost".into(),
                deleted_at: "t".into(),
            })
            .await;
        assert!(!changed);
    }

    // ── same id different sources ────────────────────────────────

    #[tokio::test]
    async fn same_dataset_id_different_sources_coexist() {
        let store = ProjectionStore::new("t0");
        let adapter = DatasetAdapter::new(store.clone());
        adapter.handle(observed("users", "postgres")).await;
        adapter.handle(observed("users", "csv")).await;
        let snap = store.snapshot();
        assert_eq!(snap.entities.len(), 2);
    }
}
