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

pub use types::{
    DigestDailyPayload, FlushStalePayload, Job, JobKind, JobStatus, NewJob, SealPayload,
};
pub use store::{claim_next, count_by_status, enqueue, get_job, mark_done, mark_failed, recover_stale_locks, DEFAULT_LOCK_DURATION_MS};
pub use handlers::handle_job;
pub use testing::drain_until_idle;
