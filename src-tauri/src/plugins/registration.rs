//! Manifest → AgentApi registration routing.
//!
//! Reads `LoadedPlugin` (manifest + paths) and registers its `PluginContribution`
//! fields into the appropriate handles:
//! - tools → AgentApi.register_tool with ToolDescriptors. The builder closure
//!   is a placeholder in Task 2; Task 3 swaps in a real McpToolProxy.
//! - commands → recorded to summary (real wiring deferred to follow-up).
//! - mcp_servers → recorded; Task 3 wires McpManager integration.
//! - skills, themes → recorded; no registration (future PRs).

use std::sync::Arc;

use crate::agent::api::AgentApi;
use crate::agent::api::tool::ToolDescriptor;
use crate::plugins::discovery::LoadedPlugin;

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {
    #[error("plugin {0} contributes 0 items")]
    EmptyContribution(String),
}

/// Summary of what was registered for a plugin.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistrationSummary {
    pub plugin_id: String,
    pub tools_registered: Vec<String>,
    pub commands_registered: Vec<String>,
    pub mcp_servers_registered: Vec<String>,
    pub skills_skipped: Vec<String>,
    pub themes_skipped: Vec<String>,
}

/// Routes plugin contributions to the appropriate registries.
///
/// Caller passes `&mut AgentApi` (boot-time mutable handle) and a
/// LoadedPlugin. The registrar walks `manifest.contributes` and routes
/// accordingly.
pub struct PluginRegistrar;

impl PluginRegistrar {
    pub fn register(
        api: &mut AgentApi,
        loaded: &LoadedPlugin,
    ) -> Result<PluginRegistrationSummary, RegistrationError> {
        let mut summary = PluginRegistrationSummary {
            plugin_id: loaded.manifest.id.clone(),
            ..Default::default()
        };
        let contrib = &loaded.manifest.contributes;

        // Tools — register as descriptors with a placeholder builder.
        // Task 3 swaps the placeholder for a real McpToolProxy delegate.
        for tool_name in &contrib.tools {
            let plugin_id = loaded.manifest.id.clone();
            let tool_name_owned = tool_name.clone();
            let prefixed_name = format!("{}:{}", plugin_id, tool_name_owned);
            let panic_tool_name = tool_name_owned.clone();
            api.register_tool(ToolDescriptor {
                name: prefixed_name.clone(),
                description: format!(
                    "Tool {} contributed by plugin {} (proxy wiring in P3-4.3)",
                    tool_name_owned, plugin_id
                ),
                parameters_schema: serde_json::json!({}),
                builder: Arc::new(move |_ctx| {
                    // Placeholder — Task 3 replaces with McpToolProxy.
                    panic!(
                        "Plugin tool {} not yet wired to a backing proxy (P3-4.3)",
                        panic_tool_name
                    )
                }),
            });
            summary.tools_registered.push(tool_name.clone());
        }

        // Commands — placeholder accounting only.
        for cmd_name in &contrib.commands {
            summary.commands_registered.push(cmd_name.clone());
        }

        // mcp_servers — handled in Task 3.
        for server_id in &contrib.mcp_servers {
            summary.mcp_servers_registered.push(server_id.clone());
        }

        // Skills + themes — record only.
        summary.skills_skipped = contrib.skills.clone();
        summary.themes_skipped = contrib.themes.clone();

        Ok(summary)
    }
}
