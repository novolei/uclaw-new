// SPDX-License-Identifier: Apache-2.0
//! Real embedder: POSTs to an OpenAI-compatible `/embeddings` endpoint.
//!
//! Validates the returned vector against [`EMBEDDING_DIM`] (1024, bge-m3) —
//! NOT the gbrain/memU `dimensions` config field. A 384-dim endpoint
//! (uClaw's default bge-small route) fails validation; configure a
//! 1024-dim model for real embeddings. Failures are non-fatal upstream
//! (best-effort seal).

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{Embedder, EMBEDDING_DIM};

/// OpenAI-compatible HTTP embedder.
pub struct OpenAiCompatEmbedder {
    client: reqwest::Client,
    embeddings_url: String,
    model: String,
}

impl OpenAiCompatEmbedder {
    /// `base_url` is the OpenAI-compatible root (e.g. `http://localhost:7337/v1`);
    /// the embeddings endpoint is `{base_url}/embeddings`.
    pub fn new(base_url: &str, model: &str) -> Self {
        let trimmed = base_url.trim_end_matches('/');
        Self {
            client: reqwest::Client::new(),
            embeddings_url: format!("{trimmed}/embeddings"),
            model: model.to_string(),
        }
    }
}

/// Build the request body for an embeddings call.
pub(crate) fn build_embedding_request(model: &str, text: &str) -> Value {
    serde_json::json!({ "model": model, "input": text })
}

/// Parse an OpenAI embeddings response, returning the first embedding.
/// Errors when `data` is empty or the embedding length != `expected_dim`.
pub(crate) fn parse_embedding_response(body: &str, expected_dim: usize) -> Result<Vec<f32>> {
    let parsed: Value = serde_json::from_str(body).context("parse embeddings JSON")?;
    let arr = parsed
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow!("embeddings response missing `data` array"))?;
    let first = arr
        .first()
        .ok_or_else(|| anyhow!("embeddings response `data` is empty"))?;
    let emb = first
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("embeddings response missing `embedding`"))?;
    let out: Vec<f32> = emb
        .iter()
        .map(|v| v.as_f64().map(|f| f as f32))
        .collect::<Option<Vec<f32>>>()
        .ok_or_else(|| anyhow!("embedding contained a non-numeric value"))?;
    if out.len() != expected_dim {
        return Err(anyhow!(
            "embedding dimension mismatch: got {}, expected {} (dimension)",
            out.len(),
            expected_dim
        ));
    }
    Ok(out)
}

#[async_trait]
impl Embedder for OpenAiCompatEmbedder {
    fn name(&self) -> &'static str {
        "openai_compat"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = build_embedding_request(&self.model, text);
        let resp = self
            .client
            .post(&self.embeddings_url)
            .json(&body)
            .send()
            .await
            .context("embeddings request failed")?
            .error_for_status()
            .context("embeddings endpoint returned error status")?;
        let text_body = resp.text().await.context("read embeddings body")?;
        parse_embedding_response(&text_body, EMBEDDING_DIM)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_has_model_and_input() {
        let body = build_embedding_request("bge-m3", "hello world");
        assert_eq!(body["model"], "bge-m3");
        assert_eq!(body["input"], "hello world");
    }

    #[test]
    fn parse_response_extracts_first_embedding() {
        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
        let v = parse_embedding_response(body, 3).unwrap();
        assert_eq!(v, vec![0.1_f32, 0.2, 0.3]);
    }

    #[test]
    fn parse_response_rejects_wrong_dimension() {
        let body = r#"{"data":[{"embedding":[0.1,0.2]}]}"#;
        let err = parse_embedding_response(body, 3).unwrap_err();
        assert!(format!("{err:#}").contains("dimension"));
    }

    #[test]
    fn parse_response_errors_on_empty_data() {
        let body = r#"{"data":[]}"#;
        assert!(parse_embedding_response(body, 3).is_err());
    }
}
