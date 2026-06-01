// SPDX-License-Identifier: Apache-2.0
//! Embedder factory: real OpenAI-compatible embedder when an endpoint is
//! configured, inert zero-vector embedder otherwise (tests / offline /
//! unconfigured).

use std::sync::Arc;

use crate::memubot_config::EmbeddingEndpointConfig;

use super::{Embedder, InertEmbedder, OpenAiCompatEmbedder, EMBEDDING_DIM};

/// Pick an embedder from config. Real when `base_url` AND `model` are both
/// non-empty; inert fallback otherwise.
///
/// `cfg.dimensions == 0` is treated as "use default" and maps to
/// [`EMBEDDING_DIM`]; any non-zero value is passed through as-is.
pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder> {
    let dim = if cfg.dimensions == 0 {
        EMBEDDING_DIM
    } else {
        cfg.dimensions as usize
    };
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        tracing::info!(
            dim,
            "[embed::factory] no embedding endpoint configured — using InertEmbedder"
        );
        return Arc::new(InertEmbedder::with_dim(dim));
    }
    tracing::info!(
        base_url = %cfg.base_url,
        model = %cfg.model,
        dim,
        "[embed::factory] using OpenAiCompatEmbedder"
    );
    Arc::new(OpenAiCompatEmbedder::new(
        &cfg.base_url,
        &cfg.model,
        cfg.embed_timeout_secs,
        dim,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::openai_compat::parse_embedding_response;

    fn cfg(base_url: &str, model: &str) -> EmbeddingEndpointConfig {
        EmbeddingEndpointConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            dimensions: 384,
            fastembed_model: "BAAI/bge-small-en-v1.5".to_string(),
            embed_timeout_secs: 8,
        }
    }

    fn cfg_dim(base_url: &str, model: &str, dimensions: u32) -> EmbeddingEndpointConfig {
        EmbeddingEndpointConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            dimensions,
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

    #[test]
    fn dimensions_from_config_propagated() {
        // non-zero dimensions → used as-is
        let e = build_embedder(&cfg("http://localhost:7337/v1", "bge-m3"));
        assert_eq!(e.dim(), 384);
    }

    #[test]
    fn zero_dimensions_falls_back_to_embedding_dim() {
        let e = build_embedder(&cfg_dim("http://localhost:7337/v1", "bge-m3", 0));
        assert_eq!(e.dim(), EMBEDDING_DIM);
    }

    #[test]
    fn inert_fallback_carries_configured_dim() {
        let e = build_embedder(&cfg("", "bge-m3"));
        assert_eq!(e.dim(), 384);
    }

    #[test]
    fn parse_embedding_response_honors_dim() {
        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
        assert!(parse_embedding_response(body, 3).is_ok());
        assert!(parse_embedding_response(body, 4).is_err());
    }
}
