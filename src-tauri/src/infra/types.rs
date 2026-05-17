//! InfraService 事件类型定义
//!
//! 定义消息总线上流转的所有事件类型和数据结构。

use serde::{Deserialize, Serialize};

// ─── 事件类型枚举 ─────────────────────────────────────────────────────

/// 消息总线支持的事件类型
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum InfraEventType {
    /// 用户消息到达（来自前端或 API）
    MessageIncoming,
    /// Bot 响应发出
    MessageOutgoing,
    /// 消息处理完成（一轮对话结束）
    MessageProcessed,
    // ─── 执行日志相关 ───
    /// Agent 执行了一个工具调用
    ToolExecuted,
    /// Agent loop 完成一轮
    LoopCompleted,
    /// Agent loop 执行失败
    LoopFailed,
    // ─── 多模态相关 ───
    /// 新的多模态输入被摄入
    MultimodalIngested,
    // ─── 记忆相关 ───
    /// 记忆提取完成
    MemoryExtracted,
    /// 新技能被学习到
    SkillLearned,
    // ─── 工作区相关 ───
    /// 活跃工作区切换事件
    WorkspaceSwitched,
    // ─── GEP 相关 ───
    /// Gene 应用后生成的 Capsule
    CapsuleCreated,
    // ─── 用户反馈相关 ───
    /// 用户否定/纠正反馈（拒绝计划、停止执行、纠正输出等）
    UserCorrection,
}

// ─── 对话消息 ─────────────────────────────────────────────────────────

/// 对话消息体，携带角色和文本内容
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConversationMessage {
    /// 消息角色: "user" / "assistant"
    pub role: String,
    /// 消息文本内容
    pub content: String,
}

// ─── 基础事件 ─────────────────────────────────────────────────────────

/// 消息总线上传输的基础事件
///
/// 每个事件携带唯一 ID、类型、平台来源、时间戳、消息体和可选元数据。
/// `Clone` 是 `tokio::sync::broadcast` 的硬性要求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InfraEvent {
    /// 自动递增的事件 ID（由 InfraService 分配）
    pub id: u64,
    /// 事件类型
    pub event_type: InfraEventType,
    /// 来源平台标识: "local"（桌面端）/ "api"（HTTP API）
    pub platform: String,
    /// Unix 时间戳（毫秒）
    pub timestamp: i64,
    /// 对话消息体
    pub message: ConversationMessage,
    /// 额外元数据（conversation_id, space_id, message_id 等）
    pub metadata: serde_json::Value,
    /// 可选的链路追踪 ID，用于关联同一轮对话的多个事件
    pub trace_id: Option<String>,
}
