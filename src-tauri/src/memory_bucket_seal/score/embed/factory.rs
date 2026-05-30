// SPDX-License-Identifier: Apache-2.0
//! Embedder factory: real OpenAI-compatible embedder when an endpoint is
//! configured, inert zero-vector embedder otherwise (tests / offline /
//! unconfigured).

use std::sync::Arc;

use crate::memubot_config::EmbeddingEndpointConfig;

use super::{Embedder, InertEmbedder, OpenAiCompatEmbedder};

/// Pick an embedder from config. Real when `base_url` AND `model` are both
/// non-empty; inert fallback otherwise.
pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder> {
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        tracing::info!("[embed::factory] no embedding endpoint configured — using InertEmbedder");
        return Arc::new(InertEmbedder::new());
    }
    tracing::info!(
        base_url = %cfg.base_url,
        model = %cfg.model,
        "[embed::factory] using OpenAiCompatEmbedder"
    );
    Arc::new(OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base_url: &str, model: &str) -> EmbeddingEndpointConfig {
        EmbeddingEndpointConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            dimensions: 384,
            fastembed_model: "BAAI/bge-small-en-v1.5".to_string(),
            embed_timeout_secs: 8,
        }
    }

    #[test]
    fn configured_yields_openai_compat() {
        let e = build_embedder(&cfg("http://localhost:7337/v1", "bge-m3"));
        assert_eq!(e.name(), "openai_compat");
    }

    #[test]
    fn empty_base_url_yields_inert() {
        let e = build_embedder(&cfg("", "bge-m3"));
        assert_eq!(e.name(), "inert");
    }

    #[test]
    fn empty_model_yields_inert() {
        let e = build_embedder(&cfg("http://localhost:7337/v1", ""));
        assert_eq!(e.name(), "inert");
    }
}
