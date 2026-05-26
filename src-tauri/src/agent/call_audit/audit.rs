//! `audit_call_outputs` — orphan-tool-call detection + synthesis.
//!
//! Algorithm:
//!
//! 1. Single linear scan of the message list.
//! 2. For every `ToolCall { call_id }` encountered, look forward for
//!    a `ToolResult { call_id: same }`. If found, pair them and
//!    continue. If not found before EOM, the call is **orphan**.
//! 3. For each orphan, splice a synthesized `ToolResult { call_id,
//!    content: "aborted", is_aborted: true }` immediately after the
//!    orphan call.
//!
//! Properties:
//!
//! - **Idempotent**: passing the result through a second time produces
//!   identical output + `EnsureStats::is_clean()`.
//! - **Stable**: relative order of all original messages is preserved.
//!   Inserts are always *immediately after* the orphan call.
//! - **Provider-agnostic**: callers map their wire format to
//!   [`AuditMessage`] before invoking. The real dispatcher bridge
//!   lives in M2-H L6 commit 2.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Provider-agnostic message shape for the audit.
///
/// Only the fields L6 needs to reason about orphans are kept — full
/// fidelity to the wire format isn't required because the audit
/// produces a strict superset of the input (an extra `ToolResult` for
/// each orphan; never modifies existing messages).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditMessage {
    /// Plain user-authored text.
    UserText { text: String },
    /// Plain assistant text (no tool call).
    AssistantText { text: String },
    /// Assistant requests a tool call. `call_id` is the
    /// provider-assigned id used to correlate with the response.
    ToolCall {
        call_id: String,
        name: String,
        args: serde_json::Value,
    },
    /// Tool result (either real or synthesized aborted placeholder).
    ToolResult {
        call_id: String,
        content: String,
        /// `true` when this message was synthesized by
        /// [`audit_call_outputs`] to fill an orphan slot.
        #[serde(default)]
        is_aborted: bool,
    },
}

/// One orphan-call observation — emitted for each call that had to
/// be patched with a synthetic result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrphanCall {
    /// Original 0-based index of the orphan call in the input slice.
    pub original_index: usize,
    pub call_id: String,
    pub tool_name: String,
}

/// Audit result — orphans found, placeholders inserted, ids touched.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EnsureStats {
    /// Number of `"aborted"` placeholder ToolResults that were
    /// synthesized.
    pub orphans_synthesized: usize,
    /// Detailed per-orphan log — call_id + tool name + original index.
    /// Order matches the input scan order.
    pub orphan_calls: Vec<OrphanCall>,
}

impl EnsureStats {
    /// `true` if no orphans were found — caller can skip re-serializing
    /// the message list.
    pub fn is_clean(&self) -> bool {
        self.orphans_synthesized == 0
    }
}

/// Default placeholder content for a synthesized aborted tool result.
pub const ABORTED_PLACEHOLDER: &str = "aborted";

