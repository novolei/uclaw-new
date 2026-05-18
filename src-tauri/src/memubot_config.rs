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
    /// Symphony runtime configuration — DAG-of-agent-runs orchestrator.
    /// Mirrors `AutomationConfig` shape with two extra knobs (concurrency
    /// cap, stall timeout) and an explicit per-day cap separate from the
    /// per-run cap. See `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md` §7.
    #[serde(default)]
    pub symphony: SymphonyConfig,
    /// Memory OS feature flags — three-layer architecture (Foundation /
    /// Cognitive / Engines, Phases 1-21). Each phase ships an additive
    /// flag that lets the user gracefully disable a feature without
    /// rolling back schema. See `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md`.
    #[serde(default)]
    pub memory_os: MemoryOsConfig,
    /// Maximum wall-clock seconds the agent loop may run for a single
    /// user message before forcibly terminating. Default 600s (10 min).
    /// Override via settings → Advanced (or edit ~/.uclaw/memubot_config.json).
    #[serde(default = "default_agent_loop_timeout_secs")]
    pub agent_loop_timeout_secs: u64,
    /// Whether Plan-mode auto-suggest is enabled. When false, the keyword
    /// detector and agent tool request_plan_mode_switch are both suppressed.
    /// Default true. Toggle exposed in Settings → Intelligence → Agent.
    #[serde(default = "default_true")]
    pub plan_mode_suggest_enabled: bool,
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
            enabled: true,
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

/// Symphony runtime configuration — guards a DAG-of-agent-runs orchestrator.
///
/// Mirrors `AutomationConfig` (cost caps + retention + max_iterations) and
/// adds three Symphony-specific knobs: cross-workflow concurrency cap,
/// per-workflow concurrency default, and node-level stall timeout.
///
/// Defaults intentionally conservative; can be raised once the feature is
/// stable. See `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md` §7.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct SymphonyConfig {
    /// Whether `SymphonyService` is registered + started by `main.rs` Stage 3.
    pub enabled: bool,
    /// Max concurrent in-flight runs across all workflows (global cap).
    pub max_concurrent_runs: usize,
    /// Default per-workflow concurrency for ready nodes (overridable in WORKFLOW.md).
    pub default_max_concurrent_nodes: usize,
    /// Per-node default cost cap (USD). Per-node override lives on the node.
    pub default_per_node_cost_cap_usd: f64,
    /// Per-run default cost cap (USD). Per-workflow override lives on the workflow.
    pub default_per_run_cost_cap_usd: f64,
    /// Daily cap across all Symphony runs (USD). Hard rejection when crossed.
    pub per_day_cost_cap_usd: f64,
    /// How long without a heartbeat before a node is considered stalled (ms).
    /// Heartbeat ticks come from `LoopDelegate::on_usage` / partial-text events.
    pub stall_timeout_ms: u64,
    /// Default max iterations for the agentic loop inside a single node.
    pub default_max_iterations: usize,
    /// Default max retry backoff cap (ms). Symphony SPEC formula:
    /// `delay = min(10_000 * 2^(attempt-1), max_retry_backoff_ms)`.
    pub max_retry_backoff_ms: u64,
    /// Per-workflow number of recent runs to retain before pruning.
    pub retention_runs_per_workflow: u32,
}

impl Default for SymphonyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_concurrent_runs: 2,
            default_max_concurrent_nodes: 4,
            default_per_node_cost_cap_usd: 1.00,
            default_per_run_cost_cap_usd: 5.00,
            per_day_cost_cap_usd: 25.00,
            stall_timeout_ms: 180_000, // 3 min
            default_max_iterations: 30,
            max_retry_backoff_ms: 300_000, // 5 min — Symphony SPEC default
            retention_runs_per_workflow: 50,
        }
    }
}

fn default_agent_loop_timeout_secs() -> u64 { 600 }
fn default_true() -> bool { true }

