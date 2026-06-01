//! memU 工具集 — 记忆检索、待办事项、用户确认
//!
//! 为 agent 提供记忆相关的工具能力，包括：
//! - `memu_memory`       — 检索用户长期记忆 (via bucket_seal)
//! - `memu_todos`        — 获取用户待办事项列表 (via bucket_seal)
//! - `wait_user_confirm` — 请求用户确认破坏性操作（主动模式专用）

use std::sync::Arc;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::memory_adapter::MemoryAdapter as _;
use crate::memory_bucket_seal::BucketSealAdapter;

// ═══════════════════════════════════════════════════════════════════════
// 1. memu_memory — 记忆检索工具
// ═══════════════════════════════════════════════════════════════════════

/// 记忆检索工具（bucket_seal 后端）
///
/// 当 agent 需要了解用户的偏好、历史信息、身份特征等时，
/// 通过此工具从 bucket_seal 存储中检索相关记忆。
pub struct MemuMemoryTool {
    adapter: Arc<BucketSealAdapter>,
    /// Workspace / space id used as the namespace for recall.
    /// Hard-coded `"default"` today since the agent loop hard-codes the
    /// same; will move to per-workspace once dynamic space_id lands.
    space_id: String,
}

impl MemuMemoryTool {
    pub fn new(adapter: Arc<BucketSealAdapter>) -> Self {
        Self {
            adapter,
            space_id: "default".to_string(),
        }
    }

    /// Override the default space_id used for the adapter fast path.
    /// Defaults to `"default"` (matches the agent loop hard-coding).
    #[allow(dead_code)]
    pub fn with_space_id(mut self, space_id: impl Into<String>) -> Self {
        self.space_id = space_id.into();
        self
    }
}

/// 记忆检索输入参数
#[derive(Debug, Deserialize)]
struct MemuMemoryInput {
    /// 查询文本，描述想了解的用户信息
    query: String,
    /// 最大返回记忆数量，默认 10
    #[serde(default = "default_limit")]
    limit: usize,
    /// Bundle 6 — opt-in for LLM-backed category enrichment.
    /// Kept for schema compatibility; ignored by the bucket_seal path.
    #[serde(default)]
    enrich_categories: bool,
}

fn default_limit() -> usize {
    10
}

const MEMU_RETRIEVE_MAX_LIMIT: usize = 20;
const MEMU_LIST_MAX_LIMIT: usize = 25;

/// Bundle 6 — hard wall-clock cap on the memu_memory tool itself.
///
/// 15s is a generous-but-bounded ceiling: long enough for the bucket_seal
/// hybrid recall path, short enough that even a stuck query can't hold the
/// user-visible turn hostage.
const MEMU_TOOL_DEADLINE_MS: u64 = 15_000;

#[async_trait]
impl Tool for MemuMemoryTool {
    fn name(&self) -> &str {
        "memu_memory"
    }

