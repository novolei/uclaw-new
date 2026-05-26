//! M2-H L6 wire-up — orphan tool-call detection on `Vec<ChatMessage>`.
//!
//! The pilot in `audit.rs` operates on a provider-agnostic
//! `AuditMessage` enum. The actual agent dispatcher hands the LLM
//! `Vec<ChatMessage>` where tool calls live inside `ContentBlock::ToolUse`
//! and tool results inside `ContentBlock::ToolResult` blocks (Anthropic
//! shape). This adapter reuses the same algorithm — collect every
//! resolved `tool_use_id`, then splice synth placeholders for the
//! orphans — but operates directly on uClaw's wire types so the
//! dispatcher doesn't pay a format-conversion round trip on every
//! turn.
//!
//! Placement rules (Anthropic / OpenAI):
//!
//! - `tool_use` blocks live in **assistant** messages.
//! - `tool_result` blocks live in **user** messages.
//! - Each missing result is filled with a synthetic user-role message
//!   carrying a single `tool_result` block keyed on the orphan call id,
//!   placed **immediately after** the assistant message that contains
//!   the orphan call.
//!
//! Why a separate synthetic user message instead of mutating the
//! assistant message in-place: assistant content is the model's
//! output — tampering with it can confuse cache invariants and reasoning
//! traces. A fresh user message is the conventional "I noticed your
//! tool call didn't have a result, here's a placeholder" signal.

use std::collections::HashSet;

use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};

use super::audit::{EnsureStats, OrphanCall, ABORTED_PLACEHOLDER};

