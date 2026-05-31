use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio_util::sync::CancellationToken;
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

/// Coarse side-effect declaration for tool scheduling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolEffects {
    bits: u8,
}

impl ToolEffects {
    const READ: u8 = 1 << 0;
    const WRITE: u8 = 1 << 1;
    const APPEND: u8 = 1 << 2;
    const NETWORK: u8 = 1 << 3;
    const PROCESS: u8 = 1 << 4;
    const BARRIER: u8 = Self::WRITE | Self::APPEND | Self::PROCESS;

    #[must_use]
    pub const fn read() -> Self {
        Self { bits: Self::READ }
    }

    #[must_use]
    pub const fn write() -> Self {
        Self { bits: Self::WRITE }
    }

    #[must_use]
    pub const fn append() -> Self {
        Self { bits: Self::APPEND }
    }

    #[must_use]
    pub const fn network() -> Self {
        Self {
            bits: Self::NETWORK,
        }
    }

    #[must_use]
    pub const fn process() -> Self {
        Self {
            bits: Self::PROCESS,
        }
    }

    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }

    #[must_use]
    pub const fn reads(self) -> bool {
        self.bits & Self::READ != 0
    }

    #[must_use]
    pub const fn writes(self) -> bool {
        self.bits & Self::WRITE != 0
    }

    #[must_use]
    pub const fn appends(self) -> bool {
        self.bits & Self::APPEND != 0
    }

    #[must_use]
    pub const fn networks(self) -> bool {
        self.bits & Self::NETWORK != 0
    }

    #[must_use]
    pub const fn processes(self) -> bool {
        self.bits & Self::PROCESS != 0
    }

    #[must_use]
    pub fn labels(self) -> Vec<&'static str> {
        let mut labels = Vec::with_capacity(5);
        if self.reads() {
            labels.push("read");
        }
        if self.writes() {
            labels.push("write");
        }
        if self.appends() {
            labels.push("append");
        }
        if self.networks() {
            labels.push("network");
        }
        if self.processes() {
            labels.push("process");
        }
        labels
    }

    #[must_use]
    pub const fn parallel_safe(self) -> bool {
        self.bits != 0 && self.bits & Self::BARRIER == 0
    }

    #[must_use]
    pub const fn compatible_with(self, other: Self) -> bool {
        self.parallel_safe() && other.parallel_safe()
    }
}

/// 工具并发性声明(Pi `executionMode` 子集)。ToolDispatcher 据此把工具分到
/// 串行内联 / 并行 JoinSet 两道。与上面的 `ToolExecutionMode`(调用点模式)是
/// 不同的轴,故另立枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolConcurrency {
    /// 串行(默认):有副作用 / 需审批 / 网络 IO 的工具。
    Sequential,
    /// 并行安全:纯只读工具,可进 JoinSet 批次。
    Parallel,
}

/// Structured context for a tool invocation.
///
/// PR-2 keeps this as adapter metadata. The canonical execution behavior still
/// flows through `Tool::execute(params)` until individual tools opt into richer
/// context-aware behavior in later PRs.
///
/// `PartialEq` is implemented manually to skip the `cancel` field (CancellationToken
/// does not implement PartialEq); two contexts are equal when all other fields match.
#[derive(Debug, Clone)]
pub struct ToolExecutionContext {
    pub session_id: String,
    pub task_id: Option<String>,
    pub message_id: Option<String>,
    pub tool_call_id: String,
    pub workspace_root: Option<PathBuf>,
    pub execution_mode: ToolExecutionMode,
    pub safety_mode: Option<SafetyMode>,
    pub capability_profile_id: Option<String>,
    /// Item 1.A — per-invocation cancellation token. When fired, flight-point
    /// aware tools (BashTool) abort their blocking await and return a cancelled
    /// result. `None` for tests and headless contexts (no-token path unchanged).
    pub cancel: Option<CancellationToken>,
}

impl PartialEq for ToolExecutionContext {
    fn eq(&self, other: &Self) -> bool {
        self.session_id == other.session_id
            && self.task_id == other.task_id
            && self.message_id == other.message_id
            && self.tool_call_id == other.tool_call_id
            && self.workspace_root == other.workspace_root
            && self.execution_mode == other.execution_mode
            && self.safety_mode == other.safety_mode
            && self.capability_profile_id == other.capability_profile_id
        // `cancel` intentionally excluded: CancellationToken does not implement PartialEq
    }
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
            cancel: None,
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

    /// 是否支持流式输出。默认 false —— 只有增量产出的工具(BashTool)override。
    /// dispatcher 用它决定是否为本次调用搭建合并节流 drain 任务。
    fn supports_streaming(&self) -> bool { false }

    /// 流式变体。默认忽略 sink、委托 `execute()`。
    /// 只有 `supports_streaming() == true` 的工具才 override 它。
    async fn execute_streaming(
        &self,
        params: serde_json::Value,
        _sink: crate::agent::tools::stream::ToolStreamSink,
    ) -> Result<ToolOutput, ToolError> {
        self.execute(params).await
    }

    /// Item 1.A — cancellation-aware streaming variant. Default delegates to
    /// `execute_streaming` (no-token path, backward compatible). Tools with
    /// blocking await points (BashTool) override this to race the await against
    /// `cancel.cancelled()` and abort early on a fired token.
    async fn execute_streaming_with_cancel(
        &self,
        params: serde_json::Value,
        sink: crate::agent::tools::stream::ToolStreamSink,
        _cancel: Option<CancellationToken>,
    ) -> Result<ToolOutput, ToolError> {
        self.execute_streaming(params, sink).await
    }

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

