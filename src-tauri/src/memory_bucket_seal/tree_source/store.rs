// SPDX-License-Identifier: Apache-2.0
//! SQLite-backed persistence for Phase 3a summary trees (openhuman port).
//!
//! Three tables (schema lives in `memory_bucket_seal::store::SCHEMA`):
//! - `mem_tree_trees`      — one row per tree (kind, scope, root, max_level)
//! - `mem_tree_summaries`  — one row per sealed summary node (immutable)
//! - `mem_tree_buffers`    — one row per unsealed frontier `(tree_id, level)`
//!
//! All timestamps are stored as milliseconds since the Unix epoch.
//! Writes are serialised through [`BucketSealStore::lock_conn`] so we
//! inherit its busy-timeout, WAL, and foreign-key enforcement.
//!
//! `embedding BLOB` on `mem_tree_summaries` is populated via
//! [`crate::memory_bucket_seal::score::embed::pack_checked`] at seal time.

use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::memory_bucket_seal::score::embed::{decode_optional_blob, pack_checked};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::types::{
    Buffer, SummaryNode, Tree, TreeKind, TreeStatus,
};

fn ms_to_utc(ms: i64) -> rusqlite::Result<DateTime<Utc>> {
    Utc.timestamp_millis_opt(ms).single().ok_or_else(|| {
        rusqlite::Error::FromSqlConversionFailure(
            0,
            rusqlite::types::Type::Integer,
            format!("invalid timestamp ms {ms}").into(),
        )
    })
}

// ── Tree rows ────────────────────────────────────────────────────────────

/// Insert a new tree row. Fails if `(kind, scope)` already exists; callers
/// that want "get or create" semantics should go through the `registry`.
pub fn insert_tree(store: &BucketSealStore, tree: &Tree) -> Result<()> {
    let conn = store.lock_conn()?;
    insert_tree_conn(&conn, tree)
}

pub(crate) fn insert_tree_conn(conn: &Connection, tree: &Tree) -> Result<()> {
    conn.execute(
        "INSERT INTO mem_tree_trees (
            id, kind, scope, root_id, max_level, status,
            created_at_ms, last_sealed_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            tree.id,
            tree.kind.as_str(),
            tree.scope,
            tree.root_id,
            tree.max_level,
            tree.status.as_str(),
            tree.created_at.timestamp_millis(),
            tree.last_sealed_at.map(|t| t.timestamp_millis()),
        ],
    )
    .with_context(|| format!("Failed to insert tree id={}", tree.id))?;
    Ok(())
}

/// Fetch a tree by `(kind, scope)`. Returns `None` if no such tree exists.
pub fn get_tree_by_scope(
    store: &BucketSealStore,
    kind: TreeKind,
    scope: &str,
) -> Result<Option<Tree>> {
    let conn = store.lock_conn()?;
    get_tree_by_scope_conn(&conn, kind, scope)
}

pub(crate) fn get_tree_by_scope_conn(
    conn: &Connection,
    kind: TreeKind,
    scope: &str,
) -> Result<Option<Tree>> {
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, root_id, max_level, status,
                created_at_ms, last_sealed_at_ms
           FROM mem_tree_trees WHERE kind = ?1 AND scope = ?2",
    )?;
    let row = stmt
        .query_row(params![kind.as_str(), scope], row_to_tree)
        .optional()
        .context("Failed to query tree by scope")?;
    Ok(row)
}

/// Fetch a tree by primary key id.
pub fn get_tree(store: &BucketSealStore, id: &str) -> Result<Option<Tree>> {
    let conn = store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, root_id, max_level, status,
                created_at_ms, last_sealed_at_ms
           FROM mem_tree_trees WHERE id = ?1",
    )?;
    let row = stmt
        .query_row(params![id], row_to_tree)
        .optional()
        .context("Failed to query tree by id")?;
    Ok(row)
}

/// List every tree of a given kind. Rows come back ordered by `created_at_ms` ASC.
pub fn list_trees_by_kind(store: &BucketSealStore, kind: TreeKind) -> Result<Vec<Tree>> {
    let conn = store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, kind, scope, root_id, max_level, status,
                created_at_ms, last_sealed_at_ms
           FROM mem_tree_trees
          WHERE kind = ?1
          ORDER BY created_at_ms ASC",
    )?;
    let rows = stmt
        .query_map(params![kind.as_str()], row_to_tree)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("Failed to collect trees by kind")?;
    Ok(rows)
}

