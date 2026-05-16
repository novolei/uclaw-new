//! 历史任务记忆管理器
//!
//! 记录用户执行的各类任务（代码生成、调试、重构等），
//! 支持按描述相似度查找历史任务及解决方案，供 ProactiveService 场景评估时参考。

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::memory_graph::models::{MemoryKeyword, MemoryNode, MemoryNodeKind};
use crate::memory_graph::store::MemoryGraphStore;

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

// ─── 任务记忆管理器 ───────────────────────────────────────────────────

/// 任务记忆管理器
///
/// 使用 MemoryGraphStore 存储任务记录（kind=Episode），
/// 支持按关键词检索历史任务。
pub struct TaskMemoryManager {
    store: Arc<MemoryGraphStore>,
}

impl TaskMemoryManager {
    /// 创建新的任务记忆管理器
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    /// 记录一个任务的执行结果
    ///
    /// 将任务序列化为 MemoryNode（kind=Episode），
    /// 通过 metadata 保存结构化字段，通过 keywords 建立索引。
    ///
    /// 返回创建的节点 ID。
    pub fn record_task(
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

        // 构建 metadata
        let metadata = serde_json::json!({
            "task_type": task.task_type.as_str(),
            "status": task.status.as_str(),
            "files_changed": task.files_changed,
            "tools_used": task.tools_used,
            "duration_ms": task.duration_ms,
            "error_messages": task.error_messages,
            "solution_summary": task.solution_summary,
            "session_id": task.session_id,
        });

        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Episode,
            title,
            metadata: Some(metadata),
            created_at: now.clone(),
            updated_at: now,
        };

        self.store.create_node(&node)?;

