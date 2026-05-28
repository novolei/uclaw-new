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

#[test]
fn register_provider_stores_by_id() {
    let mut api = AgentApi::new();
    let provider = std::sync::Arc::new(make_test_provider_service().unwrap());
    api.register_provider("openai".to_string(), provider);
    assert_eq!(api.providers.len(), 1);
    assert!(api.providers.contains_key("openai"));
}

#[test]
fn provider_query_returns_registered() {
    let mut api = AgentApi::new();
    let provider = std::sync::Arc::new(make_test_provider_service().unwrap());
    api.register_provider("openai".to_string(), provider);
    assert!(api.provider("openai").is_some());
    assert!(api.provider("nonexistent").is_none());
}

/// Helper to construct a ProviderService for tests.
/// Uses a temporary directory so file I/O succeeds without side effects.
fn make_test_provider_service() -> Result<crate::providers::service::ProviderService, crate::error::Error> {
    let temp_dir = tempfile::tempdir().map_err(|e| {
        crate::error::Error::Internal(format!("Failed to create temp dir: {e}"))
    })?;
    crate::providers::service::ProviderService::new(temp_dir.path())
}

#[test]
fn register_command_stores_by_name() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    let cmd = crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({"out": "hello"})) }.boxed()
        }),
    };
    api.register_command(cmd);
    assert_eq!(api.commands.len(), 1);
}

#[test]
fn command_query_returns_registered() {
    use futures::FutureExt;
    let mut api = AgentApi::new();
    api.register_command(crate::agent::api::command::Command {
        name: "hello".to_string(),
        description: "Say hello".to_string(),
        handler: std::sync::Arc::new(|_args| {
            async move { Ok(serde_json::json!({})) }.boxed()
        }),
    });
    assert!(api.command("hello").is_some());
    assert!(api.command("missing").is_none());
}
