use crate::agent::types::*;
use crate::agent::context::{LayeredContextBuilder, LayeredContextConfig};
use tracing;

/// Tools that pause the agent for synchronous user interaction. After
/// these resolve, the agent's natural next step is conversational
/// wrap-up — phrases like "让我推荐" / "let me also tell you" are CORRECT
/// responses, NOT missed tool calls. The tool_intent nudge must skip
/// after these or the agent gets force-retried into meta-acknowledgement
/// (see 2026-05-18 ask_user session c2fc9739).
const INTERACTIVE_TOOLS: &[&str] = &[
    "ask_user",
    "exit_plan_mode",
    "request_plan_mode_switch",
];

/// Walk message history backward; return the name of the most recent
/// `ContentBlock::ToolUse` (Some) or None if no tool has been called yet.
fn last_tool_use_name(messages: &[ChatMessage]) -> Option<&str> {
    for msg in messages.iter().rev() {
        for block in msg.content.iter().rev() {
            if let ContentBlock::ToolUse { name, .. } = block {
                return Some(name.as_str());
            }
        }
    }
    None
}

/// True if the most recent tool was an interactive (user-blocking) tool.
/// Used by the tool_intent nudge gate to avoid forcing a useless retry
/// after the user has just provided input via ask_user / exit_plan_mode.
fn last_tool_was_interactive(messages: &[ChatMessage]) -> bool {
    last_tool_use_name(messages)
        .map(|n| INTERACTIVE_TOOLS.contains(&n))
        .unwrap_or(false)
}

