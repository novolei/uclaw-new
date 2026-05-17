//! ProactiveService — 24/7 主动代理服务
//!
//! 实现 memUBot 的主动轮询能力：
//! - 定时轮询（默认 30 秒间隔）
//! - 上下文滑动窗口监控（订阅 InfraService 消息总线）
//! - 自主 agent loop 执行（检测到新上下文时触发）
//! - 用户确认机制（破坏性操作前等待用户输入）
//!
//! ## 架构
//! ```text
//! InfraService ──subscribe──▶ context_listener (tokio task)
//!                                │
//!                                ▼
//!                         context_messages (VecDeque, 滑动窗口)
//!                                │
//!                                ▼
//! tick_loop (tokio task) ──检测 has_new_context──▶ tick_inner()
//!                                                     │
//!                                                     ▼
//!                                               agent loop (placeholder)
//!                                                     │
//!                                                     ▼
//!                                           ProactiveStorage (持久化)
//! ```

use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use std::path::PathBuf;

use rusqlite::Connection;

use async_trait::async_trait;

/// Extract the content of the first `<summary>…</summary>` XML tag from a string.
/// Falls back to the first 200 characters if no tag is found.
fn extract_summary_text(response: &str) -> String {
    if let Some(start) = response.find("<summary>") {
        let after = &response[start + "<summary>".len()..];
        if let Some(end) = after.find("</summary>") {
            return after[..end].trim().to_string();
        }
    }
    // No <summary> tag — return first 200 chars stripped of XML tags
    let stripped: String = {
        let mut out = String::new();
        let mut in_tag = false;
        for ch in response.chars().take(400) {
            match ch {
                '<' => in_tag = true,
                '>' => in_tag = false,
                c if !in_tag => out.push(c),
                _ => {}
            }
        }
        out.trim().chars().take(200).collect()
    };
    stripped
}
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::agent::types::ChatMessage;
use crate::infra::{InfraEventType, InfraService};
use crate::llm::provider::CompletionConfig;
use tauri::Emitter;
use crate::memu::client::MemUClient;
use crate::memubot_config::ProactiveConfig;
use crate::memory_graph::store::MemoryGraphStore;
use crate::providers::service::ProviderService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::conversation_bridge::ConversationBridge;
use super::execution_log::ExecutionLogCollector;
use super::hybrid_search::{HybridSearchEngine, HybridSearchRequest};
use super::multimodal::MultimodalQueue;

/// Build a <system_info> block with current date/time for authoritative time context
/// in proactive scenario LLM calls. Mirrors `ChatDelegate::build_system_time_block()`.
fn build_proactive_time_block() -> String {
    use chrono::{Datelike, Local, Timelike};
    let now = Local::now();
    let weekday = match now.weekday() {
        chrono::Weekday::Mon => "周一",
        chrono::Weekday::Tue => "周二",
        chrono::Weekday::Wed => "周三",
        chrono::Weekday::Thu => "周四",
        chrono::Weekday::Fri => "周五",
        chrono::Weekday::Sat => "周六",
        chrono::Weekday::Sun => "周日",
    };
    let time = format!(
        "{}年{}月{}日 {} {:02}:{:02}",
        now.year(),
        now.month(),
        now.day(),
        weekday,
        now.hour(),
        now.minute(),
    );
    format!(
        "<system_info>\n当前时间: {}\n注意: 以上时间由系统提供，你不需要使用工具（如 bash date）获取时间，直接使用此信息回答即可。\n</system_info>",
        time
    )
}

use super::scenarios::{ScenarioContext, ScenarioManager, SessionContextWindow};
use super::storage::ProactiveStorage;
use super::failure_memory::FailureMemoryManager;
use super::personality_model::PersonalityModel;
use super::preference_extractor::PreferenceExtractor;
use super::proactive_recall::ProactiveRecallService;
use super::task_memory::{TaskMemoryManager, TaskRecord, TaskStatus, TaskType};
use super::tool_memory::{ToolUsageMemoryManager, ToolUsageRecord};
use super::types::*;
use crate::agent::gep::types::{Capsule, BlastRadius, CapsuleOutcome, EnvFingerprint, EvolutionEvent, Gene, GeneCandidate, LearningCard, LearningCardType, OutcomeStatus, StrategyHint};
use crate::agent::gep::repository::GeneRepository;
use crate::agent::gep::distillation;

/// 上下文滑动窗口最大容量（每个 session 保留最近 N 条消息）
const CONTEXT_WINDOW_SIZE: usize = 20;

/// 最多保留的 session 窗口数（超出时淘汰最久未活跃的）
const MAX_SESSION_WINDOWS: usize = 10;

// ─── 状态引用结构体 ───────────────────────────────────────────────────

/// 辅助结构：持有所有需要跨 spawned task 共享的 Arc 引用
///
/// 由于 `ProactiveService` 本身不是 `Clone`（包含 JoinHandle），
/// 需要将纯数据部分的 Arc 引用打包成可 Clone 的结构体，
/// 传递给 `tokio::spawn` 的异步任务。
#[derive(Clone)]
struct ProactiveStateRefs {
    /// 轮询计数
    tick_count: Arc<AtomicU64>,
    /// 实际行动计数
    action_count: Arc<AtomicU64>,
    /// 无需行动计数
    no_message_count: Arc<AtomicU64>,
    /// 是否有新上下文（原子标记）
    has_new_context: Arc<AtomicBool>,
    /// 上次 tick 时间
    last_tick_at: Arc<RwLock<Option<String>>>,
    /// 上次行动时间
    last_action_at: Arc<RwLock<Option<String>>>,
    /// Per-session 上下文消息滑动窗口
    context_messages: Arc<RwLock<HashMap<String, SessionContextWindow>>>,
    /// 当前状态
    state: Arc<RwLock<ProactiveState>>,
    /// 消息持久化存储
    storage: Arc<ProactiveStorage>,
    /// 消息总线（用于发布主动消息事件）
    infra: Arc<InfraService>,
    /// 场景管理器
    scenario_manager: Arc<ScenarioManager>,
    /// 执行日志收集器
    execution_log_collector: Arc<ExecutionLogCollector>,
    /// 多模态输入队列
    multimodal_queue: Arc<MultimodalQueue>,
    /// Provider 服务（用于动态获取 LLM provider）
    provider_service: Arc<ProviderService>,
    /// memU 客户端（Python 不可用时为 None）
    memu_client: Option<Arc<MemUClient>>,
    /// Memory Graph 存储
    memory_graph_store: Arc<MemoryGraphStore>,
    /// Tauri AppHandle（用于发射 IPC 事件，测试时为 None）
    app_handle: Option<tauri::AppHandle>,
    /// 自上次触发以来的新消息数
    new_message_count: Arc<AtomicUsize>,
    /// 自上次触发以来的新执行次数
    new_execution_count: Arc<AtomicUsize>,
    /// 当前活跃的 Space ID（用于记忆召回）
    active_space_id: Arc<RwLock<String>>,
    /// 最近一次有 user/assistant 消息的 session_id（来自 InfraEvent.metadata
    /// 的 conversation_id 字段）。用于把 proactive-learning IPC 事件
    /// 标记到具体 session — 否则前端就只能在每个 session 都展示，造成噪声。
    last_active_session_id: Arc<RwLock<Option<String>>>,
    /// 任务记忆管理器（记录历史任务执行结果）
    task_memory_manager: Arc<TaskMemoryManager>,
    /// 工具使用记忆管理器（追踪工具调用模式和统计）
    tool_memory_manager: Arc<ToolUsageMemoryManager>,
    /// 五路混合检索引擎（向量+关键词+时间+图关系+文件）
    hybrid_search_engine: Arc<HybridSearchEngine>,
    /// 双轨会话桥接器（将传统 conversations 统一桥接到记忆系统）
    conversation_bridge: Arc<ConversationBridge>,
    /// 失败记忆管理器
    failure_memory_manager: Arc<FailureMemoryManager>,
    /// 偏好提取器
    preference_extractor: Arc<PreferenceExtractor>,
    /// 人格模型
    personality_model: Arc<PersonalityModel>,
    /// 主动召回服务
    proactive_recall_service: Arc<ProactiveRecallService>,
    /// Gene 候选池 — 收集 self_eval 产出的 LearningCard，等待蒸馏
    gene_candidate_pool: Arc<RwLock<VecDeque<GeneCandidate>>>,
    /// Gene 候选池有新候选的标记
    new_gene_candidates: Arc<AtomicBool>,
    /// 主数据库连接（用于查询 agent_messages/agent_turns/agent_sessions）
    db: Arc<Mutex<Connection>>,
    /// 上次人格画像更新时间
    last_personality_update: Arc<Mutex<Instant>>,
    /// GEP Gene 文件存储
    gene_repo: Arc<Mutex<GeneRepository>>,
}

// ─── ProactiveService ─────────────────────────────────────────────────

/// 24/7 主动代理服务
///
/// 通过 `ManagedService` trait 由 `ServiceManager` 统一管理。
/// 启动后会创建两个后台 tokio task：
/// 1. `context_listener` — 订阅 InfraService，维护上下文滑动窗口
/// 2. `tick_loop` — 按配置间隔轮询，检测新上下文后运行 agent loop
pub struct ProactiveService {
    /// 配置
    config: ProactiveConfig,
    /// 当前状态
    state: Arc<RwLock<ProactiveState>>,
    /// 是否正在运行（控制后台任务生命周期）
    is_running: Arc<AtomicBool>,

