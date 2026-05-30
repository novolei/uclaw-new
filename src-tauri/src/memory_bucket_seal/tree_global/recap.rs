// SPDX-License-Identifier: Apache-2.0
//! Window-scoped recap retrieval for the global activity tree (Phase 3b).
//!
//! Given a duration, pick the tree level matching the time axis and return
//! the covering summaries. Falls back DOWN when the chosen level hasn't
//! sealed yet, reporting the actual `level_used`.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::types::SummaryNode;

/// Aggregated recap returned to the caller.
#[derive(Debug, Clone)]
pub struct RecapOutput {
    pub content: String,
    pub time_range: (DateTime<Utc>, DateTime<Utc>),
    pub level_used: u32,
    pub summary_ids: Vec<String>,
}

/// Recap for the given window, or `None` if no global summaries have sealed.
pub async fn recap(store: &BucketSealStore, window: Duration) -> Result<Option<RecapOutput>> {
    let target_level = pick_level(window);
    let global = get_or_create_global_tree(store)?;
    let now = Utc::now();
    let window_start = now - window;

    for level in (0..=target_level).rev() {
        let all_at_level = store::list_summaries_at_level(store, &global.id, level)?;
        if all_at_level.is_empty() {
            continue;
        }
        let covering = pick_covering(&all_at_level, window_start, now);
        if covering.is_empty() {
            continue;
        }
        return Ok(Some(assemble_recap(&covering, level)));
    }
    Ok(None)
}

/// Map a window duration to a level. <2d→0, <14d→1, <60d→2, ≥60d→3.
pub fn pick_level(window: Duration) -> u32 {
    if window < Duration::days(2) {
        0
    } else if window < Duration::days(14) {
        1
    } else if window < Duration::days(60) {
        2
    } else {
        3
    }
}

/// Summaries at a level overlapping [window_start, now], oldest→newest.
/// Falls back to the single latest-sealed when none overlap.
fn pick_covering<'a>(
    summaries: &'a [SummaryNode],
    window_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<&'a SummaryNode> {
    let mut overlapping: Vec<&SummaryNode> = summaries
        .iter()
        .filter(|s| s.time_range_end >= window_start && s.time_range_start <= now)
        .collect();
    overlapping.sort_by_key(|s| s.time_range_start);
    if overlapping.is_empty() {
        if let Some(latest) = summaries.iter().max_by_key(|s| s.sealed_at) {
            return vec![latest];
        }
    }
    overlapping
}

fn assemble_recap(covering: &[&SummaryNode], level: u32) -> RecapOutput {
    let mut parts = Vec::with_capacity(covering.len());
    let mut summary_ids = Vec::with_capacity(covering.len());
    for s in covering {
        parts.push(format!(
            "[{} → {}]\n{}",
            s.time_range_start.to_rfc3339(),
            s.time_range_end.to_rfc3339(),
            s.content
        ));
        summary_ids.push(s.id.clone());
    }
    let time_start = covering
        .iter()
        .map(|s| s.time_range_start)
        .min()
        .unwrap_or_else(Utc::now);
    let time_end = covering
        .iter()
        .map(|s| s.time_range_end)
        .max()
        .unwrap_or_else(Utc::now);
    RecapOutput {
        content: parts.join("\n\n"),
        time_range: (time_start, time_end),
        level_used: level,
        summary_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
    use crate::memory_bucket_seal::tree_global::digest::{end_of_day_digest, DigestOutcome};
    use crate::memory_bucket_seal::tree_source::store as tstore;
    use crate::memory_bucket_seal::tree_source::summariser::{inert::InertSummariser, Summariser};
    use crate::memory_bucket_seal::tree_source::types::{SummaryNode, TreeKind};
    use chrono::NaiveDate;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    fn seed_source_summary(store: &BucketSealStore, scope: &str, day: NaiveDate) {
        let tree =
            crate::memory_bucket_seal::tree_source::get_or_create_source_tree(store, scope)
                .unwrap();
        let ts = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let node = SummaryNode {
            id: format!("summary:L1:{scope}"),
            tree_id: tree.id.clone(),
            tree_kind: TreeKind::Source,
            level: 1,
            parent_id: None,
            child_ids: vec![],
            content: format!("src {scope}"),
            token_count: 300,
            entities: vec![],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.5,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        };
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        tstore::insert_summary_tx(&tx, &node).unwrap();
        tstore::update_tree_after_seal_tx(&tx, &tree.id, &node.id, 1, ts).unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn pick_level_matches_thresholds() {
        assert_eq!(pick_level(Duration::hours(1)), 0);
        assert_eq!(pick_level(Duration::days(1)), 0);
        assert_eq!(pick_level(Duration::days(2)), 1);
        assert_eq!(pick_level(Duration::days(13)), 1);
        assert_eq!(pick_level(Duration::days(14)), 2);
        assert_eq!(pick_level(Duration::days(59)), 2);
        assert_eq!(pick_level(Duration::days(60)), 3);
        assert_eq!(pick_level(Duration::days(365)), 3);
    }

    #[tokio::test]
    async fn recap_on_empty_tree_returns_none() {
        let (store, _s, _e, _dir) = fresh();
        let out = recap(&store, Duration::days(7)).await.unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn recap_one_day_returns_latest_l0() {
        let (store, s, e, _dir) = fresh();
        let day = Utc::now().date_naive();
        seed_source_summary(&store, "slack:#eng", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(outcome, DigestOutcome::Emitted { .. }));
        let r = recap(&store, Duration::hours(24))
            .await
            .unwrap()
            .expect("recap");
        assert_eq!(r.level_used, 0);
        assert_eq!(r.summary_ids.len(), 1);
        assert!(!r.content.is_empty());
    }

    #[tokio::test]
    async fn recap_weekly_window_falls_back_to_l0_when_no_l1() {
        let (store, s, e, _dir) = fresh();
        let today = Utc::now().date_naive();
        for i in 0..3 {
            let day = today - Duration::days(2 - i);
            seed_source_summary(&store, &format!("slack:#d{i}"), day);
            end_of_day_digest(&store, day, &s, &e).await.unwrap();
        }
        let r = recap(&store, Duration::days(7))
            .await
            .unwrap()
            .expect("fallback recap");
        assert_eq!(r.level_used, 0);
        assert_eq!(r.summary_ids.len(), 3);
    }
}
