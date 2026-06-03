use axum::{
    extract::State,
    http::StatusCode,
    response::{
        sse::{Event, Sse},
        IntoResponse, Json,
    },
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

// ─── API 共享状态 ─────────────────────────────────────────────────────

/// API 共享状态
pub struct ApiState {
    /// 服务启动时间，用于计算 uptime
    pub start_time: std::time::Instant,
    /// In-process embedder shared with the rest of uClaw (BucketSeal stack).
    /// Serves the `/v1/embeddings` OpenAI-compatible endpoint so external
    /// tools like gbrain continue to work without any Python bridge.
    pub embedder: Arc<dyn crate::memory_bucket_seal::Embedder>,
    /// In-process MiniCPM engine backing `/v1/chat/completions` (Slice B).
    pub local_llm: Arc<crate::local_llm::LocalLlmEngine>,
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
        .route("/v1/chat/completions", post(chat_completions))
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
    // placeholder — local_api memory routes are stubs (memory lives in bucket_seal/memory_graph)
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
// Exposes the in-process OnnxEmbedder (BAAI/bge-small-en-v1.5, 384 dim)
// behind the OpenAI `/v1/embeddings` wire format so external tools — primarily
// gbrain via its `llama-server` recipe — can call uClaw's local API instead of
// requiring their own external embedding-provider API key. No Python, no
// external process: embedding runs in-process via ort + tokenizers.
//
// gbrain config (one-time, after this endpoint ships):
//   gbrain config set embedding_model llama-server:bge-small-en-v1.5
//   gbrain config set embedding_dimensions 384
//   gbrain config set base_urls.llama-server http://localhost:27270/v1
//
// Trade-off: the bundled OnnxEmbedder model is English-focused
// so Chinese-content semantic recall will be lower-quality than a multilingual
// model. Users who want multilingual recall can either:
//   a) Configure gbrain to use a different external provider with their own
//      API key — the /v1/embeddings endpoint becomes unused but still present.
//   b) Disable embedding in gbrain entirely (unset embedding_model in gbrain
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

#[derive(Debug, Serialize)]
struct OpenAIErrorBody {
    error: OpenAIErrorPayload,
}

#[derive(Debug, Serialize)]
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
/// OpenAI-compatible endpoint backed by the shared in-process embedder
/// (BucketSeal stack, no Python bridge required).
/// Translates `input → texts`, calls `Embedder::embed_batch`, and
/// translates `vectors → data[{embedding, index}]`.
///
/// Failure modes:
/// - 400 if `encoding_format` is `"base64"` (unsupported)
/// - 400 if `input` is empty
/// - 500 if the embedder call fails
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

    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let total_chars: usize = texts.iter().map(|s| s.len()).sum();

    let vectors = state.embedder.embed_batch(&text_refs).await.map_err(|e| {
        openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("embed failed: {}", e),
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

// ===== OpenAI /v1/chat/completions (Slice B — local MiniCPM) =====

use crate::local_llm::chat_template::{render_chatml, render_chatml_no_think, ChatMessage};
use crate::local_llm::engine::GenParams;

#[derive(Debug, Deserialize)]
pub struct ChatMessageDto {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum StopField {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionsRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<ChatMessageDto>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub stop: Option<StopField>,
    /// MiniCPM5-1B is a reasoning model that emits a `<think>…</think>` block by
    /// default. The local route suppresses that (clean direct answers for the
    /// utility/summarizer scenarios this engine serves). Set `true` to opt back
    /// into chain-of-thought; defaults to off (no-think).
    #[serde(default)]
    pub enable_thinking: Option<bool>,
}

impl ChatCompletionsRequest {
    pub fn stop_strings(&self) -> Vec<String> {
        match &self.stop {
            None => Vec::new(),
            Some(StopField::One(s)) => vec![s.clone()],
            Some(StopField::Many(v)) => v.clone(),
        }
    }

    pub fn to_gen_params(&self) -> GenParams {
        let d = GenParams::default();
        GenParams {
            temperature: self.temperature.unwrap_or(d.temperature),
            top_p: self.top_p.or(d.top_p),
            top_k: self.top_k.or(d.top_k),
            max_tokens: self.max_tokens.unwrap_or(d.max_tokens),
            stop: self.stop_strings(),
            ..d
        }
    }

    fn prompt(&self) -> String {
        let msgs: Vec<ChatMessage> = self
            .messages
            .iter()
            .map(|m| ChatMessage { role: m.role.clone(), content: m.content.clone() })
            .collect();
        // Default to no-think (the prefilled empty `<think></think>` makes the
        // model generate the answer directly — no CoT block in the output).
        // Opt into reasoning only when the caller explicitly asks.
        if self.enable_thinking == Some(true) {
            render_chatml(&msgs)
        } else {
            render_chatml_no_think(&msgs)
        }
    }
}

#[derive(Serialize)]
struct RespMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct Choice {
    index: usize,
    message: RespMessage,
    finish_reason: String,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct ChatCompletionsResponse {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn response_model_name(req_model: &Option<String>) -> String {
    req_model.clone().unwrap_or_else(|| format!("local/{}", crate::local_llm::MODEL_ID))
}

/// Build the OpenAI 503 body for a not-ready local model.
fn not_ready_response(msg: String) -> (StatusCode, Json<OpenAIErrorBody>) {
    openai_error(
        StatusCode::SERVICE_UNAVAILABLE,
        msg,
        "server_error",
        Some("model_not_ready"),
    )
}

/// POST /v1/chat/completions — OpenAI-compatible, backed by the in-process
/// MiniCPM engine. Streams SSE when `stream=true`, else returns one JSON body.
/// Returns 503 `model_not_ready` when the model is unavailable so the role
/// router can fall back to the cloud active model.
async fn chat_completions(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ChatCompletionsRequest>,
) -> axum::response::Response {
    if req.stream {
        return chat_completions_stream(state, req).await;
    }

    let params = req.to_gen_params();
    let prompt = req.prompt();
    let model_name = response_model_name(&req.model);

    let buf = Arc::new(std::sync::Mutex::new(String::new()));
    let buf_w = buf.clone();
    let result = state
        .local_llm
        .generate(&prompt, &params, move |d| {
            buf_w.lock().unwrap().push_str(d);
        })
        .await;

    match result {
        Ok((reason, n_tokens)) => {
            let content = std::mem::take(&mut *buf.lock().unwrap());
            let resp = ChatCompletionsResponse {
                id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                object: "chat.completion",
                created: now_unix(),
                model: model_name,
                choices: vec![Choice {
                    index: 0,
                    message: RespMessage { role: "assistant", content },
                    finish_reason: reason.as_str().to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: n_tokens as u32,
                    total_tokens: n_tokens as u32,
                },
            };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(crate::local_llm::engine::EngineError::NotReady(m)) => {
            not_ready_response(format!("local model not ready: {m}")).into_response()
        }
        Err(e) => openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("generation failed: {e}"),
            "server_error",
            Some("generation_failed"),
        )
        .into_response(),
    }
}

#[derive(Serialize)]
struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Serialize)]
struct ChunkChoice {
    index: usize,
    delta: ChunkDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChatCompletionChunk {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<ChunkChoice>,
}

/// SSE streaming for `stream=true`. Generation runs on a blocking thread; text
/// deltas flow over an unbounded channel and are emitted as OpenAI
/// `chat.completion.chunk` events, terminated by `data: [DONE]`.
async fn chat_completions_stream(
    state: Arc<ApiState>,
    req: ChatCompletionsRequest,
) -> axum::response::Response {
    // Pre-flight readiness: if files are absent AND nothing loaded, fail with 503
    // BEFORE opening the SSE stream so the role router sees a clean error.
    if !state.local_llm.is_present() && !state.local_llm.is_ready().await {
        return not_ready_response("local model not ready: files missing".to_string())
            .into_response();
    }

    let params = req.to_gen_params();
    let prompt = req.prompt();
    let model_name = response_model_name(&req.model);
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = now_unix();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ChatCompletionChunk>();

    // First chunk: announce the assistant role (OpenAI convention).
    let _ = tx.send(ChatCompletionChunk {
        id: id.clone(),
        object: "chat.completion.chunk",
        created,
        model: model_name.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { role: Some("assistant"), content: None },
            finish_reason: None,
        }],
    });

    let engine = state.local_llm.clone();
    let id_gen = id.clone();
    let model_gen = model_name.clone();
    let tx_gen = tx.clone();
    tokio::spawn(async move {
        let tx_delta = tx_gen.clone();
        let id_d = id_gen.clone();
        let model_d = model_gen.clone();
        let result = engine
            .generate(&prompt, &params, move |d| {
                let _ = tx_delta.send(ChatCompletionChunk {
                    id: id_d.clone(),
                    object: "chat.completion.chunk",
                    created,
                    model: model_d.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: ChunkDelta { role: None, content: Some(d.to_string()) },
                        finish_reason: None,
                    }],
                });
            })
            .await;

        let finish = match result {
            Ok((reason, _)) => reason.as_str().to_string(),
            Err(e) => {
                tracing::warn!("[local_api] stream generation error: {e}");
                "error".to_string()
            }
        };
        let _ = tx_gen.send(ChatCompletionChunk {
            id: id_gen,
            object: "chat.completion.chunk",
            created,
            model: model_gen,
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta { role: None, content: None },
                finish_reason: Some(finish),
            }],
        });
    });

    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tokio_stream::StreamExt;
    let body = UnboundedReceiverStream::new(rx)
        .map(|chunk| {
            let json = serde_json::to_string(&chunk).unwrap_or_else(|_| "{}".to_string());
            Ok::<Event, std::convert::Infallible>(Event::default().data(json))
        })
        .chain(tokio_stream::iter(vec![Ok(Event::default().data("[DONE]"))]));

    Sse::new(body).into_response()
}

#[cfg(test)]
mod openai_embeddings_tests {
    use super::*;
    use axum::extract::State;
    use crate::memory_bucket_seal::InertEmbedder;

    fn make_state() -> Arc<ApiState> {
        let tmp = tempfile::tempdir().unwrap();
        Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(tmp.path().to_path_buf())),
        })
    }

    #[tokio::test]
    async fn rejects_empty_batch_input_with_400() {
        let state = make_state();
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
        let state = make_state();
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
    async fn accepts_float_encoding_format_and_returns_ok() {
        // encoding_format='float' is allowed; with the in-process InertEmbedder
        // (deterministic zeros) this should succeed and return 200.
        let state = make_state();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Single("hi".to_string()),
            model: None,
            encoding_format: Some("float".to_string()),
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        assert!(result.is_ok(), "expected Ok with in-process embedder");
    }

    #[tokio::test]
    async fn single_input_returns_one_embedding() {
        let state = make_state();
        let req = EmbeddingsRequest {
            input: EmbeddingsInput::Single("hello world".to_string()),
            model: None,
            encoding_format: None,
        };
        let result = openai_embeddings(State(state), Json(req)).await;
        let resp = result.expect("expected Ok from InertEmbedder");
        assert_eq!(resp.data.len(), 1);
        assert_eq!(resp.data[0].index, 0);
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

#[cfg(test)]
mod chat_completions_tests {
    use super::*;
    use axum::extract::State;
    use crate::memory_bucket_seal::InertEmbedder;

    fn state_with_absent_model() -> Arc<ApiState> {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().to_path_buf();
        // tempdir dropped at end of fn; engine only reads paths lazily on generate,
        // and the dir staying or not doesn't matter because files are absent either way.
        drop(tmp);
        Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(path)),
        })
    }

    #[test]
    fn request_deserializes_messages_and_flags() {
        let req: ChatCompletionsRequest = serde_json::from_str(
            r#"{"model":"local/minicpm5-1b","messages":[{"role":"user","content":"hi"}],"stream":true,"temperature":0.5,"max_tokens":10}"#,
        )
        .unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert!(req.stream);
        assert_eq!(req.max_tokens, Some(10));
    }

    #[test]
    fn stop_field_accepts_string_or_array() {
        let one: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[],"stop":"END"}"#).unwrap();
        assert_eq!(one.stop_strings(), vec!["END".to_string()]);
        let many: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[],"stop":["A","B"]}"#).unwrap();
        assert_eq!(many.stop_strings(), vec!["A".to_string(), "B".to_string()]);
        let none: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[]}"#).unwrap();
        assert!(none.stop_strings().is_empty());
    }

    #[test]
    fn params_mapping_applies_request_overrides() {
        let req: ChatCompletionsRequest = serde_json::from_str(
            r#"{"messages":[],"temperature":0.2,"top_p":0.5,"max_tokens":7}"#,
        )
        .unwrap();
        let p = req.to_gen_params();
        assert!((p.temperature - 0.2).abs() < 1e-9);
        assert_eq!(p.top_p, Some(0.5));
        assert_eq!(p.max_tokens, 7);
    }

    #[test]
    fn prompt_defaults_to_no_think_and_opts_into_thinking() {
        // Default (no enable_thinking) → suppressed CoT: assistant turn ends
        // with a prefilled empty think block so the model answers directly.
        let default_req: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[{"role":"user","content":"hi"}]}"#).unwrap();
        let p = default_req.prompt();
        assert!(p.ends_with("<|im_start|>assistant\n<think>\n</think>\n"), "got {p:?}");

        // enable_thinking: true → plain ChatML (reasoning allowed), no prefilled block.
        let think_req: ChatCompletionsRequest = serde_json::from_str(
            r#"{"messages":[{"role":"user","content":"hi"}],"enable_thinking":true}"#,
        )
        .unwrap();
        let pt = think_req.prompt();
        assert!(pt.ends_with("<|im_start|>assistant\n"), "got {pt:?}");
        assert!(!pt.contains("<think>"), "thinking-enabled prompt must not prefill think: {pt:?}");
    }

    #[tokio::test]
    async fn non_stream_returns_503_when_model_absent() {
        let state = state_with_absent_model();
        let req = ChatCompletionsRequest {
            model: Some("local/minicpm5-1b".into()),
            messages: vec![ChatMessageDto { role: "user".into(), content: "hi".into() }],
            stream: false,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            stop: None,
            enable_thinking: None,
        };
        let result = chat_completions(State(state), Json(req)).await;
        let resp = result.into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    /// Gated streaming smoke test: only runs when the model is present locally.
    #[tokio::test]
    async fn stream_emits_content_when_model_present() {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let data_dir = std::path::Path::new(&home).join(".uclaw");
        if !crate::local_llm::is_present(&data_dir) {
            eprintln!("[skip] model not present");
            return;
        }
        let state = Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(data_dir)),
        });
        let req = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessageDto { role: "user".into(), content: "2+2=".into() }],
            stream: true,
            temperature: Some(0.0),
            top_p: None,
            top_k: None,
            max_tokens: Some(16),
            stop: None,
            enable_thinking: None,
        };
        let resp = chat_completions_stream(state, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn stream_returns_503_when_model_absent() {
        let state = state_with_absent_model();
        let req = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessageDto { role: "user".into(), content: "hi".into() }],
            stream: true,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            stop: None,
            enable_thinking: None,
        };
        let resp = chat_completions_stream(state, req).await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
