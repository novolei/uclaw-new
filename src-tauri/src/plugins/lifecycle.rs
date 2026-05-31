use std::path::PathBuf;

use crate::agent::api::plugin::{PluginId, PluginRegistrationSet};
use crate::agent::api::AgentApi;
use crate::plugins::{DiscoveryError, PluginDiscovery, PluginRegistrar, PluginRegistrationSummary};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginLifecycleHealth {
    Healthy,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginLifecycleState {
    Loaded,
    DiscoveryError,
    RegistrationError,
    Unregistered,
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginLifecycleStatus {
    pub plugin_id: String,
    pub state: PluginLifecycleState,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct PluginLifecycleReport {
    pub plugins_root: PathBuf,
    pub loaded: Vec<PluginRegistrationSummary>,
    pub discovery_errors: Vec<String>,
    pub registration_errors: Vec<String>,
    pub statuses: Vec<PluginLifecycleStatus>,
}

pub struct PluginLifecycleOwner {
    plugins_root: PathBuf,
}

impl PluginLifecycleOwner {
    pub fn new(plugins_root: impl Into<PathBuf>) -> Self {
        Self {
            plugins_root: plugins_root.into(),
        }
    }

    pub fn connect_and_register(&self, api: &mut AgentApi) -> PluginLifecycleReport {
        let mut report = PluginLifecycleReport {
            plugins_root: self.plugins_root.clone(),
            ..Default::default()
        };
        let discovery = PluginDiscovery::new(&self.plugins_root);

        match discovery.discover() {
            Ok(results) => {
                for result in results {
                    match result {
                        Ok(loaded) => match PluginRegistrar::register(api, &loaded) {
                            Ok(summary) => {
                                api.register_plugin(
                                    PluginId::new(loaded.manifest.id.clone()),
                                    registration_set_from_summary(&loaded.manifest.id, &summary),
                                );
                                report.statuses.push(PluginLifecycleStatus {
                                    plugin_id: loaded.manifest.id.clone(),
                                    state: PluginLifecycleState::Loaded,
                                    detail: Some(format!(
                                        "{} tools, {} mcp servers",
                                        summary.tools_registered.len(),
                                        summary.mcp_servers_registered.len()
                                    )),
                                });
                                report.loaded.push(summary);
                            }
                            Err(error) => {
                                let message = error.to_string();
                                report.statuses.push(PluginLifecycleStatus {
                                    plugin_id: loaded.manifest.id.clone(),
                                    state: PluginLifecycleState::RegistrationError,
                                    detail: Some(message.clone()),
                                });
                                report.registration_errors.push(message);
                            }
                        },
                        Err(error) => {
                            let message = error.to_string();
                            report.statuses.push(PluginLifecycleStatus {
                                plugin_id: "unknown".to_string(),
                                state: PluginLifecycleState::DiscoveryError,
                                detail: Some(message.clone()),
                            });
                            report.discovery_errors.push(message);
                        }
                    }
                }
            }
            Err(error) => report.discovery_errors.push(error.to_string()),
        }

        report
    }

    pub fn health(&self, report: &PluginLifecycleReport) -> PluginLifecycleHealth {
        if report.discovery_errors.is_empty() && report.registration_errors.is_empty() {
            PluginLifecycleHealth::Healthy
        } else {
            PluginLifecycleHealth::Degraded
        }
    }

    pub fn unregister(&self, api: &mut AgentApi, plugin_id: &str) -> PluginLifecycleStatus {
        match api.unregister_plugin(&PluginId::new(plugin_id.to_string())) {
            Some(set) => PluginLifecycleStatus {
                plugin_id: plugin_id.to_string(),
                state: PluginLifecycleState::Unregistered,
                detail: Some(format!(
                    "{} tools, {} commands, {} renderers",
                    set.tools.len(),
                    set.commands.len(),
                    set.renderers.len()
                )),
            },
            None => PluginLifecycleStatus {
                plugin_id: plugin_id.to_string(),
                state: PluginLifecycleState::Missing,
                detail: Some("plugin was not registered".to_string()),
            },
        }
    }
}

impl From<DiscoveryError> for PluginLifecycleReport {
    fn from(error: DiscoveryError) -> Self {
        Self {
            discovery_errors: vec![error.to_string()],
            ..Default::default()
        }
    }
}

fn registration_set_from_summary(
    plugin_id: &str,
    summary: &PluginRegistrationSummary,
) -> PluginRegistrationSet {
    PluginRegistrationSet {
        tools: summary
            .tools_registered
            .iter()
            .map(|tool| crate::mcp::prefixed_tool_name(plugin_id, tool))
            .collect(),
        commands: summary.commands_registered.clone(),
        ..PluginRegistrationSet::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_plugins_dir_is_empty_success_report() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = PluginLifecycleOwner::new(tmp.path().join("missing"));
        let mut api = AgentApi::new();

        let report = owner.connect_and_register(&mut api);

        assert!(report.loaded.is_empty());
        assert!(report.discovery_errors.is_empty());
        assert!(report.registration_errors.is_empty());
        assert_eq!(owner.health(&report), PluginLifecycleHealth::Healthy);
    }

    #[test]
    fn echo_plugin_registers_health_and_unregisters_cleanly() {
        let tmp = tempfile::tempdir().unwrap();
        let plugins_root = tmp.path().join("plugins");
        let echo_dir = plugins_root.join("echo_plugin");
        std::fs::create_dir_all(&echo_dir).unwrap();
        std::fs::copy(
            std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("examples/echo_plugin/plugin.toml"),
            echo_dir.join("plugin.toml"),
        )
        .unwrap();
        let owner = PluginLifecycleOwner::new(&plugins_root);
        let mut api = AgentApi::new();

        let report = owner.connect_and_register(&mut api);

        assert_eq!(owner.health(&report), PluginLifecycleHealth::Healthy);
        assert_eq!(report.loaded.len(), 1);
        assert_eq!(report.statuses[0].state, PluginLifecycleState::Loaded);
        assert!(api.tool("mcp__echo_plugin__echo").is_some());

        let status = owner.unregister(&mut api, "echo_plugin");

        assert_eq!(status.state, PluginLifecycleState::Unregistered);
        assert!(api.tool("mcp__echo_plugin__echo").is_none());
    }

    #[test]
    fn unregister_missing_plugin_reports_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = PluginLifecycleOwner::new(tmp.path().join("plugins"));
        let mut api = AgentApi::new();

        let status = owner.unregister(&mut api, "missing");

        assert_eq!(status.state, PluginLifecycleState::Missing);
    }
}
