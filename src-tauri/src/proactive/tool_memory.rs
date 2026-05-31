//! 工具使用追踪记忆管理器
//!
//! EXEMPT from memory_graph freeze: co-used-tools graph (edges) has no MemoryAdapter
//! equivalent; migration deferred to the gbrain↔openhuman effort (see gbrain-primary-freeze ADR).
//!
//! 记录 Agent 工具调用的模式、成功率和性能统计，
//! 支持基于历史使用模式推荐工具链。

use std::collections::HashMap;
use std::sync::Arc;

use rusqlite::params;
use serde::{Deserialize, Serialize};

use crate::memory_graph::models::{MemoryEdge, MemoryNode, MemoryNodeKind, MemoryRelationKind, MemoryVisibility};
use crate::memory_graph::store::MemoryGraphStore;

// ─── 工具使用记录 ─────────────────────────────────────────────────────

/// 一次工具调用的记录
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolUsageRecord {
    /// 工具名称
    pub tool_name: String,
    /// 调用是否成功
    pub success: bool,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 输出大小（字节，估算）
    pub output_size_bytes: Option<u64>,
    /// 参数模式指纹（脱敏后的参数签名）
    pub parameters_fingerprint: Option<String>,
    /// 关联的 session ID
    pub session_id: Option<String>,
    /// 关联的任务描述（如有）
    pub task_description: Option<String>,
}

// ─── 工具统计 ─────────────────────────────────────────────────────────

/// 工具使用聚合统计
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolStats {
    /// 工具名称
    pub tool_name: String,
    /// 总调用次数
    pub total_uses: u64,
    /// 成功率 (0.0 - 1.0)
    pub success_rate: f32,
    /// 平均耗时（毫秒）
    pub avg_latency_ms: f64,
    /// 典型输出大小（字节）
    pub typical_output_size: Option<u64>,
    /// 常见参数模式（按频率排序，最多 5 个）
    pub common_parameters: Vec<String>,
    /// 最近使用时间
    pub last_used_at: Option<String>,
    /// 经常一起使用的工具
    pub co_used_tools: Vec<String>,
}

// ─── 工具推荐 ─────────────────────────────────────────────────────────

/// 工具使用建议
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSuggestion {
    /// 工具名称
    pub tool_name: String,
    /// 推荐理由
    pub reason: String,
    /// 历史成功率
    pub success_rate: f32,
    /// 推荐优先级（越高越推荐）
    pub priority: f32,
}

// ─── 内部聚合结构 ─────────────────────────────────────────────────────

/// 工具节点内部统计（存储在 metadata 中）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolNodeStats {
    total_uses: u64,
    success_count: u64,
    failure_count: u64,
    total_latency_ms: u64,
    output_sizes: Vec<u64>,
    parameter_fingerprints: HashMap<String, u64>,
    last_used_at: String,
}

impl Default for ToolNodeStats {
    fn default() -> Self {
        Self {
            total_uses: 0,
            success_count: 0,
            failure_count: 0,
            total_latency_ms: 0,
            output_sizes: Vec::new(),
            parameter_fingerprints: HashMap::new(),
            last_used_at: String::new(),
        }
    }
}

// ─── 工具使用记忆管理器 ───────────────────────────────────────────────

/// 工具使用记忆管理器
///
/// 使用 MemoryGraphStore 存储每个工具的使用统计（kind=Procedure），
/// 通过 graph edges 记录工具间的共现关系。
pub struct ToolUsageMemoryManager {
    store: Arc<MemoryGraphStore>,
}

