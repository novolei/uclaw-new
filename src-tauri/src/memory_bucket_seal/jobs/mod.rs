// SPDX-License-Identifier: Apache-2.0
//! Durable job queue for memory-tree async work (Phase 3 — openhuman jobs/ port).
//!
//! `mem_tree_jobs` (in chunks.db) backs three kinds: `Seal` (LLM cascade),
//! `DigestDaily` (cross-source digest), `FlushStale` (force-seal stale
//! buffers). The dedupe index gives per-tree serialisation, replacing
//! PR12's per-tree mutex. Worker + scheduler run as Stage-3 services.

pub mod types;

pub use types::{
    DigestDailyPayload, FlushStalePayload, Job, JobKind, JobStatus, NewJob, SealPayload,
};
