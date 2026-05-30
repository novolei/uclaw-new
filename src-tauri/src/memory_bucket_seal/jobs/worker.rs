// SPDX-License-Identifier: Apache-2.0
//! Worker pool: claims jobs, dispatches via handlers, settles. Stage-3 service.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::Semaphore;

use crate::memory_bucket_seal::jobs::{handlers, store as job_store};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::Summariser;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

const POLL_INTERVAL: Duration = Duration::from_secs(2);

pub struct JobWorkerService {
    store: Arc<BucketSealStore>,
    summariser: Arc<dyn Summariser>,
    embedder: Arc<dyn Embedder>,
    worker_count: usize,
    llm_permits: Arc<Semaphore>,
    running: Arc<AtomicBool>,
}

impl JobWorkerService {
    pub fn new(
        store: Arc<BucketSealStore>,
        summariser: Arc<dyn Summariser>,
        embedder: Arc<dyn Embedder>,
    ) -> Self {
        Self {
            store,
            summariser,
            embedder,
            worker_count: 2,
            llm_permits: Arc::new(Semaphore::new(2)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }
}

/// Claim + handle one job. Returns true if a job was processed. LLM-bound
/// kinds hold a permit for the handler duration.
pub async fn run_once(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    llm_permits: &Arc<Semaphore>,
) -> Result<bool> {
    let Some(job) = job_store::claim_next(store, job_store::DEFAULT_LOCK_DURATION_MS)? else {
        return Ok(false);
    };
    let _permit = if job.kind.is_llm_bound() {
        Some(llm_permits.clone().acquire_owned().await.expect("semaphore not closed"))
    } else {
        None
    };
    match handlers::handle_job(store, summariser, embedder, &job).await {
        Ok(()) => job_store::mark_done(store, &job)?,
        Err(e) => {
            tracing::warn!(job_id = %job.id, kind = %job.kind.as_str(), error = %format!("{e:#}"), "job failed");
            job_store::mark_failed(store, &job, &format!("{e:#}"))?;
        }
    }
    Ok(true)
}

#[async_trait]
impl ManagedService for JobWorkerService {
    fn name(&self) -> &str {
        "memory_jobs_worker"
    }

    async fn start(&self) -> Result<()> {
        if self.running.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst).is_err() {
            tracing::debug!("[{}] already running — start() ignored", self.name());
            return Ok(());
        }
        // Recover any leases orphaned by a previous crash.
        if let Err(e) = job_store::recover_stale_locks(&self.store) {
            tracing::warn!(error = %format!("{e:#}"), "recover_stale_locks failed at startup");
        }
        for _ in 0..self.worker_count {
            let store = self.store.clone();
            let summariser = self.summariser.clone();
            let embedder = self.embedder.clone();
            let permits = self.llm_permits.clone();
            let running = self.running.clone();
            tokio::spawn(async move {
                while running.load(Ordering::SeqCst) {
                    match run_once(&store, &summariser, &embedder, &permits).await {
                        Ok(true) => {}                                        // got work; loop again immediately
                        Ok(false) => tokio::time::sleep(POLL_INTERVAL).await, // idle; back off
                        Err(e) => {
                            tracing::warn!(error = %format!("{e:#}"), "worker run_once error");
                            tokio::time::sleep(Duration::from_secs(1)).await;
                        }
                    }
                }
            });
        }
        tracing::info!(workers = self.worker_count, "[memory_jobs_worker] started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        if self.running.load(Ordering::SeqCst) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None,
            last_error: None,
            metrics: serde_json::json!({ "workers": self.worker_count }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::jobs::store as job_store;
    use crate::memory_bucket_seal::jobs::types::{DigestDailyPayload, NewJob};
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use tempfile::TempDir;

    #[tokio::test]
    async fn run_once_processes_one_job() {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let permits = Arc::new(Semaphore::new(2));

        assert!(!run_once(&store, &s, &e, &permits).await.unwrap(), "no jobs → false");
        job_store::enqueue(
            &store,
            &NewJob::digest_daily(&DigestDailyPayload { date: "2026-01-01".into() }).unwrap(),
        ).unwrap();
        assert!(run_once(&store, &s, &e, &permits).await.unwrap(), "one job → true");
        assert!(!run_once(&store, &s, &e, &permits).await.unwrap(), "queue empty again");
    }
}
