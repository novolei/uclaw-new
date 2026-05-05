use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::api::auth::{HttpServerState, ApiErrorBody, extract_auth_user};

// ─── Request/Response Types ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct ConfigResponse {
    pub language: String,
    pub theme: String,
    pub data_path: String,
}

#[derive(Debug, Deserialize)]
pub struct PatchConfigRequest {
    pub language: Option<String>,
    pub theme: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderListItem {
    pub id: String,
    pub display_name: String,
    pub auth_type: String,
    pub default_base_url: String,
    pub supports_models: bool,
}

#[derive(Debug, Deserialize)]
pub struct AddProviderRequest {
    pub provider_id: String,
    pub display_name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct AddProviderResponse {
    pub provider_id: String,
    pub display_name: String,
    pub configured: bool,
}

// ─── Handlers ──────────────────────────────────────────────────────────

/// GET /api/config — get current configuration
pub async fn get_config(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    // Read config file from data_dir
    let config_path = state.data_dir.join("config.json");
    let config = if config_path.exists() {
        let content = tokio::fs::read_to_string(&config_path).await.map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
        })?;
        serde_json::from_str::<serde_json::Value>(&content).unwrap_or_default()
    } else {
        serde_json::Value::Object(Default::default())
    };

    Ok(Json(ConfigResponse {
        language: config.get("language").and_then(|v| v.as_str()).unwrap_or("en").to_string(),
        theme: config.get("theme").and_then(|v| v.as_str()).unwrap_or("system").to_string(),
        data_path: state.data_dir.to_string_lossy().to_string(),
    }))
}

/// PATCH /api/config — update configuration
pub async fn patch_config(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Json(req): Json<PatchConfigRequest>,
) -> Result<Json<ConfigResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let config_path = state.data_dir.join("config.json");

    // Load existing
    let mut config: serde_json::Value = if config_path.exists() {
        let content = tokio::fs::read_to_string(&config_path).await.unwrap_or_default();
        serde_json::from_str(&content).unwrap_or(serde_json::json!({}))
    } else {
        serde_json::json!({})
    };

    // Patch
    if let Some(lang) = &req.language {
        config["language"] = serde_json::Value::String(lang.clone());
    }
    if let Some(theme) = &req.theme {
        config["theme"] = serde_json::Value::String(theme.clone());
    }

    // Save
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;
    tokio::fs::write(&config_path, &serialized).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    Ok(Json(ConfigResponse {
        language: config.get("language").and_then(|v| v.as_str()).unwrap_or("en").to_string(),
        theme: config.get("theme").and_then(|v| v.as_str()).unwrap_or("system").to_string(),
        data_path: state.data_dir.to_string_lossy().to_string(),
    }))
}

/// GET /api/config/providers — list available LLM providers
pub async fn list_providers(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<ProviderListItem>>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let providers = crate::providers::registry::all();
    let items = providers.iter().map(|p| ProviderListItem {
        id: p.id.to_string(),
        display_name: p.display_name.to_string(),
        auth_type: format!("{:?}", p.auth_type).to_lowercase(),
        default_base_url: p.default_base_url.to_string(),
        supports_models: p.supports_models,
    }).collect();
    Ok(Json(items))
}

/// POST /api/config/providers — add/update a provider configuration
pub async fn add_provider(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Json(req): Json<AddProviderRequest>,
) -> Result<Json<AddProviderResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;

    // Validate provider_id to prevent path traversal attacks
    if !req.provider_id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
        tracing::warn!(provider_id = %req.provider_id, "Rejected invalid provider_id");
        return Err((StatusCode::BAD_REQUEST, Json(ApiErrorBody::new("invalid_input", "Invalid provider_id: only alphanumeric, underscore, and hyphen characters are allowed"))));
    }

    // Save provider config to data_dir/providers/{id}.json
    let providers_dir = state.data_dir.join("providers");
    tokio::fs::create_dir_all(&providers_dir).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    let config = serde_json::json!({
        "provider_id": req.provider_id,
        "display_name": req.display_name,
        "api_key": req.api_key,
        "base_url": req.base_url,
    });

    let file_path = providers_dir.join(format!("{}.json", req.provider_id));
    let serialized = serde_json::to_string_pretty(&config).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;
    tokio::fs::write(&file_path, &serialized).await.map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    tracing::info!("Provider configured: {}", req.provider_id);

    Ok(Json(AddProviderResponse {
        provider_id: req.provider_id,
        display_name: req.display_name,
        configured: true,
    }))
}
