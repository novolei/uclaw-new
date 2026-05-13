//! STT Tauri 命令（精简版，仅 OpenFlow / SenseVoice）。
//!
//! 历史：早期支持 whisper.cpp 本地 + Groq 云端 + OpenFlow 三 backend；Apr 2026
//! 起精简为仅 OpenFlow（见 `modules/stt/mod.rs` 的 backend 取舍说明）。

use std::sync::Arc;

use base64::Engine;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use tokio::sync::Mutex;

use crate::stt::openflow::{
    default_sensevoice_dir, download_all as openflow_download_all,
    model_is_ready as openflow_model_is_ready, OpenFlowAsrEngine, SenseVoicePreset,
};
use crate::stt::settings::{
    load as load_stt_settings, save as save_stt_settings, SttSettings,
};

/// 全局 OpenFlow 引擎（懒加载 + 跨命令复用）。
/// `None` = 还未实例化或模型不可用；首次 transcribe 时尝试构造。
static OPENFLOW_ENGINE: Lazy<Mutex<Option<Arc<OpenFlowAsrEngine>>>> =
    Lazy::new(|| Mutex::new(None));

async fn ensure_openflow_engine() -> Result<Arc<OpenFlowAsrEngine>, String> {
    let mut guard = OPENFLOW_ENGINE.lock().await;
    if let Some(engine) = guard.as_ref() {
        return Ok(engine.clone());
    }
    let dir = default_sensevoice_dir();
    if !openflow_model_is_ready(&dir) {
        return Err(format!(
            "SenseVoice 模型未下载。请去 设置 → STT 语音输入 → 一键下载（约 230MB）。期望路径：{}",
            dir.display()
        ));
    }
    let engine = OpenFlowAsrEngine::new(dir);
    *guard = Some(engine.clone());
    Ok(engine)
}

// ── Status ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SttModelStatus {
    /// SenseVoice (OpenFlow) 模型是否就绪
    pub openflow_ready: bool,
    /// SenseVoice 模型目录（即使没下载也告诉前端期望路径）
    pub openflow_model_dir: String,
}

#[tauri::command]
pub async fn stt_model_status() -> Result<SttModelStatus, String> {
    let dir = default_sensevoice_dir();
    Ok(SttModelStatus {
        openflow_ready: openflow_model_is_ready(&dir),
        openflow_model_dir: dir.to_string_lossy().to_string(),
    })
}

// ── Settings ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct SttSettingsDto {
    /// 始终为 "openflow"（保留字段以兼容旧前端）
    pub provider: String,
}

impl From<&SttSettings> for SttSettingsDto {
    fn from(_: &SttSettings) -> Self {
        Self {
            provider: "openflow".to_string(),
        }
    }
}

#[tauri::command]
pub async fn stt_get_settings(app: tauri::AppHandle) -> Result<SttSettingsDto, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    let s = load_stt_settings(&dir);
    Ok((&s).into())
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct SaveSttSettingsRequest {
    /// 接受但忽略（保留字段以兼容旧前端）
    pub provider: Option<String>,
}

#[tauri::command]
pub async fn stt_save_settings(
    app: tauri::AppHandle,
    request: SaveSttSettingsRequest,
) -> Result<SttSettingsDto, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("app_data_dir: {e}"))?;
    let _ = request; // provider override 已无意义
    let current = load_stt_settings(&dir);
    save_stt_settings(&dir, &current)?;
    Ok((&current).into())
}

// ── Transcribe ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct SttTranscribeRequest {
    /// PCM16LE base64 字节
    pub audio_bytes_base64: String,
    pub language: Option<String>,
    pub sample_rate: Option<u32>,
    /// 接受但忽略（保留字段以兼容旧前端）
    pub provider_override: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SttTranscribeResponse {
    pub text: String,
    pub language: String,
    pub elapsed_seconds: f32,
    pub provider: String,
}

