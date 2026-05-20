//! 知识摄入管线(子项目 B):拖放文件 → 后台静默解析 → LLM 抽实体 →
//! 智能合并 put_page 写入 gbrain。job 内存态,事后用户在双星云(C)看/改/回滚。

pub mod job;
pub mod sources;
pub mod chunk;
pub mod extract;
pub mod merge;

pub use job::{IngestError, IngestionJob, IngestionSource, IngestionStatus, JobId, Progress};
