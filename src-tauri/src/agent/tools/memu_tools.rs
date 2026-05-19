//! memU 工具集 — 记忆检索、待办事项、用户确认
//!
//! 为 agent 提供 memU 相关的工具能力，包括：
//! - `memu_memory`       — 检索用户长期记忆
//! - `memu_todos`        — 获取用户待办事项列表
//! - `wait_user_confirm` — 请求用户确认破坏性操作（主动模式专用）

use std::sync::Arc;
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
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
}

impl MemuMemoryTool {
    pub fn new(client: Option<Arc<MemUClient>>) -> Self {
        Self { client }
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
}

fn default_limit() -> usize {
    10
}

#[async_trait]
impl Tool for MemuMemoryTool {
    fn name(&self) -> &str {
        "memu_memory"
    }

    fn description(&self) -> &str {
        "检索或列出用户的长期记忆。当用户询问“有什么长期记忆/所有记忆/全部记忆”时也使用此工具。"
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
                    "description": "最大返回记忆数量，默认 10",
                    "default": 10
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

        match &self.client {
            Some(client) => {
                if is_list_all_memory_query(&input.query) {
                    match client
                        .list_items(None, None, Some(input.limit as u32), Some(0), None)
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
                            let result = json!({
                                "memories": memories,
                                "query": input.query,
                                "mode": "list",
                                "count": count,
                                "total": result.total,
                            });
                            return Ok(ToolOutput::new(
                                result,
                                start.elapsed().as_millis() as u64,
                            ));
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
                            return Ok(ToolOutput::new(
                                result,
                                start.elapsed().as_millis() as u64,
                            ));
                        }
                    }
                }

                match client
                    .retrieve_with_context(&input.query, None, input.limit, true)
                    .await
                {
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
                        });
                        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
                    }
                    Err(e) => {
                        // 降级处理：返回空列表而非错误
                        warn!("[MemuMemoryTool] retrieve failed: {}", e);
                        let result = json!({
                            "memories": [],
                            "query": input.query,
                            "count": 0,
                            "error": format!("retrieve failed: {}", e),
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

                match client
                    .retrieve_with_context(query, Some(&["event", "knowledge"]), 20, true)
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
pub fn register_memu_tools(registry: &mut ToolRegistry, memu_client: Option<Arc<MemUClient>>) {
    registry.register(MemuMemoryTool::new(memu_client.clone()));
    registry.register(MemuTodosTool::new(memu_client));
}

/// 将主动服务专用工具集注册到给定的 ToolRegistry
///
/// 包含所有 memU 基础工具 + wait_user_confirm
pub fn register_proactive_tools(registry: &mut ToolRegistry, memu_client: Option<Arc<MemUClient>>) {
    register_memu_tools(registry, memu_client);
    registry.register(WaitUserConfirmTool::new());
}

#[cfg(test)]
mod tests {
    use super::is_list_all_memory_query;

    #[test]
    fn list_all_memory_query_recognizes_inventory_prompts() {
        assert!(is_list_all_memory_query("所有记忆"));
        assert!(is_list_all_memory_query("都是什么记忆内容？"));
        assert!(is_list_all_memory_query("*"));
        assert!(is_list_all_memory_query("all memories"));
        assert!(!is_list_all_memory_query("天津大学"));
    }
}