/// The main Think-Act-Observe loop implementing the React pattern.
///
/// Flow:
/// 1. check_signals() — Check for stop/cancel/inject signals
/// 2. compress_context() — Compress context if approaching token budget
/// 3. before_llm_call() — Pre-LLM hook (cost guard, tool refresh)
/// 4. call_llm() — Delegate calls the LLM
/// 5. Handle response:
///    - Text: tool_intent_nudge? → inject and continue, else return
///    - ToolCalls: truncation check → execute → loop
/// 6. after_iteration() — Post-turn hook
pub async fn run_agentic_loop(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
) -> LoopOutcome {
    let mut truncation_count = 0usize;
    let mut consecutive_tool_intent_nudges = 0usize;

    // Transition: Idle → Processing
    reason_ctx.thread_state = ThreadState::Processing;
    tracing::info!("AGENTIC_LOOP: starting (max_iterations={})", config.max_iterations);

    for iteration in 1..=config.max_iterations {
        tracing::debug!("Agent loop iteration {}/{}", iteration, config.max_iterations);

        // ── 1. Check signals ──────────────────────────────────────────
        match delegate.check_signals().await {
            LoopSignal::Stop => {
                tracing::info!("Agent loop stopped by signal");
                reason_ctx.thread_state = ThreadState::Interrupted;
                return LoopOutcome::Stopped;
            }
            LoopSignal::Cancel => {
                tracing::info!("Agent loop cancelled by signal");
                reason_ctx.thread_state = ThreadState::Interrupted;
                // Preserve partial code buffer accumulated across truncated responses
                let partial_code = reason_ctx.partial_code_buffer.as_ref().map(|(lang, content)| {
                    format!("```{}\n{}\n```", lang, content)
                });
                return LoopOutcome::Cancelled { partial_code };
            }
            LoopSignal::InjectMessage { content } => {
                tracing::debug!("Injecting message into context");
                reason_ctx.messages.push(ChatMessage::user(&content));
            }
            LoopSignal::Continue => {}
        }

        // M1-T2d (R-6) — observe cancellation token between stages so the
        // loop can exit cleanly after each in-flight call completes,
        // rather than waiting for the next iteration's check_signals poll.
        // See docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md
        // §C for the full R-6 surface.
        if reason_ctx.is_cancelled() {
            tracing::info!("Agent loop cancelled mid-iteration (post check_signals)");
            reason_ctx.thread_state = ThreadState::Interrupted;
            let partial_code = reason_ctx.partial_code_buffer.as_ref().map(|(lang, content)| {
                format!("```{}\n{}\n```", lang, content)
            });
            return LoopOutcome::Cancelled { partial_code };
        }

        // ── 2. Context compression ───────────────────────────────────
        compress_context_if_needed(reason_ctx, config, delegate).await;

        // ── 3. Pre-LLM call hook ─────────────────────────────────────
        if let Some(outcome) = delegate.before_llm_call(reason_ctx, iteration).await {
            reason_ctx.thread_state = match &outcome {
                LoopOutcome::Failure { .. } => ThreadState::Failed {
                    error: "early exit from before_llm_call".into(),
                },
                LoopOutcome::NeedApproval { tool_name, tool_call_id, parameters } => {
                    ThreadState::AwaitingApproval {
                        tool_name: tool_name.clone(),
                        tool_id: tool_call_id.clone(),
                        arguments: parameters.clone(),
                    }
                }
                _ => ThreadState::Completed,
            };
            return outcome;
        }

        // ── 4. Call LLM ──────────────────────────────────────────────
        let output = match delegate.call_llm(reason_ctx, iteration).await {
            Ok(output) => output,
            Err(e) => {
                tracing::error!("LLM call failed at iteration {}: {}", iteration, e);
                reason_ctx.thread_state = ThreadState::Failed {
                    error: e.to_string(),
                };
                return LoopOutcome::Failure {
                    error: e.to_string(),
                };
            }
        };

        // M1-T2d (R-6) — check cancellation after the LLM call returns.
        // Without this, a cancel signal that arrives during a 30s
        // streaming response would only be observed at the next iteration.
        if reason_ctx.is_cancelled() {
            tracing::info!("Agent loop cancelled after call_llm");
            reason_ctx.thread_state = ThreadState::Interrupted;
            let partial_code = reason_ctx.partial_code_buffer.as_ref().map(|(lang, content)| {
                format!("```{}\n{}\n```", lang, content)
            });
            return LoopOutcome::Cancelled { partial_code };
        }

        // ── 5. Handle response ───────────────────────────────────────
        match output {
            RespondOutput::Text { text, thinking, thinking_signature, metadata } => {
                // Track token usage and emit events
                if let Some(ref usage) = metadata.usage {
                    reason_ctx.total_input_tokens += usage.input_tokens;
                    reason_ctx.total_output_tokens += usage.output_tokens;
                    delegate.on_usage(usage, reason_ctx).await;
                }

                // Tool intent nudge: LLM talks about using a tool but doesn't actually call one.
                // Skip the nudge if the most recent tool was an interactive one
                // (ask_user / exit_plan_mode / request_plan_mode_switch). After the
                // user provides input, conversational wrap-up phrases like "let me
                // recommend" or "我来推荐" are correct — forcing another tool call
                // produces meta-acknowledgement nonsense.
                //
                // Also skip when the triggering user message is purely conversational
                // (e.g. "你在干啥", "你好"). For those, "让我看看..." is a natural preamble
                // to a text reply — nudging converts it into spurious glob/ls/date calls
                // the user never requested (root cause: 2026-05-18 "你在干啥" incident).
                let nudge_user_text = reason_ctx.messages.iter().rev()
                    .find(|m| {
                        matches!(m.role, MessageRole::User)
                            && m.content.iter().any(|b| matches!(b, ContentBlock::Text { .. }))
                    })
                    .and_then(|m| m.content.iter().find_map(|b| {
                        if let ContentBlock::Text { text } = b { Some(text.as_str()) } else { None }
                    }))
                    .unwrap_or("");
                if config.enable_tool_intent_nudge
                    && consecutive_tool_intent_nudges < config.max_tool_intent_nudges
                    && !reason_ctx.force_text
                    && !last_tool_was_interactive(&reason_ctx.messages)
                    && !is_purely_conversational(nudge_user_text)
                    && llm_signals_tool_intent(&text)
                {
                    consecutive_tool_intent_nudges += 1;
                    tracing::info!(
                        iteration,
                        count = consecutive_tool_intent_nudges,
                        max = config.max_tool_intent_nudges,
                        "Tool intent nudge"
                    );

                    delegate.on_tool_intent_nudge(&text, reason_ctx).await;

                    // Record the assistant's text (and thinking if present), then inject nudge
                    let mut blocks = Vec::new();
                    if let Some(ref t) = thinking {
                        if !t.is_empty() {
                            blocks.push(ContentBlock::Thinking { thinking: t.clone(), signature: thinking_signature.clone() });
                        }
                    }
                    blocks.push(ContentBlock::Text { text: text.clone() });
                    reason_ctx.messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: blocks,
                        compacted: false,
                    });
                    reason_ctx.messages.push(ChatMessage::user(TOOL_INTENT_NUDGE));

                    delegate.after_iteration(iteration).await;
                    continue;
                }

                // Reset nudge counter on non-intent text
                if !llm_signals_tool_intent(&text) {
                    consecutive_tool_intent_nudges = 0;
                }

                // Handle text response
                match delegate
                    .handle_text_response(&text, metadata, reason_ctx)
                    .await
                {
                    TextAction::Return(outcome) => {
                        // Push the final assistant message (with its thinking
                        // block) into `reason_ctx.messages` so the caller's
                        // post-loop `extract_process_meta_from_messages` pass
                        // can persist `reasoning` to `agent_messages.reasoning`.
                        //
                        // Without this push, only intermediate Continue /
                        // ContinueWithNudge turns get their thinking persisted —
                        // simple single-turn responses (the common case) lose
                        // their thinking entirely. Symptom: the historical
                        // message in AgentMessages.tsx has no inline
                        // ThinkingBlock (because message.reasoning is empty),
                        // and the frontend streaming bubble survives the
                        // stream-complete cleanup with only a "THINKING >"
                        // pill visible — the orphan ghost row.
                        //
                        // Mirrors the Continue / ContinueWithNudge block
                        // construction so the persisted message shape is
                        // identical regardless of the loop's exit path.
                        let mut blocks = Vec::new();
                        if let Some(ref t) = thinking {
                            if !t.is_empty() {
                                blocks.push(ContentBlock::Thinking {
                                    thinking: t.clone(),
                                    signature: thinking_signature.clone(),
                                });
                            }
                        }
                        blocks.push(ContentBlock::Text { text: text.clone() });
                        reason_ctx.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: blocks,
                            compacted: false,
                        });

                        reason_ctx.thread_state = ThreadState::Completed;
                        delegate.after_iteration(iteration).await;
                        return outcome;
                    }
                    TextAction::Continue => {
                        let mut blocks = Vec::new();
                        if let Some(ref t) = thinking {
                            if !t.is_empty() {
                                blocks.push(ContentBlock::Thinking { thinking: t.clone(), signature: thinking_signature.clone() });
                            }
                        }
                        blocks.push(ContentBlock::Text { text: text.clone() });
                        reason_ctx.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: blocks,
                            compacted: false,
                        });
                        delegate.after_iteration(iteration).await;
                        continue;
                    }
                    // Dispatcher detected a condition (length truncation, pending plan steps,
                    // etc.) and wants to inject a nudge. The dispatcher must NOT push the
                    // assistant message itself — we own that here to avoid double-push.
                    TextAction::ContinueWithNudge(nudge) => {
                        let mut blocks = Vec::new();
                        if let Some(ref t) = thinking {
                            if !t.is_empty() {
                                blocks.push(ContentBlock::Thinking {
                                    thinking: t.clone(),
                                    signature: thinking_signature.clone(),
                                });
                            }
                        }
                        blocks.push(ContentBlock::Text { text: text.clone() });
                        reason_ctx.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: blocks,
                            compacted: false,
                        });
                        reason_ctx.messages.push(ChatMessage::user(&nudge));
                        delegate.after_iteration(iteration).await;
                        continue;
                    }
                    // The model outputted file content as markdown text instead of calling
                    // write_file. Dispatcher extracted synthetic ToolCalls from the code
                    // blocks. Route them through the full tool execution path (safety,
                    // approval, events) just like model-initiated calls.
                    TextAction::RescueWithToolCalls(synthetic_calls) => {
                        // Build the assistant message: text content + synthetic tool_use
                        // blocks so the API sees a valid tool_use → tool_result exchange.
                        let mut blocks = Vec::new();
                        if let Some(ref t) = thinking {
                            if !t.is_empty() {
                                blocks.push(ContentBlock::Thinking {
                                    thinking: t.clone(),
                                    signature: thinking_signature.clone(),
                                });
                            }
                        }
                        blocks.push(ContentBlock::Text { text: text.clone() });
                        for tc in &synthetic_calls {
                            blocks.push(ContentBlock::ToolUse {
                                id: tc.id.clone(),
                                name: tc.name.clone(),
                                input: tc.arguments.clone(),
                            });
                        }
                        reason_ctx.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: blocks,
                            compacted: false,
                        });

                        match delegate.execute_tool_calls(synthetic_calls, reason_ctx).await {
                            Ok(Some(outcome)) => {
                                reason_ctx.thread_state = match &outcome {
                                    LoopOutcome::NeedApproval {
                                        tool_name,
                                        tool_call_id,
                                        parameters,
                                    } => ThreadState::AwaitingApproval {
                                        tool_name: tool_name.clone(),
                                        tool_id: tool_call_id.clone(),
                                        arguments: parameters.clone(),
                                    },
                                    _ => ThreadState::Completed,
                                };
                                delegate.after_iteration(iteration).await;
                                return outcome;
                            }
                            Ok(None) => {
                                delegate.after_iteration(iteration).await;
                                continue;
                            }
                            Err(e) => {
                                tracing::error!("Rescue tool execution failed: {}", e);
                                reason_ctx.thread_state =
                                    ThreadState::Failed { error: e.to_string() };
                                return LoopOutcome::Failure { error: e.to_string() };
                            }
                        }
                    }
                }
            }

            RespondOutput::ToolCalls {
                tool_calls,
                text,
                thinking,
                thinking_signature,
                metadata,
            } => {
                // Track token usage and emit events
                if let Some(ref usage) = metadata.usage {
                    reason_ctx.total_input_tokens += usage.input_tokens;
                    reason_ctx.total_output_tokens += usage.output_tokens;
                    delegate.on_usage(usage, reason_ctx).await;
                }

                // ── Truncation handling ──────────────────────────────
                if metadata.finish_reason.as_deref() == Some("length") {
                    truncation_count += 1;
                    let names: Vec<&str> =
                        tool_calls.iter().map(|tc| tc.name.as_str()).collect();
                    tracing::warn!(
                        iteration,
                        tools = ?names,
                        truncation_count,
                        max = config.max_truncations,
                        "Discarding truncated tool calls (finish_reason=length)"
                    );

                    // Preserve partial assistant content if any
                    if let Some(ref t) = text {
                        if !t.is_empty() {
                            reason_ctx.messages.push(ChatMessage::assistant(t));
                        }
                    }

                    // Inject truncation notice
                    reason_ctx.messages.push(ChatMessage::user(TRUNCATED_TOOL_CALL_NOTICE));

                    // After repeated truncations, force text-only mode
                    if truncation_count >= config.max_truncations {
                        tracing::warn!(
                            "Max truncations ({}) reached, forcing text-only mode",
                            config.max_truncations
                        );
                        reason_ctx.force_text = true;
                    }

                    delegate.after_iteration(iteration).await;
                    continue;
                }

                // Successful tool calls reset counters
                consecutive_tool_intent_nudges = 0;
                truncation_count = 0;

                // Record the assistant's response (thinking + text + tool_use blocks)
                let mut blocks: Vec<ContentBlock> = Vec::new();
                if let Some(ref t) = thinking {
                    if !t.is_empty() {
                        blocks.push(ContentBlock::Thinking { thinking: t.clone(), signature: thinking_signature.clone() });
                    }
                }
                if let Some(t) = &text {
                    if !t.is_empty() {
                        blocks.push(ContentBlock::Text { text: t.clone() });
                    }
                }
                for tc in &tool_calls {
                    blocks.push(ContentBlock::ToolUse {
                        id: tc.id.clone(),
                        name: tc.name.clone(),
                        input: tc.arguments.clone(),
                    });
                }
                let assistant_msg = ChatMessage {
                    role: MessageRole::Assistant,
                    content: blocks,
                    compacted: false,
                };
                reason_ctx.messages.push(assistant_msg);

                // Execute tool calls
                match delegate.execute_tool_calls(tool_calls, reason_ctx).await {
                    Ok(Some(outcome)) => {
                        reason_ctx.thread_state = match &outcome {
                            LoopOutcome::NeedApproval {
                                tool_name,
                                tool_call_id,
                                parameters,
                            } => ThreadState::AwaitingApproval {
                                tool_name: tool_name.clone(),
                                tool_id: tool_call_id.clone(),
                                arguments: parameters.clone(),
                            },
                            _ => ThreadState::Completed,
                        };
                        delegate.after_iteration(iteration).await;
                        return outcome;
                    }
                    Ok(None) => {
                        // Tool calls executed, loop continues
                        delegate.after_iteration(iteration).await;
                        continue;
                    }
                    Err(e) => {
                        tracing::error!("Tool execution error: {}", e);
                        reason_ctx.thread_state = ThreadState::Failed {
                            error: e.to_string(),
                        };
                        return LoopOutcome::Failure {
                            error: e.to_string(),
                        };
                    }
                }
            }
        }
    }

    tracing::warn!(
        "Agent loop reached max iterations: {}",
        config.max_iterations
    );
    reason_ctx.thread_state = ThreadState::Completed;
    LoopOutcome::MaxIterations
}

