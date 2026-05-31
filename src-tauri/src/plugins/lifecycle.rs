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
}
