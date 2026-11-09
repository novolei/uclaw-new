// SPDX-License-Identifier: Apache-2.0
//! Pure policy functions for context compaction (threshold selection,
//! decision, cut point).

use crate::agent::types::{estimate_live_message_tokens, ChatMessage, ContentBlock, MessageRole};

/// Lower bound for the user-configured compaction threshold (tokens).
pub const MIN_COMPACTION_THRESHOLD: usize = 32_000;
/// Tokens reserved for model output.
pub const OUTPUT_RESERVE: usize = 16_384;
/// Extra safety margin below the model window.
pub const SAFETY_MARGIN: usize = 8_192;

/// Effective compaction threshold = clamp(configured, 32K, window - reserve - margin).
/// The 32K floor only constrains the configured value; a small model window cap wins.
/// `model_window == 0` means unknown: the (floored) configured value is used.
pub fn effective_compaction_threshold(configured: usize, model_window: usize) -> usize {
    let floored = configured.max(MIN_COMPACTION_THRESHOLD);
    if model_window == 0 {
        return floored;
    }
    let cap = model_window.saturating_sub(OUTPUT_RESERVE + SAFETY_MARGIN);
    floored.min(cap)
}

/// What the loop should do with the current request size (transcript mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompactionDecision {
    /// Under the threshold: send as-is.
    None,
    /// At/over the threshold but below the model's hard limit: compact.
    Compact,
    /// At/over the hard limit (`model_window - OUTPUT_RESERVE`): truncate.
    HardTruncate,
}

/// Absolute-threshold compaction decision.
/// `< threshold` → None; `>= threshold` and `< hard_limit` → Compact;
/// `>= hard_limit` → HardTruncate, where `hard_limit = model_window - OUTPUT_RESERVE`.
/// `model_window == 0` means unknown: there is no hard limit.
pub fn decide_compaction(
    estimated_request_tokens: usize,
    threshold: usize,
    model_window: usize,
) -> CompactionDecision {
    if model_window > 0 && estimated_request_tokens >= model_window.saturating_sub(OUTPUT_RESERVE) {
        return CompactionDecision::HardTruncate;
    }
    if estimated_request_tokens >= threshold {
        CompactionDecision::Compact
    } else {
        CompactionDecision::None
    }
}

/// Minimum recent context (tokens) kept verbatim after a compaction.
pub const KEEP_RECENT_TOKENS: usize = 20_000;
/// Minimum number of recent user turns kept verbatim after a compaction.
pub const KEEP_RECENT_USER_TURNS: usize = 3;
/// Every `session_block` (rendered `StructuredFold`) starts with this header.
pub const SESSION_BLOCK_HEADER: &str = "## Earlier conversation (compacted)";

/// A user message typed by a person (or injected as text) — i.e. a user
/// message that carries no `ToolResult`. Turn boundaries start here.
pub fn is_user_text(m: &ChatMessage) -> bool {
    m.role == MessageRole::User
        && !m.content.iter().any(|b| matches!(b, ContentBlock::ToolResult { .. }))
}

/// A compaction `session_block`: a user text message starting with the
/// fold header.
pub fn is_session_block(m: &ChatMessage) -> bool {
    is_user_text(m)
        && matches!(m.content.first(), Some(ContentBlock::Text { text }) if text.starts_with(SESSION_BLOCK_HEADER))
}

