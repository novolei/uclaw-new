//! 代码记忆库
//!
//! 存储生成的代码片段，支持按语义搜索代码、检测重复代码生成。
//!
//! ## 设计
//! ```text
//! Agent 生成代码 → store_code_snippet()
//!     ├─ language: 编程语言
//!     ├─ code: 代码内容
//!     ├─ purpose: 用途描述
//!     └─ file_path: 关联文件
//!     ↓
//! 创建 MemoryNode(kind=Procedure)
//!     ├─ metadata: language, purpose, file_path, dependencies
//!     └─ version.content = 带标注的代码块
//!
//! 搜索代码 → search_code() → FTS5 + 关键词匹配
//! 重复检测 → detect_duplicate() → Jaccard 相似度
//! ```

use std::collections::HashSet;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::error::Error;
use crate::memory_graph::store::MemoryGraphStore;

// ─── 代码片段 ─────────────────────────────────────────────────────────

/// 代码片段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeSnippet {
    /// 编程语言
    pub language: String,
    /// 代码内容
    pub code: String,
    /// 用途描述
    pub purpose: String,
    /// 关联文件路径
    pub file_path: Option<String>,
    /// 依赖列表
    pub dependencies: Vec<String>,
    /// 复用次数
    pub reuse_count: u32,
    /// 数据库中的节点 ID
    pub node_id: Option<String>,
    /// 创建时间
    pub created_at: Option<String>,
}

// ─── 搜索匹配 ─────────────────────────────────────────────────────────

/// 代码搜索结果
#[derive(Debug, Clone)]
pub struct CodeMatch {
    /// 匹配的代码片段
    pub snippet: CodeSnippet,
    /// 相似度分数（0-1）
    pub score: f32,
    /// 匹配原因
    pub reason: String,
}

// ─── 代码记忆管理器 ───────────────────────────────────────────────────

/// 代码记忆管理器
pub struct CodeMemoryManager {
    store: Arc<MemoryGraphStore>,
}

impl CodeMemoryManager {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    // ─── 存储代码 ────────────────────────────────────────────────