/// Memory OS feature flags — three-layer architecture.
///
/// Each phase ships ONE additive flag. Defaults are conservative:
/// Foundation Phase 1 (EntityPage CRUD) is on by default because it's
/// purely additive and read-side; subsequent phases that introduce
/// behavior changes default to on once they're stable, and to off
/// during ramp-up. New flags MUST default to a value that preserves the
/// behavior of an older binary — this is the contract that lets users
/// flip a flag, restart, and recover from a regression without rolling
/// back the binary.
///
/// Spec: `docs/superpowers/specs/2026-05-18-agent-memory-os-design.md` §5.4.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MemoryOsConfig {
    // ─── Foundation Layer (Phase 1-7) ───────────────────────────────
    /// Phase 1: EntityPage CRUD via `memory_entity_page_*` commands.
    /// When `false`, every IPC handler returns a structured error so the
    /// frontend can disable its EntityPage UI without crashing.
    pub entity_page_enabled: bool,
    /// Phase 2: Zero-LLM auto-link post-hook on `create_version` /
    /// `create_entity_page`. When `false`, version writes still happen
    /// normally but no auto_link edges are inserted and no stale-link
    /// reconciliation runs (existing auto_link rows on disk are
    /// untouched). Explicit `create_edge` calls are unaffected.
    pub auto_link_enabled: bool,
    /// Phase 3: AI Wiki view backing (`wiki_artifacts` table population).
    /// When `false`, ProactiveService stops regenerating
    /// `wiki_artifacts(kind='index')` automatically, and the manual
    /// `memory_wiki_regenerate` IPC command returns a structured error.
    /// Existing wiki_artifacts rows on disk are untouched.
    pub wiki_view_enabled: bool,
    /// Phase 4: Zero-LLM structural health checks. When `false`,
    /// ProactiveService stops running `run_health_checks` on tick and
    /// the manual `memory_health_run_now` IPC returns a structured
    /// error. Existing `memory_health_findings` rows on disk are
    /// untouched and the list/dismiss IPC commands keep working so the
    /// user can still triage findings discovered before flag was off.
    pub memory_health_enabled: bool,
    /// Phase 5: LLM-driven semantic lint. When `false`, ProactiveService
    /// stops periodic scans and `memory_lint_run_now` IPC returns a
    /// structured error. Existing `memory_health_findings` rows with
    /// `is_lint=1` stay; the list/dismiss commands work the same as
    /// for Phase 4 findings.
    pub memory_lint_enabled: bool,
    /// Phase 5: Daily token cap for the memory_lint scenario. The
    /// orchestrator sums `cost_records.model LIKE 'memory_lint%'` for
    /// today (UTC) and stops calling the analyzer once consuming the
    /// next candidate would push past this cap.
    pub memory_lint_daily_token_budget: u32,
    /// Phase 6b: Swap Phase 3's `StubSynthesizer` for `RealWikiSynthesizer`
    /// (which actually calls the configured LLM via `MemoryOsLlmClient`).
    /// Default `false` to preserve the Phase 3 behaviour for users who
    /// don't have an LLM provider configured — turn on once a provider
    /// is set up. The flag is checked at `AppState::new` bootstrap so a
    /// restart is needed after flipping it.
    pub wiki_real_synthesizer_enabled: bool,
    /// Phase 6c: Swap Phase 5's `StubAnalyzer` for `RealLintAnalyzer`.
    /// Default `false` for the same reason as Phase 6b (stub keeps
    /// working without provider credentials). The existing
    /// `memory_lint_daily_token_budget` cap applies unchanged — the
    /// real analyzer writes `cost_records.model = 'memory_lint:<actual>'`
    /// which the `LIKE 'memory_lint%'` cost guard already sums.
    pub lint_real_analyzer_enabled: bool,
    /// Phase 6.1: Run the periodic tier_escalator (mention_count →
    /// enrichment_tier). Zero LLM, so default ON. When off, EntityPages
    /// stay at whatever tier they were assigned at creation.
    pub tier_escalator_enabled: bool,
    /// Phase 6.1: Daily cap on tier upgrades. Each upgrade eventually
    /// makes a downstream synthesizer call eligible, so this is the
    /// surface that bounds upgrade-driven LLM cost. Downgrades are
    /// uncapped (they save tokens by demoting irrelevant pages).
    pub tier_escalator_daily_cap: u32,
    // (Future flags for Phase 6.2 will be added here with their
    //  defaults so older configs deserialize cleanly.)
}

