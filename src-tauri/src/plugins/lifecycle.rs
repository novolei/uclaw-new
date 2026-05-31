use std::path::PathBuf;

use crate::agent::api::AgentApi;
use crate::plugins::{DiscoveryError, PluginDiscovery, PluginRegistrar, PluginRegistrationSummary};

#[derive(Debug, Clone, Default)]
pub struct PluginLifecycleReport {
    pub plugins_root: PathBuf,
    pub loaded: Vec<PluginRegistrationSummary>,
    pub discovery_errors: Vec<String>,
    pub registration_errors: Vec<String>,
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
                            Ok(summary) => report.loaded.push(summary),
                            Err(error) => report.registration_errors.push(error.to_string()),
                        },
                        Err(error) => report.discovery_errors.push(error.to_string()),
                    }
                }
            }
            Err(error) => report.discovery_errors.push(error.to_string()),
        }

        report
    }
}

impl PluginLifecycleReport {
    /// All MCP server configs from successfully-registered plugins, for the
    /// caller (AppState::new) to add to the McpManager.
    pub fn plugin_mcp_configs(&self) -> Vec<crate::mcp::McpServerConfig> {
        self.loaded.iter().flat_map(|s| s.mcp_configs.clone()).collect()
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

#[cfg(test)]
mod tests {
    use super::*;

    // Minimal valid plugin.toml that:
    //   - has id = "test-plug" (must match the containing directory name)
    //   - sets run_subprocess = true and provides an executable
    //   - declares an mcp_servers entry so the registrar builds an McpServerConfig
    // All optional serde fields use their defaults.
    const SAMPLE_MANIFEST_TOML: &str = r#"
id = "test-plug"
version = "0.1.0"
display_name = "Test Plug"

[author]
name = "tester"

[runtime]
min_uclaw_version = "0.1.0"
executable = "server.js"

[permissions]
run_subprocess = true

[contributes]
mcp_servers = ["hello"]
"#;

    #[test]
    fn missing_plugins_dir_is_empty_success_report() {
        let tmp = tempfile::tempdir().unwrap();
        let owner = PluginLifecycleOwner::new(tmp.path().join("missing"));
        let mut api = AgentApi::new();

        let report = owner.connect_and_register(&mut api);

        assert!(report.loaded.is_empty());
        assert!(report.discovery_errors.is_empty());
        assert!(report.registration_errors.is_empty());
    }

    #[test]
    fn connect_and_register_aggregates_plugin_mcp_configs() {
        let dir = tempfile::tempdir().unwrap();
        let pdir = dir.path().join("test-plug");
        std::fs::create_dir_all(&pdir).unwrap();
        std::fs::write(pdir.join("plugin.toml"), SAMPLE_MANIFEST_TOML).unwrap();

        let mut api = AgentApi::new();
        let report = PluginLifecycleOwner::new(dir.path()).connect_and_register(&mut api);

        assert_eq!(
            report.plugin_mcp_configs().len(),
            1,
            "discovery_errors={:?} registration_errors={:?}",
            report.discovery_errors,
            report.registration_errors
        );
    }
}
