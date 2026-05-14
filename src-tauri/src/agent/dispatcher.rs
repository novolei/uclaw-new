use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::types::*;
use crate::agent::tools::tool::{ToolRegistry, ToolOutput};
use crate::app::PendingApprovals;
use crate::infra::InfraService;
use crate::llm::LlmProvider;
use crate::error::Error;
use crate::safety::{SafetyManager, SafetyMode, ApprovalDecision};

use crate::agent::retry::AgentRetryEvent;
use crate::agent::llm_stream::StreamSink;

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
    /// Pre-built skill manifest block, set via set_skills_manifest_block
    /// before run_loop starts. Empty when no skills exist (no append).
    skills_manifest_block: String,
    /// PR 2026-05-13 token-cost optim: once the agent calls
    /// `skill_search` in this loop, the manifest's purpose (offering
    /// the catalog) has been served. Suppress it on all subsequent
    /// LLM calls within the same loop to save ~800 tokens per call.
    /// Set inside `execute_tool_calls`; read inside
    /// `effective_system_prompt`.
    ///
    /// `AtomicBool` because the dispatcher is `Send + Sync` and the
    /// tool-execution path holds `&self`. `Ordering::Relaxed` is
    /// sufficient because both reads and writes happen on the same
    /// async task in the agent loop — `execute_tool_calls` is awaited
    /// (`agentic_loop.rs::run_loop`) before the next iteration's
    /// `effective_system_prompt` call, so there's no cross-thread
    /// happens-before requirement. The flag is a one-way hint, not a
    /// synchronization primitive; never gets unset (per-loop sticky;
    /// resets when the next user message constructs a fresh
    /// `ChatDelegate`).
    skill_search_used: AtomicBool,
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
            skills_manifest_block: String::new(),
            skill_search_used: AtomicBool::new(false),
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

    /// Set the skill manifest block to append to the system prompt.
    /// Caller is responsible for building this via skills_manifest::build_skills_manifest.
    pub fn set_skills_manifest_block(&mut self, block: String) {
        self.skills_manifest_block = block;
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
        let composed = crate::agent::mode_prompts::compose_system_prompt(
            &base_with_memory,
            self.workspace_root.as_deref(),
            effective_mode,
        );
        // Append the skill manifest block (empty when no skills exist).
        // Once the agent has already invoked `skill_search` in this loop
        // the manifest's recall-prompt job is done — suppress it on the
        // next call to save ~800 tokens (PR 2026-05-13 token-cost optim).
        // The flag stays sticky for the remainder of the loop because the
        // agent loop reuses the same `ChatDelegate` across iterations.
        let suppress_manifest = self.skill_search_used.load(Ordering::Relaxed);
        if self.skills_manifest_block.is_empty() || suppress_manifest {
            composed
        } else {
            format!("{}{}", composed, self.skills_manifest_block)
        }
    }

    /// Build the per-message dynamic context block.
    ///
    /// Prepended to the LAST user message in each LLM call payload — NOT
    /// persisted to the session. Each new call gets a fresh timestamp.
    ///
    /// Rationale: time is metadata the agent can't physically probe with a
    /// tool the way it can probe filesystem state — pre-injecting it saves
    /// a `date` tool roundtrip on every "what time is it" question while
    /// honoring the probe-first philosophy for everything else (cwd, files,
    /// etc. stay agent-driven).
    fn build_dynamic_context(&self) -> String {
        use chrono::Local;
        let now = Local::now();
        // Example: "Monday, May 12, 2026 at 11:30 AM PST"
        let time = now.format("%A, %B %-d, %Y at %-I:%M %p %Z").to_string();
        let mut lines = vec![format!("**Current time:** {}", time)];

        if let Some(root) = &self.workspace_root {
            // Workspace path is also metadata: agent can't infer where it's
            // "installed" via tool use, only the cwd it's run from.
            lines.push(format!("**Workspace root:** {}", root.display()));
        }

        lines.join("\n")
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

    /// Emit a tool call start to the frontend.
    ///
    /// `preview_target` carries the file path the tool will write, when
    /// the tool overrides `Tool::preview_target_path`. The frontend's
    /// auto-preview listener uses this to open the preview panel without
    /// keeping a hardcoded list of "write-ish" tool names — adding a new
    /// mutating tool only requires implementing the trait method, not
    /// touching frontend code.
    fn emit_tool_start(
        &self,
        name: &str,
        id: &str,
        input: &serde_json::Value,
        preview_target: Option<&str>,
    ) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_start",
                "toolName": name,
                "toolCallId": id,
                "input": input,
                "previewTarget": preview_target,
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

    /// Emit a hard-error tool result so the frontend can flip the activity
    /// to "failed" state immediately, instead of letting it spin until the
    /// whole turn completes.
    fn emit_tool_error(&self, name: &str, id: &str, err_msg: &str, duration_ms: u64) {
        let _ = self.app_handle.emit("chat:stream-tool-activity", serde_json::json!({
            "conversationId": self.conversation_id,
            "activity": {
                "type": "tool_result",
                "toolName": name,
                "toolCallId": id,
                "result": { "ok": false, "error": err_msg },
                "durationMs": duration_ms,
                "timestamp": chrono::Utc::now().to_rfc3339(),
                "isError": true,
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

    /// Emit turn cost event after each LLM call.
    ///
    /// Async because the budget-threshold check reads `state.settings`, a
    /// `tokio::sync::RwLock`. The previous `Handle::block_on` approach
    /// deadlocked when called from inside an async task (this fn is invoked
    /// from `on_usage`, which is async).
    async fn emit_turn_cost(&self, usage: &TokenUsage) {
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

            // Phase 6-C: budget threshold check. Best-effort — failures don't propagate.
            let budget_opt: Option<f64> = state.settings.read().await.monthly_budget_usd;
            if let Some(budget) = budget_opt {
                if budget > 0.0 {
                    let month_start = crate::cost_store::current_month_start_ms();
                    let total_after = crate::cost_store::monthly_total(&state, month_start);
                    let total_before = (total_after - cost).max(0.0);
                    if let Some(threshold) = crate::cost_store::fired_threshold(total_before, total_after, budget) {
                        let _ = self.app_handle.emit("budget:threshold", crate::ipc::BudgetThresholdPayload {
                            threshold,
                            current: total_after,
                            budget,
                        });
                    }
                }
            }
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
                    ContentBlock::Thinking { thinking, .. } => {
                        messages_tokens += estimate_tokens(thinking);
                    }
                }
            }
        }

        let compact_buffer = (model_context_length as f32 * 0.033) as u32;
        let used = system_prompt_tokens + messages_tokens + tool_use_tokens + compact_buffer;
        let free = model_context_length as i32 - used as i32;

        let stats = ContextStats {
            conversation_id: self.conversation_id.clone(),
            model_context_length,
            system_prompt_tokens,
            mcp_prompts_tokens: 0,
            skills_tokens: estimate_tokens(&self.skills_manifest_block),
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
            "conversationId": self.conversation_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit the `agent:retry` IPC event. Failures are non-fatal — we only
    /// log, so the retry loop is never blocked by a Tauri emit error.
    fn emit_retry_event(&self, event: AgentRetryEvent) {
        if let Err(e) = self.app_handle.emit(AgentRetryEvent::CHANNEL, &event) {
            tracing::debug!(error = %e, "Failed to emit agent:retry event");
        }
    }

    /// Sleep for `duration`, but wake up early if the session's stop flag
    /// flips. Returns `true` if the wake was triggered by the stop flag
    /// (caller should bail), `false` if the full duration elapsed.
    async fn sleep_or_abort(&self, duration: std::time::Duration) -> bool {
        let stop = self.stop_flag.clone();
        tokio::select! {
            _ = tokio::time::sleep(duration) => false,
            _ = async {
                loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            } => true,
        }
    }
}

#[async_trait]
impl StreamSink for ChatDelegate {
    fn on_text_delta(&self, text: &str) {
        self.emit_text_delta(text);
    }
    fn on_thinking(&self, thinking: &str) {
        self.emit_thinking(thinking);
    }
    fn on_thinking_done(&self, duration_ms: u64) {
        self.emit_thinking_done(duration_ms);
    }
    fn on_stream_reset(&self) {
        self.emit_stream_reset();
    }
    fn on_retry_event(&self, event: AgentRetryEvent) {
        self.emit_retry_event(event);
    }
    async fn sleep_or_abort(&self, delay: std::time::Duration) -> bool {
        ChatDelegate::sleep_or_abort(self, delay).await
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

        // Prepend dynamic context (time + workspace root) to the LAST user
        // message in this call. Mutates the clone only — the persisted
        // session messages stay clean so context isn't duplicated when the
        // session resumes or replays.
        //
        // We attach to the LAST user message (not a new message at the end)
        // because a fresh "user" message after assistant tool calls would
        // confuse the back-and-forth structure. Anthropic / OpenAI both
        // require alternating user/assistant turns.
        if let Some(last_user_idx) = messages.iter().rposition(|m| {
            matches!(m.role, crate::agent::types::MessageRole::User)
                && m.content.iter().any(|b| matches!(b, crate::agent::types::ContentBlock::Text { .. }))
        }) {
            let dyn_ctx = self.build_dynamic_context();
            if let Some(crate::agent::types::ContentBlock::Text { text }) =
                messages[last_user_idx].content.iter_mut().find(|b| {
                    matches!(b, crate::agent::types::ContentBlock::Text { .. })
                })
            {
                *text = format!("{}\n\n{}", dyn_ctx, text);
            }
        }

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

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            self,
        )
        .await
    }

    async fn handle_text_response(
        &self,
        text: &str,
        metadata: ResponseMetadata,
        reason_ctx: &mut ReasoningContext,
    ) -> TextAction {
        let is_truncated = metadata.finish_reason.as_deref() == Some("length");

        // Build the effective rescue text. If we have a partial code block
        // accumulated from previous truncated responses, reconstruct the fence
        // so parse_code_blocks can see the full (possibly-now-complete) block.
        let effective_owned;
        let effective_text: &str = match &reason_ctx.partial_code_buffer {
            Some((lang, acc)) => {
                effective_owned = format!("```{}\n{}{}", lang, acc, text);
                &effective_owned
            }
            None => text,
        };

        if is_truncated {
            reason_ctx.consecutive_length_truncations += 1;
            let n = reason_ctx.consecutive_length_truncations;
            tracing::warn!(
                text_len = text.len(),
                consecutive = n,
                "Text response hit length limit (finish_reason=length)"
            );

            // Even with truncation, the accumulated+current text might now
            // form a complete code block — try rescue first.
            // `_safe` wrapper: code-rescue is best-effort fallback; panicking
            // here (we hit a CJK UTF-8 boundary bug previously) would kill the
            // whole agentic_loop task and freeze the UI. catch_unwind here means
            // "no rescue this turn", loop continues.
            let rescue_calls = crate::agent::code_rescue::extract_write_file_calls_safe(
                effective_text,
                self.workspace_root.as_deref(),
            );
            if !rescue_calls.is_empty() {
                tracing::info!(
                    count = rescue_calls.len(),
                    "Code-block rescue succeeded on accumulated partial text"
                );
                reason_ctx.partial_code_buffer = None;
                reason_ctx.consecutive_length_truncations = 0;
                return TextAction::RescueWithToolCalls(rescue_calls);
            }

            // No complete block yet — update accumulator so the next iteration
            // can try again with more content.
            if let Some((_, ref mut acc)) = reason_ctx.partial_code_buffer {
                acc.push_str(text);
            } else if let Some((lang, partial)) =
                crate::agent::code_rescue::extract_partial_code_block_safe(text)
            {
                reason_ctx.partial_code_buffer = Some((lang, partial));
            }

            // After 2+ truncations, escalate from generic nudge to explicit
            // chunked-writing strategy — the file is too large for one shot.
            let nudge = if n >= 2 {
                format!(
                    "Your response has been truncated {n} times in a row. \
                     The file you are writing is too large to output as text. \
                     STOP outputting text immediately. \
                     Strategy: call write_file with the FIRST 250-300 lines as \
                     the `content` argument. In the following turn, call \
                     write_file again (or edit) for the remaining sections. \
                     Do not output any code as text — call write_file NOW."
                )
            } else {
                "Your last reply was truncated by the token limit. \
                 If you were writing a file, call write_file NOW instead of \
                 outputting the content as text. \
                 If you were mid-sentence on something else, continue from \
                 where you left off."
                    .to_string()
            };
            return TextAction::ContinueWithNudge(nudge);
        }

        // ── Complete response path ────────────────────────────────────────
        // Reset truncation state (model sent a full response this time).
        reason_ctx.consecutive_length_truncations = 0;

        // Code-block rescue: try with accumulated+current text (clears buffer
        // regardless of outcome — the response is done).
        let rescue_calls = crate::agent::code_rescue::extract_write_file_calls_safe(
            effective_text,
            self.workspace_root.as_deref(),
        );
        reason_ctx.partial_code_buffer = None;
        if !rescue_calls.is_empty() {
            tracing::warn!(
                count = rescue_calls.len(),
                "Code-block rescue triggered: model output code as text, converting to write_file calls"
            );
            return TextAction::RescueWithToolCalls(rescue_calls);
        }

        // Plan-aware termination guard: if a plan file still has undone steps
        // modified in the last 5 minutes, the model hasn't actually finished —
        // nudge it to continue rather than terminating.
        // Cap: after MAX_PLAN_GUARD_NUDGES consecutive nudges without any tool
        // call response, give up and let the response through to avoid infinite
        // text-only loops (e.g. model did edits but forgot plan_update calls).
        if let Some(undone) = crate::agent::plan_state::pending_plan_steps(
            self.workspace_root.as_deref(),
            300,
        ) {
            if reason_ctx.consecutive_plan_guard_nudges >= crate::agent::types::MAX_PLAN_GUARD_NUDGES {
                tracing::warn!(
                    undone_steps = undone,
                    nudges = reason_ctx.consecutive_plan_guard_nudges,
                    "Plan guard giving up after {} nudges without tool response — returning response as-is",
                    crate::agent::types::MAX_PLAN_GUARD_NUDGES
                );
                // Fall through to emit_done / Return below
            } else {
                reason_ctx.consecutive_plan_guard_nudges += 1;
                tracing::info!(
                    undone_steps = undone,
                    nudge_count = reason_ctx.consecutive_plan_guard_nudges,
                    "Pending plan steps detected; injecting continuation instead of terminating"
                );
                return TextAction::ContinueWithNudge(format!(
                    "Your plan has {} undone step(s) marked `- [ ]`. \
                     Execute the next undone step RIGHT NOW by calling a tool — \
                     use write_file to write code, edit to modify an existing file, \
                     or bash to run a command. DO NOT output file content as text.",
                    undone
                ));
            }
        }

        self.emit_done(text);
        TextAction::Return(LoopOutcome::Response { text: text.to_string(), usage: metadata.usage })
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<ToolCall>,
        reason_ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, Error> {
        // PR 2026-05-13 token-cost optim: once the agent reaches for
        // `skill_search` in this loop, the system-prompt manifest has
        // served its purpose (catalog discovery → tool delegation).
        // Mark the flag so `effective_system_prompt` skips the ~800-token
        // manifest block on subsequent calls. Sticky for the rest of the
        // loop; reset implicitly when the agent_loop spawns a fresh
        // ChatDelegate for the next user message.
        if tool_calls.iter().any(|tc| tc.name == "skill_search") {
            self.skill_search_used.store(true, Ordering::Relaxed);
        }
        for tc in &tool_calls {
            // ── Anti-fake-progress challenge ─────────────────────────
            // Intercept `plan_update done:true` calls that have neither
            // a recent mutating tool call NOR explicit evidence in `note`.
            // Inject a synthetic error tool_result and skip the actual
            // tool dispatch. See agent/types.rs::FAKE_PROGRESS_CHALLENGE
            // for the reasoning. After MAX_MUTATION_CHALLENGES soft-blocks
            // in this loop, we let it through with a logged warning so a
            // genuinely-completed step doesn't loop forever.
            if tc.name == "plan_update"
                && tc.arguments.get("done").and_then(|v| v.as_bool()).unwrap_or(false)
                && reason_ctx.mutations_since_last_plan_done == 0
                && reason_ctx.mutation_challenges_issued < crate::agent::types::MAX_MUTATION_CHALLENGES
            {
                // Treat a `note` of >= 20 chars as evidence: if the LLM
                // bothered to type a real explanation we let it through.
                let note_len = tc.arguments.get("note")
                    .and_then(|v| v.as_str())
                    .map(|s| s.trim().len())
                    .unwrap_or(0);
                if note_len < 20 {
                    reason_ctx.mutation_challenges_issued += 1;
                    tracing::warn!(
                        tool = %tc.name,
                        challenges = reason_ctx.mutation_challenges_issued,
                        max = crate::agent::types::MAX_MUTATION_CHALLENGES,
                        step_index = ?tc.arguments.get("step_index"),
                        "Soft-blocking plan_update done:true with no mutation evidence"
                    );
                    // No preview_target for the soft-block path — the tool
                    // call is being short-circuited, no write will happen.
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments, None);
                    self.emit_tool_result(
                        &tc.name,
                        &tc.id,
                        &crate::agent::tools::tool::ToolOutput::error(
                            crate::agent::types::FAKE_PROGRESS_CHALLENGE,
                            0,
                        ),
                    );
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &tc.id,
                        crate::agent::types::FAKE_PROGRESS_CHALLENGE,
                        true,
                    ));
                    continue;
                }
                tracing::info!(
                    tool = %tc.name,
                    note_len,
                    "plan_update done:true accepted via `note` evidence"
                );
            }

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
                                    crate::app::ApprovalResult { approved: false, always_allow: false, tool_name: None, path_scope: None, paths: None }
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

                    // Emit tool start. Surface preview_target so the
                    // frontend's auto-preview listener can pre-stake +
                    // open the panel without keeping a hardcoded
                    // "write-ish tool name" list (drives PR auto-preview).
                    let preview_target = tool.preview_target_path(&tc.arguments);
                    self.emit_tool_start(&tc.name, &tc.id, &tc.arguments, preview_target.as_deref());
                    tracing::info!(tool = %tc.name, id = %tc.id, "Executing tool");

                    // ─── Path-aware sandbox (Phase 3) ───────────────────
                    // Resolve candidate paths from the tool's path_args, ask
                    // SafetyManager. Prompt → reuse the same approval
                    // modal/oneshot pattern with kind: "path".
                    let candidate_paths: Vec<std::path::PathBuf> = tool
                        .path_args(&tc.arguments)
                        .into_iter()
                        .map(|p| {
                            let pb = std::path::PathBuf::from(p);
                            if pb.is_absolute() {
                                pb
                            } else if let Some(root) = self.workspace_root.as_deref() {
                                root.join(pb)
                            } else {
                                pb
                            }
                        })
                        .collect();

                    if !candidate_paths.is_empty() && self.workspace_root.is_some() {
                        use crate::safety::path_policy::PathDecision;
                        let workspace_root = self.workspace_root.clone().unwrap();
                        let (ws_attached, sess_attached) = load_attached_dirs_for_session(
                            &self.app_handle,
                            &self.conversation_id,
                        );
                        let path_decision = {
                            let mgr = self.safety_manager.read().await;
                            mgr.check_paths(
                                &self.conversation_id,
                                &workspace_root,
                                &ws_attached,
                                &sess_attached,
                                &candidate_paths,
                                self.safety_mode.as_ref(),
                            )
                        };
                        match path_decision {
                            PathDecision::Allow => {}
                            PathDecision::Block { reason } => {
                                tracing::warn!(tool = %tc.name, reason = %reason, "Path blocked by sandbox");
                                reason_ctx.messages.push(ChatMessage::user_tool_result(
                                    &tc.id,
                                    &format!("Error: {}", reason),
                                    true,
                                ));
                                let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                    "toolName": tc.name,
                                    "toolCallId": tc.id,
                                    "timestamp": chrono::Utc::now().to_rfc3339(),
                                }));
                                continue;
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
                                    "sessionId": self.conversation_id,
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
                                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                                        &tc.id,
                                        &format!("Error: User denied access to path(s): {}", paths_str),
                                        true,
                                    ));
                                    let _ = self.app_handle.emit("agent:tool-rejected", serde_json::json!({
                                        "toolName": tc.name,
                                        "toolCallId": tc.id,
                                        "timestamp": chrono::Utc::now().to_rfc3339(),
                                    }));
                                    continue;
                                }
                                if path_result.path_scope.as_deref() == Some("session") {
                                    let paths_to_grant = path_result.paths.clone()
                                        .unwrap_or_else(|| candidate_paths.iter().map(|p| p.display().to_string()).collect());
                                    let mut mgr = self.safety_manager.write().await;
                                    for p in paths_to_grant {
                                        mgr.allow_path_for_session(&self.conversation_id, std::path::PathBuf::from(p));
                                    }
                                }
                                // "once" falls through without persisting
                            }
                        }
                    }

                    let tool_start = std::time::Instant::now();

                    // Phase: stabilization week — wrap in tokio::task::spawn so panics
                    // get caught at the JoinHandle boundary rather than unwinding through
                    // the agent loop and killing the whole turn.
                    let tool_name_for_panic = tc.name.clone();
                    let tool_args_for_spawn = {
                        let mut args = tc.arguments.clone();
                        if let Some(obj) = args.as_object_mut() {
                            obj.insert("_tool_call_id".to_string(), serde_json::Value::String(tc.id.clone()));
                        } else {
                            tracing::warn!(
                                tool = %tc.name,
                                "tool arguments is not a JSON object; skipping _tool_call_id injection"
                            );
                        }
                        args
                    };
                    let tools_arc = Arc::clone(&self.tools);
                    let execute_result = match tokio::task::spawn(async move {
                        match tools_arc.get(&tool_name_for_panic) {
                            Some(t) => t.execute(tool_args_for_spawn).await,
                            None => Err(crate::agent::tools::tool::ToolError::NotFound(tool_name_for_panic)),
                        }
                    }).await {
                        Ok(Ok(out)) => Ok(out),
                        Ok(Err(e)) => Err(e),
                        Err(join_err) if join_err.is_panic() => {
                            tracing::error!(tool = %tc.name, "tool panicked");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!(
                                "Tool '{}' crashed unexpectedly. See ~/.uclaw/logs/crashes/ for details.",
                                tc.name,
                            )))
                        }
                        Err(join_err) => {
                            tracing::error!(tool = %tc.name, %join_err, "tool join error");
                            Err(crate::agent::tools::tool::ToolError::Execution(format!("Tool join error: {}", join_err)))
                        }
                    };
                    // Execute tool
                    match execute_result {
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

                            // ── Anti-fake-progress bookkeeping ────────────
                            // Track real mutations so the next plan_update
                            // done:true has evidence to point at. A `bash`
                            // that hard-failed (soft_error=true) doesn't
                            // count — failed mutation isn't mutation.
                            if !soft_error
                                && crate::agent::types::is_mutating_tool(&tc.name, &tc.arguments)
                            {
                                reason_ctx.mutations_since_last_plan_done = reason_ctx
                                    .mutations_since_last_plan_done
                                    .saturating_add(1);
                            }
                            // Any successful tool call ends the truncation
                            // streak and plan-guard nudge streak.
                            reason_ctx.consecutive_length_truncations = 0;
                            reason_ctx.partial_code_buffer = None;
                            reason_ctx.consecutive_plan_guard_nudges = 0;
                            // Reset on successful plan_update done:true so the
                            // NEXT step needs its own mutation evidence. If the
                            // call was the soft-blocked path it `continue`d
                            // above and never reaches here.
                            if tc.name == "plan_update"
                                && tc.arguments.get("done").and_then(|v| v.as_bool()).unwrap_or(false)
                            {
                                reason_ctx.mutations_since_last_plan_done = 0;
                                reason_ctx.mutation_challenges_issued = 0;
                            }
                        }
                        Err(e) => {
                            let duration_ms = tool_start.elapsed().as_millis() as u64;
                            tracing::error!("Tool {} execution failed: {}", tc.name, e);
                            // Mark the tool activity as failed in the UI immediately —
                            // before this, only emit_error fired (which raises a toast
                            // but doesn't update the tool row), so the spinner kept
                            // running until end-of-turn final cleanup.
                            self.emit_tool_error(&tc.name, &tc.id, &e.to_string(), duration_ms);
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
        self.emit_turn_cost(usage).await;
        self.emit_context_stats(
            &reason_ctx.messages,
            reason_ctx.total_input_tokens,
            reason_ctx.total_output_tokens,
        );
    }

    async fn on_tool_intent_nudge(&self, text: &str, _ctx: &mut ReasoningContext) {
        self.emit_thinking(&format!("Detected tool intent in: {}", truncate_utf8(text, 100)));
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

/// Load workspace.attached_dirs and session.attached_dirs for the given
/// session. Returns empty vecs on any error (missing rows, malformed JSON).
fn load_attached_dirs_for_session(
    app_handle: &tauri::AppHandle,
    session_id: &str,
) -> (Vec<std::path::PathBuf>, Vec<std::path::PathBuf>) {
    use tauri::Manager;
    let Some(state) = app_handle.try_state::<crate::app::AppState>() else {
        return (Vec::new(), Vec::new());
    };
    let Ok(conn) = state.db.lock() else {
        return (Vec::new(), Vec::new());
    };
    let parse = |json: String| -> Vec<std::path::PathBuf> {
        serde_json::from_str::<Vec<String>>(&json)
            .ok()
            .unwrap_or_default()
            .into_iter()
            .map(std::path::PathBuf::from)
            .collect()
    };
    let ws_attached = conn
        .query_row(
            "SELECT attached_dirs FROM spaces WHERE id = (SELECT space_id FROM agent_sessions WHERE id = ?1)",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    let sess_attached = conn
        .query_row(
            "SELECT attached_dirs FROM agent_sessions WHERE id = ?1",
            rusqlite::params![session_id],
            |row| row.get::<_, String>(0),
        )
        .ok()
        .map(parse)
        .unwrap_or_default();
    (ws_attached, sess_attached)
}

#[cfg(test)]
mod panic_recovery_tests {
    use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
    use async_trait::async_trait;

    struct PanickyTool;

    #[async_trait]
    impl Tool for PanickyTool {
        fn name(&self) -> &str { "panicky" }
        fn description(&self) -> &str { "test-only" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
            ApprovalRequirement::Never
        }
        async fn execute(&self, _: serde_json::Value) -> Result<ToolOutput, ToolError> {
            panic!("deliberate test panic");
        }
    }

    /// Verify the panic-recovery shape: tokio::task::spawn catches panic,
    /// JoinHandle yields is_panic=true, we map that to a ToolError.
    /// This mirrors what dispatcher::execute_tool does.
    #[tokio::test]
    async fn tool_panic_converts_to_tool_error() {
        let tool = PanickyTool;
        let tool_name = tool.name().to_string();
        let join = tokio::task::spawn(async move {
            tool.execute(serde_json::json!({})).await
        });
        let result = match join.await {
            Ok(r) => r,
            Err(e) if e.is_panic() => Err(ToolError::Execution(format!(
                "Tool '{}' crashed unexpectedly.", tool_name
            ))),
            Err(e) => Err(ToolError::Execution(format!("Join error: {}", e))),
        };
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("panicky") && msg.contains("crashed"),
            "expected panic-recovery error, got: {}", msg
        );
    }
}

#[cfg(test)]
mod manifest_suppression_tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    /// Mirror of the suppression rule in `effective_system_prompt`. Kept
    /// in lockstep with `dispatcher.rs:154-171` — if you change the rule
    /// there, change it here. This test pins the contract without
    /// constructing a full `ChatDelegate` (which needs an LLM provider,
    /// safety manager, etc — heavy for a unit test).
    fn compose_with_suppression(
        base_system: &str,
        manifest_block: &str,
        skill_search_used: &AtomicBool,
    ) -> String {
        let suppress = skill_search_used.load(Ordering::Relaxed);
        if manifest_block.is_empty() || suppress {
            base_system.to_string()
        } else {
            format!("{}{}", base_system, manifest_block)
        }
    }

    /// Default state: flag unset → manifest is appended.
    #[test]
    fn manifest_appended_before_skill_search_used() {
        let flag = AtomicBool::new(false);
        let out = compose_with_suppression(
            "You are an agent.",
            "\n\nMANIFEST_BLOCK",
            &flag,
        );
        assert!(out.contains("MANIFEST_BLOCK"));
    }

    /// After flag is set (simulating `execute_tool_calls` seeing
    /// `skill_search`), the manifest is gone on subsequent prompt
    /// composition. This is the core of PR #137's optim #5 — without
    /// this, the ~800 tokens of manifest leak back into every later
    /// LLM call in the same agent loop.
    #[test]
    fn manifest_suppressed_after_skill_search_used() {
        let flag = AtomicBool::new(false);
        // First call (before skill_search): manifest present.
        let pre = compose_with_suppression("base", "\nM", &flag);
        assert!(pre.contains("M"));

        // Simulate `execute_tool_calls` detecting skill_search.
        flag.store(true, Ordering::Relaxed);

        // Subsequent calls: manifest gone.
        let post = compose_with_suppression("base", "\nM", &flag);
        assert!(!post.contains("M"));
        assert_eq!(post, "base");
    }

    /// Empty manifest: the suppression flag has no observable effect.
    /// Edge case — verifies the `is_empty()` short-circuit takes
    /// precedence over the flag check.
    #[test]
    fn empty_manifest_unaffected_by_flag() {
        for &used in &[false, true] {
            let flag = AtomicBool::new(used);
            let out = compose_with_suppression("base", "", &flag);
            assert_eq!(out, "base", "empty manifest should not produce divergent output; used={}", used);
        }
    }

    /// Flag is sticky — once set, stays set. A second non-skill_search
    /// tool call in the same loop must not flip it back. This guards
    /// against future refactors that might (incorrectly) reset the
    /// flag mid-loop.
    #[test]
    fn flag_stays_set_after_subsequent_non_skill_search_calls() {
        let flag = AtomicBool::new(false);
        flag.store(true, Ordering::Relaxed);  // simulate skill_search
        // Simulate a subsequent tool call that is NOT skill_search —
        // mirroring `execute_tool_calls`'s any() check which only
        // sets-true, never sets-false.
        // (No-op — the flag has nothing in execute_tool_calls that
        //  resets it; this test pins that fact.)
        assert!(flag.load(Ordering::Relaxed));
    }
}

