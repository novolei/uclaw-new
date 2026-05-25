use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use crate::agent::types::ToolDefinition;
use crate::safety::SafetyMode;

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
    /// A required precondition for the operation does not hold (e.g. the file
    /// was modified externally since last read). The LLM should re-establish
    /// the precondition — typically by re-reading — before retrying. Used by
    /// EditTool's stale-file gate (spec §8.4 — hard reject, not a warning).
    PreconditionFailed,
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
            Self::PreconditionFailed => "PreconditionFailed",
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

/// Execution mode for a tool invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolExecutionMode {
    /// Tool call produced by an agent loop turn.
    AgentTurn,
    /// Direct/manual invocation outside a normal agent turn.
    Direct,
}

/// Structured context for a tool invocation.
///
/// PR-2 keeps this as adapter metadata. The canonical execution behavior still
/// flows through `Tool::execute(params)` until individual tools opt into richer
/// context-aware behavior in later PRs.
#[derive(Debug, Clone, PartialEq)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub task_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: String,
    pub workspace_root: Option<PathBuf>,
    pub execution_mode: ToolExecutionMode,
    pub safety_mode: Option<SafetyMode>,
    pub capability_profile_id: Option<String>,
}

impl ToolExecutionContext {
    pub fn agent_turn(
        session_id: impl Into<String>,
        tool_call_id: impl Into<String>,
        workspace_root: Option<PathBuf>,
        safety_mode: Option<SafetyMode>,
    ) -> Self {
        Self {
            session_id: session_id.into(),
            task_id: None,
            message_id: None,
            tool_call_id: tool_call_id.into(),
            workspace_root,
            execution_mode: ToolExecutionMode::AgentTurn,
            safety_mode,
            capability_profile_id: None,
        }
    }

    pub fn for_subcall(&self, tool_call_id: impl Into<String>) -> Self {
        let mut next = self.clone();
        next.tool_call_id = tool_call_id.into();
        next
    }

    pub fn resolve_candidate_path(&self, path: impl AsRef<Path>) -> PathBuf {
        let path = path.as_ref();
        if path.is_absolute() {
            path.to_path_buf()
        } else if let Some(root) = &self.workspace_root {
            root.join(path)
        } else {
            path.to_path_buf()
        }
    }
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

    /// If this tool **writes** to a file, return the path it will write.
    /// Drives the auto-preview popup: the dispatcher includes this string
    /// in the `chat:stream-tool-activity` event payload so the frontend
    /// can open the preview panel without having to maintain a hardcoded
    /// `Set<toolName>` of "write-ish" tools (the failure mode Proma's
    /// auto-preview suffers from — renaming `Write` → `write_v2` silently
    /// disables the trigger).
    ///
    /// Default returns `None` — non-mutating tools (search/grep/web/...)
    /// inherit this. WriteFile, Edit, and other mutating tools override
    /// to extract their path from `args`.
    fn preview_target_path(&self, _args: &serde_json::Value) -> Option<String> {
        None
    }
}

pub async fn execute_tool_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    _ctx: &ToolExecutionContext,
) -> Result<ToolOutput, ToolError> {
    tool.execute(params).await
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
        let mut defs: Vec<ToolDefinition> = self.tools.iter().map(|(name, tool)| {
            ToolDefinition {
                name: name.clone(),
                description: tool.description().to_string(),
                parameters: tool.parameters_schema(),
            }
        }).collect();
        // Deterministic order ensures Anthropic prompt cache breakpoint lands
        // on the same tool every iteration — random HashMap order breaks caching.
        defs.sort_by(|a, b| a.name.cmp(&b.name));
        defs
    }

    pub fn len(&self) -> usize { self.tools.len() }
    pub fn is_empty(&self) -> bool { self.tools.is_empty() }
}

impl Default for ToolRegistry {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
#[path = "tool_tests.rs"]
mod tests;
