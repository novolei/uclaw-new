use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use crate::agent::types::ToolDefinition;

/// Tool execution result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolOutput {
    pub result: serde_json::Value,
    pub cost: Option<f64>,
    pub duration_ms: u64,
}

impl ToolOutput {
    pub fn new(result: serde_json::Value, duration_ms: u64) -> Self {
        Self { result, cost: None, duration_ms }
    }
    pub fn success(text: &str, duration_ms: u64) -> Self {
        Self { result: serde_json::json!({"ok": true, "content": text}), cost: None, duration_ms }
    }
    pub fn error(text: &str, duration_ms: u64) -> Self {
        Self { result: serde_json::json!({"ok": false, "error": text}), cost: None, duration_ms }
    }
}

/// Tool error
#[derive(Debug, thiserror::Error)]
pub enum ToolError {
    #[error("Tool execution failed: {0}")]
    Execution(String),
    #[error("Invalid parameters: {0}")]
    InvalidParams(String),
    #[error("Tool not found: {0}")]
    NotFound(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

impl serde::Serialize for ToolError {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

/// Approval requirement for tool execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ApprovalRequirement {
    Never,
    UnlessAutoApproved,
    Always,
}

impl Default for ApprovalRequirement {
    fn default() -> Self { Self::Never }
}

/// Tool trait — implement for each tool
#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn parameters_schema(&self) -> serde_json::Value;

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError>;

    fn estimated_cost(&self, _params: &serde_json::Value) -> Option<f64> { None }
    fn estimated_duration(&self, _params: &serde_json::Value) -> Option<Duration> { None }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement { ApprovalRequirement::default() }

    /// Return the argument keys that name filesystem paths. The dispatcher
    /// uses these to consult the SafetyManager's PathPolicy before invoking
    /// `execute`. Default impl returns empty — tools without path args (web,
    /// plan, exit_plan_mode, etc.) inherit this.
    fn path_args<'a>(&self, _arguments: &'a serde_json::Value) -> Vec<&'a str> {
        Vec::new()
    }
}

/// Tool registry
pub struct ToolRegistry {
    tools: std::collections::HashMap<String, Box<dyn Tool>>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self { tools: std::collections::HashMap::new() }
    }

    pub fn register<T: Tool + 'static>(&mut self, tool: T) {
        self.tools.insert(tool.name().to_string(), Box::new(tool));
    }

    pub fn get(&self, name: &str) -> Option<&dyn Tool> {
        self.tools.get(name).map(|t| t.as_ref())
    }

    pub fn list_definitions(&self) -> Vec<ToolDefinition> {
        self.tools.iter().map(|(name, tool)| {
            ToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            }
        }).collect()
    }

    pub fn len(&self) -> usize { self.tools.len() }
    pub fn is_empty(&self) -> bool { self.tools.is_empty() }
}

impl Default for ToolRegistry {
    fn default() -> Self { Self::new() }
}
