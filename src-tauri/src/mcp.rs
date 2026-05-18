//! MCP (Model Context Protocol) client integration.
//!
//! Manages connections to MCP servers for extended tool capabilities.
//! Supports stdio (subprocess) and HTTP transports with JSON-RPC 2.0.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, RwLock, oneshot};
use tokio::task::JoinHandle;

// ─── JSON-RPC 2.0 Protocol Types ───────────────────────────────────────

pub const PROTOCOL_VERSION: &str = "2024-11-05";

// ─── PR-3 — auto-reconnect / health loop tunables ──────────────────────

/// How often the per-server health loop pings the MCP server. 30s is
/// chosen as the smallest interval that's clearly "background" rather
/// than "spammy"; users with multiple servers won't see noisy logs.
pub const HEALTH_PING_INTERVAL_SECS: u64 = 30;

/// First reconnect attempt fires this long after a ping failure.
/// Subsequent attempts double the delay up to `RECONNECT_MAX_DELAY_SECS`.
pub const RECONNECT_INITIAL_DELAY_SECS: u64 = 10;

/// Hard ceiling on the per-attempt reconnect wait. 5 minutes matches
/// the spirit of "don't hammer a dead server" without making recovery
/// feel hopeless if it comes back later.
pub const RECONNECT_MAX_DELAY_SECS: u64 = 300;

// ─── PR-4 — server notification routing ────────────────────────────────

/// Event surfaced by the stdio reader task whenever an MCP server
/// pushes a notification (JSON-RPC frame with `method` set, no `id`).
/// The manager-side consumer dispatches by method: today we only
/// special-case `notifications/tools/list_changed` (auto-refresh +
/// frontend event). Other methods log at debug for forward-compat —
/// future spec additions surface here without code changes.
#[derive(Debug, Clone)]
pub struct McpNotificationEvent {
    pub server_id: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// Method string for the canonical "tools list changed" notification
/// defined by the MCP spec. uClaw declares
/// `capabilities.tools.listChanged = true` in `initialize` (line 56)
/// so well-behaved servers will fire this whenever they add/remove a
/// tool while connected.
pub const NOTIFY_TOOLS_LIST_CHANGED: &str = "notifications/tools/list_changed";

// ─── PR-5 — env redaction + audit log ──────────────────────────────────

/// Replace any substring matching one of `env`'s values with
/// `[REDACTED]`. Used on every error message that goes to the UI / audit
/// log so a subprocess spawn failure can't leak `GITHUB_TOKEN=ghp_xxx`
/// in a screenshot or shared log.
///
/// We only redact values longer than 4 chars (avoids false positives on
/// boolean-ish env values like `1` or `true` that often appear in
/// general error strings). Empty values are skipped for the same
/// reason — `str::replace("", _)` infinite-loops on some allocators
/// and isn't useful anyway.
pub fn redact_env_values(s: &str, env: &HashMap<String, String>) -> String {
    let mut out = s.to_string();
    for v in env.values() {
        if v.len() >= 5 {
            out = out.replace(v, "[REDACTED]");
        }
    }
    out
}

/// Kinds of events written to `mcp_audit`. Stored as the literal
/// string in the `event_kind` column; new variants are append-only so
/// historical rows stay parseable.
#[derive(Debug, Clone, Copy)]
pub enum McpAuditKind {
    ConnectAttempt,
    ConnectSucceeded,
    ConnectFailed,
    HealthFailed,
    Reconnected,
    Disconnect,
    Removed,
    ToolsChanged,
}

impl McpAuditKind {
    pub fn as_str(self) -> &'static str {
        match self {
            McpAuditKind::ConnectAttempt => "connect_attempt",
            McpAuditKind::ConnectSucceeded => "connect_succeeded",
            McpAuditKind::ConnectFailed => "connect_failed",
            McpAuditKind::HealthFailed => "health_failed",
            McpAuditKind::Reconnected => "reconnected",
            McpAuditKind::Disconnect => "disconnect",
            McpAuditKind::Removed => "removed",
            McpAuditKind::ToolsChanged => "tools_changed",
        }
    }
}

/// Single audit-log row as exposed to the frontend / list IPC.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpAuditEntry {
    pub id: String,
    pub server_id: String,
    pub event_kind: String,
    pub message_redacted: String,
    pub created_at: i64,
}

