//! SenseVoice 模型下载器（vendored from open-flow `model_store.rs`，MIT，已删去 CLI 输出与配置写入逻辑）。
//!
//! 默认下载量化版（230 MB）；HuggingFace 主源失败时自动 fallback 到 hf-mirror.com。
//! 用 `progress_cb` 回调向 Tauri 前端推送下载进度。

#![allow(dead_code)]

use std::io::Write;
use std::path::{Path, PathBuf};

use tokio::io::AsyncWriteExt;

/// 量化版 base URL。
pub const MODEL_BASE_QUANTIZED: &str =
    "https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main";

/// FP16 版 base URL。
pub const MODEL_BASE_FP16: &str =
    "https://huggingface.co/ruska1117/SenseVoiceSmall-onnx-fp16/resolve/main";

/// HF 国内镜像 fallback。
pub const DEFAULT_HF_MIRROR_BASE: &str = "https://hf-mirror.com";

/// 量化版需要的文件 (远端文件名, 本地保存名, size_hint)。
pub const MODEL_FILES_QUANTIZED: &[(&str, &str, &str)] = &[
    ("model_quant.onnx", "model.onnx", "~230 MB"),
    ("am.mvn", "am.mvn", "~11 KB"),
    ("tokens.json", "tokens.json", "~344 KB"),
    ("config.yaml", "config.yaml", "~2 KB"),
];

/// FP16 版需要的文件。
pub const MODEL_FILES_FP16: &[(&str, &str, &str)] = &[
    ("model.onnx", "model.onnx", "~4.3 MB"),
    ("model.onnx.data", "model.onnx.data", "~446 MB"),
    ("am.mvn", "am.mvn", "~11 KB"),
    ("tokens.json", "tokens.json", "~344 KB"),
    ("config.yaml", "config.yaml", "~2 KB"),
];

/// 进度回调签名：(file_name, downloaded_bytes, total_bytes_optional)。
pub type ProgressCallback = Box<dyn Fn(&str, u64, Option<u64>) + Send + Sync>;

/// 预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SenseVoicePreset {
    Quantized,
    Fp16,
}

impl SenseVoicePreset {
    pub fn files(self) -> &'static [(&'static str, &'static str, &'static str)] {
        match self {
            Self::Quantized => MODEL_FILES_QUANTIZED,
            Self::Fp16 => MODEL_FILES_FP16,
        }
    }

    pub fn base_url(self) -> &'static str {
        match self {
            Self::Quantized => MODEL_BASE_QUANTIZED,
            Self::Fp16 => MODEL_BASE_FP16,
        }
    }
}

fn rewrite_huggingface_base(base_url: &str, mirror_root: &str) -> String {
    if let Some(rest) = base_url.strip_prefix("https://huggingface.co/") {
        format!("{}/{}", mirror_root.trim_end_matches('/'), rest)
    } else {
        base_url.to_string()
    }
}

fn model_base_candidates(base_url: &str) -> Vec<String> {
    let candidates = vec![
        base_url.trim_end_matches('/').to_string(),
        rewrite_huggingface_base(base_url, DEFAULT_HF_MIRROR_BASE),
    ];
    let mut deduped: Vec<String> = Vec::new();
    for c in candidates {
        if !deduped.contains(&c) {
            deduped.push(c);
        }
    }
    deduped
}

/// 下载完整模型到 `dest_dir`；已存在的文件跳过。
pub async fn download_all(
    dest_dir: &Path,
    preset: SenseVoicePreset,
    force: bool,
    progress: Option<ProgressCallback>,
) -> Result<PathBuf, String> {
    let base_candidates = model_base_candidates(preset.base_url());
    std::fs::create_dir_all(dest_dir)
        .map_err(|e| format!("无法创建目录 {}: {e}", dest_dir.display()))?;

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(900))
        .redirect(reqwest::redirect::Policy::limited(10))
        .user_agent("if2ai-backend/openflow-stt")
        .build()
        .map_err(|e| format!("reqwest client build: {e}"))?;

    for (remote_name, local_name, _size_hint) in preset.files() {
        let dest_path = dest_dir.join(local_name);
        if dest_path.exists() && !force {
            tracing::info!(file = %local_name, "SenseVoice 文件已存在，跳过");
            continue;
        }
        download_with_fallback(
            &client,
            &base_candidates,
            remote_name,
            local_name,
            &dest_path,
            progress.as_deref(),
        )
        .await?;
    }
    Ok(dest_dir.to_path_buf())
}