// ─── Context Compression ────────────────────────────────────────────────

/// Compress conversation context if token usage exceeds configured thresholds.
///
/// Strategy:
/// - At 80% budget: keep system prompt + last N turns, drop older messages.
/// - At 95% budget: aggressively drop oldest messages until under budget.
async fn compress_context_if_needed(
    reason_ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    delegate: &dyn LoopDelegate,
) {
    if config.token_budget == 0 {
        return;
    }

    // Cap the effective budget by the model's actual context window.
    // For GPT-4o (128K) using a 200K budget, this prevents budget drift
    // that would let the context exceed the model's hard limit before
    // our compression heuristics kick in.
    let effective_budget = if config.model_context_length > 0 {
        config.token_budget.min(config.model_context_length as usize)
    } else {
        config.token_budget
    };

    let estimated_tokens = reason_ctx.estimate_token_count();
    let hard_limit = (effective_budget as f32 * config.hard_truncation_threshold) as usize;
    let soft_limit = (effective_budget as f32 * config.compression_threshold) as usize;

    if estimated_tokens >= hard_limit {
        // Hard truncation: aggressively remove oldest messages until under soft limit
        tracing::warn!(
            estimated_tokens,
            hard_limit,
            "Hard context truncation triggered"
        );
        hard_truncate_context(reason_ctx, soft_limit);
    } else if estimated_tokens >= soft_limit {
        // Soft compression: keep last N turns
        tracing::info!(
            estimated_tokens,
            soft_limit,
            keep_turns = config.compression_keep_turns,
            "Context compression triggered"
        );
        soft_compress_context(reason_ctx, config.compression_keep_turns, config.model_context_length, delegate).await;
    }
}