/// Append one row to `mcp_audit`. Best-effort: a DB lock failure logs
/// + swallows. Caller passes a pre-redacted message (use
/// `redact_env_values` first).
pub fn append_audit_row(
    db: &Arc<std::sync::Mutex<rusqlite::Connection>>,
    server_id: &str,
    kind: McpAuditKind,
    message: &str,
) {
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().timestamp_millis();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("[mcp_audit] DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO mcp_audit (id, server_id, event_kind, message_redacted, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![id, server_id, kind.as_str(), message, now],
    ) {
        tracing::warn!("[mcp_audit] INSERT failed: {}", e);
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<u64>,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl JsonRpcRequest {
    pub fn new(id: u64, method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: Some(id),
            method: method.into(),
            params,
        }
    }

    pub fn notification(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id: None,
            method: method.into(),
            params,
        }
    }

    pub fn initialize(id: u64) -> Self {
        Self::new(
            id,
            "initialize",
            Some(serde_json::json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": {
                    "roots": { "listChanged": true }
                },
                "clientInfo": {
                    "name": "uclaw",
                    "version": env!("CARGO_PKG_VERSION")
                }
            })),
        )
    }

    pub fn initialized_notification() -> Self {
        Self::notification("notifications/initialized", None)
    }

    pub fn list_tools(id: u64) -> Self {
        Self::new(id, "tools/list", None)
    }

    pub fn call_tool(id: u64, name: &str, arguments: serde_json::Value) -> Self {
        Self::new(
            id,
            "tools/call",
            Some(serde_json::json!({
                "name": name,
                "arguments": arguments
            })),
        )
    }

    pub fn list_resources(id: u64) -> Self {
        Self::new(id, "resources/list", None)
    }

    pub fn read_resource(id: u64, uri: &str) -> Self {
        Self::new(
            id,
            "resources/read",
            Some(serde_json::json!({ "uri": uri })),
        )
    }

    pub fn ping(id: u64) -> Self {
        Self::new(id, "ping", None)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl std::fmt::Display for JsonRpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

// ─── MCP Protocol Result Types ──────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    #[serde(default)]
    pub protocol_version: Option<String>,
    #[serde(default)]
    pub capabilities: ServerCapabilities,
    #[serde(default)]
    pub server_info: Option<ServerInfo>,
    #[serde(default)]
    pub instructions: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ServerCapabilities {
    #[serde(default)]
    pub tools: Option<serde_json::Value>,
    #[serde(default)]
    pub resources: Option<serde_json::Value>,
    #[serde(default)]
    pub prompts: Option<serde_json::Value>,
    #[serde(default)]
    pub logging: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListToolsResult {
    pub tools: Vec<McpRemoteTool>,
    #[serde(default)]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpRemoteTool {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default = "default_input_schema", alias = "input_schema")]
    pub input_schema: serde_json::Value,
}

fn default_input_schema() -> serde_json::Value {
    serde_json::json!({"type": "object", "properties": {}})
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallToolResult {
    pub content: Vec<ContentBlock>,
    #[serde(default, rename = "isError")]
    pub is_error: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    Text { text: String },
    Image { data: String, mime_type: String },
    Resource { resource: serde_json::Value },
}

// ─── MCP Server Status & Config ─────────────────────────────────────────

/// MCP server connection status
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum McpServerStatus {
    Disconnected,
    Connecting,
    Connected,
    Error,
}

/// Transport type for MCP server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TransportType {
    Stdio,
    Http,
}

impl Default for TransportType {
    fn default() -> Self {
        Self::Stdio
    }
}

/// MCP server configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerConfig {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(default)]
    pub transport_type: TransportType,
    /// Command to execute (stdio transport)
    #[serde(default)]
    pub command: String,
    /// Command arguments (stdio transport)
    #[serde(default)]
    pub args: Vec<String>,
    /// Environment variables (stdio transport)
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// URL for HTTP transport
    #[serde(default)]
    pub url: Option<String>,
    pub enabled: bool,
    pub auto_approve: bool,
}

/// MCP tool definition from a server
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDef {
    pub server_id: String,
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

// ─── Transport Trait ────────────────────────────────────────────────────

#[async_trait]
pub(crate) trait McpTransport: Send + Sync {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError>;
    async fn shutdown(&self) -> Result<(), McpError>;
}

#[derive(Debug, thiserror::Error)]
pub enum McpError {
    #[error("Transport error: {0}")]
    Transport(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Timeout: {0}")]
    Timeout(String),
    #[error("Server error: {0}")]
    Server(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

// ─── Stdio Transport ────────────────────────────────────────────────────

struct StdioTransport {
    server_name: String,
    stdin: Arc<Mutex<tokio::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>>,
    reader_handle: Mutex<Option<JoinHandle<()>>>,
    stderr_handle: Mutex<Option<JoinHandle<()>>>,
    child: Arc<Mutex<tokio::process::Child>>,
}

impl StdioTransport {
    async fn spawn(
        name: impl Into<String>,
        command: &str,
        args: &[String],
        env: &HashMap<String, String>,
        // PR-4 — when `Some`, the stdout reader publishes JSON-RPC
        // notifications (frames with `method` but no `id`) onto this
        // sender keyed by the supplied `server_id`. `None` matches the
        // pre-PR-4 behaviour: notifications are logged + discarded.
        server_id: impl Into<String>,
        notification_tx: Option<tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>>,
    ) -> Result<Self, McpError> {
        let server_name = name.into();
        let server_id = server_id.into();

        let mut cmd = tokio::process::Command::new(command);
        cmd.args(args)
            .envs(env)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            McpError::Transport(format!(
                "[{}] Failed to spawn MCP server '{}': {}",
                server_name, command, e
            ))
        })?;

        let stdin = child.stdin.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stdin", server_name))
        })?;

        let stdout = child.stdout.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stdout", server_name))
        })?;

        let stderr = child.stderr.take().ok_or_else(|| {
            McpError::Transport(format!("[{}] Failed to capture stderr", server_name))
        })?;

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<JsonRpcResponse>>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Spawn stdout reader
        let reader_pending = pending.clone();
        let reader_name = server_name.clone();
        let reader_server_id = server_id.clone();
        let reader_notify = notification_tx.clone();
        let reader_handle = tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let value = match serde_json::from_str::<serde_json::Value>(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::debug!("[{}] Failed to parse JSON-RPC: {}", reader_name, e);
                        continue;
                    }
                };

                // Check if it's a response (has result or error, no method)
                if let Some(method) = value.get("method").and_then(|m| m.as_str()) {
                    // PR-4 — route via the notification channel when wired.
                    // Notifications have `method` and no `id`; the same
                    // frame shape technically encodes server-side
                    // requests (with `id`) but uClaw doesn't expose any
                    // `sampling/createMessage`-style handler today, so
                    // we treat both as fire-and-forget events.
                    tracing::debug!(
                        "[{}] Received server notification: {}",
                        reader_name,
                        method
                    );
                    if let Some(tx) = reader_notify.as_ref() {
                        let event = McpNotificationEvent {
                            server_id: reader_server_id.clone(),
                            method: method.to_string(),
                            params: value
                                .get("params")
                                .cloned()
                                .unwrap_or(serde_json::Value::Null),
                        };
                        // send() returns Err only when every receiver
                        // is dropped — log + continue (the consumer
                        // task died for some reason, but the reader
                        // should keep up its end of the protocol).
                        if let Err(e) = tx.send(event) {
                            tracing::warn!(
                                "[{}] Failed to forward notification: {}",
                                reader_name,
                                e
                            );
                        }
                    }
                    continue;
                }

                let response: JsonRpcResponse = match serde_json::from_value(value) {
                    Ok(r) => r,
                    Err(e) => {
                        tracing::debug!("[{}] Failed to parse response: {}", reader_name, e);
                        continue;
                    }
                };

                if let Some(id) = response.id {
                    let mut map = reader_pending.lock().await;
                    if let Some(tx) = map.remove(&id) {
                        let _ = tx.send(response);
                    } else {
                        tracing::debug!("[{}] Response for unknown id {}", reader_name, id);
                    }
                }
            }
            tracing::debug!("[{}] JSON-RPC reader finished", reader_name);
        });

        // Spawn stderr reader
        let stderr_name = server_name.clone();
        let stderr_handle = tokio::spawn(async move {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::debug!("[{}] stderr: {}", stderr_name, line);
            }
        });

        Ok(Self {
            server_name,
            stdin: Arc::new(Mutex::new(stdin)),
            pending,
            reader_handle: Mutex::new(Some(reader_handle)),
            stderr_handle: Mutex::new(Some(stderr_handle)),
            child: Arc::new(Mutex::new(child)),
        })
    }
}

#[async_trait]
impl McpTransport for StdioTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let json = serde_json::to_string(request).map_err(|e| {
            McpError::Protocol(format!("Failed to serialize request: {}", e))
        })?;

        // For notifications (no id), just send and return empty response
        if request.id.is_none() {
            let mut writer = self.stdin.lock().await;
            writer.write_all(json.as_bytes()).await.map_err(|e| {
                McpError::Transport(format!("[{}] Write failed: {}", self.server_name, e))
            })?;
            writer.write_all(b"\n").await.map_err(|e| {
                McpError::Transport(format!("[{}] Write newline failed: {}", self.server_name, e))
            })?;
            writer.flush().await.map_err(|e| {
                McpError::Transport(format!("[{}] Flush failed: {}", self.server_name, e))
            })?;
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: None,
            });
        }

        let id = request.id.unwrap();
        let (tx, rx) = oneshot::channel();

        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        // Write request
        {
            let mut writer = self.stdin.lock().await;
            if let Err(e) = async {
                writer.write_all(json.as_bytes()).await?;
                writer.write_all(b"\n").await?;
                writer.flush().await?;
                Ok::<_, std::io::Error>(())
            }.await {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                return Err(McpError::Transport(format!(
                    "[{}] Write failed: {}", self.server_name, e
                )));
            }
        }

        // Wait for response with timeout
        match tokio::time::timeout(Duration::from_secs(60), rx).await {
            Ok(Ok(response)) => Ok(response),
            Ok(Err(_)) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(McpError::Transport(format!(
                    "[{}] Server closed connection before responding",
                    self.server_name
                )))
            }
            Err(_) => {
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(McpError::Timeout(format!(
                    "[{}] Timeout waiting for response to request {}",
                    self.server_name, id
                )))
            }
        }
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        {
            let mut child = self.child.lock().await;
            let _ = child.kill().await;
        }
        if let Some(handle) = self.reader_handle.lock().await.take() {
            handle.abort();
        }
        if let Some(handle) = self.stderr_handle.lock().await.take() {
            handle.abort();
        }
        {
            let mut pending = self.pending.lock().await;
            pending.clear();
        }
        tracing::debug!("[{}] Stdio transport shut down", self.server_name);
        Ok(())
    }
}

// ─── HTTP Transport ─────────────────────────────────────────────────────

struct HttpTransport {
    server_name: String,
    url: String,
    client: reqwest::Client,
}

impl HttpTransport {
    fn new(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            server_name: name.into(),
            url: url.into(),
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
        }
    }
}

