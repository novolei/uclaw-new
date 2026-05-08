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

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;

use crate::agent::types::ChatMessage;
use crate::infra::{ConversationMessage, InfraEventType, InfraService};
use crate::llm::provider::CompletionConfig;
use tauri::Emitter;
use crate::memu::client::MemUClient;
use crate::memubot_config::ProactiveConfig;
use crate::memory_graph::store::MemoryGraphStore;
use crate::providers::service::ProviderService;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::execution_log::ExecutionLogCollector;
use super::multimodal::MultimodalQueue;
use super::scenarios::{ScenarioContext, ScenarioManager};
use super::storage::ProactiveStorage;
use super::types::*;

/// 上下文滑动窗口最大容量（保留最近 N 条消息）
const CONTEXT_WINDOW_SIZE: usize = 20;

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
    /// 上下文消息滑动窗口
    context_messages: Arc<RwLock<VecDeque<ConversationMessage>>>,
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

    /// 上下文消息滑动窗口（最近 CONTEXT_WINDOW_SIZE 条）
    context_messages: Arc<RwLock<VecDeque<ConversationMessage>>>,
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
    ) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(ProactiveState::Stopped)),
            is_running: Arc::new(AtomicBool::new(false)),
            context_messages: Arc::new(RwLock::new(VecDeque::with_capacity(
                CONTEXT_WINDOW_SIZE,
            ))),
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
        }
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

        let handle = tokio::spawn(async move {
            while is_running.load(Ordering::SeqCst) {
                match rx.recv().await {
                    Ok(event) => {
                        match event.event_type {
                            // 用户消息和 Bot 回复 → 维护上下文滑动窗口
                            InfraEventType::MessageIncoming | InfraEventType::MessageOutgoing => {
                                let mut ctx = context.write().await;
                                ctx.push_back(event.message);
                                // 超出窗口大小时，淘汰最旧消息
                                if ctx.len() > CONTEXT_WINDOW_SIZE {
                                    ctx.pop_front();
                                }
                                has_new.store(true, Ordering::SeqCst);
                                new_msg_count.fetch_add(1, Ordering::SeqCst);
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
                                    tool_name,
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
        // 1. 构建 ScenarioContext
        let context_messages = refs.context_messages.read().await;
        let recent_messages: Vec<_> = context_messages.iter().cloned().collect();
        drop(context_messages);

        let execution_logs = refs.execution_log_collector.recent(50).await;
        let pending_multimodal = refs.multimodal_queue.peek_all().await;
        let last_trigger_map = refs.scenario_manager.get_last_trigger_map().await;
        let tick_count = refs.tick_count.load(Ordering::SeqCst);
        let new_message_count = refs.new_message_count.load(Ordering::SeqCst);
        let new_execution_count = refs.new_execution_count.load(Ordering::SeqCst);

        // 检查最近是否有失败
        let recent_failures = refs.execution_log_collector.failures(1).await;
        let has_failures = !recent_failures.is_empty();

        let active_space_id = refs.active_space_id.read().await.clone();

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
                    // 增强 system prompt：注入召回的记忆
                    let enhanced_system_prompt = {
                        let recall_engine = crate::memory_graph::recall::MemoryRecallEngine::new(
                            refs.memory_graph_store.clone(),
                            refs.memu_client.clone(),
                            crate::memory_graph::recall::MemoryRecallConfig::default(),
                        );

                        // 用场景描述构建召回查询
                        let recall_query = format!("{} {}", scenario.name(), scenario.description());
                        match recall_engine.build_recall_plan(&scenario_ctx.active_space_id, &recall_query, false).await {
                            Ok(plan) => {
                                let memory_ctx = crate::memory_graph::recall::MemoryRecallEngine::format_recall_for_prompt(&plan);
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
                                tracing::debug!(scenario = %scenario.name(), error = %e, "Recall for scenario failed, using base prompt");
                                output.system_prompt.clone()
                            }
                        }
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
                                    let parsed_skills = crate::proactive::skill_parser::parse_skill_report(&llm_response);
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
                                    let summary: String = llm_response.chars().take(200).collect();
                                    if let Some(ref handle) = refs.app_handle {
                                        let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                            "scenario": "skill_extraction",
                                            "items_extracted": items_extracted,
                                            "categories": ["procedure"],
                                            "timestamp": chrono::Utc::now().to_rfc3339(),
                                            "summary": summary,
                                        }));
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
                                                let summary: String = llm_response.chars().take(200).collect();
                                                let scenario_key = match scenario.name() {
                                                    "conversation_learning" => "conversation_learning",
                                                    "multimodal_context" => "multimodal_context",
                                                    _ => "conversation_learning",
                                                };
                                                if let Some(ref handle) = refs.app_handle {
                                                    let _ = handle.emit("agent:proactive-learning", serde_json::json!({
                                                        "scenario": scenario_key,
                                                        "items_extracted": result.items_extracted,
                                                        "categories": result.categories_updated,
                                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                                        "summary": summary,
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
            system_prompt: None, // system prompt 已包含在 messages 中
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

    /// 获取当前主动服务状态报告
    pub async fn get_proactive_status(&self) -> ProactiveStatus {
        let state = *self.state.read().await;
        let context_count = self.context_messages.read().await.len();

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

        // 验证上下文窗口
        let ctx = service.context_messages.read().await;
        assert_eq!(ctx.len(), 1);
        assert_eq!(ctx[0].content, "你好");

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
}
