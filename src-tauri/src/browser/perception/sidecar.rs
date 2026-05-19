use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::browser::perception::VisualPerceptionProviderKind;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualPerceptionSidecarConfig {
    pub provider: VisualPerceptionProviderKind,
    pub command: PathBuf,
    #[serde(default)]
    pub args: Vec<String>,
    pub timeout_ms: u64,
}

impl VisualPerceptionSidecarConfig {
    pub fn easyocr(command: PathBuf) -> Self {
        Self {
            provider: VisualPerceptionProviderKind::EasyOcr,
            command,
            args: Vec::new(),
            timeout_ms: 10_000,
        }
    }

    pub fn paddleocr(command: PathBuf) -> Self {
        Self {
            provider: VisualPerceptionProviderKind::PaddleOcr,
            command,
            args: Vec::new(),
            timeout_ms: 10_000,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sidecar_config_serializes_camelcase() {
        let config = VisualPerceptionSidecarConfig::easyocr(PathBuf::from("/usr/bin/easyocr"));
        let json = serde_json::to_string(&config).unwrap();
        assert!(json.contains("\"provider\":\"easy_ocr\""), "{json}");
        assert!(json.contains("\"timeoutMs\":10000"), "{json}");
    }
}