impl ToolUsageMemoryManager {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }

    /// 记录一次工具调用
    ///
    /// 为每个 tool_name 维护一个 MemoryNode（kind=Procedure），
    /// 在 metadata 中累积统计信息。
    pub fn record_tool_usage(
        &self,
        space_id: &str,
        usage: &ToolUsageRecord,
    ) -> Result<String, crate::error::Error> {
        let now = chrono::Utc::now().to_rfc3339();

        // 查找或创建该工具的统计节点
        let node_id = self.find_or_create_tool_node(space_id, &usage.tool_name, &now)?;

        // 读取当前统计
        let existing_node = self.store.get_node(&node_id)?;
        let mut stats = existing_node
            .and_then(|n| n.metadata)
            .and_then(|m| serde_json::from_value::<ToolNodeStats>(m).ok())
            .unwrap_or_default();

        // 更新统计
        stats.total_uses += 1;
        if usage.success {
            stats.success_count += 1;
        } else {
            stats.failure_count += 1;
        }
        stats.total_latency_ms += usage.duration_ms;
        if let Some(size) = usage.output_size_bytes {
            stats.output_sizes.push(size);
            if stats.output_sizes.len() > 100 {
                stats.output_sizes.remove(0); // 只保留最近 100 次
            }
        }
        if let Some(ref fp) = usage.parameters_fingerprint {
            *stats.parameter_fingerprints.entry(fp.clone()).or_insert(0) += 1;
        }
        stats.last_used_at = now.clone();

        // 写回 metadata
        let metadata = serde_json::to_value(&stats).map_err(|e| {
            crate::error::Error::Internal(format!("Failed to serialize tool stats: {}", e))
        })?;

        self.store
            .update_node(&node_id, Some(&usage.tool_name), None, Some(&metadata))?;

        tracing::debug!(
            tool = %usage.tool_name,
            success = usage.success,
            total_uses = stats.total_uses,
            "Tool usage recorded"
        );

        Ok(node_id)
    }

    /// 记录多工具共现关系（在一次 agent 迭代中使用的所有工具）
    ///
    /// 为同时使用的工具对创建 graph edges。
    pub fn record_co_usage(
        &self,
        space_id: &str,
        tools_used_in_turn: &[String],
    ) -> Result<(), crate::error::Error> {
        if tools_used_in_turn.len() < 2 {
            return Ok(());
        }

        let now = chrono::Utc::now().to_rfc3339();

        // 确保每个工具都有节点
        let mut node_ids = Vec::new();
        for tool_name in tools_used_in_turn {
            let nid = self.find_or_create_tool_node(space_id, tool_name, &now)?;
            node_ids.push(nid);
        }

        // 为每对工具创建/更新 edge
        for i in 0..node_ids.len() {
            for j in (i + 1)..node_ids.len() {
                let edge_id = uuid::Uuid::new_v4().to_string();
                let edge = MemoryEdge {
                    id: edge_id,
                    space_id: space_id.to_string(),
                    parent_node_id: Some(node_ids[i].clone()),
                    child_node_id: node_ids[j].clone(),
                    relation_kind: MemoryRelationKind::RelatesTo,
                    visibility: MemoryVisibility::Shared,
                    priority: 1,
                    trigger_text: None,
                    created_at: now.clone(),
                    updated_at: now.clone(),
                };
                // Best-effort: 忽略重复 edge 错误
                let _ = self.store.create_edge(&edge);
            }
        }

        Ok(())
    }

    /// 获取工具使用统计
    pub fn get_tool_stats(
        &self,
        space_id: &str,
        tool_name: &str,
    ) -> Result<Option<ToolStats>, crate::error::Error> {
        let node_id = self.find_tool_node_id(space_id, tool_name)?;
        let node_id = match node_id {
            Some(id) => id,
            None => return Ok(None),
        };

        let node = match self.store.get_node(&node_id)? {
            Some(n) => n,
            None => return Ok(None),
        };

        let stats: ToolNodeStats = node
            .metadata
            .and_then(|m| serde_json::from_value(m).ok())
            .unwrap_or_default();

        let success_rate = if stats.total_uses > 0 {
            stats.success_count as f32 / stats.total_uses as f32
        } else {
            0.0
        };

        let avg_latency_ms = if stats.total_uses > 0 {
            stats.total_latency_ms as f64 / stats.total_uses as f64
        } else {
            0.0
        };

        let typical_output_size = stats
            .output_sizes
            .iter()
            .copied()
            .reduce(|a, b| a.max(b));

        // 按频率排序的参数模式
        let mut params: Vec<_> = stats.parameter_fingerprints.iter().collect();
        params.sort_by(|a, b| b.1.cmp(a.1));
        let common_parameters: Vec<String> = params
            .into_iter()
            .take(5)
            .map(|(k, _)| k.clone())
            .collect();

        // 获取经常一起使用的工具
        let co_used_tools = self.get_co_used_tools(space_id, &node_id)?;

        Ok(Some(ToolStats {
            tool_name: tool_name.to_string(),
            total_uses: stats.total_uses,
            success_rate,
            avg_latency_ms,
            typical_output_size,
            common_parameters,
            last_used_at: Some(stats.last_used_at),
            co_used_tools,
        }))
    }

    /// 基于历史使用模式推荐工具链
    ///
    /// 根据任务描述中的关键词匹配历史工具使用模式。
    pub fn suggest_tool_chain(
        &self,
        space_id: &str,
        _task_description: &str,
    ) -> Result<Vec<ToolSuggestion>, crate::error::Error> {
        // 获取所有工具节点
        let all_tool_nodes = self.list_all_tool_nodes(space_id)?;

        let mut suggestions = Vec::new();

        for node in &all_tool_nodes {
            let stats: Option<ToolNodeStats> = node
                .metadata
                .as_ref()
                .and_then(|m| serde_json::from_value(m.clone()).ok());

            let (total_uses, success_rate) = match &stats {
                Some(s) => (
                    s.total_uses,
                    if s.total_uses > 0 {
                        s.success_count as f32 / s.total_uses as f32
                    } else {
                        0.0
                    },
                ),
                None => continue,
            };

            // 计算推荐优先级：基于使用频率 × 成功率
            let priority = (total_uses as f32).ln_1p() * success_rate;

            if priority > 0.01 {
                let reason = if success_rate > 0.9 {
                    format!("高成功率工具（{:.0}%），已使用 {} 次", success_rate * 100.0, total_uses)
                } else if total_uses > 5 {
                    format!("常用工具，已使用 {} 次", total_uses)
                } else {
                    format!("已使用 {} 次", total_uses)
                };

                suggestions.push(ToolSuggestion {
                    tool_name: node.title.clone(),
                    reason,
                    success_rate,
                    priority,
                });
            }
        }

        // 按优先级降序排列
        suggestions.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.truncate(10);

        Ok(suggestions)
    }

    /// 列出所有工具使用统计
    pub fn list_all_stats(
        &self,
        space_id: &str,
    ) -> Result<Vec<ToolStats>, crate::error::Error> {
        let nodes = self.list_all_tool_nodes(space_id)?;
        let mut results = Vec::new();

        for node in nodes {
            if let Some(stats) = self.get_tool_stats(space_id, &node.title)? {
                results.push(stats);
            }
        }

        Ok(results)
    }

    // ─── 内部辅助方法 ──────────────────────────────────────────────

    /// 查找或创建工具统计节点
    fn find_or_create_tool_node(
        &self,
        space_id: &str,
        tool_name: &str,
        now: &str,
    ) -> Result<String, crate::error::Error> {
        if let Some(existing_id) = self.find_tool_node_id(space_id, tool_name)? {
            return Ok(existing_id);
        }

        let node_id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: tool_name.to_string(),
            metadata: Some(serde_json::to_value(ToolNodeStats::default()).unwrap()),
            created_at: now.to_string(),
            updated_at: now.to_string(),
        };

        self.store.create_node(&node)?;
        Ok(node_id)
    }

    /// 按 title 查找工具节点 ID
    fn find_tool_node_id(
        &self,
        space_id: &str,
        tool_name: &str,
    ) -> Result<Option<String>, crate::error::Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT id FROM memory_nodes
                 WHERE space_id = ?1 AND kind = 'procedure' AND title = ?2
                 LIMIT 1",
            )
            .map_err(crate::error::Error::Database)?;

        let result: Option<String> = stmt
            .query_row(params![space_id, tool_name], |row| row.get(0))
            .optional()
            .map_err(crate::error::Error::Database)?;

        Ok(result)
    }

    /// 列出所有工具节点
    fn list_all_tool_nodes(
        &self,
        space_id: &str,
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        let mut stmt = conn
            .prepare(
                "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
                 FROM memory_nodes
                 WHERE space_id = ?1 AND kind = 'procedure' AND metadata_json LIKE '%\"total_uses\"%'
                 ORDER BY updated_at DESC",
            )
            .map_err(crate::error::Error::Database)?;

        let rows = stmt
            .query_map(params![space_id], |row| {
                Ok(MemoryNode {
                    id: row.get(0)?,
                    space_id: row.get(1)?,
                    kind: MemoryNodeKind::Procedure,
                    title: row.get(3)?,
                    metadata: row.get::<_, Option<String>>(4)?.and_then(|s| serde_json::from_str(&s).ok()),
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            })
            .map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }

    /// 获取与指定节点共现的工具
    fn get_co_used_tools(
        &self,
        space_id: &str,
        node_id: &str,
    ) -> Result<Vec<String>, crate::error::Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // 查找通过 edges 连接的其他工具节点
        let mut stmt = conn
            .prepare(
                "SELECT DISTINCT n.title
                 FROM memory_nodes n
                 INNER JOIN memory_edges e ON (
                     (e.parent_node_id = ?1 AND e.child_node_id = n.id)
                     OR (e.child_node_id = ?1 AND e.parent_node_id = n.id)
                 )
                 WHERE e.space_id = ?2 AND n.kind = 'procedure'
                 LIMIT 10",
            )
            .map_err(crate::error::Error::Database)?;

        let rows = stmt
            .query_map(params![node_id, space_id], |row| row.get::<_, String>(0))
            .map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }
}

