//! Downloads bge-small-en-v1.5 ONNX + tokenizer into <data_dir>/models/bge-small-en-v1.5/
//! (HuggingFace primary, hf-mirror fallback). Idempotent: skips files already present.
//!
//! Mirrors the reqwest client construction and HF-then-mirror fallback idiom from
//! `stt/openflow/downloader.rs`: same 900s timeout, redirect policy, streaming
//! chunk-based download with atomic tmp→final rename.

use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

const HF_BASE: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main";
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

/// Download any missing files (HF then mirror). Errors propagate so the lazy caller can retry.
pub async fn ensure_model(dir: &Path) -> anyhow::Result<()> {
    if is_present(dir) {
        return Ok(());
    }
    std::fs::create_dir_all(dir)?;
    for (rel, local) in FILES {
        let dest = dir.join(local);
        if dest.exists() {
            continue;
        }
        let bytes = fetch_with_fallback(rel).await?;
        std::fs::write(&dest, &bytes)?;
    }
    Ok(())
}

async fn fetch_with_fallback(rel: &str) -> anyhow::Result<Vec<u8>> {
    // Mirror STT's reqwest client construction: 900s timeout, redirect policy, user-agent.
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("uclaw-backend/embed-downloader")
        .build()
        .map_err(|e| anyhow::anyhow!("reqwest client build: {e}"))?;

    let candidates = [
        format!("{}/{}", HF_BASE.trim_end_matches('/'), rel),
        format!("{}/{}", MIRROR_BASE.trim_end_matches('/'), rel),
    ];

    let mut errors: Vec<String> = Vec::new();
    for (idx, url) in candidates.iter().enumerate() {
        tracing::info!(
            source = idx + 1,
            total = candidates.len(),
            url = %url,
            "downloading bge-small file"
        );
        match fetch_one(&client, url).await {
            Ok(bytes) => return Ok(bytes),
            Err(e) => {
                errors.push(format!("{url} ({e})"));
                tracing::warn!(url = %url, error = %e, "download source failed, trying next");
            }
        }
    }

    anyhow::bail!(
        "all download sources failed:\n{}",
        errors
            .into_iter()
            .map(|s| format!(" - {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}

/// Streaming GET with chunk accumulation. Uses a tokio tmp file then reads back to Vec<u8>
/// for large files (model.onnx ~130 MB). Mirrors `download_one` in STT's downloader.
async fn fetch_one(client: &reqwest::Client, url: &str) -> anyhow::Result<Vec<u8>> {
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?
        .error_for_status()
        .map_err(|e| anyhow::anyhow!("server error: {e}"))?;

    // Write chunks to a temp file (avoids holding ~130 MB in RAM before writing).
    let tmp_path = {
        let mut p = std::env::temp_dir();
        p.push(format!("uclaw-embed-{}.tmp", uuid::Uuid::new_v4()));
        p
    };
    {
        let mut file = tokio::fs::File::create(&tmp_path)
            .await
            .map_err(|e| anyhow::anyhow!("create tmp file: {e}"))?;
        while let Some(chunk) = resp
            .chunk()
            .await
            .map_err(|e| anyhow::anyhow!("read stream: {e}"))?
        {
            file.write_all(&chunk)
                .await
                .map_err(|e| anyhow::anyhow!("write chunk: {e}"))?;
        }
        file.flush()
            .await
            .map_err(|e| anyhow::anyhow!("flush: {e}"))?;
    }

    let bytes = tokio::fs::read(&tmp_path)
        .await
        .map_err(|e| anyhow::anyhow!("read back tmp: {e}"))?;
    let _ = tokio::fs::remove_file(&tmp_path).await;
    Ok(bytes)
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
