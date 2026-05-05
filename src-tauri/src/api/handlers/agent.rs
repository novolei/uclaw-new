use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
    response::sse::{Event, Sse},
};
use futures::stream::Stream;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use tokio_stream::wrappers::ReceiverStream;
use crate::api::auth::{HttpServerState, ApiErrorBody, extract_auth_user};
use crate::agent::types::*;

// ─── Request/Response Types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct ChatRequest {
    pub content: String,
    pub attachments: Option<Vec<String>>,
}

#[derive(Debug, Serialize)]
pub struct SessionResponse {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct SessionDetailResponse {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub messages: Vec<MessageItem>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
pub struct MessageItem {
    pub id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct ApproveRequest {
    pub tool_id: String,
    pub approved: bool,
}

// ─── SSE Chat Handler ──────────────────────────────────────────────────

/// POST /api/spaces/:id/chat — send message, return SSE stream
pub async fn chat_stream(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Path(space_id): Path<String>,
    Json(req): Json<ChatRequest>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    // Find or create a session for this space
    let conversation_id = {
        let mut session_mgr = state.session_manager.write().await;
        // Check if there's an active session for this space
        let existing = session_mgr.list().into_iter()
            .find(|s| s.space_id == space_id);

        match existing {
            Some(s) => s.id,
            None => {
                let summary = session_mgr.create("New Chat", &space_id);
                summary.id
            }
        }
    };

    // Add user message
    {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.add_message(&conversation_id, ChatMessage::user(&req.content));
    }

    // Create SSE stream
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(32);

    // Spawn async task to process the agent response
    let state_clone = state.clone();
    let content = req.content.clone();
    tokio::spawn(async move {
        // Send text_delta events simulating the agent response
        // In a full implementation, this would connect to the LLM provider
        let _ = tx.send(Ok(
            Event::default()
                .event("text_delta")
                .data(serde_json::json!({"content": format!("Received: {}", content)}).to_string())
        )).await;

        // Simulate processing
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Save assistant message
        {
            let mut session_mgr = state_clone.session_manager.write().await;
            session_mgr.add_message(
                &conversation_id,
                ChatMessage::assistant(&format!("Received: {}. Use WebSocket for full AI responses.", content)),
            );
        }

        // Send done event
        let _ = tx.send(Ok(
            Event::default()
                .event("done")
                .data(serde_json::json!({"usage": {"input_tokens": 0, "output_tokens": 0}}).to_string())
        )).await;
    });

    let stream = ReceiverStream::new(rx);
    Ok(Sse::new(stream))
}

// ─── Session Handlers ──────────────────────────────────────────────────

/// GET /api/spaces/:id/sessions — list sessions for a space
pub async fn list_sessions(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(space_id): Path<String>,
) -> Result<Json<Vec<SessionResponse>>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let session_mgr = state.session_manager.read().await;
    let sessions: Vec<SessionResponse> = session_mgr.list()
        .into_iter()
        .filter(|s| s.space_id == space_id)
        .map(|s| SessionResponse {
            id: s.id,
            space_id: s.space_id,
            title: s.title,
            message_count: s.message_count,
            created_at: s.created_at,
            updated_at: s.updated_at,
        })
        .collect();
    Ok(Json(sessions))
}

/// GET /api/sessions/:id — get session detail
pub async fn get_session(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<SessionDetailResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let session_mgr = state.session_manager.read().await;
    let session = session_mgr.get(&id).ok_or_else(|| {
        (StatusCode::NOT_FOUND, Json(ApiErrorBody::new("not_found", format!("Session '{}' not found", id))))
    })?;

    let messages: Vec<MessageItem> = session.messages.iter().enumerate().map(|(i, msg)| {
        let role = match msg.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        };
        let content = msg.content.iter()
            .filter_map(|b| {
                if let ContentBlock::Text { text } = b {
                    Some(text.clone())
                } else {
                    None
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        MessageItem {
            id: format!("msg-{}", i),
            role: role.into(),
            content,
            created_at: session.updated_at.clone(),
        }
    }).collect();

    Ok(Json(SessionDetailResponse {
        id: session.id.clone(),
        space_id: session.space_id.clone(),
        title: session.title.clone(),
        messages,
        created_at: session.created_at.clone(),
        updated_at: session.updated_at.clone(),
    }))
}

/// DELETE /api/sessions/:id — delete a session
pub async fn delete_session(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let mut session_mgr = state.session_manager.write().await;
    let deleted = session_mgr.delete(&id);
    if deleted {
        Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
    } else {
        Err((StatusCode::NOT_FOUND, Json(ApiErrorBody::new("not_found", format!("Session '{}' not found", id)))))
    }
}

/// POST /api/sessions/:id/stop — stop an ongoing agent loop
pub async fn stop_session(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    // In a full implementation, this would signal the running agent loop to stop
    tracing::info!("Stop requested for session: {}", id);
    Ok(Json(serde_json::json!({ "stopped": true, "session_id": id })))
}

/// POST /api/sessions/:id/approve — approve a tool call
pub async fn approve_tool_call(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Path(id): Path<String>,
    Json(req): Json<ApproveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    // In a full implementation, this would send approval to the running agent loop
    tracing::info!("Tool approval for session {}: tool_id={}, approved={}", id, req.tool_id, req.approved);
    Ok(Json(serde_json::json!({
        "session_id": id,
        "tool_id": req.tool_id,
        "approved": req.approved,
    })))
}
