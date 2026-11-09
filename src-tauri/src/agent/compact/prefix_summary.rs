// SPDX-License-Identifier: Apache-2.0
//! Context continuity P2 — prefix-reusing compaction summary.
//!
//! The summary request reuses exactly the main conversation request's tools,
//! system prompt and message list (`build_request_messages`), plus ONE
//! trailing user instruction. Every byte before that instruction is what the
//! previous main request already sent, so the provider's prompt cache is hit
//! and the summary costs little more than its output.
//!
//! Known, intentional difference vs. the main request (provider level, not
//! visible in the `ChatMessage` list built here): the Anthropic adapter
//! stamps message-level `cache_control` markers on the two most recent user
//! messages, so in the summary request they land on [instruction, main's
//! last user message] instead of [main's last user, 2nd-to-last user]. The
//! main request's newest breakpoint is still marked, so the cached prefix up
//! to it is read. The summary uses non-streaming `complete` (no `stream:
//! true`), which is not part of the cache key.
//!
//! A side request: nothing is streamed to the UI and nothing is persisted.
//! Any unusable reply (tool calls, empty text, non-JSON) → `None`, and the
//! caller falls back to the standalone summarizer.

use crate::agent::turn::TurnSnapshot;
use crate::agent::types::{ChatMessage, ContentBlock, ReasoningContext, RespondOutput};
use crate::llm::{CompletionConfig, LlmProvider};

use super::fold::StructuredFold;
use super::summarize::{fold_json_schema, parse_fold_from_text};

/// Characters of the first kept user message quoted in the instruction.
const PREVIEW_CHARS: usize = 80;

/// The first `PREVIEW_CHARS` characters of `messages[cut]`'s text — the
/// boundary the summary must stop at. `None` when `cut` is past the end
/// (summarise everything) or the message carries no text.
pub(crate) fn first_kept_preview(messages: &[ChatMessage], cut: usize) -> Option<String> {
    let text = messages.get(cut)?.content.iter().find_map(|b| match b {
        ContentBlock::Text { text } => Some(text.as_str()),
        _ => None,
    })?;
    let preview: String = text.trim().chars().take(PREVIEW_CHARS).collect();
    (!preview.is_empty()).then_some(preview)
}

/// The trailing user instruction of the prefix-reusing summary request.
/// `first_kept_preview = None` → summarise the whole conversation.
pub(crate) fn instruction(first_kept_preview: Option<&str>, target_tokens: usize) -> String {
    let scope = match first_kept_preview {
        Some(p) => format!(
            "Summarize ONLY the conversation BEFORE the user message that begins with \"{p}\" \
             (that message and everything after it stay verbatim in context — do not summarize them)."
        ),
        None => "Summarize the ENTIRE conversation above.".to_string(),
    };
    format!(
        "[Context compaction request — not a task message]\n\
         {scope}\n\
         Do NOT call any tools. Reply with ONLY one JSON object (no prose, no code fence), ~{target_tokens} tokens.\n\
         `user_requests` is the most important field: list EVERY requirement, decision and correction the user stated, \
         as short sentences keeping the user's own words. `current_state`: where the work stood right before the next step. \
         `failed_attempts[].resolution`: how an error was fixed (null if not).\n\n\
         {schema}",
        schema = fold_json_schema(),
    )
}

/// The full summary request messages: exactly what `call_llm` would send for
/// this context + snapshot, plus the trailing instruction.
pub(crate) fn build_request(
    reason_ctx: &ReasoningContext,
    snapshot: &TurnSnapshot,
    cut: usize,
    target_tokens: usize,
    model_supports_images: bool,
) -> Vec<ChatMessage> {
    let mut messages =
        crate::agent::dispatcher::build_request_messages(reason_ctx, snapshot, model_supports_images);
    let preview = first_kept_preview(&reason_ctx.messages, cut);
    messages.push(ChatMessage::user(&instruction(preview.as_deref(), target_tokens)));
    messages
}

/// Parse the summary reply. Tool calls, empty text and non-JSON → `None`.
pub(crate) fn parse_response(resp: &RespondOutput) -> Option<StructuredFold> {
    let text = match resp {
        RespondOutput::Text { text, .. } => text,
        RespondOutput::ToolCalls { .. } => {
            tracing::warn!("prefix_summary: model returned tool calls; falling back");
            return None;
        }
    };
    if text.trim().is_empty() {
        tracing::warn!("prefix_summary: empty reply; falling back");
        return None;
    }
    match parse_fold_from_text(text) {
        Ok(fold) => Some(fold),
        Err(e) => {
            tracing::warn!(error = %e, "prefix_summary: reply is not StructuredFold JSON; falling back");
            None
        }
    }
}