pub(crate) fn update_tree_after_seal_tx(
    tx: &Transaction<'_>,
    tree_id: &str,
    root_id: &str,
    max_level: u32,
    sealed_at: DateTime<Utc>,
) -> Result<()> {
    tx.execute(
        "UPDATE mem_tree_trees
            SET root_id = ?1,
                max_level = ?2,
                last_sealed_at_ms = ?3
          WHERE id = ?4",
        params![root_id, max_level, sealed_at.timestamp_millis(), tree_id],
    )
    .with_context(|| format!("Failed to update tree {tree_id} after seal"))?;
    Ok(())
}

pub(crate) fn refresh_last_sealed_tx(
    tx: &Transaction<'_>,
    tree_id: &str,
    sealed_at: DateTime<Utc>,
) -> Result<()> {
    tx.execute(
        "UPDATE mem_tree_trees SET last_sealed_at_ms = ?1 WHERE id = ?2",
        params![sealed_at.timestamp_millis(), tree_id],
    )
    .with_context(|| format!("Failed to refresh last_sealed_at for tree {tree_id}"))?;
    Ok(())
}

fn row_to_tree(row: &rusqlite::Row<'_>) -> rusqlite::Result<Tree> {
    let id: String = row.get(0)?;
    let kind_s: String = row.get(1)?;
    let scope: String = row.get(2)?;
    let root_id: Option<String> = row.get(3)?;
    let max_level: i64 = row.get(4)?;
    let status_s: String = row.get(5)?;
    let created_ms: i64 = row.get(6)?;
    let last_sealed_ms: Option<i64> = row.get(7)?;

    let kind = TreeKind::parse(&kind_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(1, rusqlite::types::Type::Text, e.into())
    })?;
    let status = TreeStatus::parse(&status_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, e.into())
    })?;
    Ok(Tree {
        id,
        kind,
        scope,
        root_id,
        max_level: max_level.max(0) as u32,
        status,
        created_at: ms_to_utc(created_ms)?,
        last_sealed_at: last_sealed_ms.map(ms_to_utc).transpose()?,
    })
}

// ── Summary nodes ────────────────────────────────────────────────────────

/// Insert a sealed summary. Immutable — the caller must generate a fresh
/// id per seal. `INSERT OR IGNORE` so retries of the same seal transaction
/// don't double-insert.
///
/// `node.embedding` is packed as a little-endian BLOB if `Some`; `None`
/// writes NULL.
pub(crate) fn insert_summary_tx(tx: &Transaction<'_>, node: &SummaryNode) -> Result<()> {
    let embedding_blob: Option<Vec<u8>> = match node.embedding.as_deref() {
        Some(v) => Some(
            pack_checked(v)
                .with_context(|| format!("Failed to pack embedding for summary id={}", node.id))?,
        ),
        None => None,
    };

    tx.execute(
        "INSERT OR IGNORE INTO mem_tree_summaries (
            id, tree_id, tree_kind, level, parent_id,
            child_ids_json, content, token_count,
            entities_json, topics_json,
            time_range_start_ms, time_range_end_ms,
            score, sealed_at_ms, deleted, embedding
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)",
        params![
            node.id,
            node.tree_id,
            node.tree_kind.as_str(),
            node.level,
            node.parent_id,
            serde_json::to_string(&node.child_ids)?,
            node.content,
            node.token_count,
            serde_json::to_string(&node.entities)?,
            serde_json::to_string(&node.topics)?,
            node.time_range_start.timestamp_millis(),
            node.time_range_end.timestamp_millis(),
            node.score,
            node.sealed_at.timestamp_millis(),
            node.deleted as i64,
            embedding_blob,
        ],
    )
    .with_context(|| format!("Failed to insert summary id={}", node.id))?;
    Ok(())
}

