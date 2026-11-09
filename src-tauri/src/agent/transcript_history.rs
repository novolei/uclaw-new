// SPDX-License-Identifier: Apache-2.0
//! Context continuity P1 — per-iteration transcript persistence with a
//! tool-call pairing guard.
//!
//! `flush_transcript` appends the not-yet-persisted tail of
//! `ReasoningContext.messages` to the transcript via
//! `LoopDelegate::persist_transcript`, but never past an assistant message
//! whose `ToolUse` blocks are still missing results (`complete_prefix_len`).
//! `finalize_transcript` closes a run: dangling tool calls get a synthesized
//! `[工具调用已取消]` error result so the persisted transcript is always a
//! valid tool_use → tool_result sequence.

use crate::agent::types::{ChatMessage, ContentBlock, LoopDelegate, MessageRole, ReasoningContext};

/// Result text synthesized for a `ToolUse` that never got a result.
pub const CANCELLED_TOOL_RESULT: &str = "[工具调用已取消]";

/// Tool-use ids still awaiting a result after scanning `msgs`, in order of
/// appearance, plus the longest prefix length at which none were pending.
/// `compacted` messages are neutral (neither open nor close calls).
fn scan(msgs: &[ChatMessage]) -> (usize, Vec<String>) {
    let mut pending: Vec<String> = Vec::new();
    let mut complete = 0;
    for (i, m) in msgs.iter().enumerate() {
        if !m.compacted {
            for b in &m.content {
                match b {
                    ContentBlock::ToolUse { id, .. } => pending.push(id.clone()),
                    ContentBlock::ToolResult { tool_use_id, .. } => {
                        pending.retain(|p| p != tool_use_id)
                    }
                    _ => {}
                }
            }
        }
        if pending.is_empty() {
            complete = i + 1;
        }
    }
    (complete, pending)
}

/// Length of the longest prefix of `msgs` that ends with no `ToolUse`
/// lacking its matching `ToolResult` (parallel calls need ALL results).
pub fn complete_prefix_len(msgs: &[ChatMessage]) -> usize {
    scan(msgs).0
}

/// For `ToolUse` blocks left dangling at the end of `msgs`, build one
/// `user` message carrying an `is_error = true` `[工具调用已取消]` result per
/// dangling call. `None` when every call already has a result.
pub fn synthesize_missing_tool_results(msgs: &[ChatMessage]) -> Option<ChatMessage> {
    let (_, pending) = scan(msgs);
    if pending.is_empty() {
        return None;
    }
    Some(ChatMessage {
        role: MessageRole::User,
        content: pending
            .into_iter()
            .map(|id| ContentBlock::ToolResult {
                tool_use_id: id,
                content: CANCELLED_TOOL_RESULT.to_string(),
                is_error: Some(true),
            })
            .collect(),
        compacted: false,
    })
}

/// Persist `messages[transcript_persisted_len .. +complete_prefix_len(tail)]`
/// (skipping `compacted` messages) and advance the counter on success. A
/// persist failure is logged and the counter is left alone so the next flush
/// retries; it never aborts the loop.
pub async fn flush_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext) {
    let start = ctx.transcript_persisted_len;
    let Some(tail) = ctx.messages.get(start..) else {
        tracing::warn!(
            persisted = start,
            len = ctx.messages.len(),
            "transcript flush: persisted watermark beyond history; skipping"
        );
        return;
    };
    let n = complete_prefix_len(tail);
    if n == 0 {
        return;
    }
    let to_write: Vec<ChatMessage> = tail[..n].iter().filter(|m| !m.compacted).cloned().collect();
    if !to_write.is_empty() {
        if let Err(e) = delegate.persist_transcript(&to_write).await {
            tracing::warn!(error = %e, count = to_write.len(), "transcript flush failed; will retry next flush");
            return;
        }
    }
    ctx.transcript_persisted_len = start + n;
}

