//! Manifest → AgentApi registration routing.
//!
//! Reads `LoadedPlugin` (manifest + paths) and registers its `PluginContribution`
//! fields into the appropriate handles:
//! - tools → AgentApi.register_tool with ToolDescriptors whose builder closure
//!   constructs an `McpToolProxy` at session-build time (Task 3).
//! - commands → registered as real Commands routing to the plugin's MCP call_tool.
//! - mcp_servers → recorded; future PRs wire full McpManager integration.
//! - skills, themes → recorded; no registration (future PRs).

use std::sync::Arc;

use crate::agent::api::command::{Command, CommandHandlerFn};
use crate::agent::api::tool::ToolDescriptor;
use crate::agent::api::AgentApi;
use crate::mcp::{ContentBlock, McpServerConfig, SharedMcpManager, TransportType};
use crate::plugins::discovery::LoadedPlugin;
use crate::plugins::{PluginPreflightReport, PluginPreflightVerdict};

#[derive(Debug, thiserror::Error)]
pub enum RegistrationError {}

/// Summary of what was registered for a plugin.
#[derive(Debug, Clone, Default)]
pub struct PluginRegistrationSummary {
    pub plugin_id: String,
    /// Filesystem directory that contains this plugin's files.  Retained on
    /// the summary so boot code can pass it to
    /// `SkillsRegistry::discover_plugin_skills` without re-deriving it from
    /// the LoadedPlugin (which is consumed earlier in the lifecycle).
    pub plugin_dir: std::path::PathBuf,
    pub tools_registered: Vec<String>,
    pub commands_registered: Vec<String>,
    pub mcp_servers_registered: Vec<String>,
    pub preflight: Option<PluginPreflightReport>,
    pub skills_skipped: Vec<String>,
    pub themes_skipped: Vec<String>,
    /// MCP server configs built from this plugin's manifest (permission-gated).
    /// Callers (e.g. AppState::new via PluginLifecycleReport) add these to
    /// McpManager at boot time.
    pub mcp_configs: Vec<McpServerConfig>,
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
        mcp_manager: &SharedMcpManager,
    ) -> Result<PluginRegistrationSummary, RegistrationError> {
        let mut summary = PluginRegistrationSummary {
            plugin_id: loaded.manifest.id.clone(),
            plugin_dir: loaded.plugin_dir.clone(),
            ..Default::default()
        };
        let contrib = &loaded.manifest.contributes;
        let preflight = PluginPreflightReport::for_loaded_plugin(loaded);
        summary.preflight = Some(preflight.clone());

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

        // Commands — register a Command whose handler routes to the plugin's MCP
        // server via call_tool (command name == an MCP tool/method the plugin
        // handles). Captures the mcp_manager Arc; by call time it is connected.
        for cmd_name in &contrib.commands {
            let mgr = mcp_manager.clone();
            let pid = loaded.manifest.id.clone();
            let cname = cmd_name.clone();
            let handler: CommandHandlerFn = Arc::new(move |args: serde_json::Value| {
                let mgr = mgr.clone();
                let pid = pid.clone();
                let cname = cname.clone();
                Box::pin(async move {
                    let res = mgr
                        .read()
                        .await
                        .call_tool(&pid, &cname, args)
                        .await
                        .map_err(|e| format!("plugin command '{cname}' failed: {e}"))?;
                    if res.is_error {
                        return Err(format!("plugin command '{cname}' returned an error"));
                    }
                    let text = res
                        .content
                        .iter()
                        .filter_map(|b| match b {
                            ContentBlock::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    Ok(serde_json::Value::String(text))
                })
            });
            api.register_command(Command {
                name: cmd_name.clone(),
                description: format!("Command from plugin {}", loaded.manifest.id),
                handler,
            });
            summary.commands_registered.push(cmd_name.clone());
        }

        // mcp_servers — build an MCP config for the existing subprocess/RPC
        // adapter when the manifest passes preflight.
        if !contrib.mcp_servers.is_empty() {
            if matches!(preflight.verdict, PluginPreflightVerdict::Fail) {
                if !loaded.manifest.permissions.run_subprocess {
                    summary.permission_skipped.push(loaded.manifest.id.clone());
                }
                tracing::warn!(
                    plugin_id = %loaded.manifest.id,
                    findings = ?preflight.findings,
                    "plugin preflight failed; skipping MCP config contribution"
                );
            } else if let Some(executable) = &loaded.manifest.runtime.executable {
                let exe_path = std::path::Path::new(executable);
                let command = if exe_path.is_absolute() {
                    executable.clone()
                } else {
                    loaded
                        .plugin_dir
                        .join(exe_path)
                        .to_string_lossy()
                        .to_string()
                };
                let tool_allowlist = if contrib.tools.is_empty() {
                    None
                } else {
                    Some(contrib.tools.clone())
                };
                summary.mcp_configs.push(McpServerConfig {
                    id: loaded.manifest.id.clone(),
                    name: loaded.manifest.display_name.clone(),
                    description: loaded.manifest.description.clone().unwrap_or_default(),
                    transport_type: TransportType::Stdio,
                    command,
                    args: loaded.manifest.runtime.args.clone(),
                    env: std::collections::HashMap::new(),
                    url: None,
                    enabled: true,
                    auto_approve: false,
                    tool_allowlist,
                    sandbox: Some(crate::plugins::sandbox::PluginSandboxPolicy {
                        plugin_dir: loaded.plugin_dir.clone(),
                        allow_network: loaded.manifest.permissions.network,
                        allow_fs_read: loaded.manifest.permissions.filesystem_read,
                        allow_fs_write: loaded.manifest.permissions.filesystem_write,
                    }),
                });
                summary
                    .mcp_servers_registered
                    .extend(contrib.mcp_servers.iter().cloned());
            }
        }

        // Skills + themes — record only.
        summary.skills_skipped = contrib.skills.clone();
        summary.themes_skipped = contrib.themes.clone();

        // Pi-3b — attribute contributions so build_session_registry can filter a
        // disabled plugin's tools. Tool names MUST be PREFIXED (match descriptors).
        let mut set = crate::agent::api::plugin::PluginRegistrationSet::default();
        for raw in &contrib.tools {
            set.tools.push(crate::mcp::prefixed_tool_name(&loaded.manifest.id, raw));
        }
        set.commands = summary.commands_registered.clone();
        api.register_plugin(
            crate::agent::api::plugin::PluginId::new(loaded.manifest.id.clone()),
            set,
        );

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
        let tmp = tempfile::tempdir().unwrap();
        let loaded = fixture_plugin(
            true,
            Some("server.js"),
            vec!["--flag".into()],
            vec!["hello".into()],
            vec!["greet".into()],
        );
        let mut api = AgentApi::new();
        let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::mcp::McpManager::new(tmp.path()),
        ));
        let summary = PluginRegistrar::register(&mut api, &loaded, &mgr).unwrap();
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
        let tmp = tempfile::tempdir().unwrap();
        let loaded = fixture_plugin(false, Some("server.js"), vec![], vec!["hello".into()], vec![]);
        let mut api = AgentApi::new();
        let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::mcp::McpManager::new(tmp.path()),
        ));
        let summary = PluginRegistrar::register(&mut api, &loaded, &mgr).unwrap();
        assert!(summary.mcp_configs.is_empty());
        assert_eq!(summary.permission_skipped, vec!["test-plug".to_string()]);
    }

    #[test]
    fn register_skips_mcp_when_no_executable() {
        let tmp = tempfile::tempdir().unwrap();
        let loaded = fixture_plugin(true, None, vec![], vec!["hello".into()], vec![]);
        let mut api = AgentApi::new();
        let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::mcp::McpManager::new(tmp.path()),
        ));
        let summary = PluginRegistrar::register(&mut api, &loaded, &mgr).unwrap();
        assert!(summary.mcp_configs.is_empty());
        // No permission_skipped entry — executable is just missing, not a permission issue.
        assert!(summary.permission_skipped.is_empty());
    }

    /// Pi-3b — sandbox policy is built from manifest permissions and plugin_dir.
    #[test]
    fn register_builds_sandbox_policy_from_manifest_permissions() {
        use crate::plugin_manifest::schema::{
            PluginAuthor, PluginContribution, PluginManifest, PluginPermissions,
            PluginRuntimeRequirement,
        };
        use crate::plugins::discovery::LoadedPlugin;

        let plugin_dir = PathBuf::from("/tmp/plug");
        let manifest = PluginManifest {
            id: "net-plug".into(),
            version: "0.1.0".into(),
            display_name: "Net Plug".into(),
            description: Some("Network-capable plugin".into()),
            author: PluginAuthor {
                name: "tester".into(),
                email: None,
                url: None,
            },
            runtime: PluginRuntimeRequirement {
                min_uclaw_version: "0.1.0".into(),
                kind: None,
                executable: Some("server.js".to_string()),
                args: vec![],
                working_dir: None,
            },
            permissions: PluginPermissions {
                run_subprocess: true,
                network: true,
                filesystem_read: false,
                filesystem_write: false,
                ..Default::default()
            },
            contributes: PluginContribution {
                mcp_servers: vec!["hello".into()],
                tools: vec!["greet".into()],
                ..Default::default()
            },
        };
        let loaded = LoadedPlugin {
            manifest_path: plugin_dir.join("plugin.toml"),
            plugin_dir: plugin_dir.clone(),
            manifest,
        };

        let mut api = AgentApi::new();
        let tmp_mgr = tempfile::tempdir().unwrap();
        let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::mcp::McpManager::new(tmp_mgr.path()),
        ));
        let summary = PluginRegistrar::register(&mut api, &loaded, &mgr).unwrap();
        assert_eq!(summary.mcp_configs.len(), 1);

        let cfg = &summary.mcp_configs[0];
        let policy = cfg
            .sandbox
            .as_ref()
            .expect("registration must set sandbox: Some(...)");
        assert_eq!(policy.plugin_dir, plugin_dir, "plugin_dir must match loaded.plugin_dir");
        assert!(policy.allow_network, "allow_network must reflect permissions.network=true");
        assert!(!policy.allow_fs_read, "allow_fs_read must reflect permissions.filesystem_read=false");
        assert!(!policy.allow_fs_write, "allow_fs_write must reflect permissions.filesystem_write=false");
    }

    /// Task 1 Step 5 — plugin command is registered AND gated in plugin_index.
    ///
    /// Confirms that:
    ///   1. `api.command("greet")` returns Some after registration.
    ///   2. The plugin_index entry for "test-plug" has "greet" in its commands vec
    ///      (gating pathway intact).
    #[test]
    fn register_plugin_command_is_wired_and_gated() {
        use crate::plugin_manifest::schema::{
            PluginAuthor, PluginContribution, PluginManifest, PluginPermissions,
            PluginRuntimeRequirement,
        };
        use crate::plugins::discovery::LoadedPlugin;

        let plugin_dir = PathBuf::from("/tmp/plug");
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
                executable: Some("server.js".to_string()),
                args: vec![],
                working_dir: None,
            },
            permissions: PluginPermissions {
                run_subprocess: true,
                ..Default::default()
            },
            contributes: PluginContribution {
                commands: vec!["greet".into()],
                ..Default::default()
            },
        };
        let loaded = LoadedPlugin {
            manifest_path: plugin_dir.join("plugin.toml"),
            plugin_dir,
            manifest,
        };

        let mut api = AgentApi::new();
        let tmp = tempfile::tempdir().unwrap();
        let mgr = std::sync::Arc::new(tokio::sync::RwLock::new(
            crate::mcp::McpManager::new(tmp.path()),
        ));
        let summary = PluginRegistrar::register(&mut api, &loaded, &mgr).unwrap();

        // 1. Command is registered in AgentApi.
        assert!(
            api.command("greet").is_some(),
            "api.command(\"greet\") should be Some after registration"
        );

        // 2. Summary records the command.
        assert_eq!(summary.commands_registered, vec!["greet".to_string()]);

        // 3. Plugin index gating entry has "greet" in its commands vec.
        let pid = crate::agent::api::plugin::PluginId::new("test-plug");
        let set = api
            .plugin_index
            .get(&pid)
            .expect("plugin_index should have an entry for test-plug");
        assert!(
            set.commands.contains(&"greet".to_string()),
            "plugin_index entry for test-plug should contain command 'greet'"
        );
    }
}