#[async_trait]
impl McpTransport for HttpTransport {
    async fn send(&self, request: &JsonRpcRequest) -> Result<JsonRpcResponse, McpError> {
        let resp = self
            .client
            .post(&self.url)
            .header("Content-Type", "application/json")
            .json(request)
            .send()
            .await
            .map_err(|e| {
                McpError::Transport(format!(
                    "[{}] HTTP request failed: {}",
                    self.server_name, e
                ))
            })?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(McpError::Transport(format!(
                "[{}] HTTP {} — {}",
                self.server_name, status, body
            )));
        }

        // For notifications, the server may return 202 with no body
        if request.id.is_none() {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: None,
                result: None,
                error: None,
            });
        }

        let response: JsonRpcResponse = resp.json().await.map_err(|e| {
            McpError::Protocol(format!(
                "[{}] Failed to parse HTTP response: {}",
                self.server_name, e
            ))
        })?;

        Ok(response)
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        tracing::debug!("[{}] HTTP transport shut down", self.server_name);
        Ok(())
    }
}

// ─── MCP Client (per-server connection) ─────────────────────────────────

struct McpConnection {
    transport: Arc<dyn McpTransport>,
    next_id: AtomicU64,
    initialized: bool,
    tools: Vec<McpRemoteTool>,
    server_info: Option<ServerInfo>,
}

impl McpConnection {
    fn next_id(&self) -> u64 {
        self.next_id.fetch_add(1, Ordering::SeqCst)
    }

    async fn initialize(&mut self) -> Result<InitializeResult, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::initialize(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!(
                "Initialize failed: {}", error
            )));
        }

        let init_result: InitializeResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in initialize response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid initialize result: {}", e))
                })
            })?;

        self.server_info = init_result.server_info.clone();

        // Send initialized notification
        let notification = JsonRpcRequest::initialized_notification();
        if let Err(e) = self.transport.send(&notification).await {
            tracing::debug!("Failed to send initialized notification: {}", e);
        }

        self.initialized = true;
        Ok(init_result)
    }

    async fn discover_tools(&mut self) -> Result<Vec<McpRemoteTool>, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::list_tools(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("tools/list failed: {}", error)));
        }

        let result: ListToolsResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in tools/list response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid tools/list result: {}", e))
                })
            })?;

        self.tools = result.tools.clone();
        Ok(result.tools)
    }

    async fn call_tool(
        &self,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::call_tool(id, tool_name, arguments);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("tools/call failed: {}", error)));
        }

        let result: CallToolResult = response
            .result
            .ok_or_else(|| McpError::Protocol("No result in tools/call response".into()))
            .and_then(|r| {
                serde_json::from_value(r).map_err(|e| {
                    McpError::Protocol(format!("Invalid tools/call result: {}", e))
                })
            })?;

        Ok(result)
    }

    async fn list_resources(&self) -> Result<serde_json::Value, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::list_resources(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("resources/list failed: {}", error)));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    async fn read_resource(&self, uri: &str) -> Result<serde_json::Value, McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::read_resource(id, uri);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("resources/read failed: {}", error)));
        }

        Ok(response.result.unwrap_or(serde_json::Value::Null))
    }

    async fn ping(&self) -> Result<(), McpError> {
        let id = self.next_id();
        let request = JsonRpcRequest::ping(id);
        let response = self.transport.send(&request).await?;

        if let Some(error) = &response.error {
            return Err(McpError::Server(format!("ping failed: {}", error)));
        }

        Ok(())
    }

    async fn shutdown(&self) -> Result<(), McpError> {
        self.transport.shutdown().await
    }
}

// ─── MCP Server Runtime State ───────────────────────────────────────────

/// MCP server runtime state
pub struct McpServerState {
    pub config: McpServerConfig,
    pub status: McpServerStatus,
    pub tools: Vec<McpToolDef>,
    pub error: Option<String>,
    connection: Option<McpConnection>,
}

// ─── MCP Tool Proxy (exposes MCP tools as Tool trait) ───────────────────

/// Prefix applied to every agent-facing MCP tool name. Lets the rest of
/// uClaw — SafetyManager, telemetry, the prompt manifest — distinguish
/// MCP-sourced tool calls from builtins at a glance via the tool name
/// alone (no need to consult a separate registry).
pub const MCP_TOOL_PREFIX: &str = "mcp__";

/// Build the agent-facing tool name for an MCP-proxied tool. The
/// format `mcp__{server_id}__{tool_name}` matches the convention used
/// by Cline / Roo / Claude Desktop and is what users will recognize.
pub fn prefixed_tool_name(server_id: &str, tool_name: &str) -> String {
    format!("{}{}__{}", MCP_TOOL_PREFIX, server_id, tool_name)
}

/// Inverse of `prefixed_tool_name`. Returns `Some((server_id, tool_name))`
/// when `name` matches the expected `mcp__SERVER__TOOL` shape, `None`
/// otherwise (so callers can fast-path non-MCP tool names without
/// expensive checks). The split is on the first `__` AFTER the prefix
/// so server ids containing single underscores are preserved.
pub fn parse_mcp_tool_name(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix(MCP_TOOL_PREFIX)?;
    let idx = rest.find("__")?;
    let (server_id, tail) = rest.split_at(idx);
    let tool_name = &tail[2..]; // strip the "__" separator
    if server_id.is_empty() || tool_name.is_empty() {
        return None;
    }
    Some((server_id, tool_name))
}

#[derive(Clone)]
pub struct McpToolProxy {
    /// Source server id — used to route the JSON-RPC call back through
    /// the right transport, and (with `tool_name`) to identify which
    /// MCP server a proxied call originated from when auditing.
    server_id: String,
    /// Raw MCP tool name as the server reported it (e.g. "create_issue").
    /// Used in the JSON-RPC `tools/call` request — must NOT include the
    /// uClaw-side `mcp__{server_id}__` prefix or the server won't know
    /// what to invoke.
    tool_name: String,
    /// Agent-facing tool name = `mcp__{server_id}__{tool_name}`. This is
    /// what `name()` returns to `ToolRegistry`, what shows up in the LLM
    /// tool manifest, and what `SafetyManager` keys on. The prefix
    /// guarantees uniqueness across servers (two MCP servers can ship
    /// identically-named tools without colliding).
    prefixed_name: String,
    description: String,
    input_schema: serde_json::Value,
    manager: SharedMcpManager,
    /// Snapshotted from `McpServerConfig.auto_approve` at proxy
    /// construction time. Drives `requires_approval` so SafetyManager
    /// can grant `Never` (no approval prompt) for tools sourced from
    /// servers the user marked as trusted in the Integrations UI.
    /// Snapshot is OK because changing the flag triggers a manager
    /// refresh which rebuilds proxies for the next agent turn.
    auto_approve: bool,
}

#[async_trait]
impl crate::agent::tools::tool::Tool for McpToolProxy {
    fn name(&self) -> &str {
        &self.prefixed_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
    }

    /// Soft-honor the server's `auto_approve` flag. `Never` lets
    /// SafetyManager short-circuit straight to AutoApprove regardless
    /// of the active SafetyMode (see safety/mod.rs:221-224).
    /// `UnlessAutoApproved` keeps the normal Supervised-mode gating
    /// in place: the user can still allow specific tools via the
    /// auto-approved whitelist, but unknown calls require confirmation.
    fn requires_approval(
        &self,
        _params: &serde_json::Value,
    ) -> crate::agent::tools::tool::ApprovalRequirement {
        if self.auto_approve {
            crate::agent::tools::tool::ApprovalRequirement::Never
        } else {
            crate::agent::tools::tool::ApprovalRequirement::UnlessAutoApproved
        }
    }

