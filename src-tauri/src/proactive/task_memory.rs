//! 历史任务记忆管理器
//!
//! 记录用户执行的各类任务（代码生成、调试、重构等），
//! 支持按描述相似度查找历史任务及解决方案，供 ProactiveService 场景评估时参考。
//!
//! C.2 migration (2026-05-31): Episode write/read now uses `MemoryAdapter`
//! (bucket_seal backend, namespace `proactive:episode:{space_id}`) instead of
//! the frozen `MemoryGraphStore`. `record_task`, `list_recent_tasks`, and
//! `find_similar_tasks` are all `async fn`.  `list_recent_tasks` uses
//! `adapter.list(Some(&ns), …)` (not `recall("", …)`) because FTS5 MATCH
//! rejects an empty query string.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::memory_adapter::{MemoryAdapter, MemoryCategory, RecallOpts};

// ─── 任务类型 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskType {
    CodeGeneration,
    Debugging,
    Refactoring,
    CodeReview,
    Testing,
    Documentation,
    Configuration,
    Research,
    Planning,
    Other,
}

impl TaskType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::CodeGeneration => "code_generation",
            Self::Debugging => "debugging",
            Self::Refactoring => "refactoring",
            Self::CodeReview => "code_review",
            Self::Testing => "testing",
            Self::Documentation => "documentation",
            Self::Configuration => "configuration",
            Self::Research => "research",
            Self::Planning => "planning",
            Self::Other => "other",
        }
    }
}

// ─── 任务状态 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Success,
    Partial,
    Failed,
}

impl TaskStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Partial => "partial",
            Self::Failed => "failed",
        }
    }
}

// ─── 任务记录 ─────────────────────────────────────────────────────────

/// 一条已完成任务的完整记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskRecord {
    /// 任务类型
    pub task_type: TaskType,
    /// 任务描述（用户原始请求摘要）
    pub description: String,
    /// 执行结果状态
    pub status: TaskStatus,
    /// 操作涉及的文件列表
    pub files_changed: Vec<String>,
    /// 使用的工具列表
    pub tools_used: Vec<String>,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 遇到的错误信息（如有）
    pub error_messages: Vec<String>,
    /// 解决方案摘要（如有）
    pub solution_summary: Option<String>,
    /// 关联的 session ID
    pub session_id: Option<String>,
}

// ─── 相似任务 ─────────────────────────────────────────────────────────

/// 搜索结果：一条与当前查询相似的历史任务
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimilarTask {
    /// 记忆节点 ID
    pub node_id: String,
    /// 任务标题
    pub title: String,
    /// 任务类型
    pub task_type: String,
    /// 执行状态
    pub status: String,
    /// 方案摘要
    pub solution_summary: Option<String>,
    /// 涉及文件
    pub files_changed: Vec<String>,
    /// 匹配分数（越高越相关）
    pub score: f32,
    /// 记录时间
    pub recorded_at: String,
}

// ─── 序列化/反序列化辅助 ──────────────────────────────────────────────

/// Serialise a task episode into a compact JSON string for MemoryAdapter storage.
///
/// All fields needed to reconstruct a `SimilarTask` are included.
fn task_to_content(
    title: &str,
    task_type: &str,
    status: &str,
    solution_summary: Option<&str>,
    files_changed: &[String],
    recorded_at: &str,
    keywords: &[String],
) -> String {
    serde_json::json!({
        "title": title,
        "task_type": task_type,
        "status": status,
        "solution_summary": solution_summary,
        "files_changed": files_changed,
        "recorded_at": recorded_at,
        "keywords": keywords,
    })
    .to_string()
}

