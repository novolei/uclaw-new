use async_trait::async_trait;
use futures::stream::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use crate::agent::types::*;
use crate::error::Error;
use crate::llm::provider::{CompletionConfig, LlmProvider};

/// Default request timeout in seconds.
const DEFAULT_TIMEOUT_SECS: u64 = 60;
/// Maximum number of retry attempts.
const MAX_RETRIES: u32 = 3;

pub struct AnthropicProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl AnthropicProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let raw = base_url.unwrap_or_else(|| "https://api.anthropic.com".into());
        let base = normalize_base_url(&raw);
        Self {
            api_key,
            base_url: base,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<serde_json::Value> {
        messages
            .iter()
            .filter_map(|m| {
                let role = match m.role {
                    MessageRole::System => return None,
                    MessageRole::User => "user",
                    MessageRole::Assistant => "assistant",
                };
                let content = self.convert_content(&m.content);
                Some(serde_json::json!({ "role": role, "content": content }))
            })
            .collect()
    }

    fn convert_content(&self, blocks: &[ContentBlock]) -> serde_json::Value {
        if blocks.len() == 1 {
            if let ContentBlock::Text { text } = &blocks[0] {
                return serde_json::json!(text);
            }
        }
        let content: Vec<serde_json::Value> = blocks
            .iter()
            .map(|b| match b {
                ContentBlock::Text { text } => {
                    serde_json::json!({"type": "text", "text": text})
                }
                ContentBlock::Thinking { thinking } => {
                    serde_json::json!({"type": "thinking", "thinking": thinking})
                }
                ContentBlock::ToolUse { id, name, input } => {
                    serde_json::json!({"type": "tool_use", "id": id, "name": name, "input": input})
                }
                ContentBlock::ToolResult {
                    tool_use_id,
                    content,
                    is_error,
                } => {
                    let mut val = serde_json::json!({"type": "tool_result", "tool_use_id": tool_use_id, "content": content});
                    if let Some(e) = is_error {
                        val["is_error"] = serde_json::json!(e);
                    }
                    val
                }
            })
            .collect();
        serde_json::json!(content)
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "name": t.name,
                    "description": t.description,
                    "input_schema": t.parameters,
                })
            })
            .collect()
    }

    fn build_request_body(
        &self,
        messages: &[ChatMessage],
        tools: &[ToolDefinition],
        config: &CompletionConfig,
        stream: bool,
    ) -> serde_json::Value {
        let system = messages
            .iter()
            .find(|m| m.role == MessageRole::System)
            .and_then(|m| {
                if let Some(ContentBlock::Text { text }) = m.content.first() {
                    Some(text.clone())
                } else {
                    None
                }
            });

        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "messages": self.convert_messages(messages),
        });

        if let Some(sys) = &system {
            body["system"] = serde_json::json!(sys);
        }
        if !tools.is_empty() {
            body["tools"] = serde_json::json!(self.convert_tools(tools));
        }
        if stream {
            body["stream"] = serde_json::json!(true);
        }
        if config.thinking_enabled {
            body["thinking"] = serde_json::json!({
                "type": "enabled",
                "budget_tokens": 8000
            });
            // Extended thinking requires temperature=1
            body["temperature"] = serde_json::json!(1.0);
            // Signal to send_with_retry to add the beta header
            body["_betas"] = serde_json::json!(["interleaved-thinking-2025-05-14"]);
        }
        body
    }

    /// Send a request with exponential backoff retry.
    async fn send_with_retry(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, Error> {
        let mut last_error = None;

        // Extract and remove _betas sentinel field before sending
        let mut body = body.clone();
        let betas = body["_betas"].take();
        let beta_header: Option<String> = if betas.is_array() {
            let beta_str = betas.as_array().unwrap().iter()
                .filter_map(|b| b.as_str())
                .collect::<Vec<_>>()
                .join(",");
            if beta_str.is_empty() { None } else { Some(beta_str) }
        } else {
            None
        };

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::info!(attempt, delay_ms = delay.as_millis(), "Retrying Anthropic request");
                tokio::time::sleep(delay).await;
            }

            let mut request = self
                .client
                .post(format!("{}/v1/messages", self.base_url))
                .header("x-api-key", &self.api_key)
                .header("anthropic-version", "2023-06-01")
                .header("content-type", "application/json")
                .timeout(self.timeout);

            if let Some(ref beta_str) = beta_header {
                request = request.header("anthropic-beta", beta_str);
            }

            let result = request.json(&body).send().await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    // Retry on 429 (rate limit) or 5xx (server error)
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error()
                    {
                        tracing::warn!(
                            status = status.as_u16(),
                            attempt,
                            "Anthropic retryable error"
                        );
                        last_error = Some(Error::Internal(format!(
                            "Anthropic API returned status {}",
                            status
                        )));
                        continue;
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if e.is_timeout() {
                        tracing::warn!(attempt, "Anthropic request timed out");
                        last_error = Some(Error::Internal("Anthropic request timed out".into()));
                    } else if e.is_connect() {
                        tracing::warn!(attempt, "Anthropic connection error: {}", e);
                        last_error =
                            Some(Error::Internal(format!("Anthropic connection error: {}", e)));
                    } else {
                        // Non-retryable network error
                        return Err(Error::Internal(format!("Anthropic request failed: {}", e)));
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Internal("Anthropic request failed after retries".into())))
    }
}

