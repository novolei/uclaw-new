// SPDX-License-Identifier: Apache-2.0
//! Context continuity P2 — batch clearing of old tool results (spec §6.4).
//!
//! When the request nears the compaction threshold, the results of every
//! tool-call group except the most recent `keep_groups` are replaced in the
//! model-facing history by a one-line placeholder starting with
//! [`CLEARED_PREFIX`]. A "group" is one assistant message carrying tool calls
//! plus all of its results. Clearing runs only when it frees at least
//! `floor_tokens` (so each prompt-cache invalidation is worth it), only on
//! already-persisted messages (the current iteration's results are never
//! touched) and never twice on the same result.
//!
//! The cleared version is stored in the transcript's overlay column
//! (`agent_transcript.cleared_json`, see `transcript::set_cleared`); the
//! original `message_json` is never rewritten. Every later replay reads the
//! overlay, so the cleared prefix stays byte-stable across turns. A rewind
//! does not undo clearing: kept old results stay placeholders and the model
//! re-reads the file if it needs the content.

use std::collections::{HashMap, HashSet};

use uclaw_message_types::estimate_tokens;

use crate::agent::types::{ChatMessage, ContentBlock, LoopDelegate, ReasoningContext};

/// Fixed placeholder prefix (the frontend matches on it).
pub const CLEARED_PREFIX: &str = "[已清理的工具结果：";

/// Characters of a command / generic argument quoted in a placeholder.
const KEY_ARG_CHARS: usize = 60;

/// What a clearing pass replaces: `indices` are the messages (indices into
/// the history, all `< persisted_len`) holding at least one cleared result;
/// `tool_call_ids` the cleared results; `freed_tokens` the estimated saving
/// (original result tokens minus placeholder tokens).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClearPlan {
    pub indices: Vec<usize>,
    pub tool_call_ids: Vec<String>,
    pub freed_tokens: usize,
}

/// `tool_use_id → (tool name, arguments)` for every tool call in `msgs`.
fn tool_calls(msgs: &[ChatMessage]) -> HashMap<&str, (&str, &serde_json::Value)> {
    msgs.iter()
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, name, input } => Some((id.as_str(), (name.as_str(), input))),
            _ => None,
        })
        .collect()
}

/// Tool-call ids of the most recent `keep_groups` groups (assistant messages
/// carrying tool calls) in `msgs`.
fn protected_ids(msgs: &[ChatMessage], keep_groups: usize) -> HashSet<&str> {
    msgs.iter()
        .rev()
        .filter(|m| m.content.iter().any(|b| matches!(b, ContentBlock::ToolUse { .. })))
        .take(keep_groups)
        .flat_map(|m| m.content.iter())
        .filter_map(|b| match b {
            ContentBlock::ToolUse { id, .. } => Some(id.as_str()),
            _ => None,
        })
        .collect()
}

/// Select the results to clear (pure). Considers only non-`compacted`
/// messages before `persisted_len`; skips the most recent `keep_groups`
/// groups, results already starting with [`CLEARED_PREFIX`], results whose
/// call is unknown, and results the placeholder would not shrink. `None`
/// when the total saving is below `floor_tokens` (or nothing qualifies).
pub fn plan_clearing(
    msgs: &[ChatMessage],
    persisted_len: usize,
    keep_groups: usize,
    floor_tokens: usize,
) -> Option<ClearPlan> {
    let calls = tool_calls(msgs);
    let protected = protected_ids(msgs, keep_groups);
    let mut plan = ClearPlan { indices: Vec::new(), tool_call_ids: Vec::new(), freed_tokens: 0 };
    for (i, m) in msgs.iter().enumerate().take(persisted_len.min(msgs.len())) {
        if m.compacted {
            continue;
        }
        let mut hit = false;
        for b in &m.content {
            let ContentBlock::ToolResult { tool_use_id, content, .. } = b else { continue };
            if protected.contains(tool_use_id.as_str()) || content.starts_with(CLEARED_PREFIX) {
                continue;
            }
            let Some((name, args)) = calls.get(tool_use_id.as_str()) else { continue };
            let original = estimate_tokens(content) as usize;
            let saved = original.saturating_sub(estimate_tokens(&placeholder(name, args, original)) as usize);
            if saved == 0 {
                continue;
            }
            plan.freed_tokens += saved;
            plan.tool_call_ids.push(tool_use_id.clone());
            hit = true;
        }
        if hit {
            plan.indices.push(i);
        }
    }
    (!plan.tool_call_ids.is_empty() && plan.freed_tokens >= floor_tokens).then_some(plan)
}

