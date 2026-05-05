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

pub struct OpenAIProvider {
    api_key: String,
    base_url: String,
    client: reqwest::Client,
    timeout: Duration,
}

impl OpenAIProvider {
    pub fn new(api_key: String, base_url: Option<String>) -> Self {
        let raw = base_url.unwrap_or_else(|| "https://api.openai.com".into());
        let base = normalize_base_url(&raw);
        Self {
            api_key,
            base_url: base,
            client: reqwest::Client::new(),
            timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECS),
        }
    }

    /// Convert internal ChatMessage list to OpenAI API message format.
    ///
    /// Key differences from internal representation:
    /// - ToolUse blocks become `tool_calls` array on assistant messages (not content parts)
    /// - ToolResult blocks become separate `role: "tool"` messages
    /// - Content parts only include `text` and `image_url` types
    fn convert_messages(&self, messages: &[ChatMessage]) -> Vec<serde_json::Value> {
        let mut openai_messages: Vec<serde_json::Value> = Vec::new();

        for m in messages {
            match m.role {
                MessageRole::System => {
                    let text = self.extract_text_content(&m.content);
                    openai_messages.push(serde_json::json!({
                        "role": "system",
                        "content": text
                    }));
                }
                MessageRole::User => {
                    // User messages may contain ToolResult blocks (from our internal format).
                    // Split them out as separate role="tool" messages first,
                    // then emit the user text (if any).
                    let mut has_text = false;
                    for block in &m.content {
                        match block {
                            ContentBlock::ToolResult { tool_use_id, content, .. } => {
                                openai_messages.push(serde_json::json!({
                                    "role": "tool",
                                    "tool_call_id": tool_use_id,
                                    "content": content
                                }));
                            }
                            ContentBlock::Text { .. } => {
                                has_text = true;
                            }
                            _ => {}
                        }
                    }
                    if has_text {
                        let text = self.extract_text_content(&m.content);
                        openai_messages.push(serde_json::json!({
                            "role": "user",
                            "content": text
                        }));
                    }
                }
                MessageRole::Assistant => {
                    let text = self.extract_text_content(&m.content);
                    let thinking = self.extract_thinking_content(&m.content);
                    let tool_calls: Vec<serde_json::Value> = m
                        .content
                        .iter()
                        .filter_map(|b| {
                            if let ContentBlock::ToolUse { id, name, input } = b {
                                Some(serde_json::json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": serde_json::to_string(input).unwrap_or_default()
                                    }
                                }))
                            } else {
                                None
                            }
                        })
                        .collect();

                    if tool_calls.is_empty() {
                        let mut msg = serde_json::json!({
                            "role": "assistant",
                            "content": text
                        });
                        if let Some(ref t) = thinking {
                            msg["reasoning_content"] = serde_json::json!(t);
                        }
                        openai_messages.push(msg);
                    } else {
                        let mut msg = serde_json::json!({
                            "role": "assistant",
                            "tool_calls": tool_calls
                        });
                        if !text.is_empty() {
                            msg["content"] = serde_json::json!(text);
                        }
                        if let Some(ref t) = thinking {
                            msg["reasoning_content"] = serde_json::json!(t);
                        }
                        openai_messages.push(msg);
                    }
                }
            }
        }

        tracing::debug!(message_count = openai_messages.len(), "Converted messages to OpenAI format");
        openai_messages
    }

    /// Extract only text content from blocks, ignoring ToolUse, ToolResult, and Thinking.
    fn extract_text_content(&self, blocks: &[ContentBlock]) -> String {
        blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.as_str())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Extract thinking/reasoning content from blocks.
    fn extract_thinking_content(&self, blocks: &[ContentBlock]) -> Option<String> {
        let parts: Vec<&str> = blocks
            .iter()
            .filter_map(|b| {
                if let ContentBlock::Thinking { thinking } = b {
                    Some(thinking.as_str())
                } else {
                    None
                }
            })
            .collect();
        if parts.is_empty() {
            None
        } else {
            Some(parts.join("\n"))
        }
    }

    fn convert_tools(&self, tools: &[ToolDefinition]) -> Vec<serde_json::Value> {
        tools
            .iter()
            .map(|t| {
                serde_json::json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.parameters,
                    }
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
        let mut body = serde_json::json!({
            "model": config.model,
            "max_tokens": config.max_tokens,
            "temperature": config.temperature,
            "messages": self.convert_messages(messages),
        });

        if !tools.is_empty() {
            body["tools"] = serde_json::json!(self.convert_tools(tools));
            body["tool_choice"] = serde_json::json!("auto");
        }
        if stream {
            body["stream"] = serde_json::json!(true);
            // Include usage in stream for token counting
            body["stream_options"] = serde_json::json!({"include_usage": true});
        }
        body
    }

    /// Send a request with exponential backoff retry.
    async fn send_with_retry(
        &self,
        body: &serde_json::Value,
    ) -> Result<reqwest::Response, Error> {
        let mut last_error = None;

        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let delay = Duration::from_millis(500 * 2u64.pow(attempt - 1));
                tracing::info!(attempt, delay_ms = delay.as_millis(), "Retrying OpenAI request");
                tokio::time::sleep(delay).await;
            }

            let result = self
                .client
                .post(format!("{}/v1/chat/completions", self.base_url))
                .header("Authorization", format!("Bearer {}", self.api_key))
                .header("content-type", "application/json")
                .timeout(self.timeout)
                .json(body)
                .send()
                .await;

            match result {
                Ok(resp) => {
                    let status = resp.status();
                    if status == reqwest::StatusCode::TOO_MANY_REQUESTS
                        || status.is_server_error()
                    {
                        tracing::warn!(
                            status = status.as_u16(),
                            attempt,
                            "OpenAI retryable error"
                        );
                        last_error = Some(Error::Internal(format!(
                            "OpenAI API returned status {}",
                            status
                        )));
                        continue;
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if e.is_timeout() {
                        tracing::warn!(attempt, "OpenAI request timed out");
                        last_error = Some(Error::Internal("OpenAI request timed out".into()));
                    } else if e.is_connect() {
                        tracing::warn!(attempt, "OpenAI connection error: {}", e);
                        last_error =
                            Some(Error::Internal(format!("OpenAI connection error: {}", e)));
                    } else {
                        return Err(Error::Internal(format!("OpenAI request failed: {}", e)));
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::Internal("OpenAI request failed after retries".into())))
    }
}

#[async_trait]
impl LlmProvider for OpenAIProvider {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<RespondOutput, Error> {
        let body = self.build_request_body(&messages, &tools, config, false);

        tracing::debug!(model = %config.model, "OpenAI complete request");

        let resp = self.send_with_retry(&body).await?;
        let status = resp.status();
        let json: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| Error::Internal(format!("OpenAI response parse failed: {}", e)))?;

        if !status.is_success() {
            let err_msg = json["error"]["message"]
                .as_str()
                .unwrap_or("Unknown error");
            return Err(Error::Internal(format!("OpenAI API error: {}", err_msg)));
        }

        let choice = &json["choices"][0];
        let message = &choice["message"];

        let metadata = ResponseMetadata {
            model: json["model"]
                .as_str()
                .unwrap_or(&config.model)
                .to_string(),
            finish_reason: choice["finish_reason"].as_str().map(|s| s.to_string()),
            usage: json["usage"].as_object().map(|u| TokenUsage {
                input_tokens: u["prompt_tokens"].as_u64().unwrap_or(0) as u32,
                output_tokens: u["completion_tokens"].as_u64().unwrap_or(0) as u32,
                cache_read_tokens: u.get("prompt_tokens_details")
                    .and_then(|d| d["cached_tokens"].as_u64())
                    .unwrap_or(0) as u32,
                cache_creation_tokens: 0,
            }),
        };

        let text = message["content"].as_str().map(|s| s.to_string());
        let thinking = message["reasoning_content"].as_str().map(|s| s.to_string());

        let tool_calls: Vec<ToolCall> = message["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        let func = &tc["function"];
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            name: func["name"].as_str()?.to_string(),
                            arguments: serde_json::from_str(
                                func["arguments"].as_str()?,
                            )
                            .unwrap_or(serde_json::json!({})),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        if !tool_calls.is_empty() {
            Ok(RespondOutput::ToolCalls {
                tool_calls,
                text,
                thinking,
                metadata,
            })
        } else {
            Ok(RespondOutput::Text {
                text: text.unwrap_or_default(),
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

        tracing::debug!(model = %config.model, "OpenAI stream request");

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
            return Err(Error::Internal(format!("OpenAI API error: {}", err_msg)));
        }

        let byte_stream = resp.bytes_stream();
        let stream = OpenAISseStream::new(byte_stream);
        Ok(Box::new(stream))
    }
}

// ─── SSE Stream Implementation ──────────────────────────────────────────────

/// Parses OpenAI-compatible SSE events into StreamDelta items.
struct OpenAISseStream {
    inner: Pin<Box<dyn Stream<Item = Result<StreamDelta, Error>> + Send>>,
}

impl OpenAISseStream {
    fn new(
        byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    ) -> Self {
        let stream = futures::stream::unfold(
            OpenAiSseState::new(byte_stream),
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

impl Stream for OpenAISseStream {
    type Item = Result<StreamDelta, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}

impl Unpin for OpenAISseStream {}

/// Internal state for parsing OpenAI-compatible SSE.
struct OpenAiSseState {
    byte_stream: Pin<Box<dyn Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send>>,
    buffer: String,
    done: bool,
    /// Deferred finish_reason from a chunk that arrived before usage.
    pending_finish_reason: Option<Option<String>>,
    /// Accumulated usage from a usage-bearing chunk.
    accumulated_usage: Option<TokenUsage>,
}

impl OpenAiSseState {
    fn new(
        byte_stream: impl Stream<Item = Result<bytes::Bytes, reqwest::Error>> + Send + 'static,
    ) -> Self {
        Self {
            byte_stream: Box::pin(byte_stream),
            buffer: String::new(),
            done: false,
            pending_finish_reason: None,
            accumulated_usage: None,
        }
    }

    async fn next_delta(&mut self) -> Option<Result<StreamDelta, Error>> {
        use futures::StreamExt;

        if self.done {
            return None;
        }

        loop {
            // Try to extract a complete line from buffer
            if let Some(line) = self.extract_line() {
                let trimmed = line.trim();

                // Skip empty lines and comments
                if trimmed.is_empty() || trimmed.starts_with(':') {
                    continue;
                }

                // Must be a "data: " line
                if let Some(data) = trimmed.strip_prefix("data: ") {
                    if data == "[DONE]" {
                        self.done = true;
                        // Return accumulated usage (from prior usage-only chunk)
                        // along with any pending finish_reason.
                        let finish_reason = self.pending_finish_reason.take()
                            .flatten()
                            .or_else(|| Some("stop".into()));
                        return Some(Ok(StreamDelta::Done {
                            finish_reason,
                            usage: self.accumulated_usage.take(),
                        }));
                    }

                    match serde_json::from_str::<serde_json::Value>(data) {
                        Ok(json) => {
                            if let Some(delta) = self.parse_chunk(&json) {
                                return Some(Ok(delta));
                            }
                            // No meaningful delta in this chunk, continue
                            continue;
                        }
                        Err(e) => {
                            tracing::warn!("OpenAI stream JSON parse error: {}", e);
                            continue;
                        }
                    }
                }
                // Skip non-data lines (like "event:" which some proxies send)
                continue;
            }

            // Need more data from the byte stream
            match self.byte_stream.next().await {
                Some(Ok(bytes)) => {
                    let text = String::from_utf8_lossy(&bytes);
                    self.buffer.push_str(&text);
                }
                Some(Err(e)) => {
                    self.done = true;
                    return Some(Err(Error::Internal(format!(
                        "OpenAI stream read error: {}",
                        e
                    ))));
                }
                None => {
                    if !self.done {
                        self.done = true;
                        let finish_reason = self.pending_finish_reason.take()
                            .flatten()
                            .or_else(|| Some("stream_ended".into()));
                        return Some(Ok(StreamDelta::Done {
                            finish_reason,
                            usage: self.accumulated_usage.take(),
                        }));
                    }
                    return None;
                }
            }
        }
    }

    fn extract_line(&mut self) -> Option<String> {
        if let Some(pos) = self.buffer.find('\n') {
            let line: String = self.buffer.drain(..=pos).collect();
            Some(line)
        } else {
            None
        }
    }

    fn parse_chunk(&mut self, json: &serde_json::Value) -> Option<StreamDelta> {
        let choices = json["choices"].as_array();

        // Helper: extract usage from a JSON object
        let extract_usage = |u: &serde_json::Map<String, serde_json::Value>| -> TokenUsage {
            TokenUsage {
                input_tokens: u.get("prompt_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                output_tokens: u.get("completion_tokens").and_then(|v| v.as_u64()).unwrap_or(0) as u32,
                cache_read_tokens: u.get("prompt_tokens_details")
                    .and_then(|d| d["cached_tokens"].as_u64())
                    .unwrap_or(0) as u32,
                cache_creation_tokens: 0,
            }
        };

        // Handle usage-only chunk (OpenAI sends a final chunk with usage but empty choices)
        // Don't emit Done here; stash the usage so it's included in the [DONE] event.
        if choices.map_or(true, |c| c.is_empty()) {
            if let Some(u) = json.get("usage").and_then(|v| v.as_object()) {
                self.accumulated_usage = Some(extract_usage(u));
                tracing::debug!("OpenAI stream: stashed usage from usage-only chunk");
            }
            return None;
        }

        let choices = choices.unwrap();
        let choice = choices.first()?;
        let delta = &choice["delta"];
        let finish_reason = choice["finish_reason"].as_str().map(|s| s.to_string());

        // Stash any usage that arrives alongside content chunks
        if let Some(u) = json.get("usage").and_then(|v| v.as_object()) {
            self.accumulated_usage = Some(extract_usage(u));
        }

        // On finish_reason with empty delta: don't emit Done yet.
        // Store the finish_reason and wait for the usage-only chunk or [DONE].
        if finish_reason.is_some() && delta.as_object().map_or(true, |o| o.is_empty()) {
            self.pending_finish_reason = Some(finish_reason);
            tracing::debug!("OpenAI stream: deferred Done, waiting for usage chunk");
            return None;
        }

        // Reasoning/thinking content (DeepSeek format)
        if let Some(reasoning) = delta["reasoning_content"].as_str() {
            if !reasoning.is_empty() {
                return Some(StreamDelta::ThinkingDelta {
                    thinking: reasoning.to_string(),
                });
            }
        }

        // Text content
        if let Some(content) = delta["content"].as_str() {
            if !content.is_empty() {
                return Some(StreamDelta::TextDelta {
                    text: content.to_string(),
                });
            }
        }

        // Tool calls
        if let Some(tool_calls) = delta["tool_calls"].as_array() {
            if let Some(tc) = tool_calls.first() {
                let id = tc["id"].as_str().unwrap_or("").to_string();
                let func = &tc["function"];
                let name = func["name"].as_str().map(|s| s.to_string());
                let arguments = func["arguments"].as_str().map(|s| s.to_string());

                // Only emit if there's meaningful data
                if name.is_some() || arguments.is_some() || !id.is_empty() {
                    return Some(StreamDelta::ToolCallDelta {
                        id,
                        name,
                        input_json: arguments,
                    });
                }
            }
        }

        None
    }
}

/// Strip trailing `/v1` from a base URL so endpoint paths like
/// `/v1/chat/completions` can be appended without doubling.
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
