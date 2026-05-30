// SPDX-License-Identifier: Apache-2.0
//! Tree registry — get-or-create for source trees (openhuman port — Phase 3a).
//!
//! The registry is the entry point for the ingest path to look up the
//! tree for a given (kind, scope). Phase 3a only touches source trees;
//! topic / global trees will reuse the same `(kind, scope)` convention
//! in Phases 3b / 3c.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::types::{Tree, TreeKind, TreeStatus};

/// Look up the source tree for `scope`, or create a new one.
///
/// Scope format convention (Phase 3a): use the ingested chunk's
/// `metadata.source_id` verbatim, so re-ingesting the same Slack channel
/// or Gmail account keeps appending to the same tree.
pub fn get_or_create_source_tree(store: &BucketSealStore, scope: &str) -> Result<Tree> {
    if let Some(existing) = store::get_tree_by_scope(store, TreeKind::Source, scope)? {
        tracing::debug!(
            tree_id = %existing.id,
            scope = %scope,
            "[tree_source::registry] found existing source tree"
        );
        return Ok(existing);
    }

    let tree = Tree {
        id: new_tree_id(TreeKind::Source),
        kind: TreeKind::Source,
        scope: scope.to_string(),
        root_id: None,
        max_level: 0,
        status: TreeStatus::Active,
        created_at: Utc::now(),
        last_sealed_at: None,
    };
    match store::insert_tree(store, &tree) {
        Ok(()) => {
            tracing::info!(
                tree_id = %tree.id,
                scope = %scope,
                "[tree_source::registry] created source tree"
            );
            Ok(tree)
        }
        Err(err) if is_unique_violation(&err) => {
            // Race: another caller created a tree for the same scope
            // between our initial lookup and this insert. UNIQUE(kind,
            // scope) rejected our row; re-query and return the winner.
            tracing::debug!(
                scope = %scope,
                "[tree_source::registry] UNIQUE race — re-querying"
            );
            store::get_tree_by_scope(store, TreeKind::Source, scope)?.ok_or_else(|| {
                anyhow::anyhow!(
                    "UNIQUE violation on insert but no row found on re-query for scope {scope}"
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
    // Fallback: scan the rendered message.
    let msg = format!("{err:#}");
    msg.contains("UNIQUE constraint failed")
}

fn new_tree_id(kind: TreeKind) -> String {
    format!("{}:{}", kind.as_str(), Uuid::new_v4())
}

/// Public id generator for summary nodes — exported so `bucket_seal` can
/// share the same format. The Unix-ms timestamp is the leading sort key so
/// `ORDER BY id` is globally chronological across all levels.
/// `:013` zero-pads the millisecond field to 13 digits so the
/// lexicographic order matches numeric order through year 2286.
/// Level is suffixed for filter-by-level queries (`LIKE '%:L1-%'`).
/// 8-hex of `u32` entropy shrinks same-millisecond collision probability
/// to ~2⁻³² per pair.
pub fn new_summary_id(level: u32) -> String {
    let ms = chrono::Utc::now().timestamp_millis() as u64;
    let rand_tail: u32 = rand::random();
    format!("summary:{:013}:L{}-{:08x}", ms, level, rand_tail)
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
    fn get_or_create_is_idempotent_on_scope() {
        let (store, _dir) = fresh_store();
        let first = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let second = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        assert_eq!(first.id, second.id);
        assert_eq!(first.kind, TreeKind::Source);
        assert_eq!(first.status, TreeStatus::Active);
    }

    #[test]
    fn different_scopes_yield_different_trees() {
        let (store, _dir) = fresh_store();
        let a = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let b = get_or_create_source_tree(&store, "gmail:user@example.com").unwrap();
        assert_ne!(a.id, b.id);
        assert_ne!(a.scope, b.scope);
    }

    #[test]
    fn tree_id_has_expected_prefix() {
        let id = new_tree_id(TreeKind::Source);
        assert!(id.starts_with("source:"), "expected 'source:' prefix in {id}");
    }

    #[test]
    fn summary_id_format_is_correct() {
        let id = new_summary_id(1);
        assert!(id.starts_with("summary:"), "expected 'summary:' prefix in {id}");
        // Format: summary:<13-digit-ms>:L<level>-<8hex>
        assert!(id.contains(":L1-"), "expected level suffix ':L1-' in {id}");
        let rest = &id["summary:".len()..];
        let ms_part = rest.split(':').next().expect("ms segment");
        assert_eq!(ms_part.len(), 13, "ms must be 13 digits in {id}");
        assert!(
            ms_part.chars().all(|c| c.is_ascii_digit()),
            "ms must be all digits in {id}"
        );
    }

    #[test]
    fn summary_id_is_lexicographically_chronological() {
        // Construct two ids with known ms values and verify the
        // lexicographic order matches the chronological order.
        let earlier_ms: u64 = 1_700_000_000_000;
        let later_ms: u64 = 1_700_000_000_001;
        let earlier = format!("summary:{:013}:L1-{:08x}", earlier_ms, u32::MAX);
        let later = format!("summary:{:013}:L9-{:08x}", later_ms, 0u32);
        assert!(
            earlier < later,
            "expected {earlier} < {later} (ms must outrank level + tail)"
        );
    }

    #[test]
    fn get_or_create_recovers_from_unique_race() {
        // Pre-insert a tree, then simulate a second insert with the same scope.
        let (store, _dir) = fresh_store();
        let pre_existing = Tree {
            id: "source:preexisting".into(),
            kind: TreeKind::Source,
            scope: "slack:#eng".into(),
            root_id: None,
            max_level: 0,
            status: TreeStatus::Active,
            created_at: Utc::now(),
            last_sealed_at: None,
        };
        store::insert_tree(&store, &pre_existing).unwrap();

        // get_or_create should find the existing row via get_tree_by_scope.
        let got = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        assert_eq!(got.id, "source:preexisting");

        // Direct UNIQUE violation check via is_unique_violation helper.
        let dup = Tree {
            id: "source:would-collide".into(),
            ..pre_existing.clone()
        };
        let err = store::insert_tree(&store, &dup).unwrap_err();
        assert!(
            is_unique_violation(&err),
            "expected UNIQUE violation, got: {err:#}"
        );
    }
}
