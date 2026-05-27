//! ToolDispatcher —— 从 ChatDelegate 抽离的工具派发缝(Sprint 3 ①)。
//! loop-agnostic:不依赖 ReasoningContext;reason_ctx bookkeeping 经 outcome 上报。
use std::path::PathBuf;
use std::sync::Arc;
use crate::agent::tools::tool::{Tool, ToolRegistry, ToolOutput, ToolError};
use crate::safety::{SafetyMode, ApprovalDecision};
use uclaw_tool_types::ToolCall;
use tauri::Emitter;

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

    /// 派发一组 tool calls,返回每个的结构化 outcome(输入顺序)。
    ///
    /// 按 `tool.concurrency()` 分道:
    /// - `ToolConcurrency::Parallel`  → 收进 JoinSet 批次并发执行;
    /// - `ToolConcurrency::Sequential`→ 内联串行执行。
    /// 结果按输入下标还原顺序后返回。
    /// `self: &Arc<Self>` 满足 JoinSet spawn 的 'static 约束。
    pub async fn dispatch(self: &Arc<Self>, calls: Vec<ToolCall>, ctx: &ToolDispatchContext) -> Vec<ToolDispatchOutcome>
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

        // ── 审批门(移植自 dispatcher.rs:2490-2601) ──────────────────────
        match self.approve(tool, tc, ctx).await {
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

        // ── PreToolUse hook (observe-only, after all gates pass) ────────────
        self.hook_bus.dispatch_observe(&crate::agent::hook_bus::HookEvent::PreToolUse {
            task_id: ctx.session_id.clone(),
            tool_name: tc.name.clone(),
            args_json: tc.arguments.to_string(),
        }).await;

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
                    use crate::harness::trajectory::TurnRecord;
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

    /// 执行工具:流式工具搭 coalescer drain,否则直接 execute。移植自 dispatcher.rs:2751-2834。
    ///
    /// 流式路径:channel(256) → spawned drain task(~50ms / 8KB flush) → execute_streaming → drop(sink) → handle.await。
    /// 非流式路径:直接 execute。
    async fn run_tool(&self, tool: &dyn Tool, tc: &ToolCall, ctx: &ToolDispatchContext) -> Result<ToolOutput, ToolError>
    where
        R: 'static,
    {
        // Inject `_tool_call_id` into the args before execute, mirroring the old
        // ChatDelegate::execute_tool_calls behavior. load_skill / skill_search read
        // `params["_tool_call_id"]` to stamp the `toolCallId` on their UI events
        // (agent:skill-recalled). Without this they'd emit an empty toolCallId.
        let args = {
            let mut a = tc.arguments.clone();
            if let Some(obj) = a.as_object_mut() {
                obj.insert("_tool_call_id".to_string(), serde_json::Value::String(tc.id.clone()));
            } else {
                tracing::warn!(
                    tool = %tc.name,
                    "tool arguments is not a JSON object; skipping _tool_call_id injection"
                );
            }
            a
        };
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
            // execute_streaming owns the sink clone; original sink is dropped after execute returns.
            let sink_clone = sink.clone();
            let result = tool.execute_streaming(args, sink_clone).await;
            // 工具结束 → 关 sink(drop) → coalescer 收尾 flush 后退出。
            drop(sink);
            let _ = handle.await;
            result
        } else {
            tool.execute(args).await
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
                // Old block path pushed `format!("Error: Tool blocked — {}", reason)`.
                let message = format!("Error: Tool blocked — {}", reason);
                ApprovalGate::Rejected { reason, message }
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
                    // Old user-reject path pushed `"Error: Tool execution was rejected by the user."`.
                    // The bare `reason` (no "Error: " prefix) is what the old hard-error path
                    // would have produced via `format!("Error: {e}")` — but the old code took the
                    // rejection `continue` branch BEFORE execute, pushing the prefixed literal.
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
        ));
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
}