    async fn execute(
        &self,
        params: serde_json::Value,
    ) -> Result<crate::agent::tools::tool::ToolOutput, crate::agent::tools::tool::ToolError> {
        let start = std::time::Instant::now();

        // Acquire read lock only to get the transport handle, then release immediately
        let (transport, req_id) = {
            let mgr = self.manager.read().await;
            mgr.get_transport(&self.server_id).map_err(|e| {
                crate::agent::tools::tool::ToolError::Execution(e.to_string())
            })?
        };
        // Lock is now released — execute the network call without holding it
        tracing::debug!("Calling MCP tool '{}' on server '{}' (lock-free)", self.tool_name, self.server_id);
        let request = JsonRpcRequest::call_tool(req_id, &self.tool_name, params);
        let response = transport.send(&request).await;
        let duration_ms = start.elapsed().as_millis() as u64;

        let result = match response {
            Ok(resp) => {
                if let Some(error) = &resp.error {
                    Err(McpError::Server(format!("tools/call failed: {}", error)))
                } else {
                    resp.result
                        .ok_or_else(|| McpError::Protocol("No result in tools/call response".into()))
                        .and_then(|r| {
                            serde_json::from_value::<CallToolResult>(r).map_err(|e| {
                                McpError::Protocol(format!("Invalid tools/call result: {}", e))
                            })
                        })
                }
            }
            Err(e) => Err(e),
        };

        match result {
            Ok(call_result) => {
                let text = call_result
                    .content
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("\n");

                if call_result.is_error {
                    Ok(crate::agent::tools::tool::ToolOutput::error(&text, duration_ms))
                } else {
                    Ok(crate::agent::tools::tool::ToolOutput::success(&text, duration_ms))
                }
            }
            Err(e) => Ok(crate::agent::tools::tool::ToolOutput::error(
                &e.to_string(),
                duration_ms,
            )),
        }
    }
}

// ─── MCP Manager ────────────────────────────────────────────────────────

/// MCP client manager
pub struct McpManager {
    servers: HashMap<String, McpServerState>,
    config_path: std::path::PathBuf,
    /// PR-3 — per-server health/reconnect task handles. Keyed by server
    /// id. Inserted by `start_health_loop`, aborted + removed by
    /// `stop_health_loop` (called on disconnect/remove). The handle is
    /// `pub(crate)` only — outside callers can't poke at the loops.
    health_tasks: HashMap<String, JoinHandle<()>>,
    /// PR-4 — shared sender pushed into every stdio transport at
    /// `connect_server` time. `None` until `set_notification_tx` is
    /// called (main.rs wires it once at boot). When None the reader
    /// tasks fall back to log-and-discard behaviour.
    notification_tx: Option<tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>>,
    /// PR-5 — main app DB handle for writing `mcp_audit` rows. `None`
    /// until `set_db_handle` is called at boot. When None, audit writes
    /// silently no-op so unit tests don't need DB setup.
    db: Option<Arc<std::sync::Mutex<rusqlite::Connection>>>,
}

