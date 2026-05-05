use axum::{
    http::{StatusCode, header},
    response::{IntoResponse, Response},
    Json,
};
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::RwLock;
use crate::agent::session::SessionManager;

/// JWT claims for API authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject: user ID
    pub sub: String,
    /// Device ID for device-scoped tokens
    pub device_id: String,
    /// Issued at (Unix timestamp)
    pub iat: usize,
    /// Expiration (Unix timestamp)
    pub exp: usize,
}

/// Shared state for the HTTP server
#[derive(Clone)]
pub struct HttpServerState {
    pub session_manager: Arc<RwLock<SessionManager>>,
    pub jwt_secret: String,
    pub data_dir: std::path::PathBuf,
    pub ws_manager: crate::api::ws::WsConnectionManager,
}

/// Authenticated user extracted from JWT token.
/// Use as an Axum extractor on protected routes.
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub device_id: String,
}

/// Unified API error response
#[derive(Debug, Serialize)]
pub struct ApiErrorBody {
    pub error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
pub struct ApiErrorDetail {
    pub code: String,
    pub message: String,
}

impl ApiErrorBody {
    pub fn new(code: &str, message: impl Into<String>) -> Self {
        Self {
            error: ApiErrorDetail {
                code: code.to_string(),
                message: message.into(),
            },
        }
    }
}

impl IntoResponse for ApiErrorBody {
    fn into_response(self) -> Response {
        let status = match self.error.code.as_str() {
            "unauthorized" => StatusCode::UNAUTHORIZED,
            "forbidden" => StatusCode::FORBIDDEN,
            "not_found" => StatusCode::NOT_FOUND,
            "invalid_input" | "bad_request" => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(self)).into_response()
    }
}

/// Extract AuthUser from request headers (call manually in handlers)
pub fn extract_auth_user(
    headers: &axum::http::HeaderMap,
    jwt_secret: &str,
) -> Result<AuthUser, (StatusCode, Json<ApiErrorBody>)> {
    let auth_header = headers
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(extract_bearer_token);

    let token = auth_header.ok_or_else(|| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiErrorBody::new("unauthorized", "Missing Bearer token")),
        )
    })?;

    let claims = verify_token(token, jwt_secret).map_err(|_| {
        (
            StatusCode::UNAUTHORIZED,
            Json(ApiErrorBody::new("unauthorized", "Invalid or expired token")),
        )
    })?;

    Ok(AuthUser {
        user_id: claims.sub,
        device_id: claims.device_id,
    })
}

/// Generate a JWT token for a user/device
pub fn create_token(user_id: &str, device_id: &str, secret: &str) -> Result<String, crate::error::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let claims = Claims {
        sub: user_id.to_string(),
        device_id: device_id.to_string(),
        iat: now,
        exp: now + 86400 * 30, // 30 days
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| crate::error::Error::Internal(format!("JWT encode: {}", e)))
}

/// Generate a token with custom expiry (in seconds)
pub fn generate_token(user_id: &str, device_id: &str, secret: &str, expiry_secs: usize) -> Result<String, crate::error::Error> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;

    let claims = Claims {
        sub: user_id.to_string(),
        device_id: device_id.to_string(),
        iat: now,
        exp: now + expiry_secs,
    };

    encode(&Header::default(), &claims, &EncodingKey::from_secret(secret.as_bytes()))
        .map_err(|e| crate::error::Error::Internal(format!("JWT encode: {}", e)))
}

/// Validate and decode a JWT token
pub fn verify_token(token: &str, secret: &str) -> Result<Claims, crate::error::Error> {
    let token_data = decode::<Claims>(
        token,
        &DecodingKey::from_secret(secret.as_bytes()),
        &Validation::default(),
    )
    .map_err(|e| crate::error::Error::InvalidInput(format!("Invalid token: {}", e)))?;

    Ok(token_data.claims)
}

/// Refresh an existing (valid) token with a new expiry
pub fn refresh_token(old_token: &str, secret: &str) -> Result<String, crate::error::Error> {
    let claims = verify_token(old_token, secret)?;
    // Issue a new token with 30-day expiry
    create_token(&claims.sub, &claims.device_id, secret)
}

/// Generate a random JWT secret (256-bit, hex-encoded)
pub fn generate_secret() -> String {
    use rand::Rng;
    let bytes: [u8; 32] = rand::thread_rng().r#gen();
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Extract bearer token from Authorization header value
pub fn extract_bearer_token(header_value: &str) -> Option<&str> {
    header_value.strip_prefix("Bearer ")
}
