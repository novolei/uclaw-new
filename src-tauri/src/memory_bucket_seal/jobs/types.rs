// SPDX-License-Identifier: Apache-2.0
//! Job kinds, status, and payloads for the memory-tree job queue.

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobKind {
    Seal,
    DigestDaily,
    FlushStale,
}

impl JobKind {
    pub fn as_str(self) -> &'static str {
        match self {
            JobKind::Seal => "seal",
            JobKind::DigestDaily => "digest_daily",
            JobKind::FlushStale => "flush_stale",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "seal" => JobKind::Seal,
            "digest_daily" => JobKind::DigestDaily,
            "flush_stale" => JobKind::FlushStale,
            other => return Err(anyhow!("unknown JobKind '{other}'")),
        })
    }
    /// True for kinds that call the LLM summariser/embedder — gated by the
    /// worker's LLM concurrency permit. FlushStale is pure-SQL (only enqueues).
    pub fn is_llm_bound(self) -> bool {
        matches!(self, JobKind::Seal | JobKind::DigestDaily)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum JobStatus {
    Ready,
    Running,
    Done,
    Failed,
    Cancelled,
}

impl JobStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            JobStatus::Ready => "ready",
            JobStatus::Running => "running",
            JobStatus::Done => "done",
            JobStatus::Failed => "failed",
            JobStatus::Cancelled => "cancelled",
        }
    }
    pub fn parse(s: &str) -> Result<Self> {
        Ok(match s {
            "ready" => JobStatus::Ready,
            "running" => JobStatus::Running,
            "done" => JobStatus::Done,
            "failed" => JobStatus::Failed,
            "cancelled" => JobStatus::Cancelled,
            other => return Err(anyhow!("unknown JobStatus '{other}'")),
        })
    }
    pub fn is_terminal(self) -> bool {
        matches!(self, JobStatus::Done | JobStatus::Failed | JobStatus::Cancelled)
    }
}

/// A claimed/persisted job row.
#[derive(Clone, Debug)]
pub struct Job {
    pub id: String,
    pub kind: JobKind,
    pub payload_json: String,
    pub dedupe_key: String,
    pub status: JobStatus,
    pub attempts: u32,
    pub max_attempts: u32,
    pub available_at_ms: i64,
    pub locked_until_ms: Option<i64>,
    pub last_error: Option<String>,
    pub created_at_ms: i64,
    pub started_at_ms: Option<i64>,
    pub completed_at_ms: Option<i64>,
}

/// A job to enqueue. `id` is generated; `max_attempts` defaults if None.
#[derive(Clone, Debug)]
pub struct NewJob {
    pub id: String,
    pub kind: JobKind,
    pub payload_json: String,
    pub dedupe_key: String,
    pub max_attempts: Option<u32>,
}

impl NewJob {
    fn build(kind: JobKind, dedupe_key: String, payload: &impl Serialize) -> Result<Self> {
        Ok(Self {
            id: format!("{}:{}", kind.as_str(), uuid::Uuid::new_v4()),
            kind,
            payload_json: serde_json::to_string(payload)?,
            dedupe_key,
            max_attempts: None,
        })
    }
    pub fn seal(p: &SealPayload) -> Result<Self> {
        Self::build(JobKind::Seal, p.dedupe_key(), p)
    }
    pub fn digest_daily(p: &DigestDailyPayload) -> Result<Self> {
        Self::build(JobKind::DigestDaily, p.dedupe_key(), p)
    }
    pub fn flush_stale(p: &FlushStalePayload) -> Result<Self> {
        Self::build(JobKind::FlushStale, p.dedupe_key(), p)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SealPayload {
    pub tree_id: String,
    pub from_level: u32,
    /// When true, the seal handler passes `force_now=Some(now)` so a
    /// below-threshold stale buffer still seals (used by FlushStale).
    pub force: bool,
}
impl SealPayload {
    pub fn dedupe_key(&self) -> String {
        format!("seal:{}", self.tree_id)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DigestDailyPayload {
    pub date: String, // "YYYY-MM-DD"
}
impl DigestDailyPayload {
    pub fn dedupe_key(&self) -> String {
        format!("digest:{}", self.date)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FlushStalePayload {
    pub date: String, // "YYYY-MM-DD" — buckets flush runs per day
}
impl FlushStalePayload {
    pub fn dedupe_key(&self) -> String {
        format!("flush:{}", self.date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_kind_round_trip() {
        for k in [JobKind::Seal, JobKind::DigestDaily, JobKind::FlushStale] {
            assert_eq!(JobKind::parse(k.as_str()).unwrap(), k);
        }
        assert!(JobKind::parse("bogus").is_err());
    }

    #[test]
    fn job_status_round_trip_and_terminal() {
        for s in [JobStatus::Ready, JobStatus::Running, JobStatus::Done, JobStatus::Failed, JobStatus::Cancelled] {
            assert_eq!(JobStatus::parse(s.as_str()).unwrap(), s);
        }
        assert!(JobStatus::Done.is_terminal());
        assert!(JobStatus::Failed.is_terminal());
        assert!(JobStatus::Cancelled.is_terminal());
        assert!(!JobStatus::Ready.is_terminal());
        assert!(!JobStatus::Running.is_terminal());
    }

    #[test]
    fn is_llm_bound_matches_kinds() {
        assert!(JobKind::Seal.is_llm_bound());
        assert!(JobKind::DigestDaily.is_llm_bound());
        assert!(!JobKind::FlushStale.is_llm_bound());
    }

    #[test]
    fn dedupe_keys_are_stable() {
        assert_eq!(SealPayload { tree_id: "t1".into(), from_level: 0, force: false }.dedupe_key(), "seal:t1");
        assert_eq!(SealPayload { tree_id: "t1".into(), from_level: 2, force: true }.dedupe_key(), "seal:t1");
        assert_eq!(DigestDailyPayload { date: "2026-05-30".into() }.dedupe_key(), "digest:2026-05-30");
        assert_eq!(FlushStalePayload { date: "2026-05-30".into() }.dedupe_key(), "flush:2026-05-30");
    }

    #[test]
    fn new_job_builders_serialise_payload() {
        let nj = NewJob::seal(&SealPayload { tree_id: "t1".into(), from_level: 0, force: false }).unwrap();
        assert_eq!(nj.kind, JobKind::Seal);
        assert_eq!(nj.dedupe_key, "seal:t1");
        assert!(nj.payload_json.contains("t1"));
    }
}
