// SPDX-License-Identifier: Apache-2.0
//! In-process local LLM inference (MiniCPM5-1B via candle quantized_llama).
//!
//! Mirrors the ONNX embedder (`memory_bucket_seal/score/embed/onnx.rs`):
//! model + tokenizer live behind a `tokio::Mutex<Option<Loaded>>`, lazy-loaded
//! on first request (no 688 MB at startup), generation serialized behind the
//! lock. Exposed over HTTP by `LocalApiService` at `:7337/v1/chat/completions`.
//!
//! Cache-path contract with Slice C: model files live under
//! `<data_dir>/models/minicpm5-1b/`. Slice B only READS them; Slice C downloads.

use std::path::{Path, PathBuf};

pub mod chat_template;
pub mod engine;
pub mod env_check;
pub mod model_manager;

/// The default model identifier as registered with the provider registry.
pub const MODEL_ID: &str = "minicpm5-1b";

/// GGUF filename for the default quant (Q4_K_M). Slice C writes this path.
pub const GGUF_FILENAME: &str = "MiniCPM5-1B-Q4_K_M.gguf";

/// External tokenizer (candle does NOT use the GGUF-embedded tokenizer).
pub const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// Resolve the model cache directory under the uClaw data dir
/// (e.g. `~/.uclaw/models/minicpm5-1b/`).
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join(MODEL_ID)
}

/// `(gguf_path, tokenizer_path)` inside the model dir.
pub fn model_paths(data_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = model_dir(data_dir);
    (dir.join(GGUF_FILENAME), dir.join(TOKENIZER_FILENAME))
}

/// True when both required files exist on disk (does NOT mean loaded).
pub fn is_present(data_dir: &Path) -> bool {
    let (gguf, tok) = model_paths(data_dir);
    gguf.exists() && tok.exists()
}

use std::sync::Arc;

use engine::{EngineError, FinishReason, GenParams, Loaded};

/// Long-lived in-process MiniCPM engine. Model + tokenizer are lazy-loaded on
/// first use and cached behind a `tokio::Mutex`; generation is serialized by the
/// lock. Mirrors `OnnxEmbedder`'s two-phase (async-load / blocking-infer) shape.
pub struct LocalLlmEngine {
    data_dir: PathBuf,
    inner: Arc<tokio::sync::Mutex<Option<Loaded>>>,
}

impl LocalLlmEngine {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir, inner: Arc::new(tokio::sync::Mutex::new(None)) }
    }

    /// True iff the model is currently loaded in memory.
    pub async fn is_ready(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    /// True iff the model files exist on disk (Slice C has downloaded them).
    pub fn is_present(&self) -> bool {
        is_present(&self.data_dir)
    }

    /// Ensure the model is loaded; `NotReady` if files are absent. Idempotent.
    async fn ensure_loaded(&self) -> Result<(), EngineError> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        if !is_present(&self.data_dir) {
            return Err(EngineError::NotReady(format!(
                "model files missing under {:?}",
                model_dir(&self.data_dir)
            )));
        }
        let (gguf, tok) = model_paths(&self.data_dir);
        let loaded = tokio::task::spawn_blocking(move || {
            let device = engine::select_device();
            engine::load(&gguf, &tok, device)
        })
        .await
        .map_err(|e| EngineError::LoadFailed(format!("spawn_blocking join: {e}")))??;
        *guard = Some(loaded);
        tracing::info!("[local_llm] model loaded");
        Ok(())
    }

    /// Load + run a 1-token forward to JIT Metal kernels / page in weights.
    pub async fn warmup(&self) -> Result<(), EngineError> {
        self.ensure_loaded().await?;
        let params = GenParams { max_tokens: 1, temperature: 0.0, ..Default::default() };
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<(), EngineError> {
            let mut guard = inner.blocking_lock();
            let loaded = guard.as_mut().ok_or_else(|| EngineError::NotReady("unloaded".into()))?;
            engine::generate(loaded, "<|im_start|>assistant\n", &params, |_| {})?;
            Ok(())
        })
        .await
        .map_err(|e| EngineError::Generation(format!("spawn_blocking join (warmup): {e}")))??;
        tracing::info!("[local_llm] warmup complete");
        Ok(())
    }

    /// Drop the loaded model, freeing ~688 MB.
    pub async fn unload(&self) {
        *self.inner.lock().await = None;
        tracing::info!("[local_llm] model unloaded");
    }

    /// Generate to completion, invoking `on_delta` per text fragment. Serialized
    /// behind the model lock. `NotReady` when files are absent.
    pub async fn generate(
        &self,
        prompt: &str,
        params: &GenParams,
        mut on_delta: impl FnMut(&str) + Send + 'static,
    ) -> Result<(FinishReason, usize), EngineError> {
        self.ensure_loaded().await?;
        let inner = self.inner.clone();
        let prompt = prompt.to_string();
        let params = params.clone();
        tokio::task::spawn_blocking(move || -> Result<(FinishReason, usize), EngineError> {
            let mut guard = inner.blocking_lock();
            let loaded = guard.as_mut().ok_or_else(|| EngineError::NotReady("unloaded".into()))?;
            engine::generate(loaded, &prompt, &params, |d| on_delta(d))
        })
        .await
        .map_err(|e| EngineError::Generation(format!("spawn_blocking join (gen): {e}")))?
    }
}

#[cfg(test)]
mod engine_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn not_ready_when_files_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let eng = LocalLlmEngine::new(tmp.path().to_path_buf());
        assert!(!eng.is_ready().await);
        let err = eng
            .generate("<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n", &engine::GenParams::default(), |_| {})
            .await
            .expect_err("absent model must error");
        assert!(matches!(err, engine::EngineError::NotReady(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn warmup_on_absent_model_reports_not_ready() {
        let tmp = tempfile::tempdir().unwrap();
        let eng = LocalLlmEngine::new(tmp.path().to_path_buf());
        let err = eng.warmup().await.expect_err("absent model warmup must error");
        assert!(matches!(err, engine::EngineError::NotReady(_)));
    }
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn model_dir_under_data() {
        let d = model_dir(Path::new("/tmp/uclaw"));
        assert!(d.ends_with("models/minicpm5-1b"), "got {d:?}");
    }

    #[test]
    fn model_paths_name_the_two_files() {
        let (g, t) = model_paths(Path::new("/tmp/uclaw"));
        assert!(g.ends_with("models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf"));
        assert!(t.ends_with("models/minicpm5-1b/tokenizer.json"));
    }

    #[test]
    fn is_present_requires_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let dir = model_dir(data);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_present(data));
        std::fs::write(dir.join(GGUF_FILENAME), b"x").unwrap();
        assert!(!is_present(data));
        std::fs::write(dir.join(TOKENIZER_FILENAME), b"x").unwrap();
        assert!(is_present(data));
    }
}
