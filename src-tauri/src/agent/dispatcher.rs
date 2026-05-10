use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::types::*;
use crate::agent::tools::tool::{ToolRegistry, ToolOutput};
use crate::app::PendingApprovals;
use crate::infra::InfraService;
use crate::llm::LlmProvider;
use crate::llm::stream_error::{classify_stream_error, StreamErrorKind};
use crate::error::Error;
use crate::safety::{SafetyManager, SafetyMode, ApprovalDecision};

/// Maximum number of stream retries before surfacing the error.
/// Each retry only fires after a real stall or transient network error,
/// not on every iteration — so the worst-case wall time is bounded by
/// (stall_timeout + retry_overhead) × this many.
const MAX_STREAM_RETRIES: u32 = 2;

/// ChatDelegate implements LoopDelegate for chat-based interactions.
/// It assembles the conversation context, delegates LLM calls, and executes tools.
pub struct ChatDelegate {
    /// LLM provider for making API calls
    llm: Arc<dyn LlmProvider>,
    /// Tool registry for executing tool calls
    tools: Arc<ToolRegistry>,
    /// Tauri app handle for emitting events to frontend
    app_handle: tauri::AppHandle,
    /// LLM model to use
    model: String,
    /// System prompt for the conversation
    system_prompt: String,
    /// External stop flag — set to true to gracefully stop the loop.
    stop_flag: Arc<AtomicBool>,
    /// Safety manager for tool approval decisions
    safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,
    /// Safety mode for this session (overrides global if set)
    safety_mode: Option<SafetyMode>,
    /// Pending approvals registry for awaiting user decisions
    pending_approvals: Arc<PendingApprovals>,
    /// Conversation ID for this session (used in approval events)
    conversation_id: String,
    /// Optional memory context to prepend to system prompt (from recall engine)
    memory_context: Option<String>,
    /// InfraService for publishing tool execution events
    infra_service: Option<Arc<InfraService>>,
    /// Optional trajectory store for recording tool turns
    trajectory_store: Option<Arc<crate::harness::TrajectoryStore>>,
    /// Optional tool budget manager for truncating large results
    tool_budget: Option<Arc<crate::harness::ToolBudgetManager>>,
    /// Monotonic turn counter across all tool calls in this session
    turn_index: Arc<AtomicU32>,
    /// Whether extended thinking/reasoning is enabled for this session
    thinking_enabled: bool,
    /// Per-session monotonic sequence counter for chat:stream-reasoning events.
    /// Lets the frontend deduplicate events that arrive more than once (e.g. due
    /// to HMR or React Strict Mode registering multiple listeners).
    thinking_seq: Arc<AtomicU64>,
    /// Workspace root used to source `uclaw.md` for prompt composition.
    workspace_root: Option<std::path::PathBuf>,
}