/// Public entry point for forcing a compaction outside the auto-trigger
/// in the main loop. Used by the `/compact` user command (handled in
/// tauri_commands::send_message): drains all but the last `keep_turns`
/// messages and prepends a summary placeholder. Idempotent when the
/// session is already small.
///
/// Uses template-based placeholder summary (no LLM call) because the
/// `/compact` command runs synchronously without access to the LLM provider.
pub fn force_compact(reason_ctx: &mut ReasoningContext, keep_turns: usize) {
    force_compact_sync(reason_ctx, keep_turns);
}

/// Synchronous compaction that always uses the template placeholder.
/// Internal helper shared by `force_compact` and available for cases
/// where async LLM access is not feasible.
fn force_compact_sync(reason_ctx: &mut ReasoningContext, keep_turns: usize) {
    let active_indices: Vec<usize> = reason_ctx.messages.iter()
        .enumerate()
        .filter(|(_, m)| !m.compacted)
        .map(|(i, _)| i)
        .collect();
    let active_count = active_indices.len();
    if active_count <= keep_turns {
        return;
    }

    let desired_removed = active_count - keep_turns;
    let desired_split_idx = active_indices[desired_removed];

    // Transaction-safe boundary calculation
    let split_idx = find_safe_compaction_boundary(&reason_ctx.messages, desired_split_idx);

    // Messages to compact
    let messages_to_compact: Vec<ChatMessage> = reason_ctx.messages[0..split_idx].iter()
        .filter(|m| !m.compacted)
        .cloned()
        .collect();
    if messages_to_compact.is_empty() {
        return;
    }

    let removed_count = messages_to_compact.len();

    // Logical marking
    for i in 0..split_idx {
        reason_ctx.messages[i].compacted = true;
    }

    // Synchronous fold using the extractive fallback fold
    let fallback_fold = crate::agent::compact::summarize::extractive_fallback_fold(&messages_to_compact);
    let fold_markdown = fallback_fold.to_markdown();
    let padded_summary = crate::agent::compact::cache_align::align_to_1024_tokens(&fold_markdown);

    let summary = format!(
        "[Context compressed: {} earlier messages compacted]\n\n{}",
        removed_count,
        padded_summary
    );

    reason_ctx.messages.insert(0, ChatMessage::user(&summary));

    tracing::info!(
        removed = removed_count,
        remaining = reason_ctx.messages.len(),
        "Context force-compacted (sync, extractive fold summary)"
    );
}

