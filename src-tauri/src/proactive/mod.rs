//! proactive 模块 — 24/7 主动代理服务
//!
//! 提供 `ProactiveService` 实现 memUBot 的主动轮询能力，
//! 包括上下文监控、自主 agent loop 执行和用户确认机制。

pub mod code_memory;
pub mod conversation_bridge;
pub mod execution_log;
pub mod failure_memory;
pub mod feedback;
pub mod hybrid_search;
pub mod multimodal;
pub mod personality_model;
pub mod preference_extractor;
pub mod proactive_recall;
pub mod scenarios;
pub mod skill_distillation;
pub mod skill_parser;
pub mod skill_telemetry;
pub mod task_memory;
pub mod tool_memory;
pub mod review_scheduler;
pub mod daily_summary;
mod service;
mod storage;
mod types;

pub use service::{MemoryOsRuntimeConfig, ProactiveService};
pub use storage::ProactiveStorage;
pub use types::*;
