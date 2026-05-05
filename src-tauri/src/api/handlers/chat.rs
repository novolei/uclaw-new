use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::api::auth::HttpServerState;

// ─── Request/Response Types ────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct CreateConversationRequest {
    pub title: Option<String>,
    pub space_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConversationResponse {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct SendMessageRequest {
    pub conversation_id: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct SendMessageResponse {
    pub message_id: String,
    pub conversation_id: String,
    pub response: String,
}

#[derive(Debug, Deserialize)]
pub struct GetMessagesQuery {
    pub conversation_id: String,
}

#[derive(Debug, Serialize)]
pub struct MessageResponse {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

// ─── Handlers ──────────────────────────────────────────────────────────

/// GET /api/conversations — list all conversations
pub async fn list_conversations(
    State(state): State<HttpServerState>,
) -> Result<Json<Vec<ConversationResponse>>, (StatusCode, Json<ApiError>)> {
    let session_mgr = state.session_manager.read().await;
    let conversations = session_mgr.list().into_iter().map(|s| ConversationResponse {
        id: s.id,
        space_id: s.space_id,
        title: s.title,
        message_count: s.message_count,
        created_at: s.created_at,
        updated_at: s.updated_at,
    }).collect();
    Ok(Json(conversations))
}

/// POST /api/conversations — create a new conversation
pub async fn create_conversation(
    State(state): State<HttpServerState>,
    Json(req): Json<CreateConversationRequest>,
) -> Result<Json<ConversationResponse>, (StatusCode, Json<ApiError>)> {
    let space_id = req.space_id.unwrap_or_else(|| "default".into());
    let title = req.title.unwrap_or_else(|| "New Chat".into());

    let summary = {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.create(&title, &space_id)
    };

    Ok(Json(ConversationResponse {
        id: summary.id,
        space_id: summary.space_id,
        title: summary.title,
        message_count: summary.message_count,
        created_at: summary.created_at,
        updated_at: summary.updated_at,
    }))
}

/// GET /api/messages?conversation_id=X — get messages for a conversation
pub async fn get_messages(
    State(state): State<HttpServerState>,
    axum::extract::Query(query): axum::extract::Query<GetMessagesQuery>,
) -> Result<Json<Vec<MessageResponse>>, (StatusCode, Json<ApiError>)> {
    let session_mgr = state.session_manager.read().await;
    if let Some(session) = session_mgr.get(&query.conversation_id) {
        let messages = session.messages.iter().enumerate().map(|(i, msg)| {
            let role = match msg.role {
                crate::agent::types::MessageRole::System => "system",
                crate::agent::types::MessageRole::User => "user",
                crate::agent::types::MessageRole::Assistant => "assistant",
            };
            let content = msg.content.iter()
                .filter_map(|b| {
                    if let crate::agent::types::ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            MessageResponse {
                id: format!("msg-{}", i),
                conversation_id: query.conversation_id.clone(),
                role: role.into(),
                content,
                created_at: chrono::Utc::now().to_rfc3339(),
            }
        }).collect();
        Ok(Json(messages))
    } else {
        Ok(Json(Vec::new()))
    }
}

/// DELETE /api/conversations/:id — delete a conversation
pub async fn delete_conversation(
    State(state): State<HttpServerState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<bool>, (StatusCode, Json<ApiError>)> {
    let mut session_mgr = state.session_manager.write().await;
    Ok(Json(session_mgr.delete(&id)))
}

/// POST /api/chat — send a message (non-streaming response for REST)
pub async fn send_message(
    State(state): State<HttpServerState>,
    Json(req): Json<SendMessageRequest>,
) -> Result<Json<SendMessageResponse>, (StatusCode, Json<ApiError>)> {
    // Add user message to session
    {
        let mut session_mgr = state.session_manager.write().await;
        session_mgr.add_message(
            &req.conversation_id,
            crate::agent::types::ChatMessage::user(&req.content),
        );
    }
    // Return a simple acknowledgment — actual AI response would require LLM config
    let message_id = Uuid::new_v4().to_string();
    Ok(Json(SendMessageResponse {
        message_id,
        conversation_id: req.conversation_id,
        response: "Message received. Use WebSocket for real-time AI responses.".into(),
    }))
}
