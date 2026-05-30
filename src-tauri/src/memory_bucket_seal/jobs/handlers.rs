// SPDX-License-Identifier: Apache-2.0
//! Per-kind job handlers. Dispatched by the worker via `handle_job`.

use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{Duration, NaiveDate, Utc};

use crate::memory_bucket_seal::jobs::store as job_store;
use crate::memory_bucket_seal::jobs::types::{
    DigestDailyPayload, FlushStalePayload, Job, JobKind, NewJob, SealPayload,
};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::Summariser;
use crate::memory_bucket_seal::tree_source::types::{TreeKind, DEFAULT_FLUSH_AGE_SECS};
use crate::memory_bucket_seal::tree_source::{cascade_all_from, LabelStrategy};
use crate::memory_bucket_seal::tree_source::store as tree_store;

/// Dispatch one claimed job to its handler.
pub async fn handle_job(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    job: &Job,
) -> Result<()> {
    match job.kind {
        JobKind::Seal => handle_seal(store, summariser, embedder, &job.payload_json).await,
        JobKind::DigestDaily => handle_digest(store, summariser, embedder, &job.payload_json).await,
        JobKind::FlushStale => handle_flush(store, &job.payload_json).await,
    }
}

async fn handle_seal(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    payload_json: &str,
) -> Result<()> {
    let p: SealPayload = serde_json::from_str(payload_json).context("parse SealPayload")?;
    let Some(tree) = tree_store::get_tree(store, &p.tree_id)? else {
        tracing::debug!(tree_id = %p.tree_id, "seal job: tree gone — nothing to seal");
        return Ok(());
    };
    let force_now = if p.force { Some(Utc::now()) } else { None };
    cascade_all_from(store, &tree, p.from_level, summariser, embedder, force_now, &LabelStrategy::Empty)
        .await
        .context("seal job cascade")?;
    Ok(())
}

async fn handle_digest(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    payload_json: &str,
) -> Result<()> {
    let p: DigestDailyPayload = serde_json::from_str(payload_json).context("parse DigestDailyPayload")?;
    let day = NaiveDate::parse_from_str(&p.date, "%Y-%m-%d").context("parse digest date")?;
    crate::memory_bucket_seal::tree_global::end_of_day_digest(store, day, summariser, embedder)
        .await
        .context("digest job")?;
    Ok(())
}

pub(crate) async fn handle_flush(store: &Arc<BucketSealStore>, payload_json: &str) -> Result<()> {
    let _p: FlushStalePayload = serde_json::from_str(payload_json).context("parse FlushStalePayload")?;
    let now = Utc::now();
    let max_age = Duration::seconds(DEFAULT_FLUSH_AGE_SECS);
    for kind in [TreeKind::Source, TreeKind::Topic, TreeKind::Global] {
        for tree in tree_store::list_trees_by_kind(store, kind)? {
            // Find the lowest stale buffer level in this tree.
            for level in 0..=tree.max_level {
                let buf = tree_store::get_buffer(store, &tree.id, level)?;
                if buf.is_stale(now, max_age) {
                    let _ = job_store::enqueue(
                        store,
                        &NewJob::seal(&SealPayload {
                            tree_id: tree.id.clone(),
                            from_level: level,
                            force: true,
                        })?,
                    );
                    break; // one forced seal per tree; cascade handles upward
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::jobs::store as job_store;
    use crate::memory_bucket_seal::jobs::testing::drain_until_idle;
    use crate::memory_bucket_seal::jobs::types::{DigestDailyPayload, NewJob};
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use tempfile::TempDir;

    fn fresh() -> (Arc<BucketSealStore>, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    #[tokio::test]
    async fn seal_job_on_missing_tree_is_ok() {
        let (store, s, e, _d) = fresh();
        let job = Job {
            id: "j1".into(),
            kind: JobKind::Seal,
            payload_json: serde_json::to_string(&SealPayload {
                tree_id: "gone".into(),
                from_level: 0,
                force: false,
            }).unwrap(),
            dedupe_key: "seal:gone".into(),
            status: crate::memory_bucket_seal::jobs::types::JobStatus::Running,
            attempts: 1,
            max_attempts: 5,
            available_at_ms: 0,
            locked_until_ms: None,
            last_error: None,
            created_at_ms: 0,
            started_at_ms: Some(0),
            completed_at_ms: None,
        };
        handle_job(&store, &s, &e, &job).await.unwrap(); // no panic, Ok
    }

    #[tokio::test]
    async fn flush_enqueues_seal_for_stale_buffer() {
        let (store, s, e, _d) = fresh();
        // Seed a source tree + a stale L0 buffer (oldest_at_ms = 0 → very old).
        let tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "slack:#eng").unwrap();
        {
            let conn = store.lock_conn().unwrap();
            // Insert a stale buffer row directly (oldest_at_ms = 0 → epoch = very old).
            conn.execute(
                "INSERT INTO mem_tree_buffers (tree_id, level, item_ids_json, token_sum, oldest_at_ms, updated_at_ms)
                 VALUES (?1, 0, '[\"x\"]', 100, 0, 0)
                 ON CONFLICT(tree_id, level) DO UPDATE SET
                     item_ids_json = excluded.item_ids_json,
                     token_sum = excluded.token_sum,
                     oldest_at_ms = excluded.oldest_at_ms,
                     updated_at_ms = excluded.updated_at_ms",
                rusqlite::params![tree.id],
            ).unwrap();
        }
        // Run flush directly.
        handle_flush(
            &store,
            &serde_json::to_string(&FlushStalePayload { date: "2026-05-30".into() }).unwrap(),
        )
        .await
        .unwrap();
        // A forced seal job for the tree should now exist.
        let conn = store.lock_conn().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mem_tree_jobs WHERE dedupe_key = ?1",
            rusqlite::params![format!("seal:{}", tree.id)],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(n, 1, "flush enqueues one seal for the stale tree");
        let _ = (s, e);
    }

    #[tokio::test]
    async fn drain_processes_enqueued_jobs() {
        let (store, s, e, _d) = fresh();
        // Enqueue a digest job for a day with no source material → EmptyDay → done.
        job_store::enqueue(
            &store,
            &NewJob::digest_daily(&DigestDailyPayload { date: "2026-01-01".into() }).unwrap(),
        ).unwrap();
        let processed = drain_until_idle(&store, &s, &e).await.unwrap();
        assert_eq!(processed, 1);
        // The job settled to done.
        let conn = store.lock_conn().unwrap();
        let status: String = conn.query_row(
            "SELECT status FROM mem_tree_jobs WHERE kind='digest_daily'",
            [],
            |r| r.get(0),
        ).unwrap();
        assert_eq!(status, "done");
    }
}