/// Keep only the last `keep_turns` messages, inserting a L1 archive summary
/// placeholder. Uses LayeredContextBuilder for proper L0/L1 structural
/// organization and model-aware token budgeting. (P2 fix: 2026-05-16)
///
/// Now uses logical marking (`compacted = true`) instead of physical `drain()`.
/// Compacted messages stay in the vec for full replay in the frontend but are
/// skipped by `estimate_token_count` and the LLM context builder.
/// (P1 logical-marking: 2026-05-16)
///
/// When `delegate.summarize_for_compression` returns a summary, it is used as
/// the L1 archive summary. Otherwise, falls back to the template-based placeholder.
fn is_boundary_safe(messages: &[ChatMessage], split_idx: usize) -> bool {
    if split_idx == 0 || split_idx >= messages.len() {
        return false;
    }

    // 1. The first message on the active side (at split_idx) must be a human User message
    let active_first = &messages[split_idx];
    if active_first.role != MessageRole::User {
        return false;
    }
    // Check if it contains any ToolResult content blocks
    let has_tool_result = active_first.content.iter().any(|block| {
        matches!(block, ContentBlock::ToolResult { .. })
    });
    if has_tool_result {
        return false;
    }

    // 2. Check for split tool transactions.
    // Collect all ToolUse IDs on the compacted side (0..split_idx)
    let mut compacted_tool_use_ids = std::collections::HashSet::new();
    for msg in &messages[0..split_idx] {
        for block in &msg.content {
            if let ContentBlock::ToolUse { id, .. } = block {
                compacted_tool_use_ids.insert(id.clone());
            }
        }
    }

    // Collect all ToolResult tool_use_ids on the active side (split_idx..)
    let mut active_tool_result_ids = std::collections::HashSet::new();
    for msg in &messages[split_idx..] {
        for block in &msg.content {
            if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                active_tool_result_ids.insert(tool_use_id.clone());
            }
        }
    }

    // If there is any intersection between compacted_tool_use_ids and active_tool_result_ids,
    // then at least one tool transaction has been split!
    let has_split = compacted_tool_use_ids.intersection(&active_tool_result_ids).next().is_some();
    if has_split {
        return false;
    }

    true
}

fn find_safe_compaction_boundary(messages: &[ChatMessage], desired_split: usize) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let max_len = messages.len();
    let mut offset = 0;
    while desired_split + offset < max_len || desired_split >= offset {
        if desired_split + offset < max_len && is_boundary_safe(messages, desired_split + offset) {
            return desired_split + offset;
        }
        if offset > 0 && desired_split >= offset && is_boundary_safe(messages, desired_split - offset) {
            return desired_split - offset;
        }
        offset += 1;
        if offset > max_len {
            break;
        }
    }

    // Fallback: If no safe boundary could be found, find any User message that is not a tool result
    for i in (1..messages.len()).rev() {
        if messages[i].role == MessageRole::User {
            let has_tool_result = messages[i].content.iter().any(|block| {
                matches!(block, ContentBlock::ToolResult { .. })
            });
            if !has_tool_result {
                return i;
            }
        }
    }

    // Hard fallback: return desired_split
    desired_split
}