impl McpManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let config_path = data_dir.join("mcp_servers.json");
        let mut manager = Self {
            servers: HashMap::new(),
            config_path,
            health_tasks: HashMap::new(),
            notification_tx: None,
            db: None,
        };
        manager.load_config();
        manager
    }

    /// PR-4 — install the channel sender that every stdio transport
    /// will forward notifications onto. Called once at app boot from
    /// `main.rs`; the matching receiver lives in a tokio task that
    /// dispatches by method (today only `tools/list_changed` is
    /// special-cased).
    pub fn set_notification_tx(
        &mut self,
        tx: tokio::sync::mpsc::UnboundedSender<McpNotificationEvent>,
    ) {
        self.notification_tx = Some(tx);
    }

    /// PR-5 — install the app DB handle so lifecycle events get
    /// persisted to the `mcp_audit` table. Called once at boot.
    pub fn set_db_handle(
        &mut self,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    ) {
        self.db = Some(db);
    }

    /// PR-5 — helper that pairs `redact_env_values` with `append_audit_row`.
    /// Looks up the server's env (if known) for redaction; if the
    /// server doesn't exist (e.g. the audit row is for `Removed`) the
    /// message goes in verbatim. No-op when `db` isn't installed.
    fn record_audit(&self, server_id: &str, kind: McpAuditKind, message: &str) {
        let redacted = self
            .servers
            .get(server_id)
            .map(|s| redact_env_values(message, &s.config.env))
            .unwrap_or_else(|| message.to_string());
        if let Some(db) = self.db.as_ref() {
            append_audit_row(db, server_id, kind, &redacted);
        }
    }

    // ── Config Persistence ──────────────────────────────────────────

    fn load_config(&mut self) {
        if let Ok(content) = std::fs::read_to_string(&self.config_path) {
            if let Ok(servers) = serde_json::from_str::<Vec<McpServerConfig>>(&content) {
                for config in servers {
                    self.servers.insert(
                        config.id.clone(),
                        McpServerState {
                            config,
                            status: McpServerStatus::Disconnected,
                            tools: Vec::new(),
                            error: None,
                            connection: None,
                        },
                    );
                }
            }
        }
    }

    fn save_config(&self) {
        let configs: Vec<&McpServerConfig> =
            self.servers.values().map(|s| &s.config).collect();
        if let Ok(json) = serde_json::to_string_pretty(&configs) {
            let _ = std::fs::write(&self.config_path, json);
        }
    }

    // ── Server CRUD ─────────────────────────────────────────────────

    pub fn add_server(&mut self, config: McpServerConfig) -> Result<(), String> {
        if self.servers.contains_key(&config.id) {
            return Err(format!("Server {} already exists", config.id));
        }
        self.servers.insert(
            config.id.clone(),
            McpServerState {
                config,
                status: McpServerStatus::Disconnected,
                tools: Vec::new(),
                error: None,
                connection: None,
            },
        );
        self.save_config();
        Ok(())
    }

    /// gbrain Sprint 2.1 — seed the bundled gbrain stdio MCP entry if
    /// no entry with id="gbrain" already exists. Called once at boot
    /// from main.rs's Stage 3.
    ///
    /// Idempotent + non-destructive:
    /// - If the entry exists (regardless of `enabled`), do nothing.
    ///   That way users who explicitly disable / remove gbrain don't
    ///   get it re-added on every restart.
    /// - The entry is auto_approve=true because it's the bundled
    ///   service we ship + sign — same trust level as the local
    ///   user's filesystem (which builtin tools already get).
    ///
    /// Inputs:
    /// - `bun_path`: absolute path to `bunembed/bun` (resource or dev)
    /// - `entry_path`: absolute path to gbrain's CLI entry (resource
    ///   or dev `src/cli.ts`). Spawned via `bun <entry> serve`.
    /// - `gbrain_home`: writable directory that becomes `$GBRAIN_HOME`.
    ///   gbrain reads its config from `$GBRAIN_HOME/.gbrain/config.json`
    ///   (created by `ensure_bundled_gbrain_initialized`) and stores
    ///   PGLite data under `$GBRAIN_HOME/.gbrain/brain.pglite/`.
    ///   Caller MUST have invoked `ensure_bundled_gbrain_initialized`
    ///   first — without an initialized brain, gbrain serve exits
    ///   immediately on every connect attempt.
    ///
    /// Returns `Ok(true)` if seeded, `Ok(false)` if entry already
    /// existed (no-op). Errors propagate from `add_server`.
    pub fn seed_bundled_gbrain(
        &mut self,
        bun_path: &std::path::Path,
        entry_path: &std::path::Path,
        gbrain_home: &std::path::Path,
    ) -> Result<bool, String> {
        if self.servers.contains_key("gbrain") {
            tracing::debug!(
                "seed_bundled_gbrain: 'gbrain' entry already in config (keeping user state)"
            );
            return Ok(false);
        }
        let mut env = HashMap::new();
        env.insert(
            "GBRAIN_HOME".to_string(),
            gbrain_home.to_string_lossy().to_string(),
        );
        let config = McpServerConfig {
            id: "gbrain".to_string(),
            name: "gbrain (bundled)".to_string(),
            description: "Local semantic-retrieval engine — wiki / entity-graph / dream-cycle. \
                         Bundled via Bun + gbrain source. PGLite brain at \
                         ~/.uclaw/gbrain/.gbrain/brain.pglite/."
                .to_string(),
            transport_type: TransportType::Stdio,
            command: bun_path.to_string_lossy().to_string(),
            // `bun <entry> serve` matches gbrain's stdio MCP CLI per
            // Sprint 2.0 Mac-side verification.
            args: vec![
                entry_path.to_string_lossy().to_string(),
                "serve".to_string(),
            ],
            env,
            url: None,
            enabled: true,
            auto_approve: true,
        };
        self.add_server(config)?;
        tracing::info!(
            bun = %bun_path.display(),
            entry = %entry_path.display(),
            gbrain_home = %gbrain_home.display(),
            "gbrain Sprint 2.1: seeded bundled MCP entry"
        );
        Ok(true)
    }

    pub fn remove_server(&mut self, id: &str) -> Option<McpServerConfig> {
        // PR-3 — abort any health loop for this server first so a
        // delayed reconnect attempt can't recreate the connection
        // after removal.
        self.stop_health_loop(id);
        // PR-5 — audit BEFORE the state is dropped so we have access
        // to the env for redaction (record_audit looks up the server).
        self.record_audit(id, McpAuditKind::Removed, "Server removed by user");
        let state = self.servers.remove(id)?;
        self.save_config();
        Some(state.config)
    }

    pub fn update_server(&mut self, id: &str, config: McpServerConfig) -> Result<(), String> {
        if !self.servers.contains_key(id) {
            return Err(format!("Server {} not found", id));
        }
        if let Some(state) = self.servers.get_mut(id) {
            state.config = config;
        }
        self.save_config();
        Ok(())
    }

    pub fn set_enabled(&mut self, id: &str, enabled: bool) -> bool {
        if let Some(state) = self.servers.get_mut(id) {
            state.config.enabled = enabled;
            self.save_config();
            return true;
        }
        false
    }

    pub fn set_auto_approve(&mut self, id: &str, auto_approve: bool) -> bool {
        if let Some(state) = self.servers.get_mut(id) {
            state.config.auto_approve = auto_approve;
            self.save_config();
            return true;
        }
        false
    }

    // ── Status & Queries ────────────────────────────────────────────

    pub fn set_status(&mut self, id: &str, status: McpServerStatus) {
        if let Some(state) = self.servers.get_mut(id) {
            state.status = status;
        }
    }

    pub fn set_tools(&mut self, id: &str, tools: Vec<McpToolDef>) {
        if let Some(state) = self.servers.get_mut(id) {
            state.tools = tools;
        }
    }

    /// Set or clear the error message for a server. PR-5: redact env
    /// values from the message before storing so a screenshot of the
    /// detail drawer can't leak `GITHUB_TOKEN=ghp_xxxx`. Clearing
    /// (passing `None`) is unchanged.
    pub fn set_error(&mut self, id: &str, error: Option<String>) {
        if let Some(state) = self.servers.get_mut(id) {
            let redacted = error.map(|e| redact_env_values(&e, &state.config.env));
            let is_err = redacted.is_some();
            state.error = redacted;
            if is_err {
                state.status = McpServerStatus::Error;
            }
        }
    }

    pub fn enabled_servers(&self) -> Vec<&McpServerConfig> {
        self.servers
            .values()
            .filter(|s| s.config.enabled)
            .map(|s| &s.config)
            .collect()
    }

    pub fn all_servers(&self) -> Vec<&McpServerConfig> {
        self.servers.values().map(|s| &s.config).collect()
    }

    pub fn all_tools(&self) -> Vec<McpToolDef> {
        self.servers
            .values()
            .filter(|s| s.status == McpServerStatus::Connected)
            .flat_map(|s| s.tools.clone())
            .collect()
    }

    pub fn status(&self, id: &str) -> Option<McpServerStatus> {
        self.servers.get(id).map(|s| s.status.clone())
    }

    /// Get detailed status for all servers (for IPC)
    pub fn all_server_statuses(&self) -> Vec<(String, McpServerStatus, Option<String>)> {
        self.servers
            .values()
            .map(|s| (s.config.id.clone(), s.status.clone(), s.error.clone()))
            .collect()
    }

    // ── Connection Lifecycle ────────────────────────────────────────

    /// Connect to an MCP server: spawn transport, initialize, discover tools
    pub async fn connect_server(&mut self, id: &str) -> Result<(), McpError> {
        let config = {
            let state = self.servers.get(id).ok_or_else(|| {
                McpError::Server(format!("Server {} not found", id))
            })?;
            state.config.clone()
        };

        // Update status to connecting
        if let Some(state) = self.servers.get_mut(id) {
            state.status = McpServerStatus::Connecting;
            state.error = None;
        }
        // PR-5 — log the attempt so audits show "tried then failed" not
        // just the failure (helps distinguish boot races from later
        // user actions).
        self.record_audit(
            id,
            McpAuditKind::ConnectAttempt,
            &format!("Connecting to {}", config.name),
        );

        tracing::info!("Connecting to MCP server '{}' ({})", config.name, id);

        // Create transport based on type
        let transport: Arc<dyn McpTransport> = match config.transport_type {
            TransportType::Stdio => {
                let t = StdioTransport::spawn(
                    &config.name,
                    &config.command,
                    &config.args,
                    &config.env,
                    // PR-4 — server id + (optional) notification sender
                    // get wired into the reader task so server-pushed
                    // events route through the manager-level consumer.
                    id,
                    self.notification_tx.clone(),
                )
                .await
                .map_err(|e| {
                    if let Some(state) = self.servers.get_mut(id) {
                        state.status = McpServerStatus::Error;
                        state.error = Some(e.to_string());
                    }
                    e
                })?;
                Arc::new(t)
            }
            TransportType::Http => {
                let url = config.url.clone().unwrap_or_default();
                if url.is_empty() {
                    let err = McpError::Server("HTTP transport requires a URL".into());
                    if let Some(state) = self.servers.get_mut(id) {
                        state.status = McpServerStatus::Error;
                        state.error = Some(err.to_string());
                    }
                    return Err(err);
                }
                Arc::new(HttpTransport::new(&config.name, &url))
            }
        };

        let mut conn = McpConnection {
            transport,
            next_id: AtomicU64::new(1),
            initialized: false,
            tools: Vec::new(),
            server_info: None,
        };

        // Initialize
        match conn.initialize().await {
            Ok(init_result) => {
                tracing::info!(
                    "MCP server '{}' initialized (protocol: {:?}, server: {:?})",
                    config.name,
                    init_result.protocol_version,
                    init_result.server_info.as_ref().map(|s| &s.name),
                );
            }
            Err(e) => {
                tracing::error!("MCP server '{}' initialize failed: {}", config.name, e);
                let _ = conn.shutdown().await;
                if let Some(state) = self.servers.get_mut(id) {
                    state.status = McpServerStatus::Error;
                    state.error = Some(e.to_string());
                }
                return Err(e);
            }
        }

        // Discover tools
        match conn.discover_tools().await {
            Ok(remote_tools) => {
                let tool_defs: Vec<McpToolDef> = remote_tools
                    .iter()
                    .map(|t| McpToolDef {
                        server_id: id.to_string(),
                        name: t.name.clone(),
                        description: t.description.clone(),
                        parameters: t.input_schema.clone(),
                    })
                    .collect();
                tracing::info!(
                    "MCP server '{}' has {} tool(s): [{}]",
                    config.name,
                    tool_defs.len(),
                    tool_defs.iter().map(|t| t.name.as_str()).collect::<Vec<_>>().join(", ")
                );
                if let Some(state) = self.servers.get_mut(id) {
                    state.tools = tool_defs;
                }
            }
            Err(e) => {
                tracing::warn!("MCP server '{}' tools/list failed: {}", config.name, e);
                // Non-fatal: server may not support tools
            }
        }

        // Mark connected
        let tool_count = self
            .servers
            .get(id)
            .map(|s| s.tools.len())
            .unwrap_or(0);
        if let Some(state) = self.servers.get_mut(id) {
            state.status = McpServerStatus::Connected;
            state.error = None;
            state.connection = Some(conn);
        }
        // PR-5 — successful connect lands an audit row with the tool
        // count so the table doubles as a "what was visible when" log.
        self.record_audit(
            id,
            McpAuditKind::ConnectSucceeded,
            &format!("Connected ({} tool(s) discovered)", tool_count),
        );

        Ok(())
    }

    /// Disconnect from an MCP server. Also aborts the health loop
    /// (PR-3) so a pending reconnect can't fight a user-initiated
    /// disconnect. Caller is expected to call `start_health_loop`
    /// again after the next successful connect.
    pub async fn disconnect_server(&mut self, id: &str) -> Result<(), McpError> {
        self.stop_health_loop(id);
        if let Some(state) = self.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                conn.shutdown().await?;
            }
            state.status = McpServerStatus::Disconnected;
            state.tools.clear();
            state.error = None;
            tracing::info!("Disconnected from MCP server '{}'", state.config.name);
        }
        // PR-5 — outside the `if let` so we still audit the call even
        // when the server isn't in the map (e.g. removed mid-flight).
        self.record_audit(id, McpAuditKind::Disconnect, "Disconnected");
        Ok(())
    }

    /// Restart a server connection
    pub async fn restart_server(&mut self, id: &str) -> Result<(), McpError> {
        self.disconnect_server(id).await.ok();
        self.connect_server(id).await
    }

    // ── PR-3 — health loop management ───────────────────────────────

    /// Spawn (or replace) the per-server health/reconnect background
    /// task. Idempotent: if a loop is already running for this id it's
    /// aborted before the new one starts. Caller passes the shared
    /// manager arc so the spawned task can re-acquire the lock for
    /// ping + reconnect without holding a borrow across the spawn.
    pub fn start_health_loop(&mut self, mgr: SharedMcpManager, id: &str) {
        if let Some(h) = self.health_tasks.remove(id) {
            h.abort();
        }
        let id_owned = id.to_string();
        let handle = tokio::spawn(async move {
            Self::run_health_loop(mgr, id_owned).await;
        });
        self.health_tasks.insert(id.to_string(), handle);
    }

    /// Abort the loop for `id` if any. Caller invokes this from
    /// `disconnect_server` / `remove_server`.
    pub fn stop_health_loop(&mut self, id: &str) {
        if let Some(h) = self.health_tasks.remove(id) {
            h.abort();
            tracing::debug!("[{}] health loop aborted", id);
        }
    }

    /// The actual loop body. Lives outside the impl block conceptually
    /// (it doesn't take &self) so the spawn closure isn't required to
    /// hold a borrow back into the manager.
    ///
    /// Two phases per iteration:
    /// 1. Sleep `HEALTH_PING_INTERVAL_SECS`, then ping. On success
    ///    reset the backoff and loop.
    /// 2. On failure, flip the server's status to Error with a
    ///    descriptive message, sleep `min(INITIAL * 2^attempt, MAX)`,
    ///    then call `reconnect_server`. Success resets attempt; failure
    ///    bumps attempt and loops back to phase 2's sleep.
    ///
    /// The loop is cancellation-aware: `tokio::spawn`-ed tasks die when
    /// the JoinHandle is aborted, which is what `stop_health_loop`
    /// does. No explicit shutdown signal needed.
    async fn run_health_loop(mgr: SharedMcpManager, id: String) {
        let mut attempt: u32 = 0;
        loop {
            tokio::time::sleep(Duration::from_secs(HEALTH_PING_INTERVAL_SECS)).await;

            // Ping under a read lock — short critical section.
            let ping_result = {
                let m = mgr.read().await;
                m.ping_server(&id).await
            };

            match ping_result {
                Ok(()) => {
                    // Healthy: reset attempt counter and continue the
                    // outer loop. If the server was in Error from a
                    // previous failure-then-recovery cycle, the
                    // reconnect path below already cleared it.
                    attempt = 0;
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        "[{}] health ping failed: {} (attempt {})",
                        id,
                        e,
                        attempt + 1
                    );
                    // Compute backoff *before* messaging so the UI
                    // shows "next attempt in 80s" rather than the
                    // misleading current attempt's delay.
                    let delay = std::cmp::min(
                        RECONNECT_INITIAL_DELAY_SECS
                            .saturating_mul(2u64.saturating_pow(attempt)),
                        RECONNECT_MAX_DELAY_SECS,
                    );
                    {
                        let msg = format!(
                            "Health check failed: {} — reconnecting in {}s (attempt {})",
                            e,
                            delay,
                            attempt + 1
                        );
                        let mut m = mgr.write().await;
                        m.set_error(&id, Some(msg.clone()));
                        // PR-5 — also persist to the audit table so the
                        // user can review history across restarts.
                        m.record_audit(&id, McpAuditKind::HealthFailed, &msg);
                    }
                    tokio::time::sleep(Duration::from_secs(delay)).await;

                    let reconnect_result = {
                        let mut m = mgr.write().await;
                        m.reconnect_server(&id).await
                    };
                    match reconnect_result {
                        Ok(()) => {
                            tracing::info!(
                                "[{}] reconnect succeeded after {} attempt(s)",
                                id,
                                attempt + 1
                            );
                            attempt = 0;
                        }
                        Err(rc_err) => {
                            tracing::warn!(
                                "[{}] reconnect attempt {} failed: {}",
                                id,
                                attempt + 1,
                                rc_err
                            );
                            attempt = attempt.saturating_add(1);
                        }
                    }
                }
            }
        }
    }

    /// Internal reconnect — like `restart_server` but doesn't abort
    /// the health loop (we *are* the health loop). The user-facing
    /// `restart_server` calls `disconnect_server` which aborts the
    /// loop; we don't want to commit suicide here.
    async fn reconnect_server(&mut self, id: &str) -> Result<(), McpError> {
        // Mirror restart_server's shape but inline the disconnect so we
        // can skip stop_health_loop.
        if let Some(state) = self.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                conn.shutdown().await.ok();
            }
            state.status = McpServerStatus::Disconnected;
        }
        self.connect_server(id).await
    }

    /// Connect all enabled servers
    pub async fn connect_all_enabled(&mut self) {
        let ids: Vec<String> = self
            .servers
            .values()
            .filter(|s| s.config.enabled)
            .map(|s| s.config.id.clone())
            .collect();

        for id in ids {
            if let Err(e) = self.connect_server(&id).await {
                tracing::error!("Failed to connect MCP server '{}': {}", id, e);
            }
        }
    }

    /// Disconnect all servers. Also aborts every health loop (PR-3) —
    /// `disconnect_server` does it per id but iterating that way is
    /// `O(n)` ops on the health_tasks map; doing it in one drain is
    /// cleaner and matches the "we're shutting down" intent.
    pub async fn disconnect_all(&mut self) {
        for (_id, h) in self.health_tasks.drain() {
            h.abort();
        }
        let ids: Vec<String> = self.servers.keys().cloned().collect();
        for id in ids {
            self.disconnect_server(&id).await.ok();
        }
    }

    /// Health check (ping) a connected server
    pub async fn ping_server(&self, id: &str) -> Result<(), McpError> {
        let state = self.servers.get(id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", id))
        })?;
        conn.ping().await
    }

    /// Refresh tools for a connected server
    pub async fn refresh_tools(&mut self, id: &str) -> Result<Vec<McpToolDef>, McpError> {
        let remote_tools = {
            let state = self.servers.get_mut(id).ok_or_else(|| {
                McpError::Server(format!("Server {} not found", id))
            })?;
            let conn = state.connection.as_mut().ok_or_else(|| {
                McpError::Server(format!("Server {} is not connected", id))
            })?;
            conn.discover_tools().await?
        };

        let tool_defs: Vec<McpToolDef> = remote_tools
            .iter()
            .map(|t| McpToolDef {
                server_id: id.to_string(),
                name: t.name.clone(),
                description: t.description.clone(),
                parameters: t.input_schema.clone(),
            })
            .collect();

        if let Some(state) = self.servers.get_mut(id) {
            state.tools = tool_defs.clone();
        }

        Ok(tool_defs)
    }

    // ── Tool Proxying ───────────────────────────────────────────────

    /// Get a cloneable transport handle and a next-id generator for a connected server.
    /// Used by McpToolProxy to call tools without holding the manager lock.
    pub(crate) fn get_transport(
        &self,
        server_id: &str,
    ) -> Result<(Arc<dyn McpTransport>, u64), McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        let id = conn.next_id();
        Ok((conn.transport.clone(), id))
    }

    /// Call a tool on a connected MCP server
    pub async fn call_tool(
        &self,
        server_id: &str,
        tool_name: &str,
        arguments: serde_json::Value,
    ) -> Result<CallToolResult, McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;

        tracing::debug!("Calling MCP tool '{}' on server '{}'", tool_name, server_id);
        conn.call_tool(tool_name, arguments).await
    }

    /// List resources from a connected MCP server
    pub async fn list_resources(&self, server_id: &str) -> Result<serde_json::Value, McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        conn.list_resources().await
    }

    /// Read a resource from a connected MCP server
    pub async fn read_resource(
        &self,
        server_id: &str,
        uri: &str,
    ) -> Result<serde_json::Value, McpError> {
        let state = self.servers.get(server_id).ok_or_else(|| {
            McpError::Server(format!("Server {} not found", server_id))
        })?;
        let conn = state.connection.as_ref().ok_or_else(|| {
            McpError::Server(format!("Server {} is not connected", server_id))
        })?;
        conn.read_resource(uri).await
    }

    /// Create McpToolProxy instances for every tool exposed by the
    /// currently-connected servers. These wrap the MCP transport in the
    /// agent's `Tool` trait so the dispatcher can call MCP tools the
    /// same way it calls builtins.
    ///
    /// `locked` is the already-acquired read guard on the manager — by
    /// taking it explicitly we (a) avoid re-locking inside this method
    /// and (b) make the snapshot semantics obvious at the call site.
    /// `manager` is the shared handle each proxy keeps so it can run
    /// the actual `tools/call` JSON-RPC without re-borrowing.
    ///
    /// Names are emitted in the prefixed form
    /// `mcp__{server_id}__{tool_name}` so they're unambiguous in the
    /// LLM's tool manifest and in SafetyManager logs. `auto_approve` is
    /// snapshotted per-proxy so `Tool::requires_approval` short-circuits
    /// approval prompts for trusted servers.
    pub fn create_tool_proxies(
        manager: &SharedMcpManager,
        locked: &McpManager,
    ) -> Vec<McpToolProxy> {
        let auto_approve_by_id: HashMap<String, bool> = locked
            .all_servers()
            .iter()
            .map(|c| (c.id.clone(), c.auto_approve))
            .collect();
        locked
            .all_tools()
            .into_iter()
            .map(|tool| {
                let auto_approve = auto_approve_by_id
                    .get(&tool.server_id)
                    .copied()
                    .unwrap_or(false);
                let prefixed_name = prefixed_tool_name(&tool.server_id, &tool.name);
                McpToolProxy {
                    server_id: tool.server_id.clone(),
                    tool_name: tool.name.clone(),
                    prefixed_name,
                    description: tool.description.clone(),
                    input_schema: tool.parameters.clone(),
                    manager: manager.clone(),
                    auto_approve,
                }
            })
            .collect()
    }
}

