//! Manifest → AgentApi registration routing.
//!
//! Reads `LoadedPlugin` (manifest + paths) and registers its `PluginContribution`
//! fields into the appropriate handles:
//! - tools → AgentApi.register_tool with ToolDescriptors whose builder closure
//!   constructs an `McpToolProxy` at session-build time (Task 3).
//! - commands → recorded to summary (real wiring deferred to follow-up).
//! - mcp_servers → recorded; future PRs wire full McpManager integration.
//! - skills, themes → recorded; no registration (future PRs).

use std::sync::Arc;

use crate::agent::api::AgentApi;
use crate::agent::api::tool::ToolDescriptor;
use crate::plugins::discovery::LoadedPlugin;

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {}

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

        // Tools — register ToolDescriptors whose builder closure constructs a
        // real McpToolProxy at session-build time (P3-4.3).  The plugin's id is
        // used as the MCP server id so the call is routed through the right
        // transport; the tool name is un-prefixed (McpToolProxy::for_plugin
        // applies the `mcp__{server}__{tool}` prefix internally).
        for tool_name in &contrib.tools {
            let plugin_id = loaded.manifest.id.clone();
            let tool_name_owned = tool_name.clone();
            let prefixed_name = crate::mcp::prefixed_tool_name(&plugin_id, &tool_name_owned);
            api.register_tool(ToolDescriptor {
                name: prefixed_name.clone(),
                description: format!(
                    "Tool {} contributed by plugin {}",
                    tool_name_owned, plugin_id
                ),
                parameters_schema: serde_json::json!({}),
                builder: Arc::new(move |ctx| {
                    Box::new(crate::mcp::McpToolProxy::for_plugin(
                        plugin_id.clone(),
                        tool_name_owned.clone(),
                        ctx.app_state.mcp_manager.clone(),
                    ))
                }),
            });
            summary.tools_registered.push(tool_name.clone());
        }

        // Commands — placeholder accounting only.
        for cmd_name in &contrib.commands {
            summary.commands_registered.push(cmd_name.clone());
        }

        // mcp_servers — recorded; full McpManager wiring deferred to later tasks.
        for server_id in &contrib.mcp_servers {
            summary.mcp_servers_registered.push(server_id.clone());
        }

        // Skills + themes — record only.
        summary.skills_skipped = contrib.skills.clone();
        summary.themes_skipped = contrib.themes.clone();

        Ok(summary)
    }
}
