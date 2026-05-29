//! `uclaw` MCP capability extension.
//!
//! Plugins that opt in advertise `"uclaw": { ... }` in their MCP
//! `initialize` response. uClaw clients (PluginRegistrar) detect this
//! and register the additional contribution kinds (hooks, renderers,
//! commands beyond standard MCP).

use serde::{Deserialize, Serialize};

/// uClaw extension capability advertised in the MCP initialize response.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct UclawCapability {
    /// Extension version. Currently "1.0".
    pub version: String,
    /// Hooks the plugin wants to listen to.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hooks: Vec<String>,
    /// Renderers the plugin contributes.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub renderers: Vec<String>,
}

/// Outcome of the uclaw capability detection from an MCP InitializeResult.
#[derive(Debug, Clone)]
pub enum UclawCapabilityNegotiation {
    /// No uclaw extension advertised — plain MCP plugin.
    Absent,
    /// uclaw extension present with the given capability.
    Present(UclawCapability),
}

impl UclawCapabilityNegotiation {
    /// Detect from an MCP server's InitializeResult's capabilities object.
    /// Returns Absent if no `uclaw` key; Present with parsed payload otherwise.
    pub fn detect_from_capabilities(capabilities: &serde_json::Value) -> Self {
        let Some(uclaw) = capabilities.get("uclaw") else {
            return Self::Absent;
        };
        match serde_json::from_value::<UclawCapability>(uclaw.clone()) {
            Ok(cap) => Self::Present(cap),
            Err(_) => Self::Absent,
        }
    }
}
