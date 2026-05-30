// SPDX-License-Identifier: Apache-2.0
//! Topic-tree registry — idempotent lookup keyed by entity string.
//!
//! Mirrors [`crate::memory_bucket_seal::tree_source::registry`] but with
//! [`TreeKind::Topic`].

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::store as tree_store;
use crate::memory_bucket_seal::tree_source::types::{Tree, TreeKind, TreeStatus};

/// Look up the topic tree for `entity` or create it if it doesn't exist.
/// Idempotent — calling twice for the same entity returns the same row.
pub fn get_or_create_topic_tree(store: &BucketSealStore, entity: &str) -> Result<Tree> {
    if let Some(tree) = tree_store::get_tree_by_scope(store, TreeKind::Topic, entity)? {
        tracing::debug!(
            tree_id = %tree.id,
            entity = %entity,
            "[tree_topic::registry] found existing topic tree"
        );
        return Ok(tree);
    }

    let tree = Tree {
        id: format!("topic:{}", Uuid::new_v4()),
        kind: TreeKind::Topic,
        scope: entity.to_string(),
        root_id: None,
        max_level: 0,
        status: TreeStatus::Active,
        created_at: Utc::now(),
        last_sealed_at: None,
    };
    match tree_store::insert_tree(store, &tree) {
        Ok(()) => {
            tracing::info!(
                tree_id = %tree.id,
                entity = %entity,
                "[tree_topic::registry] created topic tree"
            );
            Ok(tree)
        }
        Err(err) if is_unique_violation(&err) => {
            // Race: another caller created a tree for the same entity
            // between our initial lookup and this insert. Re-query and return
            // the winner.
            tracing::debug!(
                entity = %entity,
                "[tree_topic::registry] UNIQUE race — re-querying"
            );
            tree_store::get_tree_by_scope(store, TreeKind::Topic, entity)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "UNIQUE violation on insert but no row found on re-query for entity {entity}"
                )
            })
        }
        Err(err) => Err(err),
    }
}

/// Return true if `err` represents a SQLite UNIQUE constraint violation.
fn is_unique_violation(err: &anyhow::Error) -> bool {
    if let Some(rusqlite_err) = err.downcast_ref::<rusqlite::Error>() {
        if let rusqlite::Error::SqliteFailure(sqlite_err, _) = rusqlite_err {
            return sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation;
        }
    }
    let msg = format!("{err:#}");
    msg.contains("UNIQUE constraint failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::store::BucketSealStore;
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn creates_topic_tree_for_new_entity() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_topic_tree(&store, "Alice").unwrap();
        assert_eq!(tree.scope, "Alice");
        assert_eq!(tree.kind, TreeKind::Topic);
        assert_eq!(tree.status, TreeStatus::Active);
        assert_eq!(tree.max_level, 0);
        assert!(tree.root_id.is_none());
    }

    #[test]
    fn idempotent_returns_same_tree_id() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_topic_tree(&store, "Bob").unwrap();
        let t2 = get_or_create_topic_tree(&store, "Bob").unwrap();
        assert_eq!(t1.id, t2.id);
    }

    #[test]
    fn distinct_entities_get_distinct_trees() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_topic_tree(&store, "@alice").unwrap();
        let t2 = get_or_create_topic_tree(&store, "#design").unwrap();
        let t3 = get_or_create_topic_tree(&store, "Project Phoenix").unwrap();
        assert_ne!(t1.id, t2.id);
        assert_ne!(t2.id, t3.id);
        assert_ne!(t1.id, t3.id);
    }

    #[test]
    fn topic_and_source_trees_with_same_scope_are_distinct() {
        let (store, _dir) = fresh_store();
        let source_tree =
            crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "shared_name")
                .unwrap();
        let topic_tree = get_or_create_topic_tree(&store, "shared_name").unwrap();
        assert_ne!(source_tree.id, topic_tree.id);
        assert_eq!(source_tree.kind, TreeKind::Source);
        assert_eq!(topic_tree.kind, TreeKind::Topic);
    }
}
