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
    use crate::memory_bucket_seal::tree_source::store as tree_store;

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

    /// End-to-end test: durable Seal job → worker claims → handle_seal →
    /// real cascade → `mem_tree_summaries` row persisted → mark_done.
    ///
    /// This is the headline guarantee of PR13: a Seal job enqueued by
    /// `store()` flows through the worker and produces a real summary row.
    #[tokio::test]
    async fn seal_job_through_worker_persists_summary() {
        use crate::memory_bucket_seal::{stage_chunks, Chunk, Metadata, SourceKind};
        use crate::memory_bucket_seal::tree_source::{
            append_leaf_deferred, get_or_create_source_tree, LeafRef,
        };
        use chrono::{TimeZone, Utc};

        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());

        // 1. Create a source tree.
        let tree = get_or_create_source_tree(&store, "e2e_worker_seal").unwrap();

        // 2. Seed a real chunk into mem_tree_chunks so hydrate_leaf_inputs finds it.
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let chunk = Chunk {
            id: "e2e_chunk_0001".to_string(),
            content: "End-to-end worker seal test content with sufficient signal.".to_string(),
            metadata: Metadata {
                source_kind: SourceKind::Document,
                source_id: "e2e_worker_seal".to_string(),
                owner: "test".to_string(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec![],
                source_ref: None,
            },
            token_count: 20,
            seq_in_source: 0,
            created_at: ts,
            partial_message: false,
        };
        let staged = stage_chunks(&dir.path().join("content"), &[chunk.clone()]).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        // 3. Append the chunk to the L0 buffer via append_leaf_deferred.
        let leaf = LeafRef {
            chunk_id: chunk.id.clone(),
            token_count: chunk.token_count,
            timestamp: ts,
            content: chunk.content.clone(),
            entities: vec![],
            topics: vec![],
            score: 0.5,
        };
        append_leaf_deferred(&store, &tree, &leaf).unwrap();

        // 4. Enqueue a forced seal (force:true bypasses the token-budget gate,
        //    so even a single chunk in the buffer is sealed immediately).
        job_store::enqueue(
            &store,
            &NewJob::seal(&SealPayload { tree_id: tree.id.clone(), from_level: 0, force: true }).unwrap(),
        ).unwrap();

        // 5. Drain the worker queue until idle.
        let processed = drain_until_idle(&store, &summariser, &embedder).await.unwrap();
        assert_eq!(processed, 1, "exactly one seal job must be processed");

        // 6. Assert the Seal job settled to done.
        {
            let conn = store.lock_conn().unwrap();
            let status: String = conn.query_row(
                "SELECT status FROM mem_tree_jobs WHERE kind='seal'", [], |r| r.get(0),
            ).unwrap();
            assert_eq!(status, "done", "seal job must settle to done via the worker path");
        }

        // 7. Assert at least one summary row was persisted via the worker→cascade path.
        let n = tree_store::count_summaries(&store, &tree.id).unwrap();
        assert!(
            n >= 1,
            "worker→handle_seal→cascade must persist at least one mem_tree_summaries row; got {n}"
        );
    }
}
