//! 失败经验库
//!
//! 记录失败案例（编译错误、运行时错误、工具错误等），
//! 支持按错误模式检索历史失败及已知解决方案，
//! 帮助 Agent 在相似场景中避免重复错误。
//!
//! ## 设计
//! ```text
//! Agent 执行失败 → record_failure()
//!     ├─ failure_type: compilation_error | runtime_error | tool_error
//!     ├─ error_pattern: 标准化错误模式签名
//!     ├─ context: 触发上下文
//!     └─ resolution: 已知解决方案（可选）
//!     ↓
//! 创建 MemoryNode(kind=Episode)
//!     ├─ metadata: failure_type, error_pattern, severity
//!     └─ 关联到相关 Procedure/Reference 节点（via edges）
//!
//! Agent 遇到错误 → find_related_failures()
//!     ├─ FTS5 搜索 error_pattern
//!     └─ 返回相关失败案例 + 解决方案
//! ```

use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 失败类型 ─────────────────────────────────────────────────────────

/// 失败类型
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureType {
    /// 编译错误
    CompilationError,
    /// 运行时错误
    RuntimeError,
    /// 工具执行错误
    ToolError,
    /// 测试失败
    TestFailure,
    /// LLM 调用失败
    LlmError,
    /// 其他错误
    Other,
}

impl FailureType {
    pub fn as_str(&self) -> &str {
        match self {
            Self::CompilationError => "compilation_error",
            Self::RuntimeError => "runtime_error",
            Self::ToolError => "tool_error",
            Self::TestFailure => "test_failure",
            Self::LlmError => "llm_error",
            Self::Other => "other",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "compilation_error" => Self::CompilationError,
            "runtime_error" => Self::RuntimeError,
            "tool_error" => Self::ToolError,
            "test_failure" => Self::TestFailure,
            "llm_error" => Self::LlmError,
            _ => Self::Other,
        }
    }

    /// 推断失败类型（基于工具名或错误内容）
    pub fn infer(tool_name: &str, error_message: &str) -> Self {
        let combined = format!("{} {}", tool_name, error_message).to_lowercase();

        if combined.contains("compil")
            || combined.contains("rustc")
            || combined.contains("cargo check")
        {
            return Self::CompilationError;
        }

        if combined.contains("test fail")
            || combined.contains("assertion")
            || combined.contains("panic")
        {
            return Self::TestFailure;
        }

        if combined.contains("tool")
            || combined.contains("command")
            || combined.contains("execute")
        {
            return Self::ToolError;
        }

        if combined.contains("runtime")
            || combined.contains("thread")
            || combined.contains("segfault")
            || combined.contains("null pointer")
        {
            return Self::RuntimeError;
        }

        Self::Other
    }
}

// ─── 严重程度 ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    Critical,
    Moderate,
    Minor,
}

impl Severity {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Critical => "critical",
            Self::Moderate => "moderate",
            Self::Minor => "minor",
        }
    }
}

// ─── 失败记录 ─────────────────────────────────────────────────────────

/// 失败记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FailureRecord {
    /// 失败类型
    pub failure_type: FailureType,
    /// 错误模式签名（用于匹配相似错误）
    pub error_pattern: String,
    /// 触发上下文
    pub context: String,
    /// 已知解决方案
    pub resolution: Option<String>,
    /// 严重程度
    pub severity: Severity,
    /// 发生时间（ISO 8601）
    pub occurred_at: String,
    /// 解决时间
    pub resolved_at: Option<String>,
    /// 关联的工具名
    pub tool_name: Option<String>,
    /// 关联的文件路径
    pub file_paths: Vec<String>,
    /// 数据库中的节点 ID（创建后填充）
    pub node_id: Option<String>,
}

// ─── 失败经验管理器 ───────────────────────────────────────────────────

/// 失败经验管理器
pub struct FailureMemoryManager {
    store: Arc<MemoryGraphStore>,
}

impl FailureMemoryManager {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    // ─── 记录失败 ────────────────────────────────────────────────

    /// 记录失败案例。
    ///
    /// 创建 MemoryNode(kind=Episode)，包含完整失败信息。
    /// 返回创建的 node_id。
    pub fn record_failure(
        &self,
        space_id: &str,
        failure: &FailureRecord,
    ) -> Result<String, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let node_id = uuid::Uuid::new_v4().to_string();
        let version_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let title = format!(
            "失败: {} - {}",
            failure.failure_type.as_str(),
            &failure.error_pattern.chars().take(50).collect::<String>()
        );

        // 标准化错误模式（提取关键特征）
        let normalized_pattern = Self::normalize_error_pattern(&failure.error_pattern);