/// Scan `messages` for orphan `ToolUse` blocks and splice synthetic
/// `ToolResult` user messages for each.
///
/// Returns the rewritten history plus an [`EnsureStats`] describing
/// what was patched. The input vector is consumed to avoid cloning the
/// (often-large) message stream.
///
/// Idempotent — a second pass over the output is a no-op
/// (`EnsureStats::is_clean()`).
pub fn audit_chat_history(messages: Vec<ChatMessage>) -> (Vec<ChatMessage>, EnsureStats) {
    let mut stats = EnsureStats::default();

    // Pass 1: collect every tool_use_id that already has a matching result.
    let resolved: HashSet<String> = messages
        .iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolResult { tool_use_id, .. } => Some(tool_use_id.clone()),
            _ => None,
        })
        .collect();

    // Pass 2: rebuild message list, inserting synth user messages
    // immediately after any assistant message that holds an orphan call.
    //
    // `original_index` on OrphanCall is the assistant message's position
    // in the input — useful for log correlation. We keep stats per call,
    // not per-message, because one assistant message can hold multiple
    // ToolUse blocks.
    let mut out: Vec<ChatMessage> = Vec::with_capacity(messages.len() + 4);

    for (msg_idx, msg) in messages.into_iter().enumerate() {
        // Collect orphan calls in this message first so we can build
        // ONE synthetic ToolResult-bearing user message that pairs them
        // all (Anthropic actually requires every ToolUse in an assistant
        // message to be paired by ToolResult blocks in the immediately-
        // following user message — so we batch).
        let mut orphan_results: Vec<ContentBlock> = Vec::new();
        if matches!(msg.role, MessageRole::Assistant) {
            for block in &msg.content {
                if let ContentBlock::ToolUse { id, name, .. } = block {
                    if !resolved.contains(id) {
                        stats.orphans_synthesized += 1;
                        stats.orphan_calls.push(OrphanCall {
                            original_index: msg_idx,
                            call_id: id.clone(),
                            tool_name: name.clone(),
                        });
                        orphan_results.push(ContentBlock::ToolResult {
                            tool_use_id: id.clone(),
                            content: ABORTED_PLACEHOLDER.to_string(),
                            is_error: Some(true),
                        });
                    }
                }
            }
        }

        out.push(msg);
        if !orphan_results.is_empty() {
            out.push(ChatMessage {
                role: MessageRole::User,
                content: orphan_results,
                compacted: false,
            });
        }
    }

    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user_text(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }

    fn assistant_text(text: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![ContentBlock::Text {
                text: text.to_string(),
            }],
            compacted: false,
        }
    }

    fn assistant_with_tool_use(id: &str, name: &str) -> ChatMessage {
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

    fn user_with_tool_result(id: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::User,
            content: vec![ContentBlock::ToolResult {
                tool_use_id: id.to_string(),
                content: content.to_string(),
                is_error: None,
            }],
            compacted: false,
        }
    }

    #[test]
    fn empty_history_is_clean() {
        let (out, stats) = audit_chat_history(vec![]);
        assert!(stats.is_clean());
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn well_formed_pair_is_clean() {
        let input = vec![
            user_text("run shell"),
            assistant_with_tool_use("c1", "shell"),
            user_with_tool_result("c1", "ok"),
            assistant_text("done"),
        ];
        let original_count = input.len();
        let (out, stats) = audit_chat_history(input);
        assert!(stats.is_clean());
        assert_eq!(out.len(), original_count);
    }

    #[test]
    fn orphan_gets_synth_user_message_appended_immediately_after() {
        let input = vec![
            user_text("run shell"),
            assistant_with_tool_use("c1", "shell"),
            // No matching ToolResult — agent was cancelled mid-turn
        ];
        let (out, stats) = audit_chat_history(input);
        assert_eq!(stats.orphans_synthesized, 1);
        assert_eq!(stats.orphan_calls[0].call_id, "c1");
        assert_eq!(stats.orphan_calls[0].tool_name, "shell");
        assert_eq!(stats.orphan_calls[0].original_index, 1);

        // Output: user, assistant(ToolUse), synth_user(ToolResult).
        assert_eq!(out.len(), 3);
        assert!(matches!(out[2].role, MessageRole::User));
        match &out[2].content[0] {
            ContentBlock::ToolResult {
                tool_use_id,
                content,
                is_error,
            } => {
                assert_eq!(tool_use_id, "c1");
                assert_eq!(content, ABORTED_PLACEHOLDER);
                assert_eq!(*is_error, Some(true));
            }
            other => panic!("expected synth ToolResult, got {other:?}"),
        }
    }

    #[test]
    fn multiple_tool_uses_in_one_message_batch_into_one_synth_message() {
        let input = vec![assistant_with_two_tool_uses("a", "shell", "b", "read")];
        let (out, stats) = audit_chat_history(input);
        assert_eq!(stats.orphans_synthesized, 2);

        // One assistant message + one batched synth user message.
        assert_eq!(out.len(), 2);
        assert!(matches!(out[1].role, MessageRole::User));
        assert_eq!(out[1].content.len(), 2);
        assert!(matches!(
            &out[1].content[0],
            ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "a"
        ));
        assert!(matches!(
            &out[1].content[1],
            ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == "b"
        ));
    }

    #[test]
    fn second_pass_is_noop() {
        let input = vec![user_text("run"), assistant_with_tool_use("c1", "shell")];
        let (once, _) = audit_chat_history(input);
        let (twice, stats) = audit_chat_history(once.clone());
        assert!(stats.is_clean(), "second pass should find no orphans");
        assert_eq!(once.len(), twice.len());
    }

    #[test]
    fn out_of_order_match_still_resolves() {
        // tool_result appears AFTER an unrelated assistant message.
        let input = vec![
            user_text("run"),
            assistant_with_tool_use("c1", "shell"),
            assistant_text("thinking..."),
            user_with_tool_result("c1", "ok"),
        ];
        let original_count = input.len();
        let (out, stats) = audit_chat_history(input);
        assert!(stats.is_clean());
        assert_eq!(out.len(), original_count);
    }

    #[test]
    fn user_role_tool_result_blocks_dont_get_synth_pairs() {
        // ToolResult in a user message is the resolved side — it should
        // never trigger orphan synthesis (orphans only come from
        // unmatched ToolUse blocks on assistant messages).
        let input = vec![
            user_with_tool_result("phantom", "stale"),
            assistant_text("hi"),
        ];
        let original_count = input.len();
        let (out, stats) = audit_chat_history(input);
        assert!(stats.is_clean());
        assert_eq!(out.len(), original_count);
    }

    // Test helper for multi-tool-use assistant messages.
    fn assistant_with_two_tool_uses(id1: &str, name1: &str, id2: &str, name2: &str) -> ChatMessage {
        ChatMessage {
            role: MessageRole::Assistant,
            content: vec![
                ContentBlock::ToolUse {
                    id: id1.to_string(),
                    name: name1.to_string(),
                    input: json!({}),
                },
                ContentBlock::ToolUse {
                    id: id2.to_string(),
                    name: name2.to_string(),
                    input: json!({}),
                },
            ],
            compacted: false,
        }
    }
}