    fn description(&self) -> &str {
        "检索或列出用户的长期记忆。当用户询问『有什么长期记忆/所有记忆/全部记忆』时也使用此工具。默认快速模式（无 LLM 富化），如需类别标签可显式传 enrich_categories=true。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "查询文本，描述你想了解的用户信息；如需列出全部记忆，可传 all/所有记忆/全部记忆"
                },
                "limit": {
                    "type": "integer",
                    "description": "最大返回记忆数量，默认 10；普通检索最多 20，列出全部记忆最多 25",
                    "default": 10
                },
                "enrich_categories": {
                    "type": "boolean",
                    "description": "是否对每条结果做 LLM 类别富化（默认 false，开启会显著拖慢：约 4 秒/条；仅在确实需要 category 标签时才设为 true）",
                    "default": false
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: MemuMemoryInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("参数解析失败: {}", e)))?;

        info!(
            "[memu_memory] 检索记忆: query={}, limit={}",
            input.query, input.limit
        );
        let retrieve_limit = bounded_memory_limit(input.limit, MEMU_RETRIEVE_MAX_LIMIT);
        let list_limit = bounded_memory_limit(input.limit, MEMU_LIST_MAX_LIMIT);

        // Fast path for skill-ranking queries via adapter (bucket_seal).
        //
        // Dev log showed "请列出使用次数前5的技能" routing into retrieve paths
        // and hitting timeouts — because the query is fundamentally a SQL
        // aggregation over skill usage_count.
        //
        // When the query reads as a ranking question, answer it directly from
        // the adapter. Falls through to the regular retrieve path otherwise.
        if is_skill_ranking_query(&input.query) {
            let limit = input.limit.clamp(1, 50);
            // top_skills takes &Arc<dyn MemoryAdapter> — coerce concrete adapter.
            let trait_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter> =
                Arc::clone(&self.adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>;
            match crate::memory_adapter::skills::top_skills(
                &trait_adapter,
                &self.space_id,
                limit,
            )
            .await
            {
                Ok(skills) => {
                    let memories: Vec<serde_json::Value> = skills
                        .into_iter()
                        .enumerate()
                        .map(|(idx, s)| {
                            json!({
                                "rank": idx + 1,
                                // node_id is not stored in Skill; use slug as stable identifier.
                                "node_id": s.slug,
                                "title": s.name,
                                "usage_count": s.usage_count,
                                "cited_count": s.cited_count,
                                // last_cited_at is not stored in Skill; use null.
                                "last_cited_at": serde_json::Value::Null,
                            })
                        })
                        .collect();
                    let count = memories.len();
                    let result = json!({
                        "memories": memories,
                        "query": input.query,
                        "mode": "skill_ranking",
                        "count": count,
                        "note": "Returned via adapter fast path (ranked by usage_count DESC, then cited_count). LLM-backed semantic retrieval was skipped because this looks like a ranking question.",
                    });
                    info!(
                        duration_ms = start.elapsed().as_millis() as u64,
                        count,
                        "[memu_memory] skill_ranking adapter fast path returned"
                    );
                    return Ok(ToolOutput::new(
                        result,
                        start.elapsed().as_millis() as u64,
                    ));
                }
                Err(e) => {
                    // Fall through to hybrid recall path.
                    warn!(
                        "[memu_memory] skill_ranking adapter fast path failed, falling back: {:#}",
                        e
                    );
                }
            }
        }

        // "List all" path — use adapter.list() to enumerate entries.
        if is_list_all_memory_query(&input.query) {
            match self.adapter.list(Some(&self.space_id), None, None).await {
                Ok(entries) => {
                    // Take up to list_limit entries (list() is capped at 200 internally).
                    let memories: Vec<serde_json::Value> = entries
                        .into_iter()
                        .take(list_limit)
                        .map(|e| {
                            let cat_str = e.category.to_string();
                            json!({
                                "content": e.content,
                                "type": cat_str,
                                "categories": [cat_str],
                                "created_at": e.timestamp,
                                "id": e.id,
                            })
                        })
                        .collect();
                    let count = memories.len();
                    let result = json!({
                        "memories": memories,
                        "query": input.query,
                        "mode": "list",
                        "count": count,
                        "limit": list_limit,
                        "note": format!("Returned the first {} memories only. Ask a narrower question or increase pagination in a dedicated UI flow for more.", list_limit),
                    });
                    return Ok(ToolOutput::new(
                        result,
                        start.elapsed().as_millis() as u64,
                    ));
                }
                Err(e) => {
                    warn!("[MemuMemoryTool] list failed: {}", e);
                    let result = json!({
                        "memories": [],
                        "query": input.query,
                        "mode": "list",
                        "count": 0,
                        "error": format!("list failed: {}", e),
                    });
                    return Ok(ToolOutput::new(
                        result,
                        start.elapsed().as_millis() as u64,
                    ));
                }
            }
        }

        // Standard hybrid recall path with a 15s wall-clock cap.
        let adapter = Arc::clone(&self.adapter);
        let query = input.query.clone();
        let space_id = self.space_id.clone();
        let retrieve_fut = async move {
            adapter.recall_hybrid(&query, Some(&space_id), retrieve_limit).await
        };

        let retrieve_result = tokio::time::timeout(
            std::time::Duration::from_millis(MEMU_TOOL_DEADLINE_MS),
            retrieve_fut,
        )
        .await;

        match retrieve_result {
            Err(_elapsed) => {
                warn!(
                    deadline_ms = MEMU_TOOL_DEADLINE_MS,
                    "[MemuMemoryTool] tool-level deadline exceeded; returning empty + hint"
                );
                let result = json!({
                    "memories": [],
                    "query": input.query,
                    "count": 0,
                    "error": format!("memu_memory exceeded {}ms tool-level deadline", MEMU_TOOL_DEADLINE_MS),
                    "hint": "recall took too long. Skip this tool for the current turn and answer from existing context — or retry with a more specific query and lower limit.",
                    "kind": "deadline",
                });
                Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
            }
            Ok(items) => {
                let result = json!({
                    "memories": items.iter().map(|e| {
                        let cat_str = e.category.to_string();
                        json!({
                            "content": e.content,
                            "type": cat_str,
                            "relevance": e.score.unwrap_or(0.0),
                            "categories": [cat_str],
                        })
                    }).collect::<Vec<_>>(),
                    "query": input.query,
                    "count": items.len(),
                    "limit": retrieve_limit,
                    "enriched": input.enrich_categories,
                });
                info!(
                    duration_ms = start.elapsed().as_millis() as u64,
                    count = items.len(),
                    "[memu_memory] recall_hybrid returned"
                );
                Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
            }
        }
    }
}

