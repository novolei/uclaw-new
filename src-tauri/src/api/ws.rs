//! WebSocket real-time communication module.
//!
//! Implements a JSON-RPC style protocol for bidirectional messaging,
//! event subscriptions, and Agent streaming event forwarding.

use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::IntoResponse,
};
use chrono::{DateTime, Utc};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use crate::api::auth::{HttpServerState, verify_token};

// ─── WebSocket Message Protocol ──────────────────────────────────────────────

/// Client -> Server request (JSON-RPC style)
#[derive(Debug, Clone, Deserialize)]
pub struct WsRequest {
    /// Request ID for correlation
    pub id: Option<String>,
    /// Method name (e.g. "chat.send", "subscribe")
    pub method: String,
    /// Method parameters
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Server -> Client response (correlated to a request)
#[derive(Debug, Clone, Serialize)]
pub struct WsResponse {
    /// Request ID (echoed from request)
    pub id: Option<String>,
    /// Result payload (present on success)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    /// Error payload (present on failure)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WsError>,
}

/// Server -> Client event push (unsolicited)
#[derive(Debug, Clone, Serialize)]
pub struct WsEvent {
    /// Event name (e.g. "stream.text_delta")
    pub event: String,
    /// Event data payload
    pub data: serde_json::Value,
}

/// Error detail in WsResponse
#[derive(Debug, Clone, Serialize)]
pub struct WsError {
    pub code: i32,
    pub message: String,
}

/// Outbound message envelope sent to a connection's write task
#[derive(Debug, Clone)]
pub enum WsOutbound {
    Response(WsResponse),
    Event(WsEvent),
    RawPing,
}

impl WsOutbound {
    fn into_text(self) -> Option<String> {
        match self {
            WsOutbound::Response(r) => serde_json::to_string(&r).ok(),
            WsOutbound::Event(e) => serde_json::to_string(&e).ok(),
            WsOutbound::RawPing => None,
        }
    }
}

// ─── Error Codes ─────────────────────────────────────────────────────────────

const ERR_PARSE: i32 = -32700;
const ERR_METHOD_NOT_FOUND: i32 = -32601;
const ERR_INVALID_PARAMS: i32 = -32602;
#[allow(dead_code)]
const ERR_INTERNAL: i32 = -32603;

fn err_response(id: Option<String>, code: i32, msg: impl Into<String>) -> WsResponse {
    WsResponse {
        id,
        result: None,
        error: Some(WsError { code, message: msg.into() }),
    }
}

fn ok_response(id: Option<String>, result: serde_json::Value) -> WsResponse {
    WsResponse { id, result: Some(result), error: None }
}

// ─── Connection & Connection Manager ─────────────────────────────────────────

/// A single WebSocket connection
#[allow(dead_code)]
struct WsConnection {
    id: String,
    user_id: String,
    sender: mpsc::Sender<WsOutbound>,
    subscriptions: HashSet<String>,
    connected_at: DateTime<Utc>,
    last_activity: DateTime<Utc>,
}

/// Manages all active WebSocket connections
#[derive(Clone)]
pub struct WsConnectionManager {
    connections: Arc<RwLock<HashMap<String, WsConnection>>>,
}

impl WsConnectionManager {
    pub fn new() -> Self {
        Self {
            connections: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new connection. Returns its sender handle.
    async fn register(
        &self,
        conn_id: &str,
        user_id: &str,
    ) -> mpsc::Receiver<WsOutbound> {
        let (tx, rx) = mpsc::channel::<WsOutbound>(256);
        let conn = WsConnection {
            id: conn_id.to_string(),
            user_id: user_id.to_string(),
            sender: tx,
            subscriptions: HashSet::new(),
            connected_at: Utc::now(),
            last_activity: Utc::now(),
        };
        self.connections.write().await.insert(conn_id.to_string(), conn);
        tracing::info!(conn_id, user_id, "WebSocket connection registered");
        rx
    }

    /// Unregister a connection
    async fn unregister(&self, conn_id: &str) {
        if self.connections.write().await.remove(conn_id).is_some() {
            tracing::info!(conn_id, "WebSocket connection unregistered");
        }
    }

    /// Update last activity timestamp
    async fn touch(&self, conn_id: &str) {
        if let Some(conn) = self.connections.write().await.get_mut(conn_id) {
            conn.last_activity = Utc::now();
        }
    }

    /// Subscribe a connection to channels
    async fn subscribe(&self, conn_id: &str, channels: Vec<String>) -> Vec<String> {
        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(conn_id) {
            for ch in &channels {
                conn.subscriptions.insert(ch.clone());
            }
            tracing::debug!(conn_id, ?channels, "Subscribed to channels");
            conn.subscriptions.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Unsubscribe a connection from channels
    async fn unsubscribe(&self, conn_id: &str, channels: Vec<String>) -> Vec<String> {
        let mut conns = self.connections.write().await;
        if let Some(conn) = conns.get_mut(conn_id) {
            for ch in &channels {
                conn.subscriptions.remove(ch);
            }
            tracing::debug!(conn_id, ?channels, "Unsubscribed from channels");
            conn.subscriptions.iter().cloned().collect()
        } else {
            Vec::new()
        }
    }

    /// Broadcast an event to all connections subscribed to a channel
    pub async fn broadcast_to_channel(&self, channel: &str, event: WsEvent) {
        let conns = self.connections.read().await;
        let mut sent = 0u32;
        for conn in conns.values() {
            if conn.subscriptions.contains(channel) {
                let _ = conn.sender.try_send(WsOutbound::Event(event.clone()));
                sent += 1;
            }
        }
        if sent > 0 {
            tracing::trace!(channel, sent, "Broadcast event to subscribers");
        }
    }

    /// Broadcast an event to ALL connections (system-level)
    pub async fn broadcast_all(&self, event: WsEvent) {
        let conns = self.connections.read().await;
        for conn in conns.values() {
            let _ = conn.sender.try_send(WsOutbound::Event(event.clone()));
        }
    }

    /// Send an event to a specific connection
    pub async fn send_to(&self, conn_id: &str, outbound: WsOutbound) {
        let conns = self.connections.read().await;
        if let Some(conn) = conns.get(conn_id) {
            let _ = conn.sender.try_send(outbound);
        }
    }

    /// Get the number of active connections
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Disconnect stale connections (no activity for `timeout` duration)
    async fn disconnect_stale(&self, timeout: chrono::Duration) {
        let now = Utc::now();
        let mut conns = self.connections.write().await;
        let stale: Vec<String> = conns
            .iter()
            .filter(|(_, c)| now.signed_duration_since(c.last_activity) > timeout)
            .map(|(id, _)| id.clone())
            .collect();
        for id in &stale {
            conns.remove(id);
            tracing::info!(conn_id = %id, "Disconnected stale WebSocket connection");
        }
    }

    /// Broadcast a stream event to subscribers of a session channel
    pub async fn emit_stream_event(&self, session_id: &str, event_name: &str, data: serde_json::Value) {
        let channel = format!("session:{}", session_id);
        self.broadcast_to_channel(
            &channel,
            WsEvent {
                event: event_name.to_string(),
                data,
            },
        )
        .await;
    }
}

// ─── Query Params ────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct WsParams {
    /// JWT token for authentication
    pub token: Option<String>,
}

// ─── WebSocket Upgrade Handler ───────────────────────────────────────────────

/// GET /api/ws — WebSocket upgrade endpoint
pub async fn ws_handler(
    State(state): State<HttpServerState>,
    Query(params): Query<WsParams>,
    ws: WebSocketUpgrade,
) -> Result<impl IntoResponse, StatusCode> {
    // Authenticate via query token
    let user_id = if let Some(token) = &params.token {
        match verify_token(token, &state.jwt_secret) {
            Ok(claims) => claims.sub,
            Err(_) => return Err(StatusCode::UNAUTHORIZED),
        }
    } else {
        // Allow anonymous connections for local use
        "anonymous".to_string()
    };

    Ok(ws.on_upgrade(move |socket| handle_ws(socket, state, user_id)))
}

// ─── Connection Lifecycle ────────────────────────────────────────────────────

/// Handle a single WebSocket connection's full lifecycle
async fn handle_ws(socket: WebSocket, state: HttpServerState, user_id: String) {
    let conn_id = uuid::Uuid::new_v4().to_string();
    let ws_mgr = &state.ws_manager;

    // Register connection and get outbound receiver
    let mut outbound_rx = ws_mgr.register(&conn_id, &user_id).await;

    let (mut ws_sink, mut ws_stream) = socket.split();

    // Send welcome event
    let welcome = WsEvent {
        event: "connected".to_string(),
        data: serde_json::json!({
            "connection_id": conn_id,
            "version": env!("CARGO_PKG_VERSION"),
        }),
    };
    if let Some(text) = WsOutbound::Event(welcome).into_text() {
        let _ = ws_sink.send(Message::Text(text.into())).await;
    }

    // Clone what we need for the outbound writer task
    let conn_id_write = conn_id.clone();
    let _ws_mgr_write = ws_mgr.clone();

    // ── Outbound writer task ─────────────────────────────────────────────
    let write_handle = tokio::spawn(async move {
        // Heartbeat: send ping every 30 seconds
        let mut ping_interval = tokio::time::interval(std::time::Duration::from_secs(30));
        ping_interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                // Outbound messages from the application
                msg = outbound_rx.recv() => {
                    match msg {
                        Some(WsOutbound::RawPing) => {
                            if ws_sink.send(Message::Ping(vec![].into())).await.is_err() {
                                break;
                            }
                        }
                        Some(outbound) => {
                            if let Some(text) = outbound.into_text() {
                                if ws_sink.send(Message::Text(text.into())).await.is_err() {
                                    break;
                                }
                            }
                        }
                        None => break, // channel closed
                    }
                }
                // Periodic ping
                _ = ping_interval.tick() => {
                    if ws_sink.send(Message::Ping(vec![].into())).await.is_err() {
                        break;
                    }
                }
            }
        }
        // Try graceful close
        let _ = ws_sink.close().await;
        let _ = &conn_id_write; // prevent drop warning
    });

    // ── Inbound reader loop ──────────────────────────────────────────────
    let inactivity_timeout = std::time::Duration::from_secs(300); // 5 minutes

    loop {
        let read_result = tokio::time::timeout(inactivity_timeout, ws_stream.next()).await;

        match read_result {
            // Timeout — no activity for 5 minutes
            Err(_) => {
                tracing::info!(conn_id = %conn_id, "WebSocket inactivity timeout, closing");
                break;
            }
            // Stream ended
            Ok(None) => break,
            // Message received
            Ok(Some(Ok(msg))) => {
                ws_mgr.touch(&conn_id).await;

                match msg {
                    Message::Text(text) => {
                        let response = handle_text_message(
                            &text,
                            &conn_id,
                            &user_id,
                            &state,
                        )
                        .await;
                        if let Some(resp) = response {
                            ws_mgr.send_to(&conn_id, WsOutbound::Response(resp)).await;
                        }
                    }
                    Message::Ping(_) => {
                        // axum handles pong automatically
                    }
                    Message::Pong(_) => {
                        // heartbeat acknowledged
                    }
                    Message::Close(_) => {
                        tracing::debug!(conn_id = %conn_id, "Received close frame");
                        break;
                    }
                    _ => {}
                }
            }
            // Read error
            Ok(Some(Err(e))) => {
                tracing::warn!(conn_id = %conn_id, error = %e, "WebSocket read error");
                break;
            }
        }
    }

    // ── Cleanup ──────────────────────────────────────────────────────────
    ws_mgr.unregister(&conn_id).await;
    write_handle.abort();
    tracing::info!(conn_id = %conn_id, "WebSocket connection closed");
}

// ─── Request Routing ─────────────────────────────────────────────────────────

/// Parse and route a text message from a WebSocket client
async fn handle_text_message(
    text: &str,
    conn_id: &str,
    user_id: &str,
    state: &HttpServerState,
) -> Option<WsResponse> {
    let req: WsRequest = match serde_json::from_str(text) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(conn_id, error = %e, "Failed to parse WS message");
            return Some(err_response(None, ERR_PARSE, format!("Parse error: {}", e)));
        }
    };

