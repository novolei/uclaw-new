//! 工具使用追踪记忆管理器
//!
//! EXEMPT from memory_graph freeze: co-used-tools graph (edges) has no MemoryAdapter
//! equivalent; migration deferred to the gbrain↔openhuman effort (see gbrain-primary-freeze ADR).
//!
//! 记录 Agent 工具调用的模式、成功率和性能统计，
//! 支持基于历史使用模式推荐工具链。

use std::sync::Arc;

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

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

// ─── 工具使用记忆管理器 ───────────────────────────────────────────────

/// 工具使用记忆管理器
///
/// 使用 bucket_seal MemoryAdapter（tool_stats facade + edges）存储每个工具的使用统计。
/// `suggest_tool_chain` 通过 `tool_transitions` 有向加权图给出后继工具推荐（openhuman-E）。
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

    /// openhuman-E — suggest the tools that most often, most recently, and most
    /// successfully follow the space's most-recent tool. Reads the directed
    /// `tool_transitions` graph. Score = count × recency_decay × success_rate.
    pub fn suggest_tool_chain(
        &self,
        space_id: &str,
        _task_description: &str,
        recency_half_life_days: f64,
    ) -> Result<Vec<ToolSuggestion>, crate::error::Error> {
        let conn = self
            .store
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        let last_tool: Option<String> = conn
            .query_row(
                "SELECT t.tool_name
                 FROM agent_turns t
                 JOIN agent_sessions s ON s.id = t.session_id
                 WHERE s.space_id = ?1 AND t.role = 'tool' AND t.tool_name IS NOT NULL
                 ORDER BY t.created_at DESC
                 LIMIT 1",
                rusqlite::params![space_id],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(crate::error::Error::Database)?;

        let Some(last) = last_tool else {
            return Ok(Vec::new());
        };

        let rows = crate::memory_graph::tool_transitions::top_transitions_from(&conn, space_id, &last, 50)
            .map_err(crate::error::Error::Database)?;

        let now_ms = chrono::Utc::now().timestamp_millis();
        let mut suggestions: Vec<ToolSuggestion> = rows
            .into_iter()
            .map(|r| {
                let success_rate = if r.count > 0 {
                    r.success_count as f32 / r.count as f32
                } else {
                    0.0
                };
                let recency = if recency_half_life_days <= 0.0 {
                    1.0_f64
                } else {
                    let age_days = ((now_ms - r.last_seen_ms).max(0) as f64) / 86_400_000.0;
                    (-(age_days / recency_half_life_days)).exp()
                };
                let priority = (r.count as f64) * recency * (success_rate as f64);
                ToolSuggestion {
                    tool_name: r.to_tool.clone(),
                    reason: format!(
                        "常接在 {} 之后（{} 次, 成功率 {:.0}%）",
                        last, r.count, success_rate * 100.0
                    ),
                    success_rate,
                    priority: priority as f32,
                }
            })
            .filter(|s| s.priority > 0.0)
            .collect();

        suggestions.sort_by(|a, b| b.priority.partial_cmp(&a.priority).unwrap_or(std::cmp::Ordering::Equal));
        suggestions.truncate(10);
        Ok(suggestions)
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
        crate::db::migrations::run(&conn).unwrap();
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

    #[tokio::test]
    async fn suggest_tool_chain_ranks_by_weighted_score() {
        let store = make_test_store();
        let manager = make_manager(store.clone());
        let now_ms = chrono::Utc::now().timestamp_millis();
        {
            let c = store.conn.lock().unwrap();
            c.execute("INSERT INTO agent_sessions (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) VALUES ('s1','default','t','{}',0,0,0,0,0)", []).unwrap();
            // Most-recent tool turn is 'read' (created_at = now_ms)
            c.execute(
                "INSERT INTO agent_turns (id, session_id, turn_index, role, content, tool_name, tool_args, tool_result, reasoning, is_error, duration_ms, created_at) VALUES ('t1','s1',1,'tool',NULL,'read',NULL,NULL,NULL,0,0,?1)",
                rusqlite::params![now_ms],
            ).unwrap();
            // 10 read→edit transitions all successful, recent timestamps
            for ts in 0..10i64 { crate::memory_graph::tool_transitions::upsert_transition(&c,"default","read","edit",true, now_ms - ts * 1000).unwrap(); }
            // 10 read→grep transitions, only 2 successful, older timestamps (1 day ago)
            for ts in 0..10i64 { crate::memory_graph::tool_transitions::upsert_transition(&c,"default","read","grep", ts<2, now_ms - 86_400_000 - ts * 1000).unwrap(); }
        }
        let s = manager.suggest_tool_chain("default", "anything", 30.0).unwrap();
        assert!(!s.is_empty());
        assert_eq!(s[0].tool_name, "edit"); // higher success + more recent than grep
    }

    #[tokio::test]
    async fn suggest_tool_chain_empty_when_no_prior_tool() {
        let store = make_test_store();
        let manager = make_manager(store);
        let s = manager.suggest_tool_chain("default", "x", 30.0).unwrap();
        assert!(s.is_empty());
    }
}
