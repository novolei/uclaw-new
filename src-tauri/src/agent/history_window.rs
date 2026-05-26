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

use crate::agent::types::estimate_tokens;

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
            .map(|i| {
                (
                    if i % 2 == 0 { "user" } else { "assistant" }.to_string(),
                    format!("{} msg {}", long_content, i),
                )
            })
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
        let input = msgs(&[("user", "hi"), ("assistant", "hello"), ("user", "bye")]);
        let result = history_budget_window(input.clone(), 50);
        // With such a small budget and overlapping head/tail, returns all
        // (or at most original length, no gap).
        let has_gap = result
            .iter()
            .any(|(_, c)| c.starts_with("[Earlier context omitted"));
        assert!(!has_gap);
    }
}