fn is_list_all_memory_query(query: &str) -> bool {
    let normalized = query.trim().to_lowercase();
    matches!(
        normalized.as_str(),
        ""
            | "*"
            | "all"
            | "all memory"
            | "all memories"
            | "list"
            | "list all"
            | "所有记忆"
            | "全部记忆"
            | "长期记忆"
            | "都是什么记忆内容"
            | "都是什么记忆内容？"
            | "有什么长期记忆"
            | "有什么长期记忆？"
            | "memu里有什么长期记忆"
            | "memu里有什么长期记忆？"
    ) || normalized.contains("所有记忆")
        || normalized.contains("全部记忆")
        || normalized.contains("有什么长期记忆")
}

fn bounded_memory_limit(requested: usize, max: usize) -> usize {
    requested.clamp(1, max)
}

/// Bundle 5 — does the query read as "rank skills by usage / frequency"?
///
/// Heuristic over the lowercased query. Requires BOTH a skill-noun
/// signal AND a ranking/count signal — otherwise innocent queries like
/// "top of mind" or "我用过的命令" would false-positive.
///
/// Returns `true` for:
/// - "请列出使用次数前 5 的技能"
/// - "top 5 skills by usage count"
/// - "排名前十的 skill"
/// - "skill ranking by use frequency"
///
/// Returns `false` for:
/// - "记忆里有什么 skill" (no ranking signal)
/// - "top 5 movies" (no skill signal)
fn is_skill_ranking_query(query: &str) -> bool {
    let normalized = query.trim().to_lowercase();
    if normalized.is_empty() {
        return false;
    }
    let skill_signal = [
        "技能", "skill", "skills",
        "工具使用", "工具调用", "tool usage",
    ]
    .iter()
    .any(|kw| normalized.contains(kw));
    if !skill_signal {
        return false;
    }
    let ranking_signal = [
        // Chinese
        "排名", "排行", "排序", "前 5", "前5", "前 10", "前10", "前 3", "前3",
        "使用次数", "调用次数", "次数最多", "用得最多", "用的最多",
        "最常用", "最频繁",
        // English
        "top ", "ranking", "rank by", "by usage", "by use", "by count",
        "most used", "most frequently", "usage count", "use count",
    ]
    .iter()
    .any(|kw| normalized.contains(kw));
    ranking_signal
}

// ═══════════════════════════════════════════════════════════════════════
// 2. memu_todos — 待办事项工具
// ═══════════════════════════════════════════════════════════════════════

/// 待办事项工具（bucket_seal 后端）
///
/// 获取用户的待办事项列表，支持按状态过滤。
pub struct MemuTodosTool {
    adapter: Arc<BucketSealAdapter>,
    space_id: String,
}

impl MemuTodosTool {
    pub fn new(adapter: Arc<BucketSealAdapter>) -> Self {
        Self {
            adapter,
            space_id: "default".to_string(),
        }
    }
}

/// 待办事项输入参数
#[derive(Debug, Deserialize)]
struct MemuTodosInput {
    /// 过滤状态: all / pending / completed
    #[serde(default = "default_status")]
    status: String,
}

fn default_status() -> String {
    "all".to_string()
}

#[async_trait]
impl Tool for MemuTodosTool {
    fn name(&self) -> &str {
        "memu_todos"
    }

    fn description(&self) -> &str {
        "获取用户的待办事项列表。当用户询问待办事项或你需要检查用户的任务时使用。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "status": {
                    "type": "string",
                    "description": "过滤状态: all/pending/completed",
                    "enum": ["all", "pending", "completed"],
                    "default": "all"
                }
            }
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: MemuTodosInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("参数解析失败: {}", e)))?;

        info!("[memu_todos] 获取待办事项: status={}", input.status);

        let query = match input.status.as_str() {
            "pending" => "pending todos and tasks that need to be done",
            "completed" => "completed todos and finished tasks",
            _ => "all todos and tasks",
        };

        let entries = self
            .adapter
            .recall_hybrid(query, Some(&self.space_id), 20)
            .await;

        let todos: Vec<serde_json::Value> = entries
            .into_iter()
            .filter(|e| {
                e.content.to_lowercase().contains("todo")
                    || e.content.contains("待办")
            })
            .map(|e| {
                json!({
                    "content": e.content,
                    "categories": [],
                    "created_at": e.timestamp,
                })
            })
            .collect();

        let result = json!({
            "todos": todos,
            "status_filter": input.status,
            "count": todos.len(),
        });
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. wait_user_confirm — 用户确认工具（ProactiveService 专用）
// ═══════════════════════════════════════════════════════════════════════