/// Start index of the verbatim tail to keep when compacting `msgs`.
/// The tail holds at least the last `KEEP_RECENT_USER_TURNS` user text
/// messages or at least `KEEP_RECENT_TOKENS` tokens, whichever keeps more,
/// and always starts at a user text message (so a ToolUse and its
/// ToolResult are never separated). A leading `session_block` is not a turn.
/// `None` when nothing (beyond a leading `session_block`) would be compacted.
pub fn choose_cut_index(msgs: &[ChatMessage]) -> Option<usize> {
    let first = msgs.first()?;
    let compactable_start = usize::from(is_session_block(first));
    let user_idx: Vec<usize> = msgs
        .iter()
        .enumerate()
        .skip(compactable_start)
        .filter(|(_, m)| is_user_text(m))
        .map(|(i, _)| i)
        .collect();
    if user_idx.is_empty() {
        return None;
    }
    let by_turns = user_idx[user_idx.len().saturating_sub(KEEP_RECENT_USER_TURNS)];

    // Walk back from the end until the tail holds KEEP_RECENT_TOKENS, then
    // snap down to a user text message. Never reached → keep everything.
    let mut acc = 0usize;
    let mut reached: Option<usize> = None;
    for (i, m) in msgs.iter().enumerate().rev() {
        if !m.compacted {
            acc += estimate_live_message_tokens(m);
        }
        if acc >= KEEP_RECENT_TOKENS {
            reached = Some(i);
            break;
        }
    }
    let by_tokens = reached
        .and_then(|r| user_idx.iter().rev().find(|&&u| u <= r).copied())
        .unwrap_or(0);

    let cut = by_turns.min(by_tokens);
    (cut > compactable_start).then_some(cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effective_threshold_uses_configured_when_window_large() {
        assert_eq!(effective_compaction_threshold(128_000, 1_000_000), 128_000);
    }

    #[test]
    fn effective_threshold_clamps_to_small_window() {
        let t = effective_compaction_threshold(128_000, 32_768);
        assert_eq!(t, 32_768 - 16_384 - 8_192);
        assert!(t <= 32_768);
    }

    #[test]
    fn effective_threshold_enforces_minimum_on_config() {
        assert_eq!(effective_compaction_threshold(10_000, 1_000_000), 32_000);
    }

    #[test]
    fn effective_threshold_unknown_window_uses_configured() {
        assert_eq!(effective_compaction_threshold(128_000, 0), 128_000);
    }

    #[test]
    fn decision_none_below_threshold() {
        assert_eq!(decide_compaction(127_999, 128_000, 1_000_000), CompactionDecision::None);
        assert_eq!(decide_compaction(0, 128_000, 0), CompactionDecision::None);
    }

    #[test]
    fn decision_compacts_at_threshold() {
        assert_eq!(decide_compaction(128_000, 128_000, 1_000_000), CompactionDecision::Compact);
        assert_eq!(decide_compaction(500_000, 128_000, 1_000_000), CompactionDecision::Compact);
        // Unknown window (0) = no hard ceiling: never HardTruncate.
        assert_eq!(decide_compaction(10_000_000, 128_000, 0), CompactionDecision::Compact);
    }

    #[test]
    fn decision_hard_truncates_past_window() {
        let window = 200_000;
        let hard_limit = window - OUTPUT_RESERVE;
        assert_eq!(decide_compaction(hard_limit - 1, 128_000, window), CompactionDecision::Compact);
        assert_eq!(decide_compaction(hard_limit, 128_000, window), CompactionDecision::HardTruncate);
        assert_eq!(decide_compaction(1_000_000, 128_000, window), CompactionDecision::HardTruncate);
    }

    #[test]
    fn decision_compacts_when_window_shrinks() {
        // The same estimate is fine in a 1M window but must act in a 32K one.
        let estimate = 20_000;
        let big = effective_compaction_threshold(128_000, 1_000_000);
        assert_eq!(decide_compaction(estimate, big, 1_000_000), CompactionDecision::None);
        let small = effective_compaction_threshold(128_000, 32_768);
        assert_eq!(decide_compaction(estimate, small, 32_768), CompactionDecision::HardTruncate);
        assert_eq!(decide_compaction(small, small, 32_768), CompactionDecision::Compact);
    }

    fn big_user(tokens: usize) -> ChatMessage {
        ChatMessage::user(&"a".repeat(tokens * 4))
    }

    #[test]
    fn choose_cut_index_keeps_last_three_user_turns() {
        // 5 turns, each user message ~15K tokens: the token rule alone would
        // keep only the last two turns (30K >= 20K); three turns keep more.
        let mut msgs = Vec::new();
        for i in 0..5 {
            msgs.push(big_user(15_000));
            msgs.push(ChatMessage::assistant(&format!("reply {i}")));
        }
        assert_eq!(choose_cut_index(&msgs), Some(4));
    }

    #[test]
    fn choose_cut_index_token_rule_keeps_more_when_turns_are_small() {
        // Old big turn, then 4 tiny turns: the last three turns are far below
        // 20K tokens, so the token rule reaches back further.
        let mut msgs = vec![big_user(30_000), ChatMessage::assistant("r0")];
        msgs.push(big_user(25_000));
        msgs.push(ChatMessage::assistant("r1"));
        for i in 0..4 {
            msgs.push(ChatMessage::user(&format!("small {i}")));
            msgs.push(ChatMessage::assistant(&format!("r{i}")));
        }
        assert_eq!(choose_cut_index(&msgs), Some(2));
    }

    #[test]
    fn choose_cut_index_never_splits_tool_pair() {
        let msgs = vec![
            ChatMessage::user("u0"),
            ChatMessage::assistant("a0"),
            ChatMessage::user("u1"),
            ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({})),
            // The 20K-token boundary lands on this tool result.
            ChatMessage::user_tool_result("t1", &"a".repeat(25_000 * 4), false),
            ChatMessage::assistant("a1"),
            ChatMessage::user("u2"),
            ChatMessage::assistant("a2"),
            ChatMessage::user("u3"),
            ChatMessage::assistant("a3"),
            ChatMessage::user("u4"),
            ChatMessage::assistant("a4"),
        ];
        let cut = choose_cut_index(&msgs).expect("something to compact");
        assert_eq!(cut, 2, "tail must start at the user message owning the tool pair");
        assert!(is_user_text(&msgs[cut]));
    }

    #[test]
    fn choose_cut_index_none_when_nothing_to_compact() {
        assert_eq!(choose_cut_index(&[]), None);
        let small = vec![
            ChatMessage::user("u0"),
            ChatMessage::assistant("a0"),
            ChatMessage::user("u1"),
            ChatMessage::assistant("a1"),
        ];
        assert_eq!(choose_cut_index(&small), None);
        // Only a leading session_block would be compacted → None.
        let after_compaction = vec![
            ChatMessage::user(&format!("{SESSION_BLOCK_HEADER}\n- x")),
            big_user(15_000),
            ChatMessage::assistant("a"),
            big_user(15_000),
            ChatMessage::assistant("a"),
            big_user(15_000),
            ChatMessage::assistant("a"),
        ];
        assert_eq!(choose_cut_index(&after_compaction), None);
    }
}
