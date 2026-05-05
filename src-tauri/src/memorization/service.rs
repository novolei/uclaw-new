//! MemorizationService — 后台记忆提取服务
//!
//! 替代当前每轮同步 reflection 的模式，改为：
//! 1. 订阅 InfraService 的消息事件
//! 2. 将消息持久化到 SQLite 队列
//! 3. 当消息数达到阈值（默认 20 条）时立即触发记忆提取
//! 4. 当消息数 >= min_messages（默认 2 条）时启动防抖定时器（默认 60 分钟）
//! 5. 调用 memU memorize API 进行语义提取
//!
//! ## 设计要点
//! - 实现 `ManagedService` trait，由 `ServiceManager` 统一管理生命周期
//! - 使用独立 SQLite 数据库 `~/.uclaw/memorization.db`
//! - memU 不可用时降级为日志记录模式
//! - 进程重启后可恢复未完成的提取任务

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::infra::{InfraEventType, InfraService};
use crate::memubot_config::MemorizationConfig;
use crate::memu::client::MemUClient;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::storage::MemorizationStorage;
use super::types::*;

/// 待处理任务超时时间（毫秒）— 超过此时间的任务视为过期
const PENDING_TASK_TIMEOUT_MS: i64 = 30 * 60 * 1000; // 30 分钟

/// 后台记忆提取服务
///
/// 持续收集来自 InfraService 的对话消息，
/// 当消息积累达到阈值或防抖超时时，批量发送给 memU 进行语义提取。
pub struct MemorizationService {
    /// 记忆提取配置（阈值、防抖时间等）
    config: MemorizationConfig,
    /// 当前状态机阶段
    state: Arc<RwLock<MemorizationState>>,
    /// 持久化存储（SQLite 队列 + 任务状态）
    storage: Arc<MemorizationStorage>,
    /// 中央消息总线引用
    infra: Arc<InfraService>,
    /// memU 客户端（可选，不可用时降级）
    memu_client: Arc<RwLock<Option<Arc<MemUClient>>>>,
    /// 是否正在运行
    is_running: Arc<AtomicBool>,
    /// 历史总记忆提取次数
    total_memorized: Arc<AtomicU64>,
    /// 上次记忆提取完成时间
    last_memorization_at: Arc<RwLock<Option<String>>>,
    /// 防抖定时器的取消信号发送端
    debounce_cancel: Arc<RwLock<Option<tokio::sync::oneshot::Sender<()>>>>,
    /// 后台监听任务 handle
    listener_handle: Arc<RwLock<Option<JoinHandle<()>>>>,
    /// 服务启动时间（用于计算 uptime）
    started_at: Arc<RwLock<Option<Instant>>>,
    /// 最近一次错误信息
    last_error: Arc<RwLock<Option<String>>>,
}

