// SPDX-License-Identifier: Apache-2.0
//! candle quantized_llama load + generation for MiniCPM5-1B.
//!
//! Mirrors the ONNX embedder's two-phase pattern: async lock to lazy-load,
//! then `spawn_blocking` + `blocking_lock` for the (synchronous, &mut) forward
//! loop. Generation is serialized behind the lock by construction.

use candle_transformers::generation::{LogitsProcessor, Sampling};

/// Generation parameters (mapped from the OpenAI request, with defaults).
#[derive(Debug, Clone)]
pub struct GenParams {
    pub temperature: f64,
    pub top_p: Option<f64>,
    pub top_k: Option<usize>,
    pub repeat_penalty: f32,
    /// How many recent tokens the repeat penalty considers.
    pub repeat_last_n: usize,
    pub max_tokens: usize,
    pub seed: u64,
    /// Extra stop strings beyond the built-in EOS / `<|im_end|>`.
    pub stop: Vec<String>,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: None,
            repeat_penalty: 1.1,
            repeat_last_n: 64,
            max_tokens: 512,
            seed: 299792458,
            stop: Vec::new(),
        }
    }
}

/// Why a generation could not run / produce output.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Model files missing on disk, or load not yet attempted/succeeded.
    /// Maps to HTTP 503 `model_not_ready` so the caller can fall back to cloud.
    #[error("model not ready: {0}")]
    NotReady(String),
    /// Load failed (corrupt GGUF, OOM, device init). Also a fall-back signal.
    #[error("model load failed: {0}")]
    LoadFailed(String),
    /// Inference-time failure (forward / sampling / decode).
    #[error("generation failed: {0}")]
    Generation(String),
}

/// Build a candle `Sampling` from params. `temperature <= 0` ⇒ greedy ArgMax
/// (deterministic), matching the candle quantized example's convention.
pub fn build_sampling(p: &GenParams) -> Sampling {
    if p.temperature <= 0.0 {
        return Sampling::ArgMax;
    }
    let t = p.temperature;
    match (p.top_k, p.top_p) {
        (None, None) => Sampling::All { temperature: t },
        (Some(k), None) => Sampling::TopK { k, temperature: t },
        (None, Some(pp)) => Sampling::TopP { p: pp, temperature: t },
        (Some(k), Some(pp)) => Sampling::TopKThenTopP { k, p: pp, temperature: t },
    }
}

/// Construct the candle logits processor for these params.
pub fn build_logits_processor(p: &GenParams) -> LogitsProcessor {
    LogitsProcessor::from_sampling(p.seed, build_sampling(p))
}

#[cfg(test)]
mod sampling_tests {
    use super::*;

    #[test]
    fn zero_temperature_is_argmax() {
        let p = GenParams { temperature: 0.0, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::ArgMax));
    }

    #[test]
    fn temp_only_is_all() {
        let p = GenParams { temperature: 0.8, top_p: None, top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::All { .. }));
    }

    #[test]
    fn temp_and_top_p_is_top_p() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopP { .. }));
    }

    #[test]
    fn top_k_and_top_p_is_combined() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: Some(40), ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopKThenTopP { .. }));
    }

    #[test]
    fn defaults_are_sane() {
        let p = GenParams::default();
        assert_eq!(p.max_tokens, 512);
        assert_eq!(p.repeat_last_n, 64);
        assert!((p.repeat_penalty - 1.1).abs() < 1e-6);
    }
}
