// SPDX-License-Identifier: Apache-2.0
//! Singleton registry for the global activity digest tree (Phase 3b).
//!
//! Unlike source trees (one per source_id) or topic trees (one per entity),
//! the global tree is a true singleton per workspace — scope is the literal
//! string `"global"`. Lookup + race-recovery otherwise mirror
//! `tree_source::registry::get_or_create_source_tree`.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::GLOBAL_SCOPE;
use crate::memory_bucket_seal::tree_source::store as tree_store;
use crate::memory_bucket_seal::tree_source::types::{Tree, TreeKind, TreeStatus};

/// Return the workspace's singleton global tree, creating it lazily on
/// first call. Safe to call repeatedly — subsequent calls short-circuit to
/// the existing row.
pub fn get_or_create_global_tree(store: &BucketSealStore) -> Result<Tree> {
    if let Some(existing) =
        tree_store::get_tree_by_scope(store, TreeKind::Global, GLOBAL_SCOPE)?
    {
        return Ok(existing);
    }

    let tree = Tree {
        id: format!("{}:{}", TreeKind::Global.as_str(), Uuid::new_v4()),
        kind: TreeKind::Global,
        scope: GLOBAL_SCOPE.to_string(),
        root_id: None,
        max_level: 0,
        status: TreeStatus::Active,
        created_at: Utc::now(),
        last_sealed_at: None,
    };
    match tree_store::insert_tree(store, &tree) {
        Ok(()) => Ok(tree),
        Err(err) if is_unique_violation(&err) => {
            // Race: another caller created the global tree between our initial
            // lookup and this insert. Re-query and return the winner.
            tree_store::get_tree_by_scope(store, TreeKind::Global, GLOBAL_SCOPE)?.ok_or_else(
                || {
                    anyhow::anyhow!(
                        "UNIQUE violation on global-tree insert but no row found on re-query"
                    )
                },
            )
        }
        Err(err) => Err(err),
    }
}

/// True when `err` wraps a SQLite UNIQUE constraint violation. Duplicated
/// from `tree_source::registry` (private there) to keep this module
/// self-contained — same shape as the `tree_topic` copy.
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
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn creates_singleton_global_tree() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_global_tree(&store).unwrap();
        assert_eq!(tree.scope, "global");
        assert_eq!(tree.kind, TreeKind::Global);
        assert_eq!(tree.status, TreeStatus::Active);
        assert!(tree.id.starts_with("global:"));
    }

    #[test]
    fn idempotent_returns_same_tree() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_global_tree(&store).unwrap();
        let t2 = get_or_create_global_tree(&store).unwrap();
        assert_eq!(t1.id, t2.id);
    }

    #[test]
    fn global_distinct_from_source_and_topic_same_scope() {
        let (store, _dir) = fresh_store();
        let global = get_or_create_global_tree(&store).unwrap();
        let source =
            crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "global")
                .unwrap();
        assert_ne!(global.id, source.id);
        assert_eq!(global.kind, TreeKind::Global);
        assert_eq!(source.kind, TreeKind::Source);
    }
}
