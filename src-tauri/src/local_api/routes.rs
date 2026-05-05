use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ─── API 共享状态 ─────────────────────────────────────────────────────

/// API 共享状态
/// 注意：实际的 ServiceManager、MemUClient 等引用在 Task 11 集成时添加
/// 目前先用 placeholder
pub struct ApiState {
    /// 服务启动时间，用于计算 uptime
    pub start_time: std::time::Instant,
}

// ─── 路由创建 ─────────────────────────────────────────────────────────

/// 创建所有 API 路由
///
/// 路由结构：
/// - GET  /api/health             — 健康检查
/// - GET  /api/v1/status          — 应用状态
/// - GET  /api/v1/services        — 所有服务健康信息
/// - POST /api/v1/memory/retrieve — 记忆检索
/// - POST /api/v1/memory/memorize — 记忆提取（存入）
/// - GET  /api/v1/memory/categories — 记忆分类列表
/// - POST /api/v1/invoke          — 调用自定义 action
pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/v1/status", get(status))
        .route("/api/v1/services", get(services_status))
        .route("/api/v1/memory/retrieve", post(memory_retrieve))
        .route("/api/v1/memory/memorize", post(memory_memorize))
        .route("/api/v1/memory/categories", get(memory_categories))
        .route("/api/v1/invoke", post(invoke_action))
        .with_state(state)
}

// ===== 健康检查 =====

#[derive(Serialize)]
struct HealthResponse {
    status: String,
    version: String,
    uptime_secs: u64,
}

/// GET /api/health
/// 返回服务健康状态、版本号和运行时长
async fn health(State(state): State<Arc<ApiState>>) -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "healthy".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
    })
}

// ===== 应用状态 =====

#[derive(Serialize)]
struct StatusResponse {
    status: String,
    uptime_secs: u64,
    services: serde_json::Value,
}

/// GET /api/v1/status
/// 返回应用运行状态概览
async fn status(State(state): State<Arc<ApiState>>) -> Json<StatusResponse> {
    Json(StatusResponse {
        status: "running".to_string(),
        uptime_secs: state.start_time.elapsed().as_secs(),
        services: serde_json::json!({}), // placeholder，Task 11 填充
    })
}

// ===== 服务健康信息 =====

/// GET /api/v1/services
/// 返回所有受管服务的健康摘要
async fn services_status(State(_state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    // placeholder — Task 11 集成 ServiceManager 后填充实际数据
    Json(serde_json::json!({
        "total": 0,
        "running": 0,
        "services": []
    }))
}

// ===== 记忆检索 =====

#[derive(Deserialize)]
#[allow(dead_code)]
struct RetrieveRequest {
    query: String,
    #[serde(default)]
    limit: Option<usize>,
}

/// POST /api/v1/memory/retrieve
/// 根据查询语句检索相关记忆
async fn memory_retrieve(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<RetrieveRequest>,
) -> Json<serde_json::Value> {
    // placeholder — Task 11 集成 MemUClient 后填充实际数据
    tracing::info!("[LocalAPI] 记忆检索请求: {}", req.query);
    Json(serde_json::json!({
        "items": [],
        "query": req.query
    }))
}

// ===== 记忆提取（存入） =====

#[derive(Deserialize)]
#[allow(dead_code)]
struct MemorizeRequest {
    content: String,
    #[serde(default = "default_modality")]
    modality: String,
}

fn default_modality() -> String {
    "text".to_string()
}

/// POST /api/v1/memory/memorize
/// 将内容提取为记忆并持久化
async fn memory_memorize(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<MemorizeRequest>,
) -> Json<serde_json::Value> {
    // placeholder — Task 11 集成后填充实际逻辑
    tracing::info!(
        "[LocalAPI] 记忆提取请求: {}...",
        &req.content[..req.content.len().min(100)]
    );
    Json(serde_json::json!({
        "status": "accepted",
        "content_length": req.content.len()
    }))
}

// ===== 记忆分类列表 =====

/// GET /api/v1/memory/categories
/// 返回所有可用的记忆分类
async fn memory_categories(State(_state): State<Arc<ApiState>>) -> Json<serde_json::Value> {
    // placeholder — Task 11 填充实际分类数据
    Json(serde_json::json!({ "categories": [] }))
}

// ===== 调用自定义 action =====

#[derive(Deserialize)]
#[allow(dead_code)]
struct InvokeRequest {
    action: String,
    #[serde(default)]
    data: serde_json::Value,
}

#[derive(Serialize)]
struct InvokeResponse {
    success: bool,
    action: String,
    result: serde_json::Value,
}

/// POST /api/v1/invoke
/// 调用自定义 action（可扩展的通用调用端点）
async fn invoke_action(
    State(_state): State<Arc<ApiState>>,
    Json(req): Json<InvokeRequest>,
) -> (StatusCode, Json<InvokeResponse>) {
    tracing::info!("[LocalAPI] 调用 action: {}", req.action);
    // placeholder — Task 11 实现实际的 action dispatch
    (
        StatusCode::OK,
        Json(InvokeResponse {
            success: true,
            action: req.action,
            result: serde_json::json!({"message": "Action received"}),
        }),
    )
}
