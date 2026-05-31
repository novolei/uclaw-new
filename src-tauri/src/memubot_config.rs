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
    /// Embedding endpoint configuration (gbrain pointer + memU FastEmbed
    /// model). New in Sprint 2.2 followon #4.
    #[serde(default)]
    pub embedding_endpoint: EmbeddingEndpointConfig,
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
    /// Bundle 27-B — max idle time (seconds) between two streaming chunks
    /// before the LLM stream is treated as silently-dropped and re-driven
    /// through the existing `RetryBudget`. Targets the "Kimi K silent
    /// drop" failure mode: national LLM provider load balancers
    /// sometimes close long-lived HTTP streams without a TCP FIN,
    /// leaving the consumer hung in `stream.next().await` forever. 90s
    /// is generous enough that legitimate slow responses (large prompt,
    /// deep reasoning) aren't false-positives, but short enough that
    /// the user sees a recovery attempt rather than a permanent hang.
    /// Lower this (e.g. 30-60s) for providers with frequent silent
    /// drops; raise it for slow/long-reasoning models.
    /// Toggle exposed in Settings → System → Stream & Skill thresholds.
    #[serde(default = "default_stream_idle_timeout_secs")]
    pub stream_idle_timeout_secs: u64,
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

/// Embedding endpoint configuration (Sprint 2.2 followon #4)
///
/// Three gbrain config keys + one memU env var, surfaced as a single
/// settings page section so the user doesn't have to coordinate them
/// manually.
///
/// Default points gbrain at uClaw's own `/v1/embeddings` route
/// (`POST http://localhost:<local_api.port>/v1/embeddings` — backed by
/// memU's FastEmbed bridge, ~100ms warm-path per chunk, no external
/// API key required). Users can override to point at OpenAI / Voyage /
/// llama-server / ollama / any openai-compatible endpoint.
///
/// `fastembed_model` drives the actual FastEmbed model memU loads
/// inside its Python bridge (read at bridge spawn time via
/// `FASTEMBED_MODEL` env). Changing this requires a memU bridge
/// restart, which `set_embedding_config` handles transparently.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EmbeddingEndpointConfig {
    /// gbrain's `base_urls.llama-server` value. Default
    /// `http://localhost:7337/v1` (uClaw LocalApiService; pairs with
    /// `local_api.port = 7337`).
    pub base_url: String,
    /// gbrain's `embedding_model` value, in the `<recipe>:<model>` shape
    /// gbrain expects. Default `llama-server:bge-small-en-v1.5`.
    pub model: String,
    /// gbrain's `embedding_dimensions` value. Default `384` (bge-small).
    pub dimensions: u32,
    /// FastEmbed model id loaded by the memU bridge (via
    /// `FASTEMBED_MODEL` env). Default `BAAI/bge-small-en-v1.5`.
    /// Changing this triggers a memU bridge restart so the new model
    /// is loaded on the next embed call.
    pub fastembed_model: String,
    /// HTTP timeout (seconds) for calls to the embedding endpoint.
    /// Default 8s — generous enough that warm-path calls (≈100ms) never
    /// time out, short enough that a hung endpoint doesn't stall a turn
    /// forever. Raise for slow/remote providers; lower for local-only setups
    /// where a hung call is always a bug. Requires a restart to take effect.
    /// Toggle exposed in Settings → Memory → Embedding endpoint.
    #[serde(default = "default_embed_timeout_secs")]
    pub embed_timeout_secs: u64,
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
    /// Bundle 17-B — drift threshold (sum of added + removed + changed
    /// across all 8 StructuredFold axes) below which `/compact` renders
    /// the new fold as a `<context_changes_since_last_fold>` delta block
    /// stacked on top of the byte-stable prior fold, instead of emitting
    /// a fresh full re-render. Smaller placeholder → next-turn prompt
    /// cache breakpoint sits on a stable prefix → cached_input_tokens
    /// kicks in more on subsequent turns.
    ///
    /// Default 50 — loose default, favors delta path while telemetry stabilizes.
    /// Spec
    /// [`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md`](../../docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md) §6.1
    /// commits to retuning from telemetry within 2 weeks of merge.
    /// Settings-editable via `set_fold_delta_threshold` Tauri command,
    /// also surfaced as the FoldDeltaThresholdSection on the System
    /// settings tab; clamped to `[1, 50]` on write.
    #[serde(default = "default_fold_delta_threshold")]
    pub fold_delta_threshold: u32,
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

