use std::path::Path;

use serde::{Deserialize, Serialize};

/// MEMUBOT 功能配置
/// 控制 uClaw 的 24/7 主动记忆代理能力
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemubotConfig {
    /// 后台记忆提取配置
    pub memorization: MemorizationConfig,
    /// 主动服务配置
    pub proactive: ProactiveConfig,
    /// 本地 API 服务配置
    pub local_api: LocalApiConfig,
    /// 防休眠配置
    pub power: PowerConfig,
    /// 上下文管理配置
    pub context: ContextConfig,
    /// 可观测性配置
    pub observability: ObservabilityConfig,
    /// Proactive 场景配置
    #[serde(default)]
    pub scenarios: ScenariosConfig,
    /// Automation runtime configuration (cost caps + retention).
    #[serde(default)]
    pub automation: AutomationConfig,
    /// Gene evolution configuration (GEP protocol).
    #[serde(default)]
    pub gene_evolution: GeneEvolutionConfig,
    /// Maximum wall-clock seconds the agent loop may run for a single
    /// user message before forcibly terminating. Default 600s (10 min).
    /// Override via settings → Advanced (or edit ~/.uclaw/memubot_config.json).
    #[serde(default = "default_agent_loop_timeout_secs")]
    pub agent_loop_timeout_secs: u64,
}

/// 后台记忆提取配置
/// 控制何时自动从对话中提取并持久化记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemorizationConfig {
    /// 是否启用后台记忆提取
    pub enabled: bool,
    /// 立即触发提取的消息数阈值（积累到此数量立即触发）
    pub message_threshold: usize,
    /// 防抖时间（毫秒），默认 3600000（60 分钟）
    pub time_threshold_ms: u64,
    /// 触发提取所需的最少消息数
    pub min_messages: usize,
}

/// 主动服务配置
/// 控制 memubot 的主动轮询行为（如主动提醒、建议等）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ProactiveConfig {
    /// 是否启用主动服务
    pub enabled: bool,
    /// 轮询间隔（毫秒），默认 30000（30 秒）
    pub interval_ms: u64,
    /// agent loop 单次运行的最大迭代次数
    pub max_iterations: usize,
    /// 自定义系统提示（为空时使用内置默认提示）
    pub system_prompt: Option<String>,
}

/// 本地 API 服务配置
/// 控制 memubot 暴露的本地 HTTP API
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LocalApiConfig {
    /// 是否启用本地 API
    pub enabled: bool,
    /// 监听端口
    pub port: u16,
}

/// 防休眠配置
/// 控制是否阻止系统进入睡眠状态以保持 memubot 持续运行
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PowerConfig {
    /// 是否阻止系统休眠
    pub prevent_sleep: bool,
}

/// 上下文管理配置
/// 控制 memubot 构建提示时的上下文窗口大小和 token 预算分配
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ContextConfig {
    /// 上下文中包含的最大消息数
    pub max_context_messages: usize,
    /// 上下文总 token 上限
    pub max_context_tokens: usize,
    /// L0 层（最近消息）的 token 预算
    pub l0_target_tokens: usize,
    /// L1 层（档案摘要）的 token 预算
    pub l1_target_tokens: usize,
    /// 用户提示的最大 token 数
    pub max_prompt_tokens: usize,
    /// 是否启用会话压缩（长对话自动摘要）
    pub enable_session_compression: bool,
}

/// 可观测性配置
/// 控制 memubot 的指标采集和追踪
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ObservabilityConfig {
    /// 是否启用指标采集
    pub enable_metrics: bool,
    /// 是否启用分布式追踪
    pub enable_tracing: bool,
}

/// Automation runtime configuration — cost guardrails + run-session retention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    /// Hard USD cap for a single run. When cumulative cost crosses this,
    /// the run terminates as ErrorTerminal.
    pub per_run_cost_cap_usd: f64,
    /// Hard USD cap for all automation runs in a calendar day (UTC). When
    /// the day's total is at/over this, new runs do not start.
    pub per_day_cost_cap_usd: f64,
    /// Per-spec, the number of most-recent run-session transcripts to keep.
    /// Older run-sessions are pruned (agent_messages + agent_session row
    /// deleted, automation_activities.session_id set NULL); the ledger row
    /// itself is never deleted.
    pub retention_runs_per_spec: u32,
    /// Max agentic-loop iterations for an automation run.
    pub max_iterations: usize,
}

/// 三种 Proactive 场景的统一配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ScenariosConfig {
    pub conversation_learning: ConversationLearningConfig,
    pub skill_extraction: SkillExtractionConfig,
    pub multimodal_context: MultimodalContextConfig,
}

/// 场景 1: Always-Learning Assistant 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ConversationLearningConfig {
    /// 是否启用对话学习场景
    pub enabled: bool,
    /// 触发阈值：每 N 条新消息后触发一次分析
    pub trigger_message_count: usize,
    /// 最小触发间隔（毫秒）
    pub min_interval_ms: u64,
    /// 关注的记忆类型
    pub memory_types: Vec<String>,
    /// 自定义系统提示（覆盖默认）
    pub system_prompt: Option<String>,
}