/// Keep only the last `keep_turns` messages, inserting a L1 archive summary
/// placeholder. Uses LayeredContextBuilder for proper L0/L1 structural
/// organization and model-aware token budgeting. (P2 fix: 2026-05-16)
///
/// Now uses logical marking (`compacted = true`) instead of physical `drain()`.
/// Compacted messages stay in the vec for full replay in the frontend but are
/// skipped by `estimate_token_count` and the LLM context builder.
/// (P1 logical-marking: 2026-05-16)
///
/// When `delegate.summarize_for_compression` returns a summary, it is used as
/// the L1 archive summary. Otherwise, falls back to the template-based placeholder.
async fn soft_compress_context(
    reason_ctx: &mut ReasoningContext,
    keep_turns: usize,
    model_window: u32,
    delegate: &dyn LoopDelegate,
) {
    let active_indices: Vec<usize> = reason_ctx.messages.iter()
        .enumerate()
        .filter(|(_, m)| !m.compacted)
        .map(|(i, _)| i)
        .collect();
    let active_count = active_indices.len();

    // Double-Threshold Trigger
    if active_count <= keep_turns + 4 { // OVERFLOW_TURNS = 4
        tracing::debug!(
            active_count,
            keep_turns,
            "Bypassing context compression: active turns within target+overflow safety window (Double-Threshold)"
        );
        return;
    }

    let desired_removed = active_count - keep_turns;
    let desired_split_idx = active_indices[desired_removed];

    // Transaction-safe boundary calculation
    let split_idx = find_safe_compaction_boundary(&reason_ctx.messages, desired_split_idx);

    // Messages to compact in this turn
    let messages_to_compact: Vec<ChatMessage> = reason_ctx.messages[0..split_idx].iter()
        .filter(|m| !m.compacted)
        .cloned()
        .collect();
    if messages_to_compact.is_empty() {
        return;
    }

    let removed_count = messages_to_compact.len();

    // Logical marking of messages before split_idx as compacted
    for i in 0..split_idx {
        reason_ctx.messages[i].compacted = true;
    }

    // Build L1 archive summary: Call summarize_to_fold, fallback to extractive fold on failure
    let fold = if let Some(f) = delegate.summarize_to_fold(&messages_to_compact).await {
        tracing::info!(
            summary_type = "llm_fold",
            removed = removed_count,
            "Using LLM-generated StructuredFold for context compaction"
        );
        f
    } else {
        tracing::info!(
            summary_type = "fallback_fold",
            removed = removed_count,
            "LLM fold summarization failed/returned None, falling back to extractive fold"
        );
        crate::agent::compact::summarize::extractive_fallback_fold(&messages_to_compact)
    };

    let fold_markdown = fold.to_markdown();
    let padded_summary = crate::agent::compact::cache_align::align_to_1024_tokens(&fold_markdown);

    let summary = format!(
        "[Context compressed: {} earlier messages compacted]\n\n{}",
        removed_count,
        padded_summary
    );

    // ── Assemble with model-aware LayeredContextBuilder ───────────────
    // Estimate L0 token budget from the remaining (non-compacted) messages.
    let l0_estimate: usize = reason_ctx.messages.iter()
        .filter(|m| !m.compacted)
        .map(|m| m.content.iter().map(|b| match b {
            ContentBlock::Text { text } => estimate_tokens(text) as usize,
            ContentBlock::Thinking { thinking, .. } => estimate_tokens(thinking) as usize,
            ContentBlock::ToolUse { name, input, .. } => {
                estimate_tokens(name) as usize + estimate_tokens(&input.to_string()) as usize + 20
            }
            ContentBlock::ToolResult { content, .. } => estimate_tokens(content) as usize + 10,
        }).sum::<usize>())
        .sum();

    // Use model-aware layered config — L0/L1/L2 budgets scale with model window.
    // When model_window is 0 (unknown model), fall back to static defaults.
    let layered_config = if model_window > 0 {
        LayeredContextConfig::from_model_window(model_window)
    } else {
        LayeredContextConfig {
            max_context_tokens: l0_estimate + 4000,
            l0_target_tokens: l0_estimate.max(1),
            l1_target_tokens: 4000, // archive summary budget
            ..LayeredContextConfig::default()
        }
    };

    let mut builder = LayeredContextBuilder::new(layered_config.clone());
    builder.add_archive(&summary);

    // Prepend L1 archive summary as system-style user message at position 0.
    reason_ctx
        .messages
        .insert(0, ChatMessage::user(&summary));

    let stats = builder.get_token_stats();
    let l1_tokens = stats.l1_archive_tokens;
    let l1_budget = layered_config.l1_target_tokens;
    tracing::info!(
        removed = removed_count,
        remaining = reason_ctx.messages.len(),
        l0_estimate_tokens = l0_estimate,
        l1_summary_tokens = l1_tokens,
        l1_budget,
        model_window,
        layered_budget_remaining = stats.budget_remaining,
        "Context soft-compressed (layered L0/L1, model-aware)"
    );
}