    /// 存储代码片段。
    ///
    /// 创建 MemoryNode(kind=Procedure)，包含完整代码和元数据。
    pub fn store_code_snippet(
        &self,
        space_id: &str,
        snippet: &CodeSnippet,
    ) -> Result<String, Error> {
        // 先检查是否已存在高度相似的代码
        if let Some(existing) = self.detect_duplicate(space_id, snippet)? {
            // 已存在相似代码 → 更新复用计数
            if let Some(ref node_id) = existing.node_id {
                self.increment_reuse_count(node_id)?;
                tracing::info!(
                    node_id = %node_id,
                    purpose = %snippet.purpose,
                    "[CodeMemoryManager] incremented reuse count for existing snippet"
                );
                return Ok(node_id.clone());
            }
        }

        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let node_id = uuid::Uuid::new_v4().to_string();
        let version_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let title = format!(
            "代码: {} - {}",
            snippet.language,
            &snippet.purpose.chars().take(60).collect::<String>()
        );

        // 创建节点
        conn.execute(
            "INSERT INTO memory_nodes
             (id, space_id, kind, title,
              metadata_json,
              created_at, updated_at)
             VALUES (?1, ?2, 'procedure', ?3,
                     json_object(
                         'language', ?4,
                         'purpose', ?5,
                         'file_path', ?6,
                         'dependencies', ?7,
                         'reuse_count', 1,
                         'code_size', ?8,
                         'is_code_snippet', 'true'
                     ),
                     ?9, ?10)",
            params![
                node_id,
                space_id,
                title,
                snippet.language,
                snippet.purpose,
                snippet.file_path,
                serde_json::to_string(&snippet.dependencies).unwrap_or_default(),
                snippet.code.len() as i64,
                now,
                now,
            ],
        )
        .map_err(|e| Error::Database(e))?;

        // 创建版本（带标注的代码块）
        let content = format!(
            "// Language: {}\n// Purpose: {}\n// File: {}\n// Dependencies: {:?}\n\n{}",
            snippet.language,
            snippet.purpose,
            snippet.file_path.as_deref().unwrap_or("N/A"),
            snippet.dependencies,
            snippet.code
        );

        conn.execute(
            "INSERT INTO memory_versions
             (id, node_id, content, status, embedding_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'active', NULL, ?4, ?5)",
            params![version_id, node_id, content, now, now],
        )
        .map_err(|e| Error::Database(e))?;

        // 创建关键词索引（便于快速搜索）
        for keyword in Self::extract_code_keywords(&snippet.purpose, &snippet.code) {
            let kw_id = uuid::Uuid::new_v4().to_string();
            let _ = conn.execute(
                "INSERT OR IGNORE INTO memory_keywords
                 (id, space_id, node_id, keyword, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![kw_id, space_id, node_id, keyword, now],
            );
        }

        tracing::info!(
            node_id = %node_id,
            language = %snippet.language,
            code_size = snippet.code.len(),
            "[CodeMemoryManager] stored code snippet"
        );

        Ok(node_id)
    }

    // ─── 搜索代码 ────────────────────────────────────────────────

    /// 按语义搜索代码片段。
    pub fn search_code(
        &self,
        space_id: &str,
        query: &str,
        language: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CodeMatch>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut matches = Vec::new();

        // 策略 1: 关键词匹配（通过 memory_keywords 表）
        let keywords = Self::extract_code_keywords(query, "");
        for kw in &keywords {
            let mut stmt = conn
                .prepare(
                    "SELECT DISTINCT n.id, n.metadata_json, v.content, n.created_at
                     FROM memory_keywords mk
                     JOIN memory_nodes n ON n.id = mk.node_id
                     JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                     WHERE mk.space_id = ?1
                       AND n.kind = 'procedure'
                       AND json_extract(n.metadata_json, '$.is_code_snippet') = 'true'
                       AND mk.keyword LIKE ?2
                     ORDER BY n.updated_at DESC
                     LIMIT ?3",
                )
                .map_err(|e| Error::Database(e))?;

            let kw_matches: Vec<CodeMatch> = stmt
                .query_map(
                    params![space_id, format!("%{}%", kw), limit as i64],
                    |row| Self::row_to_code_match(row, query),
                )
                .map_err(|e| Error::Database(e))?
                .filter_map(|r| r.ok())
                .collect();

            for m in kw_matches {
                if !matches.iter().any(|existing: &CodeMatch| {
                    existing.snippet.node_id == m.snippet.node_id
                }) {
                    matches.push(m);
                }
            }
        }

        // 策略 2: FTS5 全文搜索（如果关键词匹配不足）
        if matches.len() < limit {
            let search_terms: Vec<&str> = query
                .split_whitespace()
                .filter(|w| w.len() >= 2)
                .collect();

            for term in &search_terms {
                if matches.len() >= limit {
                    break;
                }

                let mut stmt = conn
                    .prepare(
                        "SELECT n.id, n.metadata_json, v.content, n.created_at
                         FROM memory_nodes n
                         JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                         WHERE n.space_id = ?1
                           AND n.kind = 'procedure'
                           AND json_extract(n.metadata_json, '$.is_code_snippet') = 'true'
                           AND (v.content LIKE ?2 OR n.title LIKE ?2)
                         ORDER BY n.updated_at DESC
                         LIMIT 10",
                    )
                    .map_err(|e| Error::Database(e))?;

                let fts_matches: Vec<CodeMatch> = stmt
                    .query_map(
                        params![space_id, format!("%{}%", term)],
                        |row| Self::row_to_code_match(row, query),
                    )
                    .map_err(|e| Error::Database(e))?
                    .filter_map(|r| r.ok())
                    .collect();

                for m in fts_matches {
                    if !matches.iter().any(|existing: &CodeMatch| {
                        existing.snippet.node_id == m.snippet.node_id
                    }) {
                        matches.push(m);
                    }
                }
            }
        }

        // 按语言过滤
        if let Some(lang) = language {
            matches.retain(|m| {
                m.snippet.language.to_lowercase() == lang.to_lowercase()
            });
        }

        // 按分数排序
        matches.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        matches.truncate(limit);

        Ok(matches)
    }

    // ─── 重复检测 ────────────────────────────────────────────────

    /// 检测重复代码生成。
    ///
    /// 使用 Jaccard 相似度在 token 级别比较代码。
    /// 如果相似度 > 0.85，返回已存在的代码片段。
    pub fn detect_duplicate(
        &self,
        space_id: &str,
        new_snippet: &CodeSnippet,
    ) -> Result<Option<CodeSnippet>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        // 先按语言和用途关键词快速过滤
        let mut stmt = conn
            .prepare(
                "SELECT n.id, n.metadata_json, v.content, n.created_at
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'procedure'
                   AND json_extract(n.metadata_json, '$.is_code_snippet') = 'true'
                   AND json_extract(n.metadata_json, '$.language') = ?2
                 ORDER BY n.updated_at DESC
                 LIMIT 20",
            )
            .map_err(|e| Error::Database(e))?;

        let candidates: Vec<(String, String, String)> = stmt
            .query_map(params![space_id, new_snippet.language], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        // 计算 Jaccard 相似度
        let new_tokens = tokenize_code(&new_snippet.code);
        let threshold = 0.85;

        for (node_id, metadata_str, content) in &candidates {
            // 从版本内容中提取代码部分
            let existing_code = extract_code_from_version(content);

            if existing_code.is_empty() {
                continue;
            }

            let existing_tokens = tokenize_code(&existing_code);
            let similarity = jaccard_similarity(&new_tokens, &existing_tokens);

            if similarity > threshold {
                let meta: serde_json::Value =
                    serde_json::from_str(metadata_str).unwrap_or_default();

                return Ok(Some(CodeSnippet {
                    language: meta
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    code: existing_code,
                    purpose: meta
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    file_path: meta
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    dependencies: meta
                        .get("dependencies")
                        .and_then(|v| v.as_str())
                        .map(|s| serde_json::from_str(s).unwrap_or_default())
                        .unwrap_or_default(),
                    reuse_count: meta
                        .get("reuse_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u32,
                    node_id: Some(node_id.clone()),
                    created_at: None,
                }));
            }
        }

        Ok(None)
    }

    // ─── 查询 ────────────────────────────────────────────────────

    /// 列出指定语言的所有代码片段
    pub fn list_by_language(
        &self,
        space_id: &str,
        language: &str,
        limit: usize,
    ) -> Result<Vec<CodeSnippet>, Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT n.id, n.metadata_json, v.content, n.created_at
                 FROM memory_nodes n
                 JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
                 WHERE n.space_id = ?1
                   AND n.kind = 'procedure'
                   AND json_extract(n.metadata_json, '$.is_code_snippet') = 'true'
                   AND json_extract(n.metadata_json, '$.language') = ?2
                 ORDER BY n.updated_at DESC
                 LIMIT ?3",
            )
            .map_err(|e| Error::Database(e))?;

        let snippets: Vec<CodeSnippet> = stmt
            .query_map(params![space_id, language, limit as i64], |row| {
                let id: String = row.get(0)?;
                let metadata_str: String = row.get(1)?;
                let content: String = row.get(2)?;
                let created_at: String = row.get(3)?;
                let meta: serde_json::Value =
                    serde_json::from_str(&metadata_str).unwrap_or_default();

                Ok(CodeSnippet {
                    language: meta
                        .get("language")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    code: extract_code_from_version(&content),
                    purpose: meta
                        .get("purpose")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string(),
                    file_path: meta
                        .get("file_path")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string()),
                    dependencies: meta
                        .get("dependencies")
                        .and_then(|v| v.as_str())
                        .map(|s| serde_json::from_str(s).unwrap_or_default())
                        .unwrap_or_default(),
                    reuse_count: meta
                        .get("reuse_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(1) as u32,
                    node_id: Some(id),
                    created_at: Some(created_at),
                })
            })
            .map_err(|e| Error::Database(e))?
            .filter_map(|r| r.ok())
            .collect();

        Ok(snippets)
    }