    /// Per-session 上下文消息滑动窗口
    context_messages: Arc<RwLock<HashMap<String, SessionContextWindow>>>,
    /// 是否有新的上下文消息（自上次 tick 以来）
    has_new_context: Arc<AtomicBool>,

    /// 是否正在等待用户输入（wait_user_confirm 工具）
    is_waiting_user_input: Arc<AtomicBool>,
    /// 用户输入的响应内容
    user_input_response: Arc<RwLock<Option<String>>>,

    /// 消息总线
    infra: Arc<InfraService>,
    /// 消息持久化存储
    storage: Arc<ProactiveStorage>,

    /// 场景管理器
    scenario_manager: Arc<ScenarioManager>,
    /// 执行日志收集器
    execution_log_collector: Arc<ExecutionLogCollector>,
    /// 多模态输入队列
    multimodal_queue: Arc<MultimodalQueue>,
    /// Provider 服务（动态获取 LLM 配置）
    provider_service: Arc<ProviderService>,
    /// memU 客户端（Python 不可用时为 None）
    memu_client: Option<Arc<MemUClient>>,
    /// Memory Graph 存储
    memory_graph_store: Arc<MemoryGraphStore>,
    /// Tauri AppHandle（用于发射 IPC 事件，测试时为 None）
    app_handle: Option<tauri::AppHandle>,

    /// 统计：轮询次数
    tick_count: Arc<AtomicU64>,
    /// 统计：实际行动次数
    action_count: Arc<AtomicU64>,
    /// 统计：无需行动次数
    no_message_count: Arc<AtomicU64>,
    /// 上次 tick 时间
    last_tick_at: Arc<RwLock<Option<String>>>,
    /// 上次行动时间
    last_action_at: Arc<RwLock<Option<String>>>,
    /// 自上次场景触发以来的新消息数
    new_message_count: Arc<AtomicUsize>,
    /// 自上次场景触发以来的新执行次数
    new_execution_count: Arc<AtomicUsize>,
    /// 当前活跃的 Space ID（用于记忆召回，默认 "default"）
    active_space_id: Arc<RwLock<String>>,
    /// 最近一次有消息流过来的 session_id（见 ProactiveStateRefs 同名字段）
    last_active_session_id: Arc<RwLock<Option<String>>>,
    /// 任务记忆管理器
    task_memory_manager: Arc<TaskMemoryManager>,
    /// 工具使用记忆管理器
    tool_memory_manager: Arc<ToolUsageMemoryManager>,
    /// 五路混合检索引擎
    hybrid_search_engine: Arc<HybridSearchEngine>,
    /// 双轨会话桥接器
    conversation_bridge: Arc<ConversationBridge>,
    /// 失败记忆管理器
    failure_memory_manager: Arc<FailureMemoryManager>,
    /// 偏好提取器
    preference_extractor: Arc<PreferenceExtractor>,
    /// 人格模型
    personality_model: Arc<PersonalityModel>,
    /// 主动召回服务
    proactive_recall_service: Arc<ProactiveRecallService>,

    /// 主数据库连接
    db: Arc<Mutex<Connection>>,

    /// 上次人格画像更新时间
    last_personality_update: Arc<Mutex<Instant>>,

    /// GEP Gene 文件存储
    gene_repo: Arc<Mutex<GeneRepository>>,

    /// Gene 候选池 — 收集 self_eval 产出的 LearningCard，等待蒸馏
    gene_candidate_pool: Arc<RwLock<VecDeque<GeneCandidate>>>,
    /// Gene 候选池有新候选的标记
    new_gene_candidates: Arc<AtomicBool>,