    let req_id = req.id.clone();
    tracing::debug!(conn_id, method = %req.method, "Handling WS request");

    let result = match req.method.as_str() {
        "ping" => handle_ping(req_id.clone()),
        "subscribe" => handle_subscribe(conn_id, &req.params, state).await,
        "unsubscribe" => handle_unsubscribe(conn_id, &req.params, state).await,
        "chat.send" => handle_chat_send(conn_id, user_id, &req.params, state).await,
        "chat.stop" => handle_chat_stop(&req.params, state).await,
        "chat.approve" => handle_chat_approve(&req.params, state).await,
        _ => {
            Err(err_response(
                req_id.clone(),
                ERR_METHOD_NOT_FOUND,
                format!("Unknown method: {}", req.method),
            ))
        }
    };

    Some(match result {
        Ok(value) => ok_response(req_id, value),
        Err(resp) => resp,
    })
}

// ─── Method Handlers ─────────────────────────────────────────────────────────

/// Handle "ping" — simple heartbeat
fn handle_ping(_req_id: Option<String>) -> Result<serde_json::Value, WsResponse> {
    Ok(serde_json::json!({
        "pong": true,
        "timestamp": Utc::now().to_rfc3339(),
    }))
}

/// Handle "subscribe" — subscribe to event channels
async fn handle_subscribe(
    conn_id: &str,
    params: &serde_json::Value,
    state: &HttpServerState,
) -> Result<serde_json::Value, WsResponse> {
    let channels: Vec<String> = params
        .get("channels")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    if channels.is_empty() {
        return Err(err_response(None, ERR_INVALID_PARAMS, "Missing or empty 'channels' array"));
    }

    // Validate channel format
    for ch in &channels {
        if !is_valid_channel(ch) {
            return Err(err_response(
                None,
                ERR_INVALID_PARAMS,
                format!("Invalid channel format: {}", ch),
            ));
        }
    }

    let current = state.ws_manager.subscribe(conn_id, channels).await;
    Ok(serde_json::json!({
        "subscriptions": current,
    }))
}

