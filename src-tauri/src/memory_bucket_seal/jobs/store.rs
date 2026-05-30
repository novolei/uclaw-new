// SPDX-License-Identifier: Apache-2.0
//! `mem_tree_jobs` persistence — enqueue, claim, settle, recover. Kind-agnostic.

use anyhow::{Context, Result};
use rusqlite::OptionalExtension;

use crate::memory_bucket_seal::jobs::types::{Job, JobKind, JobStatus, NewJob};
use crate::memory_bucket_seal::store::BucketSealStore;

pub const DEFAULT_LOCK_DURATION_MS: i64 = 5 * 60 * 1000;
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;
const BASE_BACKOFF_MS: i64 = 2_000;
const CAP_BACKOFF_MS: i64 = 300_000;

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn backoff_ms(attempts: u32) -> i64 {
    let exp = attempts.min(8); // guard pow overflow
    (BASE_BACKOFF_MS.saturating_mul(1i64 << exp)).min(CAP_BACKOFF_MS)
}

const JOB_COLS: &str = "id, kind, payload_json, dedupe_key, status, attempts, max_attempts, \
                        available_at_ms, locked_until_ms, last_error, created_at_ms, \
                        started_at_ms, completed_at_ms";

fn row_to_job(row: &rusqlite::Row<'_>) -> rusqlite::Result<Job> {
    let kind_s: String = row.get(1)?;
    let status_s: String = row.get(4)?;
    Ok(Job {
        id: row.get(0)?,
        kind: JobKind::parse(&kind_s)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                1,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            ))?,
        payload_json: row.get(2)?,
        dedupe_key: row.get(3)?,
        status: JobStatus::parse(&status_s)
            .map_err(|e| rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string())),
            ))?,
        attempts: row.get::<_, i64>(5)? as u32,
        max_attempts: row.get::<_, i64>(6)? as u32,
        available_at_ms: row.get(7)?,
        locked_until_ms: row.get(8)?,
        last_error: row.get(9)?,
        created_at_ms: row.get(10)?,
        started_at_ms: row.get(11)?,
        completed_at_ms: row.get(12)?,
    })
}

/// Enqueue a job. Idempotent on dedupe_key while an active (ready/running)
/// row shares it. Returns `Some(id)` on insert, `None` when deduped.
pub fn enqueue(store: &BucketSealStore, job: &NewJob) -> Result<Option<String>> {
    let conn = store.lock_conn()?;
    enqueue_conn(&conn, job)
}

/// Enqueue inside a caller-held connection/tx (atomic producer).
pub fn enqueue_conn(conn: &rusqlite::Connection, job: &NewJob) -> Result<Option<String>> {
    let max_attempts = job.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS) as i64;
    let now = now_ms();
    let changed = conn.execute(
        "INSERT OR IGNORE INTO mem_tree_jobs
            (id, kind, payload_json, dedupe_key, status, attempts, max_attempts,
             available_at_ms, created_at_ms)
         VALUES (?1, ?2, ?3, ?4, 'ready', 0, ?5, ?6, ?6)",
        rusqlite::params![
            job.id, job.kind.as_str(), job.payload_json, job.dedupe_key, max_attempts, now
        ],
    )
    .context("INSERT mem_tree_jobs")?;
    Ok(if changed == 1 { Some(job.id.clone()) } else { None })
}

/// Atomically claim the next due ready job. Single UPDATE...RETURNING.
pub fn claim_next(store: &BucketSealStore, lock_duration_ms: i64) -> Result<Option<Job>> {
    let conn = store.lock_conn()?;
    let now = now_ms();
    let sql = format!(
        "UPDATE mem_tree_jobs
            SET status='running', attempts=attempts+1, started_at_ms=?1, locked_until_ms=?2
          WHERE id = (SELECT id FROM mem_tree_jobs
                       WHERE status='ready' AND available_at_ms <= ?1
                       ORDER BY available_at_ms LIMIT 1)
        RETURNING {JOB_COLS}"
    );
    let job = conn
        .query_row(&sql, rusqlite::params![now, now + lock_duration_ms], row_to_job)
        .optional()
        .context("claim_next")?;
    Ok(job)
}