/// Deserialise a `MemoryEntry` back into a `SimilarTask`.
///
/// Returns `None` on malformed JSON so callers can `filter_map` gracefully.
fn entry_to_similar_task(id: &str, content: &str, score: f64) -> Option<SimilarTask> {
    let v: serde_json::Value = serde_json::from_str(content).ok()?;
    Some(SimilarTask {
        node_id: id.to_string(),
        title: v
            .get("title")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
        task_type: v
            .get("task_type")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string(),
        status: v
            .get("status")
            .and_then(|x| x.as_str())
            .unwrap_or("unknown")
            .to_string(),
        solution_summary: v
            .get("solution_summary")
            .and_then(|x| x.as_str())
            .map(String::from),
        files_changed: v
            .get("files_changed")
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        // MemoryEntry.score is f64; SimilarTask.score is f32.
        score: score as f32,
        recorded_at: v
            .get("recorded_at")
            .and_then(|x| x.as_str())
            .unwrap_or("")
            .to_string(),
    })
}

// ─── 任务记忆管理器 ───────────────────────────────────────────────────

/// 任务记忆管理器
///
/// C.2: backed by `MemoryAdapter` (bucket_seal), namespace
/// `proactive:episode:{space_id}`.  Methods are `async` because the adapter
/// trait is async.
pub struct TaskMemoryManager {
    adapter: Arc<dyn MemoryAdapter>,
}

impl TaskMemoryManager {
    /// 创建新的任务记忆管理器
    pub fn new(adapter: Arc<dyn MemoryAdapter>) -> Self {
        Self { adapter }
    }

    /// 记录一个任务的执行结果
    ///
    /// Serialises the task into a JSON content string and stores it via the
    /// MemoryAdapter under namespace `proactive:episode:{space_id}`.
    ///
    /// 返回创建的节点 ID。
    pub async fn record_task(
        &self,
        space_id: &str,
        task: &TaskRecord,
    ) -> Result<String, crate::error::Error> {
        let node_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        // 构建标题：task_type + description 前 80 字符
        let title = format!(
            "[{}] {}",
            task.task_type.as_str(),
            truncate_str(&task.description, 80)
        );

        // Extract keywords and include them in the content blob so FTS can
        // match against description terms when find_similar_tasks queries.
        let keywords = extract_keywords(&task.description, &task.files_changed);

        let content = task_to_content(
            &title,
            task.task_type.as_str(),
            task.status.as_str(),
            task.solution_summary.as_deref(),
            &task.files_changed,
            &now,
            &keywords,
        );

        let ns = format!("proactive:episode:{}", space_id);
        self.adapter
            .store(
                &ns,
                &node_id,
                &content,
                MemoryCategory::Core,
                task.session_id.as_deref(),
            )
            .await
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        tracing::info!(
            node_id = %node_id,
            task_type = %task.task_type.as_str(),
            status = %task.status.as_str(),
            namespace = %ns,
            "Task recorded via MemoryAdapter (C.2)"
        );

        Ok(node_id)
    }

    /// 查找与当前任务描述相似的历史任务
    ///
    /// C.2: uses `adapter.recall(query, limit, RecallOpts { namespace })`.
    /// The query is the joined keywords extracted from `current_task_desc`.
    pub async fn find_similar_tasks(
        &self,
        space_id: &str,
        current_task_desc: &str,
        limit: usize,
    ) -> Result<Vec<SimilarTask>, crate::error::Error> {
        let ns = format!("proactive:episode:{}", space_id);
        // Build a FTS-friendly query from the description keywords.
        let keywords = extract_query_keywords(current_task_desc);
        let query = keywords.join(" ");

        if query.trim().is_empty() {
            // No useful keywords — skip the adapter call and return empty.
            return Ok(Vec::new());
        }

        let entries = self
            .adapter
            .recall(
                &query,
                limit,
                RecallOpts {
                    namespace: Some(&ns),
                    ..Default::default()
                },
            )
            .await
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        let results: Vec<SimilarTask> = entries
            .into_iter()
            .filter_map(|e| entry_to_similar_task(&e.id, &e.content, e.score.unwrap_or(0.0)))
            .collect();

        Ok(results)
    }