fn default_agent_loop_timeout_secs() -> u64 {
    600
}
fn default_true() -> bool {
    true
}
/// Bundle 27-B — 90s default. See doc on
/// `MemubotConfig::stream_idle_timeout_secs` for rationale.
fn default_stream_idle_timeout_secs() -> u64 {
    90
}
/// Bundle 26-B — 30 days matches `review_scheduler.rs` cold-storage
/// window. See doc on `MemoryOsConfig::skill_prune_min_unused_days`.
fn default_skill_prune_min_unused_days() -> u32 {
    30
}
/// Bundle 26-D — 3 returns matches the geneticist's gene-candidate→gene
/// threshold. See doc on `MemoryOsConfig::skill_promote_min_returned_count`.
fn default_skill_promote_min_returned_count() -> u32 {
    3
}
/// PR16 — 8s matches the PR15 hot-path constant for the embedder HTTP timeout.
/// See `EmbeddingEndpointConfig::embed_timeout_secs`.
fn default_embed_timeout_secs() -> u64 {
    8
}
/// PR16 — 5000 matches the PR15 hot-path constant for `recall_semantic` scan cap.
/// See `MemoryOsConfig::recall_semantic_max_scan`.
fn default_recall_semantic_max_scan() -> usize {
    5000
}
/// item2 — project-check advisory is opt-in; default OFF preserves existing
/// edit behaviour for all users who have not explicitly enabled the feature.
/// See `MemoryOsConfig::edit_project_check_enabled`.
fn default_edit_project_check_enabled() -> bool {
    false
}
/// item2 — 5 s is generous enough for a fast incremental check (cargo check
/// with a warm cache, ruff) while staying well under any interactive
/// response-time budget. See `MemoryOsConfig::edit_project_check_timeout_secs`.
fn default_edit_project_check_timeout_secs() -> u64 {
    5
}
/// item3.3b — 100_000 matches the `MAX_READ_CHARS` baseline constant in
/// `agent/tools/builtin/file.rs`. See `MemoryOsConfig::read_file_max_chars`.
fn default_read_file_max_chars() -> usize {
    100_000
}

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
    /// Phase 6.2: Swap the EntitySynthesizer from Stub (deterministic
    /// placeholder) to Real (LLM via MemoryOsLlmClient). Default OFF
    /// for the same reason as Phase 6b/6c — opt-in once a provider
    /// is configured. The manual `memory_entity_page_synthesize_now`
    /// IPC still works with the Stub, just produces placeholder text.
    pub entity_synthesizer_enabled: bool,
    /// Phase 7.4: Opt-in fs watcher over
    /// `~/Documents/workground/brain/`. When ON, edits to `.md` files
    /// under the brain dir auto-trigger `sync_from_disk` after a
    /// 500ms debounce. Default OFF because fs events are noisy on
    /// macOS and `Sync` button (Phase 7.2) covers most users.
    pub brain_watcher_enabled: bool,
    /// Sprint 1 (post-Phase-7): openhuman-style stability_detector +
    /// PROFILE.md system-prompt injection. When ON, ProactiveService
    /// rebuilds the FacetCache every 30 min and injects active facets
    /// into the agent system prompt. Default ON (zero cost when there
    /// are no candidates) — flip OFF to A/B test prompts.
    pub learning_enabled: bool,
    /// Sprint 2.1b — daily token budget for the per-turn LLM extractor
    /// inside `ChatDelegate::before_llm_call`. The dispatcher sums
    /// `cost_records.model LIKE 'memory_learning%'` for today (UTC) and
    /// skips the LLM layer when spend exceeds this. The regex layer is
    /// free and always runs regardless. Defaults to 30_000 tokens/day
    /// (≈$0.05 with Haiku) — a comfortable ceiling for "openhuman
    /// warm-start" budgets where the LLM is a fallback to regex.
    pub learning_llm_daily_token_budget: u32,
    /// Sprint 2.4b — gbrain chat-turn auto-extractor. When ON,
    /// `ChatDelegate::before_llm_call` at iteration=0 spawns
    /// `crate::gbrain::chat_extractor::extract_from_chat_turn` on the
    /// user's latest message. Proposals with `confidence >= 0.7` are
    /// fired as `mcp__gbrain__put_page` calls. Default ON because
    /// Sprint 2.3 (PR #223) post-merge QA validated the agent does call
    /// gbrain on explicit instructions — the extractor is the safety net
    /// for cases where the agent missed an obvious entity, gated by the
    /// daily token budget below.
    pub gbrain_extractor_enabled: bool,
    /// Sprint 2.4b — daily token budget for the gbrain chat-turn
    /// extractor. Mirrors `learning_llm_daily_token_budget` shape; sum
    /// is over `cost_records.model LIKE 'gbrain_extract%'`. Defaults to
    /// 30_000 tokens/day (≈$0.05 with Haiku), parallel to the learning
    /// extractor's budget — the two producers can each spend this much
    /// per day without crossing into "noticeable" spend territory.
    pub gbrain_extractor_daily_token_budget: u32,
    /// L3 §4.12.1 RETAINED — gates the periodic Importance-Aware Decay
    /// batch in ProactiveService. When ON, every 360 ticks (~3h) the
    /// loop picks up to `importance_decay_daily_batch_size` nodes
    /// from `memory_nodes` (filtered to `DEFAULT_BATCH_KINDS`) and
    /// refreshes their `memory_importance_scores` row. Zero LLM cost.
    /// Flip OFF to A/B test the decay path or disable on memory-tight
    /// machines; flipping doesn't lose data (existing score rows stay).
    pub importance_decay_enabled: bool,
    /// L3 §4.12.1 RETAINED — bound on per-batch work. Default 100
    /// keeps the per-tick work negligible (a 100-node batch on a
    /// modern SSD is single-digit milliseconds). Raise for big
    /// knowledge bases; set to 0 to disable the loop without flipping
    /// the bool (the loop short-circuits on limit=0).
    pub importance_decay_batch_size: u32,
    /// L3 §4.12.4 R1 — gates the periodic Concept Drift Detection
    /// scan. When ON, every 480 ticks (~4h @ 30s) the scenario scans
    /// EntityPages with multiple versions, computes content drift, and
    /// records a `drift_events` row when drift crosses threshold.
    /// Zero LLM cost. Default ON.
    pub drift_detection_enabled: bool,
    /// L3 §4.12.4 R1 — per-scan cap on candidate EntityPages.
    /// Default 50. Set to 0 to disable without flipping the bool.
    pub drift_detection_batch_size: u32,
    /// Bundle 26-B — minimum idle days before an auto-extracted skill
    /// is considered "stale" and archived. The prune pass (runs every
    /// ~2h via `ProactiveService::tick_inner`) walks
    /// `~/.uclaw/skills/_auto_extracted/` and archives directories
    /// where `last_used_at` is older than this AND `returned_count`
    /// ≤ 1. Default 30 — matches the cold-storage window already
    /// established by `review_scheduler.rs`. Lower the value to
    /// archive aggressively on small/noisy libraries; raise it to
    /// keep skills around longer. Nothing is destroyed — archived
    /// skills move to `_auto_extracted/_archive/<YYYYMMDD-HHMM>/`
    /// and are reversible via `mv`.
    /// Toggle exposed in Settings → System → Stream & Skill thresholds.
    #[serde(default = "default_skill_prune_min_unused_days")]
    pub skill_prune_min_unused_days: u32,
    /// Bundle 26-D — minimum `returned_count` (number of times a
    /// skill has been returned by `skill_search`) before the skill
    /// becomes eligible for promotion into the GEP
    /// `gene_candidate_pool` as a `source: "skill_promotion"`
    /// candidate. The promotion pass runs every ~30 min via
    /// `ProactiveService::tick_inner`. Default 3 — aligns with the
    /// geneticist scenario's existing gene-candidate→gene threshold
    /// so there's no awkward "skill hot but not gene-worthy" mid-band.
    /// Each skill promotes at most once (stamped via
    /// `meta.json::promoted_at`). Lower to promote earlier; raise to
    /// require more empirical evidence.
    /// Toggle exposed in Settings → System → Stream & Skill thresholds.
    #[serde(default = "default_skill_promote_min_returned_count")]
    pub skill_promote_min_returned_count: u32,
    /// PR16 — per-turn scan cap for `recall_semantic`. The semantic recall
    /// path iterates over all summary nodes that carry an embedding; on a
    /// very large memory store this could be tens of thousands of rows.
    /// The cap short-circuits iteration and logs a warning when hit, so
    /// latency stays bounded even on a pathologically large store.
    /// Default 5000 — the PR15 hot-path constant. Raise for very large
    /// stores where recall quality matters more than worst-case latency;
    /// lower for memory-constrained or latency-sensitive deployments.
    /// Toggle exposed in Settings → Memory → Recall.
    #[serde(default = "default_recall_semantic_max_scan")]
    pub recall_semantic_max_scan: usize,
    /// When true, after an edit to a code file the agent runs the project's
    /// check command (cargo/ruff/etc.) time-boxed + best-effort and attaches
    /// any new diagnostics as an advisory. Default off.
    #[serde(default = "default_edit_project_check_enabled")]
    pub edit_project_check_enabled: bool,
    /// Hard timeout (seconds) for the per-edit project check; the check is
    /// skipped (no advisory) if it exceeds this. Keeps slow whole-project
    /// checks (cargo/tsc) from blocking edits.
    #[serde(default = "default_edit_project_check_timeout_secs")]
    pub edit_project_check_timeout_secs: u64,
    /// Max characters `read_file` emits before truncating with a paging footer.
    /// Default 100_000 (the `MAX_READ_CHARS` baseline). Floor-clamped to 1000
    /// at the tool so a tiny value can't truncate everything.
    #[serde(default = "default_read_file_max_chars")]
    pub read_file_max_chars: usize,
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
            // Phase 6.2 default OFF. The IPC works either way — with
            // the flag off, the manual Synthesize button produces a
            // clearly-labelled stub; with it on, runs through the
            // configured LLM. Restart required to swap.
            entity_synthesizer_enabled: false,
            // Phase 7.4 default OFF. The watcher is fine but enables
            // background DB writes whenever any file under brain/
            // changes; the manual Sync button (Phase 7.2) is the
            // safer default. Restart required to swap.
            brain_watcher_enabled: false,
            // Sprint 1 default ON — facet store is zero-cost when
            // empty + first rebuild happens 30 min after first
            // candidate is pushed. Flip OFF only to A/B test the
            // prompt without the learned profile block.
            learning_enabled: true,
            // Sprint 2.1b default 30_000 tokens/day. Regex layer is
            // free + always runs; LLM layer is the budgeted fallback.
            // Set to 0 to disable the LLM layer entirely (regex-only
            // operation, no per-day token spend on extraction).
            learning_llm_daily_token_budget: 30_000,
            // Sprint 2.4b default ON. Sprint 2.3 (PR #223) validated
            // post-merge that the agent does fire put_page on explicit
            // prompts; the extractor is the safety net for missed
            // entities, budget-gated by the field below. Flip OFF only
            // to A/B compare against a no-extractor baseline.
            gbrain_extractor_enabled: true,
            // Sprint 2.4b default 30_000 tokens/day. Set to 0 to disable
            // the extractor entirely without flipping the boolean (the
            // dispatcher short-circuits on budget==0 even when enabled).
            gbrain_extractor_daily_token_budget: 30_000,
            // L3 Q1c default ON. Zero-LLM cost; runs every 360 ticks
            // (~3h @ 30s tick interval). The cost guard is the batch
            // size cap, not a token budget.
            importance_decay_enabled: true,
            // L3 Q1c default 100 nodes/batch. At 8 batches/day = 800
            // importance refreshes — easily covers a typical knowledge
            // base. Set to 0 to disable the loop without flipping the
            // boolean.
            importance_decay_batch_size: 100,
            // L3 §4.12.4 R1 default ON. Zero-LLM content scan; ~4h cadence.
            drift_detection_enabled: true,
            // L3 §4.12.4 R1 default 50 EntityPages/scan.
            drift_detection_batch_size: 50,
            // Bundle 26-B default 30 days — see field doc and
            // `default_skill_prune_min_unused_days()`.
            skill_prune_min_unused_days: 30,
            // Bundle 26-D default 3 returns — see field doc and
            // `default_skill_promote_min_returned_count()`.
            skill_promote_min_returned_count: 3,
            // PR16 — matches default_recall_semantic_max_scan().
            recall_semantic_max_scan: 5000,
            // item2 — matches default_edit_project_check_enabled().
            edit_project_check_enabled: false,
            // item2 — matches default_edit_project_check_timeout_secs().
            edit_project_check_timeout_secs: 5,
            // item3.3b — matches default_read_file_max_chars().
            read_file_max_chars: 100_000,
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
            embedding_endpoint: EmbeddingEndpointConfig::default(),
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
            stream_idle_timeout_secs: 90,
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
        // Bundle 24 — extraction cadence doubled after Bundle 20/22/23
        // proved the loop produces useful skills end-to-end. Old
        // defaults (10 calls / 2 min) were a v0 safety belt; the
        // pipeline is now stable (dedup at the Procedure-node layer
        // catches near-duplicates, Bundle 22 persists to disk,
        // Bundle 23 makes them same-session-visible). 5 calls /
        // 60 s gives roughly 2× the learning surface without
        // flooding the LLM budget — the extraction prompt size
        // hasn't changed, only the call frequency.
        //
        // If junk skills start landing on disk in volume, tune
        // these back via Settings → Memory OS (the field is
        // hot-reloaded per tick from `memubot_config`, so no
        // rebuild needed for an in-flight rollback).
        Self {
            enabled: true,
            trigger_execution_count: 5,
            trigger_on_failure: true,
            min_interval_ms: 60_000, // 1 分钟
            memory_types: vec!["skill".to_string(), "tool".to_string()],
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

impl Default for EmbeddingEndpointConfig {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:7337/v1".to_string(),
            model: "llama-server:bge-small-en-v1.5".to_string(),
            dimensions: 384,
            fastembed_model: "BAAI/bge-small-en-v1.5".to_string(),
            // PR16 — matches default_embed_timeout_secs().
            embed_timeout_secs: 8,
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
            fold_delta_threshold: default_fold_delta_threshold(),
        }
    }
}

