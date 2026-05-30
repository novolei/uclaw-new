// SPDX-License-Identifier: Apache-2.0
//! Count-based cascade-seal for the global activity digest tree (Phase 3b).
//!
//! Trigger is count-based, not token-based: seal L0→L1 at 7 daily nodes,
//! L1→L2 at 4 weekly, L2→L3 at 12 monthly. Reuses PR8's storage primitives
//! without the token-budget gate. Every global buffer level holds summary
//! ids (even L0 — a daily digest is a SummaryNode), so hydration always
//! reads `mem_tree_summaries`.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::{
    GLOBAL_TOKEN_BUDGET, MONTHLY_SEAL_THRESHOLD, WEEKLY_SEAL_THRESHOLD, YEARLY_SEAL_THRESHOLD,
};
use crate::memory_bucket_seal::tree_source::registry::new_summary_id;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::summariser::{Summariser, SummaryContext, SummaryInput};
use crate::memory_bucket_seal::tree_source::types::{Buffer, SummaryNode, Tree, TreeKind};

const MAX_CASCADE_DEPTH: u32 = 32;

/// Append one L0 (daily) summary id to the global tree's L0 buffer, then
/// cascade-seal upward if count thresholds are crossed. The caller
/// (`digest::end_of_day_digest`) has already inserted the daily node into
/// `mem_tree_summaries`; this only does buffer accounting + cascade.
pub async fn append_daily_and_cascade(
    store: &BucketSealStore,
    tree: &Tree,
    daily_summary: &SummaryNode,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<Vec<String>> {
    append_to_buffer(
        store,
        &tree.id,
        0,
        &daily_summary.id,
        daily_summary.token_count as i64,
        daily_summary.time_range_start,
    )?;
    cascade_seals(store, tree, summariser, embedder).await
}

/// Transactionally append a summary id to the buffer at (tree_id, level).
/// Idempotent on (tree_id, level, item_id). Mirrors PR8's append_to_buffer.
fn append_to_buffer(
    store: &BucketSealStore,
    tree_id: &str,
    level: u32,
    item_id: &str,
    token_delta: i64,
    item_ts: DateTime<Utc>,
) -> Result<()> {
    let mut conn = store.lock_conn()?;
    let tx = conn.transaction()?;
    let mut buf = store::get_buffer_conn(&tx, tree_id, level)?;
    if buf.item_ids.iter().any(|existing| existing == item_id) {
        return Ok(());
    }
    buf.item_ids.push(item_id.to_string());
    buf.token_sum = buf.token_sum.saturating_add(token_delta);
    buf.oldest_at = match buf.oldest_at {
        Some(existing) => Some(existing.min(item_ts)),
        None => Some(item_ts),
    };
    store::upsert_buffer_tx(&tx, &buf)?;
    tx.commit()?;
    Ok(())
}

async fn cascade_seals(
    store: &BucketSealStore,
    tree: &Tree,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<Vec<String>> {
    let mut sealed_ids: Vec<String> = Vec::new();
    let mut level: u32 = 0;
    for _ in 0..MAX_CASCADE_DEPTH {
        let buf = store::get_buffer(store, &tree.id, level)?;
        if !should_seal(&buf, level) {
            break;
        }
        let summary_id = seal_one_level(store, tree, &buf, summariser, embedder).await?;
        sealed_ids.push(summary_id);
        level += 1;
    }
    Ok(sealed_ids)
}

/// Count-based threshold per level. 0→7 (weekly), 1→4 (monthly), 2→12
/// (yearly). Levels ≥ 3 never seal — yearly is the top.
pub(crate) fn should_seal(buf: &Buffer, level: u32) -> bool {
    let threshold = match level {
        0 => WEEKLY_SEAL_THRESHOLD,
        1 => MONTHLY_SEAL_THRESHOLD,
        2 => YEARLY_SEAL_THRESHOLD,
        _ => return false,
    };
    !buf.is_empty() && buf.item_ids.len() >= threshold
}

async fn seal_one_level(
    store: &BucketSealStore,
    tree: &Tree,
    buf: &Buffer,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<String> {
    let level = buf.level;
    let target_level = level + 1;

    let inputs = hydrate_summary_inputs(store, &buf.item_ids)?;
    if inputs.is_empty() {
        anyhow::bail!(
            "[tree_global::seal] refused to seal empty buffer tree_id={} level={}",
            tree.id,
            level
        );
    }

    let time_range_start = inputs
        .iter()
        .map(|i| i.time_range_start)
        .min()
        .unwrap_or_else(Utc::now);
    let time_range_end = inputs
        .iter()
        .map(|i| i.time_range_end)
        .max()
        .unwrap_or_else(Utc::now);
    let score = inputs
        .iter()
        .map(|i| i.score)
        .fold(f32::NEG_INFINITY, f32::max)
        .max(0.0);

    let ctx = SummaryContext {
        tree_id: &tree.id,
        tree_kind: TreeKind::Global,
        target_level,
        token_budget: GLOBAL_TOKEN_BUDGET,
    };
    let output = summariser
        .summarise(&inputs, &ctx)
        .await
        .context("summariser failed during global seal")?;

    // Union entity/topic labels from already-labeled inputs — global is a
    // sink; no second-pass extractor.
    let mut entities_set: BTreeSet<String> = BTreeSet::new();
    let mut topics_set: BTreeSet<String> = BTreeSet::new();
    for inp in &inputs {
        entities_set.extend(inp.entities.iter().cloned());
        topics_set.extend(inp.topics.iter().cloned());
    }

    // Embed BEFORE the write tx so an embed error aborts the seal cleanly.
    let embedding = embedder
        .embed(&output.content)
        .await
        .with_context(|| {
            format!(
                "embed global summary tree_id={} level={}",
                tree.id, level
            )
        })?;

    let now = Utc::now();
    let summary_id = new_summary_id(target_level);
    let node = SummaryNode {
        id: summary_id.clone(),
        tree_id: tree.id.clone(),
        tree_kind: TreeKind::Global,
        level: target_level,
        parent_id: None,
        child_ids: buf.item_ids.clone(),
        content: output.content,
        token_count: output.token_count,
        entities: entities_set.into_iter().collect(),
        topics: topics_set.into_iter().collect(),
        time_range_start,
        time_range_end,
        score,
        sealed_at: now,
        deleted: false,
        embedding: Some(embedding),
    };

    {
        let mut conn = store.lock_conn()?;
        let tx = conn.transaction()?;

        // Re-read max_level inside the tx so cascading seals see updated value.
        let current_max: u32 = tx
            .query_row(
                "SELECT max_level FROM mem_tree_trees WHERE id = ?1",
                rusqlite::params![&tree.id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n.max(0) as u32)
            .context("read current max_level for global tree")?;

        store::insert_summary_tx(&tx, &node)?;

        // Backlink children → new parent. In the global tree EVERY level
        // (incl. L0) holds global-owned summary nodes, so always backlink.
        for child_id in &node.child_ids {
            tx.execute(
                "UPDATE mem_tree_summaries SET parent_id = ?1 WHERE id = ?2 AND parent_id IS NULL",
                rusqlite::params![&summary_id, child_id],
            )
            .context("backlink global summary to parent")?;
        }

        store::clear_buffer_tx(&tx, &tree.id, level)?;

        // Append to parent buffer.
        let mut parent = store::get_buffer_conn(&tx, &tree.id, target_level)?;
        parent.item_ids.push(summary_id.clone());
        parent.token_sum = parent.token_sum.saturating_add(node.token_count as i64);
        parent.oldest_at = match parent.oldest_at {
            Some(existing) => Some(existing.min(time_range_start)),
            None => Some(time_range_start),
        };
        store::upsert_buffer_tx(&tx, &parent)?;

        // Update tree root/max_level if we just climbed; otherwise refresh timestamp.
        if target_level > current_max {
            store::update_tree_after_seal_tx(&tx, &tree.id, &summary_id, target_level, now)?;
        } else {
            store::refresh_last_sealed_tx(&tx, &tree.id, now)?;
        }

        tx.commit()?;
    }

    tracing::info!(
        tree_id = %tree.id,
        level = level,
        target_level = target_level,
        summary_id = %summary_id,
        children = buf.item_ids.len(),
        "[tree_global::seal] sealed"
    );

    Ok(summary_id)
}

/// Hydrate summary rows for buffer ids. Global buffers at every level hold
/// summary ids, so always pull from `mem_tree_summaries`.
pub(crate) fn hydrate_summary_inputs(
    store: &BucketSealStore,
    summary_ids: &[String],
) -> Result<Vec<SummaryInput>> {
    let mut out = Vec::with_capacity(summary_ids.len());
    for id in summary_ids {
        let Some(node) = store::get_summary(store, id)? else {
            tracing::warn!(
                summary_id = %id,
                "[tree_global::seal] hydrate: missing summary — skipping"
            );
            continue;
        };
        out.push(SummaryInput {
            id: node.id.clone(),
            content: node.content.clone(),
            token_count: node.token_count,
            entities: node.entities.clone(),
            topics: node.topics.clone(),
            time_range_start: node.time_range_start,
            time_range_end: node.time_range_end,
            score: node.score,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
    use crate::memory_bucket_seal::tree_source::summariser::inert::InertSummariser;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn fresh(
    ) -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    fn mk_daily(id: &str, tree_id: &str, day_ms: i64) -> SummaryNode {
        let ts = Utc.timestamp_millis_opt(day_ms).single().unwrap();
        SummaryNode {
            id: id.to_string(),
            tree_id: tree_id.to_string(),
            tree_kind: TreeKind::Global,
            level: 0,
            parent_id: None,
            child_ids: vec![],
            content: format!("daily digest {id}"),
            token_count: 200,
            entities: vec![],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.5,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        }
    }

    fn insert_daily(store: &BucketSealStore, node: &SummaryNode) {
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        store::insert_summary_tx(&tx, node).unwrap();
        tx.commit().unwrap();
    }

    #[tokio::test]
    async fn below_threshold_does_not_seal() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        for i in 0..3 {
            let node = mk_daily(
                &format!("summary:L0:day{i}"),
                &tree.id,
                1_700_000_000_000 + i,
            );
            insert_daily(&store, &node);
            let sealed = append_daily_and_cascade(&store, &tree, &node, &s, &e)
                .await
                .unwrap();
            assert!(sealed.is_empty());
        }
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids.len(), 3);
    }

    #[tokio::test]
    async fn crossing_weekly_threshold_seals_l1() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        for i in 0..WEEKLY_SEAL_THRESHOLD {
            let node = mk_daily(
                &format!("summary:L0:day{i}"),
                &tree.id,
                1_700_000_000_000 + i as i64,
            );
            insert_daily(&store, &node);
            let sealed = append_daily_and_cascade(&store, &tree, &node, &s, &e)
                .await
                .unwrap();
            if i + 1 < WEEKLY_SEAL_THRESHOLD {
                assert!(sealed.is_empty());
            } else {
                assert_eq!(sealed.len(), 1);
            }
        }
        let l0 = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert!(l0.is_empty());
        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert_eq!(l1.item_ids.len(), 1);
        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 1);
        let weekly = store::get_summary(&store, &l1.item_ids[0])
            .unwrap()
            .unwrap();
        assert_eq!(weekly.level, 1);
        assert_eq!(weekly.tree_kind, TreeKind::Global);
        assert_eq!(weekly.child_ids.len(), WEEKLY_SEAL_THRESHOLD);
    }

    #[tokio::test]
    async fn append_is_idempotent_on_retry() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        let node = mk_daily("summary:L0:dayA", &tree.id, 1_700_000_000_000);
        insert_daily(&store, &node);
        append_daily_and_cascade(&store, &tree, &node, &s, &e)
            .await
            .unwrap();
        append_daily_and_cascade(&store, &tree, &node, &s, &e)
            .await
            .unwrap();
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids.len(), 1);
        assert_eq!(buf.token_sum, 200);
    }

    #[tokio::test]
    async fn full_cascade_l0_to_l2() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();

        // 28 daily nodes = 4 weekly seals; the 4th weekly seal crosses the
        // monthly threshold (4) and seals L1→L2.
        let total = WEEKLY_SEAL_THRESHOLD * MONTHLY_SEAL_THRESHOLD; // 28
        for i in 0..total {
            let node = mk_daily(
                &format!("summary:L0:day{i}"),
                &tree.id,
                1_700_000_000_000 + i as i64,
            );
            insert_daily(&store, &node);
            append_daily_and_cascade(&store, &tree, &node, &s, &e)
                .await
                .unwrap();
        }

        // After 28 dailies: L0 empty, L1 empty (its 4 weeklies sealed to L2),
        // L2 holds exactly one monthly node.
        let l0 = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert!(l0.is_empty(), "L0 buffer should be empty after 28 dailies");
        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert!(l1.is_empty(), "L1 buffer should be empty after 4 weekly seals");
        let l2 = store::get_buffer(&store, &tree.id, 2).unwrap();
        assert_eq!(l2.item_ids.len(), 1, "L2 buffer holds one monthly node");

        // Tree climbed to level 2.
        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 2);

        // The monthly node has 4 weekly children, each weekly has 7 daily children.
        let monthly = store::get_summary(&store, &l2.item_ids[0]).unwrap().unwrap();
        assert_eq!(monthly.level, 2);
        assert_eq!(monthly.tree_kind, TreeKind::Global);
        assert_eq!(monthly.child_ids.len(), MONTHLY_SEAL_THRESHOLD);
    }
}
