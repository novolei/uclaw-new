use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::playwright_mcp::PlaywrightMcpAction;

pub const PLAYWRIGHT_MCP_SERVER_ID: &str = "playwright";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightMcpAdapterError {
    RawToolNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpAdapterToolCall {
    pub server_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

impl PlaywrightMcpAdapterToolCall {
    pub fn new(tool_name: &str, arguments: Value) -> Result<Self, PlaywrightMcpAdapterError> {
        validate_playwright_mcp_tool(tool_name)?;
        Ok(Self {
            server_id: PLAYWRIGHT_MCP_SERVER_ID.to_string(),
            tool_name: tool_name.to_string(),
            arguments,
        })
    }

    pub fn navigate(url: &str) -> Self {
        Self::new("browser_navigate", json!({ "url": url }))
            .expect("browser_navigate is allowlisted")
    }

    pub fn from_action(action: &PlaywrightMcpAction) -> Result<Self, PlaywrightMcpAdapterError> {
        match action {
            PlaywrightMcpAction::AccessibilitySnapshot { .. }
            | PlaywrightMcpAction::DiscoverLocators { .. } => {
                Self::new("browser_snapshot", json!({}))
            }
            PlaywrightMcpAction::Trace { .. } => Self::new("browser_start_tracing", json!({})),
            PlaywrightMcpAction::Navigate { url } => {
                Self::new("browser_navigate", json!({ "url": url }))
            }
            PlaywrightMcpAction::Click { locator } => Self::new(
                "browser_click",
                json!({ "element": locator, "ref": locator }),
            ),
            PlaywrightMcpAction::Type { locator, text } => Self::new(
                "browser_type",
                json!({ "element": locator, "ref": locator, "text": text }),
            ),
        }
    }
}

pub fn validate_playwright_mcp_tool(tool_name: &str) -> Result<(), PlaywrightMcpAdapterError> {
    if crate::mcp::playwright_mcp_tool_allowlist()
        .iter()
        .any(|allowed| allowed == tool_name)
    {
        Ok(())
    } else {
        Err(PlaywrightMcpAdapterError::RawToolNotAllowed)
    }
}

#[cfg(test)]
#[path = "playwright_mcp_adapter_tests.rs"]
mod playwright_mcp_adapter_tests;