    /// 轮询循环任务句柄
    tick_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// 上下文监听任务句柄
    listener_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl ProactiveService {
    /// 创建新的 ProactiveService 实例
    ///
    /// - `config`: 主动服务配置（来自 MemubotConfig）
    /// - `infra`: 消息总线（用于订阅对话事件、发布主动消息事件）
    /// - `storage`: 消息持久化存储
    pub fn new(
        config: ProactiveConfig,
        infra: Arc<InfraService>,
        storage: Arc<ProactiveStorage>,
        scenario_manager: Arc<ScenarioManager>,
        execution_log_collector: Arc<ExecutionLogCollector>,
        multimodal_queue: Arc<MultimodalQueue>,
        provider_service: Arc<ProviderService>,
        memu_client: Option<Arc<MemUClient>>,
        memory_graph_store: Arc<MemoryGraphStore>,
        app_handle: Option<tauri::AppHandle>,
        db: Arc<Mutex<Connection>>,
        gene_repo: Arc<Mutex<GeneRepository>>,
    ) -> Self {
        let task_memory_manager = Arc::new(TaskMemoryManager::new(memory_graph_store.clone()));
        let tool_memory_manager = Arc::new(ToolUsageMemoryManager::new(memory_graph_store.clone()));
        let hybrid_search_engine = Arc::new(HybridSearchEngine::new(
            memory_graph_store.clone(),
            memu_client.clone(),
        ));
        let conversation_bridge = Arc::new(ConversationBridge::new(memory_graph_store.clone()));
        let failure_memory_manager = Arc::new(FailureMemoryManager::new(memory_graph_store.clone()));
        let preference_extractor = Arc::new(PreferenceExtractor::new(memory_graph_store.clone()));
        let personality_model = Arc::new(PersonalityModel::new(memory_graph_store.clone()));
        let proactive_recall_service = Arc::new(ProactiveRecallService::new(
            memory_graph_store.clone(),
            memu_client.clone(),
            task_memory_manager.clone(),
            tool_memory_manager.clone(),
            failure_memory_manager.clone(),
        ));
        Self {
            config,
            state: Arc::new(RwLock::new(ProactiveState::Stopped)),
            is_running: Arc::new(AtomicBool::new(false)),
            context_messages: Arc::new(RwLock::new(HashMap::new())),
            has_new_context: Arc::new(AtomicBool::new(false)),
            is_waiting_user_input: Arc::new(AtomicBool::new(false)),
            user_input_response: Arc::new(RwLock::new(None)),
            infra,
            storage,
            scenario_manager,
            execution_log_collector,
            multimodal_queue,
            provider_service,
            memu_client,
            memory_graph_store,
            app_handle,
            tick_count: Arc::new(AtomicU64::new(0)),
            action_count: Arc::new(AtomicU64::new(0)),
            no_message_count: Arc::new(AtomicU64::new(0)),
            last_tick_at: Arc::new(RwLock::new(None)),
            last_action_at: Arc::new(RwLock::new(None)),
            new_message_count: Arc::new(AtomicUsize::new(0)),
            new_execution_count: Arc::new(AtomicUsize::new(0)),
            active_space_id: Arc::new(RwLock::new("default".to_string())),
            last_active_session_id: Arc::new(RwLock::new(None)),
            task_memory_manager,
            tool_memory_manager,
            hybrid_search_engine,
            conversation_bridge,
            failure_memory_manager,
            preference_extractor,
            personality_model,
            proactive_recall_service,
            db,
            last_personality_update: Arc::new(Mutex::new(Instant::now())),
            gene_repo,
            gene_candidate_pool: Arc::new(RwLock::new(VecDeque::new())),
            new_gene_candidates: Arc::new(AtomicBool::new(false)),
            tick_handle: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// 克隆所有 Arc 引用，打包成 ProactiveStateRefs
    ///
    /// 用于传递给 spawned 的 tokio task，避免直接 move self。
    fn clone_state_refs(&self) -> ProactiveStateRefs {
        ProactiveStateRefs {
            tick_count: self.tick_count.clone(),
            action_count: self.action_count.clone(),
            no_message_count: self.no_message_count.clone(),
            has_new_context: self.has_new_context.clone(),
            last_tick_at: self.last_tick_at.clone(),
            last_action_at: self.last_action_at.clone(),
            context_messages: self.context_messages.clone(),
            state: self.state.clone(),
            storage: self.storage.clone(),
            infra: self.infra.clone(),
            scenario_manager: self.scenario_manager.clone(),
            execution_log_collector: self.execution_log_collector.clone(),
            multimodal_queue: self.multimodal_queue.clone(),
            provider_service: self.provider_service.clone(),
            memu_client: self.memu_client.clone(),
            memory_graph_store: self.memory_graph_store.clone(),
            app_handle: self.app_handle.clone(),
            new_message_count: self.new_message_count.clone(),
            new_execution_count: self.new_execution_count.clone(),
            active_space_id: self.active_space_id.clone(),
            last_active_session_id: self.last_active_session_id.clone(),
            task_memory_manager: self.task_memory_manager.clone(),
            tool_memory_manager: self.tool_memory_manager.clone(),
            hybrid_search_engine: self.hybrid_search_engine.clone(),
            conversation_bridge: self.conversation_bridge.clone(),
            failure_memory_manager: self.failure_memory_manager.clone(),
            preference_extractor: self.preference_extractor.clone(),
            personality_model: self.personality_model.clone(),
            proactive_recall_service: self.proactive_recall_service.clone(),
            db: self.db.clone(),
            last_personality_update: self.last_personality_update.clone(),
            gene_repo: self.gene_repo.clone(),
            gene_candidate_pool: self.gene_candidate_pool.clone(),
            new_gene_candidates: self.new_gene_candidates.clone(),
        }
    }

    /// Get a reference to the GEP GeneRepository.
    pub fn gene_repository(&self) -> Arc<Mutex<GeneRepository>> {
        self.gene_repo.clone()
    }

    /// 启动上下文监听任务
    ///
    /// 订阅 InfraService 的消息总线，将 Incoming / Outgoing 消息
    /// 写入上下文滑动窗口，并设置 `has_new_context` 标记。
    async fn start_context_listener(&self) {
        let mut rx = self.infra.subscribe();
        let context = self.context_messages.clone();
        let has_new = self.has_new_context.clone();
        let is_running = self.is_running.clone();
        let new_msg_count = self.new_message_count.clone();
        let new_exec_count = self.new_execution_count.clone();
        let exec_log_collector = self.execution_log_collector.clone();
        let last_session = self.last_active_session_id.clone();
        let active_space_id = self.active_space_id.clone();
        let tool_memory = self.tool_memory_manager.clone();
        let gene_pool = self.gene_candidate_pool.clone();
        let new_gene_candidates_flag = self.new_gene_candidates.clone();

        let handle = tokio::spawn(async move {
            while is_running.load(Ordering::SeqCst) {
                match rx.recv().await {
                    Ok(event) => {
                        match event.event_type {
                            // 用户消息和 Bot 回复 → 维护上下文滑动窗口
                            InfraEventType::MessageIncoming | InfraEventType::MessageOutgoing => {
                                // Determine session key from metadata
                                let session_key = event.metadata
                                    .get("conversation_id")
                                    .or_else(|| event.metadata.get("session_id"))
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("default")
                                    .to_string();

                                let mut windows = context.write().await;
                                let window = windows
                                    .entry(session_key.clone())
                                    .or_insert_with(|| SessionContextWindow::new(
                                        session_key.clone(),
                                        CONTEXT_WINDOW_SIZE,
                                    ));
                                window.messages.push_back(event.message);
                                if window.messages.len() > CONTEXT_WINDOW_SIZE {
                                    window.messages.pop_front();
                                }
                                window.touch();

                                // Enforce max session windows (LRU eviction)
                                if windows.len() > MAX_SESSION_WINDOWS {
                                    let mut entries: Vec<_> = windows.iter().collect();
                                    entries.sort_by_key(|(_, w)| w.last_active_at);
                                    if let Some((oldest_key, _)) = entries.first() {
                                        let key = (*oldest_key).to_string();
                                        windows.remove(&key);
                                        tracing::debug!(
                                            "[ProactiveService] LRU evicted session window: {}",
                                            key
                                        );
                                    }
                                }

                                has_new.store(true, Ordering::SeqCst);
                                new_msg_count.fetch_add(1, Ordering::SeqCst);
                                // Track last-active session so the next proactive
                                // extraction tick can tag its IPC payload with
                                // the session that most likely sourced it.
                                *last_session.write().await = Some(session_key);
                            }
                            // 工具执行事件 → 记录到 ExecutionLogCollector
                            InfraEventType::ToolExecuted => {
                                let metadata = &event.metadata;
                                let tool_name = metadata.get("tool_name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let success = metadata.get("success")
                                    .and_then(|v| v.as_bool())
                                    .unwrap_or(false);
                                let duration_ms = metadata.get("duration_ms")
                                    .and_then(|v| v.as_u64())
                                    .unwrap_or(0);
                                let tool_input_str = metadata.get("tool_input")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("{}");
                                let tool_output_str = &event.message.content;

                                let log = crate::proactive::scenarios::types::ExecutionLog {
                                    session_id: String::new(),
                                    iteration: 0,
                                    tool_name: tool_name.clone(),
                                    tool_input: serde_json::json!({ "summary": tool_input_str }),
                                    tool_output: serde_json::json!({ "summary": tool_output_str }),
                                    success,
                                    duration_ms,
                                    timestamp: chrono::Utc::now().timestamp(),
                                    context_summary: "tool execution".to_string(),
                                };
                                exec_log_collector.push(log).await;
                                new_exec_count.fetch_add(1, Ordering::SeqCst);
                                has_new.store(true, Ordering::SeqCst);

                                // 记录工具使用到 ToolUsageMemoryManager
                                let space_id = active_space_id.read().await.clone();
                                let tool_usage = ToolUsageRecord {
                                    tool_name: tool_name.clone(),
                                    success,
                                    duration_ms,
                                    output_size_bytes: Some(tool_output_str.len() as u64),
                                    parameters_fingerprint: Some(tool_input_str.to_string()),
                                    session_id: last_session.read().await.clone(),
                                    task_description: None,
                                };
                                let _ = tool_memory.record_tool_usage(&space_id, &tool_usage);
                            }
                            // 工作区切换事件 → 更新 active_space_id
                            InfraEventType::WorkspaceSwitched => {
                                if let Some(new_id) = event.metadata
                                    .get("new_workspace_id")
                                    .and_then(|v| v.as_str())
                                {
                                    let old = active_space_id.read().await.clone();
                                    *active_space_id.write().await = new_id.to_string();
                                    tracing::info!(
                                        "[ProactiveService] 工作区切换: {} -> {}",
                                        old, new_id
                                    );
                                }
                            }
                            // Gene 学习事件 → 解析 LearningCard 推入候选池
                            InfraEventType::SkillLearned => {
                                if let Some(card_json) = event.metadata.get("learning_card") {
                                    let card_obj: serde_json::Value = match serde_json::from_value(card_json.clone()) {
                                        Ok(v) => v,
                                        Err(e) => {
                                            tracing::warn!("[ProactiveService] Failed to parse learning_card: {}", e);
                                            continue;
                                        }
                                    };

                                    let card_type = match card_obj.get("card_type").and_then(|v| v.as_str()).unwrap_or("noise") {
                                        "failure_lesson" => LearningCardType::FailureLesson,
                                        "success_pattern" => LearningCardType::SuccessPattern,
                                        "optimization_tip" => LearningCardType::OptimizationTip,
                                        _ => LearningCardType::Noise,
                                    };

                                    // Skip noise (self_eval already filtered, double-check)
                                    if card_type == LearningCardType::Noise {
                                        continue;
                                    }

                                    let strategy_hint: StrategyHint = card_obj
                                        .get("strategy_hint")
                                        .map(|v| serde_json::from_value(v.clone()).unwrap_or_default())
                                        .unwrap_or_default();

                                    let learning_card = LearningCard {
                                        raw: event.message.content.clone(),
                                        card_type,
                                        failure_signal: card_obj.get("failure_signal").and_then(|v| v.as_str()).map(String::from),
                                        tool_name: card_obj.get("tool_name").and_then(|v| v.as_str()).map(String::from),
                                        strategy_hint,
                                        files_touched: vec![],
                                        session_id: event.metadata
                                            .get("session_id")
                                            .and_then(|v| v.as_str())
                                            .unwrap_or("unknown")
                                            .to_string(),
                                        score: event.metadata
                                            .get("score")
                                            .and_then(|v| v.as_f64())
                                            .unwrap_or(0.0) as f32,
                                        timestamp: event.timestamp,
                                    };

                                    let candidate = GeneCandidate {
                                        source: "self_eval".to_string(),
                                        content: learning_card.raw.clone(),
                                        score: Some(learning_card.score as f64),
                                        session_id: Some(learning_card.session_id.clone()),
                                        reasoning: learning_card.strategy_hint.reason.clone(),
                                        timestamp: chrono::Utc::now(),
                                    };

                                    let mut pool = gene_pool.write().await;
                                    // Capacity control: max 20, evict lowest-score entry
                                    const MAX_CANDIDATES: usize = 20;
                                    if pool.len() >= MAX_CANDIDATES {
                                        let mut min_idx = 0usize;
                                        let mut min_score = f64::MAX;
                                        for (i, c) in pool.iter().enumerate() {
                                            let s = c.score.unwrap_or(0.0);
                                            if s < min_score {
                                                min_score = s;
                                                min_idx = i;
                                            }
                                        }
                                        pool.remove(min_idx);
                                    }
                                    pool.push_back(candidate);
                                    new_gene_candidates_flag.store(true, Ordering::SeqCst);
                                    has_new.store(true, Ordering::SeqCst);
                                    tracing::info!(
                                        "[ProactiveService] Gene candidate added, pool_size={}",
                                        pool.len()
                                    );
                                }
                            }
                            _ => {} // 忽略其他事件类型
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        tracing::warn!(
                            "[ProactiveService] 消息接收落后 {} 条，部分上下文可能丢失",
                            n
                        );
                    }
                    Err(_) => {
                        tracing::info!("[ProactiveService] 消息总线已关闭，上下文监听退出");
                        break;
                    }
                }
            }
            tracing::debug!("[ProactiveService] 上下文监听任务已退出");
        });

        *self.listener_handle.write().await = Some(handle);
    }

    /// 启动轮询循环任务
    ///
    /// 按 `config.interval_ms` 间隔执行 tick，每次 tick 检查是否有新上下文，
    /// 有则运行 agent loop。
    async fn start_tick_loop(&self) {
        let interval = self.config.interval_ms;
        let is_running = self.is_running.clone();
        let refs = self.clone_state_refs();

        let handle = tokio::spawn(async move {
            tracing::info!(
                "[ProactiveService] 轮询循环启动，间隔: {}ms",
                interval
            );

            loop {
                // 先等待一个间隔周期
                tokio::time::sleep(tokio::time::Duration::from_millis(interval)).await;

                // 检查是否需要退出
                if !is_running.load(Ordering::SeqCst) {
                    break;
                }

                // 执行单次 tick
                if let Err(e) = Self::tick_inner(&refs).await {
                    tracing::error!("[ProactiveService] tick 执行错误: {}", e);
                }
            }

            tracing::debug!("[ProactiveService] 轮询循环任务已退出");
        });

        *self.tick_handle.write().await = Some(handle);
    }

    /// 单次轮询 tick
    ///
    /// 1. 更新 tick 计数和时间戳
    /// 2. 检查是否有新上下文（无则跳过）
    /// 3. 切换状态为 Thinking
    /// 4. 运行 agent loop（当前为 placeholder）
    /// 5. 处理 agent 返回结果
    /// 6. 切换状态回 Idle
    async fn tick_inner(refs: &ProactiveStateRefs) -> anyhow::Result<()> {
        // 递增 tick 计数
        refs.tick_count.fetch_add(1, Ordering::SeqCst);
        *refs.last_tick_at.write().await = Some(chrono::Utc::now().to_rfc3339());

        // 每 20 个 tick（约 10 分钟，按默认 30s 间隔）将传统会话桥接到记忆系统
        // 解决 conversations 与 agent_sessions 双轨割裂导致的记忆流失问题
        if refs.tick_count.load(Ordering::SeqCst) % 20 == 0 {
            let space_id = refs.active_space_id.read().await.clone();
            match refs.conversation_bridge.bridge_incremental(&space_id, 50) {
                Ok(stats) => {
                    if stats.messages_enqueued > 0 {
                        tracing::info!(
                            enqueued = stats.messages_enqueued,
                            conversations = stats.conversations_with_new,
                            scanned = stats.conversations_scanned,
                            duration_ms = stats.duration_ms,
                            "[ProactiveService] ConversationBridge 增量同步完成"
                        );
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        error = %e,
                        "[ProactiveService] ConversationBridge 增量同步失败"
                    );
                }
            }
        }

        // 每 100 次 tick 更新人格画像（并带最小时间间隔 5 分钟）
        if refs.tick_count.load(Ordering::SeqCst) % 100 == 0 {
            let space = refs.active_space_id.read().await.clone();
            if !space.is_empty() {
                let should_update = refs
                    .last_personality_update
                    .lock()
                    .map(|guard| guard.elapsed() > Duration::from_secs(300))
                    .unwrap_or(false);
                if should_update {
                    if let Ok(mut guard) = refs.last_personality_update.lock() {
                        *guard = Instant::now();
                    }
                    let personality = refs.personality_model.clone();
                    tokio::spawn(async move {
                        if let Err(e) = personality.update_personality_profile(&space) {
                            tracing::warn!("personality update failed: {}", e);
                        }
                    });
                }
            }
        }

        // 检查是否有新上下文消息
        if !refs.has_new_context.load(Ordering::SeqCst) {
            return Ok(()); // 无新上下文，跳过本次 tick
        }

        // 清除新上下文标记
        refs.has_new_context.store(false, Ordering::SeqCst);

        // 切换状态为 Thinking
        *refs.state.write().await = ProactiveState::Thinking;
        tracing::info!("[ProactiveService] 检测到新上下文，运行主动 agent loop");

        // ── 场景驱动 Agent Loop ──────────────────────────────────────
        let response = Self::run_scenario_loop(refs).await?;
        // ──────────────────────────────────────────────────────────────

        // 处理 agent 返回结果
        if response.trim() == NO_MESSAGE_MARKER {
            // Agent 判断无需行动
            refs.no_message_count.fetch_add(1, Ordering::SeqCst);
            tracing::debug!("[ProactiveService] Agent 判断无需行动 (NO_MESSAGE)");
        } else {
            // Agent 生成了主动消息
            refs.action_count.fetch_add(1, Ordering::SeqCst);
            let now = chrono::Utc::now().to_rfc3339();
            *refs.last_action_at.write().await = Some(now.clone());

            let preview: String = response.chars().take(50).collect();
            tracing::info!(
                "[ProactiveService] Agent 生成主动消息: {}...",
                preview
            );

            // 构造主动消息并持久化
            let msg = ProactiveMessage {
                id: uuid::Uuid::new_v4().to_string(),
                content: response.clone(),
                generated_at: now,
                trigger_reason: "上下文变化触发".to_string(),
                tools_used: vec![],
            };

            if let Err(e) = refs.storage.save_message(&msg) {
                tracing::error!("[ProactiveService] 保存主动消息失败: {}", e);
            }

            // 通过消息总线通知前端（发布为 outgoing 消息）
            refs.infra
                .publish_outgoing(
                    "proactive",
                    &response,
                    serde_json::json!({
                        "source": "proactive",
                        "message_id": msg.id,
                    }),
                )
                .await;

            // 记录任务到 TaskMemoryManager
            let space_id = refs.active_space_id.read().await.clone();
            let task_record = TaskRecord {
                task_type: TaskType::Planning,
                description: format!(
                    "Proactive scenario evaluation: {}",
                    extract_summary_text(&response)
                ),
                status: TaskStatus::Success,
                files_changed: Vec::new(),
                tools_used: Vec::new(),
                duration_ms: 0,
                error_messages: Vec::new(),
                solution_summary: Some(response.chars().take(200).collect()),
                session_id: refs.last_active_session_id.read().await.clone(),
            };
            let _ = refs.task_memory_manager.record_task(&space_id, &task_record);
        }

        // 切换状态回 Idle
        *refs.state.write().await = ProactiveState::Idle;

        Ok(())
    }

    /// 场景驱动的主动 Agent Loop
    ///
    /// 1. 构建 ScenarioContext
    /// 2. 评估所有场景是否触发
    /// 3. 对触发的场景构建上下文并调用 LLM
    /// 4. 合并响应
    async fn run_scenario_loop(refs: &ProactiveStateRefs) -> anyhow::Result<String> {
        // 1. 构建 ScenarioContext — 先从 active session 窗口收集消息
        let active_space_id = refs.active_space_id.read().await.clone();
        let active_session_id = refs.last_active_session_id.read().await.clone();

        let ctx_windows = refs.context_messages.read().await;
        let recent_messages: Vec<_> = active_session_id
            .as_ref()
            .and_then(|sid| ctx_windows.get(sid))
            .map(|w| w.messages.iter().cloned().collect())
            .unwrap_or_default();
        drop(ctx_windows);

        let execution_logs = refs.execution_log_collector.recent(50).await;
        let pending_multimodal = refs.multimodal_queue.peek_all().await;
        let last_trigger_map = refs.scenario_manager.get_last_trigger_map().await;
        let tick_count = refs.tick_count.load(Ordering::SeqCst);
        let new_message_count = refs.new_message_count.load(Ordering::SeqCst);
        let new_execution_count = refs.new_execution_count.load(Ordering::SeqCst);

        // 检查最近是否有失败
        let recent_failures = refs.execution_log_collector.failures(1).await;
        let has_failures = !recent_failures.is_empty();

        // 构建会话上下文摘要 — 从 agent_messages/agent_turns/agent_sessions 提取
        let session_context = if let Some(ref session_id) = active_session_id {
            let tool_calls: Vec<_> = execution_logs.iter()
                .take(10)
                .map(|log| crate::proactive::scenarios::types::ToolCallSummary {
                    tool_name: log.tool_name.clone(),
                    success: log.success,
                    duration_ms: log.duration_ms,
                    summary: log.tool_output.get("summary")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                })
                .collect();

            // 尝试从主 DB 提取会话元数据（失败不影响主流程）
            let (reasoning_steps, cumulative_tokens, turn_count, workspace_files) =
                Self::extract_session_metadata(&refs.db, session_id);

            Some(crate::proactive::scenarios::types::SessionContext {
                tool_calls,
                reasoning_steps,
                cumulative_tokens,
                turn_count,
                workspace_files,
            })
        } else {
            None
        };

        // 预计算已有技能指纹（用于 skill_extraction 场景的前置去重）。
        // 查询成本低（≤50 行），且可避免不必要的大量重复技能生成，
        // ROI 远高于 token 增量（50条×100字符≈1,250 tokens）。
        let skill_fingerprints =
            Self::compute_skill_fingerprints_for_extraction(refs.memory_graph_store.as_ref(), &active_space_id, 50);

        // 从 Gene 候选池读取当前快照
        let (gene_count, gene_candidates_snapshot) = {
            let pool = refs.gene_candidate_pool.read().await;
            let count = pool.len();
            // Convert GeneCandidate to LearningCard snapshot for scenario context
            let cards: Vec<LearningCard> = pool.iter().map(|c| LearningCard {
                raw: c.content.clone(),
                card_type: LearningCardType::FailureLesson, // best-effort: candidates come from classified cards
                failure_signal: None,
                tool_name: None,
                strategy_hint: StrategyHint {
                    condition: None,
                    action: None,
                    reason: c.reasoning.clone(),
                },
                files_touched: vec![],
                session_id: c.session_id.clone().unwrap_or_default(),
                score: c.score.unwrap_or(0.0) as f32,
                timestamp: c.timestamp.timestamp_millis(),
            }).collect();
            (count, cards)
        };

        // TODO: Build existing_gene_fingerprints from GeneRepository when integrated
        let existing_gene_fingerprints = Vec::new();

        let scenario_ctx = ScenarioContext {
            recent_messages,
            execution_logs,
            pending_multimodal,
            last_trigger_at: last_trigger_map,
            tick_count,
            new_message_count,
            new_execution_count,
            has_failures,
            active_space_id,
            active_session_id,
            session_context,
            existing_skill_fingerprints: skill_fingerprints,
            gene_candidate_count: gene_count,
            gene_candidates: gene_candidates_snapshot,
            existing_gene_fingerprints,
        };

        // 2. 评估场景触发
        let triggered_scenarios = refs.scenario_manager.evaluate_all(&scenario_ctx).await;

        if triggered_scenarios.is_empty() {
            tracing::debug!("[ProactiveService] 无场景触发，返回 NO_MESSAGE");
            return Ok(NO_MESSAGE_MARKER.to_string());
        }

        tracing::info!(
            "[ProactiveService] {} 个场景触发: {:?}",
            triggered_scenarios.len(),
            triggered_scenarios.iter().map(|s| s.name()).collect::<Vec<_>>()
        );

        // 3. 对每个触发的场景构建上下文并调用 LLM
        let mut responses = Vec::new();

        for scenario in &triggered_scenarios {
            match scenario.build_context(&scenario_ctx).await {
                Ok(output) => {
                    // 增强 system prompt：通过五路混合检索引擎注入召回的记忆
                    let enhanced_system_prompt = {
                        // skill_extraction 场景使用更轻量的召回：
                        // 指纹已在 context_messages 中作为去重参考，
                        // 此处仅保留少量混合检索结果作为补充上下文。
                        let max_results = if scenario.name() == "skill_extraction" {
                            6  // 轻量：指纹已替代大部分召回
                        } else {
                            15  // 默认：五路融合结果
                        };

                        let recall_query = format!("{} {}", scenario.name(), scenario.description());
                        let hybrid_request = HybridSearchRequest {
                            query: recall_query,
                            space_id: scenario_ctx.active_space_id.clone(),
                            session_id: scenario_ctx.active_session_id.clone(),
                            time_range: None,
                            file_paths: None,
                            max_results,
                        };

                        let base_prompt = match refs.hybrid_search_engine.search(&hybrid_request, None).await {
                            Ok(result) => {
                                let memory_ctx = HybridSearchEngine::format_for_prompt(&result);
                                if memory_ctx.trim().is_empty() {
                                    output.system_prompt.clone()
                                } else {
                                    format!(
                                        "{}\n\n## Previously Learned Context\n{}",
                                        output.system_prompt,
                                        memory_ctx
                                    )
                                }
                            }
                            Err(e) => {
                                tracing::debug!(scenario = %scenario.name(), error = %e, "Hybrid recall for scenario failed, using base prompt");
                                output.system_prompt.clone()
                            }
                        };

                        // 注入当前时间作为权威时间上下文
                        let time_block = build_proactive_time_block();
                        format!("{}\n{}", base_prompt, time_block)
                    };

                    // 构建 LLM 消息
                    let mut messages = vec![];
                    messages.push(ChatMessage::system(&enhanced_system_prompt));

                    // 添加场景上下文消息
                    for (role, content) in &output.context_messages {
                        match role.as_str() {
                            "user" => messages.push(ChatMessage::user(content)),
                            "assistant" => messages.push(ChatMessage::assistant(content)),
                            _ => messages.push(ChatMessage::user(content)),
                        }
                    }

                    // 添加额外指令（如果有）
                    if let Some(ref instructions) = output.additional_instructions {
                        messages.push(ChatMessage::user(instructions));
                    }

                    // 调用 LLM
                    match Self::call_llm_for_scenario(refs, &messages).await {
                        Ok(llm_response) => {
                            // memU 记忆持久化
                            if llm_response.trim() != NO_MESSAGE_MARKER {
                                // skill_extraction 场景：解析 XML 并存储为 Procedure 节点
                                if scenario.name() == "skill_extraction" {
                                    let mut parsed_skills = crate::proactive::skill_parser::parse_skill_report(&llm_response);
                                    tracing::info!(
                                        parsed_count = parsed_skills.len(),
                                        skill_names = ?parsed_skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
                                        "[skill_extraction] parse_skill_report completed"
                                    );
                                    // Classify failure types from execution logs and stamp
                                    // each new skill with signals_seen (empirical counterpart
                                    // to the LLM-prescribed signals[]).
                                    let signals_seen = crate::proactive::scenarios::skill_extraction::extract_signals_seen(
                                        &scenario_ctx.execution_logs
                                    );
                                    for skill in &mut parsed_skills {
                                        skill.signals_seen = signals_seen.clone();
                                    }
                                    let items_extracted = if !parsed_skills.is_empty() {
                                        let space_id = scenario_ctx.active_space_id.clone();
                                        let mut stored_count = 0usize;
                                        for skill in &parsed_skills {
                                            match crate::proactive::skill_parser::store_skill_as_procedure(
                                                &refs.memory_graph_store, skill, &space_id
                                            ) {
                                                Ok(node) => {
                                                    tracing::info!(
                                                        skill_name = %skill.name,
                                                        node_id = %node.id,
                                                        "Stored learned skill as Procedure node"
                                                    );
                                                    stored_count += 1;
                                                    // Best-effort: embed the skill body and persist
                                                    // to memory_versions.embedding_json. Failure
                                                    // is logged but never aborts skill storage.
                                                    if refs.memu_client.is_some() {
                                                        let body = crate::proactive::skill_parser::build_version_content(skill);
                                                        // Construct the text to embed: body + signals so that semantic search
                                                        // can match queries against trigger phrases even if they're not in the body.
                                                        let mut embed_text = body.clone();
                                                        if !skill.signals.is_empty() {
                                                            embed_text.push_str("\n\nTrigger signals: ");
                                                            embed_text.push_str(&skill.signals.join(", "));
                                                        }
                                                        if !skill.signals_seen.is_empty() {
                                                            embed_text.push_str("\n\nObserved error types: ");
                                                            embed_text.push_str(&skill.signals_seen.join(", "));
                                                        }
                                                        let store = Arc::clone(&refs.memory_graph_store);
                                                        let memu = refs.memu_client.clone();
                                                        let node_id = node.id.clone();
                                                        tokio::spawn(async move {
                                                            if let Some(vec) = crate::memu::embedding::embed_skill_body(&memu, &embed_text).await {
                                                                let json = crate::memu::embedding::serialize_embedding(&vec);
                                                                if let Ok(Some(ver)) = store.get_active_version(&node_id) {
                                                                    if let Err(e) = store.update_version_embedding(&ver.id, &json) {
                                                                        tracing::warn!(node_id, error = %e, "failed to persist skill embedding");
                                                                    }
                                                                }
                                                            }
                                                        });
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::warn!(
                                                        skill_name = %skill.name,
                                                        error = %e,
                                                        "Failed to store skill as Procedure node"
                                                    );
                                                }
                                            }
                                        }
                                        stored_count
                                    } else {
                                        // 没有解析到技能，fallback 到 memorize_with_config
                                        if let Some(ref memu) = refs.memu_client {
                                            match memu.memorize_with_config(
                                                &llm_response,
                                                &["skill", "tool"],
                                                None,
                                                "proactive_skill_extraction",
                                            ).await {
                                                Ok(result) => result.items_extracted,
                                                Err(e) => {
                                                    tracing::warn!(
                                                        scenario = %scenario.name(),
                                                        error = %e,
                                                        "Proactive memory extraction failed (fallback)"
                                                    );
                                                    0
                                                }
                                            }
                                        } else {
                                            0
                                        }
                                    };

                                    // InfraService 事件
                                    refs.infra.publish_skill_learned(
                                        "proactive",
                                        scenario.name(),
                                        serde_json::json!({
                                            "scenario": scenario.name(),
                                            "items_extracted": items_extracted,
                                        }),
                                    ).await;

                                    // Tauri IPC 发射到前端
                                    let summary = extract_summary_text(&llm_response);
                                    let session_id = refs.last_active_session_id.read().await.clone();
                                    // Diagnostic: surface the sessionId we tag the
                                    // event with so we can correlate with the
                                    // frontend's filter when chips don't show.
                                    tracing::info!(
                                        items_extracted,
                                        session_id = ?session_id,
                                        scenario = "skill_extraction",
                                        "Emitting agent:proactive-learning IPC event"
                                    );
                                    if let Some(ref handle) = refs.app_handle {
                                        let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                            "scenario": "skill_extraction",
                                            "items_extracted": items_extracted,
                                            "categories": ["procedure"],
                                            "timestamp": chrono::Utc::now().to_rfc3339(),
                                            "summary": summary,
                                            "sessionId": session_id,
                                        }));
                                    }
                                } else if scenario.name() == "gene_evolution" {
                                    // GEP Gene Evolution: parse Gene XML from LLM output
                                    let distillation_outcome: Option<(Gene, Capsule, EvolutionEvent)> =
                                    match distillation::parse_gene_xml(&llm_response) {
                                        Ok(gene) => {
                                            // Validate gene
                                            if let Err(e) = distillation::validate_gene(&gene) {
                                                tracing::warn!("[gene_evolution] Gene validation failed: {}", e);
                                                None
                                            } else {
                                                // Check duplicates and store — keep lock scope minimal
                                                let mut repo = refs.gene_repo.lock().unwrap();
                                                let existing = repo.list_active_genes().unwrap_or_default();
                                                if let Some(dup_id) = distillation::check_duplicate(&gene, &existing) {
                                                    tracing::info!("[gene_evolution] Duplicate gene detected: {}", dup_id);
                                                    None
                                                } else {
                                                    let mut gene = gene;
                                                    match repo.store_gene(&mut gene) {
                                                        Ok(()) => {
                                                            // Generate initial Capsule
                                                            let capsule = Capsule {
                                                                id: format!("cap_init_{}", &gene.asset_id[..8]),
                                                                gene_asset_id: gene.asset_id.clone(),
                                                                gene_id: gene.gene_id.clone(),
                                                                trigger: gene.signals_match.clone(),
                                                                summary: format!("Initial capsule for gene {}", gene.gene_id),
                                                                confidence: 0.8,
                                                                blast_radius: BlastRadius { files: 0, lines: 0 },
                                                                outcome: CapsuleOutcome {
                                                                    status: OutcomeStatus::Success,
                                                                    score: 0.8,
                                                                },
                                                                raw_streak: 0,
                                                                effective_streak: 0.0,
                                                                env_fingerprint: EnvFingerprint::default(),
                                                                created_at: chrono::Utc::now().timestamp_millis(),
                                                                lineage: vec![],
                                                            };
                                                            let _ = repo.store_capsule(&capsule);

                                                            // Store EvolutionEvent
                                                            let event = EvolutionEvent {
                                                                intent: gene.category.to_string(),
                                                                capsule_id: capsule.id.clone(),
                                                                genes_used: vec![gene.asset_id.clone()],
                                                                mutations_tried: 0,
                                                                total_cycles: 1,
                                                                created_at: chrono::Utc::now().timestamp_millis(),
                                                            };
                                                            let _ = repo.store_event(&event);

                                                            tracing::info!(
                                                                gene_id = %gene.gene_id,
                                                                asset_id = %gene.asset_id,
                                                                "[gene_evolution] New gene distilled and stored"
                                                            );
                                                            Some((gene, capsule, event))
                                                        }
                                                        Err(e) => {
                                                            tracing::error!("[gene_evolution] Failed to store gene: {}", e);
                                                            None
                                                        }
                                                    }
                                                }
                                            } // MutexGuard dropped here
                                        }
                                        Err(e) => {
                                            tracing::warn!("[gene_evolution] Failed to parse Gene XML: {}", e);
                                            None
                                        }
                                    };

                                    // Clear consumed candidates (after MutexGuard is dropped)
                                    if distillation_outcome.is_some() {
                                        refs.gene_candidate_pool.write().await.clear();
                                        refs.new_gene_candidates.store(false, Ordering::SeqCst);
                                    }
                                } else {
                                    // 其他场景保持原有的 memorize_with_config 逻辑
                                    if let Some(ref memu) = refs.memu_client {
                                        let (memory_types, source_type) = match scenario.name() {
                                            "conversation_learning" => (
                                                vec!["profile", "behavior", "event", "knowledge"],
                                                "proactive_conversation_learning",
                                            ),
                                            "multimodal_context" => (
                                                vec!["multimodal", "knowledge"],
                                                "proactive_multimodal_context",
                                            ),
                                            _other => (
                                                vec!["knowledge"],
                                                "proactive_unknown",
                                            ),
                                        };

                                        match memu.memorize_with_config(
                                            &llm_response,
                                            &memory_types,
                                            None,
                                            source_type,
                                        ).await {
                                            Ok(result) => {
                                                tracing::info!(
                                                    scenario = %scenario.name(),
                                                    items = result.items_extracted,
                                                    categories = ?result.categories_updated,
                                                    "Proactive memory extraction complete"
                                                );

                                                // InfraService 事件
                                                refs.infra.publish_memory_extracted(
                                                    "proactive",
                                                    scenario.name(),
                                                    serde_json::json!({
                                                        "scenario": scenario.name(),
                                                        "items_extracted": result.items_extracted,
                                                    }),
                                                ).await;

                                                // Tauri IPC 发射到前端
                                                let summary = extract_summary_text(&llm_response);
                                                let scenario_key = match scenario.name() {
                                                    "conversation_learning" => "conversation_learning",
                                                    "multimodal_context" => "multimodal_context",
                                                    _ => "conversation_learning",
                                                };
                                                let session_id = refs.last_active_session_id.read().await.clone();
                                                tracing::info!(
                                                    items_extracted = result.items_extracted,
                                                    session_id = ?session_id,
                                                    scenario = scenario_key,
                                                    "Emitting agent:proactive-learning IPC event"
                                                );
                                                if let Some(ref handle) = refs.app_handle {
                                                    let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                                        "scenario": scenario_key,
                                                        "items_extracted": result.items_extracted,
                                                        "categories": result.categories_updated,
                                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                                        "summary": summary,
                                                        "sessionId": session_id,
                                                    }));
                                                }
                                            }
                                            Err(e) => {
                                                tracing::warn!(
                                                    scenario = %scenario.name(),
                                                    error = %e,
                                                    "Proactive memory extraction failed"
                                                );
                                            }
                                        }
                                    }
                                }

                                responses.push(llm_response);
                            }
                            // 标记场景已触发
                            refs.scenario_manager.mark_triggered(scenario.name()).await;
                        }
                        Err(e) => {
                            tracing::warn!(
                                "[ProactiveService] 场景 {} LLM 调用失败: {}",
                                scenario.name(),
                                e
                            );
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "[ProactiveService] 场景 {} build_context 失败: {}",
                        scenario.name(),
                        e
                    );
                }
            }
        }

        // 场景触发后重置计数器
        refs.new_message_count.store(0, Ordering::SeqCst);
        refs.new_execution_count.store(0, Ordering::SeqCst);

        // 如果 multimodal 场景被触发了，清空已处理的多模态队列
        if triggered_scenarios.iter().any(|s| s.name() == "multimodal_context") {
            refs.multimodal_queue.drain_all().await;
        }

        // 4. 合并响应
        if responses.is_empty() {
            Ok(NO_MESSAGE_MARKER.to_string())
        } else {
            Ok(responses.join("\n\n---\n\n"))
        }
    }

    /// 从主 DB（agent_messages / agent_turns / agent_sessions）提取会话元数据。
    ///
    /// 填充 SessionContext 中此前为空的字段：
    /// - reasoning_steps: 最近 5 条 agent_messages 中的 reasoning 文本
    /// - cumulative_tokens: agent_messages 表中 input/output tokens 累计
    /// - turn_count: agent_turns 表中该 session 的轮次总数
    /// - workspace_files: agent_sessions.attached_dirs 中的目录列表
    ///
    /// 所有 DB 错误均被吞没（返回默认空值），不影响主 tick 流程。
    fn extract_session_metadata(
        db: &Arc<Mutex<Connection>>,
        session_id: &str,
    ) -> (
        Vec<String>,
        Option<crate::proactive::scenarios::types::TokenUsage>,
        Option<usize>,
        Vec<String>,
    ) {
        let conn = match db.lock() {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("[ProactiveService] 获取 DB 锁失败: {}", e);
                return (vec![], None, None, vec![]);
            }
        };

        // 1. reasoning_steps: 最近 5 条有 reasoning 的 agent_messages
        let reasoning_steps: Vec<String> = {
            let mut stmt = match conn.prepare(
                "SELECT reasoning FROM agent_messages \
                 WHERE session_id = ?1 AND reasoning IS NOT NULL AND reasoning != '' \
                 ORDER BY created_at DESC LIMIT 5"
            ) {
                Ok(s) => s,
                Err(_) => return (vec![], None, None, vec![]),
            };
            let rows: Vec<String> = stmt
                .query_map(rusqlite::params![session_id], |row| row.get(0))
                .into_iter()
                .flat_map(|r| r.filter_map(|v| v.ok()))
                .collect();
            rows
        };

        // 2. cumulative_tokens: SUM input/output tokens from agent_messages
        let cumulative_tokens: Option<crate::proactive::scenarios::types::TokenUsage> = {
            conn.query_row(
                "SELECT COALESCE(SUM(input_tokens), 0), COALESCE(SUM(output_tokens), 0) \
                 FROM agent_messages WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| {
                    Ok(crate::proactive::scenarios::types::TokenUsage {
                        input_tokens: row.get::<_, i64>(0)? as u64,
                        output_tokens: row.get::<_, i64>(1)? as u64,
                    })
                },
            ).ok()
        };

