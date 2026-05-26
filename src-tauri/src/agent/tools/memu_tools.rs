//! memU 工具集 — 记忆检索、待办事项、用户确认
//!
//! 为 agent 提供 memU 相关的工具能力，包括：
//! - `memu_memory`       — 检索用户长期记忆
//! - `memu_todos`        — 获取用户待办事项列表
//! - `wait_user_confirm` — 请求用户确认破坏性操作（主动模式专用）

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::memory_graph::store::MemoryGraphStore;
use crate::memu::client::MemUClient;

// ═══════════════════════════════════════════════════════════════════════
// 1. memu_memory — 记忆检索工具
// ═══════════════════════════════════════════════════════════════════════

/// memU 记忆检索工具
///
/// 当 agent 需要了解用户的偏好、历史信息、身份特征等时，
/// 通过此工具从 memU 服务中检索相关记忆。
pub struct MemuMemoryTool {
    client: Option<Arc<MemUClient>>,
    /// Optional MemoryGraphStore — when set, ranking-style queries
    /// (e.g. "使用次数前 5 的技能") skip the LLM-backed memU retrieval
    /// and go straight to a SQL aggregation on `procedure` nodes. Bundle 5
    /// fast path — see [`is_skill_ranking_query`].
    store: Option<Arc<MemoryGraphStore>>,
    /// Workspace / space id passed to [`MemoryGraphStore::list_top_skills_by_usage`].
    /// Hard-coded `"default"` today since the agent loop hard-codes the
    /// same; will move to per-workspace once dynamic space_id lands.
    space_id: String,
}

impl MemuMemoryTool {
    pub fn new(client: Option<Arc<MemUClient>>) -> Self {
        Self {
            client,
            store: None,
            space_id: "default".to_string(),
        }
    }

    /// Bundle 5 — attach a MemoryGraphStore handle so ranking-style
    /// queries can take the SQL fast path. Without this, ranking
    /// queries still work but fall through to the regular memU
    /// retrieve_with_context (slow, LLM-enriched).
    pub fn with_store(mut self, store: Arc<MemoryGraphStore>) -> Self {
        self.store = Some(store);
        self
    }

    /// Override the default space_id used for the SQL fast path.
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
    /// Defaults to `false`: memU's `include_categories=true` runs a
    /// per-result LLM call to classify each item, which observably took
    /// 40s for a 10-item retrieve in dev (= 4s/item × 10). The category
    /// fields are rarely consumed by the agent's downstream reasoning,
    /// so the default is now off. Callers who genuinely need typed
    /// category labels can pass `enrich_categories: true` and accept the
    /// extra wall-clock cost.
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
/// memU's `retrieve_with_context` has its own bridge-level timeout (90s
/// after Bundle 5) but a single tool call should never burn 90s of agent
/// loop time. 15s is a generous-but-bounded ceiling: long enough for
/// FastEmbed cold start + sqlite first-touch + 5–10 results without
/// enrichment, short enough that even a stuck memU subprocess can't
/// hold the user-visible turn hostage.
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

        // Bundle 5 — fast path for skill-ranking queries.
        //
        // Dev log showed "请列出使用次数前5的技能" routing into
        // `retrieve_with_context` and hitting the 60s memU timeout
        // (duration_ms=60002) — because the LLM-backed category
        // enrichment pass is doing semantic work on a question that is
        // fundamentally a SQL aggregation over `procedure_nodes.usage_count`.
        //
        // When (a) the query reads as a ranking question AND (b) a
        // MemoryGraphStore handle is wired, answer it directly from the
        // graph DB. Falls through to the regular retrieve path if either
        // condition fails — no behavior regression for non-ranking queries.
        if let Some(ref store) = self.store {
            if is_skill_ranking_query(&input.query) {
                let limit = input.limit.clamp(1, 50);
                match store.list_top_skills_by_usage(&self.space_id, limit) {
                    Ok(rows) => {
                        let memories: Vec<serde_json::Value> = rows
                            .iter()
                            .enumerate()
                            .map(
                                |(
                                    idx,
                                    (node_id, title, usage_count, cited_count, last_cited_at),
                                )| {
                                    json!({
                                        "rank": idx + 1,
                                        "node_id": node_id,
                                        "title": title,
                                        "usage_count": usage_count,
                                        "cited_count": cited_count,
                                        "last_cited_at": last_cited_at,
                                    })
                                },
                            )
                            .collect();
                        let count = memories.len();
                        let result = json!({
                            "memories": memories,
                            "query": input.query,
                            "mode": "skill_ranking",
                            "count": count,
                            // Surface the rationale so the LLM doesn't
                            // re-call memu_memory thinking it got wrong data.
                            "note": "Returned via SQL fast path (ranked by usage_count DESC, then cited_count). LLM-backed semantic retrieval was skipped because this looks like a ranking question.",
                        });
                        info!(
                            duration_ms = start.elapsed().as_millis() as u64,
                            count, "[memu_memory] skill_ranking fast path returned"
                        );
                        return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
                    }
                    Err(e) => {
                        // Fall through to memU retrieve — SQL failure
                        // shouldn't make the whole tool unusable.
                        warn!(
                            "[memu_memory] skill_ranking SQL fast path failed, falling back to memU: {}",
                            e
                        );
                    }
                }
            }
        }

