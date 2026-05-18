use axum::{
    extract::State,
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

use crate::memu::client::MemUClient;

// ─── API 共享状态 ─────────────────────────────────────────────────────

/// API 共享状态
pub struct ApiState {
    /// 服务启动时间，用于计算 uptime
    pub start_time: std::time::Instant,
    /// memU bridge client (optional — None when pyembed isn't set up).
    /// Used by the OpenAI-compatible `/v1/embeddings` endpoint to expose
    /// the bundled FastEmbed model (bge-small-en-v1.5, 384 dim) to
    /// external tools such as gbrain (configured as a `llama-server`
    /// recipe pointing at localhost:27270/v1). See the Sprint 2.2
    /// followon hand-off for the gbrain config commands.
    pub memu_client: Option<Arc<MemUClient>>,
}

// ─── 路由创建 ─────────────────────────────────────────────────────────

/// 创建所有 API 路由
///
/// 路由结构：
/// - GET  /api/health                — 健康检查
/// - GET  /api/v1/status             — 应用状态
/// - GET  /api/v1/services           — 所有服务健康信息
/// - POST /api/v1/memory/retrieve    — 记忆检索
/// - POST /api/v1/memory/memorize    — 记忆提取（存入）
/// - GET  /api/v1/memory/categories  — 记忆分类列表
/// - POST /api/v1/invoke             — 调用自定义 action
/// - POST /v1/embeddings             — OpenAI-compatible embeddings
///   (Sprint 2.2 followon — lets gbrain reuse memU's bundled FastEmbed
///   via the `llama-server` recipe so put_page doesn't need an external
///   API key)
pub fn create_router(state: Arc<ApiState>) -> Router {
    Router::new()
        .route("/api/health", get(health))
        .route("/api/v1/status", get(status))
        .route("/api/v1/services", get(services_status))
        .route("/api/v1/memory/retrieve", post(memory_retrieve))
        .route("/api/v1/memory/memorize", post(memory_memorize))
        .route("/api/v1/memory/categories", get(memory_categories))
        .route("/api/v1/invoke", post(invoke_action))
        .route("/v1/embeddings", post(openai_embeddings))
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

// ===== OpenAI-compatible embeddings (Sprint 2.2 followon) =====
//
// Exposes the bundled memU FastEmbed model (BAAI/bge-small-en-v1.5, 384 dim)
// behind the OpenAI `/v1/embeddings` wire format so external tools — primarily
// gbrain via its `llama-server` recipe — can call uClaw's local API instead of
// requiring their own external embedding-provider API key.
//
// gbrain config (one-time, after this endpoint ships):
//   gbrain config set embedding_model llama-server:bge-small-en-v1.5
//   gbrain config set embedding_dimensions 384
//   gbrain config set base_urls.llama-server http://localhost:27270/v1
//
// Trade-off: gbrain's default expected model is OpenAI text-embedding-3-large
// (3072 dim, English-first). The bundled FastEmbed model is English-focused
// so Chinese-content semantic recall will be lower-quality than a multilingual
// model. Users who want multilingual recall can either:
//   a) Switch memU's bridge to a multilingual model (FASTEMBED_MODEL=bge-m3) —
//      both memU and the gbrain endpoint then use the same multilingual model.
//   b) Configure gbrain to use a different external provider with their own
//      API key — the /v1/embeddings endpoint becomes unused but still present.
//   c) Disable embedding in gbrain entirely (unset embedding_model in gbrain
//      config) — put_page will then use keyword-only indexing without
//      semantic vectors.

/// OpenAI input field. Per the spec the request `input` can be either a
/// single string or an array of strings; both shapes are supported.
#[derive(Deserialize)]
#[serde(untagged)]
enum EmbeddingsInput {
    Single(String),
    Batch(Vec<String>),
}

#[derive(Deserialize)]
struct EmbeddingsRequest {
    input: EmbeddingsInput,
    /// Model identifier from the client. We accept any string for
    /// compatibility (gbrain's `llama-server:<name>` form will appear
    /// here as `bge-small-en-v1.5`) — uClaw always serves whatever
    /// FastEmbed model memU's bridge currently has loaded. The model
    /// name is echoed in the response so the client can confirm what
    /// it asked for, but we do NOT validate it server-side.
    #[serde(default)]
    model: Option<String>,
    /// Optional encoding format. OpenAI clients may send `"float"` or
    /// `"base64"`; we only support `"float"` (the default). Receiving
    /// `"base64"` returns an error so clients fail loud rather than
    /// silently misinterpreting bytes.
    #[serde(default)]
    encoding_format: Option<String>,
}

#[derive(Serialize)]
struct EmbeddingObject {
    object: &'static str, // always "embedding"
    embedding: Vec<f32>,
    index: usize,
}

#[derive(Serialize)]
struct EmbeddingsUsage {
    prompt_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct EmbeddingsResponse {
    object: &'static str, // always "list"
    data: Vec<EmbeddingObject>,
    model: String,
    usage: EmbeddingsUsage,
}

#[derive(Serialize)]
struct OpenAIErrorBody {
    error: OpenAIErrorPayload,
}

#[derive(Serialize)]
struct OpenAIErrorPayload {
    message: String,
    #[serde(rename = "type")]
    error_type: &'static str,
    code: Option<&'static str>,
}

fn openai_error(
    status: StatusCode,
    message: impl Into<String>,
    error_type: &'static str,
    code: Option<&'static str>,
) -> (StatusCode, Json<OpenAIErrorBody>) {
    (
        status,
        Json(OpenAIErrorBody {
            error: OpenAIErrorPayload {
                message: message.into(),
                error_type,
                code,
            },
        }),
    )
}

/// POST /v1/embeddings
///
/// OpenAI-compatible endpoint backed by memU's bundled FastEmbed model.
/// Translates `input → texts`, calls `MemUClient::embed_text` (which
/// auto-spawns the Python bridge on first call), and translates
/// `vectors → data[{embedding, index}]`.
///
/// Failure modes:
/// - 503 if memU client isn't configured (pyembed missing on host)
/// - 400 if `encoding_format` is `"base64"` (unsupported)
/// - 400 if `input` is empty
/// - 500 if the bridge call fails (Python error, fastembed missing, etc.)
async fn openai_embeddings(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<EmbeddingsRequest>,
) -> Result<Json<EmbeddingsResponse>, (StatusCode, Json<OpenAIErrorBody>)> {
    // Reject unsupported encoding_format early so clients don't get
    // back floats when they asked for base64.
    if let Some(fmt) = req.encoding_format.as_deref() {
        if fmt != "float" {
            return Err(openai_error(
                StatusCode::BAD_REQUEST,
                format!(
                    "encoding_format='{}' not supported; only 'float' is implemented",
                    fmt
                ),
                "invalid_request_error",
                Some("unsupported_encoding_format"),
            ));
        }
    }

    let texts: Vec<String> = match req.input {
        EmbeddingsInput::Single(s) => vec![s],
        EmbeddingsInput::Batch(v) => v,
    };

    if texts.is_empty() {
        return Err(openai_error(
            StatusCode::BAD_REQUEST,
            "input must contain at least one string",
            "invalid_request_error",
            Some("empty_input"),
        ));
    }

    let client = state.memu_client.as_ref().ok_or_else(|| {
        openai_error(
            StatusCode::SERVICE_UNAVAILABLE,
            "memU bridge is not configured on this host \
             (pyembed missing — run scripts/setup-python-env.sh)",
            "server_error",
            Some("memu_unavailable"),
        )
    })?;

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let total_chars: usize = texts.iter().map(|s| s.len()).sum();

    let vectors = client.embed_text(&text_refs).await.map_err(|e| {
        openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("memU embed_text failed: {}", e),
            "server_error",
            Some("embed_failed"),
        )
    })?;

    if vectors.len() != texts.len() {
        return Err(openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "memU returned {} vectors for {} inputs",
                vectors.len(),
                texts.len()
            ),
            "server_error",
            Some("vector_count_mismatch"),
        ));
    }

    let data: Vec<EmbeddingObject> = vectors
        .into_iter()
        .enumerate()
        .map(|(index, embedding)| EmbeddingObject {
            object: "embedding",
            embedding,
            index,
        })
        .collect();

    // OpenAI's usage is token-based; we don't tokenize here, so approximate
    // via char-count / 4 (the common rule of thumb for English text). This
    // is informational only — gbrain doesn't rely on the value for billing.
    let approx_tokens: u32 = ((total_chars / 4).max(1)) as u32;

    Ok(Json(EmbeddingsResponse {
        object: "list",
        data,
        model: req
            .model
            .unwrap_or_else(|| "bge-small-en-v1.5".to_string()),
        usage: EmbeddingsUsage {
            prompt_tokens: approx_tokens,
            total_tokens: approx_tokens,
        },
    }))
}

