//! ToolDispatcher —— 从 ChatDelegate 抽离的工具派发缝(Sprint 3 ①)。
//! loop-agnostic:不依赖 ReasoningContext;reason_ctx bookkeeping 经 outcome 上报。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::{Tool, ToolRegistry, ToolOutput, ToolError};
use crate::safety::{SafetyMode, ApprovalDecision};
use uclaw_tool_types::ToolCall;
use tauri::Emitter;

/// 审批门结果 —— 内部使用,供 `approve()` 返回。
enum ApprovalGate {
    Allow,
    Rejected { reason: String },
}

/// 路径门结果 —— 内部使用,供 `gate_paths()` 返回。
enum PathGate {
    /// 路径检查通过,携带已解析的候选路径供 outcome.paths_touched 使用。
    Allow { paths: Vec<std::path::PathBuf> },
    /// 路径被沙箱拒绝或用户拒绝审批,携带拒绝原因。
    Rejected { reason: String },
}

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

    /// 单个 call 的 per-call 例程。
    /// approve() 在 execute() 之前运行,被拒绝则提前返回 rejected outcome。
    async fn run_one(&self, tc: &ToolCall, ctx: &ToolDispatchContext) -> ToolDispatchOutcome {
        let Some(tool) = self.tools.get(&tc.name) else {
            return ToolDispatchOutcome {
                tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                result: Err(ToolError::NotFound(tc.name.clone())),
                paths_touched: vec![], was_mutation: false, soft_error: None, rejected: false,
            };
        };

        // ── 审批门(移植自 dispatcher.rs:2490-2601) ──────────────────────
        match self.approve(tool, tc, ctx).await {
            ApprovalGate::Rejected { reason } => {
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason)),
                    paths_touched: vec![],
                    was_mutation: false,
                    soft_error: None,
                    rejected: true,
                };
            }
            ApprovalGate::Allow => {}
        }

        // ── 路径策略门(移植自 dispatcher.rs:2615-2715) ──────────────────
        let paths_touched = match self.gate_paths(tool, tc, ctx).await {
            PathGate::Rejected { reason } => {
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason)),
                    paths_touched: vec![],
                    was_mutation: false,
                    soft_error: None,
                    rejected: true,
                };
            }
            PathGate::Allow { paths } => paths,
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
            paths_touched,
            was_mutation: crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments),
            soft_error, rejected: false,
        }
    }

    /// 路径门:返回 Allow{paths} 或 Rejected{reason}。移植自 dispatcher.rs:2615-2715。
    ///
    /// 逻辑:
    /// 1. 调用 tool.path_args(&tc.arguments) 得到路径字符串列表;
    /// 2. 将相对路径解析到 ctx.workspace_root(绝对路径保持原样);
    /// 3. 当 candidate_paths 非空且 ctx.workspace_root 存在时,通过 SafetyManager.check_paths 检查;
    /// 4. PathDecision::Allow    → PathGate::Allow{paths};
    ///    PathDecision::Block    → 发出 agent:tool-rejected 事件 → PathGate::Rejected;
    ///    PathDecision::Prompt   → 注册 pending_approvals,发出 kind:"path" 的 agent:need_approval 事件,
    ///                             等待用户决策;若拒绝 → PathGate::Rejected;
    ///                             若批准且 path_scope=="session" → allow_path_for_session 持久化会话授权。
    async fn gate_paths(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext) -> PathGate {
        // Step 1 & 2: resolve path strings → PathBufs
        let candidate_paths: Vec<std::path::PathBuf> = tool
            .path_args(&tc.arguments)
            .into_iter()
            .map(|p| {
                let pb = std::path::PathBuf::from(p);
                if pb.is_absolute() {
                    pb
                } else if let Some(root) = ctx.workspace_root.as_deref() {
                    root.join(pb)
                } else {
                    pb
                }
            })
            .collect();

        // Step 3: only gate when there are candidate paths AND a workspace root
        if candidate_paths.is_empty() || ctx.workspace_root.is_none() {
            return PathGate::Allow { paths: candidate_paths };
        }

        use crate::safety::path_policy::PathDecision;
        let workspace_root = ctx.workspace_root.clone().unwrap();
        let (ws_attached, sess_attached) = crate::agent::dispatcher::load_attached_dirs_for_session(
            &self.app_handle,
            &ctx.conversation_id,
        );
        let path_decision = {
            let mgr = self.safety_manager.read().await;
            mgr.check_paths(
                &ctx.conversation_id,
                &workspace_root,
                &ws_attached,
                &sess_attached,
                &candidate_paths,
                ctx.safety_mode.as_ref(),
            )
        };

        // Step 4: handle PathDecision
        match path_decision {
            PathDecision::Allow => PathGate::Allow { paths: candidate_paths },
            PathDecision::Block { reason } => {
                tracing::warn!(tool = %tc.name, reason = %reason, "Path blocked by sandbox");
                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                    "toolName": tc.name,
                    "toolCallId": tc.id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                PathGate::Rejected { reason: format!("Error: {}", reason) }
            }
            PathDecision::Prompt { reason } => {
                tracing::info!(tool = %tc.name, reason = %reason, "Path requires approval");
                let approval_id = format!("{}::path", tc.id);
                let rx = self.pending_approvals.register(approval_id.clone());
                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                    "kind": "path",
                    "toolName": tc.name,
                    "toolId": approval_id,
                    "arguments": tc.arguments,
                    "paths": candidate_paths.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                    "reason": reason,
                    "sessionId": ctx.conversation_id,
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                let path_result = rx.await.unwrap_or_else(|_| {
                    crate::app::ApprovalResult {
                        approved: false,
                        always_allow: false,
                        tool_name: None,
                        path_scope: Some("deny".into()),
                        paths: None,
                    }
                });
                if !path_result.approved {
                    let paths_str = candidate_paths.iter()
                        .map(|p| p.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ");
                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                        "toolName": tc.name,
                        "toolCallId": tc.id,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    return PathGate::Rejected {
                        reason: format!("Error: User denied access to path(s): {}", paths_str),
                    };
                }
                if path_result.path_scope.as_deref() == Some("session") {
                    let paths_to_grant = path_result.paths.clone()
                        .unwrap_or_else(|| candidate_paths.iter().map(|p| p.display().to_string()).collect());
                    let mut mgr = self.safety_manager.write().await;
                    for p in paths_to_grant {
                        mgr.allow_path_for_session(&ctx.conversation_id, std::path::PathBuf::from(p));
                    }
                }
                // "once" scope falls through without persisting
                PathGate::Allow { paths: candidate_paths }
            }
        }
    }

    /// 审批门:返回 Allow 或 Rejected。移植自 ChatDelegate::execute_tool_calls(dispatcher.rs:2490-2601)。
    async fn approve(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext) -> ApprovalGate {
        use tauri::Manager;

        // Get the tool's own approval requirement
        let tool_approval = tool.requires_approval(&tc.arguments);

        tracing::info!(
            tool = %tc.name,
            tool_approval = ?tool_approval,
            session_safety_mode = ?ctx.safety_mode,
            "Evaluating tool approval"
        );

        // Consult SafetyManager with the session safety mode.
        // Uses the DB-backed resolver when AppState is available
        // (the normal case in the running app); falls back to the
        // in-memory shim if not (keeps any test path that doesn't
        // wire AppState working).
        let decision = {
            let mgr = self.safety_manager.read().await;
            let db_state = self.app_handle.try_state::<crate::app::AppState>();
            let session_mode = ctx.safety_mode.as_ref();
            // Yolo session override short-circuits without touching DB
            if matches!(session_mode, Some(SafetyMode::Yolo)) {
                ApprovalDecision::AutoApprove
            } else if let Some(state) = db_state {
                mgr.should_approve_with_db(
                    &state.db,
                    &ctx.conversation_id,
                    &tc.name,
                    &tc.arguments,
                    &tool_approval,
                    session_mode,
                )
            } else {
                mgr.should_approve(&tc.name, &tc.arguments, &tool_approval, session_mode)
            }
        };

        tracing::info!(
            tool = %tc.name,
            decision = ?decision,
            "Final approval decision for tool"
        );

        match decision {
            ApprovalDecision::Block { reason } => {
                tracing::warn!(tool = %tc.name, reason = %reason, "Tool blocked by safety policy");
                ApprovalGate::Rejected { reason }
            }
            ApprovalDecision::RequireApproval { reason } => {
                tracing::info!(tool = %tc.name, reason = %reason, "Tool requires approval, awaiting user decision");

                // Register pending approval and get receiver
                let rx = self.pending_approvals.register(tc.id.clone());

                // Emit structured approval request event (includes sessionId for frontend)
                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                    "toolName": tc.name,
                    "toolId": tc.id,
                    "arguments": tc.arguments,
                    "reason": reason,
                    "sessionId": ctx.conversation_id,
                    "riskLevel": "medium",
                    "timestamp": chrono::Utc::now().to_rfc3339(),
                }));

                // Await user's approval decision
                let approval_result = match rx.await {
                    Ok(result) => result,
                    Err(_) => {
                        // Channel dropped — treat as rejection
                        tracing::warn!(tool = %tc.name, "Approval channel dropped, treating as rejected");
                        crate::app::ApprovalResult { approved: false, always_allow: false, tool_name: None, path_scope: None, paths: None }
                    }
                };

                if !approval_result.approved {
                    tracing::info!(tool = %tc.name, "Tool execution rejected by user");
                    // Emit rejection event so frontend knows
                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                        "toolName": tc.name,
                        "toolCallId": tc.id,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    return ApprovalGate::Rejected {
                        reason: "Tool execution was rejected by the user.".to_string(),
                    };
                }

                // If always_allow was set, add to auto-approved list
                if approval_result.always_allow {
                    let mut mgr = self.safety_manager.write().await;
                    let _ = mgr.add_auto_approved(&tc.name);
                    tracing::info!(tool = %tc.name, "Tool added to auto-approved list via always_allow");
                }

                tracing::info!(tool = %tc.name, "Tool approved by user, proceeding");
                ApprovalGate::Allow
            }
            ApprovalDecision::AutoApprove => {
                tracing::debug!(tool = %tc.name, "Tool auto-approved");
                ApprovalGate::Allow
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use serde_json::json;
    use std::sync::atomic::{AtomicBool, Ordering};
    use crate::agent::hook_bus::HookBus;
    use crate::safety::SafetyPolicy;
    use tauri::test::MockRuntime;

    struct EchoTool {
        executed: Arc<AtomicBool>,
    }

    impl EchoTool {
        fn new(executed: Arc<AtomicBool>) -> Self {
            Self { executed }
        }
    }

    #[async_trait]
    impl Tool for EchoTool {
        fn name(&self) -> &str { "echo" }
        fn description(&self) -> &str { "echo" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
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
        make_dispatcher_with_policy(tools, SafetyPolicy::default())
    }

    fn make_dispatcher_with_policy(tools: Arc<ToolRegistry>, policy: SafetyPolicy) -> ToolDispatcher<MockRuntime> {
        let app = tauri::test::mock_app();
        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        mgr.set_policy(policy).ok();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
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
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));
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

    #[tokio::test]
    async fn blocked_tool_is_rejected_not_executed() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));

        // Build a policy that explicitly blocks "echo"
        let mut policy = SafetyPolicy::default();
        policy.blocked_tools.insert("echo".to_string());

        let d = make_dispatcher_with_policy(Arc::new(reg), policy);
        let outs = d.dispatch(
            vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({}) }],
            &ctx(),
        ).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].rejected, "outcome should be rejected");
        assert!(outs[0].result.is_err(), "result should be Err");
        assert!(!executed.load(Ordering::SeqCst), "tool must not have executed");
    }

    // ─── Path gate tests ─────────────────────────────────────────────────

    /// A tool that exposes a `file_path` arg as its path value.
    /// path_args() returns the VALUE of the "file_path" argument (as the
    /// real builtin impls do — e.g. ReadFileTool returns `args["path"].as_str()`).
    struct PathTool {
        executed: Arc<AtomicBool>,
    }

    impl PathTool {
        fn new(executed: Arc<AtomicBool>) -> Self { Self { executed } }
    }

    #[async_trait]
    impl Tool for PathTool {
        fn name(&self) -> &str { "path_tool" }
        fn description(&self) -> &str { "path_tool" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> crate::agent::tools::tool::ApprovalRequirement {
            crate::agent::tools::tool::ApprovalRequirement::Never
        }
        /// Returns the *value* of the "file_path" argument as a path string,
        /// mirroring the real builtin semantics (not returning keys).
        fn path_args<'a>(&self, args: &'a serde_json::Value) -> Vec<&'a str> {
            args.get("file_path")
                .and_then(|v| v.as_str())
                .map(|s| vec![s])
                .unwrap_or_default()
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "ok": true }), cost: None, duration_ms: 0 })
        }
    }

    fn make_dispatcher_with_safety_manager(
        tools: Arc<ToolRegistry>,
        mgr: crate::safety::SafetyManager,
    ) -> (ToolDispatcher<MockRuntime>, Arc<crate::app::PendingApprovals>) {
        let app = tauri::test::mock_app();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        let d = ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals.clone(),
            None,
            None,
            None,
            hook_bus,
        );
        (d, pending_approvals)
    }

    /// Branch covered: PathGate::Allow (in-workspace path).
    /// Tool executes and paths_touched is populated with the resolved path.
    #[tokio::test]
    async fn path_gate_allow_inworkspace_executes_and_populates_paths_touched() {
        let tmp_ws = tempfile::TempDir::new().unwrap();
        let target = tmp_ws.path().join("data.txt");
        std::fs::write(&target, "hello").unwrap();

        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(PathTool::new(executed.clone()));

        let mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        let (d, _pending) = make_dispatcher_with_safety_manager(Arc::new(reg), mgr);

        // ctx with workspace_root pointing at tmp_ws
        let ctx = ToolDispatchContext {
            session_id: "sess".into(),
            conversation_id: "sess".into(),
            workspace_root: Some(tmp_ws.path().to_path_buf()),
            attached_dirs: vec![],
            safety_mode: None,
            iteration: 1,
        };

        let outs = d.dispatch(
            vec![ToolCall {
                id: "c1".into(),
                name: "path_tool".into(),
                // relative path — will be resolved against workspace_root
                arguments: json!({ "file_path": "data.txt" }),
            }],
            &ctx,
        ).await;

        assert_eq!(outs.len(), 1);
        assert!(!outs[0].rejected, "should NOT be rejected for in-workspace path");
        assert!(outs[0].result.is_ok(), "tool should have executed successfully");
        assert!(executed.load(Ordering::SeqCst), "tool must have been called");
        // paths_touched should contain the resolved absolute path
        assert_eq!(outs[0].paths_touched.len(), 1, "paths_touched should have one entry");
        assert!(
            outs[0].paths_touched[0].starts_with(tmp_ws.path()),
            "paths_touched[0] should be inside workspace"
        );
    }

    /// Branch covered: PathGate::Rejected via Prompt → user explicitly denies.
    /// Out-of-workspace path causes a Prompt; a spawned task resolves the approval
    /// with approved=false. Tool must NOT execute; outcome rejected==true.
    #[tokio::test]
    async fn path_gate_prompt_deny_rejects_and_tool_not_executed() {
        let tmp_ws = tempfile::TempDir::new().unwrap();
        let outside = tempfile::TempDir::new().unwrap();
        let outside_file = outside.path().join("secret.txt");

        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(PathTool::new(executed.clone()));

        let mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        let (d, pending_approvals) = make_dispatcher_with_safety_manager(Arc::new(reg), mgr);

        let ctx = ToolDispatchContext {
            session_id: "sess2".into(),
            conversation_id: "sess2".into(),
            workspace_root: Some(tmp_ws.path().to_path_buf()),
            attached_dirs: vec![],
            safety_mode: None,
            iteration: 1,
        };

        // The gate_paths code registers approval with id "c2::path" and awaits it.
        // We spawn a task that resolves it with approved=false after a short delay.
        let pa_clone = pending_approvals.clone();
        tokio::spawn(async move {
            // Yield briefly so gate_paths can register the approval before we resolve it.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            pa_clone.resolve("c2::path", crate::app::ApprovalResult {
                approved: false,
                always_allow: false,
                tool_name: None,
                path_scope: Some("deny".into()),
                paths: None,
            });
        });

        let out_path_str = outside_file.display().to_string();
        let outs = d.dispatch(
            vec![ToolCall {
                id: "c2".into(),
                name: "path_tool".into(),
                arguments: json!({ "file_path": out_path_str }),
            }],
            &ctx,
        ).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].rejected, "should be rejected for out-of-workspace + denied prompt");
        assert!(outs[0].result.is_err(), "result should be Err");
        assert!(!executed.load(Ordering::SeqCst), "tool must NOT have executed");
    }
}