/// Mark a claimed job done. Gated on (id, attempts, started_at_ms) so a
/// stale worker's settle is a no-op.
pub fn mark_done(store: &BucketSealStore, job: &Job) -> Result<()> {
    let conn = store.lock_conn()?;
    conn.execute(
        "UPDATE mem_tree_jobs
            SET status='done', completed_at_ms=?1, locked_until_ms=NULL
          WHERE id=?2 AND attempts=?3 AND started_at_ms IS ?4",
        rusqlite::params![now_ms(), job.id, job.attempts as i64, job.started_at_ms],
    )
    .context("mark_done")?;
    Ok(())
}

/// Mark a failure. Retries with exponential backoff until max_attempts,
/// then status='failed'. Claim-token gated.
pub fn mark_failed(store: &BucketSealStore, job: &Job, err: &str) -> Result<()> {
    let conn = store.lock_conn()?;
    if job.attempts >= job.max_attempts {
        conn.execute(
            "UPDATE mem_tree_jobs
                SET status='failed', last_error=?1, completed_at_ms=?2, locked_until_ms=NULL
              WHERE id=?3 AND attempts=?4 AND started_at_ms IS ?5",
            rusqlite::params![err, now_ms(), job.id, job.attempts as i64, job.started_at_ms],
        )
        .context("mark_failed terminal")?;
    } else {
        let next = now_ms() + backoff_ms(job.attempts);
        conn.execute(
            "UPDATE mem_tree_jobs
                SET status='ready', last_error=?1, available_at_ms=?2, locked_until_ms=NULL
              WHERE id=?3 AND attempts=?4 AND started_at_ms IS ?5",
            rusqlite::params![err, next, job.id, job.attempts as i64, job.started_at_ms],
        )
        .context("mark_failed retry")?;
    }
    Ok(())
}

/// Voluntary requeue without consuming the failure budget.
pub fn mark_deferred(store: &BucketSealStore, job: &Job, retry_after_ms: i64) -> Result<()> {
    let conn = store.lock_conn()?;
    conn.execute(
        "UPDATE mem_tree_jobs
            SET status='ready', attempts=attempts-1, available_at_ms=?1, locked_until_ms=NULL
          WHERE id=?2 AND attempts=?3 AND started_at_ms IS ?4",
        rusqlite::params![now_ms() + retry_after_ms, job.id, job.attempts as i64, job.started_at_ms],
    )
    .context("mark_deferred")?;
    Ok(())
}

/// Requeue any running row whose lease expired. Returns the count recovered.
pub fn recover_stale_locks(store: &BucketSealStore) -> Result<usize> {
    let conn = store.lock_conn()?;
    let n = conn.execute(
        "UPDATE mem_tree_jobs
            SET status='ready', locked_until_ms=NULL
          WHERE status='running' AND locked_until_ms IS NOT NULL AND locked_until_ms <= ?1",
        rusqlite::params![now_ms()],
    )
    .context("recover_stale_locks")?;
    Ok(n)
}

pub fn get_job(store: &BucketSealStore, id: &str) -> Result<Option<Job>> {
    let conn = store.lock_conn()?;
    let sql = format!("SELECT {JOB_COLS} FROM mem_tree_jobs WHERE id = ?1");
    conn.query_row(&sql, rusqlite::params![id], row_to_job)
        .optional()
        .context("get_job")
}