/// Handle "unsubscribe" — unsubscribe from event channels
async fn handle_unsubscribe(
    conn_id: &str,
    params: &serde_json::Value,
    state: &HttpServerState,
) -> Result<serde_json::Value, WsResponse> {
    let channels: Vec<String> = params
        .get("channels")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    if channels.is_empty() {
        return Err(err_response(None, ERR_INVALID_PARAMS, "Missing or empty 'channels' array"));
    }

    let current = state.ws_manager.unsubscribe(conn_id, channels).await;
    Ok(serde_json::json!({
        "subscriptions": current,
    }))
}

/// Handle "chat.send" — send a message to the agent and auto-subscribe to session events
async fn handle_chat_send(
    conn_id: &str,
    _user_id: &str,
    params: &serde_json::Value,
    state: &HttpServerState,
) -> Result<serde_json::Value, WsResponse> {
    let space_id = params
        .get("space_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_response(None, ERR_INVALID_PARAMS, "Missing 'space_id'"))?;

    let message = params
        .get("message")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_response(None, ERR_INVALID_PARAMS, "Missing 'message'"))?;

    let session_id_param = params.get("session_id").and_then(|v| v.as_str());

    // Get or create session
    let session_id = {
        let mut sm = state.session_manager.write().await;
        if let Some(sid) = session_id_param {
            // Verify session exists
            if sm.get(sid).is_none() {
                return Err(err_response(None, ERR_INVALID_PARAMS, "Session not found"));
            }
            sid.to_string()
        } else {
            // Create a new session
            let summary = sm.create(
                &message.chars().take(50).collect::<String>(),
                space_id,
            );
            summary.id
        }
    };

    // Auto-subscribe this connection to the session channel
    let session_channel = format!("session:{}", session_id);
    state
        .ws_manager
        .subscribe(conn_id, vec![session_channel])
        .await;

    // Add user message to session
    {
        let mut sm = state.session_manager.write().await;
        sm.add_message(
            &session_id,
            crate::agent::types::ChatMessage::user(message),
        );
    }

    // Notify that the session has started processing
    state
        .ws_manager
        .emit_stream_event(
            &session_id,
            "stream.started",
            serde_json::json!({
                "session_id": session_id,
                "space_id": space_id,
            }),
        )
        .await;

    // NOTE: The actual agent loop invocation would happen here.
    // Since we cannot modify agent/ directory, we return the session info
    // and the caller (or a higher-level integration) would start the agent loop
    // and use ws_manager.emit_stream_event() to push deltas.

    Ok(serde_json::json!({
        "session_id": session_id,
        "space_id": space_id,
        "status": "processing",
    }))
}