#[async_trait]
impl LlmProvider for AnthropicProvider {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<RespondOutput, Error> {
        let body = self.build_request_body(&messages, &tools, config, false);

        tracing::debug!(model = %config.model, "Anthropic complete request");

        let resp = self.send_with_retry(&body).await?;
        let status = resp.status();
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Internal(format!("Anthropic response parse failed: {}", e)))?;

        if !status.is_success() {
            let err_msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(Error::Internal(format!("Anthropic API error: {}", err_msg)));
        }

        let metadata = ResponseMetadata {
            model: json["model"]
                .as_str()
                .unwrap_or(&config.model)
                .to_string(),
            finish_reason: json["stop_reason"].as_str().map(|s| s.to_string()),
            usage: json["usage"].as_object().map(|u| TokenUsage {
                input_tokens: u["input_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["output_tokens"].as_u64().unwrap_or(0) as u32,
                cache_read_tokens: u.get("cache_read_input_tokens")
                    .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                cache_creation_tokens: u.get("cache_creation_input_tokens")
                    .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
            }),
        };

        let mut tool_calls = Vec::new();
        let mut text_parts = Vec::new();
        let mut thinking_parts = Vec::new();

        if let Some(blocks) = json["content"].as_array() {
            for block in blocks {
                match block["type"].as_str() {
                    Some("text") => {
                        if let Some(t) = block["text"].as_str() {
                            text_parts.push(t.to_string());
                        }
                    }
                    Some("thinking") => {
                        if let Some(t) = block["thinking"].as_str() {
                            thinking_parts.push(t.to_string());
                        }
                    }
                    Some("tool_use") => {
                        tool_calls.push(ToolCall {
                            id: block["id"].as_str().unwrap_or("").to_string(),
                            name: block["name"].as_str().unwrap_or("").to_string(),
                            arguments: block["input"].clone(),
                        });
                    }
                    _ => {}
                }
            }
        }

        let thinking = if thinking_parts.is_empty() {
            None
        } else {
            Some(thinking_parts.join("\n"))
        };

        if !tool_calls.is_empty() {
            Ok(RespondOutput::ToolCalls {
                tool_calls,
                text: if text_parts.is_empty() {
                    None
                } else {
                    Some(text_parts.join("\n"))
                },
                thinking,
                metadata,
            })
        } else {
            Ok(RespondOutput::Text {
                text: text_parts.join("\n"),
                thinking,
                metadata,
            })
        }
    }

