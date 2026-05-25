use serde::{Deserialize, Serialize};

use crate::browser::playwright_cli::PLAYWRIGHT_CLI_PROVIDER_ID;
use crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID;
use crate::browser::provider::{BrowserProviderStatus, LOCAL_CHROMIUM_PROVIDER_ID};
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::error::Error;

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

impl BrowserRuntimeProviderConfig {
    pub fn set_enabled(&mut self, provider_id: &str, enabled: bool) -> Result<(), Error> {
        match provider_id {
            PLAYWRIGHT_CLI_PROVIDER_ID => {
                self.playwright_cli_enabled = enabled;
                self.updated_at_ms = chrono::Utc::now().timestamp_millis();
                Ok(())
            }
            PLAYWRIGHT_MCP_PROVIDER_ID => {
                self.playwright_mcp_enabled = enabled;
                self.updated_at_ms = chrono::Utc::now().timestamp_millis();
                Ok(())
            }
            LOCAL_CHROMIUM_PROVIDER_ID => Ok(()),
            _ => Err(Error::Internal(format!(
                "Unknown browser runtime provider: {provider_id}"
            ))),
        }
    }

    pub fn set_priority(&mut self, provider_ids: Vec<String>) -> Result<(), Error> {
        let mut normalized = Vec::new();
        for provider_id in provider_ids {
            validate_provider_id(&provider_id)?;
            if !normalized.iter().any(|known| known == &provider_id) {
                normalized.push(provider_id);
            }
        }
        for provider_id in default_provider_priority() {
            if !normalized.iter().any(|known| known == &provider_id) {
                normalized.push(provider_id);
            }
        }
        self.desired_priority = normalized;
        self.updated_at_ms = chrono::Utc::now().timestamp_millis();
        Ok(())
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserRuntimeRouteRole {
    DesiredFirst,
    Desired,
    Active,
    Fallback,
    Disabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeActiveProviderRoute {
    pub provider_id: String,
    pub display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeProviderLane {
    pub provider_id: String,
    pub display_name: String,
    pub enabled: bool,
    pub priority_rank: usize,
    pub readiness: String,
    pub routable: bool,
    pub route_role: BrowserRuntimeRouteRole,
    pub probe_state: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_reason: Option<String>,
    pub next_action: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_probe_artifact: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeControlCenterReport {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub desired_provider_priority: Vec<String>,
    pub active_provider_route: BrowserRuntimeActiveProviderRoute,
    pub provider_lanes: Vec<BrowserRuntimeProviderLane>,
    pub mcp_integration_summary: BrowserRuntimeMcpIntegrationSummary,
    pub updated_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeMcpIntegrationSummary {
    pub built_in: bool,
    pub enabled: bool,
    pub raw_tools_exposed: bool,
    pub configure_route_ready: bool,
}

pub fn build_control_center_report(
    config: BrowserRuntimeProviderConfig,
    runtime_pack_ready: bool,
    providers: &[BrowserProviderStatus],
) -> BrowserRuntimeControlCenterReport {
    let flags = feature_flags_from_provider_config(&config);
    let mut lanes = Vec::new();
    let mut active_provider_route = None;

    for (index, provider_id) in config.desired_priority.iter().enumerate() {
        let status = providers
            .iter()
            .find(|provider| provider.provider_id == *provider_id);
        let enabled = provider_enabled(provider_id, &config);
        let requires_pack =
            provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || provider_id == PLAYWRIGHT_MCP_PROVIDER_ID;
        let requires_probe =
            provider_id == PLAYWRIGHT_CLI_PROVIDER_ID || provider_id == PLAYWRIGHT_MCP_PROVIDER_ID;
        let readiness = status
            .map(|status| format!("{:?}", status.readiness).to_lowercase())
            .unwrap_or_else(|| "unavailable".to_string());
        let fallback_reason = if !enabled {
            Some("provider_disabled".to_string())
        } else if requires_pack && !runtime_pack_ready {
            Some("runtime_pack_not_ready".to_string())
        } else if requires_probe {
            Some("probe_not_passed".to_string())
        } else {
            None
        };
        let routable = fallback_reason.is_none();

        if routable && active_provider_route.is_none() {
            active_provider_route = Some(BrowserRuntimeActiveProviderRoute {
                provider_id: provider_id.clone(),
                display_name: provider_display_name(provider_id).to_string(),
                fallback_reason: None,
            });
        }

        lanes.push(BrowserRuntimeProviderLane {
            provider_id: provider_id.clone(),
            display_name: provider_display_name(provider_id).to_string(),
            enabled,
            priority_rank: index + 1,
            readiness,
            routable,
            route_role: if index == 0 {
                BrowserRuntimeRouteRole::DesiredFirst
            } else {
                BrowserRuntimeRouteRole::Desired
            },
            probe_state: if provider_id == LOCAL_CHROMIUM_PROVIDER_ID {
                "passed".to_string()
            } else {
                "not_run".to_string()
            },
            fallback_reason: fallback_reason.clone(),
            next_action: next_action_for_lane(provider_id, enabled, fallback_reason.as_deref()),
            last_probe_artifact: None,
        });
    }

    let active_provider_route =
        active_provider_route.unwrap_or_else(|| BrowserRuntimeActiveProviderRoute {
            provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            display_name: "Local Chromium".to_string(),
            fallback_reason: Some("all_preferred_providers_unavailable".to_string()),
        });

    for lane in &mut lanes {
        if lane.provider_id == active_provider_route.provider_id {
            lane.route_role = BrowserRuntimeRouteRole::Active;
        } else if !lane.enabled {
            lane.route_role = BrowserRuntimeRouteRole::Disabled;
        }
    }

    BrowserRuntimeControlCenterReport {
        feature_flags: flags,
        desired_provider_priority: config.desired_priority.clone(),
        active_provider_route,
        provider_lanes: lanes,
        mcp_integration_summary: BrowserRuntimeMcpIntegrationSummary {
            built_in: true,
            enabled: config.playwright_mcp_enabled,
            raw_tools_exposed: false,
            configure_route_ready: false,
        },
        updated_at_ms: config.updated_at_ms,
    }
}

pub fn feature_flags_from_provider_config(
    config: &BrowserRuntimeProviderConfig,
) -> BrowserRuntimeFeatureFlags {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = config.playwright_cli_enabled;
    flags.playwright_mcp = config.playwright_mcp_enabled;
    flags
}

fn provider_enabled(provider_id: &str, config: &BrowserRuntimeProviderConfig) -> bool {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID => config.playwright_cli_enabled,
        PLAYWRIGHT_MCP_PROVIDER_ID => config.playwright_mcp_enabled,
        LOCAL_CHROMIUM_PROVIDER_ID => true,
        _ => false,
    }
}

fn provider_display_name(provider_id: &str) -> &'static str {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID => "Playwright CLI",
        PLAYWRIGHT_MCP_PROVIDER_ID => "Playwright MCP",
        LOCAL_CHROMIUM_PROVIDER_ID => "Local Chromium",
        _ => "Unknown provider",
    }
}

fn next_action_for_lane(provider_id: &str, enabled: bool, fallback_reason: Option<&str>) -> String {
    if !enabled {
        return if provider_id == PLAYWRIGHT_MCP_PROVIDER_ID {
            "enable_mcp"
        } else {
            "enable_provider"
        }
        .to_string();
    }

    match fallback_reason {
        Some("runtime_pack_not_ready") => "prepare_runtime_pack",
        Some("probe_not_passed") => "run_probe",
        Some(_) => "view_details",
        None => "none",
    }
    .to_string()
}

fn validate_provider_id(provider_id: &str) -> Result<(), Error> {
    match provider_id {
        PLAYWRIGHT_CLI_PROVIDER_ID | PLAYWRIGHT_MCP_PROVIDER_ID | LOCAL_CHROMIUM_PROVIDER_ID => {
            Ok(())
        }
        _ => Err(Error::Internal(format!(
            "Unknown browser runtime provider: {provider_id}"
        ))),
    }
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