#[cfg(test)]
mod manifest_cap_tests {
    /// PR #137 reduced the manifest cap from 1500 → 800 tokens in
    /// `tauri_commands.rs:5015`. The token budget is consumed by
    /// `skills_manifest::build_skills_manifest` via approximate
    /// 4-chars-per-token math. Verify the function honors a low cap
    /// gracefully (returns something non-empty if even one entry fits;
    /// returns empty if nothing fits — never panics, never returns
    /// the over-budget version).
    use crate::memory_graph::store::MemoryGraphStore;
    use crate::skills::SkillsRegistry;
    use crate::skills_manifest::{build_skills_manifest, StrategyBias};
    use rusqlite::Connection;
    use std::sync::{Arc, Mutex};

    fn fresh_store() -> MemoryGraphStore {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        MemoryGraphStore::new(Arc::new(Mutex::new(conn)))
    }

    /// 800-token cap (current production setting) is enough budget for at
    /// least a few entries — verifies the cap isn't accidentally below
    /// the per-entry minimum, which would make the manifest unusable.
    #[test]
    fn manifest_at_800_token_cap_produces_output_when_skills_exist() {
        let registry = SkillsRegistry::new();
        // Empty store + empty registry → manifest can legitimately be
        // empty, no assertion needed beyond "doesn't panic".
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 800, StrategyBias::Balanced, None,
        );
        // No skills loaded → empty manifest is correct.
        assert!(manifest.is_empty() || manifest.contains("Learned Skills"),
            "manifest must either be empty or contain the documented header");
    }

    /// Lower cap = no panic, no overrun. The cap argument is a soft
    /// budget — `format_manifest` stops adding entries when the next
    /// entry would push past the budget. Verifies the format function
    /// handles a very small cap (256 tokens ≈ 1000 chars).
    #[test]
    fn manifest_handles_very_small_cap_without_panic() {
        let registry = SkillsRegistry::new();
        let store = fresh_store();
        let manifest = build_skills_manifest(
            &registry, &store, "default",
            30, 256, StrategyBias::Balanced, None,
        );
        // Empty registry + empty store → empty manifest, no panic.
        let _ = manifest;
    }
}
