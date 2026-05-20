//! Open Flow ASR 引擎对外封装：
//!
//! 给 `commands::stt` 调用：传入 PCM f32 + 采样率 → 返回 `TranscribeResult { text, language, elapsed }`。
//!
//! 引擎本身懒加载（首次 transcribe 时才 load ONNX session），后续 transcribe 复用同一个 session。
//! 用 `tokio::sync::Mutex` 保护 session（ort `Session::run` 需要 `&mut self`）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use tokio::sync::Mutex;

use super::decoder::CTCDecoder;
use super::onnx_inference::OnnxInference;
use super::preprocess::{AudioPreprocessor, TARGET_SAMPLE_RATE};
use crate::stt::TranscribeResult;

/// SenseVoice 模型目录的标准结构（与 open-flow `model_store::model_is_ready` 对齐）：
/// ```text
/// <model_dir>/
///   model.onnx        # ONNX 权重（量化版，约 230MB）
///   tokens.json       # CTC 词表
///   am.mvn            # CMVN 归一化参数
///   config.yaml       # 元信息（暂未用到，保留方便诊断）
/// ```
pub fn model_is_ready(dir: &Path) -> bool {
    if !dir.is_dir() {
        return false;
    }
    let has_model = dir.join("model.onnx").exists() || dir.join("model_quant.onnx").exists();
    let has_tokens = dir.join("tokens.json").exists();
    has_model && has_tokens
}

/// 默认 SenseVoice 模型目录：`~/.uclaw/models/sensevoice/`。
pub fn default_sensevoice_dir() -> PathBuf {
    uclaw_utils_home::uclaw_home_pathbuf()
        .unwrap_or_else(|_| PathBuf::from("./.uclaw"))
        .join("models")
        .join("sensevoice")
}

/// 内部 session bundle（懒加载后驻留内存）。
struct LoadedSession {
    preprocessor: AudioPreprocessor,
    inference: OnnxInference,
    decoder: CTCDecoder,
}

/// 对外 ASR 引擎：线程安全 + 懒加载。
pub struct OpenFlowAsrEngine {
    model_dir: PathBuf,
    inner: Mutex<Option<LoadedSession>>,
}

