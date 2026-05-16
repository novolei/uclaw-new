use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::memory_graph::recall::MemoryRecallConfigDto;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub language: String,
    pub theme: String,
    /// Optional monthly budget in USD. None disables budget alerts.
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,
    /// Memory recall configuration. When None, MemoryRecallConfig::default() is used.
    /// Persisted in config.json; hot-reloaded every agent turn.
    #[serde(default)]
    pub memory_recall_config: Option<MemoryRecallConfigDto>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: "light".to_string(),
            monthly_budget_usd: None,
            memory_recall_config: None,
        }
    }
}

impl UserSettings {
    pub fn load(path: &PathBuf) -> Result<Self, crate::error::Error> {
        if path.exists() {
            let content = std::fs::read_to_string(path)
                .map_err(|e| crate::error::Error::Io(e))?;
            let settings: UserSettings = serde_json::from_str(&content)
                .map_err(|e| crate::error::Error::Serde(e))?;
            Ok(settings)
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