    /// Declare coarse side effects used by the dispatcher scheduler.
    ///
    /// Defaults to local write effects so undeclared tools serialize fail-closed.
    fn effects(&self) -> ToolEffects { ToolEffects::write() }

    /// Compatibility adapter for older callers. New scheduling code should use
    /// `effects()` and derive batch compatibility from `ToolEffects`.
    fn concurrency(&self) -> ToolConcurrency {
        if self.effects().parallel_safe() {
            ToolConcurrency::Parallel
        } else {
            ToolConcurrency::Sequential
        }
    }
}

pub async fn execute_tool_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    _ctx: &ToolExecutionContext,
) -> Result<ToolOutput, ToolError> {
    tool.execute(params).await
}

/// 流式版本的 `execute_tool_with_context`。dispatcher 串行路径在工具
/// `supports_streaming()` 时调用它,把 sink 和 cancel token 传进工具。
pub async fn execute_streaming_with_context(
    tool: &dyn Tool,
    params: serde_json::Value,
    ctx: &ToolExecutionContext,
    sink: crate::agent::tools::stream::ToolStreamSink,
) -> Result<ToolOutput, ToolError> {
    tool.execute_streaming_with_cancel(params, sink, ctx.cancel.clone()).await
}

/// Regularize an arbitrary tool name to the `^[a-zA-Z0-9_-]+$` shape that both
/// OpenAI and Anthropic require for `function.name`. Invalid chars → '_';
/// empty → "unnamed_tool"; truncated to 64 (Anthropic's upper bound).
pub fn sanitize_tool_name(raw: &str) -> String {
    let mut s: String = raw
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
        .collect();
    if s.is_empty() { s = "unnamed_tool".to_string(); }
    if s.len() > 64 { s.truncate(64); }
    s
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
        self.insert_tool(Box::new(tool));
    }

    /// Register a pre-boxed `Tool` instance. Used by `AgentApi.build_session_registry`
    /// where descriptor builders return `Box<dyn Tool>` (the concrete type is
    /// erased at registration time).
    pub fn register_boxed(&mut self, tool: Box<dyn Tool>) {
        self.insert_tool(tool);
    }

    /// Insert a tool under a provider-safe, collision-free key. Key =
    /// sanitize_tool_name(tool.name()); on collision (exact dup OR post-sanitize
    /// clash) a numeric suffix is appended. This key is what list_definitions
    /// exposes to providers and what dispatch resolves against.
    fn insert_tool(&mut self, tool: Box<dyn Tool>) {
        let mut key = sanitize_tool_name(tool.name());
        if self.tools.contains_key(&key) {
            let base = key.clone();
            let mut n = 2;
            while self.tools.contains_key(&key) {
                key = format!("{base}_{n}");
                n += 1;
            }
            tracing::warn!(original = %tool.name(), resolved = %key,
                "tool name collision after sanitize; suffix-deduped");
        }
        self.tools.insert(key, tool);
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

#[cfg(test)]
mod stream_default_tests {
    use super::*;
    use crate::agent::tools::stream::ToolStreamSink;

    struct DummyTool;
    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str { "dummy" }
        fn description(&self) -> &str { "" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
        }
    }

    #[tokio::test]
    async fn default_execute_streaming_delegates_to_execute() {
        let tool = DummyTool;
        assert!(!tool.supports_streaming());
        let out = tool.execute_streaming(serde_json::json!({}), ToolStreamSink::noop()).await.unwrap();
        assert_eq!(out.result["ok"], serde_json::json!(true));
    }
}

#[cfg(test)]
mod concurrency_tests {
    use super::*;
    use crate::agent::tools::builtin::file::ReadFileTool;
    use crate::agent::tools::builtin::get_file_skeleton::GetFileSkeletonTool;
    use crate::agent::tools::builtin::shell::BashTool;
    use std::path::PathBuf;

    #[test]
    fn read_only_tools_are_parallel() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(ReadFileTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
        assert_eq!(GetFileSkeletonTool::new(ws.clone()).concurrency(), ToolConcurrency::Parallel);
        assert_eq!(ReadFileTool::new(ws.clone()).effects(), ToolEffects::read());
        assert_eq!(GetFileSkeletonTool::new(ws).effects(), ToolEffects::read());
    }

    #[test]
    fn other_tools_default_sequential() {
        let ws = PathBuf::from("/tmp");
        assert_eq!(BashTool::new(ws).concurrency(), ToolConcurrency::Sequential);
        assert_eq!(BashTool::new(PathBuf::from("/tmp")).effects(), ToolEffects::process());
    }
}

#[cfg(test)]
mod effects_tests {
    use super::*;

    #[test]
    fn effect_labels_are_stable_and_machine_readable() {
        let effects = ToolEffects::read().union(ToolEffects::network());
        assert_eq!(effects.labels(), vec!["read", "network"]);
    }

    #[test]
    fn read_and_network_effects_are_parallel_safe() {
        assert!(ToolEffects::read().parallel_safe());
        assert!(ToolEffects::network().parallel_safe());
        assert!(ToolEffects::read().compatible_with(ToolEffects::network()));
    }

    #[test]
    fn write_append_and_process_effects_are_barriers() {
        assert!(!ToolEffects::write().parallel_safe());
        assert!(!ToolEffects::append().parallel_safe());
        assert!(!ToolEffects::process().parallel_safe());
        assert!(!ToolEffects::read().compatible_with(ToolEffects::write()));
    }
}