async fn download_with_fallback(
    client: &reqwest::Client,
    base_candidates: &[String],
    remote_name: &str,
    local_name: &str,
    dest: &Path,
    progress: Option<&(dyn Fn(&str, u64, Option<u64>) + Send + Sync)>,
) -> Result<(), String> {
    let mut errors = Vec::new();
    for (idx, base) in base_candidates.iter().enumerate() {
        let url = format!("{}/{}", base.trim_end_matches('/'), remote_name);
        tracing::info!(
            source = idx + 1,
            total = base_candidates.len(),
            url = %url,
            "下载 SenseVoice 文件"
        );
        match download_one(client, &url, dest, local_name, progress).await {
            Ok(()) => return Ok(()),
            Err(err) => {
                let _ = tokio::fs::remove_file(dest.with_extension("tmp")).await;
                errors.push(format!("{url} ({err})"));
                tracing::warn!(url = %url, error = %err, "下载源失败，尝试下一个");
            }
        }
    }
    Err(format!(
        "所有下载源均失败：\n{}",
        errors
            .into_iter()
            .map(|s| format!(" - {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    ))
}

async fn download_one(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    local_name: &str,
    progress: Option<&(dyn Fn(&str, u64, Option<u64>) + Send + Sync)>,
) -> Result<(), String> {
    let mut resp = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("HTTP 请求失败: {e}"))?
        .error_for_status()
        .map_err(|e| format!("服务器错误: {e}"))?;

    let total = resp.content_length();
    let mut downloaded: u64 = 0;
    let tmp = dest.with_extension("tmp");
    let mut file = tokio::fs::File::create(&tmp)
        .await
        .map_err(|e| format!("创建临时文件失败: {e}"))?;

    while let Some(chunk) = resp.chunk().await.map_err(|e| format!("读流失败: {e}"))? {
        file.write_all(&chunk)
            .await
            .map_err(|e| format!("写文件失败: {e}"))?;
        downloaded += chunk.len() as u64;
        if let Some(cb) = progress {
            cb(local_name, downloaded, total);
        }
    }

    file.flush().await.map_err(|e| format!("flush 失败: {e}"))?;
    drop(file);

    tokio::fs::rename(&tmp, dest)
        .await
        .map_err(|e| format!("rename 失败: {e}"))?;

    // 静默落 stdout 进度（用于 cargo run 时人眼观察）
    let _ = std::io::stdout().lock().flush();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrite_huggingface_base_replaces_host() {
        let out = rewrite_huggingface_base(
            "https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main",
            "https://hf-mirror.com",
        );
        assert_eq!(
            out,
            "https://hf-mirror.com/haixuantao/SenseVoiceSmall-onnx/resolve/main"
        );
    }

    #[test]
    fn rewrite_huggingface_base_passes_through_non_hf() {
        let out = rewrite_huggingface_base("https://example.com/foo", "https://hf-mirror.com");
        assert_eq!(out, "https://example.com/foo");
    }

    #[test]
    fn model_base_candidates_dedups_and_includes_mirror() {
        let cs = model_base_candidates(
            "https://huggingface.co/haixuantao/SenseVoiceSmall-onnx/resolve/main",
        );
        assert_eq!(cs.len(), 2);
        assert!(cs[0].contains("huggingface.co"));
        assert!(cs[1].contains("hf-mirror.com"));
    }
}