    // ─── 辅助方法 ────────────────────────────────────────────────

    /// 增加复用计数
    fn increment_reuse_count(&self, node_id: &str) -> Result<(), Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;

        let now = chrono::Utc::now().to_rfc3339();

        conn.execute(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.reuse_count',
                     COALESCE(json_extract(metadata_json, '$.reuse_count'), 0) + 1
                 ),
                 updated_at = ?1
             WHERE id = ?2",
            params![now, node_id],
        )
        .map_err(|e| Error::Database(e))?;

        Ok(())
    }

    /// 提取代码关键词（用于索引和搜索）
    fn extract_code_keywords(purpose: &str, code: &str) -> Vec<String> {
        let mut keywords = HashSet::new();

        // 从用途描述中提取
        for word in purpose.split_whitespace() {
            let cleaned = word
                .trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase();
            if cleaned.len() >= 2 {
                keywords.insert(cleaned);
            }
        }

        // 从代码中提取标识符（前 20 个唯一标识符）
        let mut id_count = 0;
        for word in code.split(|c: char| !c.is_alphanumeric() && c != '_') {
            let cleaned = word.trim().to_lowercase();
            if cleaned.len() >= 3 && !is_common_keyword(&cleaned) {
                if keywords.insert(cleaned) {
                    id_count += 1;
                    if id_count >= 20 {
                        break;
                    }
                }
            }
        }

        keywords.into_iter().collect()
    }

    /// 从数据库行构建 CodeMatch
    fn row_to_code_match(
        row: &rusqlite::Row,
        query: &str,
    ) -> rusqlite::Result<CodeMatch> {
        let id: String = row.get(0)?;
        let metadata_str: String = row.get(1)?;
        let content: String = row.get(2)?;
        let created_at: String = row.get(3)?;
        let meta: serde_json::Value =
            serde_json::from_str(&metadata_str).unwrap_or_default();

        let code = extract_code_from_version(&content);
        let purpose = meta
            .get("purpose")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // 简单评分：基于关键词匹配度
        let query_lower = query.to_lowercase();
        let score = if purpose.to_lowercase().contains(&query_lower) {
            0.9
        } else if content.to_lowercase().contains(&query_lower) {
            0.7
        } else {
            0.3
        };

        Ok(CodeMatch {
            snippet: CodeSnippet {
                language: meta
                    .get("language")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                code,
                purpose: purpose.to_string(),
                file_path: meta
                    .get("file_path")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                dependencies: meta
                    .get("dependencies")
                    .and_then(|v| v.as_str())
                    .map(|s| serde_json::from_str(s).unwrap_or_default())
                    .unwrap_or_default(),
                reuse_count: meta
                    .get("reuse_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(1) as u32,
                node_id: Some(id),
                created_at: Some(created_at),
            },
            score,
            reason: format!("matched query: {}", query),
        })
    }
}

