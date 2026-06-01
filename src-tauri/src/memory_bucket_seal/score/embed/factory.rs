// SPDX-License-Identifier: Apache-2.0
//! Embedder factory: routes to the best available backend.
//!
//! Routing table (checked in order):
//! 1. `base_url` is empty → [`InertEmbedder`] (offline / tests / unconfigured).
//! 2. `base_url` contains `:7337` (the memU default local endpoint) →
//!    [`super::onnx::OnnxEmbedder`] running in-process (no Python, no network).
//! 3. Any other non-empty `base_url` → [`OpenAiCompatEmbedder`] for explicit
//!    remote / self-hosted endpoints.

use std::path::Path;
use std::sync::Arc;

use crate::memubot_config::EmbeddingEndpointConfig;

use super::{Embedder, InertEmbedder, OpenAiCompatEmbedder, EMBEDDING_DIM};

/// Pick an embedder from config.
///
/// `data_dir` is the uClaw home directory (e.g. `~/.uclaw`); it is used to
/// locate the ONNX model cache when the in-process backend is selected.
///
/// `cfg.dimensions == 0` is treated as "use default" and maps to
/// [`EMBEDDING_DIM`]; any non-zero value is passed through as-is.
pub fn build_embedder(cfg: &EmbeddingEndpointConfig, data_dir: &Path) -> Arc<dyn Embedder> {
    let dim = if cfg.dimensions == 0 {
        EMBEDDING_DIM
    } else {
        cfg.dimensions as usize
    };
    if cfg.base_url.trim().is_empty() {
        tracing::info!(dim, "[embed::factory] no endpoint configured — InertEmbedder");
        return Arc::new(InertEmbedder::with_dim(dim));
    }
    // memU default endpoint → embed IN-PROCESS (no Python / no :7337 server)
    if cfg.base_url.contains(":7337") {
        let model_dir = super::model_download::model_dir(data_dir);
        tracing::info!(dim, "[embed::factory] in-process OnnxEmbedder (bge-small)");
        return Arc::new(super::onnx::OnnxEmbedder::new(model_dir, dim));
    }
    // explicit remote OpenAI-compatible endpoint → keep network embedder
    tracing::info!(
        base_url = %cfg.base_url,
        model = %cfg.model,
        dim,
        "[embed::factory] OpenAiCompatEmbedder"
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
    fn default_7337_routes_to_onnx() {
        let e = build_embedder(&EmbeddingEndpointConfig::default(), std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.name(), "onnx-bge-small");
        assert_eq!(e.dim(), 384);
    }

    #[test]
    fn remote_endpoint_routes_to_openai_compat() {
        let mut cfg_val = EmbeddingEndpointConfig::default();
        cfg_val.base_url = "https://api.example.com/v1".into();
        let e = build_embedder(&cfg_val, std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.name(), "openai_compat");
    }

    #[test]
    fn empty_routes_to_inert() {
        let mut cfg_val = EmbeddingEndpointConfig::default();
        cfg_val.base_url = "".into();
        let e = build_embedder(&cfg_val, std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.name(), "inert");
    }

    #[test]
    fn dimensions_from_config_propagated() {
        // non-zero dimensions → used as-is (via ONNX path for :7337)
        let e = build_embedder(&cfg("http://localhost:7337/v1", "bge-m3"), std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.dim(), 384);
    }

    #[test]
    fn zero_dimensions_falls_back_to_embedding_dim() {
        let e = build_embedder(&cfg_dim("https://remote.example.com/v1", "bge-m3", 0), std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.dim(), EMBEDDING_DIM);
    }

    #[test]
    fn inert_fallback_carries_configured_dim() {
        let e = build_embedder(&cfg("", "bge-m3"), std::path::Path::new("/tmp/uclaw"));
        assert_eq!(e.dim(), 384);
    }

    #[test]
    fn parse_embedding_response_honors_dim() {
        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
        assert!(parse_embedding_response(body, 3).is_ok());
        assert!(parse_embedding_response(body, 4).is_err());
    }
}
