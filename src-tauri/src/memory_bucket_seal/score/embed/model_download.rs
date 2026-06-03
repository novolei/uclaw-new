//! Downloads bge-small-en-v1.5 ONNX + tokenizer into <data_dir>/models/bge-small-en-v1.5/
//! (HuggingFace primary, hf-mirror fallback). Idempotent: skips files already present.
//!
//! Delegates to `crate::model_fetch::download_manifest` — the shared manifest downloader
//! that handles streaming, atomic tmp→final rename, source fallback, and cancellation.

use std::path::{Path, PathBuf};

/// HuggingFace base URL (kept for documentation; URL construction uses hf_url/hf_mirror_url).
#[allow(dead_code)]
const HF_BASE: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main";
/// hf-mirror base URL (kept for documentation; URL construction uses hf_url/hf_mirror_url).
#[allow(dead_code)]
const MIRROR_BASE: &str = "https://hf-mirror.com/BAAI/bge-small-en-v1.5/resolve/main";

/// (remote relpath, local filename). Only model.onnx + tokenizer.json are REQUIRED.
const FILES: &[(&str, &str)] = &[
    ("onnx/model.onnx", "model.onnx"),
    ("tokenizer.json", "tokenizer.json"),
];

pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join("bge-small-en-v1.5")
}

pub fn is_present(dir: &Path) -> bool {
    dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
}

/// Download any missing bge files (HF then mirror) via the shared manifest
/// downloader. Public signature unchanged for the embedder callers.
pub async fn ensure_model(dir: &Path) -> anyhow::Result<()> {
    use crate::model_fetch::manifest::{
        hf_mirror_url, hf_url, FileSource, Host, ManifestFile, ModelManifest,
    };
    if is_present(dir) {
        return Ok(());
    }
    const REPO: &str = "BAAI/bge-small-en-v1.5";
    let files = FILES
        .iter()
        .map(|(rel, local)| ManifestFile {
            dest_name: (*local).to_string(),
            sources: vec![
                FileSource { host: Host::HuggingFace, url: hf_url(REPO, "main", rel) },
                FileSource { host: Host::HfMirror, url: hf_mirror_url(REPO, "main", rel) },
            ],
            expected_size: None,
            sha256: None,
        })
        .collect();
    let manifest = ModelManifest { cache_dir: dir.to_path_buf(), files };
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    crate::model_fetch::download_manifest(&manifest, cancel, |_| {})
        .await
        .map_err(|e| anyhow::anyhow!("bge download: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_dir_under_data() {
        assert!(
            model_dir(std::path::Path::new("/tmp/uclaw"))
                .ends_with("models/bge-small-en-v1.5")
        );
    }

    #[test]
    fn is_present_requires_both_files() {
        let t = tempfile::tempdir().unwrap();
        assert!(!is_present(t.path()));
        std::fs::write(t.path().join("model.onnx"), b"x").unwrap();
        assert!(!is_present(t.path()));
        std::fs::write(t.path().join("tokenizer.json"), b"x").unwrap();
        assert!(is_present(t.path()));
    }
}
