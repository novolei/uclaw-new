// SPDX-License-Identifier: Apache-2.0
//! Deterministic test runner — drains the queue with no wall-clock sleeps.

use std::sync::Arc;

use anyhow::Result;

use crate::memory_bucket_seal::jobs::{handlers, store as job_store};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::Summariser;

/// Claim + handle jobs until none are claimable. Settles each via
/// mark_done/mark_failed. Returns the number processed.
pub async fn drain_until_idle(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<usize> {
    let mut processed = 0usize;
    // Bound to avoid an infinite loop if a handler keeps re-enqueuing itself.
    for _ in 0..10_000 {
        let Some(job) = job_store::claim_next(store, job_store::DEFAULT_LOCK_DURATION_MS)? else {
            break;
        };
        match handlers::handle_job(store, summariser, embedder, &job).await {
            Ok(()) => job_store::mark_done(store, &job)?,
            Err(e) => job_store::mark_failed(store, &job, &format!("{e:#}"))?,
        }
        processed += 1;
    }
    Ok(processed)
}