        // 创建节点
        conn.execute(
            "INSERT INTO memory_nodes
             (id, space_id, kind, title,
              metadata_json,
              created_at, updated_at)
             VALUES (?1, ?2, 'episode', ?3,
                     json_object(
                         'failure_type', ?4,
                         'error_pattern', ?5,
                         'normalized_pattern', ?6,
                         'severity', ?7,
                         'tool_name', ?8,
                         'file_paths', ?9,
                         'resolved', ?10,
                         'resolution', ?11
                     ),
                     ?12, ?13)",
            params![
                node_id,
                space_id,
                title,
                failure.failure_type.as_str(),
                failure.error_pattern,
                normalized_pattern,
                failure.severity.as_str(),
                failure.tool_name,
                serde_json::to_string(&failure.file_paths).unwrap_or_default(),
                failure.resolved_at.is_some(),
                failure.resolution,
                now,
                now,
            ],
        )
        .map_err(|e| Error::Database(e))?;

        // 创建版本
        let content = format!(
            "【失败类型】{}\n【错误模式】{}\n【上下文】{}\n【解决方案】{}\n【严重程度】{}",
            failure.failure_type.as_str(),
            failure.error_pattern,
            failure.context,
            failure
                .resolution
                .as_deref()
                .unwrap_or("暂无"),
            failure.severity.as_str(),
        );

        conn.execute(
            "INSERT INTO memory_versions
             (id, node_id, content, status, embedding_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
            params![version_id, node_id, content, now, now],
        )
        .map_err(|e| Error::Database(e))?;

        tracing::info!(
            node_id = %node_id,
            failure_type = %failure.failure_type.as_str(),
            severity = %failure.severity.as_str(),
            "[FailureMemoryManager] recorded failure"
        );

        Ok(node_id)
    }

    // ─── 查询失败 ────────────────────────────────────────────────

    /// 查找相关失败经验。
    ///
    /// 通过关键词匹配 error_pattern 和 context。
    pub fn find_related_failures(
        &self,
        space_id: &str,
        current_context: &str,
        error_pattern: &str,
        limit: usize,
    ) -> Result<Vec<FailureRecord>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let normalized = Self::normalize_error_pattern(error_pattern);

        // FTS5 搜索（如果可用）
        let mut records = Vec::new();

        // 策略 1: 按 normalized_pattern 精确匹配
        let mut stmt = conn
            .prepare(
                "SELECT n.id, n.metadata_json, v.content, n.created_at, n.updated_at
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'episode'
                   AND json_extract(n.metadata_json, '$.normalized_pattern') = ?2
                 ORDER BY n.updated_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| Error::Database(e))?;

        let exact_matches: Vec<FailureRecord> = stmt
            .query_map(params![space_id, normalized, limit as i64], |row| {
                Self::row_to_failure_record(row)
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        records.extend(exact_matches);

        // 策略 2: 如果精确匹配不足，使用 LIKE 模糊匹配
        if records.len() < limit {
            let remaining = limit - records.len();
            let search_term = format!("%{}%", normalized);

            let mut stmt = conn
                .prepare(
                    "SELECT n.id, n.metadata_json, v.content, n.created_at, n.updated_at
                     FROM memory_nodes n
                     JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                     WHERE n.space_id = ?1
                       AND n.kind = 'episode'
                       AND json_extract(n.metadata_json, '$.error_pattern') LIKE ?2
                       AND n.id NOT IN (
                           SELECT id FROM memory_nodes
                           WHERE json_extract(metadata_json, '$.normalized_pattern') = ?3
                       )
                     ORDER BY n.updated_at DESC
                     LIMIT ?4",
                )
                .map_err(|e| Error::Database(e))?;

            let fuzzy_matches: Vec<FailureRecord> = stmt
                .query_map(
                    params![space_id, search_term, normalized, remaining as i64],
                    |row| Self::row_to_failure_record(row),
                )
                .map_err(|e| Error::Database(e))?
                .filter_map(|r| r.ok())
                .collect();

            records.extend(fuzzy_matches);
        }

        // 策略 3: 按上下文关键词搜索
        if records.len() < limit {
            let keywords: Vec<&str> = current_context
                .split_whitespace()
                .filter(|w| w.len() >= 3)
                .take(5)
                .collect();

            for kw in &keywords {
                if records.len() >= limit {
                    break;
                }
                let search = format!("%{}%", kw);
                let mut stmt = conn
                    .prepare(
                        "SELECT n.id, n.metadata_json, v.content, n.created_at, n.updated_at
                         FROM memory_nodes n
                         JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                         WHERE n.space_id = ?1
                           AND n.kind = 'episode'
                           AND v.content LIKE ?2
                           AND n.id NOT IN rtree((
                               SELECT id FROM memory_nodes WHERE normalized_pattern = ?3
                           ))
                         ORDER BY n.updated_at DESC
                         LIMIT 5",
                    )
                    .map_err(|e| Error::Database(e))?;

                let context_matches: Vec<FailureRecord> = stmt
                    .query_map(params![space_id, search, normalized], |row| {
                        Self::row_to_failure_record(row)
                    })
                    .map_err(|e| Error::Database(e))?
                    .filter_map(|r| r.ok())
                    .collect();

                for rec in context_matches {
                    if !records.iter().any(|r| r.node_id == rec.node_id) {
                        records.push(rec);
                    }
                }
            }
        }

        records.truncate(limit);

        tracing::debug!(
            count = records.len(),
            normalized_pattern = %normalized,
            "[FailureMemoryManager] found related failures"
        );

        Ok(records)
    }

    // ─── 更新解决方案 ────────────────────────────────────────────

    /// 更新失败记录的解决方案。
    pub fn update_resolution(
        &self,
        failure_id: &str,
        new_resolution: &str,
    ) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        // 更新 metadata
        conn.execute(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.resolution', ?1,
                     '$.resolved', json('true')
                 ),
                 updated_at = ?2
             WHERE id = ?3",
            params![new_resolution, now, failure_id],
        )
        .map_err(|e| Error::Database(e))?;