/// First `max` chars of `s` on one line, `…` when cut.
fn one_line_prefix(s: &str, max: usize) -> String {
    let flat: String = s.split_whitespace().collect::<Vec<_>>().join(" ");
    match flat.char_indices().nth(max) {
        Some((cut, _)) => format!("{}…", &flat[..cut]),
        None => flat,
    }
}

fn str_arg<'a>(args: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    args.get(key).and_then(|v| v.as_str()).filter(|s| !s.is_empty())
}

/// The traceable key arguments of a call: `read_file` → `path 第 a–b 行`;
/// `grep` / `glob` → `"pattern" 于 path`; `bash` → the command's first 60
/// chars; others → the first string argument's first 60 chars.
fn key_args(tool_name: &str, args: &serde_json::Value) -> String {
    match tool_name {
        "read_file" => match str_arg(args, "path") {
            Some(path) => format!("{path} {}", crate::agent::file_events::read_range(args)),
            None => String::new(),
        },
        "grep" | "glob" => {
            let pattern = str_arg(args, "pattern").map(|p| format!("\"{p}\""));
            match (pattern, str_arg(args, "path")) {
                (Some(p), Some(path)) => format!("{p} 于 {path}"),
                (Some(p), None) => p,
                (None, Some(path)) => path.to_string(),
                (None, None) => String::new(),
            }
        }
        "bash" => str_arg(args, "command").map(|c| one_line_prefix(c, KEY_ARG_CHARS)).unwrap_or_default(),
        _ => args
            .as_object()
            .and_then(|o| o.values().find_map(|v| v.as_str().filter(|s| !s.is_empty())))
            .map(|s| one_line_prefix(s, KEY_ARG_CHARS))
            .unwrap_or_default(),
    }
}

/// One-line stand-in for a cleared result, e.g.
/// `[已清理的工具结果：read_file src/foo.rs 第 1–200 行（约 3.1K token）——如需内容请重新读取]`.
pub fn placeholder(tool_name: &str, args: &serde_json::Value, original_tokens: usize) -> String {
    let key = key_args(tool_name, args);
    let label = if key.is_empty() { tool_name.to_string() } else { format!("{tool_name} {key}") };
    format!(
        "{CLEARED_PREFIX}{label}（约 {:.1}K token）——如需内容请重新读取]",
        original_tokens as f64 / 1000.0
    )
}

/// Replace the results named by `plan` (in `msgs[plan.indices]`) with their
/// placeholders. Results already cleared are left alone (idempotent).
pub fn apply_clearing(msgs: &mut [ChatMessage], plan: &ClearPlan) {
    let ids: HashSet<&str> = plan.tool_call_ids.iter().map(String::as_str).collect();
    let placeholders: HashMap<usize, Vec<(usize, String)>> = {
        let calls = tool_calls(msgs);
        plan.indices
            .iter()
            .filter_map(|&i| msgs.get(i).map(|m| (i, m)))
            .map(|(i, m)| {
                let blocks = m
                    .content
                    .iter()
                    .enumerate()
                    .filter_map(|(j, b)| match b {
                        ContentBlock::ToolResult { tool_use_id, content, .. }
                            if ids.contains(tool_use_id.as_str()) && !content.starts_with(CLEARED_PREFIX) =>
                        {
                            let (name, args) = calls.get(tool_use_id.as_str())?;
                            Some((j, placeholder(name, args, estimate_tokens(content) as usize)))
                        }
                        _ => None,
                    })
                    .collect();
                (i, blocks)
            })
            .collect()
    };
    for (i, blocks) in placeholders {
        for (j, text) in blocks {
            if let ContentBlock::ToolResult { content, .. } = &mut msgs[i].content[j] {
                *content = text;
            }
        }
    }
}