    /// 列出指定空间最近的任务记录
    ///
    /// C.2: uses `adapter.list(Some(&ns), None, None)` — NOT `recall("", …)`
    /// because FTS5 MATCH rejects an empty query string and would return an error.
    pub async fn list_recent_tasks(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<SimilarTask>, crate::error::Error> {
        let ns = format!("proactive:episode:{}", space_id);

        let mut entries = self
            .adapter
            .list(Some(&ns), None, None)
            .await
            .map_err(|e| crate::error::Error::Internal(e.to_string()))?;

        // The adapter returns up to its own cap (200 for bucket_seal); take
        // the caller-requested `limit` from the front (entries are already
        // ordered by timestamp_ms DESC from the SQL layer).
        entries.truncate(limit);

        let results: Vec<SimilarTask> = entries
            .into_iter()
            .filter_map(|e| entry_to_similar_task(&e.id, &e.content, e.score.unwrap_or(0.0)))
            .collect();

        Ok(results)
    }
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────

/// 截断字符串至 max_len 字符
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.chars().count() <= max_len {
        s.to_string()
    } else {
        format!("{}…", s.chars().take(max_len).collect::<String>())
    }
}

/// 从任务描述和文件列表提取关键词
fn extract_keywords(description: &str, files: &[String]) -> Vec<String> {
    let mut keywords = Vec::new();

    let has_cjk = description.chars().any(|c| c as u32 >= 0x4E00 && c as u32 <= 0x9FFF);

    if has_cjk {
        // 中文文本：提取 2-4 字 n-gram
        let chars: Vec<char> = description
            .chars()
            .filter(|c| c.is_alphanumeric() || *c as u32 >= 0x4E00)
            .collect();
        for len in [4, 3, 2] {
            for window in chars.windows(len) {
                let kw: String = window.iter().collect();
                keywords.push(kw.to_lowercase());
            }
        }
    } else {
        // 英文文本：按空白/标点分割
        let words: Vec<&str> = description
            .split(|c: char| {
                c.is_whitespace()
                    || c == ','
                    || c == '.'
                    || c == ':'
                    || c == ';'
                    || c == '('
                    || c == ')'
                    || c == '['
                    || c == ']'
            })
            .filter(|w| w.len() >= 2)
            .collect();
        for word in words {
            let cleaned =
                word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
            if cleaned.len() >= 2 {
                keywords.push(cleaned.to_lowercase());
            }
        }
    }

    // 添加文件名作为关键词
    for f in files {
        if let Some(name) = std::path::Path::new(f).file_name() {
            if let Some(s) = name.to_str() {
                keywords.push(s.to_lowercase());
            }
        }
    }

    // 去重 + 限制数量
    keywords.sort();
    keywords.dedup();
    keywords.truncate(30);

    keywords
}

/// 从查询描述提取检索关键词
fn extract_query_keywords(desc: &str) -> Vec<String> {
    let mut keywords: Vec<String>;

    let has_cjk = desc.chars().any(|c| c as u32 >= 0x4E00 && c as u32 <= 0x9FFF);

    if has_cjk {
        // 中文：提取 2-4 字 n-gram
        let chars: Vec<char> = desc
            .chars()
            .filter(|c| c.is_alphanumeric() || *c as u32 >= 0x4E00)
            .collect();
        keywords = Vec::new();
        for len in [4, 3, 2] {
            for window in chars.windows(len) {
                let kw: String = window.iter().collect();
                keywords.push(kw.to_lowercase());
            }
        }
    } else {
        keywords = desc
            .split(|c: char| {
                c.is_whitespace() || c == ',' || c == '.' || c == ':' || c == ';'
            })
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-')
                    .to_lowercase()
            })
            .filter(|w| w.len() >= 2)
            .collect();
    }

    keywords.sort();
    keywords.dedup();
    keywords.truncate(15);

    keywords
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── Minimal in-process adapter for tests ────────────────────────────

    /// Thread-safe in-memory `MemoryAdapter` used for unit tests.
    /// Stores entries in a `Vec`; `recall` does a simple substring match.
    struct InMemoryAdapter {
        /// (namespace, key) → MemoryEntry
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Arc<dyn MemoryAdapter> {
            Arc::new(Self {
                store: Mutex::new(HashMap::new()),
            })
        }
    }

    #[async_trait]
    impl MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str {
            "in_memory_test"
        }

        async fn store(
            &self,
            namespace: &str,
            key: &str,
            content: &str,
            category: MemoryCategory,
            session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = MemoryEntry {
                id: key.to_string(),
                key: key.to_string(),
                content: content.to_string(),
                namespace: Some(namespace.to_string()),
                category,
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: session_id.map(String::from),
                score: None,
            };
            self.store
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), entry);
            Ok(())
        }

        async fn recall(
            &self,
            query: &str,
            limit: usize,
            opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            // Split query on whitespace so that any individual term can match
            // (mirrors FTS5 OR semantics for the in-memory test adapter).
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| {
                    // Optional namespace filter
                    if let Some(ns) = opts.namespace {
                        if e.namespace.as_deref() != Some(ns) {
                            return false;
                        }
                    }
                    // Any term matches anywhere in content
                    let content_lower = e.content.to_lowercase();
                    terms.iter().any(|t| content_lower.contains(t.as_str()))
                })
                .cloned()
                .collect();
            // Stable ordering for deterministic tests
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out.truncate(limit);
            Ok(out)
        }

        async fn get(
            &self,
            namespace: &str,
            key: &str,
        ) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn list(
            &self,
            namespace: Option<&str>,
            _category: Option<&MemoryCategory>,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| match namespace {
                    Some(ns) => e.namespace.as_deref() == Some(ns),
                    None => true,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(out)
        }

        async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
            let removed = self
                .store
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()))
                .is_some();
            Ok(removed)
        }

        async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
            let mut store = self.store.lock().unwrap();
            let before = store.len();
            store.retain(|(ns, _), _| ns != namespace);
            Ok((before - store.len()) as u64)
        }

        async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
            Ok(Vec::new())
        }
    }

    // ── Serde helper round-trip tests ───────────────────────────────────

    #[test]
    fn test_task_to_content_round_trip() {
        let title = "[debugging] fix pool leak";
        let task_type = "debugging";
        let status = "success";
        let solution_summary = Some("added health check");
        let files_changed = vec!["src/db/pool.rs".to_string()];
        let recorded_at = "2026-05-31T00:00:00Z";
        let keywords = vec!["pool".to_string(), "database".to_string()];

        let content = task_to_content(
            title,
            task_type,
            status,
            solution_summary,
            &files_changed,
            recorded_at,
            &keywords,
        );

        // Parse back via entry_to_similar_task
        let task = entry_to_similar_task("node-001", &content, 0.75)
            .expect("entry_to_similar_task returned None");

        assert_eq!(task.node_id, "node-001");
        assert_eq!(task.title, title);
        assert_eq!(task.task_type, task_type);
        assert_eq!(task.status, status);
        assert_eq!(task.solution_summary.as_deref(), solution_summary);
        assert_eq!(task.files_changed, files_changed);
        assert!((task.score - 0.75_f32).abs() < 1e-5);
        assert_eq!(task.recorded_at, recorded_at);
    }

    #[test]
    fn test_entry_to_similar_task_malformed_returns_none() {
        assert!(entry_to_similar_task("id", "not json {{{{", 0.0).is_none());
        assert!(entry_to_similar_task("id", "", 0.0).is_none());
    }

    #[test]
    fn test_entry_to_similar_task_missing_fields_uses_defaults() {
        // Minimal JSON — missing most fields
        let content = r#"{"title":"t"}"#;
        let task = entry_to_similar_task("x", content, 0.5).unwrap();
        assert_eq!(task.task_type, "unknown");
        assert_eq!(task.status, "unknown");
        assert!(task.solution_summary.is_none());
        assert!(task.files_changed.is_empty());
        assert_eq!(task.recorded_at, "");
    }

    // ── Manager integration tests ────────────────────────────────────────

    #[tokio::test]
    async fn test_record_and_list_tasks() {
        let adapter = InMemoryAdapter::new();
        let manager = TaskMemoryManager::new(adapter);

        let task = TaskRecord {
            task_type: TaskType::CodeGeneration,
            description: "实现用户登录功能，包含 JWT 认证".to_string(),
            status: TaskStatus::Success,
            files_changed: vec!["src/auth/login.rs".to_string()],
            tools_used: vec!["write_file".to_string(), "run_tests".to_string()],
            duration_ms: 120_000,
            error_messages: vec![],
            solution_summary: Some("使用 jsonwebtoken crate 实现 JWT 签发和验证".to_string()),
            session_id: Some("session-1".to_string()),
        };

        let node_id = manager.record_task("default", &task).await.unwrap();
        assert!(!node_id.is_empty());

        // 列出最近任务
        let recent = manager.list_recent_tasks("default", 10).await.unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].task_type, "code_generation");
        assert_eq!(recent[0].status, "success");
    }

    #[tokio::test]
    async fn test_find_similar_tasks() {
        let adapter = InMemoryAdapter::new();
        let manager = TaskMemoryManager::new(adapter);

        // Record first task
        let task1 = TaskRecord {
            task_type: TaskType::Debugging,
            description: "修复数据库连接池泄漏问题".to_string(),
            status: TaskStatus::Success,
            files_changed: vec!["src/db/pool.rs".to_string()],
            tools_used: vec!["search_codebase".to_string(), "write_file".to_string()],
            duration_ms: 60_000,
            error_messages: vec!["connection timeout".to_string()],
            solution_summary: Some("增加连接超时配置，添加连接健康检查".to_string()),
            session_id: Some("session-1".to_string()),
        };
        manager.record_task("default", &task1).await.unwrap();

        // Record second task
        let task2 = TaskRecord {
            task_type: TaskType::Refactoring,
            description: "重构用户模块，提取公共认证逻辑".to_string(),
            status: TaskStatus::Success,
            files_changed: vec!["src/auth/mod.rs".to_string()],
            tools_used: vec!["write_file".to_string()],
            duration_ms: 90_000,
            error_messages: vec![],
            solution_summary: Some("创建 AuthService trait，实现模块化解耦".to_string()),
            session_id: Some("session-2".to_string()),
        };
        manager.record_task("default", &task2).await.unwrap();

        // Search for tasks related to "数据库连接超时问题"
        let similar = manager
            .find_similar_tasks("default", "数据库连接超时问题", 5)
            .await
            .unwrap();
        assert!(!similar.is_empty());

        // At least one result should be the database pool task
        let has_pool_task = similar.iter().any(|t| {
            t.files_changed.iter().any(|f| f.contains("pool"))
                || t.title.contains("数据库")
        });
        assert!(has_pool_task);
    }

    #[tokio::test]
    async fn test_find_similar_tasks_empty_when_no_match() {
        let adapter = InMemoryAdapter::new();
        let manager = TaskMemoryManager::new(adapter);

        let task = TaskRecord {
            task_type: TaskType::Documentation,
            description: "编写 API 文档".to_string(),
            status: TaskStatus::Success,
            files_changed: vec!["docs/api.md".to_string()],
            tools_used: vec![],
            duration_ms: 30_000,
            error_messages: vec![],
            solution_summary: None,
            session_id: None,
        };
        manager.record_task("default", &task).await.unwrap();

        // Query with a term guaranteed not to appear in any stored content
        let similar = manager
            .find_similar_tasks("default", "xyzzy_nonexistent_term", 5)
            .await
            .unwrap();
        assert!(similar.is_empty());
    }

    #[tokio::test]
    async fn test_list_recent_tasks_respects_limit() {
        let adapter = InMemoryAdapter::new();
        let manager = TaskMemoryManager::new(adapter);

        for i in 0..5 {
            let task = TaskRecord {
                task_type: TaskType::Other,
                description: format!("task {}", i),
                status: TaskStatus::Success,
                files_changed: vec![],
                tools_used: vec![],
                duration_ms: 1_000,
                error_messages: vec![],
                solution_summary: None,
                session_id: None,
            };
            manager.record_task("default", &task).await.unwrap();
        }

        let recent = manager.list_recent_tasks("default", 3).await.unwrap();
        assert!(recent.len() <= 3);
    }
}
