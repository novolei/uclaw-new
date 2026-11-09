//! Event emission + telemetry recording for ChatDelegate.
//!
//! Every method here fires a `tauri::AppHandle::emit` call, pushes a
//! telemetry snapshot, or pulses the heartbeat. None of them touch the
//! LLM, the tool registry, or the loop body. Pure I/O fan-out.

use std::sync::atomic::Ordering;

use super::ChatDelegate;
use crate::agent::retry::AgentRetryEvent;
use crate::agent::tools::tool::ToolOutput;
use crate::agent::types::{
    calculate_cost, format_cost, get_model_context_length, estimate_tokens,
    ChatMessage, ContentBlock, ContextStats, ReflectionDetail, TurnCostInfo, TokenUsage,
};
use tauri::Emitter;

impl ChatDelegate {
    /// C2-Dirac-B2 — wire the M2-J `ComposeStatsCollector`. When set,
    /// every `effective_system_prompt` call records the latest
    /// `ComposeStats` keyed on `conversation_id`. UI reads via the
    /// `get_compose_stats` Tauri command. None = telemetry off (headless
    /// / test contexts).
    pub fn set_compose_stats_collector(
        &mut self,
        collector: crate::agent::context_manager::ComposeStatsCollector,
    ) {
        self.telemetry.compose_stats = Some(collector);
    }

    /// Bundle 27-A — install a heartbeat supervisor. Builder pattern:
    /// caller constructs `HeartbeatSupervisor::new(...)` and hands the
    /// `Arc` to the dispatcher, which keeps a clone alongside the
    /// caller's clone. Both are dropped at the same time (end of agent
    /// loop), which tears down the ticker via Drop.
    pub fn set_heartbeat(
        &mut self,
        hb: std::sync::Arc<crate::agent::heartbeat::HeartbeatSupervisor>,
    ) {
        self.telemetry.heartbeat = Some(hb);
    }

    /// Bundle 27-A — tiny helper so the (many) callsites can do
    /// `self.beat(stage)` without unwrap/Option dance.
    pub(super) fn beat(&self, stage: &str) {
        if let Some(ref hb) = self.telemetry.heartbeat {
            hb.mark_activity(stage);
        }
    }

    /// Slice 1 — wire the M2-J TokenBudgetCollector. When set, every
    /// `on_usage` tick records a fresh snapshot keyed on
    /// `conversation_id` so the UI can subscribe via
    /// `get_latest_token_budget`. Pass an `AppState::token_budget_collector`
    /// clone from the Tauri command that builds the delegate.
    pub fn set_token_budget_collector(
        &mut self,
        collector: crate::agent::telemetry::TokenBudgetCollector,
    ) {
        self.telemetry.token_budget = Some(collector);
    }