        // 提取关键词并写入 memory_keywords 表
        let keywords = extract_keywords(&task.description, &task.files_changed);
        let now_keywords = chrono::Utc::now().to_rfc3339();
        for kw in &keywords {
            let mkw = MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: space_id.to_string(),
                node_id: node_id.clone(),
                keyword: kw.clone(),
                created_at: now_keywords.clone(),
            };
            self.store.create_keyword(&mkw).unwrap_or_else(|e| {
                tracing::warn!(keyword = %kw, error = %e, "Failed to add task keyword");
            });
        }

        tracing::info!(
            node_id = %node_id,
            task_type = %task.task_type.as_str(),
            status = %task.status.as_str(),
            "Task recorded in memory graph"
        );

        Ok(node_id)
    }

    /// 查找与当前任务描述相似的历史任务
    ///
    /// 使用关键词匹配（memory_keywords 表 LIKE 查询），
    /// 按 updated_at 降序排列，返回最近 limit 条。
    pub fn find_similar_tasks(
        &self,
        space_id: &str,
        current_task_desc: &str,
        limit: usize,
    ) -> Result<Vec<SimilarTask>, crate::error::Error> {
        // 从当前描述提取关键词
        let query_keywords = extract_query_keywords(current_task_desc);

        let mut all_nodes: Vec<MemoryNode> = Vec::new();
        let mut seen_ids = std::collections::HashSet::new();

        for kw in &query_keywords {
            match self.store.search_by_keyword(space_id, kw) {
                Ok(nodes) => {
                    for node in nodes {
                        if node.kind == MemoryNodeKind::Episode && seen_ids.insert(node.id.clone()) {
                            all_nodes.push(node);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(keyword = %kw, error = %e, "Keyword search failed for task recall");
                }
            }
        }

        // 按 updated_at 降序排列
        all_nodes.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let results: Vec<SimilarTask> = all_nodes
            .into_iter()
            .take(limit)
            .map(|node| {
                let meta = node.metadata.as_ref();
                let task_type = meta
                    .and_then(|m| m.get("task_type"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let status = meta
                    .and_then(|m| m.get("status"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                let solution_summary = meta
                    .and_then(|m| m.get("solution_summary"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let files_changed: Vec<String> = meta
                    .and_then(|m| m.get("files_changed"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();

                // 简单相关性分数：匹配关键词数 / 总查询关键词数
                let score = compute_match_score(&query_keywords, &node);

                SimilarTask {
                    node_id: node.id,
                    title: node.title,
                    task_type,
                    status,
                    solution_summary,
                    files_changed,
                    score,
                    recorded_at: node.created_at,
                }
            })
            .collect();

        Ok(results)
    }

    /// 列出指定空间最近的任务记录
    pub fn list_recent_tasks(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<SimilarTask>, crate::error::Error> {
        let nodes = self
            .store
            .list_nodes_by_kind(space_id, MemoryNodeKind::Episode, limit)?;

        let results: Vec<SimilarTask> = nodes
            .into_iter()
            .map(|node| {
                let meta = node.metadata.as_ref();
                SimilarTask {
                    node_id: node.id,
                    title: node.title,
                    task_type: meta
                        .and_then(|m| m.get("task_type"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    status: meta
                        .and_then(|m| m.get("status"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown")
                        .to_string(),
                    solution_summary: meta
                        .and_then(|m| m.get("solution_summary"))
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    files_changed: meta
                        .and_then(|m| m.get("files_changed"))
                        .and_then(|v| v.as_array())
                        .map(|arr| {
                            arr.iter()
                                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                .collect()
                        })
                        .unwrap_or_default(),
                    score: 0.0,
                    recorded_at: node.created_at,
                }
            })
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
            .split(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ':' || c == ';' || c == '(' || c == ')' || c == '[' || c == ']')
            .filter(|w| w.len() >= 2)
            .collect();
        for word in words {
            let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-');
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
            .split(|c: char| c.is_whitespace() || c == ',' || c == '.' || c == ':' || c == ';')
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '-').to_lowercase())
            .filter(|w| w.len() >= 2)
            .collect();
    }

    keywords.sort();
    keywords.dedup();
    keywords.truncate(15);

    keywords
}

/// 计算匹配分数：匹配到的查询关键词比例
fn compute_match_score(query_keywords: &[String], node: &MemoryNode) -> f32 {
    if query_keywords.is_empty() {
        return 0.0;
    }

    let title_lower = node.title.to_lowercase();
    let meta_text = node
        .metadata
        .as_ref()
        .map(|m| {
            serde_json::to_string(m).unwrap_or_default().to_lowercase()
        })
        .unwrap_or_default();

    let matched = query_keywords
        .iter()
        .filter(|kw| title_lower.contains(kw.as_str()) || meta_text.contains(kw.as_str()))
        .count();

    matched as f32 / query_keywords.len() as f32
}

// ─── 单元测试 ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    fn make_test_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .unwrap();
        let conn = Arc::new(std::sync::Mutex::new(conn));
        Arc::new(MemoryGraphStore::new(conn))
    }

    #[test]
    fn test_record_and_list_tasks() {
        let store = make_test_store();
        let manager = TaskMemoryManager::new(store);

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

        let node_id = manager.record_task("default", &task).unwrap();
        assert!(!node_id.is_empty());

        // 列出最近任务
        let recent = manager.list_recent_tasks("default", 10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].task_type, "code_generation");
        assert_eq!(recent[0].status, "success");
    }

    #[test]
    fn test_find_similar_tasks() {
        let store = make_test_store();
        let manager = TaskMemoryManager::new(store);

        // 记录第一条任务
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
        manager.record_task("default", &task1).unwrap();

        // 记录第二条任务
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
        manager.record_task("default", &task2).unwrap();

        // 搜索与"数据库连接问题"相关的任务
        let similar = manager
            .find_similar_tasks("default", "数据库连接超时问题", 5)
            .unwrap();
        assert!(!similar.is_empty());

        // 至少应找到与 database/pool 相关的任务
        let has_pool_task = similar.iter().any(|t| {
            t.files_changed.iter().any(|f| f.contains("pool"))
                || t.title.contains("数据库")
        });
        assert!(has_pool_task);
    }

    #[test]
    fn test_find_similar_tasks_empty_when_no_match() {
        let store = make_test_store();
        let manager = TaskMemoryManager::new(store);

        // 记录一条不相关的任务
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
        manager.record_task("default", &task).unwrap();

        // 搜索完全不相关的关键词
        let similar = manager
            .find_similar_tasks("default", "xyzzy_nonexistent_term", 5)
            .unwrap();
        assert!(similar.is_empty());
    }
}
