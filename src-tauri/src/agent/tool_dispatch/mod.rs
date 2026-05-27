//! ToolDispatcher —— 从 ChatDelegate 抽离的工具派发缝(Sprint 3 ①)。
//! loop-agnostic:不依赖 ReasoningContext;reason_ctx bookkeeping 经 outcome 上报。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::{Tool, ToolRegistry, ToolOutput, ToolError};
use uclaw_tool_types::ToolCall;

/// 每轮不可变派发输入(非 ReasoningContext)。
#[derive(Clone)]
pub struct ToolDispatchContext {
    pub session_id: String,
    pub conversation_id: String,
    pub workspace_root: Option<PathBuf>,
    pub attached_dirs: Vec<PathBuf>,
    pub safety_mode: Option<crate::safety::SafetyMode>,
    pub iteration: usize,
}

/// 每个 tool call 的结构化结果,供 loop 做 reason_ctx bookkeeping。
pub struct ToolDispatchOutcome {
    pub tool_call_id: String,
    pub tool_name: String,
    /// 原始 call 参数 —— ChatDelegate cutover(T9)用它做 file_ops.track / is_mutating bookkeeping。
    pub arguments: serde_json::Value,
    pub result: Result<ToolOutput, ToolError>,
    pub paths_touched: Vec<PathBuf>,
    pub was_mutation: bool,
    pub soft_error: Option<String>,
    pub rejected: bool,
}

/// Generic over `R: tauri::Runtime` so tests can use `MockRuntime` via
/// `tauri::test::mock_app()` while production uses the default `tauri::Wry`.
/// Mirrors the pattern in `LoadSkillTool<R>` and `SkillSearchTool<R>`.
pub struct ToolDispatcher<R: tauri::Runtime = tauri::Wry> {
    pub(crate) tools: Arc<ToolRegistry>,
    pub(crate) app_handle: tauri::AppHandle<R>,
    pub(crate) safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    pub(crate) pending_approvals: Arc<crate::app::PendingApprovals>,
    pub(crate) infra_service: Option<Arc<crate::infra::InfraService>>,
    pub(crate) trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    pub(crate) tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    pub(crate) hook_bus: Arc<crate::agent::hook_bus::HookBus>,
}

impl<R: tauri::Runtime> ToolDispatcher<R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle<R>,
        safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
        pending_approvals: Arc<crate::app::PendingApprovals>,
        infra_service: Option<Arc<crate::infra::InfraService>>,
        trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
        tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
        hook_bus: Arc<crate::agent::hook_bus::HookBus>,
    ) -> Self {
        Self { tools, app_handle, safety_manager, pending_approvals, infra_service, trajectory_store, tool_budget, hook_bus }
    }

    /// 派发一组 tool calls,返回每个的结构化 outcome。
    pub async fn dispatch(&self, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome> {
        let mut out = Vec::with_capacity(calls.len());
        for tc in calls {
            out.push(self.run_one(&tc, ctx).await);
        }
        out
    }

    /// 单个 call 的 per-call 例程(本任务:最简 resolve→execute→outcome;
    /// 后续任务在此插入 approval/path/stream/record/hook)。
    async fn run_one(&self, tc: &ToolCall, _ctx: &ToolDispatchContext) -> ToolDispatchOutcome {
        let Some(tool) = self.tools.get(&tc.name) else {
            return ToolDispatchOutcome {
                tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                result: Err(ToolError::NotFound(tc.name.clone())),
                paths_touched: vec![], was_mutation: false, soft_error: None, rejected: false,
            };
        };
        let result = tool.execute(tc.arguments.clone()).await;
        let soft_error = result.as_ref().ok().and_then(|o| {
            if crate::agent::dispatcher::detect_soft_tool_error(&o.result) {
                Some("soft_error".to_string())
            } else { None }
        });
        ToolDispatchOutcome {
            tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            result,
            paths_touched: vec![],
            was_mutation: crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments),
            soft_error, rejected: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use crate::agent::hook_bus::HookBus;
    use tauri::test::MockRuntime;

    struct EchoTool;
    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "echo" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput { result: json!({ "echoed": params }), cost: None, duration_ms: 0 })
        }
    }

    fn ctx() -> ToolDispatchContext {
        ToolDispatchContext { session_id: "s".into(), conversation_id: "s".into(),
            workspace_root: None, attached_dirs: vec![], safety_mode: None, iteration: 1 }
    }

    /// Build a ToolDispatcher for tests using tauri::test::mock_app() —
    /// the same pattern used by load_skill.rs:204 and skill_search.rs:581:
    ///   `let app = tauri::test::mock_app(); app.handle().clone()`
    /// SafetyManager::new requires a data_dir; use std::env::temp_dir().
    /// PendingApprovals::new() takes no args.
    fn make_dispatcher(tools: Arc<ToolRegistry>) -> ToolDispatcher<MockRuntime> {
        let app = tauri::test::mock_app();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(
            crate::safety::SafetyManager::new(&std::env::temp_dir()),
        ));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals,
            None,
            None,
            None,
            hook_bus,
        )
    }

    #[tokio::test]
    async fn dispatch_executes_and_returns_outcome() {
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool);
        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];
        let outs = d.dispatch(calls, &ctx()).await;
        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].tool_call_id, "c1");
        assert!(outs[0].result.is_ok());
        assert!(!outs[0].rejected);
        assert_eq!(outs[0].arguments, json!({"x":1}));
    }

    #[tokio::test]
    async fn unknown_tool_yields_not_found_outcome() {
        let d = make_dispatcher(Arc::new(ToolRegistry::new()));
        let calls = vec![ToolCall { id: "c1".into(), name: "nope".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;
        assert!(matches!(outs[0].result, Err(ToolError::NotFound(_))));
    }
}
