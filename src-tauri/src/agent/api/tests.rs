//! Unit tests for AgentApi.

use super::*;

#[test]
fn new_agent_api_has_empty_registries() {
    let api = AgentApi::new();
    assert_eq!(api.tools.len(), 0);
    assert_eq!(api.providers.len(), 0);
    assert_eq!(api.commands.len(), 0);
    assert_eq!(api.renderers.len(), 0);
    assert_eq!(api.hooks.len(), 0);
    assert_eq!(api.plugin_index.len(), 0);
}
