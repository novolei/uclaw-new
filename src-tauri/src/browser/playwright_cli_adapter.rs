#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightCliAdapterError {
    UnsupportedSkillCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightCliCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl PlaywrightCliCommand {
    pub fn install_skills() -> Self {
        Self {
            command: "playwright-cli".to_string(),
            args: vec!["install".to_string(), "--skills".to_string()],
        }
    }
}

pub struct PlaywrightCliActionCommand;

impl PlaywrightCliActionCommand {
    pub fn from_skill_command(
        command: &str,
    ) -> Result<PlaywrightCliCommand, PlaywrightCliAdapterError> {
        match command.trim() {
            "playwright-cli install --skills" => Ok(PlaywrightCliCommand::install_skills()),
            _ => Err(PlaywrightCliAdapterError::UnsupportedSkillCommand),
        }
    }
}

#[cfg(test)]
#[path = "playwright_cli_adapter_tests.rs"]
mod playwright_cli_adapter_tests;