impl Default for MemoryOsConfig {
    fn default() -> Self {
        Self {
            entity_page_enabled: true,
            auto_link_enabled: true,
            wiki_view_enabled: true,
            memory_health_enabled: true,
            // Phase 5 default ON. The cost cap is the actual safety
            // mechanism: if the cap is zero the analyzer never runs
            // even with the flag on.
            memory_lint_enabled: true,
            memory_lint_daily_token_budget: 50_000,
            // Phase 6b default OFF. Stub remains the bootstrap state so
            // users without an LLM provider keep getting deterministic
            // overview markdown. Flip to `true` once a provider is set up.
            wiki_real_synthesizer_enabled: false,
            // Phase 6c default OFF for the same reason as 6b. The
            // existing daily_token_budget cap will gate spend once
            // turned on.
            lint_real_analyzer_enabled: false,
            // Phase 6.1 default ON (zero LLM). The daily cap (10) is
            // the actual safety mechanism that bounds downstream
            // synthesis cost when Phase 6.2 lands.
            tier_escalator_enabled: true,
            tier_escalator_daily_cap: 10,
        }
    }
}

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
            symphony: SymphonyConfig::default(),
            memory_os: MemoryOsConfig::default(),
            agent_loop_timeout_secs: 600,
            plan_mode_suggest_enabled: true,
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

    #[test]
    fn symphony_config_has_defaults() {
        let c = SymphonyConfig::default();
        assert!(c.enabled);
        assert!(c.max_concurrent_runs >= 1);
        assert!(c.default_max_concurrent_nodes >= 1);
        assert!(c.default_per_node_cost_cap_usd > 0.0);
        assert!(c.default_per_run_cost_cap_usd > 0.0);
        assert!(c.per_day_cost_cap_usd >= c.default_per_run_cost_cap_usd);
        assert!(c.stall_timeout_ms > 45_000);
        assert_eq!(c.max_retry_backoff_ms, 300_000);
        assert!(c.default_max_iterations >= 5);
        assert!(c.retention_runs_per_workflow >= 1);
    }

    #[test]
    fn memubot_config_includes_symphony_section() {
        let config: MemubotConfig = serde_json::from_str("{}").unwrap();
        assert!(config.symphony.enabled);
        assert!(config.symphony.per_day_cost_cap_usd > 0.0);
    }

    #[test]
    fn plan_mode_suggest_enabled_defaults_true() {
        let config: MemubotConfig = serde_json::from_str("{}").unwrap();
        assert!(config.plan_mode_suggest_enabled);
    }

    #[test]
    fn plan_mode_suggest_enabled_can_be_set_false() {
        let json = r#"{"plan_mode_suggest_enabled": false}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.plan_mode_suggest_enabled);
    }

    #[test]
    fn symphony_config_roundtrip_serialization() {
        let original = SymphonyConfig::default();
        let json = serde_json::to_string(&original).unwrap();
        let restored: SymphonyConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.enabled, original.enabled);
        assert_eq!(restored.max_concurrent_runs, original.max_concurrent_runs);
        assert_eq!(restored.per_day_cost_cap_usd, original.per_day_cost_cap_usd);
        assert_eq!(restored.stall_timeout_ms, original.stall_timeout_ms);
        assert_eq!(restored.max_retry_backoff_ms, original.max_retry_backoff_ms);
    }

    // ─── Memory OS Foundation Phase 1 ─────────────────────────────────

    #[test]
    fn memory_os_config_default_has_entity_page_enabled() {
        let c = MemoryOsConfig::default();
        assert!(c.entity_page_enabled, "Phase 1 default should be on");
    }

    #[test]
    fn memory_os_config_default_has_auto_link_enabled() {
        let c = MemoryOsConfig::default();
        assert!(c.auto_link_enabled, "Phase 2 default should be on");
    }

    #[test]
    fn memory_os_config_default_has_wiki_view_enabled() {
        let c = MemoryOsConfig::default();
        assert!(c.wiki_view_enabled, "Phase 3 default should be on");
    }

    #[test]
    fn memory_os_phase3_explicit_disable_preserved() {
        let json = r#"{"memory_os":{"wiki_view_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.wiki_view_enabled);
        // Forward-compat: disabling Phase 3 must not flip Phase 1/2 off.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.auto_link_enabled);
    }

    #[test]
    fn memory_os_config_phase2_round_trip_off() {
        let json = r#"{"memory_os":{"auto_link_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        // Phase 2 off…
        assert!(!config.memory_os.auto_link_enabled);
        // …but Phase 1 default still applies (forward-compat: a config
        // file that mentions only Phase 2 doesn't silently disable
        // Phase 1).
        assert!(config.memory_os.entity_page_enabled);
        // Round-trip preserves both.
        let re = serde_json::to_string(&config).unwrap();
        let restored: MemubotConfig = serde_json::from_str(&re).unwrap();
        assert!(!restored.memory_os.auto_link_enabled);
        assert!(restored.memory_os.entity_page_enabled);
    }

    #[test]
    fn memubot_config_includes_memory_os_section() {
        let config: MemubotConfig = serde_json::from_str("{}").unwrap();
        assert!(config.memory_os.entity_page_enabled);
    }

    #[test]
    fn memory_os_config_respects_explicit_disable() {
        // Forward-compat: a config file written today with the flag off
        // must round-trip back to off, not silently re-enable.
        let json = r#"{"memory_os":{"entity_page_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.entity_page_enabled);
        let re_serialized = serde_json::to_string(&config).unwrap();
        let restored: MemubotConfig = serde_json::from_str(&re_serialized).unwrap();
        assert!(!restored.memory_os.entity_page_enabled);
    }

    #[test]
    fn memory_os_config_partial_json_keeps_defaults() {
        // A config file from an older binary that doesn't know `memory_os`
        // should still deserialize and supply defaults.
        let json = r#"{"agentLoopTimeoutSecs": 900}"#;
        // Note: top-level fields use serde defaults (not camelCase rename),
        // so the snake_case form works too. Just verifying the section
        // defaults populate when missing entirely.
        let config: MemubotConfig =
            serde_json::from_str(r#"{"agent_loop_timeout_secs": 900}"#).unwrap();
        assert!(config.memory_os.entity_page_enabled);
        let _ = json;
    }

    #[test]
    fn memory_os_config_default_has_memory_health_enabled() {
        let c = MemoryOsConfig::default();
        assert!(c.memory_health_enabled, "Phase 4 default should be on");
    }

    #[test]
    fn memory_os_phase4_explicit_disable_preserved() {
        let json = r#"{"memory_os":{"memory_health_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.memory_health_enabled);
        // Forward-compat: disabling Phase 4 must not flip Phase 1/2/3 off.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.auto_link_enabled);
        assert!(config.memory_os.wiki_view_enabled);
    }

    #[test]
    fn memory_os_phase5_defaults_are_sensible() {
        let c = MemoryOsConfig::default();
        assert!(c.memory_lint_enabled, "Phase 5 default should be on");
        assert!(c.memory_lint_daily_token_budget > 0, "budget must be > 0");
        assert!(
            c.memory_lint_daily_token_budget <= 200_000,
            "budget should be capped at a reasonable value"
        );
    }

    #[test]
    fn memory_os_phase5_explicit_disable_preserved() {
        let json = r#"{"memory_os":{"memory_lint_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.memory_lint_enabled);
        // Forward-compat: disabling Phase 5 must not flip earlier phases off.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.auto_link_enabled);
        assert!(config.memory_os.wiki_view_enabled);
        assert!(config.memory_os.memory_health_enabled);
        // Default budget still applies when only the flag is mentioned.
        assert_eq!(config.memory_os.memory_lint_daily_token_budget, 50_000);
    }

    #[test]
    fn memory_os_phase6b_default_keeps_stub_synthesizer() {
        // Real synth is opt-in — stub stays the default so first-boot
        // users with no provider see deterministic markdown, not a
        // structured error.
        let c = MemoryOsConfig::default();
        assert!(
            !c.wiki_real_synthesizer_enabled,
            "Phase 6b default must be OFF (stub remains baseline behaviour)"
        );
    }

    #[test]
    fn memory_os_phase6b_explicit_enable_preserved() {
        let json = r#"{"memory_os":{"wiki_real_synthesizer_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.wiki_real_synthesizer_enabled);
        // Forward-compat: opting into Phase 6b must not change Phase 1-5 defaults.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.auto_link_enabled);
        assert!(config.memory_os.wiki_view_enabled);
        assert!(config.memory_os.memory_health_enabled);
        assert!(config.memory_os.memory_lint_enabled);
    }

    #[test]
    fn memory_os_pre_phase6b_config_still_deserializes() {
        // A config file written before Phase 6b shipped won't have
        // `wiki_real_synthesizer_enabled` at all — `#[serde(default)]`
        // must let it round-trip without rejection.
        let json = r#"{"memory_os":{
            "entity_page_enabled":true,
            "auto_link_enabled":true,
            "wiki_view_enabled":true,
            "memory_health_enabled":true,
            "memory_lint_enabled":true,
            "memory_lint_daily_token_budget":50000
        }}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(
            !config.memory_os.wiki_real_synthesizer_enabled,
            "missing flag must default to OFF"
        );
    }

    #[test]
    fn memory_os_phase6c_default_keeps_stub_analyzer() {
        let c = MemoryOsConfig::default();
        assert!(
            !c.lint_real_analyzer_enabled,
            "Phase 6c default must be OFF (stub stays baseline)"
        );
    }

    #[test]
    fn memory_os_phase6c_explicit_enable_preserved() {
        let json = r#"{"memory_os":{"lint_real_analyzer_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.lint_real_analyzer_enabled);
        // Forward-compat: opting into Phase 6c doesn't change earlier flags
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.memory_lint_enabled);
        assert!(!config.memory_os.wiki_real_synthesizer_enabled,
                "6c alone must NOT flip 6b on");
        assert_eq!(config.memory_os.memory_lint_daily_token_budget, 50_000);
    }

    #[test]
    fn memory_os_phase6_both_flags_independent() {
        // Real users will likely flip both 6b and 6c together once a
        // provider is set up. Confirm the JSON shape supports that
        // without surprising interactions.
        let json = r#"{"memory_os":{
            "wiki_real_synthesizer_enabled":true,
            "lint_real_analyzer_enabled":true
        }}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.wiki_real_synthesizer_enabled);
        assert!(config.memory_os.lint_real_analyzer_enabled);
    }

    #[test]
    fn memory_os_phase61_defaults_are_sensible() {
        let c = MemoryOsConfig::default();
        assert!(
            c.tier_escalator_enabled,
            "tier_escalator is zero-LLM — default should be ON"
        );
        assert!(c.tier_escalator_daily_cap > 0);
        assert!(
            c.tier_escalator_daily_cap <= 100,
            "cap should keep upgrade-driven LLM spend bounded; got {}",
            c.tier_escalator_daily_cap
        );
    }

    #[test]
    fn memory_os_phase61_explicit_disable_preserved() {
        let json = r#"{"memory_os":{"tier_escalator_enabled":false}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.tier_escalator_enabled);
        // The cap default still applies — disabling doesn't zero it out.
        assert_eq!(config.memory_os.tier_escalator_daily_cap, 10);
        // Forward-compat: turning off 6.1 must not flip earlier phases off.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.memory_health_enabled);
        assert!(config.memory_os.memory_lint_enabled);
    }

    #[test]
    fn memory_os_phase61_explicit_cap_override_preserved() {
        let json = r#"{"memory_os":{"tier_escalator_daily_cap":3}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.memory_os.tier_escalator_daily_cap, 3);
        assert!(config.memory_os.tier_escalator_enabled, "flag default holds");
    }
}
