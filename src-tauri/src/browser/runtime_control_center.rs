use serde::{Deserialize, Serialize};

use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderConfig {
    #[serde(default)]
    pub playwright_cli_enabled: bool,
    #[serde(default)]
    pub playwright_mcp_enabled: bool,
    #[serde(default = "default_provider_priority")]
    pub desired_priority: Vec<String>,
    #[serde(default = "default_fallback_provider")]
    pub default_fallback_provider: String,
    #[serde(default)]
    pub updated_at_ms: i64,
}

impl Default for BrowserRuntimeProviderConfig {
    fn default() -> Self {
        Self {
            playwright_cli_enabled: false,
            playwright_mcp_enabled: false,
            desired_priority: default_provider_priority(),
            default_fallback_provider: default_fallback_provider(),
            updated_at_ms: 0,
        }
    }
}

pub fn default_provider_priority() -> Vec<String> {
    vec![
        PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
        PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
    ]
}

fn default_fallback_provider() -> String {
    LOCAL_CHROMIUM_PROVIDER_ID.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
    use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
    use crate::browser::provider::LOCAL_CHROMIUM_PROVIDER_ID;

    #[test]
    fn provider_config_defaults_to_cli_mcp_local_priority_with_cli_mcp_off() {
        let config = BrowserRuntimeProviderConfig::default();

        assert!(!config.playwright_cli_enabled);
        assert!(!config.playwright_mcp_enabled);
        assert_eq!(
            config.desired_priority,
            vec![
                PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
                PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
                LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            ]
        );
        assert_eq!(config.default_fallback_provider, LOCAL_CHROMIUM_PROVIDER_ID);
    }
}
