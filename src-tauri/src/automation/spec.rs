use serde::{Deserialize, Serialize};

/// Top-level structure of an automation TOML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationSpec {
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub trigger: TriggerConfig,
    pub task: String,
    #[serde(default)]
    pub max_iterations: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TriggerConfig {
    /// Cron expression — runs on a schedule (e.g. "0 9 * * 1-5" = 9am weekdays)
    Cron { expression: String },
    /// Run once at a specific RFC-3339 datetime
    Once { at: String },
    /// Triggered manually via UI / API only
    Manual,
}

impl AutomationSpec {
    pub fn from_toml(content: &str) -> Result<Self, String> {
        toml::from_str(content).map_err(|e| format!("TOML parse error: {}", e))
    }

    /// Validate that the trigger expression is well-formed.
    pub fn validate(&self) -> Result<(), String> {
        match &self.trigger {
            TriggerConfig::Cron { expression } => {
                expression.parse::<cron::Schedule>()
                    .map_err(|e| format!("Invalid cron expression '{}': {}", expression, e))?;
            }
            TriggerConfig::Once { at } => {
                at.parse::<chrono::DateTime<chrono::Utc>>()
                    .map_err(|e| format!("Invalid datetime '{}': {}", at, e))?;
            }
            TriggerConfig::Manual => {}
        }
        if self.task.trim().is_empty() {
            return Err("task must not be empty".to_string());
        }
        Ok(())
    }
}

/// Database row for an automation spec.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationSpecRow {
    pub id: String,
    pub name: String,
    pub description: String,
    pub toml_content: String,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

impl AutomationSpecRow {
    pub fn parse_spec(&self) -> Result<AutomationSpec, String> {
        AutomationSpec::from_toml(&self.toml_content)
    }
}