/// 场景 2: Self-Improving Agent 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SkillExtractionConfig {
    /// 是否启用技能提取场景
    pub enabled: bool,
    /// 触发阈值：每 N 次工具执行后触发
    pub trigger_execution_count: usize,
    /// 执行失败时是否立即触发
    pub trigger_on_failure: bool,
    /// 最小触发间隔（毫秒）
    pub min_interval_ms: u64,
    /// 关注的记忆类型
    pub memory_types: Vec<String>,
    /// 自定义系统提示
    pub system_prompt: Option<String>,
}

/// 场景 3: Multimodal Context Builder 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MultimodalContextConfig {
    /// 是否启用多模态上下文场景
    pub enabled: bool,
    /// 用于图片描述的 Vision 模型
    pub vision_model: Option<String>,
    /// 支持的输入类型
    pub supported_types: Vec<String>,
    /// 最大预处理内容长度（字符）
    pub max_content_length: usize,
    /// 最小触发间隔（毫秒）
    pub min_interval_ms: u64,
    /// 自定义系统提示
    pub system_prompt: Option<String>,
}

/// Gene 进化配置（GEP Protocol）
/// 控制 Agent 自进化引擎的 Gene 蒸馏、检索、生命周期行为
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneEvolutionConfig {
    /// 是否启用 Gene 进化引擎
    pub enabled: bool,
    /// Gene 蒸馏触发阈值（candidates 池达到多少条时触发蒸馏）
    pub gene_distillation_threshold: usize,
    /// Gene 蒸馏最小冷却时间（秒）
    pub gene_distillation_cooldown_secs: u64,
    /// 最大保留 Gene candidates 数
    pub max_gene_candidates: usize,
    /// 触发退役的连续失败 Capsule 数
    pub gene_retire_consecutive_failures: u32,
    /// 退役检查：无活动天数
    pub gene_retire_inactive_days: u32,
    /// AVOID cues 最大条数（含 Stage 1 变异增补）
    pub gene_max_avoid_cues: usize,
    /// Stage 1 变异冷却时间（秒）
    pub gene_mutation_cooldown_secs: u64,
    /// 触发 AVOID 增补的最小失败 Capsule 数
    pub gene_avoid_augment_min_failures: u32,
    /// 最大注入 Gene 数
    pub max_active_genes: usize,
}

impl Default for GeneEvolutionConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            gene_distillation_threshold: 5,
            gene_distillation_cooldown_secs: 600,
            max_gene_candidates: 20,
            gene_retire_consecutive_failures: 3,
            gene_retire_inactive_days: 180,
            gene_max_avoid_cues: 5,
            gene_mutation_cooldown_secs: 259_200, // 3 天
            gene_avoid_augment_min_failures: 2,
            max_active_genes: 2,
        }
    }
}

fn default_agent_loop_timeout_secs() -> u64 { 600 }

// ─── Default 实现 ────────────────────────────────────────────────────────

impl Default for MemubotConfig {
    fn default() -> Self {
        Self {
            memorization: MemorizationConfig::default(),
            proactive: ProactiveConfig::default(),
            local_api: LocalApiConfig::default(),
            power: PowerConfig::default(),
            context: ContextConfig::default(),
            observability: ObservabilityConfig::default(),
            scenarios: ScenariosConfig::default(),
            automation: AutomationConfig::default(),
            gene_evolution: GeneEvolutionConfig::default(),
            agent_loop_timeout_secs: 600,
        }
    }
}

impl Default for ScenariosConfig {
    fn default() -> Self {
        Self {
            conversation_learning: ConversationLearningConfig::default(),
            skill_extraction: SkillExtractionConfig::default(),
            multimodal_context: MultimodalContextConfig::default(),
        }
    }
}

impl Default for ConversationLearningConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_message_count: 5,
            min_interval_ms: 60_000, // 1 分钟
            memory_types: vec![
                "profile".to_string(),
                "behavior".to_string(),
                "event".to_string(),
                "knowledge".to_string(),
            ],
            system_prompt: None,
        }
    }
}

impl Default for SkillExtractionConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            trigger_execution_count: 10,
            trigger_on_failure: true,
            min_interval_ms: 120_000, // 2 分钟
            memory_types: vec![
                "skill".to_string(),
                "tool".to_string(),
            ],
            system_prompt: None,
        }
    }
}

impl Default for MultimodalContextConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            vision_model: None,
            supported_types: vec![
                "image".to_string(),
                "document".to_string(),
                "code".to_string(),
            ],
            max_content_length: 50_000,
            min_interval_ms: 60_000, // 1 分钟
            system_prompt: None,
        }
    }
}

