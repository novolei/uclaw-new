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
use std::time::{Duration, Instant};

use async_trait::async_trait;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

use crate::infra::{InfraEventType, InfraService};
use crate::memory_graph::store::MemoryGraphStore;
use crate::memubot_config::MemorizationConfig;
use crate::memu::client::MemUClient;
use crate::services::{ManagedService, ServiceHealth, ServiceStatus};

use super::storage::MemorizationStorage;
use super::types::*;

/// 待处理任务超时时间（毫秒）— 超过此时间的任务视为过期
const PENDING_TASK_TIMEOUT_MS: i64 = 30 * 60 * 1000; // 30 分钟

/// 防抖绝对超时时间（毫秒）— 自首条消息入队起，超过此时间强制触发记忆提取
const MAX_DEBOUNCE_WAIT_MS: u64 = 7_200_000; // 2 小时

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
    /// MemoryGraphStore 引用（延迟注入，用于持久化 memU 提取结果）
    graph_store: Arc<RwLock<Option<Arc<MemoryGraphStore>>>>,
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
    /// MCP 管理器，用于执行 Scheme A gbrain MCP 读写
    mcp_manager: Arc<RwLock<Option<crate::mcp::SharedMcpManager>>>,
    /// LLM 客户端，用于执行 Scheme A 智能合并 (Smart LLM Merge)
    llm_client: Arc<RwLock<Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>>>,
    /// 文件夹监听器句柄
    watcher_handle: Arc<RwLock<Option<super::watcher::DraftsWatcherHandle>>>,
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
            graph_store: Arc::new(RwLock::new(None)),
            is_running: Arc::new(AtomicBool::new(false)),
            total_memorized: Arc::new(AtomicU64::new(0)),
            last_memorization_at: Arc::new(RwLock::new(None)),
            debounce_cancel: Arc::new(RwLock::new(None)),
            listener_handle: Arc::new(RwLock::new(None)),
            started_at: Arc::new(RwLock::new(None)),
            last_error: Arc::new(RwLock::new(None)),
            mcp_manager: Arc::new(RwLock::new(None)),
            llm_client: Arc::new(RwLock::new(None)),
            watcher_handle: Arc::new(RwLock::new(None)),
        }
    }

    /// 设置 MCP 管理器引用
    pub async fn set_mcp_manager(&self, mcp_manager: Option<crate::mcp::SharedMcpManager>) {
        let mut guard = self.mcp_manager.write().await;
        *guard = mcp_manager;
    }

    /// 设置 MemoryOsLlm 引用
    pub async fn set_llm_client(&self, llm_client: Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>) {
        let mut guard = self.llm_client.write().await;
        *guard = llm_client;
    }

    /// 设置 memU 客户端引用
    ///
    /// 可在服务启动后动态注入，支持 memU 延迟初始化。
    /// 设为 None 时服务进入降级模式（仅记录日志）。
    pub async fn set_memu_client(&self, client: Option<Arc<MemUClient>>) {
        let mut guard = self.memu_client.write().await;
        *guard = client;
    }

    /// 设置 MemoryGraphStore 引用
    ///
    /// 用于将 memU 提取的记忆持久化到图存储。
    /// 可在服务启动后动态注入，支持延迟初始化。
    pub async fn set_graph_store(&self, store: Arc<MemoryGraphStore>) {
        let mut guard = self.graph_store.write().await;
        *guard = Some(store);
    }

    /// 触发记忆提取（从实例方法调用）
    async fn trigger_memorization(&self) {
        Self::do_memorization(
            self.state.clone(),
            self.storage.clone(),
            self.memu_client.clone(),
            self.graph_store.clone(),
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
        graph_store: Arc<RwLock<Option<Arc<MemoryGraphStore>>>>,
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

                        // 持久化 memU 提取的记忆到 MemoryGraphStore
                        let space_id = messages
                            .iter()
                            .find_map(|m| m.space_id.as_deref())
                            .unwrap_or("default")
                            .to_string();

                        // 等待 graph_store 可用（最多 10 秒），超时则记录警告
                        let persist_result = tokio::time::timeout(Duration::from_secs(10), async {
                            loop {
                                let guard = graph_store.read().await;
                                if guard.is_some() {
                                    return guard;
                                }
                                drop(guard);
                                tokio::time::sleep(Duration::from_millis(500)).await;
                            }
                        }).await;

                        match persist_result {
                            Ok(guard) => {
                                if let Some(ref store) = *guard {
                                    match persist_memorize_results(store, memu.as_ref().map(|arc| arc.as_ref()), &space_id, &result.items).await {
                                        Ok(count) => info!(
                                            "[MemorizationService] Persisted {} memory items from memU extraction",
                                            count
                                        ),
                                        Err(e) => {
                                            warn!(
                                                "[MemorizationService] Failed to persist memU results: {}",
                                                e
                                            );
                                            // TODO: enqueue_pending_results 暂未实现，持久化失败时仅记录日志
                                        }
                                    }
                                }
                            }
                            Err(_) => {
                                warn!("[MemorizationService] graph_store unavailable after 10s, memU results not persisted");
                                // TODO: 暂存结果以便后续重试 — 需要 storage.enqueue_pending_results() 方法
                            }
                        }

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

    /// Ingest a single markdown draft file into gbrain, resolving collisions via Smart LLM Merge
    pub async fn ingest_draft_file(
        mcp_manager_lock: Arc<RwLock<Option<crate::mcp::SharedMcpManager>>>,
        llm_client_lock: Arc<RwLock<Option<Arc<dyn crate::memory_graph::memory_os_llm::MemoryOsLlm>>>>,
        path: std::path::PathBuf,
    ) -> anyhow::Result<()> {
        info!("[MemorizationService] [Scheme A] Reading draft file for ingestion: {}", path.display());
        let content = std::fs::read_to_string(&path)?;
        let (frontmatter, body) = parse_markdown_draft(&content);

        // Extract title, kind/type, and creation time
        let title = frontmatter
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("untitled")
            })
            .to_string();

        let kind = frontmatter
            .get("kind")
            .or_else(|| frontmatter.get("type"))
            .and_then(|v| v.as_str())
            .unwrap_or("curated")
            .to_string();

        // 1. Generate slug
        let slug = generate_slug(&title);
        info!("[MemorizationService] [Scheme A] Resolved title: '{}', kind: '{}', slug: '{}'", title, kind, slug);

        // 2. Query mcp_manager
        let mcp_manager_opt = mcp_manager_lock.read().await;
        let mcp_manager = mcp_manager_opt.as_ref().ok_or_else(|| {
            anyhow::anyhow!("SharedMcpManager is not initialized on MemorizationService")
        })?;

        // Try getting existing page with slug
        info!("[MemorizationService] [Scheme A] Querying existing page for slug: {}", slug);
        let existing_page = crate::gbrain::browse::get_page(mcp_manager, &slug).await;

        match existing_page {
            Ok(detail) => {
                info!("[MemorizationService] [Scheme A] Collision detected on slug '{}'. Triggering Smart LLM Merge.", slug);
                
                // Get LLM client
                let llm_client_opt = llm_client_lock.read().await;
                let llm_client = llm_client_opt.as_ref().ok_or_else(|| {
                    anyhow::anyhow!("MemoryOsLlm is not initialized on MemorizationService")
                })?;

                // Prepare system & user prompts for Smart LLM Merge
                let system_prompt = r#"You are the central synthesis engine for the uClaw long-term memory system.
Your job is to merge a new memory draft into an existing knowledge page.

## STRICT CONSTRAINTS:
1. **Facts Consolidation**: Merge and de-duplicate information from both sources into a coherent, highly-structured Markdown page. Do NOT lose any unique facts from either side.
2. **Category / YAML Integrity**: Retain appropriate frontmatter properties (like title, type/kind, tags).
3. **PRESERVE ALL BACKLINKS**: This is critical. You must absolutely retain all backlink references of the format `[[SomePageSlug]]` or `[[page-slug]]` from BOTH the existing page and the new draft. Never strip or mutate these links.
4. **Markdown only**: Output ONLY the finalized Markdown content (including YAML frontmatter block starting with `---` and ending with `---`). Do NOT include any chat filler, explanations, backticks wrapping the entire output, or markdown code block blocks (` ```markdown `). Just raw markdown with YAML frontmatter at the top.
"#;

                let user_prompt = format!(
                    "### EXISTING PAGE (slug: {}):\n\n{}\n\n### NEW DRAFT FACTS:\n\n---\ntitle: {:?}\nkind: {:?}\n---\n\n{}",
                    slug, detail.raw_markdown, title, kind, body
                );

                info!("[MemorizationService] [Scheme A] Invoking Smart LLM Merge on active model");
                let response = llm_client
                    .complete_text("memory_ingest_merge", system_prompt, &user_prompt, 4000)
                    .await?;

                let merged_markdown = clean_llm_markdown_output(&response.text);

                info!("[MemorizationService] [Scheme A] Saving merged markdown to gbrain");
                let _updated = crate::gbrain::browse::put_page(mcp_manager, &slug, &merged_markdown).await
                    .map_err(|e| anyhow::anyhow!("Failed to save merged page to gbrain: {:?}", e))?;
                
                info!("[MemorizationService] [Scheme A] Successful Smart LLM Merge on slug '{}'.", slug);
            }
            Err(crate::gbrain::browse::GbrainError::CallFailed(ref msg)) if msg.contains("page_not_found") || msg.contains("not found") => {
                info!("[MemorizationService] [Scheme A] No collision. Creating new page directly for slug '{}'.", slug);
                
                // Form new markdown page with YAML frontmatter
                let mut fm_map = match frontmatter.as_object() {
                    Some(map) => map.clone(),
                    None => serde_json::Map::new(),
                };
                fm_map.insert("slug".to_string(), serde_json::json!(slug));
                fm_map.insert("title".to_string(), serde_json::json!(title));
                fm_map.insert("type".to_string(), serde_json::json!(kind));
                if !fm_map.contains_key("created_at") {
                    fm_map.insert("created_at".to_string(), serde_json::json!(chrono::Utc::now().to_rfc3339()));
                }
                
                let fm_val = serde_json::Value::Object(fm_map);
                let markdown_content = crate::gbrain::browse::build_raw_markdown(&fm_val, &body);

                let _created = crate::gbrain::browse::put_page(mcp_manager, &slug, &markdown_content).await
                    .map_err(|e| anyhow::anyhow!("Failed to write page to gbrain: {:?}", e))?;

                info!("[MemorizationService] [Scheme A] Successfully created new page for slug '{}'.", slug);
            }
            Err(e) => {
                return Err(anyhow::anyhow!("Query existing page failed with unexpected error: {:?}", e));
            }
        }

        // 3. Immediately physically delete draft file on success
        info!("[MemorizationService] [Scheme A] Cleaning up draft file: {}", path.display());
        std::fs::remove_file(&path)?;
        info!("[MemorizationService] [Scheme A] Draft file deleted successfully.");

        Ok(())
    }
}

// ─── Drafts Watcher & Parser Helpers ──────────────────────────────────────

pub fn parse_markdown_draft(content: &str) -> (serde_json::Value, String) {
    if !content.starts_with("---\n") {
        return (serde_json::Value::Null, content.to_string());
    }
    
    // Find the ending --- separator
    if let Some(end_offset) = content[4..].find("\n---\n") {
        let yaml_str = &content[4..4 + end_offset];
        let body_offset = 4 + end_offset + 5; // skip \n---\n
        let body_str = if body_offset < content.len() {
            &content[body_offset..]
        } else {
            ""
        };
        
        match serde_yml::from_str::<serde_json::Value>(yaml_str) {
            Ok(val) => (val, body_str.to_string()),
            Err(e) => {
                warn!("[parse_markdown_draft] Failed to parse frontmatter YAML: {}, ignoring", e);
                (serde_json::Value::Null, content.to_string())
            }
        }
    } else {
        (serde_json::Value::Null, content.to_string())
    }
}

pub fn generate_slug(title: &str) -> String {
    let mut slug = String::new();
    let mut last_was_dash = false;
    
    for c in title.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_was_dash = false;
        } else if c.is_whitespace() || c == '-' || c == '_' || c == '/' || c == '\\' {
            if !last_was_dash && !slug.is_empty() {
                slug.push('-');
                last_was_dash = true;
            }
        }
    }
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug = format!("untitled-{}", uuid::Uuid::new_v4());
    }
    slug
}

pub fn clean_llm_markdown_output(text: &str) -> String {
    let mut cleaned = text.trim();
    if cleaned.starts_with("```markdown") {
        cleaned = cleaned.strip_prefix("```markdown").unwrap_or(cleaned);
        if cleaned.ends_with("```") {
            cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned);
        }
    } else if cleaned.starts_with("```") {
        cleaned = cleaned.strip_prefix("```").unwrap_or(cleaned);
        if cleaned.ends_with("```") {
            cleaned = cleaned.strip_suffix("```").unwrap_or(cleaned);
        }
    }
    cleaned.trim().to_string()
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

        // [Scheme A] 启动 Ingestion Daemon / Folder Watcher
        let drafts_dir = uclaw_utils_home::uclaw_home_pathbuf()
            .map_err(|e| anyhow::anyhow!("Failed to get uClaw home: {}", e))?
            .join("inbox/gbrain_drafts/");

        let (tx_drafts, mut rx_drafts) = tokio::sync::mpsc::unbounded_channel::<std::path::PathBuf>();
        
        info!(
            "[MemorizationService] [Scheme A] Starting DraftsWatcher targeting: {}",
            drafts_dir.display()
        );
        let watcher_handle_obj = super::watcher::start_drafts_watcher(
            drafts_dir,
            500, // debounce of 500ms
            tx_drafts,
        )?;
        {
            let mut guard = self.watcher_handle.write().await;
            *guard = Some(watcher_handle_obj);
        }

        // Spawn a background task to process incoming draft paths asynchronously
        let mcp_manager_clone = self.mcp_manager.clone();
        let llm_client_clone = self.llm_client.clone();
        let is_running_clone = self.is_running.clone();
        tokio::spawn(async move {
            info!("[MemorizationService] [Scheme A] Ingestion Daemon task spawned and waiting for drafts");
            while is_running_clone.load(Ordering::Relaxed) {
                tokio::select! {
                    Some(path) = rx_drafts.recv() => {
                        info!("[MemorizationService] [Scheme A] Received draft for ingestion: {}", path.display());
                        if let Err(e) = Self::ingest_draft_file(
                            mcp_manager_clone.clone(),
                            llm_client_clone.clone(),
                            path.clone(),
                        ).await {
                            error!("[MemorizationService] [Scheme A] Ingestion of draft file failed: {}, path: {}", e, path.display());
                        }
                    }
                    else => {
                        info!("[MemorizationService] [Scheme A] Ingestion daemon receiver channel closed");
                        break;
                    }
                }
            }
            info!("[MemorizationService] [Scheme A] Ingestion Daemon task exited");
        });

        // 3. 订阅 InfraService 事件并启动后台监听循环
        let mut rx = self.infra.subscribe();
        let is_running = self.is_running.clone();
        let state = self.state.clone();
        let storage = self.storage.clone();
        let memu_client = self.memu_client.clone();
        let graph_store = self.graph_store.clone();
        let total_memorized = self.total_memorized.clone();
        let last_memorization_at = self.last_memorization_at.clone();
        let last_error = self.last_error.clone();
        let config = self.config.clone();
        let debounce_cancel = self.debounce_cancel.clone();

        let handle = tokio::spawn(async move {
            info!("[MemorizationService] 后台监听循环已启动");

            // 防抖绝对超时追踪：记录首条消息入队时间
            let mut first_pending_at: Option<Instant> = None;

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

                        // 记录首条消息入队时间（用于绝对超时检查）
                        if first_pending_at.is_none() {
                            first_pending_at = Some(Instant::now());
                        }

                        info!(
                            "[MemorizationService] 消息入队: role={}, platform={}, len={}",
                            msg.role,
                            msg.platform,
                            msg.content.len()
                        );

                        // 检查触发条件
                        let count = storage.get_count().unwrap_or(0);

                        // 条件 0: 绝对超时检查 — 防止低频消息永不触发
                        if let Some(first) = first_pending_at {
                            if first.elapsed() > Duration::from_millis(MAX_DEBOUNCE_WAIT_MS) {
                                info!(
                                    "[MemorizationService] 防抖绝对超时（已等待 {:?}），强制触发提取",
                                    first.elapsed()
                                );
                                first_pending_at = None;
                                Self::do_memorization(
                                    state.clone(),
                                    storage.clone(),
                                    memu_client.clone(),
                                    graph_store.clone(),
                                    total_memorized.clone(),
                                    last_memorization_at.clone(),
                                    last_error.clone(),
                                )
                                .await;
                                continue;
                            }
                        }

                        // 条件 1: 立即触发
                        if count >= config.message_threshold {
                            info!(
                                "[MemorizationService] 消息数 {} >= 阈值 {}，立即触发",
                                count, config.message_threshold
                            );
                            first_pending_at = None; // 触发后重置首条消息时间
                            Self::do_memorization(
                                state.clone(),
                                storage.clone(),
                                memu_client.clone(),
                                graph_store.clone(),
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
                            let graph_store_inner = graph_store.clone();
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
                                                graph_store_inner,
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

        // [Scheme A] Stop folder watcher
        {
            let mut guard = self.watcher_handle.write().await;
            if let Some(handle) = guard.take() {
                info!("[MemorizationService] [Scheme A] Stopping DraftsWatcher");
                handle.stop();
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

// ─── Helper Functions ─────────────────────────────────────────────────────

/// 将 memU 提取的记忆项持久化到 MemoryGraphStore
///
/// TODO: 待 MemoryGraphStore::with_transaction() 实现后，将整个 for 循环包装在事务中，
/// 确保所有节点、版本、关键词的创建操作原子性提交或回滚。
async fn persist_memorize_results(
    store: &MemoryGraphStore,
    memu_client: Option<&MemUClient>,
    _space_id: &str,
    items: &[serde_json::Value],
) -> anyhow::Result<usize> {
    let mut count = 0;

    for item in items {
        let title = item
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Untitled Memory");
        let content = item
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() {
            continue;
        }

        // 确定 kind: 从 item metadata 推断，默认 Curated
        let kind_str = item
            .get("kind")
            .or_else(|| item.get("category"))
            .and_then(|v| v.as_str())
            .unwrap_or("curated");

        match kind_str.to_lowercase().as_str() {
            "user_profile" | "userprofile" | "identity" | "style" | "goal" => {
                // Route to user_profile_facets SQLite table
                let class = match kind_str.to_lowercase().as_str() {
                    "style" => "style",
                    "goal" => "goal",
                    _ => "identity",
                };

                let conn = store.conn.lock().map_err(|e| anyhow::anyhow!("DB lock error: {}", e))?;
                
                // Check if a row with the same class and name already exists to preserve created_at and facet_id
                let mut existing: Option<(String, i64)> = None;
                if let Ok(mut stmt) = conn.prepare("SELECT facet_id, created_at FROM user_profile_facets WHERE class = ?1 AND name = ?2") {
                    if let Ok(mut rows) = stmt.query(rusqlite::params![class, title]) {
                        if let Ok(Some(row)) = rows.next() {
                            if let (Ok(fid), Ok(cat)) = (row.get::<_, String>(0), row.get::<_, i64>(1)) {
                                existing = Some((fid, cat));
                            }
                        }
                    }
                }

                let now_ms = chrono::Utc::now().timestamp_millis();
                let (facet_id, created_at) = match existing {
                    Some((fid, cat)) => (fid, cat),
                    None => (format!("facet-{}", uuid::Uuid::new_v4()), now_ms),
                };

                conn.execute(
                    "INSERT OR REPLACE INTO user_profile_facets \
                     (facet_id, class, name, value, state, stability, \
                      cue_families_json, evidence_count, last_seen_at, \
                      created_at, updated_at) \
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
                    rusqlite::params![
                        facet_id,
                        class,
                        title,
                        content,
                        "active",
                        0.90f64,
                        "{}",
                        1i64,
                        now_ms,
                        created_at,
                        now_ms,
                    ],
                )?;
                count += 1;
            }
            "episode" => {
                // Route to memu.db SQLite via MemUClient
                if let Some(client) = memu_client {
                    match client.create_item("episode", content, vec!["episode".to_string()], None).await {
                        Ok(_) => {
                            count += 1;
                        }
                        Err(e) => {
                            warn!("Failed to create episode in memU: {}", e);
                        }
                    }
                } else {
                    warn!("MemUClient is unavailable, skipping episode memory: {}", title);
                }
            }
            _ => {
                // Route to offline Markdown files
                let drafts_dir = uclaw_utils_home::uclaw_home_pathbuf()
                    .map_err(|e| anyhow::anyhow!("Failed to get uClaw home: {}", e))?
                    .join("inbox/gbrain_drafts/");

                std::fs::create_dir_all(&drafts_dir)?;

                let safe_title: String = title
                    .chars()
                    .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else { '_' })
                    .collect();
                let filename = format!("{}_{}.md", safe_title, uuid::Uuid::new_v4());
                let file_path = drafts_dir.join(filename);

                let now_rfc = chrono::Utc::now().to_rfc3339();
                let markdown_content = format!(
                    "---\ntitle: {:?}\nkind: {:?}\ncreated_at: {:?}\n---\n\n{}",
                    title, kind_str, now_rfc, content
                );
                std::fs::write(&file_path, markdown_content)?;
                count += 1;
            }
        }
    }

    Ok(count)
}

/// 从标题中提取关键词（简单启发式）
fn extract_keywords_from_title(title: &str) -> Vec<String> {
    title
        .split(|c: char| {
            c.is_whitespace()
                || c == ','
                || c == '.'
                || c == ':'
                || c == ';'
                || c == '/'
                || c == '-'
        })
        .filter(|w| w.len() >= 2) // 至少 2 字节（中文单字 3 bytes 也会通过）
        .map(|w| w.to_lowercase())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .take(10) // 最多 10 个关键词
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_slug() {
        // English slug with punctuation
        assert_eq!(generate_slug("Hello, World!"), "hello-world");
        assert_eq!(generate_slug("---Trim-Me---"), "trim-me");
        assert_eq!(generate_slug("Spaces   and___underscores"), "spaces-and-underscores");
        
        // Chinese / CJK characters are preserved (alphanumeric in Rust)
        assert_eq!(generate_slug("我的 记忆_page"), "我的-记忆-page");
        assert_eq!(generate_slug("深度学习-Deep Learning"), "深度学习-deep-learning");
        
        // Empty or pure symbols falls back to untitled-uuid
        let empty_slug = generate_slug("!!! @@@ ###");
        assert!(empty_slug.starts_with("untitled-"));
        assert_eq!(empty_slug.len(), "untitled-".len() + 36); // untitled + 36-char UUID
    }

    #[test]
    fn test_parse_markdown_draft() {
        // Normal YAML frontmatter + body
        let input = "---\ntitle: \"Test Topic\"\nkind: \"identity\"\n---\nBody of the page\nand some content.";
        let (fm, body) = parse_markdown_draft(input);
        assert_eq!(fm.get("title").and_then(|v| v.as_str()), Some("Test Topic"));
        assert_eq!(fm.get("kind").and_then(|v| v.as_str()), Some("identity"));
        assert_eq!(body.trim(), "Body of the page\nand some content.");

        // No frontmatter
        let input_no_fm = "Just plain body text without YAML.";
        let (fm_no, body_no) = parse_markdown_draft(input_no_fm);
        assert!(fm_no.is_null());
        assert_eq!(body_no, input_no_fm);

        // Invalid frontmatter format (no closing separator)
        let input_invalid = "---\ntitle: Missing closing\nJust content";
        let (fm_inv, body_inv) = parse_markdown_draft(input_invalid);
        assert!(fm_inv.is_null());
        assert_eq!(body_inv, input_invalid);
    }

    #[test]
    fn test_clean_llm_markdown_output() {
        // Wrapped in ```markdown
        let input_md = "```markdown\n---\ntitle: Page\n---\nContent\n```";
        assert_eq!(clean_llm_markdown_output(input_md), "---\ntitle: Page\n---\nContent");

        // Wrapped in generic ```
        let input_generic = "```\nPlain output\n```";
        assert_eq!(clean_llm_markdown_output(input_generic), "Plain output");

        // Already clean
        let input_clean = "Clean markdown without blocks";
        assert_eq!(clean_llm_markdown_output(input_clean), "Clean markdown without blocks");
    }
}