/// gbrain Sprint 2.1 init-fix — probe whether `<gbrain_home>/.gbrain/brain.pglite/`
/// has been initialized by `gbrain init --pglite`. The presence of
/// `PG_VERSION` is the canonical Postgres-data-dir initialization marker
/// (PGLite writes it as part of `initdb`).
///
/// Pure — no I/O beyond `Path::exists`. Used by
/// `ensure_bundled_gbrain_initialized` to decide whether to spawn `gbrain
/// init` or skip (idempotent). Safe to call repeatedly.
pub fn is_brain_initialized(gbrain_home: &std::path::Path) -> bool {
    gbrain_home
        .join(".gbrain")
        .join("brain.pglite")
        .join("PG_VERSION")
        .exists()
}

/// gbrain Sprint 2.1 init-fix — run `bun <cli.ts> init --pglite --yes` against
/// `gbrain_home` if the brain isn't already initialized. Synchronous +
/// blocking — first call cold-starts PGLite + runs ~63 migrations
/// (~30-60s on Apple Silicon). Subsequent calls short-circuit via
/// `is_brain_initialized` and return `Ok(false)` in O(1).
///
/// Returns:
/// - `Ok(true)`  — freshly initialized
/// - `Ok(false)` — already initialized, no work done
/// - `Err(msg)`  — spawn failed OR `gbrain init` exited non-zero. Caller
///   MUST NOT proceed to seed the MCP entry, otherwise gbrain will spawn
///   and immediately exit with "No brain configured" on every connect.
///
/// `GBRAIN_HOME` is the only env var threaded through. `gbrain init`
/// writes `<gbrain_home>/.gbrain/config.json` itself with the correct
/// `database_path` — callers MUST NOT pre-write that file (the v0.35
/// init path uses its own layout, not whatever the caller passes).
pub fn ensure_bundled_gbrain_initialized(
    bun_path: &std::path::Path,
    entry_path: &std::path::Path,
    gbrain_home: &std::path::Path,
) -> Result<bool, String> {
    if is_brain_initialized(gbrain_home) {
        tracing::debug!(
            gbrain_home = %gbrain_home.display(),
            "ensure_bundled_gbrain_initialized: brain already initialized"
        );
        return Ok(false);
    }
    if let Err(e) = std::fs::create_dir_all(gbrain_home) {
        return Err(format!(
            "create gbrain_home {}: {}",
            gbrain_home.display(),
            e
        ));
    }
    tracing::info!(
        bun = %bun_path.display(),
        entry = %entry_path.display(),
        gbrain_home = %gbrain_home.display(),
        "gbrain Sprint 2.1 init-fix: running 'gbrain init --pglite --yes' (first launch, may take 30-60s)"
    );
    let output = std::process::Command::new(bun_path)
        .arg(entry_path)
        .arg("init")
        .arg("--pglite")
        .arg("--yes")
        .env("GBRAIN_HOME", gbrain_home)
        .output()
        .map_err(|e| format!("spawn 'bun gbrain init': {}", e))?;
    if !output.status.success() {
        let stderr_tail: String = String::from_utf8_lossy(&output.stderr)
            .lines()
            .rev()
            .take(20)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "'gbrain init' exited {:?}\nstderr (last 20 lines):\n{}",
            output.status.code(),
            stderr_tail
        ));
    }
    // Defense in depth: verify the marker really landed. Catches the case
    // where gbrain init exits 0 but writes to an unexpected path (bug
    // surface we are explicitly fixing in this PR).
    if !is_brain_initialized(gbrain_home) {
        return Err(format!(
            "'gbrain init' exited 0 but {} did not appear — \
             gbrain may have written to a different GBRAIN_HOME",
            gbrain_home.join(".gbrain/brain.pglite/PG_VERSION").display()
        ));
    }
    tracing::info!(
        gbrain_home = %gbrain_home.display(),
        "gbrain Sprint 2.1 init-fix: brain initialized successfully"
    );
    Ok(true)
}