impl MemorizationService {
    /// 创建新的 MemorizationService 实例
    ///
    /// # Arguments
    /// * `config` - 记忆提取配置
    /// * `storage` - 持久化存储
    /// * `infra` - 中央消息总线
    pub fn new(
        config: MemorizationConfig,
        storage: Arc<MemorizationStorage>,
        infra: Arc<InfraService>,
    ) -> Self {
        Self {
            config,
            state: Arc::new(RwLock::new(MemorizationState::Stopped)),
            storage,
            infra,
            memu_client: Arc::new(RwLock::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
            total_memorized: Arc::new(AtomicU64::new(0)),
            last_memorization_at: Arc::new(RwLock::new(None)),
            debounce_cancel: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            started_at: Arc::new(RwLock::new(None)),
            last_error: Arc::new(RwLock::new(None)),
        }
    }

    /// 设置 memU 客户端引用
    ///
    /// 可在服务启动后动态注入，支持 memU 延迟初始化。
    /// 设为 None 时服务进入降级模式（仅记录日志）。
    pub async fn set_memu_client(&self, client: Option<Arc<MemUClient>>) {
        let mut guard = self.memu_client.write().await;
        *guard = client;
    }

    /// 触发记忆提取（从实例方法调用）
    async fn trigger_memorization(&self) {
        Self::do_memorization(
            self.state.clone(),
            self.storage.clone(),
            self.memu_client.clone(),
            self.total_memorized.clone(),
            self.last_memorization_at.clone(),
            self.last_error.clone(),
        )
        .await;
    }

    /// 执行记忆提取的核心逻辑（静态方法，可从定时器闭包中调用）
    ///
    /// 流程：
    /// 1. 设置状态为 Memorizing
    /// 2. 从队列获取所有消息
    /// 3. 保存任务状态（用于崩溃恢复）
    /// 4. 格式化为对话格式并调用 memU memorize API
    /// 5. 成功后清除队列和任务状态
    /// 6. 更新统计信息
    /// 7. 恢复状态为 Listening
    async fn do_memorization(
        state: Arc<RwLock<MemorizationState>>,
        storage: Arc<MemorizationStorage>,
        memu_client: Arc<RwLock<Option<Arc<MemUClient>>>>,
        total_memorized: Arc<AtomicU64>,
        last_memorization_at: Arc<RwLock<Option<String>>>,
        last_error: Arc<RwLock<Option<String>>>,
    ) {
        // 检查是否已经在提取中（避免重复触发）
        {
            let current = state.read().await;
            if *current == MemorizationState::Memorizing {
                info!("[MemorizationService] 已有提取任务正在执行，跳过");
                return;
            }
        }

        // 1. 切换状态为 Memorizing
        {
            let mut s = state.write().await;
            *s = MemorizationState::Memorizing;
        }

        // 2. 从队列获取所有消息
        let messages = match storage.get_queue() {
            Ok(msgs) => msgs,
            Err(e) => {
                error!("[MemorizationService] 获取队列失败: {}", e);
                let mut s = state.write().await;
                *s = MemorizationState::Listening;
                let mut err = last_error.write().await;
                *err = Some(format!("获取队列失败: {}", e));
                return;
            }
        };

        if messages.is_empty() {
            let mut s = state.write().await;
            *s = MemorizationState::Listening;
            return;
        }

        let message_count = messages.len();
        let task_id = uuid::Uuid::new_v4().to_string();

        // 3. 保存任务状态（用于崩溃恢复）
        if let Err(e) = storage.save_task_state(&task_id, message_count) {
            warn!("[MemorizationService] 保存任务状态失败: {}", e);
        }

        // 4. 格式化为对话格式
        let conversation_text = messages
            .iter()
            .map(|m| format!("[{}] {}: {}", m.platform, m.role, m.content))
            .collect::<Vec<_>>()
            .join("\n");

        info!(
            "[MemorizationService] 触发记忆提取，任务 {}，消息数: {}",
            task_id, message_count
        );

        // 5. 调用 memU memorize API（如果可用）
        let memu = memu_client.read().await;
        match memu.as_ref() {
            Some(client) => {
                match client.memorize(&conversation_text, "conversation", None).await {
                    Ok(result) => {
                        info!(
                            "[MemorizationService] memU 提取完成: {} 个记忆项",
                            result.items.len()
                        );
                        // 提取成功，清除队列
                        if let Err(e) = storage.clear_queue(message_count) {
                            error!("[MemorizationService] 清除队列失败: {}", e);
                        }
                    }
                    Err(e) => {
                        warn!(
                            "[MemorizationService] memU memorize 调用失败: {}，消息保留在队列中",
                            e
                        );
                        let mut err = last_error.write().await;
                        *err = Some(format!("memU memorize 失败: {}", e));
                        // 失败时不清除队列，等待下次重试
                        if let Err(e) = storage.clear_task_state() {
                            warn!("[MemorizationService] 清除任务状态失败: {}", e);
                        }
                        let mut s = state.write().await;
                        *s = MemorizationState::Listening;
                        return;
                    }
                }
            }
            None => {
                // memU 不可用 — 降级模式，仅记录日志
                info!(
                    "[MemorizationService] memU 不可用（降级模式），记录 {} 条消息后清除队列",
                    message_count
                );
                // 降级模式下仍然清除队列，避免无限积累
                if let Err(e) = storage.clear_queue(message_count) {
                    error!("[MemorizationService] 清除队列失败: {}", e);
                }
            }
        }

        // 6. 清除任务状态
        if let Err(e) = storage.clear_task_state() {
            warn!("[MemorizationService] 清除任务状态失败: {}", e);
        }

        // 7. 更新统计信息
        total_memorized.fetch_add(1, Ordering::Relaxed);
        {
            let mut last_at = last_memorization_at.write().await;
            *last_at = Some(chrono::Utc::now().to_rfc3339());
        }
        {
            let mut err = last_error.write().await;
            *err = None; // 清除之前的错误
        }

        // 8. 恢复状态为 Listening
        {
            let mut s = state.write().await;
            *s = MemorizationState::Listening;
        }

        info!(
            "[MemorizationService] 记忆提取完成，任务 {}",
            task_id
        );
    }

    /// 恢复上次未完成的任务
    ///
    /// 在服务启动时检查是否有未完成的提取任务。
    /// 如果任务超时（> 30 分钟），清除任务状态；否则重新触发提取。
    async fn recover_pending_task(&self) {
        {
            let mut s = self.state.write().await;
            *s = MemorizationState::Recovering;
        }

        if let Ok(Some(task)) = self.storage.get_pending_task() {
            let now = chrono::Utc::now().timestamp_millis();
            let elapsed = now - task.started_at;

            if elapsed > PENDING_TASK_TIMEOUT_MS {
                info!(
                    "[MemorizationService] 待处理任务 {} 已超时（{}ms），清除状态",
                    task.task_id, elapsed
                );
                self.storage.clear_task_state().ok();
            } else {
                info!(
                    "[MemorizationService] 恢复待处理任务: {}，消息数: {}",
                    task.task_id, task.message_count
                );
                // 重新触发提取
                self.trigger_memorization().await;
            }
        }

        {
            let mut s = self.state.write().await;
            *s = MemorizationState::Listening;
        }
    }

    /// 获取当前状态报告
    pub async fn get_status(&self) -> MemorizationStatus {
        let state = *self.state.read().await;
        let queue_count = self.storage.get_count().unwrap_or(0);
        let pending_task = self.storage.get_pending_task().unwrap_or(None);
        let total_memorized = self.total_memorized.load(Ordering::Relaxed);
        let last_memorization_at = self.last_memorization_at.read().await.clone();

        MemorizationStatus {
            state,
            queue_count,
            pending_task,
            total_memorized,
            last_memorization_at,
        }
    }
}

/// 实现 ManagedService trait，由 ServiceManager 统一管理生命周期
#[async_trait]
impl ManagedService for MemorizationService {
    fn name(&self) -> &str {
        "memorization"
    }

