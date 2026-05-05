//! 记忆提取服务的类型定义
//!
//! 包含未记忆消息、待处理任务、服务状态等核心数据结构。

use serde::{Deserialize, Serialize};

/// 未记忆的消息
///
/// 从 InfraService 接收到的消息事件转换而来，
/// 暂存在 SQLite 队列中等待批量提取。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnmemorizedMessage {
    /// 数据库自增 ID
    pub id: i64,
    /// 来源平台: "local" / "api"
    pub platform: String,
    /// 消息角色: "user" / "assistant"
    pub role: String,
    /// 消息文本内容
    pub content: String,
    /// 所属对话 ID（可选）
    pub conversation_id: Option<String>,
    /// 所属空间 ID（可选）
    pub space_id: Option<String>,
    /// Unix 时间戳（毫秒）
    pub timestamp: i64,
}

/// 待处理任务状态
///
/// 当触发记忆提取时，会先记录任务状态到 SQLite，
/// 以便在进程意外退出后恢复未完成的提取任务。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingTask {
    /// 任务唯一标识（UUID）
    pub task_id: String,
    /// 本次任务包含的消息数量
    pub message_count: usize,
    /// 任务开始时间（Unix 时间戳毫秒）
    pub started_at: i64,
}

/// 记忆提取状态机
///
/// 表示 MemorizationService 当前所处的阶段。
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MemorizationState {
    /// 累积消息中，等待触发阈值
    Listening,
    /// 正在调用 memU 进行语义提取
    Memorizing,
    /// 正在恢复上次未完成的任务
    Recovering,
    /// 服务已停止
    Stopped,
}

/// 记忆提取服务状态报告
///
/// 供前端 UI 展示和健康检查使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemorizationStatus {
    /// 当前状态机阶段
    pub state: MemorizationState,
    /// 队列中待处理的消息数
    pub queue_count: usize,
    /// 当前进行中的任务（如有）
    pub pending_task: Option<PendingTask>,
    /// 历史总记忆提取次数
    pub total_memorized: u64,
    /// 上次记忆提取完成时间（RFC3339 格式）
    pub last_memorization_at: Option<String>,
}
