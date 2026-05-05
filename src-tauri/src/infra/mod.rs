//! infra 模块 — 中央消息总线
//!
//! 提供 `InfraService` 作为所有后台服务间通信的中枢，
//! 对标 memUBot 的 InfraService (EventEmitter 模式)。

mod service;
mod types;

pub use service::InfraService;
pub use types::*;
