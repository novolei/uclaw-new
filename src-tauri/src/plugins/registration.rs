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
    /// MCP server configs built from this plugin's manifest (permission-gated).
    /// Callers (e.g. AppState::new via PluginLifecycleReport) add these to
    /// McpManager at boot time.
    pub mcp_configs: Vec<crate::mcp::McpServerConfig>,
    /// Plugin ids whose mcp_servers were skipped because run_subprocess
    /// permission was not granted in the manifest.
    pub permission_skipped: Vec<String>,
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

        // mcp_servers — build McpServerConfig, permission-gated.
        if !contrib.mcp_servers.is_empty() {
            let perms = &loaded.manifest.permissions;
            match (&loaded.manifest.runtime.executable, perms.run_subprocess) {
                (Some(exe), true) => {
                    let exe_path = std::path::Path::new(exe);
                    let command = if exe_path.is_absolute() {
                        exe.clone()
                    } else {
                        loaded.plugin_dir.join(exe_path).to_string_lossy().to_string()
                    };
                    let tool_allowlist = if contrib.tools.is_empty() {
                        None
                    } else {
                        Some(contrib.tools.clone())
                    };
                    summary.mcp_configs.push(crate::mcp::McpServerConfig {
                        id: loaded.manifest.id.clone(),
                        name: loaded.manifest.display_name.clone(),
                        description: loaded.manifest.description.clone().unwrap_or_default(),
                        transport_type: Default::default(),
                        command,
                        args: loaded.manifest.runtime.args.clone(),
                        env: std::collections::HashMap::new(),
                        url: None,
                        enabled: true,
                        auto_approve: false,
                        tool_allowlist,
                    });
                    summary.mcp_servers_registered.push(loaded.manifest.id.clone());
                }
                (Some(_), false) => {
                    tracing::warn!(
                        plugin = %loaded.manifest.id,
                        "plugin declares mcp_servers but lacks run_subprocess permission; skipping spawn"
                    );
                    summary.permission_skipped.push(loaded.manifest.id.clone());
                }
                (None, _) => {
                    tracing::warn!(
                        plugin = %loaded.manifest.id,
                        "plugin declares mcp_servers but has no runtime.executable; skipping"
                    );
                }
            }
        }

        // Skills + themes — record only.
        summary.skills_skipped = contrib.skills.clone();
        summary.themes_skipped = contrib.themes.clone();

        Ok(summary)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::plugin_manifest::schema::{
        PluginAuthor, PluginContribution, PluginManifest, PluginPermissions,
        PluginRuntimeRequirement,
    };
    use crate::plugins::discovery::LoadedPlugin;

    /// Build a `LoadedPlugin` for unit tests.
    ///
    /// - `id` is always `"test-plug"` and `plugin_dir` is `/tmp/plug`.
    /// - `run_subprocess` controls `permissions.run_subprocess`.
    /// - `executable` goes into `runtime.executable`.
    /// - `args` goes into `runtime.args`.
    /// - `mcp_servers` populates `contributes.mcp_servers`.
    /// - `tools` populates `contributes.tools`.
    fn fixture_plugin(
        run_subprocess: bool,
        executable: Option<&str>,
        args: Vec<String>,
        mcp_servers: Vec<String>,
        tools: Vec<String>,
    ) -> LoadedPlugin {
        let manifest = PluginManifest {
            id: "test-plug".into(),
            version: "0.1.0".into(),
            display_name: "Test Plug".into(),
            description: Some("A test plugin".into()),
            author: PluginAuthor {
                name: "tester".into(),
                email: None,
                url: None,
            },
            runtime: PluginRuntimeRequirement {
                min_uclaw_version: "0.1.0".into(),
                kind: None,
                executable: executable.map(str::to_string),
                args,
                working_dir: None,
            },
            permissions: PluginPermissions {
                run_subprocess,
                ..Default::default()
            },
            contributes: PluginContribution {
                mcp_servers,
                tools,
                ..Default::default()
            },
        };
        let plugin_dir = PathBuf::from("/tmp/plug");
        LoadedPlugin {
            manifest_path: plugin_dir.join("plugin.toml"),
            plugin_dir,
            manifest,
        }
    }

    #[test]
    fn register_builds_mcp_config_when_run_subprocess_granted() {
        let loaded = fixture_plugin(
            true,
            Some("server.js"),
            vec!["--flag".into()],
            vec!["hello".into()],
            vec!["greet".into()],
        );
        let mut api = AgentApi::new();
        let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
        assert_eq!(summary.mcp_configs.len(), 1);
        let cfg = &summary.mcp_configs[0];
        assert_eq!(cfg.id, "test-plug");
        assert!(
            cfg.command.ends_with("server.js") && std::path::Path::new(&cfg.command).is_absolute(),
            "command should be an absolute path ending in server.js, got: {}",
            cfg.command
        );
        assert_eq!(cfg.args, vec!["--flag".to_string()]);
        assert_eq!(cfg.tool_allowlist, Some(vec!["greet".to_string()]));
        assert!(cfg.enabled);
        assert!(summary.permission_skipped.is_empty());
    }

    #[test]
    fn register_skips_mcp_when_run_subprocess_denied() {
        let loaded = fixture_plugin(false, Some("server.js"), vec![], vec!["hello".into()], vec![]);
        let mut api = AgentApi::new();
        let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
        assert!(summary.mcp_configs.is_empty());
        assert_eq!(summary.permission_skipped, vec!["test-plug".to_string()]);
    }

    #[test]
    fn register_skips_mcp_when_no_executable() {
        let loaded = fixture_plugin(true, None, vec![], vec!["hello".into()], vec![]);
        let mut api = AgentApi::new();
        let summary = PluginRegistrar::register(&mut api, &loaded).unwrap();
        assert!(summary.mcp_configs.is_empty());
        // No permission_skipped entry — executable is just missing, not a permission issue.
        assert!(summary.permission_skipped.is_empty());
    }
}