/// Bundle 17-B default — across-axis-cumulative delta count below which
/// `/compact` takes the delta-rendered path. See `ContextConfig::fold_delta_threshold`.
///
/// **Initially 50 (loose default, 2026-05-22).** The original spec §6.1 picked
/// 5 as a wild guess. Live E2E on session `78c1d9fd-...` showed: 2 of 3
/// `/compact` attempts failed at the LLM tier (one "high risk" rejection +
/// one timeout/empty-response → JSON parse failure) before the delta path
/// could even be exercised. With 50 we favor *any* successful delta path
/// firing until telemetry from C1.1 PR-2 (`FoldDeltaStats`) tells us a
/// data-driven number. Retune from histogram of observed `drift` values
/// within 2 weeks of merge per spec §6.1 + Bundle 17-D resilience design.
fn default_fold_delta_threshold() -> u32 {
    50
}

/// Bundle 17-B — clamp range for `fold_delta_threshold` write path.
/// Below 1 disables the delta path entirely (every compact re-renders);
/// above 50 would let nearly-fresh folds slip through as deltas, defeating
/// the cache-stability benefit. Used by `set_fold_delta_threshold` Tauri
/// command and by anyone updating MemubotConfig programmatically.
pub const FOLD_DELTA_THRESHOLD_MIN: u32 = 1;
pub const FOLD_DELTA_THRESHOLD_MAX: u32 = 50;

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
        let content = serde_json::to_string_pretty(self).map_err(crate::error::Error::Serde)?;
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
        assert_eq!(config.skill_extraction.trigger_execution_count, 5);
        assert!(config.skill_extraction.trigger_on_failure);
        assert_eq!(config.skill_extraction.min_interval_ms, 60_000);
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
        assert_eq!(deserialized.skill_extraction.trigger_execution_count, 5);
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
    fn memory_os_config_default_has_importance_decay_enabled() {
        // L3 §4.12.1 RETAINED. Default ON (zero LLM cost) so the
        // scheduled batch updates importance scores nightly.
        let c = MemoryOsConfig::default();
        assert!(
            c.importance_decay_enabled,
            "Importance Decay (L3 §4.12.1) default should be on"
        );
        assert_eq!(
            c.importance_decay_batch_size, 100,
            "default batch size should be 100"
        );
    }

    #[test]
    fn memory_os_config_default_has_drift_detection_enabled() {
        let c = MemoryOsConfig::default();
        assert!(c.drift_detection_enabled, "Drift Detection (L3 §4.12.4) default on");
        assert_eq!(c.drift_detection_batch_size, 50);
    }

    #[test]
    fn memory_os_config_drift_detection_partial_json_keeps_defaults() {
        let json = r#"{"memory_os":{"wiki_view_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.drift_detection_enabled);
        assert_eq!(config.memory_os.drift_detection_batch_size, 50);
    }

    #[test]
    fn memory_os_config_importance_decay_partial_json_keeps_defaults() {
        // Forward-compat: omitting importance_decay_* keys in user
        // config doesn't accidentally flip them off.
        let json = r#"{"memory_os":{"wiki_view_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.importance_decay_enabled);
        assert_eq!(config.memory_os.importance_decay_batch_size, 100);
    }

    #[test]
    fn memory_os_config_importance_decay_explicit_disable_preserved() {
        let json = r#"{"memory_os":{"importance_decay_enabled":false,"importance_decay_batch_size":0}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(!config.memory_os.importance_decay_enabled);
        assert_eq!(config.memory_os.importance_decay_batch_size, 0);
        // Other flags must NOT be affected by toggling these.
        assert!(config.memory_os.entity_page_enabled);
        assert!(config.memory_os.auto_link_enabled);
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
    fn stream_idle_timeout_secs_default_is_90() {
        // Bundle 27-B — 90s is the documented default. Lowering this
        // here without updating settings UI copy + PR body would be a
        // silent UX regression.
        let c = MemubotConfig::default();
        assert_eq!(c.stream_idle_timeout_secs, 90);
    }

    #[test]
    fn stream_idle_timeout_secs_explicit_value_preserved() {
        let json = r#"{"stream_idle_timeout_secs": 30}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.stream_idle_timeout_secs, 30);
        // Forward-compat: opting into a custom timeout doesn't change
        // sibling defaults.
        assert_eq!(config.agent_loop_timeout_secs, 600);
        assert!(config.plan_mode_suggest_enabled);
    }

    #[test]
    fn pre_bundle_27b_config_still_deserializes() {
        // A config file written before Bundle 27-B exposed the
        // setting won't have `stream_idle_timeout_secs` at all —
        // `#[serde(default)]` must fall back to 90s.
        let json = r#"{
            "agent_loop_timeout_secs": 600,
            "plan_mode_suggest_enabled": true
        }"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.stream_idle_timeout_secs, 90,
            "missing field must default to 90"
        );
    }

    #[test]
    fn skill_distillation_thresholds_have_documented_defaults() {
        // Bundle 26-B/26-D — defaults must match the values
        // claimed in the PR body and the settings UI help copy.
        // The original 26-B commit shipped an inline 7.0 literal
        // by mistake; this default-correction is intentional
        // (matches review_scheduler.rs's 30d cold-storage window).
        let c = MemoryOsConfig::default();
        assert_eq!(c.skill_prune_min_unused_days, 30);
        assert_eq!(c.skill_promote_min_returned_count, 3);
    }

    #[test]
    fn pre_settings_exposure_memory_os_still_deserializes() {
        // A `memory_os` block written before the settings exposure
        // PR won't have the two new skill_* fields. `#[serde(default)]`
        // must populate them with the documented defaults.
        let json = r#"{"memory_os":{
            "entity_page_enabled": true,
            "auto_link_enabled": true,
            "wiki_view_enabled": true,
            "memory_health_enabled": true,
            "memory_lint_enabled": true,
            "memory_lint_daily_token_budget": 50000,
            "tier_escalator_enabled": true,
            "tier_escalator_daily_cap": 10
        }}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.memory_os.skill_prune_min_unused_days, 30,
            "missing skill_prune_min_unused_days must default to 30"
        );
        assert_eq!(
            config.memory_os.skill_promote_min_returned_count, 3,
            "missing skill_promote_min_returned_count must default to 3"
        );
    }

    #[test]
    fn skill_distillation_explicit_values_preserved() {
        let json = r#"{"memory_os":{
            "skill_prune_min_unused_days": 7,
            "skill_promote_min_returned_count": 5
        }}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.memory_os.skill_prune_min_unused_days, 7);
        assert_eq!(config.memory_os.skill_promote_min_returned_count, 5);
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
        assert!(
            !config.memory_os.wiki_real_synthesizer_enabled,
            "6c alone must NOT flip 6b on"
        );
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
        assert!(
            config.memory_os.tier_escalator_enabled,
            "flag default holds"
        );
    }

    #[test]
    fn memory_os_phase62_default_keeps_stub_synthesizer() {
        let c = MemoryOsConfig::default();
        assert!(
            !c.entity_synthesizer_enabled,
            "Phase 6.2 default must be OFF (stub stays baseline)"
        );
    }

    #[test]
    fn memory_os_phase62_explicit_enable_preserved() {
        let json = r#"{"memory_os":{"entity_synthesizer_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.entity_synthesizer_enabled);
        // Forward-compat: 6.2 alone doesn't flip the related flags.
        assert!(!config.memory_os.wiki_real_synthesizer_enabled);
        assert!(!config.memory_os.lint_real_analyzer_enabled);
        assert!(config.memory_os.tier_escalator_enabled);
    }

    #[test]
    fn memory_os_phase74_default_keeps_watcher_off() {
        let c = MemoryOsConfig::default();
        assert!(
            !c.brain_watcher_enabled,
            "Phase 7.4 default must be OFF — fs events too noisy for blanket-on"
        );
    }

    #[test]
    fn memory_os_phase74_explicit_enable_preserved() {
        let json = r#"{"memory_os":{"brain_watcher_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(config.memory_os.brain_watcher_enabled);
        // Forward-compat — turning on 7.4 leaves Phase 6 flags alone.
        assert!(!config.memory_os.wiki_real_synthesizer_enabled);
        assert!(!config.memory_os.entity_synthesizer_enabled);
    }
}

