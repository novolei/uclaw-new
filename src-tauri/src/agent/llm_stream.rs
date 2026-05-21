//! Shared LLM streaming helper. Drives a provider stream to completion,
//! handling tiered-timeout retries via RetryBudget, and surfaces streaming
//! side-effects through the `StreamSink` trait so both the interactive
//! ChatDelegate (IPC-emitting) and the headless AutomationDelegate (no-op
//! sink) can reuse one implementation.

use crate::agent::retry::{AgentRetryEvent, BudgetDecision, RetryBudget};
use crate::agent::types::{ChatMessage, RespondOutput, ResponseMetadata, StreamDelta, ToolCall, ToolDefinition};
use crate::error::{Error, LlmError};
use crate::llm::stream_error::{classify_stream_error, StreamErrorKind};
use crate::llm::CompletionConfig;
use crate::llm::LlmProvider;
use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use std::time::Duration;

/// Bundle 27-B — max idle time between two streaming chunks before we
/// give up on the current stream and retry via the existing
/// RetryBudget. Targets the "silent-drop" failure mode of national
/// LLM providers (Kimi K series in particular) where the upstream
/// load balancer drops the long-lived HTTP connection without sending
/// a TCP FIN, leaving the stream consumer hung in `.next().await`
/// forever. 90s is generous enough that legitimate slow responses
/// (large prompts, deep reasoning) aren't false-positives, but short
/// enough that the user sees a recovery attempt rather than a
/// permanent hang.
const STREAM_IDLE_TIMEOUT: Duration = Duration::from_secs(90);

/// Side-effects a streaming completion produces. The interactive delegate
/// emits IPC events; the headless automation delegate uses `NoopSink`.
#[async_trait]
pub trait StreamSink: Send + Sync {
    fn on_text_delta(&self, text: &str);
    fn on_thinking(&self, thinking: &str);
    fn on_thinking_done(&self, duration_ms: u64);
    fn on_stream_reset(&self);
    fn on_retry_event(&self, event: AgentRetryEvent);
    /// Sleep for `delay`, returning `true` if the caller should abort
    /// (e.g. a stop flag was set during the sleep).
    async fn sleep_or_abort(&self, delay: Duration) -> bool;
}

/// A `StreamSink` that does nothing and never aborts on sleep. Used by the
/// headless automation path, which has no frontend to emit to.
pub struct NoopSink;

#[async_trait]
impl StreamSink for NoopSink {
    fn on_text_delta(&self, _text: &str) {}
    fn on_thinking(&self, _thinking: &str) {}
    fn on_thinking_done(&self, _duration_ms: u64) {}
    fn on_stream_reset(&self) {}
    fn on_retry_event(&self, _event: AgentRetryEvent) {}
    async fn sleep_or_abort(&self, delay: Duration) -> bool {
        tokio::time::sleep(delay).await;
        false
    }
}

