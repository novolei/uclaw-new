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