#[cfg(test)]
mod embedding_endpoint_tests {
    use super::*;

    #[test]
    fn default_points_at_local_api() {
        let cfg = EmbeddingEndpointConfig::default();
        assert_eq!(cfg.base_url, "http://localhost:7337/v1");
        assert_eq!(cfg.model, "llama-server:bge-small-en-v1.5");
        assert_eq!(cfg.dimensions, 384);
        assert_eq!(cfg.fastembed_model, "BAAI/bge-small-en-v1.5");
    }

    #[test]
    fn memubot_default_includes_embedding_endpoint() {
        let cfg = MemubotConfig::default();
        // The field is present + has the right default.
        assert_eq!(cfg.embedding_endpoint.dimensions, 384);
    }

    #[test]
    fn embedding_endpoint_round_trips_through_json() {
        let cfg = EmbeddingEndpointConfig {
            base_url: "https://api.openai.com/v1".to_string(),
            model: "openai:text-embedding-3-large".to_string(),
            dimensions: 3072,
            fastembed_model: "BAAI/bge-m3".to_string(),
            embed_timeout_secs: 15,
        };
        let json = serde_json::to_string(&cfg).unwrap();
        let parsed: EmbeddingEndpointConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.base_url, cfg.base_url);
        assert_eq!(parsed.model, cfg.model);
        assert_eq!(parsed.dimensions, cfg.dimensions);
        assert_eq!(parsed.fastembed_model, cfg.fastembed_model);
        assert_eq!(parsed.embed_timeout_secs, cfg.embed_timeout_secs);
    }

    #[test]
    fn missing_field_falls_back_to_default() {
        // Older config files won't have embedding_endpoint at all —
        // verify `#[serde(default)]` on the field + `#[serde(default)]`
        // on EmbeddingEndpointConfig together cover this.
        let legacy_json = r#"{}"#;
        let cfg: MemubotConfig = serde_json::from_str(legacy_json).unwrap();
        // Default values land:
        assert_eq!(cfg.embedding_endpoint.base_url, "http://localhost:7337/v1");
    }

    // ─── PR16 config field tests ────────────────────────────────────────────

    #[test]
    fn embedding_config_default_timeout_is_8() {
        assert_eq!(EmbeddingEndpointConfig::default().embed_timeout_secs, 8);
    }

    #[test]
    fn memory_os_default_scan_cap_is_5000() {
        assert_eq!(MemoryOsConfig::default().recall_semantic_max_scan, 5000);
    }

    #[test]
    fn embedding_config_deserializes_without_timeout_field() {
        // Old config files lack the key → serde default fills 8.
        let json = r#"{"base_url":"http://x/v1","model":"m","dimensions":384,"fastembed_model":"f"}"#;
        let cfg: EmbeddingEndpointConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.embed_timeout_secs, 8);
    }

    #[test]
    fn memory_os_deserializes_without_recall_max_scan_field() {
        // Old config files lack the key → serde default fills 5000.
        let json = r#"{"memory_os":{"entity_page_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.memory_os.recall_semantic_max_scan, 5000,
            "missing recall_semantic_max_scan must default to 5000"
        );
    }

    #[test]
    fn embedding_config_explicit_timeout_preserved() {
        let json = r#"{"base_url":"http://x/v1","model":"m","dimensions":384,"fastembed_model":"f","embed_timeout_secs":30}"#;
        let cfg: EmbeddingEndpointConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.embed_timeout_secs, 30);
    }

    #[test]
    fn memory_os_explicit_scan_cap_preserved() {
        let json = r#"{"memory_os":{"recall_semantic_max_scan":1000}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(config.memory_os.recall_semantic_max_scan, 1000);
        // Forward-compat: setting this alone doesn't flip other flags.
        assert!(config.memory_os.entity_page_enabled);
    }

    // ─── item2 config field tests ───────────────────────────────────────────

    #[test]
    fn memory_os_default_project_check_fields() {
        let cfg = MemoryOsConfig::default();
        assert!(!cfg.edit_project_check_enabled);
        assert_eq!(cfg.edit_project_check_timeout_secs, 5);
    }

    #[test]
    fn memory_os_deserializes_without_project_check_fields() {
        // Old config files that predate item2 lack both keys.
        // Serde per-field defaults must fill them in.
        let json = r#"{"memory_os":{"entity_page_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert!(
            !config.memory_os.edit_project_check_enabled,
            "missing edit_project_check_enabled must default to false"
        );
        assert_eq!(
            config.memory_os.edit_project_check_timeout_secs, 5,
            "missing edit_project_check_timeout_secs must default to 5"
        );
    }

    // ─── item3.3b config field tests ────────────────────────────────────────

    #[test]
    fn memory_os_default_read_file_max_chars() {
        // Default must equal the MAX_READ_CHARS baseline constant.
        let cfg = MemoryOsConfig::default();
        assert_eq!(cfg.read_file_max_chars, 100_000);
    }

    #[test]
    fn memory_os_deserializes_without_read_file_max_chars_field() {
        // Old config files that predate item3.3b lack the key.
        // Serde per-field default must fill 100_000.
        let json = r#"{"memory_os":{"entity_page_enabled":true}}"#;
        let config: MemubotConfig = serde_json::from_str(json).unwrap();
        assert_eq!(
            config.memory_os.read_file_max_chars, 100_000,
            "missing read_file_max_chars must default to 100_000"
        );
    }
}