    async fn start(&self) -> anyhow::Result<()> {
        // 检查是否启用
        if !self.config.enabled {
            info!("[MemorizationService] 记忆提取已禁用，跳过启动");
            return Ok(());
        }

        // 检查是否已在运行
        if self.is_running.load(Ordering::Relaxed) {
            warn!("[MemorizationService] 服务已在运行中");
            return Ok(());
        }

        info!("[MemorizationService] 启动记忆提取服务...");

        // 1. 恢复上次未完成的任务
        self.recover_pending_task().await;

        // 2. 标记为运行中
        self.is_running.store(true, Ordering::Relaxed);
        {
            let mut s = self.state.write().await;
            *s = MemorizationState::Listening;
        }
        {
            let mut sa = self.started_at.write().await;
            *sa = Some(Instant::now());
        }

        // 3. 订阅 InfraService 事件并启动后台监听循环
        let mut rx = self.infra.subscribe();
        let is_running = self.is_running.clone();
        let state = self.state.clone();
        let storage = self.storage.clone();
        let memu_client = self.memu_client.clone();
        let total_memorized = self.total_memorized.clone();
        let last_memorization_at = self.last_memorization_at.clone();
        let last_error = self.last_error.clone();
        let config = self.config.clone();
        let debounce_cancel = self.debounce_cancel.clone();

        let handle = tokio::spawn(async move {
            info!("[MemorizationService] 后台监听循环已启动");

            while is_running.load(Ordering::Relaxed) {
                match rx.recv().await {
                    Ok(event) => {
                        // 只处理消息到达和消息发出事件
                        match event.event_type {
                            InfraEventType::MessageIncoming
                            | InfraEventType::MessageOutgoing => {}
                            _ => continue,
                        }

                        // 从事件元数据中提取 conversation_id 和 space_id
                        let conversation_id = event
                            .metadata
                            .get("conversation_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());
                        let space_id = event
                            .metadata
                            .get("space_id")
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string());

                        let msg = UnmemorizedMessage {
                            id: 0,
                            platform: event.platform.clone(),
                            role: event.message.role.clone(),
                            content: event.message.content.clone(),
                            conversation_id,
                            space_id,
                            timestamp: event.timestamp,
                        };

                        // 追加到持久化队列
                        if let Err(e) = storage.append_message(&msg) {
                            error!("[MemorizationService] 消息入队失败: {}", e);
                            continue;
                        }

                        info!(
                            "[MemorizationService] 消息入队: role={}, platform={}, len={}",
                            msg.role,
                            msg.platform,
                            msg.content.len()
                        );

                        // 检查触发条件
                        let count = storage.get_count().unwrap_or(0);

                        // 条件 1: 立即触发
                        if count >= config.message_threshold {
                            info!(
                                "[MemorizationService] 消息数 {} >= 阈值 {}，立即触发",
                                count, config.message_threshold
                            );
                            Self::do_memorization(
                                state.clone(),
                                storage.clone(),
                                memu_client.clone(),
                                total_memorized.clone(),
                                last_memorization_at.clone(),
                                last_error.clone(),
                            )
                            .await;
                        }
                        // 条件 2: 启动/重置防抖定时器
                        else if count >= config.min_messages {
                            // 取消之前的定时器
                            {
                                let mut cancel_guard = debounce_cancel.write().await;
                                if let Some(sender) = cancel_guard.take() {
                                    let _ = sender.send(());
                                }
                            }

                            let (cancel_tx, cancel_rx) =
                                tokio::sync::oneshot::channel::<()>();
                            {
                                let mut cancel_guard = debounce_cancel.write().await;
                                *cancel_guard = Some(cancel_tx);
                            }

                            let duration = std::time::Duration::from_millis(
                                config.time_threshold_ms,
                            );
                            let is_running_inner = is_running.clone();
                            let state_inner = state.clone();
                            let storage_inner = storage.clone();
                            let memu_inner = memu_client.clone();
                            let total_inner = total_memorized.clone();
                            let last_at_inner = last_memorization_at.clone();
                            let last_err_inner = last_error.clone();
                            let min_msgs = config.min_messages;

                            tokio::spawn(async move {
                                tokio::select! {
                                    _ = tokio::time::sleep(duration) => {
                                        if !is_running_inner.load(Ordering::Relaxed) {
                                            return;
                                        }
                                        let count = storage_inner.get_count().unwrap_or(0);
                                        if count >= min_msgs {
                                            info!(
                                                "[MemorizationService] 防抖到期，消息数 {}，触发提取",
                                                count
                                            );
                                            Self::do_memorization(
                                                state_inner,
                                                storage_inner,
                                                memu_inner,
                                                total_inner,
                                                last_at_inner,
                                                last_err_inner,
                                            )
                                            .await;
                                        }
                                    }
                                    _ = cancel_rx => {}
                                }
                            });
                        }
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                        warn!(
                            "[MemorizationService] 消息总线滞后，跳过 {} 条事件",
                            n
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                        info!("[MemorizationService] 消息总线已关闭，退出监听");
                        break;
                    }
                }
            }

            info!("[MemorizationService] 后台监听循环已退出");
        });

        // 保存 handle
        {
            let mut h = self.listener_handle.write().await;
            *h = Some(handle);
        }

        info!(
            "[MemorizationService] 启动完成（阈值: {} 条, 防抖: {}ms, 最少: {} 条）",
            self.config.message_threshold,
            self.config.time_threshold_ms,
            self.config.min_messages,
        );

        Ok(())
    }

    async fn stop(&self) -> anyhow::Result<()> {
        if !self.is_running.load(Ordering::Relaxed) {
            return Ok(());
        }

        info!("[MemorizationService] 正在停止...");

        // 1. 标记为非运行状态
        self.is_running.store(false, Ordering::Relaxed);

        // 2. 取消防抖定时器
        {
            let mut cancel_guard = self.debounce_cancel.write().await;
            if let Some(sender) = cancel_guard.take() {
                let _ = sender.send(());
            }
        }

        // 3. 中止后台监听任务
        {
            let mut h = self.listener_handle.write().await;
            if let Some(handle) = h.take() {
                handle.abort();
            }
        }

        // 4. 更新状态
        {
            let mut s = self.state.write().await;
            *s = MemorizationState::Stopped;
        }
        {
            let mut sa = self.started_at.write().await;
            *sa = None;
        }

        info!("[MemorizationService] 已停止");
        Ok(())
    }

    fn status(&self) -> ServiceStatus {
        if self.is_running.load(Ordering::Relaxed) {
            ServiceStatus::Running
        } else {
            ServiceStatus::Stopped
        }
    }

    fn health(&self) -> ServiceHealth {
        let uptime_secs = {
            // 由于 health() 是同步的，无法 await RwLock
            // 使用 try_read 尝试获取
            self.started_at
                .try_read()
                .ok()
                .and_then(|guard| guard.map(|start| start.elapsed().as_secs()))
        };

        let last_error = self
            .last_error
            .try_read()
            .ok()
            .and_then(|guard| guard.clone());

        let queue_count = self.storage.get_count().unwrap_or(0);

        ServiceHealth {
            name: self.name().to_string(),
            status: self.status(),
            uptime_secs,
            last_error,
            metrics: serde_json::json!({
                "queue_count": queue_count,
                "total_memorized": self.total_memorized.load(Ordering::Relaxed),
                "enabled": self.config.enabled,
                "message_threshold": self.config.message_threshold,
                "time_threshold_ms": self.config.time_threshold_ms,
            }),
        }
    }
}