    async fn stream(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<Box<dyn Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error> {
        let body = self.build_request_body(&messages, &tools, config, true);

        tracing::debug!(model = %config.model, "Anthropic stream request");

        let resp = self.send_with_retry(&body).await?;
        let status = resp.status();

        if !status.is_success() {
            let json: serde_json::Value = resp
                .json()
                .await
                .unwrap_or_else(|_| serde_json::json!({"error": {"message": "Unknown error"}}));
            let err_msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(Error::Internal(format!("Anthropic API error: {}", err_msg)));
        }

        let byte_stream = resp.bytes_stream();
        let stream = AnthropicSseStream::new(byte_stream);
        Ok(Box::new(stream))
    }
}

// ─── SSE Stream Implementation ──────────────────────────────────────────────

/// Parses Anthropic SSE events into StreamDelta items.
struct AnthropicSseStream {
    inner: Pin<Box<dyn Stream<Item = Result<StreamDelta, Error>> + Send>>,
}

impl AnthropicSseStream {
    fn new(byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static) -> Self {
        let stream = futures::stream::unfold(
            SseParserState::new(byte_stream),
            |mut state| async move {
                loop {
                    match state.next_delta().await {
                        Some(Ok(delta)) => return Some((Ok(delta), state)),
                        Some(Err(e)) => return Some((Err(e), state)),
                        None => return None,
                    }
                }
            },
        );

        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Stream for AnthropicSseStream {
    type Item = Result<StreamDelta, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Unpin for AnthropicSseStream {}

/// Internal state for parsing SSE from Anthropic's byte stream.
struct SseParserState {
    byte_stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    /// Tracks the current content_block index → (block_type, tool_id, tool_name)
    current_tool_id: Option<String>,
    current_tool_name: Option<String>,
    /// Whether the current content block is a thinking block
    current_block_is_thinking: bool,
    /// Accumulated usage from message_start and message_delta events
    accumulated_usage: Option<TokenUsage>,
    done: bool,
}

impl SseParserState {
    fn new(byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static) -> Self {
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            current_tool_id: None,
            current_tool_name: None,
            current_block_is_thinking: false,
            accumulated_usage: None,
            done: false,
        }
    }

    async fn next_delta(&mut self) -> Option<Result<StreamDelta, Error>> {
        use futures::StreamExt;

        if self.done {
            return None;
        }

        loop {
            // Try to extract a complete SSE event from buffer
            if let Some(event) = self.extract_event() {
                match self.parse_event(&event) {
                    EventResult::Delta(delta) => return Some(Ok(delta)),
                    EventResult::Done(reason) => {
                        self.done = true;
                        return Some(Ok(StreamDelta::Done {
                            finish_reason: reason,
                            usage: self.accumulated_usage.take(),
                        }));
                    }
                    EventResult::Skip => continue,
                    EventResult::Error(e) => return Some(Err(e)),
                }
            }

            // Need more data
            match self.byte_stream.next().await {
                Some(Ok(bytes)) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.buffer.push_str(&text);
                }
                Some(Err(e)) => {
                    self.done = true;
                    return Some(Err(Error::Internal(format!(
                        "Anthropic stream read error: {}",
                        e
                    ))));
                }
                None => {
                    // Stream ended without a message_stop
                    if !self.done {
                        self.done = true;
                        return Some(Ok(StreamDelta::Done {
                            finish_reason: Some("stream_ended".into()),
                            usage: self.accumulated_usage.take(),
                        }));
                    }
                    return None;
                }
            }
        }
    }

    /// Try to extract one complete SSE event from the buffer.
    /// SSE events are separated by double newlines.
    fn extract_event(&mut self) -> Option<SseEvent> {
        // Look for event boundary (double newline)
        let boundary = if let Some(pos) = self.buffer.find("\n\n") {
            pos
        } else if let Some(pos) = self.buffer.find("\r\n\r\n") {
            pos
        } else {
            return None;
        };

        let event_text: String = self.buffer.drain(..boundary).collect();
        // Remove the delimiter efficiently using drain instead of O(n²) remove(0)
        let skip = self.buffer.len() - self.buffer.trim_start_matches(['\n', '\r']).len();
        if skip > 0 {
            self.buffer.drain(..skip);
        }

        let mut event_type = String::new();
        let mut data = String::new();

        for line in event_text.lines() {
            if let Some(val) = line.strip_prefix("event: ") {
                event_type = val.trim().to_string();
            } else if let Some(val) = line.strip_prefix("data: ") {
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(val);
            } else if line.starts_with("event:") {
                event_type = line["event:".len()..].trim().to_string();
            } else if line.starts_with("data:") {
                let val = line["data:".len()..].trim();
                if !data.is_empty() {
                    data.push('\n');
                }
                data.push_str(val);
            }
        }

        Some(SseEvent { event_type, data })
    }

    fn parse_event(&mut self, event: &SseEvent) -> EventResult {
        match event.event_type.as_str() {
            "message_start" => {
                // Extract initial usage (input_tokens) from message_start
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    if let Some(u) = json["message"]["usage"].as_object() {
                        let usage = TokenUsage {
                            input_tokens: u.get("input_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            output_tokens: u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            cache_read_tokens: u.get("cache_read_input_tokens")
                                .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                            cache_creation_tokens: u.get("cache_creation_input_tokens")
                                .and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                        };
                        self.accumulated_usage = Some(usage);
                    }
                }
                EventResult::Skip
            }
            "ping" => EventResult::Skip,

            "content_block_start" => {
                // Parse the content block to identify tool_use and thinking blocks
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    let block = &json["content_block"];
                    match block["type"].as_str() {
                        Some("tool_use") => {
                            self.current_block_is_thinking = false;
                            self.current_tool_id =
                                block["id"].as_str().map(|s| s.to_string());
                            self.current_tool_name =
                                block["name"].as_str().map(|s| s.to_string());
                            // Emit the start of tool call
                            return EventResult::Delta(StreamDelta::ToolCallDelta {
                                id: self.current_tool_id.clone().unwrap_or_default(),
                                name: self.current_tool_name.clone(),
                                input_json: None,
                            });
                        }
                        Some("thinking") => {
                            self.current_block_is_thinking = true;
                        }
                        _ => {
                            self.current_block_is_thinking = false;
                        }
                    }
                }
                EventResult::Skip
            }

            "content_block_delta" => {
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    let delta = &json["delta"];
                    match delta["type"].as_str() {
                        Some("text_delta") => {
                            if let Some(text) = delta["text"].as_str() {
                                return EventResult::Delta(StreamDelta::TextDelta {
                                    text: text.to_string(),
                                });
                            }
                        }
                        Some("thinking_delta") => {
                            if let Some(thinking) = delta["thinking"].as_str() {
                                return EventResult::Delta(StreamDelta::ThinkingDelta {
                                    thinking: thinking.to_string(),
                                });
                            }
                        }
                        Some("input_json_delta") => {
                            if let Some(partial) = delta["partial_json"].as_str() {
                                return EventResult::Delta(StreamDelta::ToolCallDelta {
                                    id: self.current_tool_id.clone().unwrap_or_default(),
                                    name: None,
                                    input_json: Some(partial.to_string()),
                                });
                            }
                        }
                        _ => {}
                    }
                }
                EventResult::Skip
            }

            "content_block_stop" => {
                // Reset tool and thinking tracking on block end
                self.current_tool_id = None;
                self.current_tool_name = None;
                self.current_block_is_thinking = false;
                EventResult::Skip
            }

            "message_delta" => {
                // Contains stop_reason and usage (output_tokens)
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.data) {
                    // Merge output_tokens from message_delta usage
                    if let Some(u) = json["usage"].as_object() {
                        let output_tokens = u.get("output_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                        if let Some(ref mut acc) = self.accumulated_usage {
                            acc.output_tokens = output_tokens;
                        } else {
                            self.accumulated_usage = Some(TokenUsage {
                                input_tokens: 0,
                                output_tokens,
                                cache_read_tokens: 0,
                                cache_creation_tokens: 0,
                            });
                        }
                    }
                    let stop_reason = json["delta"]["stop_reason"]
                        .as_str()
                        .map(|s| s.to_string());
                    if stop_reason.is_some() {
                        return EventResult::Done(stop_reason);
                    }
                }
                EventResult::Skip
            }

            "message_stop" => EventResult::Done(Some("end_turn".into())),

            "error" => {
                let msg = if let Ok(json) = serde_json::from_str::<serde_json::Value>(&event.data)
                {
                    json["error"]["message"]
                        .as_str()
                        .unwrap_or("Unknown stream error")
                        .to_string()
                } else {
                    "Unknown stream error".to_string()
                };
                EventResult::Error(Error::Internal(format!("Anthropic stream error: {}", msg)))
            }

            _ => EventResult::Skip,
        }
    }
}

struct SseEvent {
    event_type: String,
    data: String,
}

enum EventResult {
    Delta(StreamDelta),
    Done(Option<String>),
    Skip,
    Error(Error),
}

/// Strip trailing `/v1` from a base URL.
fn normalize_base_url(raw: &str) -> String {
    let trimmed = raw.trim_end_matches('/');
    if trimmed.len() >= 3 {
        let suffix = &trimmed[trimmed.len() - 3..];
        if suffix.eq_ignore_ascii_case("/v1") {
            return trimmed[..trimmed.len() - 3].to_string();
        }
    }
    trimmed.to_string()
}