/// Audit `messages` for orphan tool calls. Returns a new vector with
/// `"aborted"` placeholders spliced in immediately after each orphan
/// call, plus a stats record describing what was patched.
///
/// The input is consumed (`Vec<AuditMessage>`) so the audit can avoid
/// cloning the (often-large) tail of the message list.
pub fn audit_call_outputs(messages: Vec<AuditMessage>) -> (Vec<AuditMessage>, EnsureStats) {
    let mut stats = EnsureStats::default();

    // Pass 1: scan to find which call_ids have a matching result.
    //
    // Forward-lookahead would let us decide orphan-ness inline, but a
    // pre-scan is simpler and lets pass 2 stay a single iteration.
    let resolved: HashSet<String> = messages
        .iter()
        .filter_map(|m| match m {
            AuditMessage::ToolResult { call_id, .. } => Some(call_id.clone()),
            _ => None,
        })
        .collect();

    // Pass 2: rebuild list, inserting placeholders after orphans.
    // Per-provider invariant: call_id is unique per call, so we don't
    // need to track newly-inserted placeholders against further input.
    let mut out: Vec<AuditMessage> = Vec::with_capacity(messages.len());
    for (i, m) in messages.into_iter().enumerate() {
        if let AuditMessage::ToolCall { call_id, name, .. } = &m {
            if !resolved.contains(call_id) {
                stats.orphans_synthesized += 1;
                stats.orphan_calls.push(OrphanCall {
                    original_index: i,
                    call_id: call_id.clone(),
                    tool_name: name.clone(),
                });
                let synth = AuditMessage::ToolResult {
                    call_id: call_id.clone(),
                    content: ABORTED_PLACEHOLDER.into(),
                    is_aborted: true,
                };
                out.push(m);
                out.push(synth);
                continue;
            }
        }
        out.push(m);
    }

    (out, stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn user(text: &str) -> AuditMessage {
        AuditMessage::UserText { text: text.into() }
    }

    fn assistant(text: &str) -> AuditMessage {
        AuditMessage::AssistantText { text: text.into() }
    }

    fn call(id: &str, name: &str) -> AuditMessage {
        AuditMessage::ToolCall {
            call_id: id.into(),
            name: name.into(),
            args: json!({}),
        }
    }

    fn result(id: &str, content: &str) -> AuditMessage {
        AuditMessage::ToolResult {
            call_id: id.into(),
            content: content.into(),
            is_aborted: false,
        }
    }

    fn aborted_synth(id: &str) -> AuditMessage {
        AuditMessage::ToolResult {
            call_id: id.into(),
            content: ABORTED_PLACEHOLDER.into(),
            is_aborted: true,
        }
    }

    // ── empty / no-tool cases ───────────────────────────────────────

    #[test]
    fn empty_list_is_clean() {
        let (out, stats) = audit_call_outputs(vec![]);
        assert!(stats.is_clean());
        assert_eq!(out.len(), 0);
    }

    #[test]
    fn pure_text_conversation_is_clean() {
        let input = vec![user("hi"), assistant("hello"), user("how are you?")];
        let (out, stats) = audit_call_outputs(input.clone());
        assert!(stats.is_clean());
        assert_eq!(out, input);
    }

    // ── well-formed call/result pairs ────────────────────────────────

    #[test]
    fn matched_call_result_pair_is_clean() {
        let input = vec![
            user("run shell"),
            call("c1", "shell"),
            result("c1", "output"),
            assistant("done"),
        ];
        let (out, stats) = audit_call_outputs(input.clone());
        assert!(stats.is_clean());
        assert_eq!(out, input);
    }

    #[test]
    fn multiple_matched_pairs_are_clean() {
        let input = vec![
            call("a", "shell"),
            result("a", "ok"),
            call("b", "read"),
            result("b", "contents"),
            call("c", "search"),
            result("c", "matches"),
        ];
        let (out, stats) = audit_call_outputs(input.clone());
        assert!(stats.is_clean());
        assert_eq!(out, input);
    }

    // ── single orphan ───────────────────────────────────────────────

    #[test]
    fn single_orphan_call_gets_synthesized_result() {
        let input = vec![user("run"), call("c1", "shell"), assistant("interrupted")];
        let (out, stats) = audit_call_outputs(input);
        assert_eq!(stats.orphans_synthesized, 1);
        assert_eq!(stats.orphan_calls.len(), 1);
        assert_eq!(stats.orphan_calls[0].call_id, "c1");
        assert_eq!(stats.orphan_calls[0].tool_name, "shell");
        assert_eq!(stats.orphan_calls[0].original_index, 1);

        // Synthesized placeholder must come IMMEDIATELY AFTER the call.
        assert_eq!(out.len(), 4);
        assert_eq!(out[0], user("run"));
        assert_eq!(out[1], call("c1", "shell"));
        assert_eq!(out[2], aborted_synth("c1"));
        assert_eq!(out[3], assistant("interrupted"));
    }

    #[test]
    fn synthesized_placeholder_marked_is_aborted_true() {
        let input = vec![call("c1", "shell")];
        let (out, _) = audit_call_outputs(input);
        assert_eq!(out.len(), 2);
        match &out[1] {
            AuditMessage::ToolResult {
                is_aborted,
                content,
                ..
            } => {
                assert!(*is_aborted);
                assert_eq!(content, ABORTED_PLACEHOLDER);
            }
            other => panic!("expected synthesized ToolResult, got {other:?}"),
        }
    }

    // ── multiple orphans ────────────────────────────────────────────

    #[test]
    fn multiple_orphans_get_independent_placeholders() {
        let input = vec![
            call("a", "shell"),
            // missing result for a
            call("b", "read"),
            // missing result for b
            call("c", "search"),
            result("c", "found"),
        ];
        let (out, stats) = audit_call_outputs(input);
        assert_eq!(stats.orphans_synthesized, 2);
        assert_eq!(
            stats
                .orphan_calls
                .iter()
                .map(|o| o.call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        // After: a, a-aborted, b, b-aborted, c, c-result
        assert_eq!(out.len(), 6);
        assert_eq!(out[1], aborted_synth("a"));
        assert_eq!(out[3], aborted_synth("b"));
        assert_eq!(out[4], call("c", "search"));
        assert_eq!(out[5], result("c", "found"));
    }

    // ── interleaved orphan + non-orphan ─────────────────────────────

    #[test]
    fn orphan_in_middle_doesnt_break_later_pairs() {
        let input = vec![
            call("a", "shell"),
            result("a", "ok"),
            call("orphan", "search"),
            // no result
            call("c", "read"),
            result("c", "contents"),
        ];
        let (out, stats) = audit_call_outputs(input);
        assert_eq!(stats.orphans_synthesized, 1);
        assert_eq!(stats.orphan_calls[0].call_id, "orphan");
        // After: a, a-result, orphan, orphan-aborted, c, c-result
        assert_eq!(out.len(), 6);
        assert_eq!(out[2], call("orphan", "search"));
        assert_eq!(out[3], aborted_synth("orphan"));
        assert_eq!(out[5], result("c", "contents"));
    }

    // ── out-of-order result still counts as resolved ────────────────

    #[test]
    fn result_appearing_after_unrelated_messages_still_matches() {
        // Real provider traces sometimes interleave assistant text
        // between call and result.
        let input = vec![
            call("a", "shell"),
            assistant("thinking..."),
            result("a", "output"),
        ];
        let (out, stats) = audit_call_outputs(input.clone());
        assert!(stats.is_clean(), "result later in list is still a match");
        assert_eq!(out, input);
    }

    // ── idempotency ─────────────────────────────────────────────────

    #[test]
    fn second_pass_is_noop() {
        let input = vec![call("a", "shell"), assistant("crash")];
        let (once, stats1) = audit_call_outputs(input);
        assert_eq!(stats1.orphans_synthesized, 1);

        let (twice, stats2) = audit_call_outputs(once.clone());
        assert!(stats2.is_clean(), "second pass should find no orphans");
        assert_eq!(once, twice);
    }

    // ── serde tags use snake_case ───────────────────────────────────

    #[test]
    fn serde_roundtrip_each_variant() {
        let inputs = vec![
            user("hi"),
            assistant("hello"),
            call("c1", "shell"),
            result("c1", "ok"),
            aborted_synth("c2"),
        ];
        for m in inputs {
            let json = serde_json::to_string(&m).unwrap();
            let back: AuditMessage = serde_json::from_str(&json).unwrap();
            assert_eq!(m, back);
        }
    }

    #[test]
    fn serde_tag_is_snake_case() {
        let m = call("c1", "shell");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["kind"], json!("tool_call"));

        let m = aborted_synth("c1");
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["kind"], json!("tool_result"));
        assert_eq!(v["is_aborted"], json!(true));
    }

    // ── stats accuracy ──────────────────────────────────────────────

    #[test]
    fn stats_record_original_index_not_post_insert_index() {
        // Orphan calls at original positions 0 and 2.
        let input = vec![
            call("a", "shell"), // orig 0
            assistant("…"),     // orig 1
            call("b", "read"),  // orig 2
        ];
        let (_, stats) = audit_call_outputs(input);
        assert_eq!(stats.orphans_synthesized, 2);
        assert_eq!(stats.orphan_calls[0].original_index, 0);
        assert_eq!(stats.orphan_calls[1].original_index, 2);
    }
}
