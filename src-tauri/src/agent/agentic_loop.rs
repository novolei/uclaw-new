use crate::agent::types::*;
use tracing;

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
                return LoopOutcome::Cancelled;
            }
            LoopSignal::InjectMessage { content } => {
                tracing::debug!("Injecting message into context");
                reason_ctx.messages.push(ChatMessage::user(&content));
            }
            LoopSignal::Continue => {}
        }

        // ── 2. Context compression ───────────────────────────────────
        compress_context_if_needed(reason_ctx, config);

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

        // ── 5. Handle response ───────────────────────────────────────
        match output {
            RespondOutput::Text { text, thinking, thinking_signature, metadata } => {
                // Track token usage and emit events
                if let Some(ref usage) = metadata.usage {
                    reason_ctx.total_input_tokens += usage.input_tokens;
                    reason_ctx.total_output_tokens += usage.output_tokens;
                    delegate.on_usage(usage, reason_ctx).await;
                }

                // Tool intent nudge: LLM talks about using a tool but doesn't actually call one
                if config.enable_tool_intent_nudge
                    && consecutive_tool_intent_nudges < config.max_tool_intent_nudges
                    && !reason_ctx.force_text
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
                                blocks.push(ContentBlock::Thinking { thinking: t.clone() });
                            }
                        }
                        blocks.push(ContentBlock::Text { text: text.clone() });
                        reason_ctx.messages.push(ChatMessage {
                            role: MessageRole::Assistant,
                            content: blocks,
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
                                blocks.push(ContentBlock::Thinking { thinking: t.clone() });
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
fn compress_context_if_needed(reason_ctx: &mut ReasoningContext, config: &AgenticLoopConfig) {
    if config.token_budget == 0 {
        return;
    }

    let estimated_tokens = reason_ctx.estimate_token_count();
    let hard_limit = (config.token_budget as f32 * config.hard_truncation_threshold) as usize;
    let soft_limit = (config.token_budget as f32 * config.compression_threshold) as usize;

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
        soft_compress_context(reason_ctx, config.compression_keep_turns);
    }
}

/// Keep only the last `keep_turns` messages, inserting a summary placeholder.
fn soft_compress_context(reason_ctx: &mut ReasoningContext, keep_turns: usize) {
    let total = reason_ctx.messages.len();
    if total <= keep_turns {
        return;
    }

    let removed_count = total - keep_turns;
    let removed: Vec<ChatMessage> = reason_ctx.messages.drain(..removed_count).collect();

    // Collect a brief summary of what was removed
    let tool_names: Vec<String> = removed
        .iter()
        .flat_map(|m| {
            m.content.iter().filter_map(|b| match b {
                ContentBlock::ToolUse { name, .. } => Some(name.clone()),
                _ => None,
            })
        })
        .collect();

    let summary = if tool_names.is_empty() {
        format!(
            "[Context compressed: {} earlier messages removed to stay within token budget]",
            removed_count
        )
    } else {
        let unique_tools: Vec<String> = {
            let mut seen = std::collections::HashSet::new();
            tool_names
                .into_iter()
                .filter(|n| seen.insert(n.clone()))
                .collect()
        };
        format!(
            "[Context compressed: {} earlier messages removed. Tools used: {}]",
            removed_count,
            unique_tools.join(", ")
        )
    };

    // Prepend summary as a system-style user message
    reason_ctx
        .messages
        .insert(0, ChatMessage::user(&summary));

    tracing::info!(
        removed = removed_count,
        remaining = reason_ctx.messages.len(),
        "Context soft-compressed"
    );
}

/// Remove oldest messages one-by-one until estimated token count is below target.
fn hard_truncate_context(reason_ctx: &mut ReasoningContext, target_tokens: usize) {
    let mut removed = 0;
    while reason_ctx.messages.len() > 2 && reason_ctx.estimate_token_count() > target_tokens {
        reason_ctx.messages.remove(0);
        removed += 1;
    }

    if removed > 0 {
        reason_ctx.messages.insert(
            0,
            ChatMessage::user(&format!(
                "[Context hard-truncated: {} oldest messages removed to prevent overflow]",
                removed
            )),
        );
        tracing::warn!(
            removed,
            remaining = reason_ctx.messages.len(),
            "Context hard-truncated"
        );
    }
}

// ─── Constants ─────────────────────────────────────────────────────────

pub const TRUNCATED_TOOL_CALL_NOTICE: &str =
    "Your previous response was truncated and tool calls were discarded. \
     Please try a different approach or break down your work into smaller steps.";
