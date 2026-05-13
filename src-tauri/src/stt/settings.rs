//! STT 设置（精简版，仅 OpenFlow）。
//!
//! 设置文件 `<app_data>/stt_settings.json`。Apr 2026 起 provider 字段实际只剩
//! OpenFlow 一个，保留 enum/字段是为了：
//!   1. 老用户的设置文件 (`provider: "whisper" | "groq"`) 反序列化不报错
//!   2. 未来重新引入第二个 backend 时不需要破坏 schema
//!
//! 老 provider 值反序列化时会被 `From<&SttSettings> for SttSettingsDto`
//! 静默归一化为 `"openflow"`（见 commands/stt.rs）。

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SttProvider {
    /// 本地 SenseVoice ONNX（vendored from open-flow）
    #[default]
    #[serde(rename = "openflow", alias = "whisper", alias = "groq")]
    OpenFlow,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub struct SttSettings {
    pub provider: SttProvider,
}

fn settings_path(app_data_dir: &std::path::Path) -> PathBuf {
    app_data_dir.join("stt_settings.json")
}

pub fn load(app_data_dir: &std::path::Path) -> SttSettings {
    let path = settings_path(app_data_dir);
    if !path.exists() {
        return SttSettings::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<SttSettings>(&s).ok())
        .unwrap_or_default()
}

pub fn save(app_data_dir: &std::path::Path, settings: &SttSettings) -> Result<(), String> {
    std::fs::create_dir_all(app_data_dir).map_err(|e| format!("create dir: {e}"))?;
    let path = settings_path(app_data_dir);
    let content = serde_json::to_string_pretty(settings).map_err(|e| format!("serialize: {e}"))?;
    std::fs::write(&path, content).map_err(|e| format!("write: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_returns_default_when_file_missing() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let s = load(tmp.path());
        assert_eq!(s.provider, SttProvider::OpenFlow);
    }

    #[test]
    fn save_then_load_roundtrip() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let s = SttSettings::default();
        save(tmp.path(), &s).expect("save ok");
        let loaded = load(tmp.path());
        assert_eq!(loaded.provider, SttProvider::OpenFlow);
    }

    #[test]
    fn legacy_whisper_provider_alias_loads_as_openflow() {
        // 老用户的 settings 文件可能写着 provider="whisper" 或 "groq"，
        // SttProvider 的 serde alias 应当兼容并归一化到 OpenFlow。
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::write(
            tmp.path().join("stt_settings.json"),
            br#"{"provider":"whisper"}"#,
        )
        .expect("write");
        let s = load(tmp.path());
        assert_eq!(s.provider, SttProvider::OpenFlow);
    }
}
