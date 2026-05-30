//! ToolDispatcher —— 从 ChatDelegate 抽离的工具派发缝(Sprint 3 ①)。
//! loop-agnostic:不依赖 ReasoningContext;reason_ctx bookkeeping 经 outcome 上报。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::{Tool, ToolRegistry, ToolOutput, ToolError};
use crate::safety::{SafetyMode, ApprovalDecision};
use uclaw_tool_types::ToolCall;
use tauri::Emitter;
use tokio_util::sync::CancellationToken;

/// 审批门结果 —— 内部使用,供 `approve()` 返回。
///
/// `reason` 进入 `ToolError::Execution` (rejected outcome 的 `result`);
/// `message` 进入 `ToolDispatchOutcome.message_content`(推入 reason_ctx 的 user_tool_result),
/// 与旧 ChatDelegate::execute_tool_calls 的逐路径 push 文案逐字一致:
/// - Block      → message = "Error: Tool blocked — {reason}"
/// - User reject → message = "Error: Tool execution was rejected by the user."
enum ApprovalGate {
    Allow,
    Rejected { reason: String, message: String },
    /// Slice 1b — non-chat origin's `handle_ask` returned `Escalated`. The
    /// caller (`run_one`) must build an outcome via `Self::escalated_outcome`
    /// so the outcome carries `rejected: false` + `is_error: true`, letting
    /// HeadlessDelegate (Task 2) distinguish escalation from explicit denial.
    Escalated,
}

/// 路径门结果 —— 内部使用,供 `gate_paths()` 返回。
enum PathGate {
    /// 路径检查通过,携带已解析的候选路径供 outcome.paths_touched 使用。
    Allow { paths: Vec<std::path::PathBuf> },
    /// 路径被沙箱拒绝或用户拒绝审批。
    /// `reason` 进入 `ToolError::Execution`;`message` 进入 user_tool_result,
    /// 与旧路径文案逐字一致:
    /// - Block → message = "Error: {reason}"
    /// - Deny  → message = "Error: User denied access to path(s): {paths_str}"
    Rejected { reason: String, message: String },
}

/// Origin of the dispatch — drives the `ApprovalOrigin` passed to `handle_ask`
/// and lets the dispatcher know which handler to consult on `RequireApproval`.
#[derive(Debug, Clone)]
pub enum ApprovalOriginKind {
    Chat { conversation_id: String },
    Automation { activity_id: String },
    BrowserSubLoop { conversation_id: String, browser_task_id: String },
}

impl ApprovalOriginKind {
    pub fn to_approval_origin(&self) -> crate::safety::ApprovalOrigin {
        match self {
            Self::Chat { conversation_id } =>
                crate::safety::ApprovalOrigin::Chat { conversation_id: conversation_id.clone() },
            Self::Automation { activity_id } =>
                crate::safety::ApprovalOrigin::Automation { activity_id: activity_id.clone() },
            Self::BrowserSubLoop { conversation_id, browser_task_id } =>
                crate::safety::ApprovalOrigin::BrowserSubLoop {
                    conversation_id: conversation_id.clone(),
                    browser_task_id: browser_task_id.clone(),
                },
        }
    }
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
    /// M1-T2e — per-run cancellation token. When fired, `dispatch` aborts
    /// in-flight tools and returns one cancelled outcome per call. None for
    /// contexts without cancellation (tests, headless).
    pub cancel: Option<CancellationToken>,
    /// Slice 1b — declarative pre-authorization. `Some` for automation;
    /// `None` for chat & browser sub-loop.
    pub permissions: Option<crate::automation::runtime::PermissionSet>,
    /// Slice 1b — origin of this dispatch. Determines which `ApprovalHandler`
    /// is consulted on `RequireApproval`.
    pub origin_kind: ApprovalOriginKind,
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
    /// 推入 reason_ctx.messages 的 user_tool_result 内容 —— 与 dispatcher emit 用的同一份
    /// (budget-truncated for Ok; error string for Err)。保证 LLM 看到的与旧行为一致。
    pub message_content: String,
    /// 该结果是否为错误(soft_error 或 hard error)—— user_tool_result 的 is_error 位。
    pub is_error: bool,
}

/// Generic over `R: tauri::Runtime` so tests can use `MockRuntime` via
/// `tauri::test::mock_app()` while production uses the default `tauri::Wry`.
/// Mirrors the pattern in `LoadSkillTool<R>` and `SkillSearchTool<R>`.
pub struct ToolDispatcher<R: tauri::Runtime = tauri::Wry> {
    pub(crate) tools: Arc<ToolRegistry>,
    pub(crate) app_handle: tauri::AppHandle<R>,
    pub(crate) safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
    pub(crate) approval_handler: Arc<dyn crate::safety::ApprovalHandler>,
    /// Retained for backward access during migration — chat dispatch's per-tool
    /// approval flow uses register/resolve keyed by tool_call_id. Wrapped in
    /// ChatApprovalHandler::new for the new approval_handler field's default.
    /// Both fields coexist in this slice; chat call sites continue to use this
    /// field directly for byte-equivalence.
    pub(crate) pending_approvals: Arc<crate::app::PendingApprovals>,
    pub(crate) infra_service: Option<Arc<crate::infra::InfraService>>,
    pub(crate) trajectory_store: Option<Arc<crate::agent::trajectory::TrajectoryStore>>,
    pub(crate) tool_budget: Option<Arc<crate::agent::tool_budget::ToolBudgetManager>>,
    pub(crate) hook_bus: Arc<crate::agent::hook_bus::HookBus>,
    /// Bundle 27-A — optional heartbeat supervisor. Mirrors the field in
    /// `ChatDelegate`. When set, `emit_tool_start` calls `mark_activity` at
    /// every tool boundary to prevent spurious stall detection for long-running
    /// tools (bash, browser navigate, etc.). None for headless/test contexts.
    pub(crate) heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
}

