//! Deterministic zero-vector embedder for tests.
//!
//! `InertEmbedder::embed` always returns a fresh `Vec<f32>` of length
//! `self.dim()` filled with zeros — no network, no randomness,
//! no per-text variation. Useful in tests that want to exercise the
//! ingest/seal embedding plumbing without standing up Ollama.
//!
//! Note: because every chunk and summary ends up with the same
//! zero-vector embedding, cosine similarity between them is always 0.0
//! (see [`super::cosine_similarity`] — zero-magnitude vectors short to
//! 0.0 instead of NaN). Retrieval tests that want to see reranking work
//! should hand-stitch embeddings via the store accessors rather than
//! rely on the inert path.

use anyhow::Result;
use async_trait::async_trait;

use super::{Embedder, EMBEDDING_DIM};

/// Zero-vector embedder. Returns `vec![0.0; self.dim()]` for every call.
#[derive(Clone, Copy, Debug)]
pub struct InertEmbedder {
    dim: usize,
}

impl InertEmbedder {
    /// Construct an inert embedder with the default [`EMBEDDING_DIM`].
    pub fn new() -> Self {
        Self { dim: EMBEDDING_DIM }
    }

    /// Construct an inert embedder with an explicit dimension.
    /// Useful in tests that exercise a non-default provider dimension.
    pub fn with_dim(dim: usize) -> Self {
        Self { dim }
    }
}

impl Default for InertEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Embedder for InertEmbedder {
    fn name(&self) -> &'static str {
        "inert"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0; self.dim])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_zero_vector_of_embedding_dim() {
        let e = InertEmbedder::new();
        let v = e.embed("anything").await.unwrap();
        assert_eq!(v.len(), EMBEDDING_DIM);
        assert!(v.iter().all(|f| *f == 0.0));
    }

    #[tokio::test]
    async fn name_is_inert() {
        assert_eq!(InertEmbedder::new().name(), "inert");
    }

    #[tokio::test]
    async fn empty_input_still_returns_full_vector() {
        let v = InertEmbedder::new().embed("").await.unwrap();
        assert_eq!(v.len(), EMBEDDING_DIM);
    }

    #[tokio::test]
    async fn inert_with_dim_returns_that_many_zeros() {
        let e = InertEmbedder::with_dim(384);
        assert_eq!(e.dim(), 384);
        assert_eq!(e.embed("x").await.unwrap().len(), 384);
        assert_eq!(InertEmbedder::new().dim(), EMBEDDING_DIM);
    }
}