/// 用户确认工具
///
/// 在主动模式下执行破坏性操作（删除文件、发送消息、修改配置等）前，
/// 必须通过此工具请求用户确认。仅在主动服务上下文中可用。
pub struct WaitUserConfirmTool;

impl WaitUserConfirmTool {
    pub fn new() -> Self {
        Self
    }
}

/// 用户确认输入参数
#[derive(Debug, Deserialize)]
struct WaitUserConfirmInput {
    /// 向用户展示的确认提示信息
    prompt: String,
    /// 等待用户响应的超时时间（秒），默认 600（10 分钟）
    #[serde(default = "default_timeout")]
    timeout_secs: u64,
}

fn default_timeout() -> u64 {
    600
}

#[async_trait]
impl Tool for WaitUserConfirmTool {
    fn name(&self) -> &str {
        "wait_user_confirm"
    }

    fn description(&self) -> &str {
        "请求用户确认破坏性操作。在执行删除文件、发送消息、修改重要配置等操作前必须使用此工具。仅在主动模式下可用。"
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "prompt": {
                    "type": "string",
                    "description": "向用户展示的确认提示信息"
                },
                "timeout_secs": {
                    "type": "integer",
                    "description": "等待用户响应的超时时间（秒），默认 600（10分钟）",
                    "default": 600
                }
            },
            "required": ["prompt"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // 此工具本身就是确认机制，不需要额外审批
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: WaitUserConfirmInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("参数解析失败: {}", e)))?;

        info!(
            "[wait_user_confirm] 请求用户确认: prompt={}, timeout={}s",
            input.prompt, input.timeout_secs
        );

        // TODO: 完整实现需要：
        // 1. 通过 InfraService 或 Tauri IPC 发送确认请求到前端
        // 2. 等待用户通过 set_user_input() 响应
        // 3. 超时处理（tokio::time::timeout）
        //
        // 当前实现：记录请求并返回等待状态
        let result = json!({
            "confirmed": false,
            "prompt": input.prompt,
            "timeout_secs": input.timeout_secs,
            "status": "awaiting_frontend_integration",
            "message": "User confirmation request logged. Frontend integration pending.",
        });

        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 辅助函数 — 批量注册
// ═══════════════════════════════════════════════════════════════════════

use crate::agent::tools::tool::ToolRegistry;

/// 将 memU 基础工具（memu_memory + memu_todos）注册到给定的 ToolRegistry
///
/// Both tools use the `bucket_seal` in-process store — no memU subprocess
/// client is needed. Pass the concrete `BucketSealAdapter` arc directly.
pub fn register_memu_tools(
    registry: &mut ToolRegistry,
    adapter: Arc<BucketSealAdapter>,
) {
    registry.register(MemuMemoryTool::new(Arc::clone(&adapter)));
    registry.register(MemuTodosTool::new(adapter));
}

