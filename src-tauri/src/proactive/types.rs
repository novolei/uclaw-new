//! 主动服务类型定义
//!
//! 定义 ProactiveService 使用的所有数据结构和常量。

use serde::{Deserialize, Serialize};

// ─── 主动服务状态 ─────────────────────────────────────────────────────

/// 主动服务的运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProactiveState {
    /// 等待下个轮询周期
    Idle,
    /// 正在执行 agent loop
    Thinking,
    /// 等待用户确认输入（wait_user_confirm 工具触发）
    WaitingUserInput,
    /// 已停止
    Stopped,
}

// ─── 主动消息 ─────────────────────────────────────────────────────────

/// 主动服务生成的消息
///
/// 当 agent 判断需要主动向用户推送信息时，生成此结构并持久化到 storage。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveMessage {
    /// 消息唯一 ID (UUID v4)
    pub id: String,
    /// 消息文本内容
    pub content: String,
    /// 生成时间 (ISO 8601)
    pub generated_at: String,
    /// 触发原因描述（什么触发了这次主动行为）
    pub trigger_reason: String,
    /// 使用的工具列表
    pub tools_used: Vec<String>,
}

// ─── 状态报告 ─────────────────────────────────────────────────────────

/// 主动服务状态报告，用于前端仪表盘展示
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProactiveStatus {
    /// 当前状态
    pub state: ProactiveState,
    /// 是否正在运行
    pub is_running: bool,
    /// 已执行的 tick（轮询）次数
    pub tick_count: u64,
    /// 已执行的主动行动次数（生成了实际消息）
    pub action_count: u64,
    /// 判断为无需行动的次数（返回 NO_MESSAGE）
    pub no_message_count: u64,
    /// 上次 tick 时间 (ISO 8601)
    pub last_tick_at: Option<String>,
    /// 上次实际行动时间 (ISO 8601)
    pub last_action_at: Option<String>,
    /// 当前上下文滑动窗口中的消息数
    pub context_message_count: usize,
}

// ─── 常量 ──────────────────────────────────────────────────────────────

/// "[NO_MESSAGE]" — 主动 agent 判断无需发送消息时返回的标记
pub const NO_MESSAGE_MARKER: &str = "[NO_MESSAGE]";

/// 默认主动服务系统提示
pub const DEFAULT_PROACTIVE_SYSTEM_PROMPT: &str = r#"你是 uClaw 的主动助手，工作在自主模式。
你会定期检查上下文变化并自主决定是否需要采取行动。

主要工作:
1. 检查用户最近的对话，判断是否有需要跟进的事项
2. 检查待办事项列表，提醒或执行到期任务
3. 基于用户记忆，提供个性化建议

[重要] 如果没有任何需要处理的事项，你必须返回 "[NO_MESSAGE]"（不要添加任何其他文字）。
[重要] 执行任何破坏性操作前，必须使用 wait_user_confirm 工具请求用户确认。
[重要] 不要重复提醒用户已经知道的事情。
"#;