        match &self.client {
            Some(client) => {
                if is_list_all_memory_query(&input.query) {
                    match client
                        .list_items(None, None, Some(list_limit as u32), Some(0), None)
                        .await
                    {
                        Ok(result) => {
                            let memories = result
                                .items
                                .iter()
                                .map(|item| {
                                    json!({
                                        "content": item
                                            .get("content")
                                            .or_else(|| item.get("summary"))
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                        "type": item
                                            .get("memory_type")
                                            .or_else(|| item.get("type"))
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                        "categories": item
                                            .get("categories")
                                            .cloned()
                                            .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                                        "created_at": item.get("created_at").cloned(),
                                        "id": item.get("id").cloned(),
                                    })
                                })
                                .collect::<Vec<_>>();
                            let count = memories.len();
                            let total = result.total;
                            let result = json!({
                                "memories": memories,
                                "query": input.query,
                                "mode": "list",
                                "count": count,
                                "total": total,
                                "limit": list_limit,
                                "truncated": count < total as usize,
                                "note": format!("Returned the first {} memories only. Ask a narrower question or increase pagination in a dedicated UI flow for more.", list_limit),
                            });
                            return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
                        }
                        Err(e) => {
                            warn!("[MemuMemoryTool] list_items failed: {}", e);
                            let result = json!({
                                "memories": [],
                                "query": input.query,
                                "mode": "list",
                                "count": 0,
                                "error": format!("list_items failed: {}", e),
                            });
                            return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
                        }
                    }
                }

                // Bundle 6 — Two latency guards on the slow path:
                //
                //  1. `enrich_categories` defaults to false. The previous
                //     hardcoded `true` triggered memU's per-result LLM
                //     classification (4s/result × limit) — observed at
                //     40s for a 10-item retrieve in dev. The agent rarely
                //     reads the `categories` field, so the default flips
                //     to off and the LLM can opt in via the input schema.
                //
                //  2. Tool-level wall-clock cap via `tokio::time::timeout`.
                //     memU's bridge timeout is 90s (Bundle 5); 15s here
                //     is the agent-loop budget — no single tool call
                //     should burn 1.5 min of user-visible turn time.
                let retrieve_fut = client.retrieve_with_context(
                    &input.query,
                    None,
                    retrieve_limit,
                    input.enrich_categories,
                );
                let retrieve_result = tokio::time::timeout(
                    std::time::Duration::from_millis(MEMU_TOOL_DEADLINE_MS),
                    retrieve_fut,
                )
                .await;

                let inner = match retrieve_result {
                    Ok(inner) => inner,
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
                            "hint": "memU retrieve took too long. Skip this tool for the current turn and answer from existing context — or retry with a more specific query and lower limit.",
                            "kind": "deadline",
                        });
                        return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
                    }
                };

                match inner {
                    Ok(items) => {
                        let result = json!({
                            "memories": items.iter().map(|item| {
                                json!({
                                    "content": item.content,
                                    "type": item.memory_type,
                                    "relevance": item.relevance_score,
                                    "categories": item.categories,
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
                            enriched = input.enrich_categories,
                            "[memu_memory] retrieve returned"
                        );
                        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
                    }
                    Err(e) => {
                        // 降级处理：返回空列表而非错误，并给 LLM 一个结构化的
                        // hint —— 否则 raw "Request timed out after 60s" 容易
                        // 让 LLM 误以为整个 memU 服务不可用并放弃后续 tool 调用。
                        warn!("[MemuMemoryTool] retrieve failed: {}", e);
                        let err_str = e.to_string();
                        let is_timeout =
                            err_str.contains("timed out") || err_str.contains("Timeout");
                        let hint = if is_timeout {
                            "memU retrieve timed out. This may be a semantic-search-heavy query — consider rephrasing with a more concrete keyword, or for skill-ranking questions ask 'top N skills by usage_count' so the SQL fast path kicks in."
                        } else {
                            "memU retrieve failed — the service may be temporarily unavailable. Proceed without long-term memory context."
                        };
                        let result = json!({
                            "memories": [],
                            "query": input.query,
                            "count": 0,
                            "error": format!("retrieve failed: {}", e),
                            "hint": hint,
                            "kind": if is_timeout { "timeout" } else { "unknown" },
                        });
                        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
                    }
                }
            }
            None => {
                let result = json!({
                    "memories": [],
                    "query": input.query,
                    "count": 0,
                    "message": "memU client not available",
                });
                Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
            }
        }
    }
}

fn is_list_all_memory_query(query: &str) -> bool {
    let normalized = query.trim().to_lowercase();
    matches!(
        normalized.as_str(),
        "" | "*"
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
        "技能",
        "skill",
        "skills",
        "工具使用",
        "工具调用",
        "tool usage",
    ]
    .iter()
    .any(|kw| normalized.contains(kw));
    if !skill_signal {
        return false;
    }
    let ranking_signal = [
        // Chinese
        "排名",
        "排行",
        "排序",
        "前 5",
        "前5",
        "前 10",
        "前10",
        "前 3",
        "前3",
        "使用次数",
        "调用次数",
        "次数最多",
        "用得最多",
        "用的最多",
        "最常用",
        "最频繁",
        // English
        "top ",
        "ranking",
        "rank by",
        "by usage",
        "by use",
        "by count",
        "most used",
        "most frequently",
        "usage count",
        "use count",
    ]
    .iter()
    .any(|kw| normalized.contains(kw));
    ranking_signal
}

// ═══════════════════════════════════════════════════════════════════════
// 2. memu_todos — 待办事项工具
// ═══════════════════════════════════════════════════════════════════════

/// memU 待办事项工具
///
/// 获取用户的待办事项列表，支持按状态过滤。
pub struct MemuTodosTool {
    client: Option<Arc<MemUClient>>,
}

impl MemuTodosTool {
    pub fn new(client: Option<Arc<MemUClient>>) -> Self {
        Self { client }
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

        match &self.client {
            Some(client) => {
                let query = match input.status.as_str() {
                    "pending" => "pending todos and tasks that need to be done",
                    "completed" => "completed todos and finished tasks",
                    _ => "all todos and tasks",
                };

                // Bundle 6 — drop the LLM-backed category enrichment
                // here too (was `true`). Todo listings care about
                // content/status, not category labels.
                match client
                    .retrieve_with_context(query, Some(&["event", "knowledge"]), 20, false)
                    .await
                {
                    Ok(items) => {
                        let todos: Vec<_> = items
                            .iter()
                            .filter(|item| {
                                item.categories
                                    .iter()
                                    .any(|c| c.to_lowercase().contains("todo"))
                                    || item.content.to_lowercase().contains("todo")
                                    || item.content.to_lowercase().contains("待办")
                            })
                            .map(|item| {
                                json!({
                                    "content": item.content,
                                    "categories": item.categories,
                                    "created_at": item.created_at,
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
                    Err(e) => {
                        warn!("[MemuTodosTool] retrieve failed: {}", e);
                        let result = json!({
                            "todos": [],
                            "status_filter": input.status,
                            "count": 0,
                            "error": format!("retrieve failed: {}", e),
                        });
                        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
                    }
                }
            }
            None => {
                let result = json!({
                    "todos": [],
                    "status_filter": input.status,
                    "count": 0,
                    "message": "memU client not available",
                });
                Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
            }
        }
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
/// `memory_graph_store` is optional — when provided, `memu_memory` will
/// route ranking-style queries ("top N skills by usage") through a SQL
/// fast path instead of the LLM-backed memU retrieve. Pass `None` to
/// keep the legacy retrieve-only behavior (e.g. proactive service path
/// where the store handle isn't readily plumbed in).
pub fn register_memu_tools(
    registry: &mut ToolRegistry,
    memu_client: Option<Arc<MemUClient>>,
    memory_graph_store: Option<Arc<MemoryGraphStore>>,
) {
    let mut memory_tool = MemuMemoryTool::new(memu_client.clone());
    if let Some(store) = memory_graph_store {
        memory_tool = memory_tool.with_store(store);
    }
    registry.register(memory_tool);
    registry.register(MemuTodosTool::new(memu_client));
}

/// 将主动服务专用工具集注册到给定的 ToolRegistry
///
/// 包含所有 memU 基础工具 + wait_user_confirm
pub fn register_proactive_tools(
    registry: &mut ToolRegistry,
    memu_client: Option<Arc<MemUClient>>,
    memory_graph_store: Option<Arc<MemoryGraphStore>>,
) {
    register_memu_tools(registry, memu_client, memory_graph_store);
    registry.register(WaitUserConfirmTool::new());
}

#[cfg(test)]
mod tests {
    use super::{
        bounded_memory_limit, is_list_all_memory_query, is_skill_ranking_query,
        MEMU_LIST_MAX_LIMIT, MEMU_RETRIEVE_MAX_LIMIT,
    };

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
        assert_eq!(
            bounded_memory_limit(1000, MEMU_RETRIEVE_MAX_LIMIT),
            MEMU_RETRIEVE_MAX_LIMIT
        );
        assert_eq!(
            bounded_memory_limit(1000, MEMU_LIST_MAX_LIMIT),
            MEMU_LIST_MAX_LIMIT
        );
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
}