/// Run the prefix-reusing summary: same `tools` (`snapshot.tools`), system
/// and message prefix as the main request, plus one instruction message;
/// `config` must be the main request's config (same model / thinking).
/// Non-streaming side request — nothing reaches the UI or the transcript.
pub(crate) async fn summarize_with_prefix(
    llm: &dyn LlmProvider,
    config: &CompletionConfig,
    reason_ctx: &ReasoningContext,
    snapshot: &TurnSnapshot,
    cut: usize,
    target_tokens: usize,
    model_supports_images: bool,
) -> Option<StructuredFold> {
    let messages = build_request(reason_ctx, snapshot, cut, target_tokens, model_supports_images);
    let tools = (*snapshot.tools).clone();
    let started = std::time::Instant::now();
    let resp = match llm.complete(messages, tools, config).await {
        Ok(resp) => resp,
        Err(e) => {
            tracing::warn!(error = %e, "prefix_summary: request failed; falling back");
            return None;
        }
    };
    let usage = match &resp {
        RespondOutput::Text { metadata, .. } | RespondOutput::ToolCalls { metadata, .. } => {
            metadata.usage.clone().unwrap_or_default()
        }
    };
    tracing::info!(
        "prefix_summary cache_read={} input={} output={} ms={}",
        usage.cache_read_tokens,
        usage.input_tokens,
        usage.output_tokens,
        started.elapsed().as_millis()
    );
    parse_response(&resp)
}

#[cfg(test)]
pub(crate) mod test_support {
    use crate::agent::types::{ChatMessage, RespondOutput, ResponseMetadata, ToolCall, ToolDefinition};
    use crate::error::Error;
    use crate::llm::CompletionConfig;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    enum Reply {
        Text(String),
        ToolCall,
        Error,
    }

    /// LLM double: returns one scripted reply and records every request.
    pub(crate) struct ScriptedLlm {
        reply: Reply,
        calls: AtomicUsize,
        requests: Mutex<Vec<(Vec<ChatMessage>, Vec<ToolDefinition>, CompletionConfig)>>,
    }

    impl ScriptedLlm {
        fn new(reply: Reply) -> Arc<Self> {
            Arc::new(Self { reply, calls: AtomicUsize::new(0), requests: Mutex::new(Vec::new()) })
        }
        pub(crate) fn text(t: &str) -> Arc<Self> {
            Self::new(Reply::Text(t.to_string()))
        }
        pub(crate) fn tool_call() -> Arc<Self> {
            Self::new(Reply::ToolCall)
        }
        pub(crate) fn error() -> Arc<Self> {
            Self::new(Reply::Error)
        }
        pub(crate) fn calls(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }
        pub(crate) fn last_request(&self) -> (Vec<ChatMessage>, Vec<ToolDefinition>, CompletionConfig) {
            self.requests.lock().unwrap().last().cloned().expect("a request was sent")
        }
    }

    fn meta() -> ResponseMetadata {
        ResponseMetadata { model: "m".into(), finish_reason: None, usage: None }
    }

