//! memorization 模块 — 后台记忆提取服务
//!
//! 替代当前每轮同步 reflection 的模式，
//! 通过后台队列 + 阈值/防抖触发机制批量进行记忆提取。
//!
//! ## 模块结构
//! - `service`: MemorizationService 核心服务实现
//! - `storage`: SQLite 持久化队列
//! - `types`: 类型定义

mod service;
mod storage;
mod types;
pub mod watcher;

pub use service::MemorizationService;
pub use storage::MemorizationStorage;
pub use types::*;
pub use watcher::{DraftsWatcherHandle, start_drafts_watcher};
