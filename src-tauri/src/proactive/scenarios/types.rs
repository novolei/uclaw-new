use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

use crate::infra::ConversationMessage;

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
