// SPDX-License-Identifier: Apache-2.0
//! End-of-day digest builder for the global activity tree (Phase 3b).
//!
//! Once per calendar day: walk every active source tree, collect the
//! summary material covering that day, fold it into one cross-source recap,
//! persist as an L0 node in the singleton global tree, then cascade-seal.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use rusqlite::OptionalExtension;

use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
use crate::memory_bucket_seal::tree_global::seal::append_daily_and_cascade;
use crate::memory_bucket_seal::tree_global::GLOBAL_TOKEN_BUDGET;
use crate::memory_bucket_seal::tree_source::registry::new_summary_id;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::summariser::{Summariser, SummaryContext, SummaryInput};
use crate::memory_bucket_seal::tree_source::types::{SummaryNode, Tree, TreeKind};

/// Outcome of one `end_of_day_digest` call.
#[derive(Debug, Clone)]
pub enum DigestOutcome {
    /// Emitted one L0 daily node; `sealed_ids` lists any L1/L2/L3 nodes
    /// that sealed during the cascade.
    Emitted {
        daily_id: String,
        source_count: usize,
        sealed_ids: Vec<String>,
    },
    /// No source tree had material for the day — nothing written.
    EmptyDay,
    /// An L0 node already exists for the day (re-run) — nothing written.
    Skipped { existing_id: String },
}

/// Run an end-of-day digest for `day` (UTC calendar date). Appends one L0
/// node to the global tree + cascade-seals if thresholds cross.
pub async fn end_of_day_digest(
    store: &BucketSealStore,
    day: NaiveDate,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<DigestOutcome> {
    let (day_start, day_end) = day_bounds_utc(day)?;
    let global = get_or_create_global_tree(store)?;

    if let Some(existing) = find_existing_daily(store, &global.id, day_start)? {
        return Ok(DigestOutcome::Skipped {
            existing_id: existing.id,
        });
    }

    let source_trees = store::list_trees_by_kind(store, TreeKind::Source)?;
    let mut inputs: Vec<SummaryInput> = Vec::with_capacity(source_trees.len());
    for source_tree in &source_trees {
        if let Some(inp) = pick_source_contribution(store, source_tree, day_start, day_end)? {
            inputs.push(inp);
        }
    }

    if inputs.is_empty() {
        return Ok(DigestOutcome::EmptyDay);
    }

    let ctx = SummaryContext {
        tree_id: &global.id,
        tree_kind: TreeKind::Global,
        target_level: 0,
        token_budget: GLOBAL_TOKEN_BUDGET,
    };
    let output = summariser
        .summarise(&inputs, &ctx)
        .await
        .context("summariser failed during end-of-day digest")?;

    let score = inputs
        .iter()
        .map(|i| i.score)
        .fold(f32::NEG_INFINITY, f32::max)
        .max(0.0);

    let embedding = embedder
        .embed(&output.content)
        .await
        .context("embed daily summary during end_of_day_digest")?;

    let mut entities_set: BTreeSet<String> = BTreeSet::new();
    let mut topics_set: BTreeSet<String> = BTreeSet::new();
    for inp in &inputs {
        entities_set.extend(inp.entities.iter().cloned());
        topics_set.extend(inp.topics.iter().cloned());
    }

    let now = Utc::now();
    let daily_id = new_summary_id(0);
    let source_count = inputs.len();
    let daily = SummaryNode {
        id: daily_id.clone(),
        tree_id: global.id.clone(),
        tree_kind: TreeKind::Global,
        level: 0,
        parent_id: None,
        child_ids: inputs.iter().map(|i| i.id.clone()).collect(),
        content: output.content,
        token_count: output.token_count,
        entities: entities_set.into_iter().collect(),
        topics: topics_set.into_iter().collect(),
        time_range_start: day_start,
        time_range_end: day_end,
        score,
        sealed_at: now,
        deleted: false,
        embedding: Some(embedding),
    };

    // Persist the daily node. Do NOT backlink child_ids — those are
    // source-tree summary references owned by their source trees, not the
    // global tree.
    {
        let mut conn = store.lock_conn()?;
        let tx = conn.transaction()?;
        store::insert_summary_tx(&tx, &daily)?;
        tx.commit()?;
    }

    let sealed_ids =
        append_daily_and_cascade(store, &global, &daily, summariser, embedder).await?;

    Ok(DigestOutcome::Emitted {
        daily_id: daily.id,
        source_count,
        sealed_ids,
    })
}

/// [00:00, +24h) UTC bounds for a calendar day.
pub(crate) fn day_bounds_utc(day: NaiveDate) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let start_naive = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("invalid day {day} — failed to build 00:00 timestamp"))?;
    let start = Utc
        .from_local_datetime(&start_naive)
        .single()
        .ok_or_else(|| anyhow::anyhow!("non-unique UTC time for day {day}"))?;
    Ok((start, start + Duration::days(1)))
}