pub fn count_by_status(store: &BucketSealStore) -> Result<Vec<(JobStatus, u64)>> {
    let conn = store.lock_conn()?;
    let mut stmt = conn.prepare("SELECT status, COUNT(*) FROM mem_tree_jobs GROUP BY status")?;
    let rows = stmt.query_map([], |r| {
        let s: String = r.get(0)?;
        let n: i64 = r.get(1)?;
        Ok((s, n.max(0) as u64))
    })?;
    let mut out = Vec::new();
    for r in rows {
        let (s, n) = r?;
        if let Ok(status) = JobStatus::parse(&s) {
            out.push((status, n));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::jobs::types::{NewJob, SealPayload};
    use crate::memory_bucket_seal::store::BucketSealStore;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }
    fn seal_job(tree: &str) -> NewJob {
        NewJob::seal(&SealPayload { tree_id: tree.into(), from_level: 0, force: false }).unwrap()
    }

    #[test]
    fn enqueue_then_claim_then_done() {
        let (store, _d) = fresh();
        let id = enqueue(&store, &seal_job("t1")).unwrap().expect("enqueued");
        let job = claim_next(&store, 60_000).unwrap().expect("claimed");
        assert_eq!(job.id, id);
        assert_eq!(job.status, JobStatus::Running);
        assert_eq!(job.attempts, 1);
        mark_done(&store, &job).unwrap();
        assert!(claim_next(&store, 60_000).unwrap().is_none(), "no ready jobs after done");
    }

    #[test]
    fn enqueue_dedupes_active() {
        let (store, _d) = fresh();
        assert!(enqueue(&store, &seal_job("t1")).unwrap().is_some());
        assert!(enqueue(&store, &seal_job("t1")).unwrap().is_none(), "second active enqueue deduped");
    }

    #[test]
    fn claim_respects_available_at() {
        let (store, _d) = fresh();
        // Enqueue then bump available_at far into the future → not claimable now.
        let id = enqueue(&store, &seal_job("t1")).unwrap().unwrap();
        {
            let conn = store.lock_conn().unwrap();
            conn.execute("UPDATE mem_tree_jobs SET available_at_ms = ?1 WHERE id = ?2",
                rusqlite::params![i64::MAX, id]).unwrap();
        }
        assert!(claim_next(&store, 60_000).unwrap().is_none());
    }

    #[test]
    fn mark_failed_backs_off_then_fails() {
        let (store, _d) = fresh();
        // Force max_attempts low by inserting directly.
        enqueue(&store, &seal_job("t1")).unwrap();
        // Drive attempts up to max via repeated claim+fail (re-claim after backoff window).
        // Use a tiny lock + reset available_at between iterations to simulate time passing.
        for _ in 0..5 {
            if let Some(job) = claim_next(&store, 60_000).unwrap() {
                mark_failed(&store, &job, "boom").unwrap();
                let conn = store.lock_conn().unwrap();
                conn.execute("UPDATE mem_tree_jobs SET available_at_ms = 0 WHERE id = ?1",
                    rusqlite::params![job.id]).unwrap();
            }
        }
        let conn = store.lock_conn().unwrap();
        let status: String = conn.query_row(
            "SELECT status FROM mem_tree_jobs WHERE dedupe_key='seal:t1'", [], |r| r.get(0)).unwrap();
        assert_eq!(status, "failed", "exhausting max_attempts marks failed");
    }

    #[test]
    fn recover_stale_locks_requeues() {
        let (store, _d) = fresh();
        let id = enqueue(&store, &seal_job("t1")).unwrap().unwrap();
        let _job = claim_next(&store, 60_000).unwrap().unwrap(); // now running, lease 60s
        // Expire the lease.
        {
            let conn = store.lock_conn().unwrap();
            conn.execute("UPDATE mem_tree_jobs SET locked_until_ms = 1 WHERE id = ?1",
                rusqlite::params![id]).unwrap();
        }
        let n = recover_stale_locks(&store).unwrap();
        assert_eq!(n, 1);
        assert!(claim_next(&store, 60_000).unwrap().is_some(), "recovered job is claimable again");
    }

    #[test]
    fn stale_settle_is_noop() {
        let (store, _d) = fresh();
        enqueue(&store, &seal_job("t1")).unwrap();
        let job = claim_next(&store, 60_000).unwrap().unwrap();
        // Simulate a recovery: bump attempts so the held `job` token is stale.
        {
            let conn = store.lock_conn().unwrap();
            conn.execute("UPDATE mem_tree_jobs SET attempts = attempts + 1 WHERE id = ?1",
                rusqlite::params![job.id]).unwrap();
        }
        // The stale worker's mark_done must NOT terminate the row.
        mark_done(&store, &job).unwrap();
        let conn = store.lock_conn().unwrap();
        let status: String = conn.query_row(
            "SELECT status FROM mem_tree_jobs WHERE id = ?1", rusqlite::params![job.id], |r| r.get(0)).unwrap();
        assert_ne!(status, "done", "stale settle gated by claim token");
    }

    #[test]
    fn count_by_status_groups() {
        let (store, _d) = fresh();
        enqueue(&store, &seal_job("t1")).unwrap();
        enqueue(&store, &seal_job("t2")).unwrap();
        let counts = count_by_status(&store).unwrap();
        let ready: u64 = counts.iter().find(|(s, _)| *s == JobStatus::Ready).map(|(_, n)| *n).unwrap_or(0);
        assert_eq!(ready, 2);
    }
}