/// Build a compression summary placeholder from compacted messages.
/// Collects tool names used and user message topic previews.
/// Extracted as a standalone helper so future LLM-based summarization
/// can replace just this function without touching the layered assembly.
fn build_compression_summary_refs(removed: &[&ChatMessage], removed_count: usize) -> String {
    // Collect tool names from removed messages
    let tool_names: Vec<String> = removed
        .iter()
        .flat_map(|m| {
            m.content.iter().filter_map(|b| match b {
                ContentBlock::ToolUse { name, .. } => Some(name.clone()),
                _ => None,
            })
        })
        .collect();

    // Capture first + last 80 chars of each user message as topics.
    // Users often place key instructions at the end of long messages;
    // capturing both ends preserves more signal than head-only.
    let user_topics: Vec<String> = removed
        .iter()
        .filter(|m| matches!(m.role, MessageRole::User))
        .filter_map(|m| {
            let text: String = m.content.iter().filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            }).collect::<Vec<_>>().join(" ");
            if text.is_empty() { return None }
            let char_count = text.chars().count();
            let topic = if char_count > 160 {
                let first: String = text.chars().take(80).collect();
                let last: String = text.chars().rev().take(80).collect::<String>().chars().rev().collect();
                format!("{}...{}", first.trim(), last.trim())
            } else {
                text.trim().to_string()
            };
            let trimmed = topic.trim();
            if trimmed.is_empty() { None } else { Some(trimmed.to_string()) }
        })
        .collect();

    let unique_tools: Vec<String> = {
        let mut seen = std::collections::HashSet::new();
        tool_names
            .into_iter()
            .filter(|n| seen.insert(n.clone()))
            .collect()
    };

    let mut parts: Vec<String> = vec![format!(
        "[Context compressed: {} earlier messages compacted to stay within token budget]",
        removed_count
    )];

    if !unique_tools.is_empty() {
        parts.push(format!("Tools used: {}", unique_tools.join(", ")));
    }

    if !user_topics.is_empty() {
        let preview_count = user_topics.len().min(5);
        let topics_preview: Vec<&str> = user_topics.iter().take(preview_count).map(|s| s.as_str()).collect();
        let suffix = if user_topics.len() > preview_count {
            format!(" (+{} more)", user_topics.len() - preview_count)
        } else {
            String::new()
        };
        parts.push(format!(
            "Earlier topics: {}{}",
            topics_preview.join("; "),
            suffix
        ));
    }

    parts.join("\n")
}

/// Logically mark oldest non-compacted messages as compacted until estimated
/// token count is below target. Uses logical marking instead of physical
/// removal so the frontend can replay all messages.
/// (P1 logical-marking: 2026-05-16)
fn hard_truncate_context(reason_ctx: &mut ReasoningContext, target_tokens: usize) {
    let mut removed = 0;
    // Mark oldest non-compacted messages one-by-one until under target
    while reason_ctx.messages.len() > 2 && reason_ctx.estimate_token_count() > target_tokens {
        // Find the first non-compacted message and mark it
        if let Some(pos) = reason_ctx.messages.iter().position(|m| !m.compacted) {
            reason_ctx.messages[pos].compacted = true;
            removed += 1;
        } else {
            break; // all messages already compacted
        }
    }

    if removed > 0 {
        reason_ctx.messages.insert(
            0,
            ChatMessage::user(&format!(
                "[Context hard-truncated: {} oldest messages compacted to prevent overflow]",
                removed
            )),
        );
        tracing::warn!(
            removed,
            remaining = reason_ctx.messages.len(),
            "Context hard-truncated (logical marking)"
        );
    }
}

// ─── Constants ─────────────────────────────────────────────────────────

pub const TRUNCATED_TOOL_CALL_NOTICE: &str =
    "Your previous response was truncated and tool calls were discarded. \
     Please try a different approach or break down your work into smaller steps.";

#[cfg(test)]
mod interactive_tool_gate_tests {
    use super::*;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
    use serde_json::json;