        // 更新版本内容
        conn.execute(
            "UPDATE memory_versions
             SET content = content || ?1 || ?2,
                 updated_at = ?3
             WHERE node_id = ?4 AND status = 'active'",
            params![
                "\n【更新解决方案】",
                new_resolution,
                now,
                failure_id
            ],
        )
        .map_err(|e| Error::Database(e))?;

        tracing::info!(
            failure_id = %failure_id,
            "[FailureMemoryManager] updated resolution"
        );

        Ok(())
    }

    // ─── 统计 ────────────────────────────────────────────────────

    /// 获取失败统计（按类型）
    pub fn get_failure_stats(
        &self,
        space_id: &str,
    ) -> Result<Vec<(String, usize)>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT json_extract(metadata_json, '$.failure_type') as ft,
                        COUNT(*) as cnt
                 FROM memory_nodes
                 WHERE space_id = ?1
                   AND kind = 'episode'
                   AND json_extract(metadata_json, '$.failure_type') IS NOT NULL
                 GROUP BY ft
                 ORDER BY cnt DESC",
            )
            .map_err(|e| Error::Database(e))?;

        let stats: Vec<(String, usize)> = stmt
            .query_map(params![space_id], |row| {
                Ok((
                    row.get::<_, String>(0)
                        .unwrap_or_else(|_| "other".to_string()),
                    row.get::<_, i64>(1).unwrap_or(0) as usize,
                ))
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(stats)
    }

    // ─── 辅助方法 ────────────────────────────────────────────────

    /// 标准化错误模式：提取关键特征，忽略变量名、行号等。
    fn normalize_error_pattern(error: &str) -> String {
        let normalized = error
            .replace(|c: char| c.is_ascii_digit(), "")
            .replace('\'', "'")
            .replace('"', "'")
            .replace("`", "'")
            .split_whitespace()
            .filter(|w| w.len() > 1)
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase();

        // 截断到合理的长度
        if normalized.len() > 200 {
            normalized[..200].to_string()
        } else {
            normalized
        }
    }

    /// 从数据库行构建 FailureRecord
    fn row_to_failure_record(
        row: &rusqlite::Row,
    ) -> rusqlite::Result<FailureRecord> {
        let id: String = row.get(0)?;
        let metadata_str: String = row.get(1)?;
        let _content: String = row.get(2)?;
        let created_at: String = row.get(3)?;
        let _updated_at: String = row.get(4)?;

        let meta: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or_default();

        Ok(FailureRecord {
            failure_type: FailureType::from_str(
                meta.get("failure_type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("other"),
            ),
            error_pattern: meta
                .get("error_pattern")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            context: String::new(), // 从 version content 中提取
            resolution: meta
                .get("resolution")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            severity: match meta
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("moderate")
            {
                "critical" => Severity::Critical,
                "minor" => Severity::Minor,
                _ => Severity::Moderate,
            },
            occurred_at: created_at,
            resolved_at: meta
                .get("resolved")
                .and_then(|v| v.as_bool())
                .filter(|b| *b)
                .map(|_| _updated_at),
            tool_name: meta
                .get("tool_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            file_paths: meta
                .get("file_paths")
                .and_then(|v| v.as_str())
                .map(|s| serde_json::from_str(s).unwrap_or_default())
                .unwrap_or_default(),
            node_id: Some(id),
        })
    }
}