#[cfg(test)]
mod openai_embeddings_tests {
    use super::*;
    use axum::extract::State;

    fn state_without_memu() -> Arc<ApiState> {
        Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            memu_client: None,
        })
    }

    #[tokio::test]
    async fn returns_503_when_memu_client_is_none() {
        let state = state_without_memu();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Single("hello".to_string()),
            model: None,
            encoding_format: None,
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        let err = result.err().expect("expected error when memU is None");
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(err.1.error.error_type, "server_error");
        assert_eq!(err.1.error.code, Some("memu_unavailable"));
    }

    #[tokio::test]
    async fn rejects_empty_batch_input_with_400() {
        let state = state_without_memu();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Batch(vec![]),
            model: None,
            encoding_format: None,
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        let err = result.err().expect("expected error for empty batch");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1.error.code, Some("empty_input"));
    }

    #[tokio::test]
    async fn rejects_base64_encoding_format_with_400() {
        let state = state_without_memu();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Single("hi".to_string()),
            model: None,
            encoding_format: Some("base64".to_string()),
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        let err = result.err().expect("expected error for base64 encoding_format");
        assert_eq!(err.0, StatusCode::BAD_REQUEST);
        assert_eq!(err.1.error.code, Some("unsupported_encoding_format"));
    }

    #[tokio::test]
    async fn accepts_float_encoding_format() {
        // encoding_format='float' is allowed; we still fail at the
        // memu_client check (state has no memU), so we expect 503 — but
        // crucially NOT 400 from the encoding_format guard. This pins
        // that 'float' bypasses the format reject.
        let state = state_without_memu();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Single("hi".to_string()),
            model: None,
            encoding_format: Some("float".to_string()),
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        let err = result.err().expect("expected 503 since memU is None");
        assert_eq!(err.0, StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn request_deserializes_both_input_shapes() {
        // Single string
        let single: EmbeddingsRequest =
            serde_json::from_str(r#"{"input":"hello","model":"x"}"#).unwrap();
        match single.input {
            EmbeddingsInput::Single(s) => assert_eq!(s, "hello"),
            _ => panic!("expected Single variant"),
        }
        // Array of strings
        let batch: EmbeddingsRequest =
            serde_json::from_str(r#"{"input":["a","b"],"model":"x"}"#).unwrap();
        match batch.input {
            EmbeddingsInput::Batch(v) => assert_eq!(v, vec!["a".to_string(), "b".to_string()]),
            _ => panic!("expected Batch variant"),
        }
    }
}