    /// Emit a text delta to the frontend
    pub(super) fn emit_text_delta(&self, chunk: &str) {
        let seq = self.chunk_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.app_handle.emit("chat:stream-chunk", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": chunk,
            "seq": seq,
        }));
        // Bundle 27-A — feed the partial buffer + beat. The buffer is
        // what gets persisted as "[interrupted-recovered]" assistant
        // text if the process dies mid-stream. Done as fire-and-forget
        // task so we don't block the streaming hot path on a mutex
        // we don't strictly need to await here.
        if let Some(ref hb) = self.telemetry.heartbeat {
            let hb = hb.clone();
            let chunk = chunk.to_string();
            tokio::spawn(async move {
                hb.append_partial(&chunk).await;
                hb.mark_activity(crate::agent::heartbeat::stages::LLM_STREAM);
            });
        }
    }

    /// Emit a tool call start to the frontend.
    ///
    /// `preview_target` carries the file path the tool will write, when
    /// the tool overrides `Tool::preview_target_path`. The frontend's
    /// auto-preview listener uses this to open the preview panel without
    /// keeping a hardcoded list of "write-ish" tool names — adding a new
    /// mutating tool only requires implementing the trait method, not
    /// touching frontend code.
    pub(super) fn emit_tool_start(
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
        // Bundle 27-A — tool boundaries are the typical place where
        // the agent hangs (browser navigate, long bash, MCP roundtrip).
        // Mark activity at start AND completion so heartbeat reflects
        // the actual stage.
        self.beat(&format!(
            "{}:{}",
            crate::agent::heartbeat::stages::TOOL_CALL, name
        ));
    }

    /// Emit a tool result to the frontend
    pub(super) fn emit_tool_result(&self, name: &str, id: &str, output: &ToolOutput) {
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
                "isError": super::detect_soft_tool_error(&output.result),
            }
        }));
    }

    /// Emit a completion event to the frontend
    pub(super) fn emit_done(&self, text: &str, truncated: bool) {
        let _ = self.app_handle.emit("chat:stream-complete", serde_json::json!({
            "conversationId": self.conversation_id,
            "text": text,
            "truncated": truncated,
        }));
        // Bundle 27-A fix (2026-05-22) — DO NOT call `hb.shutdown()` here.
        // Earlier draft removed the flight file at emit_done, which meant
        // the kill-recovery window was approximately zero: by the time the
        // user reacted to "Agent's streaming, let me kill -9", emit_done
        // had often already run.
        //
        // New behavior:
        // - emit_done only marks the DONE stage so the heartbeat indicator
        //   stops pulsing in the UI (heartbeat banner clears via the
        //   `chat:stream-complete` listener anyway).
        // - The supervisor's `Drop` impl (fired when `_hb_arc` goes out of
        //   scope at the END of `send_agent_message`, AFTER post-loop
        //   persistence) is the one place that removes the flight file.
        //   This makes the recovery window cover the full
        //   send_agent_message lifetime — from agent start to function
        //   return — not just the streaming phase.
        // - The partial buffer is no longer drained here either; Drop
        //   handles cleanup. A residual partial in memory at Drop time
        //   is harmless (the Arc owns the buffer).
        if let Some(ref hb) = self.telemetry.heartbeat {
            hb.mark_activity(crate::agent::heartbeat::stages::DONE);
        }
    }

    /// Tell the frontend a queued banner card (by uuid) has been consumed by the
    /// agent loop, so it can remove the card. Reuses the 引导 banner UI.
    pub(super) fn emit_queued_consumed(&self, uuid: &str) {
        let _ = self.app_handle.emit(
            "agent:queued-consumed",
            serde_json::json!({ "sessionId": self.conversation_id, "uuid": uuid }),
        );
    }

    /// Emit thinking block to the frontend
    pub(super) fn emit_thinking(&self, text: &str) {
        let seq = self.thinking_seq.fetch_add(1, Ordering::Relaxed);
        let _ = self.app_handle.emit("chat:stream-reasoning", serde_json::json!({
            "conversationId": self.conversation_id,
            "delta": text,
            "seq": seq,
        }));
    }

    /// Emit thinking-done event to the frontend
    pub(super) fn emit_thinking_done(&self, _duration_ms: u64) {
        // Absorbed into the reasoning stream; no separate frontend event needed
    }

    /// Emit turn cost event after each LLM call.
    ///
    /// `model` is taken from the frozen `TurnSnapshot` rather than `self.model`
    /// so cost records reflect the model that actually ran the turn — not the
    /// session-level default (which may differ once hot model swap lands).
    ///
    /// Async because the budget-threshold check reads `state.settings`, a
    /// `tokio::sync::RwLock`. The previous `Handle::block_on` approach
    /// deadlocked when called from inside an async task (this fn is invoked
    /// from `on_usage`, which is async).
    pub(super) async fn emit_turn_cost(&self, usage: &TokenUsage, model: &str) {
        let cost = calculate_cost(model, usage.input_tokens, usage.output_tokens);
        // Cache semantics come from the wire protocol (resolve_api picked the
        // provider), not the model name: Anthropic Messages input excludes
        // cache reads even for non-Claude models (kimi-coding, minimax);
        // claude-* behind an OpenAI-compatible proxy includes them.
        let input_includes_cache = self.llm.usage_input_includes_cache();
        let turn_cost = TurnCostInfo {
            input_tokens: usage.input_tokens,
            output_tokens: usage.output_tokens,
            cache_read_tokens: usage.cache_read_tokens,
            cache_creation_tokens: usage.cache_creation_tokens,
            cost_usd: format_cost(cost),
        };
        tracing::info!(
            input_tokens = usage.input_tokens,
            output_tokens = usage.output_tokens,
            cache_read_tokens = usage.cache_read_tokens,
            cache_creation_tokens = usage.cache_creation_tokens,
            cost_usd = %turn_cost.cost_usd,
            "Emitting agent:turn_cost"
        );

        // Persist BEFORE emitting so the dashboard never undercounts even if
        // the frontend listener races. Best-effort — failures don't propagate.
        use tauri::Manager;
        if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
            crate::cost_store::record_usage(
                &state,
                &self.conversation_id,
                model,
                usage.input_tokens,
                usage.output_tokens,
                usage.cache_read_tokens,
                usage.cache_creation_tokens,
                input_includes_cache,
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
            "cacheReadTokens": turn_cost.cache_read_tokens,
            "cacheCreationTokens": turn_cost.cache_creation_tokens,
            "costUsd": turn_cost.cost_usd,
            // Wire cache semantics (true = input includes cache reads,
            // false = Anthropic Messages: input excludes reads and writes,
            // null = unknown) so the context meter can total the prompt.
            "inputIncludesCache": input_includes_cache,
            "cacheHitRatio": crate::cost_store::cache_hit_ratio(
                input_includes_cache,
                model,
                usage.input_tokens as u64,
                usage.cache_read_tokens as u64,
                usage.cache_creation_tokens as u64,
            ),
        }));
    }

    /// Emit context stats after each LLM call.
    ///
    /// `model` is taken from the frozen `TurnSnapshot` so context-length
    /// lookups use the model that actually ran the turn (Tier 1.3).
    ///
    /// `tools` are the definitions sent with the call (from the snapshot).
    /// Transcript mode reports the frozen frame (prompt, tool surface,
    /// rebuild reason, this turn's `<turn_context>` report).
    pub(super) fn emit_context_stats(
        &self,
        messages: &[ChatMessage],
        cumulative_input: u32,
        cumulative_output: u32,
        model: &str,
        tools: &[crate::agent::types::ToolDefinition],
    ) {
        let report = self.last_turn_context_report.lock().ok().and_then(|r| r.clone());
        let frame = self
            .transcript_sink
            .as_ref()
            .and_then(|_| self.frame.get())
            .map(|f| FrameStatsInput {
                system_prompt: &f.system_prompt,
                tool_def_tokens: tool_def_tokens(tools),
                tool_count: tools.len() as u32,
                rebuild_reason: f.rebuild_reason,
                created_at: f.created_at,
                report: report.as_ref(),
            });
        let stats = build_context_stats(
            &self.conversation_id,
            &self.system_prompt,
            &self.prompt_blocks.skills_manifest,
            messages,
            cumulative_input,
            cumulative_output,
            model,
            self.compaction_threshold_tokens,
            frame.as_ref(),
        );
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
    pub(super) fn emit_stream_reset(&self) {
        tracing::debug!("Emitting stream reset to frontend");
        let _ = self.app_handle.emit("agent:stream-reset", serde_json::json!({
            "conversationId": self.conversation_id,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }

    /// Emit the `agent:retry` IPC event. Failures are non-fatal — we only
    /// log, so the retry loop is never blocked by a Tauri emit error.
    pub(super) fn emit_retry_event(&self, event: AgentRetryEvent) {
        if let Err(e) = self.app_handle.emit(AgentRetryEvent::CHANNEL, &event) {
            tracing::debug!(error = %e, "Failed to emit agent:retry event");
        }
    }
}

/// Context continuity P3 — the frozen-frame facts surfaced in
/// `agent:context_stats` (transcript mode only).
#[derive(Debug, Clone, Copy)]
pub(crate) struct FrameStatsInput<'a> {
    /// The frozen system prompt actually sent.
    pub system_prompt: &'a str,
    pub tool_def_tokens: u32,
    pub tool_count: u32,
    pub rebuild_reason: crate::agent::session_frame::FrameRebuildReason,
    pub created_at: i64,
    /// This turn's `<turn_context>` report, if one was rendered.
    pub report: Option<&'a crate::agent::turn_context::TurnContextReport>,
}

/// Estimated tokens of the tool definitions sent with an LLM call (same
/// formula as the `Calling LLM` log line).
pub(crate) fn tool_def_tokens(tools: &[crate::agent::types::ToolDefinition]) -> u32 {
    tools
        .iter()
        .map(|t| estimate_tokens(&t.name) + estimate_tokens(&t.description) + estimate_tokens(&t.parameters.to_string()))
        .sum()
}

/// Pure builder for the `agent:context_stats` payload (testable without a
/// Tauri runtime). `configured_compaction_threshold` is `Some` only for
/// transcript-mode sessions; it is clamped to the same effective threshold
/// the agent loop compacts at and surfaced as `compactionThresholdTokens`.
/// `frame` is `Some` only in transcript mode: `systemPromptTokens` then
/// counts the frozen frame actually sent (not the base prompt) and the
/// frame fields are filled.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_context_stats(
    conversation_id: &str,
    system_prompt: &str,
    skills_manifest: &str,
    messages: &[ChatMessage],
    cumulative_input: u32,
    cumulative_output: u32,
    model: &str,
    configured_compaction_threshold: Option<usize>,
    frame: Option<&FrameStatsInput<'_>>,
) -> ContextStats {
        let model_context_length = get_model_context_length(model);
        let system_prompt_tokens = estimate_tokens(frame.map_or(system_prompt, |f| f.system_prompt));
        let mut messages_tokens: u32 = 0;
        let mut tool_use_tokens: u32 = 0;

        for msg in messages {
            // Skip compacted messages — they stay in memory for UI replay
            // but must not inflate the context usage estimate. (P1 logical-marking)
            if msg.compacted {
                continue;
            }
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

        let compaction_threshold_tokens = configured_compaction_threshold.map(|configured| {
            crate::agent::compaction_policy::effective_compaction_threshold(
                configured,
                model_context_length as usize,
            ) as u32
        });
        ContextStats {
            conversation_id: conversation_id.to_string(),
            model_context_length,
            system_prompt_tokens,
            mcp_prompts_tokens: 0,
            skills_tokens: estimate_tokens(skills_manifest),
            messages_tokens,
            tool_use_tokens,
            compact_buffer_tokens: compact_buffer,
            free_tokens: free,
            cumulative_input_tokens: Some(cumulative_input),
            cumulative_output_tokens: Some(cumulative_output),
            compaction_threshold_tokens,
            frozen_system_prompt: frame.map(|_| true),
            tool_def_tokens: frame.map(|f| f.tool_def_tokens),
            tool_count: frame.map(|f| f.tool_count),
            frame_rebuild_reason: frame.map(|f| f.rebuild_reason.as_str().to_string()),
            frame_created_at: frame.map(|f| f.created_at),
            memory_injection: frame.and_then(|f| f.report).map(|r| r.memory),
            turn_context_tokens: frame.and_then(|f| f.report).map(|r| r.total_tokens),
        }
}

#[cfg(test)]
mod context_stats_tests {
    use super::*;

    #[test]
    fn context_stats_includes_compaction_threshold() {
        let model = "claude-sonnet-4-20250514";
        let window = get_model_context_length(model) as usize;
        let stats = build_context_stats(
            "s1", "sys", "", &[ChatMessage::user("hi")], 0, 0, model, Some(128_000), None,
        );
        let json = serde_json::to_value(&stats).unwrap();
        let expected =
            crate::agent::compaction_policy::effective_compaction_threshold(128_000, window);
        assert_eq!(json["compactionThresholdTokens"], serde_json::json!(expected));

        // The effective clamp applies: a configured value below the floor is raised.
        let floored = build_context_stats("s1", "sys", "", &[], 0, 0, model, Some(1_000), None);
        let json = serde_json::to_value(&floored).unwrap();
        assert_eq!(
            json["compactionThresholdTokens"],
            serde_json::json!(crate::agent::compaction_policy::effective_compaction_threshold(1_000, window))
        );
    }

    #[test]
    fn context_stats_omits_compaction_threshold_for_non_transcript() {
        let stats = build_context_stats("s1", "sys", "", &[], 0, 0, "claude-sonnet-4-20250514", None, None);
        let json = serde_json::to_value(&stats).unwrap();
        assert!(json.get("compactionThresholdTokens").is_none(), "got {json}");
    }

    #[test]
    fn context_stats_includes_frame_fields_only_in_transcript_mode() {
        use crate::agent::session_frame::FrameRebuildReason;
        use crate::agent::turn_context::{MemoryInjection, TurnContextReport};
        let model = "claude-sonnet-4-20250514";
        let frozen = "FROZEN FRAME PROMPT ".repeat(50);

        // Non-transcript: no frame fields; system prompt = the base prompt.
        let legacy = build_context_stats("s1", "sys", "", &[], 0, 0, model, None, None);
        let json = serde_json::to_value(&legacy).unwrap();
        for key in [
            "frozenSystemPrompt", "toolDefTokens", "toolCount", "frameRebuildReason",
            "frameCreatedAt", "memoryInjection", "turnContextTokens",
        ] {
            assert!(json.get(key).is_none(), "{key} must be omitted: {json}");
        }
        assert_eq!(json["systemPromptTokens"], estimate_tokens("sys"));

        // Transcript: system prompt tokens reflect the frozen frame.
        let report = TurnContextReport {
            memory: MemoryInjection::Delta,
            memory_tokens: 12,
            total_tokens: 40,
            mode_changed: None,
            tools_added: vec![],
            tools_removed: vec![],
            tools_restored: false,
        };
        let frame = FrameStatsInput {
            system_prompt: &frozen,
            tool_def_tokens: 900,
            tool_count: 14,
            rebuild_reason: FrameRebuildReason::PromptChanged,
            created_at: 1_700_000_000_000,
            report: Some(&report),
        };
        let stats = build_context_stats("s1", "sys", "", &[], 0, 0, model, Some(128_000), Some(&frame));
        let json = serde_json::to_value(&stats).unwrap();
        assert_eq!(json["systemPromptTokens"], estimate_tokens(&frozen));
        assert_eq!(json["frozenSystemPrompt"], true);
        assert_eq!(json["toolDefTokens"], 900);
        assert_eq!(json["toolCount"], 14);
        assert_eq!(json["frameRebuildReason"], "prompt_changed");
        assert_eq!(json["frameCreatedAt"], 1_700_000_000_000_i64);
        assert_eq!(json["memoryInjection"], "delta");
        assert_eq!(json["turnContextTokens"], 40);

        // Transcript without a report this turn: report-derived fields omitted.
        let no_report = FrameStatsInput { report: None, ..frame };
        let json = serde_json::to_value(build_context_stats("s1", "sys", "", &[], 0, 0, model, Some(128_000), Some(&no_report))).unwrap();
        assert_eq!(json["frozenSystemPrompt"], true);
        assert!(json.get("memoryInjection").is_none() && json.get("turnContextTokens").is_none(), "{json}");
    }
}
