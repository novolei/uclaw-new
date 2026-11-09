//! Token-budget rolling window for agent message history.
//!
//! Replaces the fixed `LIMIT 40` approach with a head+tail strategy:
//! keep the oldest 40 % and newest 40 % of the budget; drop the middle
//! when the full history exceeds the budget.  A synthetic gap marker is
//! inserted between the two halves so the LLM knows context was elided.
//!
//! Rationale: head tokens provide anchoring context (what was asked,
//! what was agreed); tail tokens are the most task-relevant recent turns.
//! The middle is the least informative part of a long session.

use crate::agent::types::{estimate_tokens, ChatMessage};
#[cfg(test)]
use crate::agent::types::{ContentBlock, MessageRole};

/// Default token budget for the history window (~25% of a 128K context,
/// leaving room for system prompt + tool defs + current turn output).
pub const HISTORY_TOKEN_BUDGET: u32 = 32_000;

/// Head fraction of the token budget reserved for oldest messages.
const HEAD_RATIO: f32 = 0.40;
/// Tail fraction of the token budget reserved for newest messages.
const TAIL_RATIO: f32 = 0.40;

/// Role string used for the synthetic gap marker message.
const GAP_ROLE: &str = "user";
/// Content of the synthetic gap marker so the LLM understands the elision.
const GAP_CONTENT: &str =
    "[Earlier context omitted — messages were removed to fit the token budget. \
     The conversation continues from the most recent messages below.]";

/// Apply a token-budget head+tail window to a chronologically-ordered
/// (ASC) list of `(role, content)` message pairs.
///
/// If the total estimated token count is within `budget`, the input is
/// returned unchanged.  Otherwise the middle is dropped and a gap marker
/// is inserted between the head and tail halves.
pub fn history_budget_window(
    messages: Vec<(String, String)>,
    budget: u32,
) -> Vec<(String, String)> {
    let total: u32 = messages
        .iter()
        .map(|(_, content)| estimate_tokens(content))
        .sum();

    if total <= budget {
        return messages;
    }

    let head_budget = (budget as f32 * HEAD_RATIO) as u32;
    let tail_budget = (budget as f32 * TAIL_RATIO) as u32;

    // Collect head: walk forward until head_budget exhausted.
    let mut head: Vec<(String, String)> = Vec::new();
    let mut used = 0u32;
    for msg in &messages {
        let t = estimate_tokens(&msg.1);
        if used + t > head_budget {
            break;
        }
        head.push(msg.clone());
        used += t;
    }

    // Collect tail: walk backward until tail_budget exhausted.
    let mut tail: Vec<(String, String)> = Vec::new();
    used = 0u32;
    for msg in messages.iter().rev() {
        let t = estimate_tokens(&msg.1);
        if used + t > tail_budget {
            break;
        }
        tail.push(msg.clone());
        used += t;
    }
    tail.reverse();

    // Determine where the tail starts in the original slice index.
    let tail_start_idx = messages.len().saturating_sub(tail.len());
    let head_end_idx = head.len();

    // If head and tail overlap or are contiguous, no gap needed — return all.
    if head_end_idx >= tail_start_idx {
        return messages;
    }

    let mut result = head;
    result.push((GAP_ROLE.to_string(), GAP_CONTENT.to_string()));
    result.extend(tail);

    tracing::debug!(
        total_tokens = total,
        budget,
        head_msgs = head_end_idx,
        tail_msgs = result.len() - head_end_idx - 1,
        dropped_msgs = tail_start_idx - head_end_idx,
        "history_budget_window: middle elided"
    );

    result
}

/// Turn persisted `agent_messages` `(role, content)` rows into the
/// `ChatMessage`s that seed the agent's `ReasoningContext`.
///
/// `system` rows are the `/<skill>` / `/<command>` prompts persisted by
/// `send_agent_message` just before the invoking user row. They are
/// already self-delimited (`<skill name=..>…</skill>` /
/// `<command name=..>…</command>`) and are merged into the *following*
/// user message rather than replayed as `role: system`: the Anthropic
/// provider drops in-`messages` system entries (or promotes the first one
/// to the system prompt), so a system-role replay would be lost or
/// clobber the real system prompt. Merging also keeps user/assistant
/// alternation intact. A system row with no following user row is
/// emitted as its own user message so it is never silently dropped.
/// The number of returned messages may therefore be < `history.len()`.
pub fn replay_history_messages(history: &[(String, String)]) -> Vec<ChatMessage> {
    merge_skill_rows(history.iter().map(|(r, c)| (r.as_str(), c.as_str(), ())))
        .into_iter()
        .map(|(m, ())| m)
        .collect()
}

