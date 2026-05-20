//! 摄入 job 的内存态类型。job 不落库(重启即清);已写入 gbrain 的页不丢。

use serde::{Deserialize, Serialize};

pub type JobId = String;

/// 待摄入的源。一个源 = 一个 job。
#[derive(Debug, Clone)]
pub enum IngestionSource {
    File(String),
    Url(String),
}

impl IngestionSource {
    pub fn label(&self) -> String {
        match self {
            IngestionSource::File(p) => p.rsplit('/').next().unwrap_or(p).to_string(),
            IngestionSource::Url(u) => u.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IngestionStatus {
    Queued,
    Parsing,
    Extracting,
    Writing,
    Done,
    Partial,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Progress {
    pub stage: String,
    pub done: u32,
    pub total: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IngestionJob {
    pub id: JobId,
    pub source_label: String,
    pub status: IngestionStatus,
    pub progress: Progress,
    pub pages_written: Vec<String>,
    pub error: Option<String>,
}

impl IngestionJob {
    pub fn new(id: JobId, source_label: String) -> Self {
        Self {
            id,
            source_label,
            status: IngestionStatus::Queued,
            progress: Progress::default(),
            pages_written: Vec::new(),
            error: None,
        }
    }
}

/// 管线各阶段错误。不静默吞——全部记入 job.error。
#[derive(Debug, Clone, thiserror::Error)]
pub enum IngestError {
    #[error("unsupported source: {0}")]
    Unsupported(String),
    #[error("parse failed: {0}")]
    Parse(String),
    #[error("decode failed: {0}")]
    Decode(String),
    #[error("stt failed: {0}")]
    Stt(String),
    #[error("llm failed: {0}")]
    Llm(String),
    #[error("gbrain failed: {0}")]
    Gbrain(String),
    #[error("io failed: {0}")]
    Io(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn job_new_is_queued() {
        let j = IngestionJob::new("abc".into(), "x.pdf".into());
        assert_eq!(j.status, IngestionStatus::Queued);
        assert!(j.pages_written.is_empty());
        assert!(j.error.is_none());
    }

    #[test]
    fn source_label_strips_path() {
        assert_eq!(IngestionSource::File("/a/b/c.pdf".into()).label(), "c.pdf");
        assert_eq!(IngestionSource::Url("https://x.com/p".into()).label(), "https://x.com/p");
    }

    #[test]
    fn status_serializes_snake_case() {
        let s = serde_json::to_string(&IngestionStatus::Partial).unwrap();
        assert_eq!(s, "\"partial\"");
    }
}