/// Existing L0 daily node for this day, matched on time_range_start_ms.
fn find_existing_daily(
    store: &BucketSealStore,
    global_tree_id: &str,
    day_start: DateTime<Utc>,
) -> Result<Option<SummaryNode>> {
    let start_ms = day_start.timestamp_millis();
    let opt_id: Option<String> = {
        let conn = store.lock_conn()?;
        conn.query_row(
            "SELECT id FROM mem_tree_summaries
              WHERE tree_id = ?1 AND level = 0 AND time_range_start_ms = ?2 AND deleted = 0
              LIMIT 1",
            rusqlite::params![global_tree_id, start_ms],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .context("query for existing daily node")?
    };
    match opt_id {
        Some(id) => store::get_summary(store, &id),
        None => Ok(None),
    }
}

/// Best single contribution from one source tree for the target day.
/// Priority: (1) latest summary intersecting the day window; (2) else the
/// tree's current root summary; (3) else None (no sealed summaries yet).
fn pick_source_contribution(
    store: &BucketSealStore,
    source_tree: &Tree,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Option<SummaryInput>> {
    let start_ms = day_start.timestamp_millis();
    let end_ms = day_end.timestamp_millis();
    let intersecting_id: Option<String> = {
        let conn = store.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM mem_tree_summaries
              WHERE tree_id = ?1 AND deleted = 0
                AND time_range_start_ms < ?3 AND time_range_end_ms >= ?2
              ORDER BY level DESC, sealed_at_ms DESC
              LIMIT 1",
        )?;
        stmt.query_row(
            rusqlite::params![&source_tree.id, start_ms, end_ms],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .context("query intersecting source summary")?
    };

    let chosen_id = intersecting_id.or_else(|| source_tree.root_id.clone());
    let Some(id) = chosen_id else {
        return Ok(None);
    };

    let Some(node) = store::get_summary(store, &id)? else {
        return Ok(None);
    };

    Ok(Some(SummaryInput {
        id: node.id.clone(),
        // node.content is a ≤500-char preview (PR8). Full-body read lands
        // in PR12 with content_store::read. Prefix scope for provenance.
        content: format!("[{}]\n{}", source_tree.scope, node.content),
        token_count: node.token_count,
        entities: node.entities,
        topics: node.topics,
        time_range_start: node.time_range_start,
        time_range_end: node.time_range_end,
        score: node.score,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::store as tstore;
    use crate::memory_bucket_seal::tree_source::summariser::inert::InertSummariser;
    use crate::memory_bucket_seal::tree_source::types::TreeKind;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    /// Seed a source tree with one L1 summary covering `day`, set as its root.
    fn seed_source_with_summary(store: &BucketSealStore, scope: &str, day: NaiveDate) -> String {
        let tree =
            crate::memory_bucket_seal::tree_source::get_or_create_source_tree(store, scope)
                .unwrap();
        let ts = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let summary_id = format!("summary:L1:{scope}");
        let node = SummaryNode {
            id: summary_id.clone(),
            tree_id: tree.id.clone(),
            tree_kind: TreeKind::Source,
            level: 1,
            parent_id: None,
            child_ids: vec![],
            content: format!("source summary for {scope}"),
            token_count: 300,
            entities: vec![format!("Entity_{scope}")],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.7,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        };
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        tstore::insert_summary_tx(&tx, &node).unwrap();
        tstore::update_tree_after_seal_tx(&tx, &tree.id, &summary_id, 1, ts).unwrap();
        tx.commit().unwrap();
        summary_id
    }

    #[tokio::test]
    async fn empty_day_is_noop() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(outcome, DigestOutcome::EmptyDay));
    }

    #[tokio::test]
    async fn populated_day_emits_l0_daily() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#eng", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        match outcome {
            DigestOutcome::Emitted { source_count, .. } => assert_eq!(source_count, 1),
            other => panic!("expected Emitted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rerun_same_day_skips() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#eng", day);
        end_of_day_digest(&store, day, &s, &e).await.unwrap();
        let second = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(second, DigestOutcome::Skipped { .. }));
    }

    #[tokio::test]
    async fn multi_source_fold_counts_all() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#a", day);
        seed_source_with_summary(&store, "slack:#b", day);
        seed_source_with_summary(&store, "email:gmail", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        match outcome {
            DigestOutcome::Emitted { source_count, .. } => assert_eq!(source_count, 3),
            other => panic!("expected Emitted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pick_falls_back_to_root_when_no_intersecting_summary() {
        let (store, s, e, _dir) = fresh();
        // Seed a source summary covering an OLD day (far from the digest target).
        let old_day = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        seed_source_with_summary(&store, "slack:#eng", old_day);

        // Run the digest for a DIFFERENT day. No summary intersects this day,
        // so pick_source_contribution falls back to the tree's root_id (the
        // old summary, since it's the root). The source still contributes.
        let target_day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let outcome = end_of_day_digest(&store, target_day, &s, &e).await.unwrap();
        match outcome {
            DigestOutcome::Emitted { source_count, .. } => {
                assert_eq!(source_count, 1, "root_id fallback should still contribute the source");
            }
            other => panic!("expected Emitted via root fallback, got {other:?}"),
        }
    }
}