impl OpenFlowAsrEngine {
    /// 用模型目录构造引擎；不立即加载 ONNX。
    pub fn new(model_dir: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            model_dir,
            inner: Mutex::new(None),
        })
    }

    /// 模型目录是否就绪（可调度 transcribe）。
    pub fn is_ready(&self) -> bool {
        model_is_ready(&self.model_dir)
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// 懒加载 + 缓存 session。失败时返回字符串错误。
    async fn ensure_loaded(&self) -> Result<(), String> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        if !model_is_ready(&self.model_dir) {
            return Err(format!(
                "SenseVoice 模型未就绪：{}（缺 model.onnx 或 tokens.json）",
                self.model_dir.display()
            ));
        }

        // ort 默认 features 含 download-binaries：onnxruntime 已在 cargo build
        // 时静态链接，运行时无需任何 dlopen 准备。

        let model_path = if self.model_dir.join("model.onnx").exists() {
            self.model_dir.join("model.onnx")
        } else {
            self.model_dir.join("model_quant.onnx")
        };
        let tokens_path = self.model_dir.join("tokens.json");
        let cmvn_path = self.model_dir.join("am.mvn");

        let model_dir = self.model_dir.clone();
        let loaded = tokio::task::spawn_blocking(move || -> Result<LoadedSession, String> {
            let mut preprocessor = AudioPreprocessor::new(TARGET_SAMPLE_RATE);
            if cmvn_path.exists() {
                preprocessor
                    .load_cmvn_from_file(&cmvn_path)
                    .map_err(|e| format!("加载 CMVN 失败: {e}"))?;
            } else {
                tracing::warn!(
                    dir = %model_dir.display(),
                    "am.mvn 不存在，跳过 CMVN（识别效果会下降）"
                );
            }
            let inference =
                OnnxInference::new(&model_path).map_err(|e| format!("加载 ONNX 失败: {e}"))?;
            let decoder = CTCDecoder::from_tokens_file(&tokens_path)
                .map_err(|e| format!("加载 tokens.json 失败: {e}"))?;
            Ok(LoadedSession {
                preprocessor,
                inference,
                decoder,
            })
        })
        .await
        .map_err(|e| format!("spawn_blocking join: {e}"))??;

        *guard = Some(loaded);
        Ok(())
    }

    /// 对 PCM f32 (任意采样率) 做转写，返回 `TranscribeResult`。
    ///
    /// `language`：可选 ISO 语言代码（"zh"/"en"/"yue"/"ja"/"ko"/"auto"）。`None` → "auto"。
    pub async fn transcribe(
        self: &Arc<Self>,
        audio: Vec<f32>,
        sample_rate: u32,
        language: Option<&str>,
    ) -> Result<TranscribeResult, String> {
        let start = Instant::now();
        self.ensure_loaded().await?;

        let language_owned = language.map(|s| s.to_string());

        let this = self.clone();
        let result = tokio::task::spawn_blocking(move || -> Result<(String, String), String> {
            // 用 blocking lock：spawn_blocking 中已经在 OS 线程中，避免阻塞 tokio reactor
            let mut guard = this.inner.blocking_lock();
            let session = guard
                .as_mut()
                .ok_or_else(|| "session not loaded".to_string())?;

            let lang_id = resolve_language_id(language_owned.as_deref());
            let textnorm_id = 1; // withitn：默认带标点

            let features = session
                .preprocessor
                .process(&audio, sample_rate)
                .map_err(|e| format!("预处理失败: {e}"))?;
            let (logits, _enc_lens) = session
                .inference
                .infer(&features, lang_id, textnorm_id)
                .map_err(|e| format!("ONNX 推理失败: {e}"))?;
            let text = session.decoder.decode(&logits, false);

            let detected_lang = language_owned
                .clone()
                .unwrap_or_else(|| lang_id_to_str(lang_id).to_string());
            Ok((text, detected_lang))
        })
        .await
        .map_err(|e| format!("spawn_blocking join: {e}"))??;

        Ok(TranscribeResult {
            text: result.0.trim().to_string(),
            language: result.1,
            elapsed_seconds: start.elapsed().as_secs_f32(),
        })
    }
}

/// language → SenseVoice ONNX 的 `language` 输入 id（与 FunASR/sherpa 导出一致）：
/// auto=0, zh=3, en=4, yue=5, ja=6, ko=7, nospeech=8。
fn resolve_language_id(language: Option<&str>) -> i32 {
    match language.unwrap_or("auto") {
        "auto" => 0,
        "zh" | "zh-cn" | "cn" => 3,
        "en" => 4,
        "yue" => 5,
        "ja" => 6,
        "ko" => 7,
        "nospeech" => 8,
        _ => 0,
    }
}

fn lang_id_to_str(id: i32) -> &'static str {
    match id {
        3 => "zh",
        4 => "en",
        5 => "yue",
        6 => "ja",
        7 => "ko",
        _ => "auto",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_dir_is_under_uclaw_home() {
        let dir = default_sensevoice_dir();
        let s = dir.to_string_lossy();
        assert!(s.contains(".uclaw"), "default dir should sit under ~/.uclaw, got {s}");
        assert!(s.ends_with("models/sensevoice"), "default dir should end in models/sensevoice, got {s}");
    }

    #[test]
    fn model_is_ready_false_for_missing_dir() {
        let p = PathBuf::from("/nonexistent/uclaw-test/stt");
        assert!(!model_is_ready(&p), "missing dir should report not-ready");
    }

    #[test]
    fn model_is_ready_false_for_empty_dir() {
        let tmp = tempfile::tempdir().expect("tempdir");
        assert!(!model_is_ready(tmp.path()), "empty dir should report not-ready");
    }

    #[test]
    fn model_is_ready_true_when_required_files_present() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(tmp.path().join("model.onnx"), b"fake").expect("write");
        std::fs::write(tmp.path().join("tokens.json"), b"{}").expect("write");
        assert!(model_is_ready(tmp.path()), "should be ready with model.onnx + tokens.json");
    }
}