/// 将主动服务专用工具集注册到给定的 ToolRegistry
///
/// 包含所有记忆基础工具 + wait_user_confirm
pub fn register_proactive_tools(
    registry: &mut ToolRegistry,
    adapter: Arc<BucketSealAdapter>,
) {
    register_memu_tools(registry, adapter);
    registry.register(WaitUserConfirmTool::new());
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::{
        bounded_memory_limit, is_list_all_memory_query, is_skill_ranking_query,
        MemuMemoryTool, MemuTodosTool, MEMU_LIST_MAX_LIMIT, MEMU_RETRIEVE_MAX_LIMIT,
    };
    use crate::agent::tools::tool::Tool;

    // ── BucketSealAdapter test helper ────────────────────────────────────

    fn fresh_bucket_seal_adapter() -> (Arc<crate::memory_bucket_seal::BucketSealAdapter>, tempfile::TempDir) {
        use crate::memory_bucket_seal::{
            score::embed::InertEmbedder,
            store::BucketSealStore,
            tree_source::InertSummariser,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(InertSummariser::new());
        let adapter = Arc::new(crate::memory_bucket_seal::BucketSealAdapter::new(
            store,
            content_root,
            embedder,
            summariser,
        ));
        (adapter, dir)
    }

    // ── Unit tests for query classifiers ────────────────────────────────

    #[test]
    fn list_all_memory_query_recognizes_inventory_prompts() {
        assert!(is_list_all_memory_query("所有记忆"));
        assert!(is_list_all_memory_query("都是什么记忆内容？"));
        assert!(is_list_all_memory_query("*"));
        assert!(is_list_all_memory_query("all memories"));
        assert!(!is_list_all_memory_query("天津大学"));
    }

    #[test]
    fn memory_limits_are_clamped_for_llm_context_safety() {
        assert_eq!(bounded_memory_limit(0, MEMU_RETRIEVE_MAX_LIMIT), 1);
        assert_eq!(bounded_memory_limit(10, MEMU_RETRIEVE_MAX_LIMIT), 10);
        assert_eq!(bounded_memory_limit(1000, MEMU_RETRIEVE_MAX_LIMIT), MEMU_RETRIEVE_MAX_LIMIT);
        assert_eq!(bounded_memory_limit(1000, MEMU_LIST_MAX_LIMIT), MEMU_LIST_MAX_LIMIT);
    }

    #[test]
    fn skill_ranking_query_matches_chinese_phrasing() {
        // The exact dev-log phrasing that triggered Bundle 5
        assert!(is_skill_ranking_query("请列出使用次数前5的技能"));
        assert!(is_skill_ranking_query("使用次数前 10 的技能"));
        assert!(is_skill_ranking_query("最常用的技能"));
        assert!(is_skill_ranking_query("技能排行榜"));
        assert!(is_skill_ranking_query("技能调用次数排名"));
    }

    #[test]
    fn skill_ranking_query_matches_english_phrasing() {
        assert!(is_skill_ranking_query("top 5 skills by usage"));
        assert!(is_skill_ranking_query("skill ranking by use frequency"));
        assert!(is_skill_ranking_query("most used skills"));
        assert!(is_skill_ranking_query("rank skills by usage count"));
    }

    #[test]
    fn skill_ranking_query_rejects_non_skill_questions() {
        // No skill signal — should NOT route to SQL fast path
        assert!(!is_skill_ranking_query("top 5 movies"));
        assert!(!is_skill_ranking_query("ranking of cities"));
        assert!(!is_skill_ranking_query("最常用的命令"));
    }

    #[test]
    fn skill_ranking_query_rejects_skill_browsing() {
        // Skill signal but no ranking signal — keep the existing
        // semantic retrieve path (these are genuine "what's in the
        // catalog" questions, not ranking questions).
        assert!(!is_skill_ranking_query("我有哪些技能"));
        assert!(!is_skill_ranking_query("list my skills"));
        assert!(!is_skill_ranking_query("show all skills"));
    }

    // ── Integration tests against a real (in-process) bucket_seal ───────

    #[tokio::test]
    async fn memu_memory_tool_returns_valid_json_shape_on_empty_store() {
        let (adapter, _dir) = fresh_bucket_seal_adapter();
        let tool = MemuMemoryTool::new(adapter);
        let params = serde_json::json!({"query": "user preferences"});
        let output = tool.execute(params).await.expect("should not error");
        let v = &output.result;
        // Standard retrieve path — must have memories array and query echo.
        assert!(v.get("memories").is_some(), "missing memories key");
        assert!(v.get("query").is_some(), "missing query key");
        assert!(v.get("count").is_some(), "missing count key");
        assert_eq!(v["count"], 0);
        assert!(v["memories"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn memu_memory_tool_list_all_returns_valid_json_shape() {
        let (adapter, _dir) = fresh_bucket_seal_adapter();
        let tool = MemuMemoryTool::new(adapter);
        let params = serde_json::json!({"query": "所有记忆"});
        let output = tool.execute(params).await.expect("should not error");
        let v = &output.result;
        assert_eq!(v["mode"], "list");
        assert!(v.get("memories").is_some());
        assert!(v.get("count").is_some());
    }

    #[tokio::test]
    async fn memu_todos_tool_returns_valid_json_shape_on_empty_store() {
        let (adapter, _dir) = fresh_bucket_seal_adapter();
        let tool = MemuTodosTool::new(adapter);
        let params = serde_json::json!({"status": "all"});
        let output = tool.execute(params).await.expect("should not error");
        let v = &output.result;
        assert!(v.get("todos").is_some(), "missing todos key");
        assert!(v.get("status_filter").is_some(), "missing status_filter key");
        assert!(v.get("count").is_some(), "missing count key");
        assert_eq!(v["count"], 0);
        assert!(v["todos"].as_array().unwrap().is_empty());
    }
}