/// The merge rule of [`replay_history_messages`], carrying a per-row `tag`
/// (e.g. the `agent_messages` id) through: a merged user message keeps the
/// user row's tag; an orphan system group keeps its first system row's tag;
/// assistant messages keep their own. Rows with other roles are skipped.
pub fn merge_skill_rows<'a, T>(
    rows: impl IntoIterator<Item = (&'a str, &'a str, T)>,
) -> Vec<(ChatMessage, T)> {
    let mut out = Vec::new();
    let mut pending: Vec<&str> = Vec::new();
    let mut pending_tag: Option<T> = None;
    fn flush<T>(out: &mut Vec<(ChatMessage, T)>, pending: &mut Vec<&str>, tag: &mut Option<T>) {
        if let Some(t) = tag.take() {
            out.push((ChatMessage::user(&pending.join("\n\n")), t));
        }
        pending.clear();
    }
    for (role, content, tag) in rows {
        match role {
            "user" => {
                if pending.is_empty() {
                    out.push((ChatMessage::user(content), tag));
                } else {
                    let merged = format!("{}\n\n{}", pending.join("\n\n"), content);
                    pending.clear();
                    pending_tag = None;
                    out.push((ChatMessage::user(&merged), tag));
                }
            }
            "assistant" => {
                flush(&mut out, &mut pending, &mut pending_tag);
                out.push((ChatMessage::assistant(content), tag));
            }
            "system" => {
                if pending_tag.is_none() {
                    pending_tag = Some(tag);
                }
                pending.push(content);
            }
            _ => {}
        }
    }
    flush(&mut out, &mut pending, &mut pending_tag);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msgs(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs
            .iter()
            .map(|(r, c)| (r.to_string(), c.to_string()))
            .collect()
    }

    #[test]
    fn under_budget_passthrough() {
        let input = msgs(&[("user", "hello"), ("assistant", "hi")]);
        let result = history_budget_window(input.clone(), 32_000);
        assert_eq!(result, input);
    }

    #[test]
    fn gap_inserted_when_over_budget() {
        // Build messages that are definitely over a tiny budget.
        // Each content is ~50 chars; estimate_tokens gives ≈13 tokens each.
        let long_content = "a".repeat(200); // ~50 tokens each
        let pairs: Vec<(String, String)> = (0..20)
            .map(|i| (if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                      format!("{} msg {}", long_content, i)))
            .collect();

        let result = history_budget_window(pairs.clone(), 400);

        // Should contain fewer messages than original
        assert!(result.len() < pairs.len());

        // Should contain the gap marker
        let has_gap = result
            .iter()
            .any(|(_, c)| c.starts_with("[Earlier context omitted"));
        assert!(has_gap, "gap marker should be present");

        // First message should match first of original (head preserved)
        assert_eq!(result[0], pairs[0]);

        // Last message should match last of original (tail preserved)
        assert_eq!(result.last().unwrap(), pairs.last().unwrap());
    }

    #[test]
    fn head_tail_overlap_returns_all() {
        // Only a few tiny messages — head + tail together cover everything,
        // so no gap should appear.
        let input = msgs(&[
            ("user", "hi"),
            ("assistant", "hello"),
            ("user", "bye"),
        ]);
        let result = history_budget_window(input.clone(), 50);
        // With such a small budget and overlapping head/tail, returns all
        // (or at most original length, no gap).
        let has_gap = result.iter().any(|(_, c)| c.starts_with("[Earlier context omitted"));
        assert!(!has_gap);
    }

    // ── replay_history_messages ──────────────────────────────────────

    fn text_of(m: &ChatMessage) -> String {
        m.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("")
    }

    #[test]
    fn replay_plain_user_assistant_rows_unchanged() {
        let out = replay_history_messages(&msgs(&[("user", "hi"), ("assistant", "hello")]));
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].role, MessageRole::User);
        assert_eq!(text_of(&out[0]), "hi");
        assert_eq!(out[1].role, MessageRole::Assistant);
        assert_eq!(text_of(&out[1]), "hello");
    }

    #[test]
    fn replay_merges_slash_skill_system_row_into_following_user_message() {
        let skill = "<skill name=\"tdd\" version=\"1\">\nWrite the test first.\n</skill>";
        let out = replay_history_messages(&msgs(&[
            ("user", "earlier"),
            ("assistant", "ok"),
            ("system", skill),
            ("user", "/tdd fix the parser"),
        ]));
        assert_eq!(out.len(), 3, "skill row merges into the next user turn");
        assert_eq!(out[2].role, MessageRole::User);
        let t = text_of(&out[2]);
        assert!(t.starts_with(skill), "skill instructions come first: {t}");
        assert!(t.ends_with("/tdd fix the parser"), "user text preserved: {t}");
        // Never emitted as role:system (Anthropic drops / hijacks those).
        assert!(out.iter().all(|m| m.role != MessageRole::System));
    }

    #[test]
    fn replay_orphan_system_row_is_kept_as_user_message() {
        let out = replay_history_messages(&msgs(&[
            ("system", "<command name=\"x\">\nbody\n</command>"),
            ("assistant", "done"),
            ("system", "trailing"),
        ]));
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].role, MessageRole::User);
        assert!(text_of(&out[0]).contains("body"));
        assert_eq!(out[1].role, MessageRole::Assistant);
        assert_eq!(out[2].role, MessageRole::User);
        assert_eq!(text_of(&out[2]), "trailing");
    }
}