    fn assistant_tool_use(id: &str, name: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: json!({}),
            }],
            compacted: false,
        }
    }

    fn user_tool_result(id: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: "result text".to_string(),
                is_error: Some(false),
            }],
            compacted: false,
        }
    }

    #[test]
    fn last_tool_use_name_returns_most_recent() {
        let msgs = vec![
            assistant_tool_use("c1", "read_file"),
            user_tool_result("c1"),
            assistant_tool_use("c2", "ask_user"),
            user_tool_result("c2"),
        ];
        assert_eq!(last_tool_use_name(&msgs), Some("ask_user"));
    }

    #[test]
    fn last_tool_use_name_none_when_no_tool_use() {
        let msgs = vec![
            ChatMessage::user("hi"),
            ChatMessage::assistant("hello"),
        ];
        assert_eq!(last_tool_use_name(&msgs), None);
    }

    #[test]
    fn last_tool_was_interactive_true_for_ask_user() {
        let msgs = vec![
            assistant_tool_use("c1", "ask_user"),
            user_tool_result("c1"),
        ];
        assert!(last_tool_was_interactive(&msgs));
    }

    #[test]
    fn last_tool_was_interactive_true_for_exit_plan_mode() {
        let msgs = vec![
            assistant_tool_use("c1", "exit_plan_mode"),
            user_tool_result("c1"),
        ];
        assert!(last_tool_was_interactive(&msgs));
    }

    #[test]
    fn last_tool_was_interactive_true_for_request_plan_mode_switch() {
        let msgs = vec![
            assistant_tool_use("c1", "request_plan_mode_switch"),
            user_tool_result("c1"),
        ];
        assert!(last_tool_was_interactive(&msgs));
    }

    #[test]
    fn last_tool_was_interactive_false_for_non_interactive_tools() {
        for name in &["read_file", "write_file", "edit", "bash", "grep", "glob", "plan_write", "plan_update"] {
            let msgs = vec![
                assistant_tool_use("c1", name),
                user_tool_result("c1"),
            ];
            assert!(!last_tool_was_interactive(&msgs),
                "non-interactive tool {} should not gate the nudge", name);
        }
    }

    #[test]
    fn last_tool_was_interactive_false_for_empty_history() {
        assert!(!last_tool_was_interactive(&[]));
    }

    #[test]
    fn last_tool_was_interactive_walks_backward_through_recent_messages() {
        // ask_user is older; the most recent ToolUse is read_file → gate should
        // NOT fire (gate only triggers on the LATEST tool, not any in history).
        let msgs = vec![
            assistant_tool_use("c1", "ask_user"),
            user_tool_result("c1"),
            ChatMessage::assistant("ok let me continue"),
            assistant_tool_use("c2", "read_file"),
            user_tool_result("c2"),
        ];
        assert!(!last_tool_was_interactive(&msgs));
    }
}

#[cfg(test)]
mod compaction_safety_tests {
    use super::*;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
    use serde_json::json;

    fn assistant_tool_use(id: &str, name: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::ToolUse {
                id: id.to_string(),
                name: name.to_string(),
                input: json!({}),
            }],
            compacted: false,
        }
    }

    fn user_tool_result(id: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: "result text".to_string(),
                is_error: Some(false),
            }],
            compacted: false,
        }
    }

    fn user_human(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            compacted: false,
        }
    }

    fn assistant_text(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            compacted: false,
        }
    }

    #[test]
    fn test_is_boundary_safe_basic() {
        // split_idx is out of bounds or zero
        assert!(!is_boundary_safe(&[], 0));

        let msgs = vec![
            user_human("hello"),
            assistant_text("hi"),
            user_human("how are you"),
        ];

        // split at index 1: remaining starts with Assistant text -> unsafe
        assert!(!is_boundary_safe(&msgs, 1));

        // split at index 2: remaining starts with User human -> safe
        assert!(is_boundary_safe(&msgs, 2));
    }

    #[test]
    fn test_is_boundary_safe_tool_split() {
        let msgs = vec![
            user_human("run tool"),
            assistant_tool_use("tool-1", "read_file"),
            user_tool_result("tool-1"),
            assistant_text("done with tool"),
            user_human("next question"),
        ];

        // Slicing at index 2 splits "tool-1" tool use (index 1) and tool result (index 2) -> unsafe
        assert!(!is_boundary_safe(&msgs, 2));

        // Slicing at index 3 splits "tool-1" tool use (index 1) and tool result (index 2) -> wait,
        // actually, both tool use (1) and tool result (2) are on the compacted side (< 3).
        // But the first message on the active side (index 3) is an Assistant message -> unsafe due to role
        assert!(!is_boundary_safe(&msgs, 3));

        // Slicing at index 4: compacted side has (0, 1, 2, 3), active side has (4).
        // active starts with User human (4), and no tool split -> safe!
        assert!(is_boundary_safe(&msgs, 4));
    }

    #[test]
    fn test_find_safe_compaction_boundary_adjusts() {
        let msgs = vec![
            user_human("run tool"),
            assistant_tool_use("tool-1", "read_file"),
            user_tool_result("tool-1"),
            assistant_text("done with tool"),
            user_human("next question"),
        ];

        // We desire to split at index 2 (which is unsafe).
        // The nearest safe boundary is index 4 (since it starts with user_human("next question")).
        // Let's verify find_safe_compaction_boundary resolves this to index 4.
        let safe_idx = find_safe_compaction_boundary(&msgs, 2);
        assert_eq!(safe_idx, 4);
    }
}