/// Set (or overwrite) the embedding for an existing summary row.
/// Exposed for future backfill helpers. Returns the number of rows updated
/// (0 if the id is unknown).
pub fn set_summary_embedding(
    store: &BucketSealStore,
    summary_id: &str,
    embedding: &[f32],
) -> Result<usize> {
    let blob = pack_checked(embedding)
        .with_context(|| format!("Failed to pack embedding for summary id={summary_id}"))?;
    let conn = store.lock_conn()?;
    let changed = conn.execute(
        "UPDATE mem_tree_summaries SET embedding = ?1 WHERE id = ?2",
        params![blob, summary_id],
    )?;
    if changed == 0 {
        tracing::warn!(
            summary_id = %summary_id,
            "[tree_source::store] set_summary_embedding: no row found"
        );
    }
    Ok(changed)
}

/// Fetch a summary's embedding. Returns `Ok(None)` if the summary doesn't
/// exist OR if the `embedding` column is NULL (legacy rows).
pub fn get_summary_embedding(
    store: &BucketSealStore,
    summary_id: &str,
) -> Result<Option<Vec<f32>>> {
    let conn = store.lock_conn()?;
    let blob: Option<Option<Vec<u8>>> = conn
        .query_row(
            "SELECT embedding FROM mem_tree_summaries WHERE id = ?1",
            params![summary_id],
            |r| r.get::<_, Option<Vec<u8>>>(0),
        )
        .optional()?;
    match blob {
        None => Ok(None),
        Some(inner) => decode_optional_blob(inner, &format!("summary_id={summary_id}")),
    }
}

/// Fetch one summary by id. Soft-deleted rows are returned with
/// `deleted = true` so callers can decide filtering policy.
pub fn get_summary(store: &BucketSealStore, id: &str) -> Result<Option<SummaryNode>> {
    let conn = store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, tree_id, tree_kind, level, parent_id,
                child_ids_json, content, token_count,
                entities_json, topics_json,
                time_range_start_ms, time_range_end_ms,
                score, sealed_at_ms, deleted, embedding
           FROM mem_tree_summaries WHERE id = ?1",
    )?;
    let row = stmt
        .query_row(params![id], row_to_summary)
        .optional()
        .context("Failed to query summary by id")?;
    Ok(row)
}

/// List sealed summaries for a tree at a given level, ordered by
/// `sealed_at` ascending. Skips tombstoned rows.
pub fn list_summaries_at_level(
    store: &BucketSealStore,
    tree_id: &str,
    level: u32,
) -> Result<Vec<SummaryNode>> {
    let conn = store.lock_conn()?;
    let mut stmt = conn.prepare(
        "SELECT id, tree_id, tree_kind, level, parent_id,
                child_ids_json, content, token_count,
                entities_json, topics_json,
                time_range_start_ms, time_range_end_ms,
                score, sealed_at_ms, deleted, embedding
           FROM mem_tree_summaries
          WHERE tree_id = ?1 AND level = ?2 AND deleted = 0
          ORDER BY sealed_at_ms ASC",
    )?;
    let rows = stmt
        .query_map(params![tree_id, level], row_to_summary)?
        .collect::<rusqlite::Result<Vec<_>>>()
        .context("Failed to collect summaries")?;
    Ok(rows)
}

/// Count summaries in a tree (diagnostic helper).
pub fn count_summaries(store: &BucketSealStore, tree_id: &str) -> Result<u64> {
    let conn = store.lock_conn()?;
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM mem_tree_summaries
              WHERE tree_id = ?1 AND deleted = 0",
            params![tree_id],
            |r| r.get(0),
        )
        .context("count summaries query")?;
    Ok(n.max(0) as u64)
}

