use crate::agent::types::*;
use crate::agent::context::{LayeredContextBuilder, LayeredContextConfig};
use tracing;

/// Placeholder string inserted into a fabricated `ToolResult` when a
/// `ToolUse`'s matching result was lost to compaction. Kept `pub(crate)` so
/// rollout-replay tools and the trace UI can match on it.
pub(crate) const COMPACTED_TOOL_RESULT_PLACEHOLDER: &str =
    "[result missing — compacted before next turn]";

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

/// Find the index of the next message at or after `from_idx` whose
/// `compacted` flag is `false`. Returns `None` if no such message exists.
///
/// Used by the pair-repair logic in `purge_orphaned_tool_results` to skip
/// over compacted messages (which remain in the array but are not sent to
/// the model) when locating the user turn that should contain `ToolResult`
/// blocks for an assistant's `ToolUse` blocks.
fn find_next_active_message_idx(messages: &[ChatMessage], from_idx: usize) -> Option<usize> {
    messages
        .iter()
        .enumerate()
        .skip(from_idx)
        .find(|(_, msg)| !msg.compacted)
        .map(|(idx, _)| idx)
}

/// Purges any orphaned ToolResult blocks from active messages where the
/// corresponding ToolUse has been compacted. If a message becomes empty
/// after purging, it is marked as compacted.
///
/// Also inserts placeholder ToolResult blocks for any ToolUse blocks that
/// have no matching ToolResult in the following active User message
/// (Step C — symmetric repair for both pairing directions).
pub fn purge_orphaned_tool_results(messages: &mut Vec<ChatMessage>) {
    // ─── Step A: collect active tool_use IDs ────────────────────────
    let mut active_tool_call_ids = std::collections::HashSet::new();
    for msg in messages.iter() {
        if !msg.compacted {
            for block in &msg.content {
                if let ContentBlock::ToolUse { id, .. } = block {
                    active_tool_call_ids.insert(id.clone());
                }
            }
        }
    }

    // ─── Step B: drop orphan ToolResult blocks ───────────────────────
    for msg in messages.iter_mut() {
        if !msg.compacted {
            msg.content.retain(|block| {
                if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                    active_tool_call_ids.contains(tool_use_id)
                } else {
                    true
                }
            });

            if msg.content.is_empty() {
                msg.compacted = true;
            }
        }
    }

    // ─── Step C: insert placeholders for orphan ToolUse blocks ───────
    repair_orphan_tool_use_placeholders(messages);
}

