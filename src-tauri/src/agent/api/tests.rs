//! Unit tests for AgentApi.

use super::*;
use crate::agent::tools::tool::{Tool, ToolOutput, ToolError};
use async_trait::async_trait;

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

/// Minimal dummy Tool impl for the tests in this module.
struct DummyTool {
    name: String,
}

#[async_trait]
impl Tool for DummyTool {
    fn name(&self) -> &str {
        &self.name
    }
    fn description(&self) -> &str {
        "dummy tool"
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({})
    }
    async fn execute(
        &self,
        _params: serde_json::Value,
    ) -> Result<ToolOutput, ToolError> {
        Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
    }
}

#[test]
fn register_tool_stores_by_name() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    assert_eq!(api.tools.len(), 1);
    assert!(api.tools.contains_key("echo"));
}

#[test]
fn tool_query_returns_registered_tool() {
    let mut api = AgentApi::new();
    api.register_tool(std::sync::Arc::new(DummyTool { name: "echo".into() }));
    let got = api.tool("echo");
    assert!(got.is_some());
    assert_eq!(got.unwrap().name(), "echo");
    assert!(api.tool("nonexistent").is_none());
}
