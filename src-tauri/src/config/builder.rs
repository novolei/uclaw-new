use crate::config::LlmConfig;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppConfig {
    pub llm: LlmConfig,
    pub workspace_path: PathBuf,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            llm: LlmConfig::default(),
            workspace_path: dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from(".")),
        }
    }
}