/// Handle "chat.stop" — stop a running agent session
async fn handle_chat_stop(
    params: &serde_json::Value,
    state: &HttpServerState,
) -> Result<serde_json::Value, WsResponse> {
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_response(None, ERR_INVALID_PARAMS, "Missing 'session_id'"))?;

    // Emit a stop event so subscribers know
    state
        .ws_manager
        .emit_stream_event(
            session_id,
            "stream.stopped",
            serde_json::json!({ "session_id": session_id }),
        )
        .await;

    Ok(serde_json::json!({
        "session_id": session_id,
        "status": "stopped",
    }))
}

/// Handle "chat.approve" — approve a pending tool call
async fn handle_chat_approve(
    params: &serde_json::Value,
    state: &HttpServerState,
) -> Result<serde_json::Value, WsResponse> {
    let session_id = params
        .get("session_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_response(None, ERR_INVALID_PARAMS, "Missing 'session_id'"))?;

    let tool_call_id = params
        .get("tool_call_id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| err_response(None, ERR_INVALID_PARAMS, "Missing 'tool_call_id'"))?;

    let approved = params
        .get("approved")
        .and_then(|v| v.as_bool())
        .unwrap_or(true);

    // Notify subscribers about the approval decision
    state
        .ws_manager
        .emit_stream_event(
            session_id,
            "stream.approval_result",
            serde_json::json!({
                "session_id": session_id,
                "tool_call_id": tool_call_id,
                "approved": approved,
            }),
        )
        .await;

    Ok(serde_json::json!({
        "session_id": session_id,
        "tool_call_id": tool_call_id,
        "approved": approved,
    }))
}

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Validate channel name format: `session:{id}`, `space:{id}`, or `system`
fn is_valid_channel(channel: &str) -> bool {
    if channel == "system" {
        return true;
    }
    if let Some(id) = channel.strip_prefix("session:") {
        return !id.is_empty();
    }
    if let Some(id) = channel.strip_prefix("space:") {
        return !id.is_empty();
    }
    false
}

// ─── Background Housekeeping ─────────────────────────────────────────────────

/// Spawn a background task that periodically disconnects stale connections.
/// Call this once when the server starts.
pub fn spawn_stale_connection_reaper(ws_mgr: WsConnectionManager) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            ws_mgr
                .disconnect_stale(chrono::Duration::minutes(5))
                .await;
        }
    });
}
