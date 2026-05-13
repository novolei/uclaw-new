//! Open Flow SenseVoice ONNX 本地 ASR 后端（vendored from
//! https://github.com/jqlong17/open-flow，MIT）。Apr 2026 起 uClaw 直接复刻
//! if2Ai 的精简单 backend 实现。完整 LICENSE 见 if2Ai 的
//! docs/third-party-licenses/open-flow-LICENSE.md。

pub mod decoder;
pub mod downloader;
pub mod engine;
pub mod onnx_inference;
pub mod ort_loader;
pub mod preprocess;

pub use downloader::{download_all, SenseVoicePreset};
pub use engine::{default_sensevoice_dir, model_is_ready, OpenFlowAsrEngine};
