use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use crate::agent::types::{ChatMessage, RespondOutput, StreamDelta, ToolDefinition};
use crate::error::Error;

/// Completion configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionConfig {
    pub model: String,
    pub max_tokens: u32,
    pub temperature: f32,
    pub thinking_enabled: bool,
}

impl Default for CompletionConfig {
    fn default() -> Self {
        Self { model: "claude-sonnet-4-20250514".into(), max_tokens: 16384, temperature: 0.7, thinking_enabled: false }
    }
}

/// LLM Provider trait
#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<RespondOutput, Error>;

    async fn stream(
        &self,
        messages: Vec<ChatMessage>,
        tools: Vec<ToolDefinition>,
        config: &CompletionConfig,
    ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error>;

    /// Whether this provider's wire reports input/prompt tokens INCLUDING
    /// prompt-cache reads: `Some(false)` Anthropic Messages (`input_tokens`
    /// excludes `cache_read_input_tokens`), `Some(true)` OpenAI-compatible
    /// (`prompt_tokens` includes `cached_tokens`), `None` unknown (mocks).
    fn usage_input_includes_cache(&self) -> Option<bool> {
        None
    }
}
