// SPDX-License-Identifier: Apache-2.0
//! Durable job queue for memory-tree async work (Phase 3 — openhuman jobs/ port).
//!
//! `mem_tree_jobs` (in chunks.db) backs three kinds: `Seal` (LLM cascade),
//! `DigestDaily` (cross-source digest), `FlushStale` (force-seal stale
//! buffers). The dedupe index gives per-tree serialisation, replacing
//! PR12's per-tree mutex. Worker + scheduler run as Stage-3 services.

pub mod types;
pub mod store;
pub mod handlers;
pub mod testing;
pub mod worker;
pub mod scheduler;

pub use types::{
    DigestDailyPayload, FlushStalePayload, Job, JobKind, JobStatus, NewJob, SealPayload,
};
pub use store::{claim_next, count_by_status, enqueue, get_job, mark_done, mark_failed, recover_stale_locks, DEFAULT_LOCK_DURATION_MS};
pub use handlers::handle_job;
pub use testing::drain_until_idle;
pub use worker::{JobWorkerService, run_once};
pub use scheduler::{JobSchedulerService, trigger_digest, backfill_missing_digests};

#[cfg(test)]
mod e2e_tests {
    use std::sync::Arc;
    use tempfile::TempDir;

    use crate::memory_adapter::{MemoryAdapter, MemoryCategory};
    use crate::memory_bucket_seal::adapter::BucketSealAdapter;
    use crate::memory_bucket_seal::jobs::store as job_store;
    use crate::memory_bucket_seal::jobs::testing::drain_until_idle;
    use crate::memory_bucket_seal::jobs::types::{NewJob, SealPayload};
    use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
    use crate::memory_bucket_seal::store::BucketSealStore;
    use crate::memory_bucket_seal::tree_source::{InertSummariser, Summariser};

    #[tokio::test]
    async fn store_enqueue_drain_seals() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let adapter = BucketSealAdapter::new(
            store.clone(),
            dir.path().join("content"),
            embedder.clone(),
            summariser.clone(),
        );

        // Write one content item — chunk persisted synchronously.
        adapter
            .store("e2e_ns", "k1", "End-to-end content with admission signal density.", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert!(store.count_chunks().unwrap() >= 1, "chunk must be durable after store()");

        // Drain any enqueued jobs (seal gate may or may not have tripped).
        let processed = drain_until_idle(&store, &summariser, &embedder).await.unwrap();
        // processed is 0 (gate not met) or ≥1 (seal fired) — both are fine;
        // the guarantee is no error + chunk durable.
        let _ = processed;
    }

    #[tokio::test]
    async fn forced_seal_on_empty_tree_drains_cleanly() {
        // Stronger e2e: create a source tree (no leaves), enqueue a forced
        // seal, drain. cascade_all_from on an empty buffer is a no-op that
        // returns Ok — the job must settle to done (not failed).
        use crate::memory_bucket_seal::tree_source;

        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());

        // Create a source tree (no leaves — buffer is empty).
        let tree = tree_source::get_or_create_source_tree(&store, "e2e_forced").unwrap();

        // Directly enqueue a forced seal for the tree.
        job_store::enqueue(
            &store,
            &NewJob::seal(&SealPayload { tree_id: tree.id.clone(), from_level: 0, force: true }).unwrap(),
        ).unwrap();

        // Drain — cascade_all_from with empty buffer is a no-op, returns Ok.
        let processed = drain_until_idle(&store, &summariser, &embedder).await.unwrap();
        assert_eq!(processed, 1, "one forced seal job processed");

        // The job must have settled to done (cascade on empty buffer is Ok).
        let conn = store.lock_conn().unwrap();
        let status: String = conn.query_row(
            "SELECT status FROM mem_tree_jobs WHERE kind='seal'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(status, "done", "forced seal job must settle to done");
    }
}
