// SPDX-License-Identifier: Apache-2.0
//! In-process ONNX text embedder (bge-small-en-v1.5).
//!
//! Uses CLS-token pooling + L2-normalisation to produce a 384-dim vector
//! that is vector-space-compatible with FastEmbed-produced bge-small embeddings
//! (`last_hidden_state[:, 0, :]` then L2-normalize).
//!
//! The Session and Tokenizer are lazy-loaded on the first call to [`OnnxEmbedder::embed`]
//! and cached for the lifetime of the struct. Model files are downloaded via
//! [`model_download::ensure_model`] if absent. Session::run needs `&mut Session`
//! so the pair lives behind a `tokio::sync::Mutex`.

use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ndarray::Axis;
use ort::{
    session::{builder::GraphOptimizationLevel, Session},
    value::Tensor,
};
use tokenizers::Tokenizer;

use super::{model_download, Embedder};

/// Max token sequence length. bge-small-en-v1.5 has `max_position_embeddings = 512`
/// (BERT-base); inputs longer than this overflow the position-embedding add
/// (`/embeddings/Add_1`: "broadcast 512 by <seq>") and the ONNX run fails. The
/// tokenizer is configured to truncate to this at load (FastEmbed did the same),
/// keeping `[CLS] … [SEP]` so CLS-pooling stays valid for long inputs (seal
/// summaries, daily digests).
const MAX_SEQ_LEN: usize = 512;

// ---------------------------------------------------------------------------
// Internal loaded state
// ---------------------------------------------------------------------------

struct Loaded {
    tokenizer: Tokenizer,
    session: Session,
}

// ---------------------------------------------------------------------------
// OnnxEmbedder
// ---------------------------------------------------------------------------

pub struct OnnxEmbedder {
    dim: usize,
    model_dir: PathBuf,
    inner: Arc<tokio::sync::Mutex<Option<Loaded>>>,
}

