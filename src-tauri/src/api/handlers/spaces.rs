use axum::{
    extract::{Json, Path, State},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::api::auth::{HttpServerState, ApiErrorBody, extract_auth_user};

// ─── Request/Response Types ────────────────────────────────────────────

#[derive(Debug, Serialize)]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub description: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub icon: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
}

// ─── Handlers ──────────────────────────────────────────────────────────

/// GET /api/spaces — list all spaces
pub async fn list_spaces(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<Vec<SpaceResponse>>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let session_mgr = state.session_manager.read().await;
    let db = session_mgr.db();
    let conn = db.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    // Try to query spaces table; if it doesn't exist yet return default
    let mut stmt = match conn.prepare("SELECT id, name, icon, description, created_at, updated_at FROM spaces ORDER BY created_at DESC") {
        Ok(s) => s,
        Err(_) => {
            // Table doesn't exist, return default space
            return Ok(Json(vec![SpaceResponse {
                id: "default".into(),
                name: "Default".into(),
                icon: "📁".into(),
                description: None,
                created_at: chrono::Utc::now().to_rfc3339(),
                updated_at: chrono::Utc::now().to_rfc3339(),
            }]));
        }
    };

    let spaces = stmt.query_map([], |row| {
        Ok(SpaceResponse {
            id: row.get(0)?,
            name: row.get(1)?,
            icon: row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
            description: row.get(3).ok(),
            created_at: row.get(4)?,
            updated_at: row.get(5)?,
        })
    }).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    let mut results = Vec::new();
    for space in spaces {
        if let Ok(s) = space {
            results.push(s);
        }
    }

    // Always include default space if empty
    if results.is_empty() {
        results.push(SpaceResponse {
            id: "default".into(),
            name: "Default".into(),
            icon: "📁".into(),
            description: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        });
    }

    Ok(Json(results))
}

/// POST /api/spaces — create a new space
pub async fn create_space(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Json(req): Json<CreateSpaceRequest>,
) -> Result<Json<SpaceResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let id = uuid::Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();
    let icon = req.icon.unwrap_or_else(|| "📁".into());

    {
        let session_mgr = state.session_manager.read().await;
        let db = session_mgr.db();
        let conn = db.lock().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
        })?;

        // Ensure spaces table exists
        conn.execute(
            "CREATE TABLE IF NOT EXISTS spaces (id TEXT PRIMARY KEY, name TEXT NOT NULL, icon TEXT DEFAULT '📁', description TEXT, created_at TEXT NOT NULL, updated_at TEXT NOT NULL)",
            [],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
        })?;

        conn.execute(
            "INSERT INTO spaces (id, name, icon, description, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![id, req.name, icon, req.description, now, now],
        ).map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
        })?;
    }

    // Create workspace directory for the space
    let space_dir = state.data_dir.join("spaces").join(&id).join("workspace");
    let _ = tokio::fs::create_dir_all(&space_dir).await;

    tracing::info!("Space created: {} ({})", req.name, id);

    Ok(Json(SpaceResponse {
        id,
        name: req.name,
        icon,
        description: req.description,
        created_at: now.clone(),
        updated_at: now,
    }))
}

/// GET /api/spaces/:id — get space details
pub async fn get_space(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<SpaceResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    if id == "default" {
        return Ok(Json(SpaceResponse {
            id: "default".into(),
            name: "Default".into(),
            icon: "📁".into(),
            description: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: chrono::Utc::now().to_rfc3339(),
        }));
    }

    let session_mgr = state.session_manager.read().await;
    let db = session_mgr.db();
    let conn = db.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    let space = conn.query_row(
        "SELECT id, name, icon, description, created_at, updated_at FROM spaces WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(SpaceResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get::<_, String>(2).unwrap_or_else(|_| "📁".into()),
                description: row.get(3).ok(),
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    ).map_err(|_| {
        (StatusCode::NOT_FOUND, Json(ApiErrorBody::new("not_found", format!("Space '{}' not found", id))))
    })?;

    Ok(Json(space))
}

/// PATCH /api/spaces/:id — update a space
pub async fn update_space(
    headers: axum::http::HeaderMap,
    State(state): State<HttpServerState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateSpaceRequest>,
) -> Result<Json<SpaceResponse>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    let now = chrono::Utc::now().to_rfc3339();

    {
        let session_mgr = state.session_manager.read().await;
        let db = session_mgr.db();
        let conn = db.lock().map_err(|e| {
            (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
        })?;

        if let Some(name) = &req.name {
            conn.execute(
                "UPDATE spaces SET name = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![name, now, id],
            ).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
            })?;
        }
        if let Some(icon) = &req.icon {
            conn.execute(
                "UPDATE spaces SET icon = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![icon, now, id],
            ).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
            })?;
        }
        if let Some(desc) = &req.description {
            conn.execute(
                "UPDATE spaces SET description = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![desc, now, id],
            ).map_err(|e| {
                (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
            })?;
        }
    }

    // Re-fetch the updated space
    let session_mgr2 = state.session_manager.read().await;
    let db2 = session_mgr2.db();
    let conn2 = db2.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    let space = conn2.query_row(
        "SELECT id, name, icon, description, created_at, updated_at FROM spaces WHERE id = ?1",
        rusqlite::params![id],
        |row| {
            Ok(SpaceResponse {
                id: row.get(0)?,
                name: row.get(1)?,
                icon: row.get::<_, String>(2).unwrap_or_else(|_| "\u{1f4c1}".into()),
                description: row.get(3).ok(),
                created_at: row.get(4)?,
                updated_at: row.get(5)?,
            })
        },
    ).map_err(|_| {
        (StatusCode::NOT_FOUND, Json(ApiErrorBody::new("not_found", format!("Space '{}' not found", id))))
    })?;

    Ok(Json(space))
}

/// DELETE /api/spaces/:id — delete a space
pub async fn delete_space(
    State(state): State<HttpServerState>,
    headers: axum::http::HeaderMap,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiErrorBody>)> {
    let _user = extract_auth_user(&headers, &state.jwt_secret)?;
    if id == "default" {
        return Err((StatusCode::BAD_REQUEST, Json(ApiErrorBody::new("bad_request", "Cannot delete default space"))));
    }

    let session_mgr = state.session_manager.read().await;
    let db = session_mgr.db();
    let conn = db.lock().map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    conn.execute("DELETE FROM spaces WHERE id = ?1", rusqlite::params![id]).map_err(|e| {
        (StatusCode::INTERNAL_SERVER_ERROR, Json(ApiErrorBody::new("internal", e.to_string())))
    })?;

    // Also delete associated conversations
    let _ = conn.execute("DELETE FROM conversations WHERE space_id = ?1", rusqlite::params![id]);

    tracing::info!("Space deleted: {}", id);

    Ok(Json(serde_json::json!({ "deleted": true, "id": id })))
}