    #[async_trait::async_trait]
    impl crate::llm::LlmProvider for ScriptedLlm {
        async fn complete(
            &self,
            messages: Vec<ChatMessage>,
            tools: Vec<ToolDefinition>,
            config: &CompletionConfig,
        ) -> Result<RespondOutput, Error> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            self.requests.lock().unwrap().push((messages, tools, config.clone()));
            match &self.reply {
                Reply::Text(t) => Ok(RespondOutput::Text {
                    text: t.clone(),
                    thinking: None,
                    thinking_signature: None,
                    metadata: meta(),
                }),
                Reply::ToolCall => Ok(RespondOutput::ToolCalls {
                    tool_calls: vec![ToolCall { id: "c1".into(), name: "read".into(), arguments: serde_json::json!({}) }],
                    text: Some(r#"{"user_requests":["ignored"]}"#.into()),
                    thinking: None,
                    thinking_signature: None,
                    metadata: meta(),
                }),
                Reply::Error => Err(Error::Internal("boom".into())),
            }
        }

        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<
            Box<dyn futures::Stream<Item = Result<crate::agent::types::StreamDelta, Error>> + Send + Unpin>,
            Error,
        > {
            unimplemented!("prefix summary never streams")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_support::ScriptedLlm;
    use super::*;
    use crate::agent::dispatcher::build_request_messages;
    use crate::agent::turn::TurnSnapshot;
    use crate::agent::types::{ChatMessage, ContentBlock, MessageRole, ReasoningContext, ToolDefinition};
    use crate::llm::CompletionConfig;

    fn ctx() -> ReasoningContext {
        let mut ctx = ReasoningContext::new("base".into());
        ctx.messages = vec![
            ChatMessage::user("first request: build the parser"),
            ChatMessage::assistant_with_tool_use("t1", "read", serde_json::json!({"path": "a.rs"})),
            ChatMessage::user_tool_result("t1", "fn main() {}", false),
            ChatMessage::assistant("parser drafted"),
            ChatMessage::user("second request: now add error recovery to the parser please, and keep the public API stable for callers"),
            ChatMessage::assistant("working on it"),
            ChatMessage::user("third request"),
        ];
        ctx
    }

    fn snap() -> TurnSnapshot {
        TurnSnapshot {
            turn_index: 3,
            model: "main-model".into(),
            system_prompt: std::sync::Arc::new("SYSTEM PROMPT".into()),
            dynamic_context: "<dyn>now</dyn>".into(),
            tools: std::sync::Arc::new(vec![ToolDefinition {
                name: "read".into(),
                description: "read a file".into(),
                parameters: serde_json::json!({"type": "object"}),
            }]),
            force_text: false,
        }
    }

    fn config() -> CompletionConfig {
        CompletionConfig { model: "main-model".into(), max_tokens: 16_000, temperature: 0.7, thinking_enabled: false }
    }

    const GOOD: &str = r#"{"user_requests":["build the parser"],"current_state":"parser drafted"}"#;

    #[tokio::test]
    async fn prefix_summary_request_is_prefix_plus_instruction() {
        let ctx = ctx();
        let snap = snap();
        let llm = ScriptedLlm::text(GOOD);
        let fold = summarize_with_prefix(&*llm, &config(), &ctx, &snap, 4, 3_000, true).await.expect("fold");
        assert_eq!(fold.user_requests, vec!["build the parser"]);

        let (sent, tools, cfg) = llm.last_request();
        let main = build_request_messages(&ctx, &snap, true);
        assert_eq!(sent.len(), main.len() + 1);
        assert_eq!(
            serde_json::to_string(&sent[..main.len()]).unwrap(),
            serde_json::to_string(&main).unwrap(),
            "everything but the trailing instruction is byte-identical to the main request"
        );
        assert_eq!(serde_json::to_string(&tools).unwrap(), serde_json::to_string(&*snap.tools).unwrap());
        assert_eq!(serde_json::to_string(&cfg).unwrap(), serde_json::to_string(&config()).unwrap());
        assert!(matches!(sent[0].role, MessageRole::System));

        let last = sent.last().unwrap();
        assert!(matches!(last.role, MessageRole::User));
        let ContentBlock::Text { text } = &last.content[0] else { panic!("instruction is text") };
        let preview: String = "second request: now add error recovery to the parser please, and keep the public API stable for callers"
            .chars()
            .take(80)
            .collect();
        assert_eq!(text, &instruction(Some(&preview), 3_000));
        assert!(text.contains(&preview) && text.contains("3000"));
        assert!(!text.contains("keep the public API stable for callers"), "preview is cut at 80 chars");
    }

    #[tokio::test]
    async fn prefix_summary_cut_at_end_summarizes_everything() {
        let ctx = ctx();
        let llm = ScriptedLlm::text(GOOD);
        summarize_with_prefix(&*llm, &config(), &ctx, &snap(), ctx.messages.len(), 3_000, true).await.unwrap();
        let (sent, _, _) = llm.last_request();
        let ContentBlock::Text { text } = &sent.last().unwrap().content[0] else { panic!() };
        assert_eq!(text, &instruction(None, 3_000));
    }

    #[tokio::test]
    async fn prefix_summary_rejects_tool_calls_empty_and_bad_json() {
        for llm in [ScriptedLlm::tool_call(), ScriptedLlm::text("   "), ScriptedLlm::text("sorry, I can't")] {
            assert!(summarize_with_prefix(&*llm, &config(), &ctx(), &snap(), 4, 3_000, true).await.is_none());
            assert_eq!(llm.calls(), 1);
        }
        let err = ScriptedLlm::error();
        assert!(summarize_with_prefix(&*err, &config(), &ctx(), &snap(), 4, 3_000, true).await.is_none());
    }
}
