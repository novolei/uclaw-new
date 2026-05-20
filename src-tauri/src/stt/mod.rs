//! STT 模块 —— 仅支持 OpenFlow (SenseVoice ONNX)，从 if2Ai 直接复刻。
//!
//! 架构：
//!   前端 MediaRecorder (PCM16LE) ──base64──▶ stt_transcribe (Tauri cmd)
//!                                              ├─ 解码 bytes → f32
//!                                              └─ OpenFlowAsrEngine.transcribe
//!                                                    ├─ 内部重采样到 16kHz
//!                                                    └─ ONNX 推理 → 文本

#![allow(dead_code)]

pub mod commands;
pub mod openflow;
pub mod settings;

pub use commands::transcribe_samples;

/// STT 转写结果（OpenFlow / SenseVoice engine 共享）。
#[derive(Debug, Clone)]
pub struct TranscribeResult {
    pub text: String,
    /// 语言代码（"zh" / "en" 等）；auto-detect 时由模型推断。
    pub language: String,
    /// 耗时（秒）。
    pub elapsed_seconds: f32,
}