/// Shared MCP manager for Tauri state
pub type SharedMcpManager = Arc<RwLock<McpManager>>;

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(id: &str, transport: TransportType) -> McpServerConfig {
        McpServerConfig {
            id: id.into(),
            name: format!("srv-{id}"),
            description: String::new(),
            transport_type: transport,
            command: "npx".into(),
            args: vec!["-y".into()],
            env: HashMap::new(),
            url: None,
            enabled: true,
            auto_approve: false,
        }
    }

    #[test]
    fn add_server_preserves_transport_type_and_url() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let mut http = cfg("a", TransportType::Http);
        http.url = Some("https://example.com/mcp".into());
        mgr.add_server(http).unwrap();
        let stored = mgr.all_servers().into_iter().find(|c| c.id == "a").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/mcp"));
    }

    #[test]
    fn update_server_rewrites_config_and_persists_to_disk() {
        let dir = tempfile::tempdir().unwrap();
        {
            let mut mgr = McpManager::new(dir.path());
            mgr.add_server(cfg("b", TransportType::Stdio)).unwrap();
            let mut updated = cfg("b", TransportType::Http);
            updated.url = Some("https://example.com/b".into());
            updated.auto_approve = true;
            mgr.update_server("b", updated).unwrap();
        }
        // Re-open from disk — confirms save_config persisted the update.
        let mgr2 = McpManager::new(dir.path());
        let stored = mgr2.all_servers().into_iter().find(|c| c.id == "b").unwrap();
        assert_eq!(stored.transport_type, TransportType::Http);
        assert_eq!(stored.url.as_deref(), Some("https://example.com/b"));
        assert!(stored.auto_approve);
    }

    #[test]
    fn update_server_missing_id_errors() {
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());
        let err = mgr
            .update_server("nope", cfg("nope", TransportType::Stdio))
            .unwrap_err();
        assert!(err.contains("not found"));
    }

    // ─── PR-1 — prefix helpers + auto_approve plumbing ──────────────

    #[test]
    fn prefixed_tool_name_format_matches_convention() {
        // The mcp__{server}__{tool} shape is the Cline / Roo / Claude
        // Desktop convention; consumers (SafetyManager, UI badges,
        // telemetry) rely on it to recognize MCP-sourced calls without
        // a separate registry lookup.
        let n = prefixed_tool_name("github", "create_issue");
        assert_eq!(n, "mcp__github__create_issue");
    }

    #[test]
    fn parse_mcp_tool_name_round_trips() {
        let name = prefixed_tool_name("github", "create_issue");
        let parsed = parse_mcp_tool_name(&name).unwrap();
        assert_eq!(parsed.0, "github");
        assert_eq!(parsed.1, "create_issue");
    }

    #[test]
    fn parse_mcp_tool_name_handles_underscore_in_server_id() {
        // Server ids commonly contain single underscores ("my_team_search").
        // The split must be on the FIRST "__" (double-underscore)
        // boundary, not any single underscore, or those ids round-trip
        // wrong.
        let name = "mcp__my_team_search__do_thing";
        let parsed = parse_mcp_tool_name(name).unwrap();
        assert_eq!(parsed.0, "my_team_search");
        assert_eq!(parsed.1, "do_thing");
    }

    #[test]
    fn parse_mcp_tool_name_rejects_non_mcp_names() {
        // Builtins (read_file, edit, plan_update, …) must fast-path
        // through the parser as `None` so SafetyManager doesn't waste
        // cycles searching for a non-existent MCP server.
        assert!(parse_mcp_tool_name("read_file").is_none());
        assert!(parse_mcp_tool_name("mcp__").is_none()); // no server / tool
        assert!(parse_mcp_tool_name("mcp__github__").is_none()); // empty tool
        assert!(parse_mcp_tool_name("mcp____tool").is_none()); // empty server
    }

    #[test]
    fn create_tool_proxies_emits_prefixed_names_and_honors_auto_approve() {
        // Build a manager with two configured servers, one auto-approved
        // and one not, both with a single discovered tool. Verify the
        // returned proxies carry the right prefix and the auto_approve
        // flag is snapshotted onto each.
        let dir = tempfile::tempdir().unwrap();
        let mut mgr = McpManager::new(dir.path());

        let mut trusted = cfg("trusted", TransportType::Stdio);
        trusted.auto_approve = true;
        mgr.add_server(trusted).unwrap();

        let untrusted = cfg("untrusted", TransportType::Stdio);
        mgr.add_server(untrusted).unwrap();

        // Simulate the post-connect state: tools discovered, status =
        // Connected. We bypass the actual transport here — all_tools()
        // filters on `status == Connected` so we have to set it.
        for id in ["trusted", "untrusted"] {
            if let Some(state) = mgr.servers.get_mut(id) {
                state.status = McpServerStatus::Connected;
                state.tools.push(McpToolDef {
                    server_id: id.to_string(),
                    name: "do_thing".to_string(),
                    description: format!("thing on {id}"),
                    parameters: serde_json::json!({}),
                });
            }
        }

        let shared: SharedMcpManager = Arc::new(RwLock::new(mgr));
        // Re-acquire a borrow for create_tool_proxies — we can't both
        // pass shared+locked in one statement so split the borrow.
        let proxies = {
            let locked = shared.try_read().unwrap();
            McpManager::create_tool_proxies(&shared, &*locked)
        };

        let names: Vec<&str> = proxies.iter().map(|p| p.name()).collect();
        assert!(names.contains(&"mcp__trusted__do_thing"));
        assert!(names.contains(&"mcp__untrusted__do_thing"));

        let trusted_proxy = proxies.iter().find(|p| p.server_id == "trusted").unwrap();
        let untrusted_proxy = proxies.iter().find(|p| p.server_id == "untrusted").unwrap();
        assert!(trusted_proxy.auto_approve);
        assert!(!untrusted_proxy.auto_approve);

        use crate::agent::tools::tool::{ApprovalRequirement, Tool};
        let v = serde_json::json!({});
        assert_eq!(
            trusted_proxy.requires_approval(&v),
            ApprovalRequirement::Never
        );
        assert_eq!(
            untrusted_proxy.requires_approval(&v),
            ApprovalRequirement::UnlessAutoApproved
        );
    }
}

#[cfg(test)]
mod gbrain_init_tests {
    use super::*;
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn is_brain_initialized_returns_false_for_empty_gbrain_home() {
        let dir = tempdir().unwrap();
        assert!(!is_brain_initialized(dir.path()));
    }

    #[test]
    fn is_brain_initialized_returns_false_when_brain_dir_missing_pg_version() {
        let dir = tempdir().unwrap();
        // .gbrain/brain.pglite/ exists but no PG_VERSION inside
        fs::create_dir_all(dir.path().join(".gbrain/brain.pglite")).unwrap();
        assert!(!is_brain_initialized(dir.path()));
    }

    #[test]
    fn is_brain_initialized_returns_true_when_pg_version_present() {
        let dir = tempdir().unwrap();
        let brain = dir.path().join(".gbrain/brain.pglite");
        fs::create_dir_all(&brain).unwrap();
        fs::write(brain.join("PG_VERSION"), "17\n").unwrap();
        assert!(is_brain_initialized(dir.path()));
    }
}
