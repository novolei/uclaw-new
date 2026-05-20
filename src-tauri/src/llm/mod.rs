pub mod provider;
pub mod providers;
pub mod stream_error;
// M1-T7 — eager prewarm of HTTP/2+TLS to the active LLM provider.
pub mod prewarm;

pub use provider::{CompletionConfig, LlmProvider};
pub use providers::anthropic::AnthropicProvider;
pub use providers::openai::OpenAIProvider;
pub use stream_error::{classify_stream_error, StreamErrorKind};

use crate::config::llm::LlmConfig;
use std::sync::Arc;

/// Create an LLM provider from config.
///
/// For Anthropic: uses AnthropicProvider with Messages API.
/// For all other providers: uses OpenAIProvider with the configured base URL.
/// Most providers (DeepSeek, Moonshot, Zhipu, Ollama, etc.) offer
/// OpenAI-compatible `/v1/chat/completions` endpoints.
pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, crate::error::Error> {
    match config.provider.as_str() {
        "anthropic" => Ok(Arc::new(AnthropicProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
        // All OpenAI-compatible providers
        _ => Ok(Arc::new(OpenAIProvider::new(
            config.api_key.clone(),
            config.base_url.clone(),
        ))),
    }
}

/// Build an `LlmConfig` from the active provider service model.
/// Returns `None` if no active model is configured.
pub fn llm_config_from_provider(
    provider_id: &str,
    model: &str,
    api_key: &str,
    base_url: &str,
    max_tokens: u32,
    temperature: f32,
) -> LlmConfig {
    LlmConfig {
        provider: provider_id.to_string(),
        model: model.to_string(),
        api_key: api_key.to_string(),
        base_url: if base_url.is_empty() { None } else { Some(base_url.to_string()) },
        max_tokens: Some(max_tokens),
        temperature: Some(temperature),
    }
}
