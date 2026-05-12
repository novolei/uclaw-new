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

/// Categorical label for a tool failure, exposed to the LLM as a
/// bracketed tag in the error message (e.g. `[NotFound] ...`).
///
/// Picking the right kind helps the LLM reason about retry vs.
/// alternative-approach. e.g. NotFound rarely benefits from retry but
/// suggests trying a different URL; Timeout often does benefit from a
/// retry; PermissionDenied means stop and ask the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolErrorKind {
    /// Input doesn't match schema / missing required field / malformed value.
    InvalidInput,
    /// Resource doesn't exist (HTTP 404, FS ENOENT, DB row missing).
    ResourceNotFound,
    /// Authorization or sandboxing rejection (HTTP 403, FS EACCES, SSRF).
    PermissionDenied,
    /// Operation took too long.
    Timeout,
    /// Network-level failure (DNS, connection refused, TLS error).
    NetworkError,
    /// Server-side error (HTTP 5xx, downstream service unhealthy).
    UpstreamError,
    /// HTTP 429 / API rate limit.
    RateLimited,
    /// Body exceeded buffer cap, file too large, etc.
    PayloadTooLarge,
    /// Body couldn't be parsed as expected format (JSON parse, malformed HTML).
    ParseError,
    /// Service / resource temporarily unavailable (DB locked, service starting).
    Unavailable,
    /// Catch-all when no other variant fits.
    Other,
}

impl ToolErrorKind {
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::InvalidInput => "InvalidInput",
            Self::ResourceNotFound => "NotFound",
            Self::PermissionDenied => "PermissionDenied",
            Self::Timeout => "Timeout",
            Self::NetworkError => "NetworkError",
            Self::UpstreamError => "UpstreamError",
            Self::RateLimited => "RateLimited",
            Self::PayloadTooLarge => "PayloadTooLarge",
            Self::ParseError => "ParseError",
            Self::Unavailable => "Unavailable",
            Self::Other => "Other",
        }
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
    /// Structured error with a category and a user/LLM-friendly message.
    /// Display formats as `[Kind] message` so the LLM can pattern-match
    /// on the bracketed tag.
    #[error("[{}] {message}", .kind.as_str())]
    Kinded {
        kind: ToolErrorKind,
        message: String,
        source_context: Option<String>,
    },
}

impl ToolError {
    pub fn kinded(kind: ToolErrorKind, message: impl Into<String>) -> Self {
        Self::Kinded { kind, message: message.into(), source_context: None }
    }

    pub fn kinded_with_source(
        kind: ToolErrorKind,
        message: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        Self::Kinded { kind, message: message.into(), source_context: Some(source.into()) }
    }
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

#[cfg(test)]
mod kinded_error_tests {
    use super::*;

    #[test]
    fn kinded_error_displays_with_bracketed_kind() {
        let err = ToolError::kinded(
            ToolErrorKind::ResourceNotFound,
            "Page returned 404",
        );
        assert_eq!(format!("{}", err), "[NotFound] Page returned 404");
    }

    #[test]
    fn kinded_error_serializes_through_existing_serde_path() {
        let err = ToolError::kinded(
            ToolErrorKind::PermissionDenied,
            "URL blocked",
        );
        let json = serde_json::to_string(&err).unwrap();
        // Existing serde impl uses Display; both new + legacy variants share
        // the same serialization path.
        assert!(json.contains("PermissionDenied"), "got json: {}", json);
        assert!(json.contains("URL blocked"), "got json: {}", json);
    }

    #[test]
    fn kinded_with_source_keeps_source_field() {
        let err = ToolError::kinded_with_source(
            ToolErrorKind::ParseError,
            "Could not parse JSON",
            "expected ',' at line 5",
        );
        match err {
            ToolError::Kinded { kind, message, source_context } => {
                assert_eq!(kind, ToolErrorKind::ParseError);
                assert_eq!(message, "Could not parse JSON");
                assert_eq!(source_context.as_deref(), Some("expected ',' at line 5"));
            }
            _ => panic!("expected Kinded variant"),
        }
    }
}