/// Drive `llm.stream(...)` to a `RespondOutput`, retrying transient/stalled
/// failures via a fresh `RetryBudget::for_agent_loop()`. All streaming
/// side-effects go through `sink`.
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    config: &CompletionConfig,
    sink: &dyn StreamSink,
) -> Result<RespondOutput, Error> {
    let mut retry_budget = RetryBudget::for_agent_loop();
    'stream_attempt: loop {
        match llm.stream(messages.clone(), tools.clone(), config).await {
            Ok(mut stream) => {
                let mut full_text = String::new();
                let mut full_thinking = String::new();
                let mut full_thinking_signature: Option<String> = None;
                let mut tool_calls: Vec<ToolCall> = Vec::new();
                let mut current_tool: Option<(String, String, String)> = None;
                let mut thinking_started = false;
                let mut thinking_start_time: Option<std::time::Instant> = None;
                let mut metadata: Option<ResponseMetadata> = None;

                loop {
                    // Bundle 27-B — wrap each chunk wait in an idle timeout.
                    // National providers (Kimi K) sometimes drop the streaming
                    // connection silently; without this, we'd block here
                    // forever, leaving the user staring at a frozen agent.
                    let next_result = tokio::time::timeout(
                        STREAM_IDLE_TIMEOUT,
                        stream.next(),
                    )
                    .await;
                    let item_opt = match next_result {
                        Ok(opt) => opt,
                        Err(_elapsed) => {
                            // Idle timeout fired — synthesize a "stalled"
                            // error and route through the existing retry
                            // budget, same shape as a real Stalled error
                            // would take.
                            let synthetic_err: Error = LlmError::ApiRequestFailed(
                                format!(
                                    "stream idle > {}s (Bundle 27-B fallback)",
                                    STREAM_IDLE_TIMEOUT.as_secs()
                                ),
                            )
                            .into();
                            tracing::warn!(
                                idle_secs = STREAM_IDLE_TIMEOUT.as_secs(),
                                attempt = retry_budget.attempts(),
                                "[Bundle 27-B] LLM stream idle for too long — treating as stalled"
                            );
                            match retry_budget.next_delay() {
                                BudgetDecision::Sleep(delay) => {
                                    let reason = format!(
                                        "stream idle > {}s",
                                        STREAM_IDLE_TIMEOUT.as_secs()
                                    );
                                    sink.on_stream_reset();
                                    sink.on_retry_event(AgentRetryEvent::Starting {
                                        attempt: retry_budget.attempts(),
                                        max_attempts: retry_budget.max_attempts(),
                                        delay_seconds: delay.as_secs_f64(),
                                        reason: reason.clone(),
                                    });
                                    if sink.sleep_or_abort(delay).await {
                                        sink.on_stream_reset();
                                        return Err(synthetic_err);
                                    }
                                    sink.on_retry_event(AgentRetryEvent::Attempt {
                                        attempt: retry_budget.attempts(),
                                        timestamp_ms: Utc::now().timestamp_millis(),
                                        reason,
                                    });
                                    continue 'stream_attempt;
                                }
                                BudgetDecision::Exhausted => {
                                    tracing::error!(
                                        attempts = retry_budget.attempts(),
                                        elapsed_wait_ms = retry_budget.elapsed_wait().as_millis() as u64,
                                        "[Bundle 27-B] stream idle exhausted retry budget"
                                    );
                                    sink.on_stream_reset();
                                    sink.on_retry_event(AgentRetryEvent::Exhausted {
                                        total_attempts: retry_budget.attempts(),
                                        total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                    });
                                    return Err(synthetic_err);
                                }
                            }
                        }
                    };
                    let Some(item) = item_opt else { break };
                    match item {
                        Ok(StreamDelta::TextDelta { text }) => {
                            if thinking_started {
                                thinking_started = false;
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            sink.on_text_delta(&text);
                            full_text.push_str(&text);
                        }
                        Ok(StreamDelta::ThinkingDelta { thinking }) => {
                            if !thinking_started {
                                thinking_started = true;
                                thinking_start_time = Some(std::time::Instant::now());
                            }
                            sink.on_thinking(&thinking);
                            full_thinking.push_str(&thinking);
                        }
                        Ok(StreamDelta::SignatureDelta { signature }) => {
                            full_thinking_signature = Some(signature);
                        }
                        Ok(StreamDelta::ToolCallDelta { id, name, input_json }) => {
                            if thinking_started {
                                thinking_started = false;
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            if let Some(n) = name {
                                if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                    if let Ok(args) = serde_json::from_str(&tc_args) {
                                        tool_calls.push(ToolCall { id: tc_id, name: tc_name, arguments: args });
                                    }
                                }
                                current_tool = Some((id, n, String::new()));
                            }
                            if let Some(args) = input_json {
                                if let Some((_, _, ref mut tc_args)) = current_tool {
                                    tc_args.push_str(&args);
                                }
                            }
                        }
                        Ok(StreamDelta::Done { finish_reason, usage }) => {
                            if thinking_started {
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                if let Ok(args) = serde_json::from_str(&tc_args) {
                                    tool_calls.push(ToolCall { id: tc_id, name: tc_name, arguments: args });
                                }
                            }
                            metadata = Some(ResponseMetadata {
                                model: config.model.clone(),
                                finish_reason,
                                usage,
                            });
                            let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };
                            let meta = metadata.unwrap();
                            if !tool_calls.is_empty() {
                                return Ok(RespondOutput::ToolCalls {
                                    tool_calls,
                                    text: if full_text.is_empty() { None } else { Some(full_text) },
                                    thinking,
                                    thinking_signature: full_thinking_signature,
                                    metadata: meta,
                                });
                            } else {
                                return Ok(RespondOutput::Text {
                                    text: full_text,
                                    thinking,
                                    thinking_signature: full_thinking_signature,
                                    metadata: meta,
                                });
                            }
                        }
                        Err(e) => {
                            let kind = classify_stream_error(&e);
                            match kind {
                                StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                    match retry_budget.next_delay() {
                                        BudgetDecision::Sleep(delay) => {
                                            let reason = format!("{:?}: {}", kind, e);
                                            tracing::warn!(error = %e, kind = ?kind,
                                                attempt = retry_budget.attempts(),
                                                max = retry_budget.max_attempts(),
                                                delay_ms = delay.as_millis() as u64,
                                                "Stream interrupted, retrying with a fresh stream");
                                            sink.on_stream_reset();
                                            sink.on_retry_event(AgentRetryEvent::Starting {
                                                attempt: retry_budget.attempts(),
                                                max_attempts: retry_budget.max_attempts(),
                                                delay_seconds: delay.as_secs_f64(),
                                                reason: reason.clone(),
                                            });
                                            if sink.sleep_or_abort(delay).await {
                                                sink.on_stream_reset();
                                                return Err(e);
                                            }
                                            sink.on_retry_event(AgentRetryEvent::Attempt {
                                                attempt: retry_budget.attempts(),
                                                timestamp_ms: Utc::now().timestamp_millis(),
                                                reason,
                                            });
                                            continue 'stream_attempt;
                                        }
                                        BudgetDecision::Exhausted => {
                                            tracing::error!(error = %e,
                                                attempts = retry_budget.attempts(),
                                                elapsed_wait_ms = retry_budget.elapsed_wait().as_millis() as u64,
                                                "Stream failed after exhausting retry budget");
                                            sink.on_stream_reset();
                                            sink.on_retry_event(AgentRetryEvent::Exhausted {
                                                total_attempts: retry_budget.attempts(),
                                                total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                            });
                                            return Err(e);
                                        }
                                    }
                                }
                                StreamErrorKind::Fatal => {
                                    tracing::error!(error = %e, "Stream failed with fatal error");
                                    sink.on_stream_reset();
                                    return Err(e);
                                }
                            }
                        }
                    }
                }

                // Stream ended without a Done delta.
                let meta = metadata.unwrap_or_else(|| ResponseMetadata {
                    model: config.model.clone(),
                    finish_reason: Some("stream_ended".into()),
                    usage: None,
                });
                let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };
                if !tool_calls.is_empty() {
                    return Ok(RespondOutput::ToolCalls {
                        tool_calls,
                        text: if full_text.is_empty() { None } else { Some(full_text) },
                        thinking,
                        thinking_signature: full_thinking_signature,
                        metadata: meta,
                    });
                } else {
                    return Ok(RespondOutput::Text {
                        text: full_text,
                        thinking,
                        thinking_signature: full_thinking_signature,
                        metadata: meta,
                    });
                }
            }
            Err(e) => {
                let kind = classify_stream_error(&e);
                match kind {
                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                        match retry_budget.next_delay() {
                            BudgetDecision::Sleep(delay) => {
                                let reason = format!("setup {:?}: {}", kind, e);
                                tracing::warn!(error = %e, kind = ?kind,
                                    attempt = retry_budget.attempts(),
                                    max = retry_budget.max_attempts(),
                                    delay_ms = delay.as_millis() as u64,
                                    "Stream setup failed transiently, retrying");
                                sink.on_retry_event(AgentRetryEvent::Starting {
                                    attempt: retry_budget.attempts(),
                                    max_attempts: retry_budget.max_attempts(),
                                    delay_seconds: delay.as_secs_f64(),
                                    reason: reason.clone(),
                                });
                                if sink.sleep_or_abort(delay).await {
                                    return Err(e);
                                }
                                sink.on_retry_event(AgentRetryEvent::Attempt {
                                    attempt: retry_budget.attempts(),
                                    timestamp_ms: Utc::now().timestamp_millis(),
                                    reason,
                                });
                                continue 'stream_attempt;
                            }
                            BudgetDecision::Exhausted => {
                                tracing::error!(error = %e, attempts = retry_budget.attempts(),
                                    "Stream setup failed after exhausting retry budget");
                                sink.on_retry_event(AgentRetryEvent::Exhausted {
                                    total_attempts: retry_budget.attempts(),
                                    total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                });
                                return Err(e);
                            }
                        }
                    }
                    StreamErrorKind::Fatal => {
                        tracing::error!(error = %e, "Stream setup failed, surfacing error");
                        return Err(e);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{StreamDelta, TokenUsage};
    use futures::stream;
    use std::sync::Mutex;

    /// A fake LlmProvider that yields a scripted list of StreamDeltas.
    struct ScriptedProvider {
        deltas: Vec<StreamDelta>,
    }

    impl ScriptedProvider {
        fn new(deltas: Vec<StreamDelta>) -> Self {
            Self { deltas }
        }
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn complete(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<RespondOutput, Error> {
            unimplemented!()
        }

        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error> {
            let items: Vec<Result<StreamDelta, Error>> =
                self.deltas.iter().cloned().map(Ok).collect();
            Ok(Box::new(stream::iter(items)))
        }
    }

    /// A sink that records what it received.
    #[derive(Default)]
    struct RecordingSink {
        text: Mutex<String>,
        thinking: Mutex<String>,
    }
    #[async_trait]
    impl StreamSink for RecordingSink {
        fn on_text_delta(&self, t: &str) { self.text.lock().unwrap().push_str(t); }
        fn on_thinking(&self, t: &str) { self.thinking.lock().unwrap().push_str(t); }
        fn on_thinking_done(&self, _d: u64) {}
        fn on_stream_reset(&self) {}
        fn on_retry_event(&self, _e: AgentRetryEvent) {}
        async fn sleep_or_abort(&self, _d: Duration) -> bool { false }
    }

    fn cfg() -> CompletionConfig {
        CompletionConfig {
            model: "claude-sonnet-4-6".into(),
            max_tokens: 8192,
            temperature: 0.7,
            thinking_enabled: false,
        }
    }

    #[tokio::test]
    async fn text_response_assembles_full_text_and_emits_deltas() {
        let provider = ScriptedProvider::new(vec![
            StreamDelta::TextDelta { text: "Hello ".into() },
            StreamDelta::TextDelta { text: "world".into() },
            StreamDelta::Done {
                finish_reason: Some("stop".into()),
                usage: Some(TokenUsage { input_tokens: 10, output_tokens: 5, ..Default::default() }),
            },
        ]);
        let sink = RecordingSink::default();
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink).await.unwrap();
        match out {
            RespondOutput::Text { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected Text, got {:?}", other),
        }
        assert_eq!(*sink.text.lock().unwrap(), "Hello world");
    }

    #[tokio::test]
    async fn tool_call_delta_assembles_tool_calls() {
        let provider = ScriptedProvider::new(vec![
            StreamDelta::ToolCallDelta {
                id: "tc1".into(),
                name: Some("bash".into()),
                input_json: Some(r#"{"command":"ls"}"#.into()),
            },
            StreamDelta::Done { finish_reason: Some("tool_use".into()), usage: None },
        ]);
        let sink = RecordingSink::default();
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink).await.unwrap();
        match out {
            RespondOutput::ToolCalls { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "bash");
            }
            other => panic!("expected ToolCalls, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn stream_ending_without_done_uses_stream_ended_fallback() {
        let provider = ScriptedProvider::new(vec![
            StreamDelta::TextDelta { text: "partial".into() },
            // no Done delta — stream just ends
        ]);
        let sink = RecordingSink::default();
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink).await.unwrap();
        match out {
            RespondOutput::Text { text, metadata, .. } => {
                assert_eq!(text, "partial");
                assert_eq!(metadata.finish_reason.as_deref(), Some("stream_ended"));
            }
            other => panic!("expected Text, got {:?}", other),
        }
    }
}