impl OnnxEmbedder {
    pub fn new(model_dir: PathBuf, dim: usize) -> Self {
        Self {
            dim,
            model_dir,
            inner: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

// ---------------------------------------------------------------------------
// Pure pooling helper — unit-testable with no model/network.
// ---------------------------------------------------------------------------

/// CLS pooling + L2 normalisation over `last_hidden_state` viewed as [seq, hidden].
///
/// Takes token 0 (the [CLS] token) from the sequence axis and normalises it to
/// unit length. Returns a zero vector when the norm is zero (degenerate input)
/// to keep callers stable instead of emitting NaN.
pub(crate) fn cls_pool_normalize(last_hidden_state: &ndarray::ArrayView2<f32>) -> Vec<f32> {
    let cls = last_hidden_state.index_axis(Axis(0), 0); // shape [hidden]
    let norm: f32 = cls.dot(&cls).sqrt();
    let inv = if norm > 0.0 { 1.0 / norm } else { 0.0 };
    cls.iter().map(|x| x * inv).collect()
}

// ---------------------------------------------------------------------------
// Embedder implementation
// ---------------------------------------------------------------------------

#[async_trait]
impl Embedder for OnnxEmbedder {
    fn name(&self) -> &'static str {
        "onnx-bge-small"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // --- Phase 1: lazy-load (async lock, download + Session init) --------
        {
            let mut guard = self.inner.lock().await;
            if guard.is_none() {
                model_download::ensure_model(&self.model_dir).await?;
                let dir = self.model_dir.clone();
                let loaded =
                    tokio::task::spawn_blocking(move || -> Result<Loaded> {
                        let mut tokenizer =
                            Tokenizer::from_file(dir.join("tokenizer.json"))
                                .map_err(|e| anyhow!("tokenizer load: {e}"))?;
                        // Truncate to the model's max position (512). Without this,
                        // long inputs overflow `/embeddings/Add_1` and the run fails.
                        tokenizer
                            .with_truncation(Some(tokenizers::TruncationParams {
                                max_length: MAX_SEQ_LEN,
                                ..Default::default()
                            }))
                            .map_err(|e| anyhow!("tokenizer truncation config: {e}"))?;
                        let session = Session::builder()
                            .map_err(|e| anyhow!(e.to_string()))?
                            .with_optimization_level(GraphOptimizationLevel::Level3)
                            .map_err(|e| anyhow!(e.to_string()))?
                            .commit_from_file(dir.join("model.onnx"))
                            .map_err(|e| anyhow!(e.to_string()))?;
                        Ok(Loaded { tokenizer, session })
                    })
                    .await
                    .map_err(|e| anyhow!("spawn_blocking join (load): {e}"))??;
                *guard = Some(loaded);
            }
        }

        // --- Phase 2: inference (blocking thread, blocking_lock) -------------
        // Mirrors the STT pattern: spawn_blocking → blocking_lock → run.
        let inner = self.inner.clone();
        let dim = self.dim;
        let text_owned = text.to_string();

        let v = tokio::task::spawn_blocking(move || -> Result<Vec<f32>> {
            let mut guard = inner.blocking_lock();
            let loaded = guard.as_mut().ok_or_else(|| anyhow!("embedder not loaded"))?;

            // Tokenise
            let enc = loaded
                .tokenizer
                .encode(text_owned, true)
                .map_err(|e| anyhow!("encode: {e}"))?;

            let ids: Vec<i64> = enc.get_ids().iter().map(|&x| x as i64).collect();
            let mask: Vec<i64> = enc.get_attention_mask().iter().map(|&x| x as i64).collect();
            let types: Vec<i64> = enc.get_type_ids().iter().map(|&x| x as i64).collect();
            let seq = ids.len();

            // Determine which inputs the model declares (token_type_ids may be absent).
            // session.inputs is a field (not a method) — confirmed by onnx_inference.rs L45-50.
            let declared: std::collections::HashSet<String> = loaded
                .session
                .inputs
                .iter()
                .map(|i| i.name.clone())
                .collect();

            // Snapshot the first output name BEFORE running (session.run takes &mut session,
            // and the returned SessionOutputs borrows session mutably, so we can't access
            // session.outputs afterwards without a borrow conflict).
            let first_output_name = loaded.session.outputs[0].name.clone();

            let id_t = Tensor::from_array(([1_usize, seq], ids))
                .map_err(|e| anyhow!(e.to_string()))?;
            let mask_t = Tensor::from_array(([1_usize, seq], mask))
                .map_err(|e| anyhow!(e.to_string()))?;
            let type_t = Tensor::from_array(([1_usize, seq], types))
                .map_err(|e| anyhow!(e.to_string()))?;

            // Build input map conditionally.
            let outputs = if declared.contains("token_type_ids") {
                loaded
                    .session
                    .run(ort::inputs! {
                        "input_ids"      => id_t,
                        "attention_mask" => mask_t,
                        "token_type_ids" => type_t,
                    })
                    .map_err(|e| anyhow!(e.to_string()))?
            } else {
                loaded
                    .session
                    .run(ort::inputs! {
                        "input_ids"      => id_t,
                        "attention_mask" => mask_t,
                    })
                    .map_err(|e| anyhow!(e.to_string()))?
            };

            // Resolve output name: prefer "last_hidden_state", else first output
            // (name snapshotted before run to avoid the mutable borrow conflict).
            let out_name: String = if outputs.contains_key("last_hidden_state") {
                "last_hidden_state".to_string()
            } else {
                first_output_name
            };

            let arr = outputs[out_name.as_str()]
                .try_extract_array::<f32>()
                .map_err(|e| anyhow!(e.to_string()))?;

            // Shape is [1, seq, hidden] — squeeze batch dimension.
            let arr3 = arr
                .into_dimensionality::<ndarray::Ix3>()
                .map_err(|e| anyhow!("reshape [1,seq,hidden]: {e}"))?;
            let lhs = arr3.index_axis(Axis(0), 0); // [seq, hidden]

            let out = cls_pool_normalize(&lhs);
            if out.len() != dim {
                return Err(anyhow!(
                    "embedding dim {} != expected {}",
                    out.len(),
                    dim
                ));
            }
            Ok(out)
        })
        .await
        .map_err(|e| anyhow!("spawn_blocking join (infer): {e}"))??;

        Ok(v)
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies CLS pooling extracts token-0 and L2-normalises it.
    /// Input: row 0 = [3, 4, 0] (norm=5), row 1 = [9, 9, 9] (should be ignored).
    /// Expected output: [0.6, 0.8, 0.0].
    #[test]
    fn cls_pool_takes_token0_and_normalizes() {
        let a = ndarray::array![[3.0_f32, 4.0, 0.0], [9.0, 9.0, 9.0]];
        let v = cls_pool_normalize(&a.view());
        assert_eq!(v.len(), 3);
        assert!(
            (v[0] - 0.6).abs() < 1e-6,
            "v[0]={} expected 0.6",
            v[0]
        );
        assert!(
            (v[1] - 0.8).abs() < 1e-6,
            "v[1]={} expected 0.8",
            v[1]
        );
        assert!(v[2].abs() < 1e-6, "v[2]={} expected 0.0", v[2]);
    }

    /// Zero-norm CLS row must yield a zero vector, not NaN.
    #[test]
    fn cls_pool_zero_norm_yields_zero_not_nan() {
        let a = ndarray::array![[0.0_f32, 0.0, 0.0], [1.0, 2.0, 3.0]];
        let v = cls_pool_normalize(&a.view());
        for (i, &x) in v.iter().enumerate() {
            assert!(
                !x.is_nan() && x == 0.0,
                "v[{i}]={x} should be 0.0 for zero-norm input"
            );
        }
    }

    /// Live embed test — downloads ~130 MB on first run; skipped in CI.
    #[tokio::test]
    #[ignore]
    async fn live_embed_produces_384_dim_unit_vector() {
        let tmp = tempfile::tempdir().unwrap();
        let model_dir = model_download::model_dir(tmp.path());
        let embedder = OnnxEmbedder::new(model_dir, 384);
        let v = embedder.embed("Hello, world!").await.unwrap();
        assert_eq!(v.len(), 384);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm={norm} not unit");
    }

    /// Regression: a long input (far exceeding the 512-token max position) must
    /// embed without error. Before the truncation fix this panicked the ONNX run
    /// at `/embeddings/Add_1` ("broadcast 512 by <seq>"), failing seal/digest jobs.
    /// Downloads ~130 MB on first run; `#[ignore]`d in CI.
    #[tokio::test]
    #[ignore]
    async fn live_embed_truncates_long_input() {
        let tmp = tempfile::tempdir().unwrap();
        let model_dir = model_download::model_dir(tmp.path());
        let embedder = OnnxEmbedder::new(model_dir, 384);
        // ~2000 words → well over 512 tokens (the failing seal summaries were 800–1700).
        let long = "memory ".repeat(2000);
        let v = embedder
            .embed(&long)
            .await
            .expect("long input must truncate + embed, not error");
        assert_eq!(v.len(), 384);
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5, "norm={norm} not unit");
    }

    /// Vector-space parity gate against memU's bge-small reference embeddings
    /// (captured from the FastEmbed/7337 endpoint that this embedder replaces;
    /// checked in at `testdata/memu_bge_reference.json` so the guarantee
    /// survives memU removal). Asserts cosine >= 0.999 so the ~460 pre-existing
    /// embeddings share one space with new ones. Downloads ~130 MB on first run;
    /// `#[ignore]`d in CI. Confirmed cosine=0.999999 for both fixtures.
    #[tokio::test]
    #[ignore]
    async fn parity_vs_memu_reference() {
        const FIXTURE: &str = include_str!("testdata/memu_bge_reference.json");
        fn cosine(a: &[f32], b: &[f32]) -> f32 {
            let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
            let na: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let nb: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            dot / (na * nb)
        }
        let refs: std::collections::BTreeMap<String, Vec<f32>> =
            serde_json::from_str(FIXTURE).unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let model_dir = model_download::model_dir(tmp.path());
        let embedder = OnnxEmbedder::new(model_dir, 384);
        for (text, theirs) in &refs {
            let mine = embedder.embed(text).await.unwrap();
            assert_eq!(mine.len(), theirs.len(), "dim mismatch for {text:?}");
            let cos = cosine(&mine, theirs);
            eprintln!("PARITY {text:?}: cosine={cos:.6}");
            assert!(cos >= 0.999, "cosine {cos:.6} < 0.999 for {text:?}");
        }
    }
}
