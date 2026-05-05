use axum::{
    extract::{Json, State},
    http::{StatusCode, header},
};
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use crate::api::auth::{HttpServerState, create_token, verify_token, extract_bearer_token};

#[derive(Debug, Deserialize)]
pub struct PairDeviceRequest {
    pub device_name: String,
}

#[derive(Debug, Serialize)]
pub struct PairDeviceResponse {
    pub user_id: String,
    pub device_id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateApiTokenRequest {
    pub label: Option<String>,
    pub expires_in_days: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct CreateApiTokenResponse {
    pub token: String,
    pub label: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub version: String,
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub device_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

/// POST /api/auth/pair — pair device, get JWT
pub async fn pair_device(
    State(state): State<HttpServerState>,
    Json(req): Json<PairDeviceRequest>,
) -> Result<Json<PairDeviceResponse>, (StatusCode, Json<ApiError>)> {
    let user_id = Uuid::new_v4().to_string();
    let device_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let expires_at = chrono::Utc::now()
        .checked_add_signed(chrono::Duration::days(30))
        .unwrap()
        .to_rfc3339();

    {
        let session_mgr = state.session_manager.read().await;
        let db = session_mgr.db();
        let conn = db.lock().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() }))
        })?;
        conn.execute(
            "INSERT OR REPLACE INTO users (id, device_name, device_id, created_at, last_accessed) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![user_id, req.device_name, device_id, now, now],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() }))
        })?;
    }

    let token = create_token(&user_id, &device_id, &state.jwt_secret)
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() })))?;

    Ok(Json(PairDeviceResponse { user_id, device_id, token, expires_at }))
}

/// GET /api/auth/status — check auth
pub async fn auth_status(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
) -> Result<Json<StatusResponse>, (StatusCode, Json<ApiError>)> {
    let auth_header = headers.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_bearer_token);

    match auth_header {
        Some(token) => {
            match verify_token(token, &state.jwt_secret) {
                Ok(claims) => Ok(Json(StatusResponse {
                    version: env!("CARGO_PKG_VERSION").into(),
                    authenticated: true,
                    user_id: Some(claims.sub),
                    device_id: Some(claims.device_id),
                })),
                Err(_) => Ok(Json(StatusResponse {
                    version: env!("CARGO_PKG_VERSION").into(),
                    authenticated: false,
                    user_id: None,
                    device_id: None,
                })),
            }
        }
        None => Ok(Json(StatusResponse {
            version: env!("CARGO_PKG_VERSION").into(),
            authenticated: false,
            user_id: None,
            device_id: None,
        })),
    }
}

/// POST /api/auth/token — create API token
pub async fn create_api_token(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Json(req): Json<CreateApiTokenRequest>,
) -> Result<Json<CreateApiTokenResponse>, (StatusCode, Json<ApiError>)> {
    let auth_header = headers.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_bearer_token)
        .ok_or_else(|| (StatusCode::UNAUTHORIZED, Json(ApiError { error: "Missing authorization".into() })))?;

    let claims = verify_token(auth_header, &state.jwt_secret)
        .map_err(|_| (StatusCode::UNAUTHORIZED, Json(ApiError { error: "Invalid token".into() })))?;

    use sha2::{Sha256, Digest};

    let token = Uuid::new_v4().to_string();
    let label = req.label.unwrap_or_else(|| "API Token".into());

    let mut hasher = Sha256::new();
    hasher.update(token.as_bytes());
    let token_hash = format!("{:x}", hasher.finalize());

    let expires_at = req.expires_in_days.map(|days| {
        chrono::Utc::now()
            .checked_add_signed(chrono::Duration::days(days as i64))
            .unwrap()
            .to_rfc3339()
    });

    {
        let session_mgr = state.session_manager.read().await;
        let db = session_mgr.db();
        let conn = db.lock().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() }))
        })?;
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO api_tokens (id, user_id, token_hash, label, expires_at, created_at) VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))",
            rusqlite::params![id, claims.sub, token_hash, label, expires_at],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() }))
        })?;
    }

    Ok(Json(CreateApiTokenResponse { token, label, expires_at }))
}
