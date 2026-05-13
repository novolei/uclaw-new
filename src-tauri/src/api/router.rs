use axum::{
    Router,
    routing::{get, post, patch, delete},
    http::StatusCode,
    Json,
    extract::Path,
    body::Bytes,
};
use tower_http::cors::{CorsLayer, Any};
use tower_http::trace::TraceLayer;
use tower_http::catch_panic::CatchPanicLayer;

use crate::api::auth::{HttpServerState, ApiErrorBody};
use crate::automation::sources::webhook::{global_registry, verify_signature};

/// Build the main API router with all routes and middleware
pub fn build_router(state: HttpServerState) -> Router {
    // ─── Middleware Stack ────────────────────────────────────────────────
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    let trace = TraceLayer::new_for_http();

    // ─── Public routes (no auth required) ────────────────────────────────
    let auth_routes = Router::new()
        .route("/auth/pair", post(super::handlers::auth::pair_device))
        .route("/auth/status", get(super::handlers::auth::auth_status))
        .route("/auth/refresh", post(refresh_token_handler));

    // ─── Protected routes ────────────────────────────────────────────────

    // Chat (legacy)
    let chat_routes = Router::new()
        .route("/conversations", get(super::handlers::chat::list_conversations))
        .route("/conversations", post(super::handlers::chat::create_conversation))
        .route("/conversations/{id}", delete(super::handlers::chat::delete_conversation))
        .route("/messages", get(super::handlers::chat::get_messages))
        .route("/chat", post(super::handlers::chat::send_message));

    // Artifacts
    let artifact_routes = Router::new()
        .route("/artifacts", get(super::handlers::artifacts::list_artifacts))
        .route("/artifacts/{path}", get(super::handlers::artifacts::read_artifact))
        .route("/artifacts", post(super::handlers::artifacts::write_artifact))
        .route("/artifacts/{path}", delete(super::handlers::artifacts::delete_artifact));

    // Config
    let config_routes = Router::new()
        .route("/config", get(super::handlers::config::get_config))
        .route("/config", patch(super::handlers::config::patch_config))
        .route("/config/providers", get(super::handlers::config::list_providers))
        .route("/config/providers", post(super::handlers::config::add_provider));

    // Spaces
    let space_routes = Router::new()
        .route("/spaces", get(super::handlers::spaces::list_spaces))
        .route("/spaces", post(super::handlers::spaces::create_space))
        .route("/spaces/{id}", get(super::handlers::spaces::get_space))
        .route("/spaces/{id}", patch(super::handlers::spaces::update_space))
        .route("/spaces/{id}", delete(super::handlers::spaces::delete_space));

    // Agent/Chat with SSE
    let agent_routes = Router::new()
        .route("/spaces/{id}/chat", post(super::handlers::agent::chat_stream))
        .route("/spaces/{id}/sessions", get(super::handlers::agent::list_sessions))
        .route("/sessions/{id}", get(super::handlers::agent::get_session))
        .route("/sessions/{id}", delete(super::handlers::agent::delete_session))
        .route("/sessions/{id}/stop", post(super::handlers::agent::stop_session))
        .route("/sessions/{id}/approve", post(super::handlers::agent::approve_tool_call));

    // Auth (protected)
    let auth_protected = Router::new()
        .route("/auth/token", post(super::handlers::auth::create_api_token));

    // WebSocket route for real-time streaming
    let ws_routes = Router::new()
        .route("/ws", get(super::ws::ws_handler));

    // Automation webhook ingress (no auth — verified via HMAC-SHA256 signature)
    let automation_routes = Router::new()
        .route(
            "/automation/webhook/{spec_id}/{sub_id}/*tail",
            post(automation_webhook_handler),
        );

    // ─── Assemble Router ─────────────────────────────────────────────────
    Router::new()
        .route("/api/health", get(health_check))
        .nest("/api", auth_routes)
        .nest("/api", chat_routes)
        .nest("/api", artifact_routes)
        .nest("/api", config_routes)
        .nest("/api", space_routes)
        .nest("/api", agent_routes)
        .nest("/api", auth_protected)
        .nest("/api", ws_routes)
        .nest("/api", automation_routes)
        .layer(CatchPanicLayer::new())
        .layer(trace)
        .layer(cors)
        .with_state(state)
}

/// Health check endpoint
async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "name": "uClaw API",
    }))
}

/// POST /api/automation/webhook/:spec_id/:sub_id/*tail
///
/// Public (no JWT) but optionally HMAC-SHA256 verified.
/// Signature header: `X-Humane-Signature: sha256=<hex>` (optional).
/// Returns 404 if the (spec_id, sub_id) pair is not registered,
/// 401 if a secret is configured and the signature is absent/wrong.
async fn automation_webhook_handler(
    Path((spec_id, sub_id, _tail)): Path<(String, String, String)>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let registry = global_registry();
    let guard = registry.read().await;
    let entry = guard
        .get(&(spec_id.clone(), sub_id.clone()))
        .ok_or_else(|| {
            (
                StatusCode::NOT_FOUND,
                Json(ApiErrorBody::new("not_found", "webhook subscription not found")),
            )
        })?;

    // Verify signature when secret is set
    if let Some(secret) = &entry.secret {
        let sig_header = headers
            .get("x-humane-signature")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        if !verify_signature(secret, &body, sig_header) {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(ApiErrorBody::new("unauthorized", "invalid or missing signature")),
            ));
        }
    }

    // Parse body as JSON (fall back to raw string on failure)
    let payload: serde_json::Value = serde_json::from_slice(&body).unwrap_or_else(|_| {
        serde_json::json!({ "raw": String::from_utf8_lossy(&body).as_ref() })
    });

    let callback = entry.callback.clone();
    // Drop the read guard before calling the callback (may block)
    drop(guard);
    callback(spec_id, sub_id, payload);

    Ok(Json(serde_json::json!({ "ok": true })))
}

/// POST /api/auth/refresh — refresh a JWT token
async fn refresh_token_handler(
    axum::extract::State(state): axum::extract::State<HttpServerState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let auth_header = headers
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(crate::api::auth::extract_bearer_token)
        .ok_or_else(|| {
            (StatusCode::UNAUTHORIZED, Json(ApiErrorBody::new("unauthorized", "Missing Bearer token")))
        })?;

    let new_token = crate::api::auth::refresh_token(auth_header, &state.jwt_secret)
        .map_err(|e| {
            (StatusCode::UNAUTHORIZED, Json(ApiErrorBody::new("unauthorized", e.to_string())))
        })?;

    Ok(Json(serde_json::json!({
        "token": new_token,
        "expires_in": 86400 * 30,
    })))
}
