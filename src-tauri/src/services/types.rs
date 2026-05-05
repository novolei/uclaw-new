use serde::{Deserialize, Serialize};

// ─── 服务状态枚举 ──────────────────────────────────────────────────────

/// 表示一个受管服务的运行状态。
/// 使用 `#[serde(tag = "status")]` 在序列化时以 `{ "status": "Running" }` 形式输出，
/// 方便前端直接根据 status 字段做 UI 映射。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "status")]
pub enum ServiceStatus {
    /// 已停止（尚未启动或已被关闭）
    Stopped,
    /// 正在启动中
    Starting,
    /// 正常运行
    Running,
    /// 正在停止中（优雅关闭阶段）
    Stopping,
    /// 启动或运行失败，附带失败原因
    Failed { reason: String },
}

// ─── 服务健康信息 ──────────────────────────────────────────────────────

/// 单个服务的健康快照，包含名称、状态、运行时长和最近错误等。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceHealth {
    /// 服务名称（唯一标识）
    pub name: String,
    /// 当前状态
    pub status: ServiceStatus,
    /// 从启动至今的秒数（未运行时为 None）
    pub uptime_secs: Option<u64>,
    /// 最近一次错误信息（无错误时为 None）
    pub last_error: Option<String>,
    /// 服务自定义指标（由各服务自行填充）
    pub metrics: serde_json::Value,
}

// ─── 服务摘要 ──────────────────────────────────────────────────────────

/// 所有受管服务的汇总信息，用于前端仪表盘展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServicesSummary {
    /// 已注册的服务总数
    pub total: usize,
    /// 当前处于 Running 状态的数量
    pub running: usize,
    /// 当前处于 Stopped 状态的数量
    pub stopped: usize,
    /// 当前处于 Failed 状态的数量
    pub failed: usize,
    /// 每个服务的详细健康信息
    pub services: Vec<ServiceHealth>,
}