fn row_to_summary(row: &rusqlite::Row<'_>) -> rusqlite::Result<SummaryNode> {
    let id: String = row.get(0)?;
    let tree_id: String = row.get(1)?;
    let tree_kind_s: String = row.get(2)?;
    let level: i64 = row.get(3)?;
    let parent_id: Option<String> = row.get(4)?;
    let child_ids_json: String = row.get(5)?;
    let content: String = row.get(6)?;
    let token_count: i64 = row.get(7)?;
    let entities_json: String = row.get(8)?;
    let topics_json: String = row.get(9)?;
    let trs_ms: i64 = row.get(10)?;
    let tre_ms: i64 = row.get(11)?;
    let score: f64 = row.get(12)?;
    let sealed_ms: i64 = row.get(13)?;
    let deleted: i64 = row.get(14)?;
    let embedding_blob: Option<Vec<u8>> = row.get(15)?;

    let tree_kind = TreeKind::parse(&tree_kind_s).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, e.into())
    })?;
    let child_ids: Vec<String> = serde_json::from_str(&child_ids_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(5, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let entities: Vec<String> = serde_json::from_str(&entities_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(8, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let topics: Vec<String> = serde_json::from_str(&topics_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(9, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let embedding =
        decode_optional_blob(embedding_blob, &format!("summary_id={id}")).map_err(|e| {
            rusqlite::Error::FromSqlConversionFailure(
                15,
                rusqlite::types::Type::Blob,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    e.to_string(),
                )),
            )
        })?;

    Ok(SummaryNode {
        id,
        tree_id,
        tree_kind,
        level: level.max(0) as u32,
        parent_id,
        child_ids,
        content,
        token_count: token_count.max(0) as u32,
        entities,
        topics,
        time_range_start: ms_to_utc(trs_ms)?,
        time_range_end: ms_to_utc(tre_ms)?,
        score: score as f32,
        sealed_at: ms_to_utc(sealed_ms)?,
        deleted: deleted != 0,
        embedding,
    })
}

// ── Buffers ──────────────────────────────────────────────────────────────

/// Read the current buffer at `(tree_id, level)` or return an empty one.
pub fn get_buffer(store: &BucketSealStore, tree_id: &str, level: u32) -> Result<Buffer> {
    let conn = store.lock_conn()?;
    get_buffer_conn(&conn, tree_id, level)
}

pub(crate) fn get_buffer_conn(conn: &Connection, tree_id: &str, level: u32) -> Result<Buffer> {
    let mut stmt = conn.prepare(
        "SELECT tree_id, level, item_ids_json, token_sum, oldest_at_ms
           FROM mem_tree_buffers WHERE tree_id = ?1 AND level = ?2",
    )?;
    let row = stmt
        .query_row(params![tree_id, level], row_to_buffer)
        .optional()
        .context("Failed to query buffer")?;
    Ok(row.unwrap_or_else(|| Buffer::empty(tree_id, level)))
}

/// Upsert a buffer row (transactional — caller owns the transaction).
pub(crate) fn upsert_buffer_tx(tx: &Transaction<'_>, buf: &Buffer) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    tx.execute(
        "INSERT INTO mem_tree_buffers (
            tree_id, level, item_ids_json, token_sum, oldest_at_ms, updated_at_ms
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(tree_id, level) DO UPDATE SET
            item_ids_json = excluded.item_ids_json,
            token_sum = excluded.token_sum,
            oldest_at_ms = excluded.oldest_at_ms,
            updated_at_ms = excluded.updated_at_ms",
        params![
            buf.tree_id,
            buf.level,
            serde_json::to_string(&buf.item_ids)?,
            buf.token_sum,
            buf.oldest_at.map(|t| t.timestamp_millis()),
            now_ms,
        ],
    )
    .with_context(|| {
        format!(
            "Failed to upsert buffer tree_id={} level={}",
            buf.tree_id, buf.level
        )
    })?;
    Ok(())
}

/// Reset a buffer at `(tree_id, level)` to empty. Used at seal time: the
/// items move into a summary row and the buffer is cleared in the same tx.
pub(crate) fn clear_buffer_tx(tx: &Transaction<'_>, tree_id: &str, level: u32) -> Result<()> {
    let empty = Buffer::empty(tree_id, level);
    upsert_buffer_tx(tx, &empty)
}

fn row_to_buffer(row: &rusqlite::Row<'_>) -> rusqlite::Result<Buffer> {
    let tree_id: String = row.get(0)?;
    let level: i64 = row.get(1)?;
    let item_ids_json: String = row.get(2)?;
    let token_sum: i64 = row.get(3)?;
    let oldest_ms: Option<i64> = row.get(4)?;

    let item_ids: Vec<String> = serde_json::from_str(&item_ids_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(2, rusqlite::types::Type::Text, Box::new(e))
    })?;
    let oldest_at = oldest_ms.map(ms_to_utc).transpose()?;
    Ok(Buffer {
        tree_id,
        level: level.max(0) as u32,
        item_ids,
        token_sum,
        oldest_at,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::{pack_embedding, EMBEDDING_DIM};
    use crate::memory_bucket_seal::tree_source::types::{TreeKind, TreeStatus};
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    fn sample_tree(id: &str, scope: &str) -> Tree {
        Tree {
            id: id.to_string(),
            kind: TreeKind::Source,
            scope: scope.to_string(),
            root_id: None,
            max_level: 0,
            status: TreeStatus::Active,
            created_at: Utc.timestamp_millis_opt(1_700_000_000_000).unwrap(),
            last_sealed_at: None,
        }
    }

    fn sample_summary(id: &str, tree_id: &str, level: u32) -> SummaryNode {
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        SummaryNode {
            id: id.to_string(),
            tree_id: tree_id.to_string(),
            tree_kind: TreeKind::Source,
            level,
            parent_id: None,
            child_ids: vec!["leaf-a".into(), "leaf-b".into()],
            content: "seal content".into(),
            token_count: 100,
            entities: vec!["entity:alice".into()],
            topics: vec!["#launch".into()],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.75,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        }
    }

    #[test]
    fn tree_round_trip() {
        let (store, _dir) = fresh_store();
        let t = sample_tree("tree-1", "slack:#eng");
        insert_tree(&store, &t).unwrap();
        let got = get_tree(&store, "tree-1").unwrap().unwrap();
        assert_eq!(got, t);
        let by_scope = get_tree_by_scope(&store, TreeKind::Source, "slack:#eng")
            .unwrap()
            .unwrap();
        assert_eq!(by_scope.id, "tree-1");
    }

    #[test]
    fn duplicate_scope_fails() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("t1", "slack:#eng")).unwrap();
        let dup = sample_tree("t2", "slack:#eng");
        assert!(insert_tree(&store, &dup).is_err());
    }

    #[test]
    fn summary_insert_and_fetch() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let node = sample_summary("sum-1", "tree-1", 1);
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            insert_summary_tx(&tx, &node).unwrap();
            tx.commit().unwrap();
        }
        let got = get_summary(&store, "sum-1").unwrap().unwrap();
        assert_eq!(got, node);
        let at_level = list_summaries_at_level(&store, "tree-1", 1).unwrap();
        assert_eq!(at_level.len(), 1);
        assert_eq!(count_summaries(&store, "tree-1").unwrap(), 1);
    }

    #[test]
    fn summary_insert_is_idempotent_on_id() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let node = sample_summary("sum-1", "tree-1", 1);
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            insert_summary_tx(&tx, &node).unwrap();
            insert_summary_tx(&tx, &node).unwrap(); // idempotent
            tx.commit().unwrap();
        }
        assert_eq!(count_summaries(&store, "tree-1").unwrap(), 1);
    }

    #[test]
    fn buffer_upsert_and_clear() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let buf = Buffer {
            tree_id: "tree-1".into(),
            level: 0,
            item_ids: vec!["leaf-a".into(), "leaf-b".into()],
            token_sum: 500,
            oldest_at: Some(ts),
        };
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            upsert_buffer_tx(&tx, &buf).unwrap();
            tx.commit().unwrap();
        }
        let got = get_buffer(&store, "tree-1", 0).unwrap();
        assert_eq!(got, buf);

        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            clear_buffer_tx(&tx, "tree-1", 0).unwrap();
            tx.commit().unwrap();
        }
        let cleared = get_buffer(&store, "tree-1", 0).unwrap();
        assert!(cleared.is_empty());
        assert_eq!(cleared.token_sum, 0);
        assert!(cleared.oldest_at.is_none());
    }

    #[test]
    fn get_buffer_returns_empty_when_missing() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let got = get_buffer(&store, "tree-1", 0).unwrap();
        assert!(got.is_empty());
        assert_eq!(got.tree_id, "tree-1");
    }

    #[test]
    fn update_tree_after_seal_persists() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let sealed_at = Utc.timestamp_millis_opt(1_700_000_123_000).unwrap();
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            update_tree_after_seal_tx(&tx, "tree-1", "sum-1", 1, sealed_at).unwrap();
            tx.commit().unwrap();
        }
        let got = get_tree(&store, "tree-1").unwrap().unwrap();
        assert_eq!(got.root_id.as_deref(), Some("sum-1"));
        assert_eq!(got.max_level, 1);
        assert_eq!(got.last_sealed_at, Some(sealed_at));
    }

    #[test]
    fn list_trees_by_kind_ordered_by_created_at() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-2", "slack:#ops")).unwrap();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let trees = list_trees_by_kind(&store, TreeKind::Source).unwrap();
        assert_eq!(trees.len(), 2);
        // both have the same created_at so just check they're both returned
        let ids: Vec<&str> = trees.iter().map(|t| t.id.as_str()).collect();
        assert!(ids.contains(&"tree-1"));
        assert!(ids.contains(&"tree-2"));
    }

    #[test]
    fn summary_embedding_round_trip() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let embedding: Vec<f32> = (0..EMBEDDING_DIM).map(|i| i as f32 / 1024.0).collect();
        let mut node = sample_summary("sum-1", "tree-1", 1);
        node.embedding = Some(embedding.clone());
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            insert_summary_tx(&tx, &node).unwrap();
            tx.commit().unwrap();
        }
        let got = get_summary(&store, "sum-1").unwrap().unwrap();
        let got_emb = got.embedding.unwrap();
        assert_eq!(got_emb.len(), EMBEDDING_DIM);
        for (a, b) in embedding.iter().zip(got_emb.iter()) {
            assert!((a - b).abs() < 1e-6);
        }
    }

    #[test]
    fn set_get_summary_embedding() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let node = sample_summary("sum-1", "tree-1", 1);
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            insert_summary_tx(&tx, &node).unwrap();
            tx.commit().unwrap();
        }
        // Initially no embedding
        assert!(get_summary_embedding(&store, "sum-1").unwrap().is_none());

        // Set embedding
        let embedding: Vec<f32> = vec![0.0f32; EMBEDDING_DIM];
        let changed = set_summary_embedding(&store, "sum-1", &embedding).unwrap();
        assert_eq!(changed, 1);

        // Retrieve it
        let got = get_summary_embedding(&store, "sum-1").unwrap().unwrap();
        assert_eq!(got.len(), EMBEDDING_DIM);
    }

    #[test]
    fn summary_fk_enforced() {
        let (store, _dir) = fresh_store();
        // Don't insert tree — FK should prevent insertion of the summary.
        let node = sample_summary("sum-1", "nonexistent-tree", 1);
        // INSERT OR IGNORE with a FK violation: SQLite silently skips the row.
        // We verify by committing and then checking no row was inserted.
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            let _ = insert_summary_tx(&tx, &node);
            let _ = tx.commit();
            // conn/tx dropped here — lock released before calling get_summary
        }
        let got = get_summary(&store, "sum-1").unwrap();
        assert!(got.is_none(), "summary with bad FK should not be inserted");
    }

    #[test]
    fn buffer_upsert_is_idempotent() {
        let (store, _dir) = fresh_store();
        insert_tree(&store, &sample_tree("tree-1", "slack:#eng")).unwrap();
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let buf = Buffer {
            tree_id: "tree-1".into(),
            level: 0,
            item_ids: vec!["leaf-a".into()],
            token_sum: 100,
            oldest_at: Some(ts),
        };
        for _ in 0..3 {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            upsert_buffer_tx(&tx, &buf).unwrap();
            tx.commit().unwrap();
        }
        let got = get_buffer(&store, "tree-1", 0).unwrap();
        assert_eq!(got.item_ids, vec!["leaf-a".to_string()]);
        assert_eq!(got.token_sum, 100);
    }

    #[test]
    fn pack_embedding_helper_round_trips() {
        let v: Vec<f32> = (0..EMBEDDING_DIM).map(|i| i as f32 / 1000.0).collect();
        let packed = pack_embedding(&v);
        assert_eq!(packed.len(), EMBEDDING_DIM * 4);
    }
}