        // 3. turn_count: COUNT from agent_turns
        let turn_count: Option<usize> = {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_turns WHERE session_id = ?1",
                rusqlite::params![session_id],
                |row| row.get::<_, i64>(0).map(|v| v as usize),
            ).ok()
        };

        // 4. workspace_files: attached_dirs JSON array from agent_sessions
        let workspace_files: Vec<String> = {
            let dirs_json: Option<String> = conn.query_row(
                "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
                rusqlite::params![session_id],
                |row| row.get(0),
            ).ok().flatten();

            dirs_json
                .and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
                .unwrap_or_default()
        };

        (reasoning_steps, cumulative_tokens, turn_count, workspace_files)
    }

    /// 为 skill_extraction 场景计算已有技能的紧凑指纹列表。
    ///
    /// 每条指纹格式: "title | description(≤60chars) | category | cited:N"
    /// 按 cited_count DESC 排序，取 top-N。用于注入到 LLM 上下文
    /// 供提取阶段前置去重——LLM 在生成新 `<skill>` 前可逐条对比。
    ///
    /// Token 预算：50 条 × ~100 字符 ≈ 1,250 tokens（含格式开销）。
    /// 与完整 SOP 注入（每条 300-800 字符）相比节省 60-80%。
    fn compute_skill_fingerprints_for_extraction(
        store: &MemoryGraphStore,
        space_id: &str,
        limit: usize,
    ) -> Vec<String> {
        let nodes = match store.list_top_learned_skills(space_id, limit) {
            Ok(nodes) => nodes,
            Err(e) => {
                tracing::warn!("Failed to list skills for fingerprints: {}", e);
                return vec![];
            }
        };

        nodes
            .into_iter()
            .map(|detail| {
                let node = &detail.node;
                let meta = node.metadata.as_ref();
                let desc = meta
                    .and_then(|m| m.get("description"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let desc_short: String = if desc.chars().count() > 60 {
                    format!("{}…", desc.chars().take(60).collect::<String>())
                } else {
                    desc.to_string()
                };
                let category = meta
                    .and_then(|m| m.get("category"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("-");
                let cited = meta
                    .and_then(|m| m.get("cited_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                format!(
                    "{} | {} | {} | cited:{}",
                    node.title, desc_short, category, cited
                )
            })
            .collect()
    }

    /// 调用 LLM 处理场景
    ///
    /// 从 ProviderService 动态获取当前活动的 LLM 配置，
    /// 创建 provider 并执行调用。
    async fn call_llm_for_scenario(
        refs: &ProactiveStateRefs,
        messages: &[ChatMessage],
    ) -> anyhow::Result<String> {
        // 从 ProviderService 获取当前活动的 LLM 配置
        let llm_config_tuple = refs.provider_service.get_active_llm_config().await;
        let (provider_id, model, api_key, base_url) = match llm_config_tuple {
            Some(cfg) => cfg,
            None => {
                tracing::debug!("[ProactiveService] LLM provider 未配置，返回 NO_MESSAGE");
                return Ok(NO_MESSAGE_MARKER.to_string());
            }
        };

        // 构建 LlmConfig 并创建 provider
        let llm_config = crate::llm::llm_config_from_provider(
            &provider_id,
            &model,
            &api_key,
            &base_url,
            4096,  // 主动服务使用较小的 max_tokens
            0.7,
        );
        let provider = crate::llm::create_provider(&llm_config)
            .map_err(|e| anyhow::anyhow!("LLM provider 创建失败: {}", e))?;

        let config = CompletionConfig {
            model: model.clone(),
            max_tokens: 4096,
            temperature: 0.7,
            thinking_enabled: false,
        };

        // 调用 LLM（不使用工具）
        let output = provider
            .complete(messages.to_vec(), vec![], &config)
            .await
            .map_err(|e| anyhow::anyhow!("LLM 调用失败: {}", e))?;

        // 提取文本响应
        match output {
            crate::agent::types::RespondOutput::Text { text, .. } => Ok(text),
            crate::agent::types::RespondOutput::ToolCalls { text, .. } => {
                // 主动服务当前不处理工具调用，取文本部分
                Ok(text.unwrap_or_else(|| NO_MESSAGE_MARKER.to_string()))
            }
        }
    }

    /// 设置用户确认响应
    ///
    /// 当 agent 使用 `wait_user_confirm` 工具时，状态切换为 WaitingUserInput，
    /// 前端收集用户输入后通过此方法回传。
    pub async fn set_user_input(&self, input: String) {
        *self.user_input_response.write().await = Some(input);
        self.is_waiting_user_input.store(false, Ordering::SeqCst);
        tracing::info!("[ProactiveService] 收到用户确认响应");
    }

    // ─── Accessor Methods ────────────────────────────────────────────────

    pub fn failure_memory(&self) -> &Arc<FailureMemoryManager> { &self.failure_memory_manager }
    pub fn preference_extractor(&self) -> &Arc<PreferenceExtractor> { &self.preference_extractor }
    pub fn personality_model(&self) -> &Arc<PersonalityModel> { &self.personality_model }
    pub fn proactive_recall(&self) -> &Arc<ProactiveRecallService> { &self.proactive_recall_service }

    /// 获取当前主动服务状态报告
    pub async fn get_proactive_status(&self) -> ProactiveStatus {
        let state = *self.state.read().await;
        let windows = self.context_messages.read().await;
        let context_count: usize = windows.values().map(|w| w.messages.len()).sum();
        drop(windows);

        ProactiveStatus {
            state,
            is_running: self.is_running.load(Ordering::SeqCst),
            tick_count: self.tick_count.load(Ordering::SeqCst),
            action_count: self.action_count.load(Ordering::SeqCst),
            no_message_count: self.no_message_count.load(Ordering::SeqCst),
            last_tick_at: self.last_tick_at.read().await.clone(),
            last_action_at: self.last_action_at.read().await.clone(),
            context_message_count: context_count,
        }
    }
}

// ─── cited_count 周期性衰减 ───────────────────────────────────────────

/// 对所有已学习技能的 `cited_count` 应用 5% 衰减（floor(prev * 0.95)）。
///
/// 返回实际执行了更新的节点数。`cited_count` 已为 0 的节点跳过，
/// 不产生写操作。
pub fn decay_cited_counts(
    store: &MemoryGraphStore,
    space_id: &str,
) -> Result<usize, crate::error::Error> {
    let nodes = store.list_top_learned_skills(space_id, 10_000)?;
    let mut updated = 0;
    for detail in nodes {
        let mut meta = detail.node.metadata.clone().unwrap_or(serde_json::json!({}));
        let prev = meta.get("cited_count").and_then(|v| v.as_u64()).unwrap_or(0);
        let next = ((prev as f64) * 0.95).floor() as u64;
        if next == prev {
            continue; // no change (includes cited_count == 0 case)
        }
        if let Some(obj) = meta.as_object_mut() {
            obj.insert(
                "cited_count".to_string(),
                serde_json::Value::Number(serde_json::Number::from(next)),
            );
        }
        if store.update_node(&detail.node.id, None, None, Some(&meta)).is_ok() {
            updated += 1;
        }
    }
    tracing::info!(updated, "cited_count decay tick complete");
    Ok(updated)
}

// ─── ManagedService 实现 ──────────────────────────────────────────────

#[async_trait]
impl ManagedService for ProactiveService {
    /// 服务名称
    fn name(&self) -> &str {
        "proactive"
    }

    /// 启动主动服务
    ///
    /// 如果配置中 `enabled = false`，跳过启动。
    /// 否则启动上下文监听和轮询循环两个后台任务。
    async fn start(&self) -> anyhow::Result<()> {
        if !self.config.enabled {
            tracing::info!("[ProactiveService] 主动服务已禁用，跳过启动");
            return Ok(());
        }

        tracing::info!(
            "[ProactiveService] 启动主动服务，轮询间隔: {}ms，最大迭代: {}",
            self.config.interval_ms,
            self.config.max_iterations
        );

        // 设置运行标记
        self.is_running.store(true, Ordering::SeqCst);

        // 启动上下文监听
        self.start_context_listener().await;

        // 启动轮询循环
        self.start_tick_loop().await;

        // 启动 cited_count 周衰减任务（独立于主 tick loop，每 7 天触发一次）
        {
            let store = Arc::clone(&self.memory_graph_store);
            tokio::spawn(async move {
                const ONE_WEEK: std::time::Duration =
                    std::time::Duration::from_secs(7 * 24 * 60 * 60);
                let mut ticker = tokio::time::interval(ONE_WEEK);
                ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // Skip the immediate t=0 fire — first decay runs after one full week
                ticker.tick().await;
                loop {
                    ticker.tick().await;
                    if let Err(e) = decay_cited_counts(&store, "default") {
                        tracing::warn!(err = %e, "decay_cited_counts failed");
                    }
                }
            });
        }

        // One-shot backfill: embed legacy skill versions that have NULL embedding_json.
        // Runs at idle priority (spawned independently so it never blocks the tick loop).
        if self.memu_client.is_some() {
            let store = Arc::clone(&self.memory_graph_store);
            let memu = self.memu_client.clone();
            tokio::spawn(async move {
                // Short initial delay so the bridge is fully ready.
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                // TODO(multi-space): backfill currently iterates only the "default" space.
                // Once multi-space skill storage ships, iterate all spaces here so legacy
                // embeddings get filled across the board. For v1 (single "default" space)
                // this is correct.
                match store.list_versions_without_embedding("default", 500) {
                    Ok(pairs) if !pairs.is_empty() => {
                        tracing::info!(count = pairs.len(), "embedding backfill: starting");
                        let mut filled = 0usize;
                        for (version_id, content) in &pairs {
                            if let Some(vec) = crate::memu::embedding::embed_skill_body(&memu, content).await {
                                let json = crate::memu::embedding::serialize_embedding(&vec);
                                if let Err(e) = store.update_version_embedding(version_id, &json) {
                                    tracing::warn!(version_id, error = %e, "embedding backfill: write failed");
                                } else {
                                    filled += 1;
                                }
                            }
                            // Yield between calls so we don't saturate the bridge.
                            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                        }
                        tracing::info!(filled, total = pairs.len(), "embedding backfill: complete");
                    }
                    Ok(_) => {
                        tracing::debug!("embedding backfill: no versions need embedding");
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "embedding backfill: failed to list versions");
                    }
                }
            });
        }

        // 切换状态为 Idle（等待首次 tick）
        *self.state.write().await = ProactiveState::Idle;

        tracing::info!("[ProactiveService] 主动服务启动完成");
        Ok(())
    }

    /// 停止主动服务
    ///
    /// 设置 is_running = false 并 abort 所有后台任务，
    /// 确保优雅关闭。
    async fn stop(&self) -> anyhow::Result<()> {
        tracing::info!("[ProactiveService] 正在停止主动服务...");

        // 清除运行标记（后台任务会自行检测并退出）
        self.is_running.store(false, Ordering::SeqCst);

        // 切换状态为 Stopped
        *self.state.write().await = ProactiveState::Stopped;

        // 中止轮询循环任务
        if let Some(h) = self.tick_handle.write().await.take() {
            h.abort();
            tracing::debug!("[ProactiveService] 轮询循环任务已中止");
        }

        // 中止上下文监听任务
        if let Some(h) = self.listener_handle.write().await.take() {
            h.abort();
            tracing::debug!("[ProactiveService] 上下文监听任务已中止");
        }

        tracing::info!("[ProactiveService] 主动服务已停止");
        Ok(())
    }

    /// 获取当前服务状态
    fn status(&self) -> ServiceStatus {
        if self.is_running.load(Ordering::SeqCst) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    /// 获取完整健康信息
    fn health(&self) -> ServiceHealth {
        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs: None, // TODO: 在集成阶段添加启动时间追踪
            last_error: None,
            metrics: serde_json::json!({
                "tick_count": self.tick_count.load(Ordering::SeqCst),
                "action_count": self.action_count.load(Ordering::SeqCst),
                "no_message_count": self.no_message_count.load(Ordering::SeqCst),
                "is_waiting_user_input": self.is_waiting_user_input.load(Ordering::SeqCst),
            }),
        }
    }
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    use crate::memory_graph::store::MemoryGraphStore;

    /// 创建测试用的 ProactiveStorage（内存数据库）
    fn make_test_storage() -> Arc<ProactiveStorage> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS proactive_messages (
                id TEXT PRIMARY KEY,
                content TEXT NOT NULL,
                generated_at TEXT NOT NULL,
                trigger_reason TEXT NOT NULL DEFAULT '',
                tools_used TEXT NOT NULL DEFAULT '[]',
                created_at TEXT DEFAULT CURRENT_TIMESTAMP
            );",
        )
        .unwrap();
        Arc::new(ProactiveStorage {
            conn: Mutex::new(conn),
        })
    }

    /// 创建测试用的 ProviderService
    fn make_test_provider_service() -> Arc<ProviderService> {
        // 使用临时目录创建 ProviderService
        let tmp = tempfile::tempdir().unwrap();
        Arc::new(ProviderService::new(tmp.path()).unwrap())
    }

    /// 创建测试用的 MemoryGraphStore（内存数据库）
    fn make_test_memory_graph_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = Arc::new(std::sync::Mutex::new(conn));
        Arc::new(MemoryGraphStore::new(conn))
    }

    /// 创建完整的测试服务实例
    fn make_test_service(config: ProactiveConfig, infra: Arc<InfraService>) -> ProactiveService {
        let storage = make_test_storage();
        let scenario_manager = Arc::new(ScenarioManager::new());
        let execution_log_collector = Arc::new(ExecutionLogCollector::new());
        let multimodal_queue = Arc::new(MultimodalQueue::new());
        let provider_service = make_test_provider_service();
        let memory_graph_store = make_test_memory_graph_store();
        ProactiveService::new(
            config,
            infra,
            storage,
            scenario_manager,
            execution_log_collector,
            multimodal_queue,
            provider_service,
            None, // memu_client
            memory_graph_store,
            None, // app_handle — 测试环境不需要 Tauri IPC
            Arc::new(std::sync::Mutex::new(Connection::open_in_memory().unwrap())),
            Arc::new(std::sync::Mutex::new(
                crate::agent::gep::repository::GeneRepository::new(
                    std::env::temp_dir().join("uclaw_gep_test")
                ).expect("GeneRepository for test")
            )),
        )
    }

    /// 测试：创建服务并获取初始状态
    #[tokio::test]
    async fn test_new_service_initial_state() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);
        let status = service.get_proactive_status().await;

        assert_eq!(status.state, ProactiveState::Stopped);
        assert!(!status.is_running);
        assert_eq!(status.tick_count, 0);
        assert_eq!(status.action_count, 0);
        assert_eq!(status.context_message_count, 0);
    }

    /// 测试：disabled 时 start 不会启动后台任务
    #[tokio::test]
    async fn test_start_when_disabled() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: false,
            ..Default::default()
        };
        let service = make_test_service(config, infra);
        service.start().await.unwrap();

        // 应该仍然是 Stopped 状态（因为 disabled 直接跳过）
        assert!(!service.is_running.load(Ordering::SeqCst));
    }

    /// 测试：start + stop 生命周期
    #[tokio::test]
    async fn test_start_and_stop_lifecycle() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: true,
            interval_ms: 100, // 短间隔便于测试
            ..Default::default()
        };
        let service = make_test_service(config, infra);

        // 启动
        service.start().await.unwrap();
        assert!(service.is_running.load(Ordering::SeqCst));
        assert_eq!(*service.state.read().await, ProactiveState::Idle);

        // 等待一会儿让 tick 运行
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 停止
        service.stop().await.unwrap();
        assert!(!service.is_running.load(Ordering::SeqCst));
        assert_eq!(*service.state.read().await, ProactiveState::Stopped);
    }

    /// 测试：ManagedService trait 实现
    #[tokio::test]
    async fn test_managed_service_trait() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);

        assert_eq!(service.name(), "proactive");
        assert_eq!(service.status(), ServiceStatus::Stopped);

        let health = service.health();
        assert_eq!(health.name, "proactive");
        assert_eq!(health.status, ServiceStatus::Stopped);
    }

    /// 测试：上下文监听接收消息
    #[tokio::test]
    async fn test_context_listener_receives_messages() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig {
            enabled: true,
            interval_ms: 10_000, // 长间隔避免 tick 干扰
            ..Default::default()
        };
        let service = make_test_service(config, infra.clone());
        service.start().await.unwrap();

        // 等待监听任务启动
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 通过消息总线发送消息
        infra
            .publish_incoming("test", "你好", serde_json::json!({}))
            .await;

        // 等待消息被处理
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // 验证上下文窗口（测试用 metadata 不含 conversation_id，消息进入 "default" session）
        let ctx = service.context_messages.read().await;
        let default_window = ctx.get("default").expect("default session window should exist");
        assert_eq!(default_window.messages.len(), 1);
        assert_eq!(default_window.messages[0].content, "你好");

        // has_new_context 应该为 true
        assert!(service.has_new_context.load(Ordering::SeqCst));

        // new_message_count 应该递增
        assert_eq!(service.new_message_count.load(Ordering::SeqCst), 1);

        service.stop().await.unwrap();
    }

    /// 测试：set_user_input
    #[tokio::test]
    async fn test_set_user_input() {
        let infra = Arc::new(InfraService::new());
        let config = ProactiveConfig::default();
        let service = make_test_service(config, infra);

        // 模拟等待状态
        service
            .is_waiting_user_input
            .store(true, Ordering::SeqCst);

        service.set_user_input("确认执行".to_string()).await;

        assert!(!service.is_waiting_user_input.load(Ordering::SeqCst));
        let resp = service.user_input_response.read().await;
        assert_eq!(resp.as_deref(), Some("确认执行"));
    }

    /// 辅助：向 store 插入一个带有 cited_count 的 Procedure 节点，返回 node id
    fn insert_learned_skill(
        store: &MemoryGraphStore,
        space_id: &str,
        name: &str,
        cited_count: u64,
    ) -> String {
        use crate::memory_graph::models::{MemoryNode, MemoryNodeKind};
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let meta = serde_json::json!({
            "skill_type": "learned",
            "cited_count": cited_count,
            "enabled": true,
        });
        let node = MemoryNode {
            id: id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: name.to_string(),
            metadata: Some(meta),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).expect("insert_learned_skill: create_node failed");
        id
    }

    /// 辅助：读取节点当前的 cited_count
    fn read_cited_count(store: &MemoryGraphStore, node_id: &str) -> u64 {
        let node = store
            .get_node(node_id)
            .expect("read_cited_count: get_node failed")
            .expect("read_cited_count: node not found");
        node.metadata
            .as_ref()
            .and_then(|m| m.get("cited_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0)
    }

    /// Test: cited=20 → floor(20 * 0.95) = 19 after one decay call
    #[test]
    fn decay_applies_to_learned_skills() {
        let store = make_test_memory_graph_store();
        let node_id = insert_learned_skill(&store, "default", "my_skill", 20);

        let updated = decay_cited_counts(&store, "default").expect("decay failed");
        assert_eq!(updated, 1, "expected 1 node updated");
        assert_eq!(read_cited_count(&store, &node_id), 19, "expected cited_count=19");
    }

    /// Test: cited=0 → no-op (floor(0 * 0.95) == 0, skip write)
    #[test]
    fn decay_floors_at_zero_not_negative() {
        let store = make_test_memory_graph_store();
        let node_id = insert_learned_skill(&store, "default", "zero_skill", 0);

        let updated = decay_cited_counts(&store, "default").expect("decay failed");
        assert_eq!(updated, 0, "expected 0 nodes updated (no-op)");
        assert_eq!(read_cited_count(&store, &node_id), 0, "cited_count should remain 0");
    }
}