/// For each non-compacted Assistant message that has ToolUse blocks,
/// ensure the next non-compacted User message contains a matching
/// ToolResult for each such id. Insert placeholder ToolResult blocks
/// where they are missing.
///
/// If no active User message follows the Assistant, a synthetic User
/// message is inserted at `i + 1` with the placeholder(s) as its only
/// content.
///
/// This is idempotent: on a second call the placeholder is already
/// present, so the loop skips it.
fn repair_orphan_tool_use_placeholders(messages: &mut Vec<ChatMessage>) {
    // Collect (assistant_idx, [orphan_ids]) first to avoid holding a
    // shared borrow while mutating the Vec.
    let mut to_repair: Vec<(usize, Vec<String>)> = Vec::new();

    for (i, msg) in messages.iter().enumerate() {
        if msg.compacted || msg.role != MessageRole::Assistant {
            continue;
        }
        let tool_use_ids: Vec<String> = msg
            .content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolUse { id, .. } = b {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();
        if tool_use_ids.is_empty() {
            continue;
        }

        // Check which ids already have a matching ToolResult in the next
        // active User message.
        let already_matched: std::collections::HashSet<String> =
            match find_next_active_message_idx(messages, i + 1) {
                Some(idx) if messages[idx].role == MessageRole::User => messages[idx]
                    .content
                    .iter()
                    .filter_map(|b| {
                        if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                            Some(tool_use_id.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
                _ => std::collections::HashSet::new(),
            };

        let orphans: Vec<String> = tool_use_ids
            .into_iter()
            .filter(|id| !already_matched.contains(id))
            .collect();
        if !orphans.is_empty() {
            to_repair.push((i, orphans));
        }
    }

    // Apply repairs in reverse index order so insertions don't shift
    // unprocessed assistant indices.
    for (i, orphan_ids) in to_repair.into_iter().rev() {
        let placeholders: Vec<ContentBlock> = orphan_ids
            .into_iter()
            .map(|id| ContentBlock::ToolResult {
                tool_use_id: id,
                content: COMPACTED_TOOL_RESULT_PLACEHOLDER.to_string(),
                is_error: Some(false),
            })
            .collect();

        let next_active = find_next_active_message_idx(messages, i + 1);
        match next_active {
            Some(idx) if messages[idx].role == MessageRole::User => {
                // Append placeholders to the existing active User message.
                messages[idx].content.extend(placeholders);
            }
            _ => {
                // No active User message follows — synthesize one at i+1.
                let new_msg = ChatMessage {
                    role: MessageRole::User,
                    content: placeholders,
                    compacted: false,
                };
                messages.insert(i + 1, new_msg);
            }
        }
    }
}

#[cfg(test)]
mod find_next_active_tests {
    use super::*;
    use crate::agent::types::{ChatMessage, MessageRole};

    #[test]
    fn test_find_next_active_skips_compacted() {
        let mut msgs = vec![
            ChatMessage::user("u1"),
            ChatMessage::assistant("a1"),
            ChatMessage::user("u2"),
        ];
        msgs[0].compacted = true;
        msgs[1].compacted = true;
        assert_eq!(find_next_active_message_idx(&msgs, 0), Some(2));
    }

    #[test]
    fn test_find_next_active_none_at_end() {
        let mut msgs = vec![ChatMessage::user("u1")];
        msgs[0].compacted = true;
        assert_eq!(find_next_active_message_idx(&msgs, 0), None);
    }

    #[test]
    fn test_find_next_active_returns_first_when_none_compacted() {
        let msgs = vec![
            ChatMessage::user("u1"),
            ChatMessage::assistant("a1"),
        ];
        assert_eq!(find_next_active_message_idx(&msgs, 0), Some(0));
        assert_eq!(find_next_active_message_idx(&msgs, 1), Some(1));
        assert_eq!(find_next_active_message_idx(&msgs, 2), None);
    }
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
    let soft_limit = (effective_budget as f32 * 0.75) as usize;

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

    purge_orphaned_tool_results(&mut reason_ctx.messages);

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

    // —— Pi Sprint 2:在标记 compacted 之前,用 LIVE 消息捕获切点 + 待摘要切片 ——
    let cut = crate::agent::compaction::find_compaction_cut_point(&reason_ctx.messages, split_idx);
    let main_end = if cut.is_split_turn { cut.turn_start_index.unwrap_or(split_idx) } else { split_idx };
    let main_slice: Vec<ChatMessage> =
        reason_ctx.messages[..main_end].iter().filter(|m| !m.compacted).cloned().collect();
    let split_prefix: Vec<ChatMessage> = if cut.is_split_turn {
        cut.turn_start_index
            .map(|ts| reason_ctx.messages[ts..split_idx].iter().filter(|m| !m.compacted).cloned().collect::<Vec<_>>())
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    // Logical marking of messages before split_idx as compacted
    for i in 0..split_idx {
        reason_ctx.messages[i].compacted = true;
    }

    purge_orphaned_tool_results(&mut reason_ctx.messages);

    // 增量 vs 首次。None → extractive 兜底(保持现有行为)。
    let main_fold_opt = if let Some(prior) = reason_ctx.compaction_state.previous_fold.clone() {
        tracing::info!(
            summary_type = "incremental_fold",
            removed = removed_count,
            compactions_done = reason_ctx.compaction_state.compactions_done,
            "Using incremental StructuredFold update for context compaction"
        );
        delegate.update_fold_incremental(&prior, &main_slice).await
    } else {
        tracing::info!(
            summary_type = "llm_fold",
            removed = removed_count,
            "Using LLM-generated StructuredFold for context compaction (first compaction)"
        );
        delegate.summarize_to_fold(&main_slice).await
    };
    let mut fold = main_fold_opt
        .unwrap_or_else(|| {
            tracing::info!(
                summary_type = "fallback_fold",
                removed = removed_count,
                "LLM fold summarization failed/returned None, falling back to extractive fold"
            );
            crate::agent::compact::summarize::extractive_fallback_fold(&main_slice)
        });

    if !split_prefix.is_empty() {
        let prefix_fold = delegate
            .summarize_to_fold(&split_prefix)
            .await
            .unwrap_or_else(|| crate::agent::compact::summarize::extractive_fallback_fold(&split_prefix));
        let mut caps = fold.micro_capsules.clone();
        caps.push(crate::agent::compact::fold::MicroCapsule {
            turn_index: caps.len(),
            user_query: "Turn Context (split turn)".to_string(),
            agent_outcome: prefix_fold.to_markdown(),
        });
        fold = fold.with_micro_capsules(caps);
    }

    // 更新压缩状态 (下次走增量)。存储的是「主摘要 fold」(含 split capsule),
    // 不含 file_ops 合并 (file_ops 在下面按现有逻辑合并并用于注入)。
    reason_ctx.compaction_state.previous_fold = Some(fold.clone());
    reason_ctx.compaction_state.compactions_done += 1;

    // Pi Sprint 1 — merge accumulated file ops into the fold so paths
    // survive this compaction cycle. reason_ctx.file_ops already holds the
    // cumulative set for this session (dispatcher appends on each successful
    // file-touching tool call), so we attach it to the fold verbatim.
    // After compaction the same set remains in reason_ctx.file_ops so the
    // next compression window continues accumulating from the right baseline.
    let fold = fold.with_file_ops(reason_ctx.file_ops.clone());

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

    purge_orphaned_tool_results(&mut reason_ctx.messages);

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

    #[test]
    fn test_purge_orphaned_tool_results_basic_purged() {
        // Case 1: ToolUse is compacted (or not in any active message) but ToolResult remains -> purged
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                compacted: true,
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    content: "file content".to_string(),
                    is_error: Some(false),
                }],
                compacted: false,
            },
        ];
        purge_orphaned_tool_results(&mut messages);
        assert!(messages[1].compacted);
        assert!(messages[1].content.is_empty());
    }

    #[test]
    fn test_purge_orphaned_tool_results_active_kept() {
        // Case 2: ToolUse and ToolResult both active -> NOT deleted
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tool-1".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                compacted: false,
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "tool-1".to_string(),
                    content: "file content".to_string(),
                    is_error: Some(false),
                }],
                compacted: false,
            },
        ];
        purge_orphaned_tool_results(&mut messages);
        assert!(!messages[0].compacted);
        assert!(!messages[1].compacted);
        assert_eq!(messages[1].content.len(), 1);
    }

    #[test]
    fn test_purge_orphaned_tool_results_empty_message_marked_compacted() {
        // Case 3: Purging causing empty message -> message marked compacted
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::User,
                content: vec![ContentBlock::ToolResult {
                    tool_use_id: "orphan-1".to_string(),
                    content: "result".to_string(),
                    is_error: Some(false),
                }],
                compacted: false,
            },
        ];
        purge_orphaned_tool_results(&mut messages);
        assert!(messages[0].compacted);
        assert!(messages[0].content.is_empty());
    }

    #[test]
    fn test_purge_orphaned_tool_results_multi_mixed() {
        // Case 5: Multi-tool calls, multi-results, out-of-order, cross-message boundaries
        let mut messages = vec![
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![
                    ContentBlock::ToolUse {
                        id: "tool-compacted".to_string(),
                        name: "web_search".to_string(),
                        input: json!({}),
                    },
                    ContentBlock::ToolUse {
                        id: "tool-active".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                    },
                ],
                compacted: true,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tool-compacted".to_string(),
                    name: "web_search".to_string(),
                    input: json!({}),
                }],
                compacted: true,
            },
            ChatMessage {
                role: MessageRole::Assistant,
                content: vec![ContentBlock::ToolUse {
                    id: "tool-active".to_string(),
                    name: "read_file".to_string(),
                    input: json!({}),
                }],
                compacted: false,
            },
            ChatMessage {
                role: MessageRole::User,
                content: vec![
                    ContentBlock::Text { text: "Here is a prefix text".to_string() },
                    ContentBlock::ToolResult {
                        tool_use_id: "tool-compacted".to_string(),
                        content: "compacted result".to_string(),
                        is_error: Some(false),
                    },
                    ContentBlock::ToolResult {
                        tool_use_id: "tool-active".to_string(),
                        content: "active result".to_string(),
                        is_error: Some(false),
                    },
                ],
                compacted: false,
            },
        ];
        purge_orphaned_tool_results(&mut messages);
        assert!(!messages[3].compacted);
        assert_eq!(messages[3].content.len(), 2);
        match &messages[3].content[0] {
            ContentBlock::Text { text } => assert_eq!(text, "Here is a prefix text"),
            _ => panic!("Expected text block"),
        }
        match &messages[3].content[1] {
            ContentBlock::ToolResult { tool_use_id, .. } => assert_eq!(tool_use_id, "tool-active"),
            _ => panic!("Expected tool result block"),
        }
    }

    #[test]
    fn test_purge_orphaned_tool_results_all_compact_paths() {
        // Case 4: soft_compress_context / hard_truncate_context / force_compact_sync three paths do not produce orphaned ToolResults
        let mut reason_ctx = ReasoningContext {
            messages: vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: "Hello".to_string() }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentBlock::ToolUse {
                        id: "tool-1".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                    }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: "tool-1".to_string(),
                        content: "content".to_string(),
                        is_error: Some(false),
                    }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: "Keep this".to_string() }],
                    compacted: false,
                },
            ],
            system_prompt: "system".to_string(),
            force_text: false,
            thread_state: ThreadState::Completed,
            total_input_tokens: 0,
            total_output_tokens: 0,
            mutations_since_last_plan_done: 0,
            mutation_challenges_issued: 0,
            consecutive_length_truncations: 0,
            partial_code_buffer: None,
            consecutive_plan_guard_nudges: 0,
            cancellation_token: None,
            file_ops: Default::default(),
            compaction_state: Default::default(),
        };

        // If we force compact with keep_turns = 1, it will compact older messages.
        // It will compact Hello, and tool-1 ToolUse, but if tool-1 ToolResult is kept active,
        // it will become orphaned. Calling force_compact_sync should heal this!
        force_compact_sync(&mut reason_ctx, 1);

        // Let's check that tool-1 ToolResult is either compacted or removed, so no active orphaned tool result exists
        for msg in &reason_ctx.messages {
            if !msg.compacted {
                for block in &msg.content {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                        assert_ne!(tool_use_id, "tool-1", "Should not have active tool-1 ToolResult");
                    }
                }
            }
        }

        // Let's test hard_truncate_context
        let mut reason_ctx_hard = ReasoningContext {
            messages: vec![
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: "Hello".to_string() }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::Assistant,
                    content: vec![ContentBlock::ToolUse {
                        id: "tool-2".to_string(),
                        name: "read_file".to_string(),
                        input: json!({}),
                    }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::ToolResult {
                        tool_use_id: "tool-2".to_string(),
                        content: "content".to_string(),
                        is_error: Some(false),
                    }],
                    compacted: false,
                },
                ChatMessage {
                    role: MessageRole::User,
                    content: vec![ContentBlock::Text { text: "Keep this".to_string() }],
                    compacted: false,
                },
            ],
            system_prompt: "system".to_string(),
            force_text: false,
            thread_state: ThreadState::Completed,
            total_input_tokens: 0,
            total_output_tokens: 0,
            mutations_since_last_plan_done: 0,
            mutation_challenges_issued: 0,
            consecutive_length_truncations: 0,
            partial_code_buffer: None,
            consecutive_plan_guard_nudges: 0,
            cancellation_token: None,
            file_ops: Default::default(),
            compaction_state: Default::default(),
        };

        // Truncate to a target token size of 5 tokens, which will force logical marking of older messages.
        hard_truncate_context(&mut reason_ctx_hard, 5);

        // Check that tool-2 ToolResult is not orphaned (either compacted or removed)
        for msg in &reason_ctx_hard.messages {
            if !msg.compacted {
                for block in &msg.content {
                    if let ContentBlock::ToolResult { tool_use_id, .. } = block {
                        assert_ne!(tool_use_id, "tool-2", "Should not have active tool-2 ToolResult");
                    }
                }
            }
        }
    }

    // ── Test-only constructors for Step C tests ──────────────────────────
    // These helpers exist only in the test module; NOT added to production code.

    /// Build an Assistant message with a single ToolUse block.
    fn assistant_with_tool_use_test(
        _text: &str,
        id: &str,
        name: &str,
    ) -> ChatMessage {
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

    /// Build a User message with a single ToolResult block.
    fn user_with_tool_result_test(tool_use_id: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: tool_use_id.to_string(),
                content: content.to_string(),
                is_error: Some(false),
            }],
            compacted: false,
        }
    }

    /// Count placeholder ToolResult blocks across all active messages.
    fn count_placeholders(messages: &[ChatMessage]) -> usize {
        messages
            .iter()
            .filter(|m| !m.compacted)
            .flat_map(|m| m.content.iter())
            .filter(|b| {
                matches!(b, ContentBlock::ToolResult { content, .. }
                    if content.contains("result missing"))
            })
            .count()
    }

    // ── 5 new Step C tests ────────────────────────────────────────────────

    #[test]
    fn test_purge_orphaned_tool_results_inserts_placeholder_for_orphan_tool_use() {
        // Assistant has tool_use["call_A"], next user msg has no matching ToolResult.
        // Repair must append placeholder to the user message.
        let mut messages = vec![
            assistant_with_tool_use_test("calling tool", "call_A", "ls"),
            ChatMessage::user("ack but no tool_result yet"),
        ];
        purge_orphaned_tool_results(&mut messages);

        let user_results: Vec<_> = messages[1]
            .content
            .iter()
            .filter_map(|b| {
                if let ContentBlock::ToolResult { tool_use_id, content, .. } = b {
                    Some((tool_use_id.clone(), content.clone()))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(user_results.len(), 1, "exactly one placeholder expected");
        assert_eq!(user_results[0].0, "call_A");
        assert!(
            user_results[0].1.contains("result missing"),
            "placeholder content should mention 'result missing'"
        );
    }

    #[test]
    fn test_purge_orphaned_tool_results_inserts_synthesized_user_msg_when_missing() {
        // Assistant has tool_use, no following user message at all.
        // Repair must synthesize a User message at i+1.
        let mut messages = vec![
            ChatMessage::user("initial"),
            assistant_with_tool_use_test("done", "call_X", "ls"),
        ];
        purge_orphaned_tool_results(&mut messages);
        assert_eq!(messages.len(), 3, "a synthetic User message should have been inserted");
        assert_eq!(messages[2].role, MessageRole::User);
        let placeholder_id = messages[2].content.iter().find_map(|b| {
            if let ContentBlock::ToolResult { tool_use_id, .. } = b {
                Some(tool_use_id.clone())
            } else {
                None
            }
        });
        assert_eq!(placeholder_id, Some("call_X".into()));
    }

    #[test]
    fn test_purge_orphaned_tool_results_idempotent() {
        // Running the repair twice on the same broken history should produce
        // exactly the same number of placeholders (second call is a no-op).
        let mut messages = vec![
            assistant_with_tool_use_test("done", "call_Y", "ls"),
            ChatMessage::user("user ack"),
        ];
        purge_orphaned_tool_results(&mut messages);
        let count_after_first = count_placeholders(&messages);
        let len_after_first = messages.len();

        purge_orphaned_tool_results(&mut messages);
        let count_after_second = count_placeholders(&messages);
        let len_after_second = messages.len();

        assert_eq!(count_after_first, count_after_second, "placeholder count must be stable");
        assert_eq!(len_after_first, len_after_second, "message count must be stable");
    }

    #[test]
    fn test_purge_orphaned_tool_results_mixed_orphan_directions() {
        // Direction 1: tool_use survived, tool_result lost → placeholder inserted.
        // Direction 2: tool_result survived, tool_use compacted → orphan ToolResult dropped.
        let mut messages = vec![
            assistant_with_tool_use_test("a1", "call_lost", "ls"),
            ChatMessage::user("u1"), // no ToolResult for call_lost
            user_with_tool_result_test("call_phantom", "stale output"),
        ];
        // All three are active (compacted = false by construction).

        purge_orphaned_tool_results(&mut messages);

        // Direction 1: placeholder inserted in messages[1]
        assert!(
            messages[1].content.iter().any(|b| matches!(
                b,
                ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_lost"
            )),
            "placeholder for call_lost should be in messages[1]"
        );

        // Direction 2: stale ToolResult for call_phantom must be removed
        // (call_phantom has no active ToolUse — Step B drops it).
        assert!(
            !messages[2].content.iter().any(|b| matches!(
                b,
                ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_phantom"
            )),
            "orphan ToolResult for call_phantom should have been dropped"
        );
    }

    #[test]
    fn test_purge_orphaned_tool_results_respects_compacted_boundary() {
        // The tool_use is in a COMPACTED message → its id is NOT in the active set.
        // Step B should drop the orphan ToolResult.
        // Step C must NOT insert a new placeholder (the ToolUse is not active).
        let mut messages = vec![
            assistant_with_tool_use_test("compacted assistant", "call_C", "ls"),
            user_with_tool_result_test("call_C", "result"),
            ChatMessage::user("active follow-up"),
        ];
        messages[0].compacted = true; // tool_use is in a compacted message

        purge_orphaned_tool_results(&mut messages);

        // Step B: orphan ToolResult for call_C must be removed.
        assert!(
            !messages[1].content.iter().any(|b| matches!(
                b,
                ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "call_C"
            )),
            "orphan ToolResult for compacted call_C should have been dropped by Step B"
        );

        // Step C: no placeholder should have been inserted (call_C is not active).
        assert_eq!(
            count_placeholders(&messages),
            0,
            "Step C must not insert a placeholder for a compacted ToolUse"
        );
    }
}

// ── Pi Sprint 2 integration tests: iterative compaction path selection ────────

#[cfg(test)]
mod pi_sprint2_compaction_tests {
    use super::*;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole, ThreadState};
    use crate::agent::compact::StructuredFold;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            compacted: false,
        }
    }

    fn assistant_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text { text: text.to_string() }],
            compacted: false,
        }
    }

    /// Minimal mock delegate that counts calls to summarize_to_fold vs
    /// update_fold_incremental, and always returns a minimal StructuredFold.
    /// Also records the length of the slice passed to summarize_to_fold on
    /// the FIRST call so tests can assert that live (non-empty) messages are
    /// received rather than the post-mark empty slice.
    struct CountingDelegate {
        full_calls: Arc<AtomicUsize>,
        incremental_calls: Arc<AtomicUsize>,
        first_summarize_slice_len: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl LoopDelegate for CountingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(
            &self,
            _reason_ctx: &mut ReasoningContext,
            _iteration: usize,
        ) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            _reason_ctx: &mut ReasoningContext,
            _iteration: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            Err(crate::error::Error::Internal("not used in test".to_string()))
        }
        async fn handle_text_response(
            &self,
            _text: &str,
            _metadata: ResponseMetadata,
            _reason_ctx: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }
        async fn execute_tool_calls(
            &self,
            _tool_calls: Vec<ToolCall>,
            _reason_ctx: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }

        async fn summarize_to_fold(
            &self,
            messages: &[ChatMessage],
        ) -> Option<StructuredFold> {
            // Record the slice length on the first call only.
            if self.full_calls.fetch_add(1, Ordering::SeqCst) == 0 {
                self.first_summarize_slice_len.store(messages.len(), Ordering::SeqCst);
            }
            Some(StructuredFold::default())
        }

        async fn update_fold_incremental(
            &self,
            _prior_fold: &StructuredFold,
            _new_messages: &[ChatMessage],
        ) -> Option<StructuredFold> {
            self.incremental_calls.fetch_add(1, Ordering::SeqCst);
            Some(StructuredFold::default())
        }
    }

    fn make_reason_ctx(n_messages: usize) -> ReasoningContext {
        let mut messages = Vec::new();
        for i in 0..n_messages {
            messages.push(user_msg(&format!("user message {}", i)));
            messages.push(assistant_msg(&format!("assistant reply {}", i)));
        }
        ReasoningContext {
            messages,
            system_prompt: "system".to_string(),
            force_text: false,
            thread_state: ThreadState::Completed,
            total_input_tokens: 0,
            total_output_tokens: 0,
            mutations_since_last_plan_done: 0,
            mutation_challenges_issued: 0,
            consecutive_length_truncations: 0,
            partial_code_buffer: None,
            consecutive_plan_guard_nudges: 0,
            cancellation_token: None,
            file_ops: Default::default(),
            compaction_state: Default::default(),
        }
    }

    /// After the first soft_compress_context call:
    ///   - previous_fold must be Some (incremental base is stored)
    ///   - compactions_done must be 1
    ///   - summarize_to_fold (full path) was called (previous_fold was None)
    ///   - update_fold_incremental was NOT called
    #[tokio::test]
    async fn first_compaction_uses_full_path_and_stores_fold() {
        let full_calls = Arc::new(AtomicUsize::new(0));
        let incr_calls = Arc::new(AtomicUsize::new(0));
        let delegate = CountingDelegate {
            full_calls: full_calls.clone(),
            incremental_calls: incr_calls.clone(),
            first_summarize_slice_len: Arc::new(AtomicUsize::new(0)),
        };

        // 20 messages + keep_turns=2 → soft_compress_context will trigger.
        let mut ctx = make_reason_ctx(10); // 20 messages
        soft_compress_context(&mut ctx, 2, 8192, &delegate).await;

        assert!(
            ctx.compaction_state.previous_fold.is_some(),
            "previous_fold must be set after first compaction"
        );
        assert_eq!(
            ctx.compaction_state.compactions_done, 1,
            "compactions_done must be 1 after first compaction"
        );
        assert!(
            full_calls.load(Ordering::SeqCst) >= 1,
            "summarize_to_fold (full path) must have been called on first compaction"
        );
        assert_eq!(
            incr_calls.load(Ordering::SeqCst), 0,
            "update_fold_incremental must NOT be called on first compaction"
        );
    }

    /// After the second soft_compress_context call (previous_fold already set):
    ///   - update_fold_incremental is called (incremental path)
    ///   - compactions_done becomes 2
    ///   - summarize_to_fold call count does NOT increase beyond what the first round used
    #[tokio::test]
    async fn second_compaction_uses_incremental_path() {
        let full_calls = Arc::new(AtomicUsize::new(0));
        let incr_calls = Arc::new(AtomicUsize::new(0));
        let delegate = CountingDelegate {
            full_calls: full_calls.clone(),
            incremental_calls: incr_calls.clone(),
            first_summarize_slice_len: Arc::new(AtomicUsize::new(0)),
        };

        let mut ctx = make_reason_ctx(10); // 20 messages

        // First compaction — establishes previous_fold.
        soft_compress_context(&mut ctx, 2, 8192, &delegate).await;
        let full_after_first = full_calls.load(Ordering::SeqCst);

        // Add more messages so there's enough active content for a second compaction.
        for i in 0..10 {
            ctx.messages.push(user_msg(&format!("new user {}", i)));
            ctx.messages.push(assistant_msg(&format!("new reply {}", i)));
        }

        // Second compaction — previous_fold is Some → incremental path.
        soft_compress_context(&mut ctx, 2, 8192, &delegate).await;

        assert_eq!(
            ctx.compaction_state.compactions_done, 2,
            "compactions_done must be 2 after second compaction"
        );
        assert!(
            incr_calls.load(Ordering::SeqCst) >= 1,
            "update_fold_incremental must have been called on second compaction"
        );
        assert_eq!(
            full_calls.load(Ordering::SeqCst), full_after_first,
            "summarize_to_fold (full path) must NOT be called again on incremental compaction"
        );
    }

    /// Regression guard: the slice passed to summarize_to_fold on the FIRST
    /// compaction must contain live (non-compacted) messages — i.e. slices must
    /// be captured BEFORE the marking loop, not after.
    ///
    /// Bug in commit 15de5cdc: cut + slices were computed after marking, so
    /// `main_slice` was always empty (all messages were already compacted).
    #[tokio::test]
    async fn first_compaction_summarize_receives_non_empty_live_slice() {
        let first_slice_len = Arc::new(AtomicUsize::new(0));
        let delegate = CountingDelegate {
            full_calls: Arc::new(AtomicUsize::new(0)),
            incremental_calls: Arc::new(AtomicUsize::new(0)),
            first_summarize_slice_len: first_slice_len.clone(),
        };

        // 20 messages (10 pairs), keep 2 → compacts 16+ messages.
        // summarize_to_fold must receive the live pre-mark slice (> 0 messages).
        let mut ctx = make_reason_ctx(10); // 20 messages
        soft_compress_context(&mut ctx, 2, 8192, &delegate).await;

        let received = first_slice_len.load(Ordering::SeqCst);
        assert!(
            received > 0,
            "summarize_to_fold must receive a non-empty live slice on first compaction (got {}); \
             slice was likely captured after marking (ordering bug)",
            received
        );
    }
}