/// End-of-run flush: append synthesized results for any dangling tool calls,
/// then `flush_transcript`.
pub async fn finalize_transcript(delegate: &dyn LoopDelegate, ctx: &mut ReasoningContext) {
    if let Some(results) = synthesize_missing_tool_results(&ctx.messages) {
        ctx.messages.push(results);
    }
    flush_transcript(delegate, ctx).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::turn::TurnSnapshot;
    use crate::agent::types::{
        LoopOutcome, LoopSignal, RespondOutput, ResponseMetadata, TextAction, ToolCall,
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordingDelegate {
        written: Mutex<Vec<ChatMessage>>,
    }

    #[async_trait::async_trait]
    impl LoopDelegate for RecordingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(&self, _: &mut ReasoningContext, _: usize) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            _: &mut ReasoningContext,
            _: &TurnSnapshot,
            _: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            Ok(RespondOutput::Text {
                text: String::new(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata { model: String::new(), finish_reason: None, usage: None },
            })
        }
        async fn handle_text_response(
            &self,
            _: &str,
            _: ResponseMetadata,
            _: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }
        async fn execute_tool_calls(
            &self,
            _: Vec<ToolCall>,
            _: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn persist_transcript(&self, new_msgs: &[ChatMessage]) -> anyhow::Result<()> {
            self.written.lock().unwrap().extend_from_slice(new_msgs);
            Ok(())
        }
    }

    fn assistant_calls(ids: &[&str]) -> ChatMessage {
        ChatMessage::assistant_from_response(
            None,
            None,
            "",
            ids.iter().map(|id| (id.to_string(), "noop".to_string(), json!({}))),
        )
    }

    fn result_ids(m: &ChatMessage) -> Vec<(String, String, Option<bool>)> {
        m.content
            .iter()
            .filter_map(|b| match b {
                ContentBlock::ToolResult { tool_use_id, content, is_error } => {
                    Some((tool_use_id.clone(), content.clone(), *is_error))
                }
                _ => None,
            })
            .collect()
    }

    #[test]
    fn complete_prefix_excludes_dangling_tool_use() {
        let mut msgs = vec![ChatMessage::user("hi"), assistant_calls(&["a"])];
        assert_eq!(complete_prefix_len(&msgs), 1);
        msgs.push(ChatMessage::user_tool_result("a", "ok", false));
        assert_eq!(complete_prefix_len(&msgs), 3);
    }

    #[test]
    fn complete_prefix_requires_all_parallel_results() {
        let msgs = vec![
            ChatMessage::user("hi"),
            assistant_calls(&["a", "b"]),
            ChatMessage::user_tool_result("a", "ok", false),
        ];
        assert_eq!(complete_prefix_len(&msgs), 1);
    }

    #[test]
    fn synthesize_is_none_when_all_paired() {
        let msgs = vec![
            ChatMessage::user("hi"),
            assistant_calls(&["a"]),
            ChatMessage::user_tool_result("a", "ok", false),
        ];
        assert!(synthesize_missing_tool_results(&msgs).is_none());
    }

    #[tokio::test]
    async fn finalize_synthesizes_results_for_dangling_tool_use() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages = vec![ChatMessage::user("hi"), assistant_calls(&["a", "b"])];

        finalize_transcript(&delegate, &mut ctx).await;

        let written = delegate.written.lock().unwrap().clone();
        assert_eq!(written.len(), 3, "user + assistant + synthesized results");
        let last = written.last().unwrap();
        assert!(matches!(last.role, MessageRole::User));
        assert_eq!(
            result_ids(last),
            vec![
                ("a".to_string(), CANCELLED_TOOL_RESULT.to_string(), Some(true)),
                ("b".to_string(), CANCELLED_TOOL_RESULT.to_string(), Some(true)),
            ]
        );
        assert_eq!(ctx.transcript_persisted_len, ctx.messages.len());
    }

    #[tokio::test]
    async fn flush_holds_back_unpaired_tail_and_skips_compacted() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("sys".into());
        let mut old = ChatMessage::user("old");
        old.compacted = true;
        ctx.messages = vec![old, ChatMessage::user("hi"), assistant_calls(&["a"])];

        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 2, "stops before the unpaired assistant");
        assert_eq!(delegate.written.lock().unwrap().len(), 1, "compacted message skipped");

        ctx.messages.push(ChatMessage::user_tool_result("a", "ok", false));
        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 4);
        assert_eq!(delegate.written.lock().unwrap().len(), 3);
    }

    struct FailingDelegate(RecordingDelegate);

    #[async_trait::async_trait]
    impl LoopDelegate for FailingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            self.0.check_signals().await
        }
        async fn before_llm_call(&self, c: &mut ReasoningContext, i: usize) -> Option<LoopOutcome> {
            self.0.before_llm_call(c, i).await
        }
        async fn call_llm(
            &self,
            c: &mut ReasoningContext,
            s: &TurnSnapshot,
            i: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            self.0.call_llm(c, s, i).await
        }
        async fn handle_text_response(
            &self,
            t: &str,
            m: ResponseMetadata,
            c: &mut ReasoningContext,
        ) -> TextAction {
            self.0.handle_text_response(t, m, c).await
        }
        async fn execute_tool_calls(
            &self,
            t: Vec<ToolCall>,
            c: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            self.0.execute_tool_calls(t, c).await
        }
        async fn persist_transcript(&self, _: &[ChatMessage]) -> anyhow::Result<()> {
            anyhow::bail!("db locked")
        }
    }

    #[tokio::test]
    async fn flush_failure_does_not_advance() {
        let delegate = FailingDelegate(RecordingDelegate::default());
        let mut ctx = ReasoningContext::new("sys".into());
        ctx.messages = vec![ChatMessage::user("hi")];
        flush_transcript(&delegate, &mut ctx).await;
        assert_eq!(ctx.transcript_persisted_len, 0);
    }
}