impl<R: tauri::Runtime> ToolDispatcher<R> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle<R>,
        safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
        pending_approvals: Arc<crate::app::PendingApprovals>,
        infra_service: Option<Arc<crate::infra::InfraService>>,
        trajectory_store: Option<Arc<crate::agent::trajectory::TrajectoryStore>>,
        tool_budget: Option<Arc<crate::agent::tool_budget::ToolBudgetManager>>,
        hook_bus: Arc<crate::agent::hook_bus::HookBus>,
        heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
    ) -> Self {
        let approval_handler: Arc<dyn crate::safety::ApprovalHandler> =
            Arc::new(crate::safety::ChatApprovalHandler::new(pending_approvals.clone()));
        Self {
            tools, app_handle, safety_manager,
            approval_handler, pending_approvals,
            infra_service, trajectory_store, tool_budget, hook_bus, heartbeat,
        }
    }

    /// Construct with an explicit `ApprovalHandler` — used by automation (Task 2)
    /// where approval escalates via DB rather than the chat IPC modal.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_approval_handler(
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle<R>,
        safety_manager: Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>,
        approval_handler: Arc<dyn crate::safety::ApprovalHandler>,
        pending_approvals: Arc<crate::app::PendingApprovals>,
        infra_service: Option<Arc<crate::infra::InfraService>>,
        trajectory_store: Option<Arc<crate::agent::trajectory::TrajectoryStore>>,
        tool_budget: Option<Arc<crate::agent::tool_budget::ToolBudgetManager>>,
        hook_bus: Arc<crate::agent::hook_bus::HookBus>,
        heartbeat: Option<Arc<crate::agent::heartbeat::HeartbeatSupervisor>>,
    ) -> Self {
        Self {
            tools, app_handle, safety_manager,
            approval_handler, pending_approvals,
            infra_service, trajectory_store, tool_budget, hook_bus, heartbeat,
        }
    }

    /// Dispatch a batch of tool calls, observing `ctx.cancel`. A fired token
    /// short-circuits (no tools run) or aborts in-flight tools (the dropped
    /// `dispatch_inner` future drops its JoinSet, which aborts spawned tasks),
    /// returning exactly one cancelled outcome per call so every tool_use gets
    /// a matching tool_result (no orphaned pairs in the message history).
    pub async fn dispatch(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
    where
        R: 'static,
    {
        let idents: Vec<(String, String, serde_json::Value)> = calls
            .iter()
            .map(|c| (c.id.clone(), c.name.clone(), c.arguments.clone()))
            .collect();
        let make_cancelled = || {
            idents
                .iter()
                .map(|(id, name, args)| Self::cancelled_outcome(id, name, args))
                .collect::<Vec<_>>()
        };

        match &ctx.cancel {
            Some(tok) if tok.is_cancelled() => make_cancelled(),
            Some(tok) => {
                let tok = tok.clone();
                tokio::select! {
                    biased;
                    _ = tok.cancelled() => {
                        tracing::info!("[M1-T2e] tool dispatch cancelled mid-flight");
                        make_cancelled()
                    }
                    out = self.dispatch_inner(calls, ctx) => out,
                }
            }
            None => self.dispatch_inner(calls, ctx).await,
        }
    }

    /// Build a cancelled outcome for one call. Mirrors the shape of a hard-error
    /// outcome so execute_tool_calls bookkeeping pushes a matching tool_result.
    fn cancelled_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
        ToolDispatchOutcome {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            arguments: args.clone(),
            result: Err(crate::agent::tools::tool::ToolError::kinded(
                crate::agent::tools::tool::ToolErrorKind::Other,
                "tool execution cancelled",
            )),
            paths_touched: vec![],
            was_mutation: false,
            soft_error: None,
            rejected: false,
            message_content: "Error: tool execution cancelled".to_string(),
            is_error: true,
        }
    }

    /// Build a denied outcome for one call. Used when `PermissionSet::covers`
    /// returns `Coverage::Denied` — the tool's category is explicitly blocked.
    fn denied_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
        ToolDispatchOutcome {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            arguments: args.clone(),
            result: Err(crate::agent::tools::tool::ToolError::kinded(
                crate::agent::tools::tool::ToolErrorKind::PermissionDenied,
                "tool denied by spec",
            )),
            paths_touched: vec![],
            was_mutation: false,
            soft_error: None,
            rejected: true,
            message_content: "Error: tool denied by spec".to_string(),
            is_error: true,
        }
    }

    /// Build an escalated outcome for one call. Used when `ApprovalHandler`
    /// returns `Escalated` — the approval request has been forwarded for async
    /// resolution (e.g. written to DB for human review).
    fn escalated_outcome(id: &str, name: &str, args: &serde_json::Value) -> ToolDispatchOutcome {
        ToolDispatchOutcome {
            tool_call_id: id.to_string(),
            tool_name: name.to_string(),
            arguments: args.clone(),
            result: Err(crate::agent::tools::tool::ToolError::kinded(
                crate::agent::tools::tool::ToolErrorKind::Other,
                "tool execution awaiting user approval",
            )),
            paths_touched: vec![],
            was_mutation: false,
            soft_error: None,
            rejected: false,
            message_content: "Error: awaiting user approval".to_string(),
            is_error: true,
        }
    }

    /// 派发一组 tool calls,返回每个的结构化 outcome(输入顺序)。
    ///
    /// 按 `tool.concurrency()` 分道:
    /// - `ToolConcurrency::Parallel`  → 收进 JoinSet 批次并发执行;
    /// - `ToolConcurrency::Sequential`→ 内联串行执行。
    /// 结果按输入下标还原顺序后返回。
    /// `self: &Arc<Self>` 满足 JoinSet spawn 的 'static 约束。
    async fn dispatch_inner(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
    where
        R: 'static,
    {
        // Preallocate result slots (None = not yet filled).
        let n = calls.len();
        let mut results: Vec<Option<ToolDispatchOutcome>> = (0..n).map(|_| None).collect();

        let mut set: tokio::task::JoinSet<(usize, ToolDispatchOutcome)> = tokio::task::JoinSet::new();

        for (idx, tc) in calls.into_iter().enumerate() {
            // Resolve concurrency mode; unknown tools default to Sequential so
            // the NotFound outcome still flows through the normal path.
            let concurrency = self.tools.get(&tc.name)
                .map(|t| t.concurrency())
                .unwrap_or(crate::agent::tools::tool::ToolConcurrency::Sequential);

            match concurrency {
                crate::agent::tools::tool::ToolConcurrency::Parallel => {
                    let me = Arc::clone(self);
                    let tc_owned = tc;
                    let ctx_owned = ctx.clone();
                    set.spawn(async move { (idx, me.run_one(&tc_owned, &ctx_owned).await) });
                }
                crate::agent::tools::tool::ToolConcurrency::Sequential => {
                    // Drain any already-spawned parallel tasks before the next
                    // sequential call to preserve causal ordering where it matters.
                    while let Some(res) = set.join_next().await {
                        if let Ok((i, outcome)) = res {
                            results[i] = Some(outcome);
                        }
                    }
                    results[idx] = Some(self.run_one(&tc, ctx).await);
                }
            }
        }

        // Collect remaining parallel tasks.
        while let Some(res) = set.join_next().await {
            if let Ok((i, outcome)) = res {
                results[i] = Some(outcome);
            }
        }

        // Unwrap — every slot must have been filled by one of the two lanes.
        results.into_iter().enumerate().map(|(i, opt)| {
            opt.unwrap_or_else(|| panic!("ToolDispatcher: result slot {i} was never filled"))
        }).collect()
    }

    /// 单个 call 的 per-call 例程。
    /// approve() 在 execute() 之前运行,被拒绝则提前返回 rejected outcome。
    async fn run_one(&self, tc: &ToolCall, ctx: &ToolDispatchContext) -> ToolDispatchOutcome
    where
        R: 'static,
    {
        let Some(tool) = self.tools.get(&tc.name) else {
            // Old NotFound path pushed `format!("Error: Tool '{}' not found", tc.name)`.
            return ToolDispatchOutcome {
                tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
                arguments: tc.arguments.clone(),
                result: Err(ToolError::NotFound(tc.name.clone())),
                paths_touched: vec![], was_mutation: false, soft_error: None, rejected: false,
                message_content: format!("Error: Tool '{}' not found", tc.name),
                is_error: true,
            };
        };

        // ── Slice 1b: PermissionSet decision (declarative pre-authorization) ──
        use crate::automation::runtime::Coverage;
        let perm_decision = ctx.permissions.as_ref().map(|p| p.covers(&tc.name));
        if matches!(perm_decision, Some(Coverage::Denied)) {
            tracing::info!(
                tool = %tc.name,
                "[Slice 1b] tool denied by PermissionSet — rejecting outcome"
            );
            return Self::denied_outcome(&tc.id, &tc.name, &tc.arguments);
        }
        let permission_mode_override = match perm_decision {
            Some(Coverage::Allowed) => Some(crate::safety::SafetyMode::Yolo),
            _ => None,
        };

        // ── 审批门(移植自 dispatcher.rs:2490-2601) ──────────────────────
        match self.approve(tool, tc, ctx, permission_mode_override.as_ref()).await {
            ApprovalGate::Escalated => {
                return Self::escalated_outcome(&tc.id, &tc.name, &tc.arguments);
            }
            ApprovalGate::Rejected { reason, message } => {
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason)),
                    paths_touched: vec![],
                    was_mutation: false,
                    soft_error: None,
                    rejected: true,
                    message_content: message,
                    is_error: true,
                };
            }
            ApprovalGate::Allow => {}
        }

        // ── 路径策略门(移植自 dispatcher.rs:2615-2715) ──────────────────
        let paths_touched = match self.gate_paths(tool, tc, ctx).await {
            PathGate::Rejected { reason, message } => {
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(),
                    tool_name: tc.name.clone(),
                    arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason)),
                    paths_touched: vec![],
                    was_mutation: false,
                    soft_error: None,
                    rejected: true,
                    message_content: message,
                    is_error: true,
                };
            }
            PathGate::Allow { paths } => paths,
        };

        // ── PreToolUse 决策门(approve/path 之后,execute 之前)──
        match self.hook_bus.dispatch_with_decision(&crate::agent::hook_bus::HookEvent::PreToolUse {
            task_id: ctx.session_id.clone(),
            tool_name: tc.name.clone(),
            args_json: tc.arguments.to_string(),
        }).await {
            crate::runtime::contracts::HookDecision::Allow => {}
            crate::runtime::contracts::HookDecision::Deny { reason } => {
                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                    "toolName": tc.name, "toolCallId": tc.id, "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                return ToolDispatchOutcome {
                    tool_call_id: tc.id.clone(), tool_name: tc.name.clone(), arguments: tc.arguments.clone(),
                    result: Err(ToolError::Execution(reason.clone())),
                    message_content: format!("Error: Hook denied tool — {reason}"),
                    is_error: true, rejected: true, paths_touched: vec![], was_mutation: false, soft_error: None,
                };
            }
            crate::runtime::contracts::HookDecision::AskUser { prompt, risk_class } => {
                let rx = self.pending_approvals.register(tc.id.clone());
                let _ = self.app_handle.emit("agent:need_approval", serde_json::json!({
                    "toolName": tc.name, "toolId": tc.id, "arguments": tc.arguments,
                    "reason": prompt, "sessionId": ctx.conversation_id,
                    "riskLevel": match risk_class { Some(r) => format!("{r:?}").to_lowercase(), None => "medium".to_string() },
                    "kind": "hook", "timestamp": chrono::Utc::now().to_rfc3339(),
                }));
                let approval = rx.await.unwrap_or(crate::app::ApprovalResult {
                    approved: false, always_allow: false, tool_name: None, path_scope: None, paths: None,
                });
                if !approval.approved {
                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                        "toolName": tc.name, "toolCallId": tc.id, "timestamp": chrono::Utc::now().to_rfc3339(),
                    }));
                    return ToolDispatchOutcome {
                        tool_call_id: tc.id.clone(), tool_name: tc.name.clone(), arguments: tc.arguments.clone(),
                        result: Err(ToolError::Execution("Hook gate: rejected by user.".to_string())),
                        message_content: "Error: Hook gate rejected by user.".to_string(),
                        is_error: true, rejected: true, paths_touched: vec![], was_mutation: false, soft_error: None,
                    };
                }
            }
        }

        // ── SP3 of 阶段 5: shadow checkpoint before mutating tools ──────────
        // Best-effort: never blocks the tool; failure logs at debug and is
        // silently ignored. Uses is_mutating_tool (the same classification
        // already used by was_mutation in the outcome) to decide whether a
        // snapshot is needed. Accesses the CheckpointStore via AppState so
        // the ToolDispatcher struct needs no new field.
        if crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments) {
            use tauri::Manager as _;
            if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
                let store = state.checkpoint_store.clone();
                let working_dir = ctx.workspace_root
                    .as_ref()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default();
                if !working_dir.is_empty() {
                    let turn = ctx.iteration as u64;
                    // Synchronous git snapshot (~10-50ms for small trees).
                    // The dispatcher runs inside a tokio task; spawn_blocking
                    // is the correct call for blocking work.
                    let store_clone = store.clone();
                    let wd_clone = working_dir.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        store_clone.ensure_checkpoint(&wd_clone, turn);
                    });
                    // Fire-and-forget: we don't await — the snapshot must not
                    // block the tool call.  The `spawn_blocking` handle is
                    // intentionally dropped here.
                }
            }
        }

        // Allow(或 AskUser 通过)→ 现在才发 tool_start,再 execute。
        self.emit_tool_start(tool, tc, ctx);

        let result = self.run_tool(tool, tc, ctx).await;

        // ── PostToolUse hook (observe-only) ─────────────────────────────────
        let hook_success = matches!(&result, Ok(o) if !crate::agent::dispatcher::detect_soft_tool_error(&o.result));
        let hook_preview = match &result {
            Ok(o) => { let s = o.result.to_string(); s.chars().take(256).collect::<String>() }
            Err(e) => format!("{e}"),
        };
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostToolUse {
            task_id: ctx.session_id.clone(),
            tool_name: tc.name.clone(),
            success: hook_success,
            result_preview: hook_preview,
        }).await;

        // ── Budget truncation + result/error emit + trajectory + infra ──────
        // Returns (soft_error_text, message_content, is_error) so T9 can rebuild
        // reason_ctx.messages + recent_tool_errors faithfully:
        //   - message_content: the EXACT string the old code pushed via user_tool_result
        //     (budget-truncated result for Ok; "Error: {e}" for Err).
        //   - is_error: the user_tool_result is_error bit (soft_error for Ok; true for Err).
        //   - soft_error_text: the extracted stderr/output text (already truncate_utf8'd to 200)
        //     for soft errors so T9 can build recent_tool_errors as "{name}: {text}". None otherwise.
        let (soft_error, message_content, is_error) = match &result {
            Ok(output) => {
                // Budget truncation (must happen before trajectory so stored result
                // matches what the LLM will see).
                let raw_result_str = serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".into());
                let turn_idx = ctx.iteration as u32;
                let result_str = if let Some(ref budget) = self.tool_budget {
                    budget.apply(&tc.name, raw_result_str, &ctx.conversation_id, turn_idx)
                } else {
                    raw_result_str
                };

                // Emit tool_result activity event (mirrors ChatDelegate::emit_tool_result).
                let is_soft_err = crate::agent::dispatcher::detect_soft_tool_error(&output.result);
                let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
                    "conversationId": ctx.conversation_id,
                    "activity": {
                        "type": "tool_result",
                        "toolName": tc.name,
                        "toolCallId": tc.id,
                        "result": output.result,
                        "durationMs": output.duration_ms,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "isError": is_soft_err,
                    }
                }));

                // Trajectory store write.
                if let Some(ref store) = self.trajectory_store {
                    use crate::agent::trajectory::TurnRecord;
                    let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                    let record = TurnRecord {
                        id: uuid::Uuid::new_v4().to_string(),
                        session_id: ctx.conversation_id.clone(),
                        turn_index: turn_idx,
                        role: "tool".into(),
                        content: None,
                        tool_name: Some(tc.name.clone()),
                        tool_args: Some(tool_args_json),
                        tool_result: Some(result_str.clone()),
                        reasoning: None,
                        is_error: is_soft_err,
                        duration_ms: output.duration_ms,
                        created_at: chrono::Utc::now().timestamp_millis(),
                    };
                    if let Err(e) = store.record_turn(&record) {
                        tracing::warn!("ToolDispatcher: failed to record trajectory turn: {e}");
                    }
                }

                // InfraService publish.
                if let Some(ref infra) = self.infra_service {
                    let input_summary = crate::agent::dispatcher::truncate_utf8(
                        &serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                    let output_summary = crate::agent::dispatcher::truncate_utf8(&result_str, 500);
                    infra.publish_tool_executed(
                        "local",
                        &tc.name,
                        &output_summary,
                        serde_json::json!({
                            "tool_name": tc.name,
                            "success": true,
                            "duration_ms": output.duration_ms,
                            "tool_input": input_summary,
                        }),
                    ).await;
                }

                // Extract the soft-error text exactly as the old serial/parallel paths did:
                // prefer `stderr`, fall back to `output`, default "tool error";
                // truncate_utf8(_, 200). T9 builds recent_tool_errors as "{name}: {text}".
                let soft_error_text = if is_soft_err {
                    let err_text = output
                        .result
                        .get("stderr")
                        .or_else(|| output.result.get("output"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("tool error");
                    Some(crate::agent::dispatcher::truncate_utf8(err_text, 200))
                } else {
                    None
                };
                // message_content = the budget-truncated result string the old code pushed.
                // is_error = the soft-error bit.
                (soft_error_text, result_str, is_soft_err)
            }
            Err(e) => {
                // Emit hard-error tool_result (mirrors ChatDelegate::emit_tool_error).
                let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
                    "conversationId": ctx.conversation_id,
                    "activity": {
                        "type": "tool_result",
                        "toolName": tc.name,
                        "toolCallId": tc.id,
                        "result": { "ok": false, "error": e.to_string() },
                        "durationMs": 0u64,
                        "timestamp": chrono::Utc::now().to_rfc3339(),
                        "isError": true,
                    }
                }));
                // Old hard-error path pushed `format!("Error: {}", e)`, is_error=true.
                // soft_error stays None: the old recent_tool_errors hard-error push used the
                // raw `e` (T9 derives that from result.is_err()), not the soft_error text.
                (None, format!("Error: {}", e), true)
            }
        };

        ToolDispatchOutcome {
            tool_call_id: tc.id.clone(), tool_name: tc.name.clone(),
            arguments: tc.arguments.clone(),
            result,
            paths_touched,
            was_mutation: crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments),
            soft_error, rejected: false,
            message_content,
            is_error,
        }
    }

    /// Emit the `tool_start` activity event and beat the heartbeat supervisor.
    /// Mirrors `ChatDelegate::emit_tool_start` exactly:
    /// - `chat:stream-tool-activity` with `type:"tool_start"`, previewTarget, etc.
    /// - `mark_activity` on the heartbeat with `"tool_call:{name}"` stage label.
    ///
    /// Called only for tools that pass ALL gates (approval + path) — matching the
    /// old `execute_tool_calls` behavior where `emit_tool_start` ran immediately
    /// before execute, after rejection / block checks had passed.
    fn emit_tool_start(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext) {
        // I1: Parallel tools must NOT emit a previewTarget (matches old parallel path
        // where preview_target_path was never consulted). Only Serial/Sequential tools
        // get the non-None previewTarget so the frontend auto-opens a preview only for
        // tools like write_file, not for plain reads (ReadFileTool is Parallel).
        let preview_target = if tool.concurrency() == crate::agent::tools::tool::ToolConcurrency::Parallel {
            None
        } else {
            tool.preview_target_path(&tc.arguments)
        };
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": ctx.conversation_id,
            "activity": {
                "type": "tool_start",
                "toolName": tc.name,
                "toolCallId": tc.id,
                "input": tc.arguments,
                "previewTarget": preview_target,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }
        }));
        // Mirror ChatDelegate::beat: call mark_activity on the heartbeat supervisor
        // with the same stage label format ("tool_call:{name}") the old code used.
        if let Some(ref hb) = self.heartbeat {
            hb.mark_activity(&format!(
                "{}:{}",
                crate::agent::heartbeat::stages::TOOL_CALL,
                tc.name
            ));
        }
    }

    /// 执行工具:流式工具搭 coalescer drain,否则直接 execute。移植自 dispatcher.rs:2751-2834。
    ///
    /// 流式路径:channel(256) → spawned drain task(~50ms / 8KB flush) → execute_streaming → drop(sink) → handle.await。
    /// 非流式路径:直接 execute。
    ///
    /// C1 panic isolation: each tool's execute/execute_streaming runs inside a
    /// `tokio::task::spawn`, re-resolving the tool from the Arc<ToolRegistry> inside
    /// the task. A panic inside the tool is caught via JoinError::is_panic() and
    /// converted to ToolError::Execution("crashed unexpectedly") so the agent turn
    /// continues rather than unwinding the caller. Matches old execute_tool_calls
    /// behavior (dispatcher.rs:2807-2828).
    async fn run_tool(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext) -> Result<ToolOutput, ToolError>
    where
        R: 'static,
    {
        // Inject `_tool_call_id` into the args before execute, mirroring the old
        // ChatDelegate::execute_tool_calls behavior. load_skill / skill_search read
        // `params["_tool_call_id"]` to stamp the `toolCallId` on their UI events
        // (agent:skill-recalled). Without this they'd emit an empty toolCallId.
        // NOTE: injection happens BEFORE moving args into the spawn so the injected
        // value is present when the tool's execute/execute_streaming sees the params.
        let mut args = tc.arguments.clone();
        if let Some(obj) = args.as_object_mut() {
            obj.insert("_tool_call_id".to_string(), serde_json::Value::String(tc.id.clone()));
        } else {
            tracing::warn!(
                tool = %tc.name,
                "tool arguments is not a JSON object; skipping _tool_call_id injection"
            );
        }

        if tool.supports_streaming() {
            let (sink, mut rx) = crate::agent::tools::stream::ToolStreamSink::channel(256);
            let app = self.app_handle.clone();
            let conv = ctx.conversation_id.clone();
            let id = tc.id.clone();
            let handle = tokio::spawn(async move {
                let mut buf_out = String::new();
                let mut buf_err = String::new();
                let mut last_seq: u64 = 0;
                let mut tick = tokio::time::interval(std::time::Duration::from_millis(50));
                tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
                // flush 作为闭包而非嵌套 fn,以捕获泛型参数 R(nested fn 无法引用外层泛型)。
                let flush = |app: &tauri::AppHandle<_>, conv: &str, id: &str, last_seq: u64,
                              buf_out: &mut String, buf_err: &mut String| {
                    if !buf_out.is_empty() {
                        let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                            "conversationId": conv,
                            "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                          "seq": last_seq, "stream": "stdout", "chunk": std::mem::take(buf_out) }
                        }));
                    }
                    if !buf_err.is_empty() {
                        let _ = app.emit("chat:stream-tool-activity", serde_json::json!({
                            "conversationId": conv,
                            "activity": { "type": "tool_output_chunk", "toolCallId": id,
                                          "seq": last_seq, "stream": "stderr", "chunk": std::mem::take(buf_err) }
                        }));
                    }
                };
                loop {
                    tokio::select! {
                        ev = rx.recv() => match ev {
                            Some(e) => {
                                last_seq = e.seq;
                                let s = String::from_utf8_lossy(&e.bytes);
                                match e.stream {
                                    crate::agent::tools::stream::ToolStream::Stdout => buf_out.push_str(&s),
                                    crate::agent::tools::stream::ToolStream::Stderr => buf_err.push_str(&s),
                                }
                                if buf_out.len() + buf_err.len() >= 8192 {
                                    flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err);
                                }
                            }
                            None => { flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err); break; }
                        },
                        _ = tick.tick() => flush(&app, &conv, &id, last_seq, &mut buf_out, &mut buf_err),
                    }
                }
            });

            // C1: execute_streaming runs inside tokio::spawn so a panic is caught
            // via JoinError::is_panic() and converted to ToolError::Execution,
            // preventing the panic from unwinding the agent turn.
            let tool_name_for_panic = tc.name.clone();
            let tools_arc = Arc::clone(&self.tools);
            let sink_for_spawn = sink.clone();
            let execute_result = match tokio::task::spawn(async move {
                match tools_arc.get(&tool_name_for_panic) {
                    Some(t) => t.execute_streaming(args, sink_for_spawn).await,
                    None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                }
            }).await {
                Ok(Ok(out)) => Ok(out),
                Ok(Err(e)) => Err(e),
                Err(join_err) if join_err.is_panic() => {
                    tracing::error!(tool = %tc.name, "tool panicked");
                    Err(crate::agent::tools::tool::ToolError::Execution(format!(
                        "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.", tc.name,
                    )))
                }
                Err(join_err) => {
                    tracing::error!(tool = %tc.name, %join_err, "tool join error");
                    Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                }
            };

            // 工具结束 → 关 sink(drop) → coalescer 收尾 flush 后退出。
            drop(sink);
            let _ = handle.await;
            execute_result
        } else {
            // C1: execute runs inside tokio::spawn so a panic is caught via
            // JoinError::is_panic() and converted to ToolError::Execution,
            // preventing the panic from unwinding the agent turn.
            let tool_name_for_panic = tc.name.clone();
            let tools_arc = Arc::clone(&self.tools);
            match tokio::task::spawn(async move {
                match tools_arc.get(&tool_name_for_panic) {
                    Some(t) => t.execute(args).await,
                    None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                }
            }).await {
                Ok(Ok(out)) => Ok(out),
                Ok(Err(e)) => Err(e),
                Err(join_err) if join_err.is_panic() => {
                    tracing::error!(tool = %tc.name, "tool panicked");
                    Err(crate::agent::tools::tool::ToolError::Execution(format!(
                        "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.", tc.name,
                    )))
                }
                Err(join_err) => {
                    tracing::error!(tool = %tc.name, %join_err, "tool join error");
                    Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                }
            }
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
                // Old path-block pushed `format!("Error: {}", reason)`.
                let message = format!("Error: {}", reason);
                PathGate::Rejected { reason: message.clone(), message }
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
                    // Old path-deny pushed `format!("Error: User denied access to path(s): {}", paths_str)`.
                    let message = format!("Error: User denied access to path(s): {}", paths_str);
                    return PathGate::Rejected { reason: message.clone(), message };
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
    async fn approve(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext, permission_mode_override: Option<&SafetyMode>) -> ApprovalGate {
        use tauri::Manager;

        // Get the tool's own approval requirement
        let tool_approval = tool.requires_approval(&tc.arguments);

        tracing::info!(
            tool = %tc.name,
            tool_approval = ?tool_approval,
            session_safety_mode = ?ctx.safety_mode,
            "Evaluating tool approval"
        );

        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PrePermission {
            task_id: ctx.session_id.clone(),
            action: "tool_use".to_string(),
            target: tc.name.clone(),
        }).await;

        // Consult SafetyManager with the session safety mode.
        // Uses the DB-backed resolver when AppState is available
        // (the normal case in the running app); falls back to the
        // in-memory shim if not (keeps any test path that doesn't
        // wire AppState working).
        //
        // Slice 1b: `permission_mode_override` (Some(Yolo)) when PermissionSet::Allowed;
        // wins over `ctx.safety_mode` so auto-approved tools skip the DB check.
        let decision = {
            let mgr = self.safety_manager.read().await;
            let db_state = self.app_handle.try_state::<crate::app::AppState>();
            let effective_mode = permission_mode_override.or(ctx.safety_mode.as_ref());
            // Yolo mode short-circuits without touching DB
            if matches!(effective_mode, Some(SafetyMode::Yolo)) {
                ApprovalDecision::AutoApprove
            } else if let Some(state) = db_state {
                mgr.should_approve_with_db(
                    &state.db,
                    &ctx.conversation_id,
                    &tc.name,
                    &tc.arguments,
                    &tool_approval,
                    effective_mode,
                )
            } else {
                mgr.should_approve(&tc.name, &tc.arguments, &tool_approval, effective_mode)
            }
        };

        tracing::info!(
            tool = %tc.name,
            decision = ?decision,
            "Final approval decision for tool"
        );

        let granted = !matches!(decision, ApprovalDecision::Block { .. });
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PostPermission {
            task_id: ctx.session_id.clone(),
            action: "tool_use".to_string(),
            granted,
        }).await;

        match decision {
            ApprovalDecision::Block { reason } => {
                tracing::warn!(tool = %tc.name, reason = %reason, "Tool blocked by safety policy");
                // Old block path pushed `format!("Error: Tool blocked — {}", reason)`.
                let message = format!("Error: Tool blocked — {}", reason);
                ApprovalGate::Rejected { reason, message }
            }
            ApprovalDecision::RequireApproval { reason } => {
                tracing::info!(tool = %tc.name, reason = %reason, "Tool requires approval, awaiting user decision");

                // Slice 1b: branch by origin_kind.
                // Chat path: byte-equivalent to pre-1b behavior — tool_call_id-keyed
                // PendingApprovals for React modal IPC.
                // Non-chat path: route through approval_handler.handle_ask, which for
                // AutomationApprovalHandler (Task 2) will escalate via DB.
                match &ctx.origin_kind {
                    ApprovalOriginKind::Chat { .. } => {
                        // Existing behavior — unchanged for byte-equivalence with chat.
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
                                message: "Error: Tool execution was rejected by the user.".to_string(),
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
                    _ => {
                        // Non-chat origins: route through ApprovalHandler.
                        // In this slice, AutomationApprovalHandler is not yet wired (Task 2).
                        // ChatApprovalHandler is the default — it returns Denied for Automation origin.
                        use crate::safety::ApprovalOutcome;
                        let outcome = self.approval_handler.handle_ask(
                            &tc.name,
                            &tc.arguments,
                            &ctx.origin_kind.to_approval_origin(),
                        ).await;
                        match outcome {
                            ApprovalOutcome::Approved => {
                                tracing::info!(tool = %tc.name, "Tool approved via approval_handler");
                                ApprovalGate::Allow
                            }
                            ApprovalOutcome::Denied => {
                                tracing::info!(tool = %tc.name, "Tool denied via approval_handler");
                                ApprovalGate::Rejected {
                                    reason: "Tool execution was rejected.".to_string(),
                                    message: "Error: Tool execution was rejected.".to_string(),
                                }
                            }
                            ApprovalOutcome::Escalated => {
                                // Escalated means the approval is pending async resolution.
                                // Return ApprovalGate::Escalated so run_one calls
                                // Self::escalated_outcome, producing rejected:false + is_error:true.
                                // Task 2's HeadlessDelegate keys on rejected:false to convert
                                // these to LoopOutcome::NeedApproval.
                                tracing::info!(tool = %tc.name, "Tool approval escalated");
                                ApprovalGate::Escalated
                            }
                        }
                    }
                }
            }
            ApprovalDecision::AutoApprove => {
                tracing::debug!(tool = %tc.name, "Tool auto-approved");
                ApprovalGate::Allow
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
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
            workspace_root: None, attached_dirs: vec![], safety_mode: None, iteration: 1,
            cancel: None, permissions: None,
            origin_kind: ApprovalOriginKind::Chat { conversation_id: "s".into() } }
    }

    /// Build a ToolDispatcher for tests using tauri::test::mock_app() —
    /// the same pattern used by load_skill.rs:204 and skill_search.rs:581:
    ///   `let app = tauri::test::mock_app(); app.handle().clone()`
    /// SafetyManager::new requires a data_dir; use std::env::temp_dir().
    /// PendingApprovals::new() takes no args.
    ///
    /// Returns Arc<ToolDispatcher> so tests can call `d.dispatch(...)` which
    /// now requires `self: &Arc<Self>` for the JoinSet 'static bound.
    fn make_dispatcher(tools: Arc<ToolRegistry>) -> Arc<ToolDispatcher<MockRuntime>> {
        make_dispatcher_with_policy(tools, SafetyPolicy::default())
    }

    fn make_dispatcher_with_policy(tools: Arc<ToolRegistry>, policy: SafetyPolicy) -> Arc<ToolDispatcher<MockRuntime>> {
        let app = tauri::test::mock_app();
        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        mgr.set_policy(policy).ok();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        Arc::new(ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals,
            None,
            None,
            None,
            hook_bus,
            None, // heartbeat: None for tests
        ))
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
    async fn dispatch_short_circuits_when_cancelled() {
        // A registry with one real tool; a pre-fired token must short-circuit
        // BEFORE the tool runs, yielding one cancelled outcome per call.
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));
        let d = make_dispatcher(Arc::new(reg));

        let token = tokio_util::sync::CancellationToken::new();
        token.cancel(); // pre-fired

        let mut c = ctx();
        c.cancel = Some(token);

        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];
        let outs = d.dispatch(calls, &c).await;

        assert_eq!(outs.len(), 1, "one outcome per call (no orphaned tool_use)");
        assert_eq!(outs[0].tool_call_id, "c1");
        assert!(outs[0].result.is_err());
        assert!(outs[0].is_error);
        assert_eq!(outs[0].message_content, "Error: tool execution cancelled");
        assert!(!executed.load(Ordering::SeqCst), "tool must not run when pre-cancelled");
    }

    // ─── NamedEchoTool helper — EchoTool with a runtime-configurable name ──

    struct NamedEchoTool {
        executed: Arc<AtomicBool>,
        tool_name: String,
    }

    impl NamedEchoTool {
        fn new(executed: Arc<AtomicBool>, name: impl Into<String>) -> Self {
            Self { executed, tool_name: name.into() }
        }
    }

    #[async_trait]
    impl Tool for NamedEchoTool {
        fn name(&self) -> &str { &self.tool_name }
        fn description(&self) -> &str { "named echo" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "echoed": params }), cost: None, duration_ms: 0 })
        }
    }

    // ─── Task 1.4: PermissionSet deny/allow tests ─────────────────────────

    #[tokio::test]
    async fn dispatch_blocks_when_permission_set_denies() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(NamedEchoTool::new(executed.clone(), "bash"));
        let d = make_dispatcher(Arc::new(reg));

        let perms = crate::automation::runtime::PermissionSet {
            spec: vec![],
            granted: vec![],
            denied: vec![crate::automation::protocol::humane_v1::Permission::Shell],
        };
        let mut c = ctx();
        c.permissions = Some(perms);
        c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-1".into() };

        let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &c).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_err());
        assert!(outs[0].is_error);
        assert_eq!(outs[0].message_content, "Error: tool denied by spec");
        assert!(!executed.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn dispatch_auto_approves_when_permission_set_allows() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(NamedEchoTool::new(executed.clone(), "bash"));
        let d = make_dispatcher(Arc::new(reg));

        let perms = crate::automation::runtime::PermissionSet {
            spec: vec![crate::automation::protocol::humane_v1::Permission::Shell],
            granted: vec![],
            denied: vec![],
        };
        let mut c = ctx();
        c.permissions = Some(perms);
        c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-2".into() };

        let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &c).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok(), "permitted tool must execute");
        assert!(executed.load(Ordering::SeqCst));
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
    ) -> (Arc<ToolDispatcher<MockRuntime>>, Arc<crate::app::PendingApprovals>) {
        let app = tauri::test::mock_app();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        let d = Arc::new(ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals.clone(),
            None,
            None,
            None,
            hook_bus,
            None, // heartbeat: None for tests
        ));
        (d, pending_approvals)
    }

    /// Build a dispatcher with a caller-provided `Arc<dyn ApprovalHandler>`.
    /// Used by `dispatch_escalated_outcome_has_correct_shape` to inject an
    /// always-Escalated handler without touching SafetyManager plumbing.
    fn make_dispatcher_with_custom_handler(
        tools: Arc<ToolRegistry>,
        handler: Arc<dyn crate::safety::ApprovalHandler>,
    ) -> Arc<ToolDispatcher<MockRuntime>> {
        let app = tauri::test::mock_app();
        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        // Force Ask mode so tools hit RequireApproval and reach the handler.
        use crate::safety::SafetyMode;
        let _ = mgr.set_global_mode(SafetyMode::Ask);
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());
        Arc::new(ToolDispatcher::new_with_approval_handler(
            tools,
            app.handle().clone(),
            safety_manager,
            handler,
            pending_approvals,
            None,
            None,
            None,
            hook_bus,
            None,
        ))
    }

    /// Build a dispatcher with a caller-provided `Arc<HookBus>`.
    /// Mirrors `make_dispatcher_with_policy` — only the hook_bus arg differs.
    /// Allows tests to inject a bus carrying denying or asking subscribers.
    fn make_dispatcher_with_bus(
        tools: Arc<ToolRegistry>,
        bus: Arc<HookBus>,
    ) -> Arc<ToolDispatcher<MockRuntime>> {
        let app = tauri::test::mock_app();
        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        mgr.set_policy(SafetyPolicy::default()).ok();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());
        Arc::new(ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals,
            None,
            None,
            None,
            bus,
            None, // heartbeat: None for tests
        ))
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
            cancel: None,
            permissions: None,
            origin_kind: ApprovalOriginKind::Chat { conversation_id: "sess".into() },
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
            cancel: None,
            permissions: None,
            origin_kind: ApprovalOriginKind::Chat { conversation_id: "sess2".into() },
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

    // ─── Serial / Parallel lane tests ────────────────────────────────────

    /// A tool that declares itself Parallel-safe (pure reader stub).
    struct ParallelTool {
        executed: Arc<AtomicBool>,
    }

    impl ParallelTool {
        fn new(executed: Arc<AtomicBool>) -> Self { Self { executed } }
    }

    #[async_trait]
    impl Tool for ParallelTool {
        fn name(&self) -> &str { "par_tool" }
        fn description(&self) -> &str { "parallel stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn concurrency(&self) -> crate::agent::tools::tool::ToolConcurrency {
            crate::agent::tools::tool::ToolConcurrency::Parallel
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "par": true }), cost: None, duration_ms: 0 })
        }
    }

    /// A tool that declares itself Sequential (default; stub for clarity).
    struct SeqTool {
        executed: Arc<AtomicBool>,
    }

    impl SeqTool {
        fn new(executed: Arc<AtomicBool>) -> Self { Self { executed } }
    }

    #[async_trait]
    impl Tool for SeqTool {
        fn name(&self) -> &str { "seq_tool" }
        fn description(&self) -> &str { "sequential stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn concurrency(&self) -> crate::agent::tools::tool::ToolConcurrency {
            crate::agent::tools::tool::ToolConcurrency::Sequential
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "seq": true }), cost: None, duration_ms: 0 })
        }
    }

    /// Both a Parallel tool and a Sequential tool execute, and outcomes arrive
    /// in input order: [par_tool call, seq_tool call] → [par outcome, seq outcome].
    #[tokio::test]
    async fn parallel_and_sequential_both_execute() {
        let par_exec = Arc::new(AtomicBool::new(false));
        let seq_exec = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(ParallelTool::new(par_exec.clone()));
        reg.register(SeqTool::new(seq_exec.clone()));

        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![
            ToolCall { id: "p1".into(), name: "par_tool".into(), arguments: json!({}) },
            ToolCall { id: "s1".into(), name: "seq_tool".into(), arguments: json!({}) },
        ];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 2, "must have exactly two outcomes");
        // Order preservation: first outcome is for par_tool, second for seq_tool.
        assert_eq!(outs[0].tool_call_id, "p1", "first outcome must match first call");
        assert_eq!(outs[1].tool_call_id, "s1", "second outcome must match second call");
        assert!(outs[0].result.is_ok(), "par_tool should succeed");
        assert!(outs[1].result.is_ok(), "seq_tool should succeed");
        assert!(par_exec.load(Ordering::SeqCst), "ParallelTool must have executed");
        assert!(seq_exec.load(Ordering::SeqCst), "SeqTool must have executed");
    }

    // ─── Streaming coalescer test ─────────────────────────────────────────

    /// A tool that advertises streaming support, sends 2 chunks via the sink,
    /// and sets a flag only in execute_streaming (to prove that code path ran).
    struct StreamingStubTool {
        streaming_path_taken: Arc<AtomicBool>,
    }

    impl StreamingStubTool {
        fn new(flag: Arc<AtomicBool>) -> Self { Self { streaming_path_taken: flag } }
    }

    #[async_trait]
    impl Tool for StreamingStubTool {
        fn name(&self) -> &str { "streaming_stub" }
        fn description(&self) -> &str { "streaming stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn supports_streaming(&self) -> bool { true }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            // Should NOT be called when supports_streaming() == true.
            Ok(ToolOutput { result: json!({ "via": "execute" }), cost: None, duration_ms: 0 })
        }
        async fn execute_streaming(
            &self,
            _params: serde_json::Value,
            sink: crate::agent::tools::stream::ToolStreamSink,
        ) -> Result<ToolOutput, ToolError> {
            // Mark that streaming path was taken.
            self.streaming_path_taken.store(true, Ordering::SeqCst);
            sink.send(crate::agent::tools::stream::ToolStream::Stdout, b"chunk1");
            sink.send(crate::agent::tools::stream::ToolStream::Stderr, b"chunk2");
            Ok(ToolOutput { result: json!({ "via": "execute_streaming" }), cost: None, duration_ms: 0 })
        }
    }

    /// streaming_tool_runs_via_execute_streaming:
    /// A tool with supports_streaming()==true whose execute_streaming sends 2
    /// chunks via the sink then returns Ok. Assert outcome is Ok and the
    /// execute_streaming code path was taken (AtomicBool set only there).
    #[tokio::test]
    async fn streaming_tool_runs_via_execute_streaming() {
        let streaming_flag = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(StreamingStubTool::new(streaming_flag.clone()));

        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![
            ToolCall { id: "st1".into(), name: "streaming_stub".into(), arguments: json!({}) },
        ];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert_eq!(outs[0].tool_call_id, "st1");
        assert!(outs[0].result.is_ok(), "streaming tool should return Ok");
        assert!(
            streaming_flag.load(Ordering::SeqCst),
            "execute_streaming code path must have been taken (streaming_path_taken flag not set)"
        );
        // Confirm the result came from execute_streaming, not execute().
        let result_val = outs[0].result.as_ref().unwrap();
        assert_eq!(result_val.result["via"], "execute_streaming");
    }

    // ─── Hook fire tests ─────────────────────────────────────────────────

    /// A `HookSubscriber` that captures all events it receives into a shared vec.
    struct CapturingSubscriber {
        captured: Arc<std::sync::Mutex<Vec<crate::agent::hook_bus::HookEvent>>>,
        kinds: &'static [crate::agent::hook_bus::HookEventKind],
    }

    #[async_trait]
    impl crate::agent::hook_bus::HookSubscriber for CapturingSubscriber {
        fn id(&self) -> crate::agent::hook_bus::SubscriberId {
            crate::agent::hook_bus::SubscriberId::new("test-capture")
        }
        fn interest_in(&self) -> &'static [crate::agent::hook_bus::HookEventKind] {
            self.kinds
        }
        async fn on_event(
            &self,
            event: &crate::agent::hook_bus::HookEvent,
        ) -> Option<crate::runtime::contracts::HookDecision> {
            self.captured.lock().unwrap().push(event.clone());
            None
        }
    }

    /// Build a dispatcher with a custom HookBus that has the CapturingSubscriber
    /// pre-registered, and return the shared captured-events vec alongside the
    /// dispatcher.
    fn make_dispatcher_with_hook_capture(
        tools: Arc<ToolRegistry>,
    ) -> (Arc<ToolDispatcher<MockRuntime>>, Arc<std::sync::Mutex<Vec<crate::agent::hook_bus::HookEvent>>>) {
        let app = tauri::test::mock_app();
        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        mgr.set_policy(SafetyPolicy::default()).ok();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(crate::app::PendingApprovals::new());

        let captured: Arc<std::sync::Mutex<Vec<crate::agent::hook_bus::HookEvent>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let sub = Arc::new(CapturingSubscriber {
            captured: captured.clone(),
            kinds: &[
                crate::agent::hook_bus::HookEventKind::PreToolUse,
                crate::agent::hook_bus::HookEventKind::PostToolUse,
            ],
        });
        let mut bus = HookBus::new();
        bus.register(sub).unwrap();

        let dispatcher = Arc::new(ToolDispatcher::new(
            tools,
            app.handle().clone(),
            safety_manager,
            pending_approvals,
            None,
            None,
            None,
            Arc::new(bus),
            None, // heartbeat: None for tests
        ));
        (dispatcher, captured)
    }

    /// Dispatching one tool call fires exactly one `PreToolUse` (before execute)
    /// and one `PostToolUse` (after execute) with the correct `tool_name`.
    #[tokio::test]
    async fn hook_bus_fires_pre_and_post_tool_use_events() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));

        let (d, captured) = make_dispatcher_with_hook_capture(Arc::new(reg));
        let calls = vec![ToolCall {
            id: "h1".into(),
            name: "echo".into(),
            arguments: json!({ "msg": "hello" }),
        }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok(), "tool should succeed");

        let events = captured.lock().unwrap();
        assert_eq!(events.len(), 2, "expected exactly one PreToolUse + one PostToolUse");

        // First event must be PreToolUse with the right tool_name.
        match &events[0] {
            crate::agent::hook_bus::HookEvent::PreToolUse { tool_name, .. } => {
                assert_eq!(tool_name, "echo", "PreToolUse tool_name mismatch");
            }
            other => panic!("expected PreToolUse, got {:?}", other),
        }

        // Second event must be PostToolUse with the right tool_name and success=true.
        match &events[1] {
            crate::agent::hook_bus::HookEvent::PostToolUse { tool_name, success, .. } => {
                assert_eq!(tool_name, "echo", "PostToolUse tool_name mismatch");
                assert!(*success, "PostToolUse success should be true for a clean echo");
            }
            other => panic!("expected PostToolUse, got {:?}", other),
        }
    }

    // ─── Step 3: preview_target_path consulted by emit_tool_start ────────
    //
    // A tool that overrides `preview_target_path` to return `Some("/preview/x")`.
    // Dispatching it should succeed (proving emit_tool_start ran without panic
    // and the tool continued to execute). Full event verification is covered by
    // manual smoke (frontend tool-activity indicator shows previewTarget).
    struct PreviewTool {
        executed: Arc<AtomicBool>,
    }

    impl PreviewTool {
        fn new(executed: Arc<AtomicBool>) -> Self { Self { executed } }
    }

    #[async_trait]
    impl Tool for PreviewTool {
        fn name(&self) -> &str { "preview_tool" }
        fn description(&self) -> &str { "preview stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> crate::agent::tools::tool::ApprovalRequirement {
            crate::agent::tools::tool::ApprovalRequirement::Never
        }
        /// Override to return a non-None preview target — exercises the
        /// `emit_tool_start` path that consults `preview_target_path`.
        fn preview_target_path(&self, _args: &serde_json::Value) -> Option<String> {
            Some("/preview/x".to_string())
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "previewed": true }), cost: None, duration_ms: 0 })
        }
    }

    /// emit_tool_start consults preview_target_path: tool with non-None preview
    /// target still executes successfully (emit_tool_start ran, no panic, tool ran).
    #[tokio::test]
    async fn emit_tool_start_consults_preview_target_path_and_tool_executes() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(PreviewTool::new(executed.clone()));

        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![ToolCall {
            id: "pt1".into(),
            name: "preview_tool".into(),
            arguments: json!({}),
        }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok(), "tool should succeed after emit_tool_start");
        assert!(executed.load(Ordering::SeqCst), "tool must have executed");
        // is_error=false for a clean result.
        assert!(!outs[0].is_error, "is_error should be false for clean success");
        // soft_error=None for a clean result.
        assert!(outs[0].soft_error.is_none(), "soft_error should be None for clean success");
        // rejected=false.
        assert!(!outs[0].rejected, "rejected should be false");
    }

    // ─── Step 4: outcome field coverage (was_mutation, soft_error, ────────
    //              message_content, is_error, Parallel/Sequential parity)

    /// A tool that simulates a soft error (returns Ok but with { ok:false, exit_code:1, stderr: "boom" }).
    struct SoftErrorTool;

    #[async_trait]
    impl Tool for SoftErrorTool {
        fn name(&self) -> &str { "soft_err_tool" }
        fn description(&self) -> &str { "soft error stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> crate::agent::tools::tool::ApprovalRequirement {
            crate::agent::tools::tool::ApprovalRequirement::Never
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            // Soft error: ok=false + exit_code non-zero → detect_soft_tool_error returns true.
            Ok(ToolOutput {
                result: json!({ "ok": false, "exit_code": 1, "stderr": "boom" }),
                cost: None,
                duration_ms: 0,
            })
        }
    }

    /// A mutating tool (name matches is_mutating_tool heuristic via write_file name).
    struct MutatingTool {
        executed: Arc<AtomicBool>,
    }

    impl MutatingTool {
        fn new(executed: Arc<AtomicBool>) -> Self { Self { executed } }
    }

    #[async_trait]
    impl Tool for MutatingTool {
        fn name(&self) -> &str { "write_file" }
        fn description(&self) -> &str { "mutating stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> crate::agent::tools::tool::ApprovalRequirement {
            crate::agent::tools::tool::ApprovalRequirement::Never
        }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "written": true }), cost: None, duration_ms: 0 })
        }
    }

    /// outcome fields: is_error=true + soft_error=Some for a soft-error result.
    #[tokio::test]
    async fn soft_error_outcome_carries_is_error_and_soft_error_text() {
        let mut reg = ToolRegistry::new();
        reg.register(SoftErrorTool);
        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![ToolCall { id: "se1".into(), name: "soft_err_tool".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok(), "soft error is still Ok(ToolOutput)");
        assert!(outs[0].is_error, "is_error should be true for soft error");
        assert!(outs[0].soft_error.is_some(), "soft_error should be Some for soft error");
        let soft = outs[0].soft_error.as_deref().unwrap_or("");
        assert!(soft.contains("boom"), "soft_error text should contain stderr content");
        assert!(!outs[0].rejected, "soft error is not rejected");
    }

    /// outcome fields: was_mutation reflects is_mutating_tool classification.
    #[tokio::test]
    async fn mutating_tool_outcome_carries_was_mutation_true() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(MutatingTool::new(executed.clone()));
        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![ToolCall { id: "m1".into(), name: "write_file".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok());
        assert!(outs[0].was_mutation, "write_file should be classified as a mutation");
        assert!(!outs[0].is_error);
        // message_content for clean success is the JSON-serialized result string.
        assert!(!outs[0].message_content.is_empty(), "message_content must not be empty for Ok result");
    }

    /// outcome fields: message_content + is_error for hard error (Err(ToolError::Execution)).
    #[tokio::test]
    async fn hard_error_outcome_carries_message_content_and_is_error() {
        // Use a missing tool: yields ToolError::NotFound → is_error=true + message_content "Error: Tool 'x' not found"
        let d = make_dispatcher(Arc::new(ToolRegistry::new()));
        let calls = vec![ToolCall { id: "c1".into(), name: "x".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_err());
        assert!(outs[0].is_error, "is_error must be true for hard error");
        assert!(outs[0].soft_error.is_none(), "soft_error must be None for hard error");
        assert!(
            outs[0].message_content.starts_with("Error:"),
            "message_content for hard error must start with 'Error:'"
        );
    }

    // ─── C1: per-tool panic isolation ───────────────────────────────────────

    /// A tool whose execute() panics. Used to test that the dispatcher converts a
    /// tool panic into ToolError::Execution rather than propagating it to the caller.
    struct PanicTool { parallel: bool }

    #[async_trait]
    impl Tool for PanicTool {
        fn name(&self) -> &str { if self.parallel { "panic_par" } else { "panic_seq" } }
        fn description(&self) -> &str { "panics" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            panic!("boom")
        }
        fn concurrency(&self) -> crate::agent::tools::tool::ToolConcurrency {
            if self.parallel {
                crate::agent::tools::tool::ToolConcurrency::Parallel
            } else {
                crate::agent::tools::tool::ToolConcurrency::Sequential
            }
        }
    }

    /// C1 regression test: a panicking tool must yield ToolError::Execution
    /// ("crashed unexpectedly"), set is_error=true on the outcome, and must NOT
    /// crash the dispatcher itself. Covers both the Sequential lane (non-streaming
    /// execute) and the Parallel lane (also non-streaming execute).
    #[tokio::test]
    async fn panicking_tool_yields_error_outcome_not_crash() {
        let mut reg = ToolRegistry::new();
        reg.register(PanicTool { parallel: false });
        reg.register(PanicTool { parallel: true });
        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![
            ToolCall { id: "s".into(), name: "panic_seq".into(), arguments: json!({}) },
            ToolCall { id: "p".into(), name: "panic_par".into(), arguments: json!({}) },
        ];
        // dispatch must NOT panic — the test completing without a panic is the proof.
        let outs = d.dispatch(calls, &ctx()).await;
        assert_eq!(outs.len(), 2, "must get two outcomes");
        for o in &outs {
            assert!(o.is_error, "panicking tool outcome should have is_error=true");
            match &o.result {
                Err(ToolError::Execution(m)) => {
                    assert!(
                        m.contains("crashed unexpectedly"),
                        "panic message should contain 'crashed unexpectedly', got: {m}"
                    );
                }
                other => panic!("expected Err(ToolError::Execution(_)), got {:?}", other),
            }
        }
    }

    // ─── PreToolUse decision gate tests ─────────────────────────────────────

    /// PreToolUse Deny → rejected outcome + tool NOT executed.
    #[tokio::test]
    async fn pretooluse_deny_blocks_and_skips_execution() {
        use crate::policy_eval::{PolicySpec, PolicyRule, MatchPattern, PolicySpecSubscriber};
        use crate::runtime::contracts::HookDecision;

        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));

        // Build a PolicySpec that denies tool_use for "echo".
        let spec = PolicySpec::new().with_rule(PolicyRule::new(
            "deny-echo",
            MatchPattern::ExactTarget {
                action_class: "tool_use".into(),
                target: "echo".into(),
            },
            HookDecision::Deny { reason: "policy denies echo".into() },
        ));
        let mut bus = HookBus::new();
        bus.register(Arc::new(PolicySpecSubscriber::new(spec))).unwrap();

        let d = make_dispatcher_with_bus(Arc::new(reg), Arc::new(bus));
        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].rejected, "denied tool must be rejected");
        assert!(outs[0].result.is_err(), "denied tool result must be Err");
        assert!(!executed.load(Ordering::SeqCst), "denied tool must NOT execute");
    }

    /// PreToolUse Allow (empty policy) → tool executes normally.
    #[tokio::test]
    async fn pretooluse_allow_executes() {
        // Empty PolicySpec (Allow-all) bus → echo executes normally.
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));

        let bus = HookBus::new(); // no subscribers → Allow
        let d = make_dispatcher_with_bus(Arc::new(reg), Arc::new(bus));
        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x": 1}) }];
        let outs = d.dispatch(calls, &ctx()).await;

        assert_eq!(outs.len(), 1);
        assert!(!outs[0].rejected, "allowed tool must not be rejected");
        assert!(outs[0].result.is_ok(), "allowed tool result must be Ok");
        assert!(executed.load(Ordering::SeqCst), "allowed tool must execute");
    }

    /// Parallel / Sequential parity: both lanes produce outcomes carrying the
    /// same outcome fields (paths_touched empty when tool has no path_args,
    /// was_mutation=false for read-only stubs, is_error=false, rejected=false).
    #[tokio::test]
    async fn parallel_sequential_outcomes_carry_full_field_set() {
        let par_exec = Arc::new(AtomicBool::new(false));
        let seq_exec = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(ParallelTool::new(par_exec.clone()));
        reg.register(SeqTool::new(seq_exec.clone()));

        let d = make_dispatcher(Arc::new(reg));
        let calls = vec![
            ToolCall { id: "p1".into(), name: "par_tool".into(), arguments: json!({"k": "v"}) },
            ToolCall { id: "s1".into(), name: "seq_tool".into(), arguments: json!({"k": "v"}) },
        ];
        let outs = d.dispatch(calls, &ctx()).await;
        assert_eq!(outs.len(), 2);

        for (i, out) in outs.iter().enumerate() {
            assert!(out.result.is_ok(), "outcome[{i}] should be Ok");
            assert!(!out.is_error, "outcome[{i}].is_error should be false");
            assert!(out.soft_error.is_none(), "outcome[{i}].soft_error should be None");
            assert!(!out.rejected, "outcome[{i}].rejected should be false");
            assert!(!out.was_mutation, "outcome[{i}].was_mutation should be false for read-only stubs");
            assert!(out.paths_touched.is_empty(), "outcome[{i}].paths_touched should be empty (no path_args)");
            assert!(!out.message_content.is_empty(), "outcome[{i}].message_content must not be empty");
            assert_eq!(out.arguments, json!({"k": "v"}), "outcome[{i}].arguments must match input");
        }
    }

    // ─── Slice 1b fix: Escalated outcome shape ───────────────────────────

    /// ApprovalHandler that always returns Escalated — used to verify that
    /// the Escalated arm in approve() produces the correct outcome shape.
    struct EscalatingHandler;

    #[async_trait]
    impl crate::safety::ApprovalHandler for EscalatingHandler {
        async fn handle_ask(
            &self,
            _tool_name: &str,
            _arguments: &serde_json::Value,
            _origin: &crate::safety::ApprovalOrigin,
        ) -> crate::safety::ApprovalOutcome {
            crate::safety::ApprovalOutcome::Escalated
        }
    }

    /// A tool that requires approval unconditionally (ApprovalRequirement::Always).
    /// Used by `dispatch_escalated_outcome_has_correct_shape` so the SafetyManager
    /// returns RequireApproval regardless of the policy mode, which then routes
    /// through the approval_handler and reaches EscalatingHandler.
    /// `pub(crate)` so browser::agent_loop tests can reuse it for the
    /// subloop_dispatch_routes_through_outer_safetymanager contract test.
    pub(crate) struct AlwaysApprovalTool {
        pub(crate) executed: Arc<AtomicBool>,
        pub(crate) tool_name: String,
    }

    impl AlwaysApprovalTool {
        pub(crate) fn new(executed: Arc<AtomicBool>, name: impl Into<String>) -> Self {
            Self { executed, tool_name: name.into() }
        }
    }

    #[async_trait]
    impl Tool for AlwaysApprovalTool {
        fn name(&self) -> &str { &self.tool_name }
        fn description(&self) -> &str { "always-approval stub" }
        fn parameters_schema(&self) -> serde_json::Value { json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> crate::agent::tools::tool::ApprovalRequirement {
            crate::agent::tools::tool::ApprovalRequirement::Always
        }
        async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            self.executed.store(true, Ordering::SeqCst);
            Ok(ToolOutput { result: json!({ "echoed": params }), cost: None, duration_ms: 0 })
        }
    }

    /// dispatch_escalated_outcome_has_correct_shape:
    /// When ApprovalHandler returns Escalated for a non-chat origin,
    /// the outcome must have:
    ///   - rejected: false  (HeadlessDelegate keys on this to distinguish from denial)
    ///   - is_error: true
    ///   - message_content: "Error: awaiting user approval"
    ///   - tool must NOT have executed
    #[tokio::test]
    async fn dispatch_escalated_outcome_has_correct_shape() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        // AlwaysApprovalTool ensures requires_approval returns Always so the
        // SafetyManager hits RequireApproval (not AutoApprove) even in Ask mode,
        // and routes through approval_handler which returns Escalated.
        reg.register(AlwaysApprovalTool::new(executed.clone(), "bash"));
        let d = make_dispatcher_with_custom_handler(
            Arc::new(reg),
            Arc::new(EscalatingHandler),
        );

        let mut c = ctx();
        c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-esc".into() };

        let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &c).await;

        assert_eq!(outs.len(), 1);
        assert!(outs[0].is_error, "escalated outcome must be is_error: true");
        assert!(
            !outs[0].rejected,
            "escalated outcomes must NOT be marked rejected (Task 2 distinguishes by this field)"
        );
        assert_eq!(
            outs[0].message_content,
            "Error: awaiting user approval",
            "escalated outcome message_content mismatch"
        );
        assert!(
            !executed.load(Ordering::SeqCst),
            "tool must not run when escalated"
        );
    }

    // ─── Task 2.4: Automation origin + uncovered permission → Escalated ──────
    //
    // The PermissionSet grants Filesystem but NOT Shell. "bash" maps to Shell →
    // FallThrough → SafetyManager returns RequireApproval (Ask mode is set by
    // make_dispatcher_with_custom_handler). The EscalatingHandler returns Escalated.
    //
    // Note: the tool must use ApprovalRequirement::Always so that RequireApproval
    // is triggered even in Ask mode (tools with Never skip the user-ask path and
    // are auto-approved). AlwaysApprovalTool satisfies this.

    #[tokio::test]
    async fn dispatch_with_automation_origin_and_permissions_escalates_uncovered() {
        use crate::automation::protocol::humane_v1::Permission;

        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        // AlwaysApprovalTool named "bash" — Shell maps to Shell permission which is
        // NOT in the PermissionSet → FallThrough → Ask mode → RequireApproval →
        // EscalatingHandler → Escalated outcome.
        reg.register(AlwaysApprovalTool::new(executed.clone(), "bash"));

        // PermissionSet grants Filesystem but NOT Shell.
        let perms = crate::automation::runtime::PermissionSet {
            spec: vec![Permission::Filesystem],
            granted: vec![],
            denied: vec![],
        };

        let d = make_dispatcher_with_custom_handler(Arc::new(reg), Arc::new(EscalatingHandler));

        let mut c = ctx();
        c.origin_kind = ApprovalOriginKind::Automation { activity_id: "act-1".into() };
        c.permissions = Some(perms);

        let calls = vec![ToolCall { id: "c1".into(), name: "bash".into(), arguments: json!({}) }];
        let outs = d.dispatch(calls, &c).await;

        assert_eq!(outs.len(), 1);
        // Escalation shape: rejected=false, is_error=true, specific message_content.
        assert!(!outs[0].rejected, "escalated outcomes must NOT be marked rejected");
        assert!(outs[0].is_error);
        assert_eq!(outs[0].message_content, "Error: awaiting user approval");
        assert!(!executed.load(Ordering::SeqCst), "uncovered escalated tool must not execute");
    }

    // ─── Task 1.5: ChatApprovalHandler byte-equivalence test ─────────────

    #[tokio::test]
    async fn dispatch_with_chat_approval_handler_byte_equivalent_to_pending_approvals() {
        let executed = Arc::new(AtomicBool::new(false));
        let mut reg = ToolRegistry::new();
        reg.register(EchoTool::new(executed.clone()));

        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        // Set Ask mode so echo must go through RequireApproval
        use crate::safety::SafetyPolicy;
        let mut policy = SafetyPolicy::default();
        policy.global_mode = crate::safety::SafetyMode::Ask;
        mgr.set_policy(policy).ok();

        let (d, pa) = make_dispatcher_with_safety_manager(Arc::new(reg), mgr);

        let c = ctx();
        let calls = vec![ToolCall { id: "c1".into(), name: "echo".into(), arguments: json!({"x":1}) }];

        let d_clone = d.clone();
        let task = tokio::spawn(async move {
            d_clone.dispatch(calls, &c).await
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        // Chat path uses tool_call_id ("c1") as the pending approval key
        pa.resolve("c1", crate::app::ApprovalResult {
            approved: true,
            always_allow: false,
            tool_name: None,
            path_scope: None,
            paths: None,
        });

        let outs = task.await.unwrap();
        assert_eq!(outs.len(), 1);
        assert!(outs[0].result.is_ok(), "approved tool must execute");
        assert!(executed.load(Ordering::SeqCst));
    }
}