impl ChatDelegate {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        tools: Arc<ToolRegistry>,
        app_handle: tauri::AppHandle,
        model: String,
        system_prompt: String,
        safety_manager: Arc<tokio::sync::RwLock<SafetyManager>>,
        safety_mode: Option<SafetyMode>,
        pending_approvals: Arc<PendingApprovals>,
        conversation_id: String,
        workspace_root: Option<std::path::PathBuf>,
    ) -> Self {
        Self {
            llm, tools, app_handle, model, system_prompt,
            stop_flag: Arc::new(AtomicBool::new(false)),
            safety_manager,
            safety_mode,
            pending_approvals,
            conversation_id,
            memory_context: None,
            infra_service: None,
            trajectory_store: None,
            tool_budget: None,
            turn_index: Arc::new(AtomicU32::new(0)),
            thinking_enabled: false,
            thinking_seq: Arc::new(AtomicU64::new(0)),
            workspace_root,
        }
    }

    /// Enable or disable extended thinking for this session.
    pub fn set_thinking_enabled(&mut self, enabled: bool) {
        self.thinking_enabled = enabled;
    }

    /// Set the InfraService for publishing tool execution events.
    pub fn set_infra_service(&mut self, infra: Arc<InfraService>) {
        self.infra_service = Some(infra);
    }

    /// Set the TrajectoryStore for recording tool execution turns.
    pub fn set_trajectory_store(&mut self, store: Arc<crate::harness::TrajectoryStore>) {
        self.trajectory_store = Some(store);
    }

    /// Set the ToolBudgetManager for truncating large tool results.
    pub fn set_tool_budget(&mut self, budget: Arc<crate::harness::ToolBudgetManager>) {
        self.tool_budget = Some(budget);
    }

    /// Set the memory context obtained from the recall engine.
    /// This will be appended to the system prompt when making LLM calls.
    pub fn set_memory_context(&mut self, context: String) {
        self.memory_context = Some(context);
    }

    /// Build the effective system prompt including memory context, the user's
    /// uclaw.md (workspace-level), Karpathy baseline, and mode-specific
    /// guardrails. Reads uclaw.md on every call (small file, OS cache).
    ///
    /// `effective_mode` should be the current resolved SafetyMode — usually
    /// the global policy mode (or the per-session override when set). Caller
    /// resolves it before invoking; we don't read SafetyManager here because
    /// this method is sync and called from the LLM hot path.
    fn effective_system_prompt(&self, effective_mode: &SafetyMode) -> String {
        let memory_block = self.memory_context.as_deref().filter(|s| !s.is_empty());
        let base_with_memory = match memory_block {
            Some(ctx) => format!("{}\n\n{}", self.system_prompt, ctx),
            None => self.system_prompt.clone(),
        };
        crate::agent::mode_prompts::compose_system_prompt(
            &base_with_memory,
            self.workspace_root.as_deref(),
            effective_mode,
        )
    }

    /// Resolve the effective SafetyMode for this call: per-session override
    /// (dispatcher's `safety_mode` field) wins; otherwise read the global
    /// policy mode from SafetyManager.
    async fn resolve_effective_mode(&self) -> SafetyMode {
        if let Some(m) = self.safety_mode.as_ref() {
            return m.clone();
        }
        self.safety_manager.read().await.policy().global_mode.clone()
    }

    /// Returns a cloneable handle that can be used to signal the loop to stop.
    pub fn stop_handle(&self) -> Arc<AtomicBool> {
        self.stop_flag.clone()
    }

    /// Emit a text delta to the frontend
    fn emit_text_delta(&self, chunk: &str) {
        let _ = self.app_handle.emit("chat:stream-chunk", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": chunk,
        }));
    }

    /// Emit a tool call start to the frontend
    fn emit_tool_start(&self, name: &str, id: &str, input: &serde_json::Value) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_start",
                "toolName": name,
                "toolCallId": id,
                "input": input,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            }
        }));
    }

    /// Emit a tool result to the frontend
    fn emit_tool_result(&self, name: &str, id: &str, output: &ToolOutput) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_result",
                "toolName": name,
                "toolCallId": id,
                "result": output.result,
                "durationMs": output.duration_ms,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                // Soft-error detection: tools like bash signal failure via
                // { ok: false, exit_code: 1, ... } even though the underlying
                // execution succeeded. Mirror the frontend fallback here so
                // both the live render and the persisted snapshot see the
                // same isError value.
                "isError": detect_soft_tool_error(&output.result),
            }
        }));
    }

    /// Emit a completion event to the frontend
    fn emit_done(&self, text: &str) {
        let _ = self.app_handle.emit("chat:stream-complete", serde_json::json!({
            "conversationId": self.conversation_id,
            "text": text,
        }));
    }

    /// Emit thinking block to the frontend
    fn emit_thinking(&self, text: &str) {
        let seq = self.thinking_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.app_handle.emit("chat:stream-reasoning", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": text,
            "seq": seq,
        }));
    }

    /// Emit thinking-done event to the frontend
    fn emit_thinking_done(&self, _duration_ms: u64) {
        // Absorbed into the reasoning stream; no separate frontend event needed
    }

    /// Emit error to the frontend
    fn emit_error(&self, error: &str) {
        let _ = self.app_handle.emit("chat:stream-error", serde_json::json!({
            "conversationId": self.conversation_id,
            "error": error,
        }));
    }

    /// Emit turn cost event after each LLM call
    fn emit_turn_cost(&self, usage: &TokenUsage) {
        let cost = calculate_cost(&self.model, usage.input_tokens, usage.output_tokens);
        let turn_cost = TurnCostInfo {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cost_usd: format_cost(cost),
        };
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cost_usd = %turn_cost.cost_usd,
            "Emitting agent:turn_cost"
        );

        // Persist BEFORE emitting so the dashboard never undercounts even if
        // the frontend listener races. Best-effort — failures don't propagate.
        use tauri::Manager;
        if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
            crate::cost_store::record(
                &state,
                &self.conversation_id,
                &self.model,
                usage.input_tokens,
                usage.output_tokens,
            );
        }

        let _ = self.app_handle.emit("agent:turn_cost", serde_json::json!({
            "conversationId": self.conversation_id,
            "inputTokens": turn_cost.input_tokens,
            "outputTokens": turn_cost.output_tokens,
            "costUsd": turn_cost.cost_usd,
        }));
    }

    /// Emit context stats after each LLM call
    fn emit_context_stats(&self, messages: &[ChatMessage], cumulative_input: u32, cumulative_output: u32) {
        let model_context_length = get_model_context_length(&self.model);
        let system_prompt_tokens = estimate_tokens(&self.system_prompt);
        let mut messages_tokens: u32 = 0;
        let mut tool_use_tokens: u32 = 0;

        for msg in messages {
            for block in &msg.content {
                match block {
                    ContentBlock::ToolUse { name, input, .. } => {
                        tool_use_tokens += estimate_tokens(name)
                            + estimate_tokens(&input.to_string()) + 10;
                    }
                    ContentBlock::ToolResult { content, .. } => {
                        tool_use_tokens += estimate_tokens(content) + 5;
                    }
                    ContentBlock::Text { text } => {
                        messages_tokens += estimate_tokens(text);
                    }
                    ContentBlock::Thinking { thinking } => {
                        messages_tokens += estimate_tokens(thinking);
                    }
                }
            }
        }

        let compact_buffer = (model_context_length as f32 * 0.033) as u32;
        let used = system_prompt_tokens + messages_tokens + tool_use_tokens + compact_buffer;
        let free = model_context_length as i32 - used as i32;

        let stats = ContextStats {
            model_context_length,
            system_prompt_tokens,
            mcp_prompts_tokens: 0,
            skills_tokens: 0,
            messages_tokens,
            tool_use_tokens,
            compact_buffer_tokens: compact_buffer,
            free_tokens: free,
            cumulative_input_tokens: Some(cumulative_input),
            cumulative_output_tokens: Some(cumulative_output),
        };
        let _ = self.app_handle.emit("agent:context_stats", &stats);
        tracing::info!(
            model_context_length = stats.model_context_length,
            used = stats.model_context_length as i32 - stats.free_tokens,
            free = stats.free_tokens,
            cumulative_input = cumulative_input,
            cumulative_output = cumulative_output,
            "Emitting agent:context_stats"
        );
    }

    /// Emit reflection status change event
    pub fn emit_reflection_status(&self, assistant_message_id: &str, status: &str) {
        let payload = serde_json::json!({
            "assistant_message_id": assistant_message_id,
            "status": status,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        });
        tracing::info!(
            assistant_message_id = %assistant_message_id,
            status = %status,
            "Emitting agent:reflection_status"
        );
        if let Err(e) = self.app_handle.emit("agent:reflection_status", &payload) {
            tracing::warn!("Failed to emit reflection_status: {}", e);
        }
    }

    /// Emit full reflection detail event
    pub fn emit_reflection(&self, detail: &ReflectionDetail) {
        tracing::info!(
            assistant_message_id = %detail.assistant_message_id,
            status = %detail.status,
            outcome = ?detail.outcome,
            summary = ?detail.summary,
            "Emitting agent:reflection"
        );
        if let Err(e) = self.app_handle.emit("agent:reflection", detail) {
            tracing::warn!("Failed to emit reflection: {}", e);
        }
    }

    /// Emit a stream reset event to the frontend, signaling it should
    /// discard any partially received streaming content before fallback.
    fn emit_stream_reset(&self) {
        tracing::debug!("Emitting stream reset to frontend");
        let _ = self.app_handle.emit("agent:stream-reset", serde_json::json!({
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }
}

#[async_trait]
impl LoopDelegate for ChatDelegate {
    async fn check_signals(&self) -> LoopSignal {
        if self.stop_flag.load(Ordering::Relaxed) {
            self.stop_flag.store(false, Ordering::Relaxed);
            return LoopSignal::Stop;
        }
        LoopSignal::Continue
    }

    async fn before_llm_call(&self, _reason_ctx: &mut ReasoningContext, _iteration: usize) -> Option<LoopOutcome> {
        None
    }

    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        // Resolve mode once (per-session override > global policy) so the
        // system prompt actually reflects the user's chosen mode. Without
        // this, dispatcher.safety_mode is None for normal sessions and the
        // composer falls through to Supervised default — meaning Plan/Ask/
        // Bypass/AcceptEdits prompt additions would never reach the LLM,
        // and the agent never learns it should call exit_plan_mode etc.
        let effective_mode = self.resolve_effective_mode().await;
        let effective_prompt = self.effective_system_prompt(&effective_mode);

        let mut messages = vec![ChatMessage::system(&effective_prompt)];
        messages.extend(reason_ctx.messages.clone());

        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            self.tools.list_definitions()
        };
        let config = crate::llm::CompletionConfig {
            model: self.model.clone(),
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: Some(effective_prompt),
            thinking_enabled: self.thinking_enabled,
        };

        tracing::info!(
            model = %self.model,
            message_count = messages.len(),
            tool_count = tools.len(),
            force_text = reason_ctx.force_text,
            "Calling LLM"
        );

        use futures::StreamExt;

        let mut stream_retries: u32 = 0;
        'stream_attempt: loop {
            match self.llm.stream(messages.clone(), tools.clone(), &config).await {
                Ok(mut stream) => {
                    // Per-attempt accumulators — reset on each retry so we don't
                    // mix partial output from a failed attempt into the next.
                    let mut full_text = String::new();
                    let mut full_thinking = String::new();
                    let mut tool_calls: Vec<ToolCall> = Vec::new();
                    let mut current_tool: Option<(String, String, String)> = None;
                    let mut thinking_started = false;
                    let mut thinking_start_time: Option<std::time::Instant> = None;
                    let mut metadata: Option<ResponseMetadata> = None;

                    while let Some(item) = stream.next().await {
                        match item {
                            Ok(StreamDelta::TextDelta { text }) => {
                                // If thinking just finished, emit thinking-done
                                if thinking_started {
                                    thinking_started = false;
                                    let duration = thinking_start_time
                                        .map(|t| t.elapsed().as_millis() as u64)
                                        .unwrap_or(0);
                                    self.emit_thinking_done(duration);
                                }
                                self.emit_text_delta(&text);
                                full_text.push_str(&text);
                            }
                            Ok(StreamDelta::ThinkingDelta { thinking }) => {
                                if !thinking_started {
                                    thinking_started = true;
                                    thinking_start_time = Some(std::time::Instant::now());
                                }
                                self.emit_thinking(&thinking);
                                full_thinking.push_str(&thinking);
                            }
                            Ok(StreamDelta::ToolCallDelta { id, name, input_json }) => {
                                // If thinking just finished, emit thinking-done
                                if thinking_started {
                                    thinking_started = false;
                                    let duration = thinking_start_time
                                        .map(|t| t.elapsed().as_millis() as u64)
                                        .unwrap_or(0);
                                    self.emit_thinking_done(duration);
                                }
                                if let Some(n) = name {
                                    // Start a new tool call
                                    if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                        if let Ok(args) = serde_json::from_str(&tc_args) {
                                            tool_calls.push(ToolCall {
                                                id: tc_id,
                                                name: tc_name,
                                                arguments: args,
                                            });
                                        }
                                    }
                                    current_tool = Some((id, n, String::new()));
                                }
                                if let Some(args) = input_json {
                                    if let Some((_, _, ref mut tc_args)) = current_tool {
                                        tc_args.push_str(&args);
                                    }
                                }
                            }
                            Ok(StreamDelta::Done { finish_reason, usage }) => {
                                // If thinking was still active, emit thinking-done.
                                // No need to reset thinking_started — loop exits after Done.
                                if thinking_started {
                                    let duration = thinking_start_time
                                        .map(|t| t.elapsed().as_millis() as u64)
                                        .unwrap_or(0);
                                    self.emit_thinking_done(duration);
                                }
                                // Flush any remaining tool call
                                if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                    if let Ok(args) = serde_json::from_str(&tc_args) {
                                        tool_calls.push(ToolCall {
                                            id: tc_id,
                                            name: tc_name,
                                            arguments: args,
                                        });
                                    }
                                }

                                metadata = Some(ResponseMetadata {
                                    model: self.model.clone(),
                                    finish_reason,
                                    usage,
                                });

                                let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };
                                let meta = metadata.unwrap();

                                if !tool_calls.is_empty() {
                                    return Ok(RespondOutput::ToolCalls {
                                        tool_calls,
                                        text: if full_text.is_empty() { None } else { Some(full_text) },
                                        thinking,
                                        metadata: meta,
                                    });
                                } else {
                                    return Ok(RespondOutput::Text { text: full_text, thinking, metadata: meta });
                                }
                            }
                            Err(e) => {
                                // Decide how to recover.
                                let kind = classify_stream_error(&e);
                                match kind {
                                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork
                                        if stream_retries < MAX_STREAM_RETRIES =>
                                    {
                                        tracing::warn!(
                                            error = %e,
                                            kind = ?kind,
                                            attempt = stream_retries + 1,
                                            max = MAX_STREAM_RETRIES,
                                            "Stream interrupted, retrying with a fresh stream"
                                        );
                                        self.emit_stream_reset();
                                        stream_retries += 1;
                                        // Brief backoff before retry
                                        tokio::time::sleep(std::time::Duration::from_millis(
                                            500 * 2u64.pow(stream_retries - 1),
                                        )).await;
                                        continue 'stream_attempt;
                                    }
                                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                        tracing::error!(
                                            error = %e,
                                            retries = stream_retries,
                                            "Stream failed after exhausting retries"
                                        );
                                        self.emit_stream_reset();
                                        return Err(e);
                                    }
                                    StreamErrorKind::Fatal => {
                                        tracing::error!(error = %e, "Stream failed with fatal error");
                                        self.emit_stream_reset();
                                        return Err(e);
                                    }
                                }
                            }
                        }
                    }

                    // Stream ended without a Done delta — build response from accumulated state.
                    let meta = metadata.unwrap_or_else(|| ResponseMetadata {
                        model: self.model.clone(),
                        finish_reason: Some("stream_ended".into()),
                        usage: None,
                    });
                    let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };

                    if !tool_calls.is_empty() {
                        return Ok(RespondOutput::ToolCalls {
                            tool_calls,
                            text: if full_text.is_empty() { None } else { Some(full_text) },
                            thinking,
                            metadata: meta,
                        });
                    } else {
                        return Ok(RespondOutput::Text { text: full_text, thinking, metadata: meta });
                    }
                }
                Err(e) => {
                    // stream() failed before producing any deltas. This is a setup
                    // problem (auth, model-not-found, etc.) — classify and either
                    // retry or fail. Do NOT auto-fallback to complete() — we want
                    // the user to see the real error, not a workaround.
                    let kind = classify_stream_error(&e);
                    match kind {
                        StreamErrorKind::TransientNetwork
                            if stream_retries < MAX_STREAM_RETRIES =>
                        {
                            tracing::warn!(
                                error = %e,
                                attempt = stream_retries + 1,
                                "Stream setup failed transiently, retrying"
                            );
                            stream_retries += 1;
                            tokio::time::sleep(std::time::Duration::from_millis(
                                500 * 2u64.pow(stream_retries - 1),
                            )).await;
                            continue 'stream_attempt;
                        }
                        _ => {
                            tracing::error!(error = %e, "Stream setup failed, surfacing error");
                            return Err(e);
                        }
                    }
                }
            }
        }
    }

    async fn handle_text_response(
        &self,
        text: &str,
        metadata: ResponseMetadata,
        reason_ctx: &mut ReasoningContext,
    ) -> TextAction {
        // If the model ran out of tokens mid-text (finish_reason="length"),
        // do NOT terminate. Treat like a truncation — record what we have
        // and ask the model to continue. Without this, multi-step plans get
        // cut off whenever the response approaches max_tokens (8192) and
        // remaining steps never execute.
        if metadata.finish_reason.as_deref() == Some("length") {
            tracing::warn!(
                text_len = text.len(),
                "Text response hit length limit (finish_reason=length); injecting continuation prompt"
            );
            reason_ctx.messages.push(ChatMessage::assistant(text));
            reason_ctx.messages.push(ChatMessage::user(
                "Your last reply was truncated by the token limit. Continue from where you left off. \
                 If you intended to call a tool next, call it now."
            ));
            return TextAction::Continue;
        }
        self.emit_done(text);
        TextAction::Return(LoopOutcome::Response { text: text.to_string(), usage: metadata.usage })
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<ToolCall>,
        reason_ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, Error> {
        for tc in &tool_calls {
            let tool = self.tools.get(&tc.name);
            match tool {
                Some(tool) => {
                    // Get the tool's own approval requirement
                    let tool_approval = tool.requires_approval(&tc.arguments);

                    tracing::info!(
                        tool = %tc.name,
                        tool_approval = ?tool_approval,
                        session_safety_mode = ?self.safety_mode,
                        "Evaluating tool approval"
                    );

                    // Consult SafetyManager with the session safety mode.
                    // Uses the DB-backed resolver when AppState is available
                    // (the normal case in the running app); falls back to the
                    // in-memory shim if not (keeps any test path that doesn't
                    // wire AppState working).
                    let decision = {
                        use tauri::Manager;
                        let mgr = self.safety_manager.read().await;
                        let db_state = self.app_handle.try_state::<crate::app::AppState>();
                        let session_mode = self.safety_mode.as_ref();
                        // Yolo session override short-circuits without touching DB
                        if matches!(session_mode, Some(SafetyMode::Yolo)) {
                            ApprovalDecision::AutoApprove
                        } else if let Some(state) = db_state {
                            mgr.should_approve_with_db(
                                &state.db,
                                &self.conversation_id,
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
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &format!("Error: Tool blocked — {}", reason),
                                true,
                            ));
                            continue;
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
                                "sessionId": self.conversation_id,
                                "riskLevel": "medium",
                                "timestamp": chrono::Utc::now().to_rfc3339(),
                            }));

                            // Await user's approval decision
                            let approval_result = match rx.await {
                                Ok(result) => result,
                                Err(_) => {
                                    // Channel dropped — treat as rejection
                                    tracing::warn!(tool = %tc.name, "Approval channel dropped, treating as rejected");
                                    crate::app::ApprovalResult { approved: false, always_allow: false, tool_name: None }
                                }
                            };

                            if !approval_result.approved {
                                tracing::info!(tool = %tc.name, "Tool execution rejected by user");
                                reason_ctx.messages.push(ChatMessage::user_tool_result(
                                    &tc.id,
                                    "Error: Tool execution was rejected by the user.",
                                    true,
                                ));
                                // Emit rejection event so frontend knows
                                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                    "toolName": tc.name,
                                    "toolCallId": tc.id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                continue;
                            }

                            // If always_allow was set, add to auto-approved list
                            if approval_result.always_allow {
                                let mut mgr = self.safety_manager.write().await;
                                let _ = mgr.add_auto_approved(&tc.name);
                                tracing::info!(tool = %tc.name, "Tool added to auto-approved list via always_allow");
                            }

                            tracing::info!(tool = %tc.name, "Tool approved by user, proceeding");
                        }
                        ApprovalDecision::AutoApprove => {
                            tracing::debug!(tool = %tc.name, "Tool auto-approved");
                        }
                    }

                    // Emit tool start
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments);
                    tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool");
                    let tool_start = std::time::Instant::now();

                    // Execute tool
                    match tool.execute(tc.arguments.clone()).await {
                        Ok(output) => {
                            let duration_ms = tool_start.elapsed().as_millis() as u64;
                            tracing::info!(
                                tool = %tc.name,
                                duration_ms = duration_ms,
                                "Tool completed"
                            );
                            self.emit_tool_result(&tc.name, &tc.id, &output);

                            let raw_result_str = serde_json::to_string(&output.result).unwrap_or_else(|_| "{}".into());

                            // Apply budget truncation (must happen before trajectory recording
                            // so the stored result matches what the LLM actually sees)
                            let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                            let result_str = if let Some(ref budget) = self.tool_budget {
                                budget.apply(&tc.name, raw_result_str, &self.conversation_id, turn_idx)
                            } else {
                                raw_result_str
                            };

                            // Record trajectory turn (also reflect soft errors so the
                            // agent_turns-based history recovery in get_agent_session_messages
                            // shows the right status when persisted JSON is missing).
                            let trajectory_is_error = detect_soft_tool_error(&output.result);
                            if let Some(ref store) = self.trajectory_store {
                                use crate::harness::trajectory::TurnRecord;
                                let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                                let record = TurnRecord {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    session_id: self.conversation_id.clone(),
                                    turn_index: turn_idx,
                                    role: "tool".into(),
                                    content: None,
                                    tool_name: Some(tc.name.clone()),
                                    tool_args: Some(tool_args_json),
                                    tool_result: Some(result_str.clone()),
                                    reasoning: None,
                                    is_error: trajectory_is_error,
                                    duration_ms,
                                    created_at: chrono::Utc::now().timestamp_millis(),
                                };
                                if let Err(e) = store.record_turn(&record) {
                                    tracing::warn!("Failed to record trajectory turn: {e}");
                                }
                            }

                            // Publish ToolExecuted event to InfraService
                            if let Some(ref infra) = self.infra_service {
                                let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                                let output_summary = truncate_utf8(&result_str, 500);
                                infra.publish_tool_executed(
                                    "local",
                                    &tc.name,
                                    &output_summary,
                                    serde_json::json!({
                                        "tool_name": tc.name,
                                        "success": true,
                                        "duration_ms": duration_ms,
                                        "tool_input": input_summary,
                                    }),
                                ).await;
                            }

                            // Detect soft errors (e.g. bash non-zero exit) so the persisted
                            // ContentBlock::ToolResult carries is_error correctly. Without this,
                            // historical view would show a green check for failed bash commands.
                            let soft_error = detect_soft_tool_error(&output.result);
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &result_str,
                                soft_error,
                            ));
                        }
                        Err(e) => {
                            let duration_ms = tool_start.elapsed().as_millis() as u64;
                            tracing::error!("Tool {} execution failed: {}", tc.name, e);
                            self.emit_error(&e.to_string());

                            let error_result_str = format!("Error: {}", e);

                            // Record error trajectory turn
                            let turn_idx = self.turn_index.fetch_add(1, Ordering::Relaxed);
                            if let Some(ref store) = self.trajectory_store {
                                use crate::harness::trajectory::TurnRecord;
                                let tool_args_json = serde_json::to_string(&tc.arguments).unwrap_or_default();
                                let record = TurnRecord {
                                    id: uuid::Uuid::new_v4().to_string(),
                                    session_id: self.conversation_id.clone(),
                                    turn_index: turn_idx,
                                    role: "tool".into(),
                                    content: None,
                                    tool_name: Some(tc.name.clone()),
                                    tool_args: Some(tool_args_json),
                                    tool_result: Some(error_result_str.clone()),
                                    reasoning: None,
                                    is_error: true,
                                    duration_ms,
                                    created_at: chrono::Utc::now().timestamp_millis(),
                                };
                                if let Err(re) = store.record_turn(&record) {
                                    tracing::warn!("Failed to record error trajectory turn: {re}");
                                }
                            }

                            // Publish ToolExecuted event (failure) to InfraService
                            if let Some(ref infra) = self.infra_service {
                                let input_summary = truncate_utf8(&serde_json::to_string(&tc.arguments).unwrap_or_default(), 500);
                                let error_summary = truncate_utf8(&e.to_string(), 500);
                                infra.publish_tool_executed(
                                    "local",
                                    &tc.name,
                                    &error_summary,
                                    serde_json::json!({
                                        "tool_name": tc.name,
                                        "success": false,
                                        "duration_ms": duration_ms,
                                        "tool_input": input_summary,
                                    }),
                                ).await;
                            }

                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &tc.id,
                                &error_result_str,
                                true,
                            ));
                        }
                    }
                }
                None => {
                    let err = format!("Tool '{}' not found", tc.name);
                    tracing::warn!("{}", err);

                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &tc.id,
                        &format!("Error: {}", err),
                        true,
                    ));
                }
            }
        }

        Ok(None)
    }

    async fn on_usage(&self, usage: &TokenUsage, reason_ctx: &ReasoningContext) {
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cumulative_input = reason_ctx.total_input_tokens,
            cumulative_output = reason_ctx.total_output_tokens,
            model = %self.model,
            "on_usage called"
        );
        self.emit_turn_cost(usage);
        self.emit_context_stats(
            &reason_ctx.messages,
            reason_ctx.total_input_tokens,
            reason_ctx.total_output_tokens,
        );
    }

    async fn on_tool_intent_nudge(&self, text: &str, _ctx: &mut ReasoningContext) {
        self.emit_thinking(&format!("Detected tool intent in: {}", &text[..text.len().min(100)]));
    }
}

/// Tools whose Rust impl returned `Ok(...)` but where the underlying operation
/// reports a logical failure via the result payload — bash uses
/// `{ ok: false, exit_code: 1, output: "..." }`, MCP uses `{ isError: true }`.
/// Mirrors the same heuristic the frontend applies in the streaming listener,
/// so both live and persisted views agree on whether a row is an error.
pub(crate) fn detect_soft_tool_error(result: &serde_json::Value) -> bool {
    matches!(result.get("ok"), Some(serde_json::Value::Bool(false)))
        || matches!(result.get("is_error"), Some(serde_json::Value::Bool(true)))
        || matches!(result.get("isError"), Some(serde_json::Value::Bool(true)))
}

/// Truncate a string to at most `max_chars` characters, ensuring UTF-8 safety.
fn truncate_utf8(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let mut result: String = s.chars().take(max_chars).collect();
        result.push_str("...");
        result
    }
}
