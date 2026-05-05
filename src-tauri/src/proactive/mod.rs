//! proactive 模块 — 24/7 主动代理服务
//!
//! 提供 `ProactiveService` 实现 memUBot 的主动轮询能力，
//! 包括上下文监控、自主 agent loop 执行和用户确认机制。

pub mod execution_log;
pub mod multimodal;
pub mod scenarios;
pub mod skill_parser;
mod service;
mod storage;
mod types;

pub use service::ProactiveService;
pub use storage::ProactiveStorage;
pub use types::*;