#[tauri::command]
pub async fn stt_transcribe(
    _app: tauri::AppHandle,
    request: SttTranscribeRequest,
) -> Result<SttTranscribeResponse, String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(&request.audio_bytes_base64)
        .map_err(|e| format!("base64 解码失败: {e}"))?;
    if bytes.is_empty() {
        return Err("音频数据为空".to_string());
    }
    let sample_rate = request.sample_rate.unwrap_or(16_000);
    let language = request.language.clone();

    let engine = ensure_openflow_engine().await?;
    // PCM16LE → f32（任意采样率，SenseVoice 内部会重采样到 16kHz）
    let pcm_f32: Vec<f32> = bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]) as f32 / 32768.0)
        .collect();
    let result = engine
        .transcribe(pcm_f32, sample_rate, language.as_deref())
        .await?;
    tracing::info!(provider="openflow", text=%result.text, "STT done");
    Ok(SttTranscribeResponse {
        text: result.text,
        language: result.language,
        elapsed_seconds: result.elapsed_seconds,
        provider: "openflow".to_string(),
    })
}

// ── OpenFlow Model Download ──────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenFlowDownloadProgress {
    pub file: String,
    pub downloaded: u64,
    pub total: Option<u64>,
    /// 0-100；total 未知时为 -1
    pub percent: i32,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DownloadOpenflowRequest {
    /// "quantized" / "fp16"，默认 quantized
    pub preset: Option<String>,
    pub force: Option<bool>,
}

/// 下载 SenseVoice 模型；过程中通过 `stt:openflow-download-progress` 事件推进度。
#[tauri::command]
pub async fn stt_download_model(
    app: tauri::AppHandle,
    request: DownloadOpenflowRequest,
) -> Result<String, String> {
    let preset = match request.preset.as_deref() {
        Some("fp16") => SenseVoicePreset::Fp16,
        _ => SenseVoicePreset::Quantized,
    };
    let force = request.force.unwrap_or(false);
    let dest = default_sensevoice_dir();

    let app_clone = app.clone();
    let cb: crate::stt::openflow::downloader::ProgressCallback =
        Box::new(move |file: &str, downloaded: u64, total: Option<u64>| {
            let percent = match total {
                Some(t) if t > 0 => ((downloaded * 100) / t) as i32,
                _ => -1,
            };
            let _ = app_clone.emit(
                "stt:openflow-download-progress",
                OpenFlowDownloadProgress {
                    file: file.to_string(),
                    downloaded,
                    total,
                    percent,
                },
            );
        });

    tracing::info!(dest = %dest.display(), "下载 SenseVoice 模型开始");
    let result_dir = openflow_download_all(&dest, preset, force, Some(cb)).await?;

    // 重置引擎缓存，下次 transcribe 重新加载新文件
    {
        let mut guard = OPENFLOW_ENGINE.lock().await;
        *guard = None;
    }

    Ok(result_dir.to_string_lossy().to_string())
}

// ── ONNX Runtime pre-warm ────────────────────────────────────────────────

/// 下载 ONNX Runtime dylib 并设置 ORT_DYLIB_PATH；幂等，已下载时立即返回路径。
/// 过程中通过 `stt:runtime_progress` 事件推送 `{phase, downloaded, total}` 进度。
#[tauri::command]
pub async fn stt_ensure_runtime(
    app: tauri::AppHandle,
) -> Result<String, String> {
    let handle = app.clone();
    let progress: crate::stt::openflow::ort_loader::ProgressCallback =
        std::sync::Arc::new(move |phase: &str, done: u64, total: Option<u64>| {
            let _ = tauri::Emitter::emit(
                &handle,
                "stt:runtime_progress",
                serde_json::json!({
                    "phase": phase,
                    "downloaded": done,
                    "total": total,
                }),
            );
        });
    crate::stt::openflow::ort_loader::ensure_onnxruntime(Some(progress))
        .await
        .map(|p| p.to_string_lossy().into_owned())
        .map_err(|e| e.to_string())
}

// ── Microphones (browser-side enumeration in v0; Tauri stub for future) ─

#[derive(Debug, Clone, Serialize)]
pub struct SttMicrophone {
    pub device_id: String,
    pub label: String,
}

#[tauri::command]
pub async fn stt_list_microphones() -> Result<Vec<SttMicrophone>, String> {
    // v0: 由前端 navigator.mediaDevices.enumerateDevices() 直接做枚举；
    //     这里返回空数组占位，方便将来切到原生（cpal）枚举不改前端契约。
    Ok(Vec::new())
}
