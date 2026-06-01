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

use crate::memory_graph::models::{MemoryNode, MemoryNodeKind};
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
/// 使用 bucket_seal MemoryAdapter（tool_stats facade + edges）存储每个工具的使用统计。
/// `suggest_tool_chain` 仍通过 MemoryGraphStore 枚举 Procedure 节点（migration deferred）。
pub struct ToolUsageMemoryManager {
    store: Arc<MemoryGraphStore>,
    /// Adapter for tool_stats facade writes (unconditional — bucket_seal backend).
    adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
}

impl ToolUsageMemoryManager {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
    ) -> Self {
        Self { store, adapter }
    }

    /// 记录一次工具调用（unconditional — writes to tool_stats facade via bucket_seal adapter）
    pub async fn record_tool_usage(
        &self,
        space_id: &str,
        usage: &ToolUsageRecord,
    ) -> Result<String, crate::error::Error> {
        use crate::memory_adapter::tool_stats::{self, ToolStatsRecord};
        let mut rec = tool_stats::get_stats(&self.adapter, space_id, &usage.tool_name)
            .await
            .ok()
            .flatten()
            .unwrap_or_else(|| ToolStatsRecord {
                space: space_id.into(),
                tool_name: usage.tool_name.clone(),
                ..Default::default()
            });
        rec.total_uses += 1;
        if usage.success {
            rec.success_count += 1;
        } else {
            rec.failure_count += 1;
        }
        rec.total_latency_ms += usage.duration_ms;
        if let Some(sz) = usage.output_size_bytes {
            rec.output_sizes.push(sz);
            if rec.output_sizes.len() > 100 {
                rec.output_sizes.remove(0); // 只保留最近 100 次
            }
        }
        if let Some(fp) = &usage.parameters_fingerprint {
            *rec.parameter_fingerprints.entry(fp.clone()).or_insert(0) += 1;
        }
        rec.last_used_at = chrono::Utc::now().to_rfc3339();
        if let Err(e) = tool_stats::put_stats(&self.adapter, &rec).await {
            tracing::warn!(
                error = %format!("{e:#}"),
                "tool_stats put failed, result not persisted"
            );
        }
        tracing::debug!(
            tool = %usage.tool_name,
            success = usage.success,
            total_uses = rec.total_uses,
            "Tool usage recorded (tool_stats facade)"
        );
        Ok(format!("tool_stats:{}:{}", space_id, usage.tool_name))
    }

    /// 记录多工具共现关系（unconditional — writes to edges facade via bucket_seal adapter）
    pub async fn record_co_usage(
        &self,
        space_id: &str,
        tools_used_in_turn: &[String],
    ) -> Result<(), crate::error::Error> {
        if tools_used_in_turn.len() < 2 {
            return Ok(());
        }

        for i in 0..tools_used_in_turn.len() {
            for j in (i + 1)..tools_used_in_turn.len() {
                if let Err(e) = crate::memory_adapter::edges::relate(
                    &self.adapter,
                    &tools_used_in_turn[i],
                    &tools_used_in_turn[j],
                    "co_used",
                )
                .await
                {
                    tracing::warn!(
                        error = %format!("{e:#}"),
                        "co_used relate failed"
                    );
                }
            }
        }
        Ok(())
    }

    /// 获取工具使用统计（unconditional — reads from tool_stats facade + edges via bucket_seal adapter）
    pub async fn get_tool_stats(
        &self,
        space_id: &str,
        tool_name: &str,
    ) -> Result<Option<ToolStats>, crate::error::Error> {
        use crate::memory_adapter::{edges, tool_stats};
        let rec = match tool_stats::get_stats(&self.adapter, space_id, tool_name).await {
            Ok(Some(r)) => r,
            Ok(None) => return Ok(None),
            Err(e) => {
                return Err(crate::error::Error::Internal(format!(
                    "tool_stats::get_stats failed: {e:#}"
                )))
            }
        };
        let total = rec.total_uses;
        let success_rate = if total > 0 {
            rec.success_count as f32 / total as f32
        } else {
            0.0
        };
        let avg_latency_ms = if total > 0 {
            rec.total_latency_ms as f64 / total as f64
        } else {
            0.0
        };
        let typical_output_size =
            rec.output_sizes.iter().copied().reduce(|a, b| a.max(b));
        let mut params: Vec<_> = rec.parameter_fingerprints.iter().collect();
        params.sort_by(|a, b| b.1.cmp(a.1));
        let common_parameters: Vec<String> =
            params.into_iter().take(5).map(|(k, _)| k.clone()).collect();
        let co_used_tools =
            edges::neighbors(&self.adapter, tool_name, Some("co_used"))
                .await
                .unwrap_or_default();
        Ok(Some(ToolStats {
            tool_name: tool_name.to_string(),
            total_uses: total,
            success_rate,
            avg_latency_ms,
            typical_output_size,
            common_parameters,
            last_used_at: if rec.last_used_at.is_empty() {
                None
            } else {
                Some(rec.last_used_at.clone())
            },
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
    pub async fn list_all_stats(
        &self,
        space_id: &str,
    ) -> Result<Vec<ToolStats>, crate::error::Error> {
        let nodes = self.list_all_tool_nodes(space_id)?;
        let mut results = Vec::new();

        for node in nodes {
            if let Some(stats) = self.get_tool_stats(space_id, &node.title).await? {
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
    use async_trait::async_trait;
    use rusqlite::Connection;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── Minimal in-process adapter for tool_memory tests ─────────────────

    struct InMemoryAdapter {
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Arc<dyn MemoryAdapter> {
            Arc::new(Self { store: Mutex::new(HashMap::new()) })
        }
    }

    #[async_trait]
    impl MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str { "in_memory_test" }

        async fn store(
            &self, namespace: &str, key: &str, content: &str,
            category: MemoryCategory, session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = MemoryEntry {
                id: key.to_string(), key: key.to_string(),
                content: content.to_string(),
                namespace: Some(namespace.to_string()),
                category, timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: session_id.map(String::from), score: None,
            };
            self.store.lock().unwrap().insert((namespace.to_string(), key.to_string()), entry);
            Ok(())
        }

        async fn recall(&self, _q: &str, _l: usize, _o: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(self.store.lock().unwrap().get(&(namespace.to_string(), key.to_string())).cloned())
        }

        async fn list(&self, namespace: Option<&str>, _c: Option<&MemoryCategory>, _s: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            Ok(store.values().filter(|e| match namespace {
                Some(ns) => e.namespace.as_deref() == Some(ns),
                None => true,
            }).cloned().collect())
        }

        async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
            Ok(self.store.lock().unwrap().remove(&(namespace.to_string(), key.to_string())).is_some())
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

    fn make_test_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .unwrap();
        let conn = Arc::new(std::sync::Mutex::new(conn));
        Arc::new(MemoryGraphStore::new(conn))
    }

    /// Construct a manager with an in-memory adapter (unconditional bucket_seal path).
    fn make_manager(store: Arc<MemoryGraphStore>) -> ToolUsageMemoryManager {
        ToolUsageMemoryManager::new(store, InMemoryAdapter::new())
    }

    #[tokio::test]
    async fn test_record_and_get_tool_stats() {
        let store = make_test_store();
        let manager = make_manager(store);

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
            .await
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
            .await
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
            .await
            .unwrap();

        // 获取 write_file 的统计
        let stats = manager
            .get_tool_stats("default", "write_file")
            .await
            .unwrap()
            .expect("should have stats");

        assert_eq!(stats.tool_name, "write_file");
        assert_eq!(stats.total_uses, 2);
        assert!((stats.success_rate - 0.5).abs() < 0.01);
        assert!((stats.avg_latency_ms - 325.0).abs() < 1.0);
        assert_eq!(stats.typical_output_size, Some(2048));
        assert!(!stats.common_parameters.is_empty());

        // 获取不存在的工具
        let missing = manager.get_tool_stats("default", "nonexistent").await.unwrap();
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn test_record_co_usage() {
        let store = make_test_store();
        let manager = make_manager(store.clone());

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
                .await
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
            .await
            .unwrap();

        // 检查 write_file 的共现工具
        let stats = manager.get_tool_stats("default", "write_file").await.unwrap().unwrap();
        assert!(!stats.co_used_tools.is_empty());
        // 应包含 run_tests 或 search_codebase
        let has_co_tool = stats
            .co_used_tools
            .iter()
            .any(|t| t == "run_tests" || t == "search_codebase");
        assert!(has_co_tool);
    }

    /// `suggest_tool_chain` still reads from memory_graph Procedure nodes (migration deferred).
    /// Since `record_tool_usage` now writes unconditionally to the adapter (not memory_graph),
    /// no Procedure nodes are seeded and suggestions return empty — which is the expected
    /// behaviour until `suggest_tool_chain` is ported to the adapter facade.
    #[tokio::test]
    async fn test_suggest_tool_chain_returns_empty_until_ported() {
        let store = make_test_store();
        let manager = make_manager(store);

        // Writes go to adapter; memory_graph has no Procedure nodes.
        manager
            .record_tool_usage(
                "default",
                &ToolUsageRecord {
                    tool_name: "write_file".to_string(),
                    success: true,
                    duration_ms: 100,
                    output_size_bytes: Some(1024),
                    parameters_fingerprint: None,
                    session_id: None,
                    task_description: None,
                },
            )
            .await
            .unwrap();

        let suggestions = manager.suggest_tool_chain("default", "write some code").unwrap();
        // suggest_tool_chain reads memory_graph Procedure nodes (not yet ported);
        // no nodes seeded → empty result is correct.
        assert!(suggestions.is_empty());
    }
}