// ─── 辅助函数 ─────────────────────────────────────────────────────────

/// 将代码 token 化为集合（用于 Jaccard 相似度）
fn tokenize_code(code: &str) -> HashSet<String> {
    code.split(|c: char| !c.is_alphanumeric() && c != '_')
        .map(|t| t.trim().to_lowercase())
        .filter(|t| t.len() >= 2 && !is_common_keyword(t))
        .collect()
}

/// 常见编程语言关键字（不应作为搜索索引）
fn is_common_keyword(word: &str) -> bool {
    matches!(
        word,
        "fn" | "let" | "mut" | "pub" | "use" | "mod" | "struct" | "enum"
            | "impl" | "for" | "while" | "if" | "else" | "match"
            | "return" | "true" | "false" | "self" | "static"
            | "type" | "where" | "async" | "await" | "move" | "ref"
            | "function" | "var" | "const" | "class" | "import" | "export"
            | "def" | "from" | "this" | "new" | "try" | "catch" | "throw"
            | "int" | "string" | "void" | "bool" | "char" | "float"
            | "double" | "long" | "short" | "byte" | "auto"
    )
}

/// 从版本内容中提取纯代码部分
fn extract_code_from_version(content: &str) -> String {
    // 跳过注释标注行（// Language:, // Purpose:, // File:, etc.）
    let code_start = content
        .find("\n\n")
        .map(|p| p + 2)
        .unwrap_or(0);

    content[code_start..].trim().to_string()
}

/// 计算两个 token 集合的 Jaccard 相似度
fn jaccard_similarity(a: &HashSet<String>, b: &HashSet<String>) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }

    let intersection = a.intersection(b).count();
    let union = a.union(b).count();

    if union == 0 {
        return 0.0;
    }

    intersection as f32 / union as f32
}
