// SPDX-License-Identifier: Apache-2.0
//! Daily scheduler: enqueues digest_daily(yesterday) + flush_stale(today)
//! at UTC 00:05. Stage-3 service. Manual trigger/backfill helpers included.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use chrono::{Duration as ChronoDuration, NaiveDate, Timelike, Utc};

use crate::memory_bucket_seal::jobs::store as job_store;
use crate::memory_bucket_seal::jobs::types::{DigestDailyPayload, FlushStalePayload, NewJob};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

pub struct JobSchedulerService {
    store: Arc<BucketSealStore>,
    running: Arc<AtomicBool>,
}

impl JobSchedulerService {
    pub fn new(store: Arc<BucketSealStore>) -> Self {
        Self { store, running: Arc::new(AtomicBool::new(false)) }
    }
}

/// Enqueue the daily jobs: digest for yesterday, flush for today. Both
/// deduped, so a duplicate/missed tick is harmless.
pub fn enqueue_daily_jobs(store: &Arc<BucketSealStore>) -> Result<()> {
    let now = Utc::now();
    let yesterday = (now.date_naive() - ChronoDuration::days(1)).format("%Y-%m-%d").to_string();
    let today = now.date_naive().format("%Y-%m-%d").to_string();
    let _ = job_store::enqueue(store, &NewJob::digest_daily(&DigestDailyPayload { date: yesterday })?);
    let _ = job_store::enqueue(store, &NewJob::flush_stale(&FlushStalePayload { date: today })?);
    Ok(())
}

/// Manually enqueue a digest for `date`. Idempotent (handler skips an
/// already-digested day; dedupe blocks a duplicate active job).
pub fn trigger_digest(store: &Arc<BucketSealStore>, date: NaiveDate) -> Result<Option<String>> {
    job_store::enqueue(store, &NewJob::digest_daily(&DigestDailyPayload { date: date.format("%Y-%m-%d").to_string() })?)
}

/// Enqueue digests for the last `days_back` calendar days (catch-up).
pub fn backfill_missing_digests(store: &Arc<BucketSealStore>, days_back: u32) -> Result<Vec<String>> {
    let today = Utc::now().date_naive();
    let mut ids = Vec::new();
    for d in 1..=days_back {
        let date = today - ChronoDuration::days(d as i64);
        if let Some(id) = trigger_digest(store, date)? {
            ids.push(id);
        }
    }
    Ok(ids)
}

/// Duration until the next UTC 00:05 tick.
fn next_tick_duration() -> Duration {
    let now = Utc::now();
    let secs_since_midnight = now.num_seconds_from_midnight() as i64;
    let target = 5 * 60; // 00:05:00
    let day = 24 * 60 * 60;
    let delta = if secs_since_midnight < target {
        target - secs_since_midnight
    } else {
        day - secs_since_midnight + target
    };
    Duration::from_secs(delta.max(1) as u64)
}

#[async_trait]
impl ManagedService for JobSchedulerService {
    fn name(&self) -> &str {
        "memory_jobs_scheduler"
    }

    async fn start(&self) -> Result<()> {
        self.running.store(true, Ordering::SeqCst);
        let store = self.store.clone();
        let running = self.running.clone();
        tokio::spawn(async move {
            while running.load(Ordering::SeqCst) {
                if let Err(e) = enqueue_daily_jobs(&store) {
                    tracing::warn!(error = %format!("{e:#}"), "scheduler enqueue failed");
                }
                tokio::time::sleep(next_tick_duration()).await;
            }
        });
        tracing::info!("[memory_jobs_scheduler] started");
        Ok(())
    }

    async fn stop(&self) -> Result<()> {
        self.running.store(false, Ordering::SeqCst);
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        if self.running.load(Ordering::SeqCst) { ServiceStatus::Running } else { ServiceStatus::Stopped }
    }

    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None,
            last_error: None,
            metrics: serde_json::json!({}),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::jobs::types::JobKind;
    use tempfile::TempDir;

    fn fresh() -> (Arc<BucketSealStore>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn enqueue_daily_creates_digest_and_flush() {
        let (store, _d) = fresh();
        enqueue_daily_jobs(&store).unwrap();
        let conn = store.lock_conn().unwrap();
        let digest: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mem_tree_jobs WHERE kind='digest_daily'", [], |r| r.get(0),
        ).unwrap();
        let flush: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mem_tree_jobs WHERE kind='flush_stale'", [], |r| r.get(0),
        ).unwrap();
        assert_eq!(digest, 1);
        assert_eq!(flush, 1);
        let _ = JobKind::Seal; // compile check
    }

    #[test]
    fn trigger_digest_idempotent() {
        let (store, _d) = fresh();
        let date = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        assert!(trigger_digest(&store, date).unwrap().is_some());
        assert!(trigger_digest(&store, date).unwrap().is_none(), "second active digest deduped");
    }
}
