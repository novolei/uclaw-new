use axum::{
    extract::{Json, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::api::auth::HttpServerState;

#[derive(Debug, Serialize)]
pub struct ArtifactNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub children: Option<Vec<ArtifactNode>>,
}

#[derive(Debug, Deserialize)]
pub struct ReadArtifactRequest {
    pub path: String,
}

#[derive(Debug, Serialize)]
pub struct ArtifactContentResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
}

#[derive(Debug, Deserialize)]
pub struct WriteArtifactRequest {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ApiError {
    pub error: String,
}

async fn build_artifact_tree(root: &std::path::Path, base: &std::path::Path) -> Result<Vec<ArtifactNode>, std::io::Error> {
    let mut nodes = Vec::new();
    if !root.is_dir() {
        return Ok(nodes);
    }
    let mut entries = tokio::fs::read_dir(root).await?;
    while let Some(entry) = entries.next_entry().await? {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown");
        let relative = path.strip_prefix(base).unwrap_or(&path);

        if name.starts_with('.') || name == "node_modules" || name == "target" {
            continue;
        }

        if path.is_dir() {
            let children = Box::pin(build_artifact_tree(&path, base)).await?;
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: true,
                size: None,
                children: if children.is_empty() { None } else { Some(children) },
            });
        } else {
            let size = entry.metadata().await.map(|m| m.len()).ok();
            nodes.push(ArtifactNode {
                name: name.into(),
                path: relative.to_string_lossy().into(),
                is_dir: false,
                size,
                children: None,
            });
        }
    }
    nodes.sort_by(|a, b| {
        if a.is_dir != b.is_dir { b.is_dir.cmp(&a.is_dir) }
        else { a.name.to_lowercase().cmp(&b.name.to_lowercase()) }
    });
    Ok(nodes)
}

/// GET /api/artifacts — list workspace files
pub async fn list_artifacts(
    State(state): State<HttpServerState>,
) -> Result<Json<Vec<ArtifactNode>>, (StatusCode, Json<ApiError>)> {
    let workspace = state.data_dir.join("workspace");
    let tree = build_artifact_tree(&workspace, &workspace).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() })))?;
    Ok(Json(tree))
}

/// GET /api/artifacts/:path — read file content
pub async fn read_artifact(
    State(state): State<HttpServerState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Json<ArtifactContentResponse>, (StatusCode, Json<ApiError>)> {
    let workspace = state.data_dir.join("workspace");
    let full_path = workspace.join(&path);
    let content = tokio::fs::read_to_string(&full_path).await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(ApiError { error: e.to_string() })))?;
    let size = content.len() as u64;
    Ok(Json(ArtifactContentResponse { path, content, size }))
}

/// POST /api/artifacts — write/create a file
pub async fn write_artifact(
    State(state): State<HttpServerState>,
    Json(req): Json<WriteArtifactRequest>,
) -> Result<Json<ArtifactContentResponse>, (StatusCode, Json<ApiError>)> {
    let workspace = state.data_dir.join("workspace");
    let full_path = workspace.join(&req.path);
    if let Some(parent) = full_path.parent() {
        tokio::fs::create_dir_all(parent).await
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() })))?;
    }
    tokio::fs::write(&full_path, &req.content).await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiError { error: e.to_string() })))?;
    let size = req.content.len() as u64;
    Ok(Json(ArtifactContentResponse { path: req.path, content: req.content, size }))
}

/// DELETE /api/artifacts/:path — delete a file
pub async fn delete_artifact(
    State(state): State<HttpServerState>,
    axum::extract::Path(path): axum::extract::Path<String>,
) -> Result<Json<bool>, (StatusCode, Json<ApiError>)> {
    let workspace = state.data_dir.join("workspace");
    let full_path = workspace.join(&path);
    tokio::fs::remove_file(&full_path).await
        .map_err(|e| (StatusCode::NOT_FOUND, Json(ApiError { error: e.to_string() })))?;
    Ok(Json(true))
}