impl Default for MemorizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            message_threshold: 20,
            time_threshold_ms: 3_600_000, // 60 分钟
            min_messages: 2,
        }
    }
}

impl Default for ProactiveConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            interval_ms: 30_000, // 30 秒
            max_iterations: 50,
            system_prompt: None,
        }
    }
}

impl Default for LocalApiConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            port: 7337,
        }
    }
}

impl Default for PowerConfig {
    fn default() -> Self {
        Self {
            prevent_sleep: false,
        }
    }
}

impl Default for ContextConfig {
    fn default() -> Self {
        Self {
            max_context_messages: 20,
            max_context_tokens: 6000,
            l0_target_tokens: 2000,
            l1_target_tokens: 2000,
            max_prompt_tokens: 1500,
            enable_session_compression: false,
        }
    }
}

impl Default for ObservabilityConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            enable_tracing: false,
        }
    }
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            per_run_cost_cap_usd: 1.00,
            per_day_cost_cap_usd: 10.00,
            retention_runs_per_spec: 50,
            max_iterations: 50,
        }
    }
}

// ─── 加载与保存 ──────────────────────────────────────────────────────────

/// 配置文件名
const CONFIG_FILE_NAME: &str = "memubot_config.json";

impl MemubotConfig {
    /// 从指定数据目录加载配置
    ///
    /// - `data_dir`: 数据目录路径（通常为 `~/.uclaw`）
    /// - 如果配置文件不存在，返回默认配置
    /// - 如果文件存在但部分字段缺失，`#[serde(default)]` 会自动补全默认值
    pub fn load(data_dir: &Path) -> Self {
        let path = data_dir.join(CONFIG_FILE_NAME);
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => serde_json::from_str(&content).unwrap_or_else(|e| {
                    tracing::warn!("memubot 配置文件解析失败，使用默认配置: {e}");
                    Self::default()
                }),
                Err(e) => {
                    tracing::warn!("memubot 配置文件读取失败，使用默认配置: {e}");
                    Self::default()
                }
            }
        } else {
            Self::default()
        }
    }

    /// 将当前配置保存到指定数据目录
    ///
    /// - `data_dir`: 数据目录路径（通常为 `~/.uclaw`）
    /// - 自动创建目录（如不存在）
    pub fn save(&self, data_dir: &Path) -> Result<(), crate::error::Error> {
        let path = data_dir.join(CONFIG_FILE_NAME);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(crate::error::Error::Io)?;
        }
        let content =
            serde_json::to_string_pretty(self).map_err(crate::error::Error::Serde)?;
        std::fs::write(&path, content).map_err(crate::error::Error::Io)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenarios_default_values() {
        let config = ScenariosConfig::default();

        assert!(config.conversation_learning.enabled);
        assert_eq!(config.conversation_learning.trigger_message_count, 5);
        assert_eq!(config.conversation_learning.min_interval_ms, 60_000);
        assert_eq!(config.conversation_learning.memory_types.len(), 4);

        assert!(config.skill_extraction.enabled);
        assert_eq!(config.skill_extraction.trigger_execution_count, 10);
        assert!(config.skill_extraction.trigger_on_failure);
        assert_eq!(config.skill_extraction.min_interval_ms, 120_000);
        assert_eq!(config.skill_extraction.memory_types.len(), 2);

        assert!(config.multimodal_context.enabled);
        assert!(config.multimodal_context.vision_model.is_none());
        assert_eq!(config.multimodal_context.supported_types.len(), 3);
        assert_eq!(config.multimodal_context.max_content_length, 50_000);
    }

    #[test]
    fn test_scenarios_deserialize_empty_json() {
        let json = r#"{}"#;
        let config: ScenariosConfig = serde_json::from_str(json).unwrap();
        assert!(config.conversation_learning.enabled);
        assert!(config.skill_extraction.enabled);
        assert!(config.multimodal_context.enabled);
    }

    #[test]
    fn test_scenarios_roundtrip_serialization() {
        let config = ScenariosConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: ScenariosConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.conversation_learning.trigger_message_count, 5);
        assert_eq!(deserialized.skill_extraction.trigger_execution_count, 10);
        assert_eq!(deserialized.multimodal_context.max_content_length, 50_000);
    }

    #[test]
    fn test_memubot_config_includes_scenarios() {
        let json = r#"{}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.scenarios.conversation_learning.enabled);
    }

    #[test]
    fn automation_config_has_defaults() {
        let c = AutomationConfig::default();
        assert!(c.per_run_cost_cap_usd > 0.0);
        assert!(c.per_day_cost_cap_usd > 0.0);
        assert!(c.retention_runs_per_spec >= 1);
    }

    #[test]
    fn memubot_config_includes_automation_section() {
        let config: MemubotConfig = serde_json::from_str("{}").unwrap();
        assert!(config.automation.per_run_cost_cap_usd > 0.0);
    }
}