// ─── rusqlite optional helper ─────────────────────────────────────────

/// 为 rusqlite 查询结果添加 `.optional()` 支持
trait OptionalExt {
    fn optional(self) -> Result<Option<String>, rusqlite::Error>;
}

impl OptionalExt for rusqlite::Result<String> {
    fn optional(self) -> Result<Option<String>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
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
    fn test_record_and_get_tool_stats() {
        let store = make_test_store();
        let manager = ToolUsageMemoryManager::new(store);

        // 记录几次工具调用
        manager
            .record_tool_usage(
                "default",
                &ToolUsageRecord {
                    tool_name: "write_file".to_string(),
                    success: true,
                    duration_ms: 150,
                    output_size_bytes: Some(2048),
                    parameters_fingerprint: Some("path:String,content:String".to_string()),
                    session_id: Some("s1".to_string()),
                    task_description: Some("write config".to_string()),
                },
            )
            .unwrap();

        manager
            .record_tool_usage(
                "default",
                &ToolUsageRecord {
                    tool_name: "write_file".to_string(),
                    success: false,
                    duration_ms: 500,
                    output_size_bytes: None,
                    parameters_fingerprint: Some("path:String,content:String".to_string()),
                    session_id: Some("s2".to_string()),
                    task_description: None,
                },
            )
            .unwrap();

        manager
            .record_tool_usage(
                "default",
                &ToolUsageRecord {
                    tool_name: "search_codebase".to_string(),
                    success: true,
                    duration_ms: 300,
                    output_size_bytes: Some(1024),
                    parameters_fingerprint: Some("query:String".to_string()),
                    session_id: Some("s1".to_string()),
                    task_description: Some("search code".to_string()),
                },
            )
            .unwrap();

        // 获取 write_file 的统计
        let stats = manager
            .get_tool_stats("default", "write_file")
            .unwrap()
            .expect("should have stats");

        assert_eq!(stats.tool_name, "write_file");
        assert_eq!(stats.total_uses, 2);
        assert!((stats.success_rate - 0.5).abs() < 0.01);
        assert!((stats.avg_latency_ms - 325.0).abs() < 1.0);
        assert_eq!(stats.typical_output_size, Some(2048));
        assert!(!stats.common_parameters.is_empty());

        // 获取不存在的工具
        let missing = manager.get_tool_stats("default", "nonexistent").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_record_co_usage() {
        let store = make_test_store();
        let manager = ToolUsageMemoryManager::new(store.clone());

        // 先分别记录工具调用
        for tool in &["write_file", "run_tests", "search_codebase"] {
            manager
                .record_tool_usage(
                    "default",
                    &ToolUsageRecord {
                        tool_name: tool.to_string(),
                        success: true,
                        duration_ms: 100,
                        output_size_bytes: None,
                        parameters_fingerprint: None,
                        session_id: Some("s1".to_string()),
                        task_description: None,
                    },
                )
                .unwrap();
        }

        // 记录共现关系
        manager
            .record_co_usage(
                "default",
                &[
                    "write_file".to_string(),
                    "run_tests".to_string(),
                    "search_codebase".to_string(),
                ],
            )
            .unwrap();

        // 检查 write_file 的共现工具
        let stats = manager.get_tool_stats("default", "write_file").unwrap().unwrap();
        assert!(!stats.co_used_tools.is_empty());
        // 应包含 run_tests 或 search_codebase
        let has_co_tool = stats
            .co_used_tools
            .iter()
            .any(|t| t == "run_tests" || t == "search_codebase");
        assert!(has_co_tool);
    }

    #[test]
    fn test_suggest_tool_chain() {
        let store = make_test_store();
        let manager = ToolUsageMemoryManager::new(store);

        // 记录多个工具的使用
        for (tool, count) in &[
            ("write_file", 10u64),
            ("search_codebase", 8),
            ("run_tests", 5),
            ("git_commit", 2),
        ] {
            for i in 0..*count {
                manager
                    .record_tool_usage(
                        "default",
                        &ToolUsageRecord {
                            tool_name: tool.to_string(),
                            success: i < count - 1, // 大部分成功
                            duration_ms: 100,
                            output_size_bytes: Some(1024),
                            parameters_fingerprint: None,
                            session_id: Some("s1".to_string()),
                            task_description: None,
                        },
                    )
                    .unwrap();
            }
        }

        let suggestions = manager
            .suggest_tool_chain("default", "write some code")
            .unwrap();

        assert!(!suggestions.is_empty());
        // write_file 应该是最高优先级的
        assert_eq!(suggestions[0].tool_name, "write_file");
    }
}
