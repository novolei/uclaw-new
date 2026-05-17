use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use crate::infra::ConversationMessage;
use crate::agent::gep::types::{GeneCandidate, LearningCard};

// ─── Per-Session 上下文窗口 ───────────────────────────────────────────

/// Per-session 上下文滑动窗口
///
/// 替代原先全局共享的 VecDeque，每个 session 独立维护自己的消息窗口，
/// 支持按 last_active_at 做 LRU 淘汰和可选的消息摘要压缩。
#[derive(Debug, Clone)]
pub struct SessionContextWindow {
    pub session_id: String,
    pub messages: VecDeque<ConversationMessage>,
    pub last_active_at: Instant,
    pub summary: Option<String>,
}

impl SessionContextWindow {
    pub fn new(session_id: String, capacity: usize) -> Self {
        Self {
            session_id,
            messages: VecDeque::with_capacity(capacity),
            last_active_at: Instant::now(),
            summary: None,
        }
    }

    /// 更新最近活跃时间戳
    pub fn touch(&mut self) {
        self.last_active_at = Instant::now();
    }
}

/// 执行日志（供 SkillExtraction 场景使用）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionLog {
    pub session_id: String,
    pub iteration: usize,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub tool_output: serde_json::Value,
    pub success: bool,
    pub duration_ms: u64,
    pub timestamp: i64,
    pub context_summary: String,
}

/// 多模态输入源类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MultimodalSourceType {
    Image,
    Document,
    Code,
    Audio,
}

/// 多模态输入
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultimodalInput {
    pub source_type: MultimodalSourceType,
    pub content_text: String,
    pub caption: String,
    pub mime_type: String,
    pub filename: Option<String>,
    pub metadata: serde_json::Value,
    pub ingested_at: i64,
}

/// 场景评估上下文
pub struct ScenarioContext {
    pub recent_messages: Vec<ConversationMessage>,
    pub execution_logs: Vec<ExecutionLog>,
    pub pending_multimodal: Vec<MultimodalInput>,
    pub last_trigger_at: HashMap<String, Instant>,
    pub tick_count: u64,
    pub new_message_count: usize,
    pub new_execution_count: usize,
    pub has_failures: bool,
    /// 当前活跃的 Space ID，用于记忆召回时定位正确的空间
    pub active_space_id: String,
    /// 当前活跃的 Session ID（最近有消息的会话），用于会话级记忆召回
    pub active_session_id: Option<String>,
    /// 会话上下文摘要（工具调用、推理链、成本等）
    pub session_context: Option<SessionContext>,
    /// 已有学得技能的紧凑指纹列表（仅 skill_extraction 场景使用）。
    /// 每项格式: "title | description(≤60chars) | category | cited:N"
    /// 在 run_scenario_loop 中预计算并注入，用于提取阶段前置去重。
    /// 空 Vec 表示不需要去重参考（其他场景或技能数不足）。
    pub existing_skill_fingerprints: Vec<String>,
    // ─── GEP Gene Evolution 字段 ───
    /// Gene 候选池当前数量
    pub gene_candidate_count: usize,
    /// Gene 候选列表（LearningCard 快照，供蒸馏场景使用）
    pub gene_candidates: Vec<LearningCard>,
    /// 已有 Gene 指纹列表（用于去重）
    pub existing_gene_fingerprints: Vec<String>,
}

/// 会话上下文摘要 — 从 Agent Session 中提取的结构化信息
#[derive(Debug, Clone)]
pub struct SessionContext {
    /// 最近 N 次工具调用的摘要
    pub tool_calls: Vec<ToolCallSummary>,
    /// 推理链关键步骤摘要
    pub reasoning_steps: Vec<String>,
    /// 当前会话累计 token 消耗
    pub cumulative_tokens: Option<TokenUsage>,
    /// 当前会话的轮次数
    pub turn_count: Option<usize>,
    /// 会话涉及的关键文件
    pub workspace_files: Vec<String>,
}

/// 工具调用摘要
#[derive(Debug, Clone)]
pub struct ToolCallSummary {
    pub tool_name: String,
    pub success: bool,
    pub duration_ms: u64,
    pub summary: String,
}

/// Token 使用统计
#[derive(Debug, Clone)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

/// 场景输出
#[derive(Debug, Clone)]
pub struct ScenarioOutput {
    pub scenario_name: String,
    pub system_prompt: String,
    pub context_messages: Vec<(String, String)>,
    pub memory_types: Vec<String>,
    pub additional_instructions: Option<String>,
}

/// Proactive 场景 trait
#[async_trait]
pub trait ProactiveScenario: Send + Sync {
    /// 场景名称
    fn name(&self) -> &str;

    /// 场景描述
    fn description(&self) -> &str;

    /// 评估是否应该触发此场景
    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool;

    /// 构建场景上下文（系统 prompt + 上下文消息）
    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput>;

    /// 获取场景的系统提示
    fn system_prompt(&self) -> &str;

    /// 获取场景关注的记忆类型
    fn memory_types(&self) -> Vec<String>;
}