/// Loop step: flush, plan with `settings`, apply in memory and persist the
/// cleared messages through the delegate (`persist_cleared`, which writes
/// the transcript overlay and notifies the UI). A persist failure restores
/// the in-memory messages (the history must keep matching the transcript).
/// Returns the freed tokens (0 = nothing cleared).
pub(crate) async fn clear_old_tool_results(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    settings: &crate::memubot_config::AgentContextSettings,
) -> usize {
    crate::agent::transcript_history::flush_transcript(delegate, reason_ctx).await;
    let Some(plan) = plan_clearing(
        &reason_ctx.messages,
        reason_ctx.transcript_persisted_len,
        settings.keep_tool_result_groups as usize,
        settings.tool_clear_floor_tokens as usize,
    ) else {
        return 0;
    };
    let originals: Vec<(usize, ChatMessage)> =
        plan.indices.iter().map(|&i| (i, reason_ctx.messages[i].clone())).collect();
    apply_clearing(&mut reason_ctx.messages, &plan);
    let cleared: Vec<(usize, ChatMessage)> =
        plan.indices.iter().map(|&i| (i, reason_ctx.messages[i].clone())).collect();
    if let Err(e) = delegate.persist_cleared(&cleared, &plan.tool_call_ids, plan.freed_tokens).await {
        tracing::warn!(error = %e, "tool result clearing not persisted; history left unchanged");
        for (i, m) in originals {
            reason_ctx.messages[i] = m;
        }
        return 0;
    }
    tracing::info!(
        "tool_result_clearing cleared={} messages={} freed_tokens={}",
        plan.tool_call_ids.len(),
        plan.indices.len(),
        plan.freed_tokens,
    );
    plan.freed_tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn call(id: &str, name: &str, args: serde_json::Value) -> ChatMessage {
        ChatMessage::assistant_with_tool_use(id, name, args)
    }

    fn result(id: &str, tokens: usize) -> ChatMessage {
        ChatMessage::user_tool_result(id, &"a".repeat(tokens * 4), false)
    }

    fn result_text<'a>(m: &'a ChatMessage, id: &str) -> &'a str {
        m.content
            .iter()
            .find_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, content, .. } if tool_use_id == id => Some(content.as_str()),
                _ => None,
            })
            .unwrap()
    }

    /// user, then g1..g4 (one read_file each, ~8K-token results), then g5 =
    /// two parallel calls answered in one message.
    fn history() -> Vec<ChatMessage> {
        let mut msgs = vec![ChatMessage::user("q")];
        for n in 1..=4 {
            let id = format!("g{n}");
            msgs.push(call(&id, "read_file", json!({"path": format!("src/{n}.rs")})));
            msgs.push(result(&id, 8_000));
        }
        msgs.push(ChatMessage {
            role: uclaw_message_types::MessageRole::Assistant,
            content: vec![
                ContentBlock::ToolUse { id: "g5a".into(), name: "bash".into(), input: json!({"command": "ls"}) },
                ContentBlock::ToolUse { id: "g5b".into(), name: "bash".into(), input: json!({"command": "pwd"}) },
            ],
            compacted: false,
        });
        msgs.push(ChatMessage {
            role: uclaw_message_types::MessageRole::User,
            content: vec![
                ContentBlock::ToolResult { tool_use_id: "g5a".into(), content: "x".repeat(8_000 * 4), is_error: None },
                ContentBlock::ToolResult { tool_use_id: "g5b".into(), content: "y".repeat(8_000 * 4), is_error: None },
            ],
            compacted: false,
        });
        msgs
    }

    #[test]
    fn clearing_respects_recent_groups_floor_and_idempotence() {
        let mut msgs = history();
        let len = msgs.len();

        // Recent 3 groups (g3, g4 and the parallel g5) are kept.
        let plan = plan_clearing(&msgs, len, 3, 10_000).expect("~16K clearable >= 10K floor");
        assert_eq!(plan.tool_call_ids, vec!["g1", "g2"]);
        assert_eq!(plan.indices, vec![2, 4]);
        assert!(plan.freed_tokens > 15_000 && plan.freed_tokens < 16_000, "{}", plan.freed_tokens);

        // Below the floor → nothing.
        assert_eq!(plan_clearing(&msgs, len, 3, 20_000), None);

        apply_clearing(&mut msgs, &plan);
        for id in ["g1", "g2"] {
            let m = msgs.iter().find(|m| result_text_opt(m, id)).unwrap();
            assert!(result_text(m, id).starts_with(CLEARED_PREFIX), "{id} cleared");
        }
        for id in ["g3", "g4", "g5a", "g5b"] {
            let m = msgs.iter().find(|m| result_text_opt(m, id)).unwrap();
            assert!(!result_text(m, id).starts_with(CLEARED_PREFIX), "{id} kept");
        }
        let after = msgs.clone();

        // Idempotent: already-cleared results are never cleared again.
        assert_eq!(plan_clearing(&msgs, len, 3, 1), None);
        apply_clearing(&mut msgs, &plan);
        assert_eq!(serde_json::to_string(&msgs).unwrap(), serde_json::to_string(&after).unwrap());
    }

    fn result_text_opt(m: &ChatMessage, id: &str) -> bool {
        m.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { tool_use_id, .. } if tool_use_id == id))
    }

    #[test]
    fn placeholder_formats_read_grep_bash() {
        assert_eq!(
            placeholder("read_file", &json!({"path": "src/foo.rs", "offset": 1, "limit": 200}), 3_100),
            "[已清理的工具结果：read_file src/foo.rs 第 1–200 行（约 3.1K token）——如需内容请重新读取]"
        );
        assert_eq!(
            placeholder("read_file", &json!({"path": "src/foo.rs"}), 12_340),
            "[已清理的工具结果：read_file src/foo.rs 全文（约 12.3K token）——如需内容请重新读取]"
        );
        assert_eq!(
            placeholder("grep", &json!({"pattern": "foo", "path": "src/"}), 1_400),
            "[已清理的工具结果：grep \"foo\" 于 src/（约 1.4K token）——如需内容请重新读取]"
        );
        assert_eq!(
            placeholder("glob", &json!({"pattern": "**/*.rs"}), 500),
            "[已清理的工具结果：glob \"**/*.rs\"（约 0.5K token）——如需内容请重新读取]"
        );
        let cmd = format!("cargo test --lib {}", "x".repeat(100));
        let expected_cmd: String = cmd.chars().take(60).collect();
        assert_eq!(
            placeholder("bash", &json!({ "command": cmd }), 2_000),
            format!("[已清理的工具结果：bash {expected_cmd}…（约 2.0K token）——如需内容请重新读取]")
        );
        assert_eq!(
            placeholder("web_fetch", &json!({"url": "https://example.com", "max": 3}), 900),
            "[已清理的工具结果：web_fetch https://example.com（约 0.9K token）——如需内容请重新读取]"
        );
        assert_eq!(
            placeholder("todo_read", &json!({}), 900),
            "[已清理的工具结果：todo_read（约 0.9K token）——如需内容请重新读取]"
        );
    }

    #[test]
    fn clearing_never_touches_unpersisted_messages() {
        let msgs = history();
        // Only user + g1 call + g1 result + g2 call are persisted: g2's
        // result (this iteration, not yet written) must stay.
        let plan = plan_clearing(&msgs, 4, 3, 1).expect("g1 is persisted and old");
        assert_eq!(plan.tool_call_ids, vec!["g1"]);
        assert_eq!(plan.indices, vec![2]);
        assert!(plan.indices.iter().all(|&i| i < 4));

        // Hard-truncated (compacted) messages are not sent: never cleared.
        let mut msgs = history();
        msgs[2].compacted = true;
        let plan = plan_clearing(&msgs, msgs.len(), 3, 1).unwrap();
        assert_eq!(plan.tool_call_ids, vec!["g2"]);
    }
}
