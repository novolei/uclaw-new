use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

impl Default for LlmConfig {
    fn default() -> Self {
        Self {
            provider: "anthropic".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_key: String::new(),
            base_url: None,
            max_tokens: Some(16384),
            temperature: Some(0.7),
        }
    }
}

impl LlmConfig {
    pub fn load(path: &PathBuf) -> Result<Self, crate::error::Error> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| crate::error::Error::Io(e))?;
            let config: LlmConfig = serde_json::from_str(&content)
                .map_err(|e| crate::error::Error::Serde(e))?;
            Ok(config)
        } else {
            Ok(Self::default())
        }
    }

    pub fn save(&self, path: &PathBuf) -> Result<(), crate::error::Error> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| crate::error::Error::Io(e))?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| crate::error::Error::Serde(e))?;
        std::fs::write(path, content)
            .map_err(|e| crate::error::Error::Io(e))?;
        Ok(())
    }
}
