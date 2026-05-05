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
    ) -> Result<Self, McpError> {
        let server_name = name.into();

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
                if value.get("method").is_some() {
                    // Server notification/request — log and skip
                    tracing::debug!(
                        "[{}] Received server notification: {}",
                        reader_name,
                        value.get("method").and_then(|m| m.as_str()).unwrap_or("?")
                    );
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

pub struct McpToolProxy {
    server_id: String,
    tool_name: String,
    description: String,
    input_schema: serde_json::Value,
    manager: SharedMcpManager,
}

#[async_trait]
impl crate::agent::tools::tool::Tool for McpToolProxy {
    fn name(&self) -> &str {
        &self.tool_name
    }

    fn description(&self) -> &str {
        &self.description
    }

    fn parameters_schema(&self) -> serde_json::Value {
        self.input_schema.clone()
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
}

impl McpManager {
    pub fn new(data_dir: &std::path::Path) -> Self {
        let config_path = data_dir.join("mcp_servers.json");
        let mut manager = Self {
            servers: HashMap::new(),
            config_path,
        };
        manager.load_config();
        manager
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

    pub fn remove_server(&mut self, id: &str) -> Option<McpServerConfig> {
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

    pub fn set_error(&mut self, id: &str, error: Option<String>) {
        if let Some(state) = self.servers.get_mut(id) {
            let is_err = error.is_some();
            state.error = error;
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

        tracing::info!("Connecting to MCP server '{}' ({})", config.name, id);

        // Create transport based on type
        let transport: Arc<dyn McpTransport> = match config.transport_type {
            TransportType::Stdio => {
                let t = StdioTransport::spawn(&config.name, &config.command, &config.args, &config.env)
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
        if let Some(state) = self.servers.get_mut(id) {
            state.status = McpServerStatus::Connected;
            state.error = None;
            state.connection = Some(conn);
        }

        Ok(())
    }

    /// Disconnect from an MCP server
    pub async fn disconnect_server(&mut self, id: &str) -> Result<(), McpError> {
        if let Some(state) = self.servers.get_mut(id) {
            if let Some(conn) = state.connection.take() {
                conn.shutdown().await?;
            }
            state.status = McpServerStatus::Disconnected;
            state.tools.clear();
            state.error = None;
            tracing::info!("Disconnected from MCP server '{}'", state.config.name);
        }
        Ok(())
    }

    /// Restart a server connection
    pub async fn restart_server(&mut self, id: &str) -> Result<(), McpError> {
        self.disconnect_server(id).await.ok();
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

    /// Disconnect all servers
    pub async fn disconnect_all(&mut self) {
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

    /// Create McpToolProxy instances for all tools from connected servers.
    /// These can be registered in the ToolRegistry for the agent loop.
    pub fn create_tool_proxies(manager: &SharedMcpManager, tools: &[McpToolDef]) -> Vec<McpToolProxy> {
        tools
            .iter()
            .map(|tool| McpToolProxy {
                server_id: tool.server_id.clone(),
                tool_name: tool.name.clone(),
                description: tool.description.clone(),
                input_schema: tool.parameters.clone(),
                manager: manager.clone(),
            })
            .collect()
    }
}

/// Shared MCP manager for Tauri state
pub type SharedMcpManager = Arc<RwLock<McpManager>>;
