use serde::{Deserialize, Serialize};

/// Common IPC result type
pub type IpcResult<T> = Result<T, crate::error::Error>;

// ─── Bootstrap ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSettingsResponse {
    pub language: String,
    pub theme: String,
    pub config_path: String,
    pub data_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSettingsInput {
    pub language: Option<String>,
    pub theme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlatformInfo {
    pub os: String,
    pub arch: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionInfo {
    pub app_version: String,
    pub tauri_version: String,
    pub rust_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BootstrapStatus {
    pub initialized: bool,
    pub db_ready: bool,
    pub config_ready: bool,
}

// ─── Chat / Messages ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageInput {
    pub conversation_id: String,
    pub content: String,
    pub attachments: Option<Vec<String>>,
    /// Safety mode for this message: "ask", "supervised", or "yolo"
    pub safety_mode: Option<String>,
    /// Explicit provider to use for this message (overrides active_model).
    pub provider_id: Option<String>,
    /// Explicit model to use for this message (overrides active_model).
    pub model_id: Option<String>,
    /// Enable extended thinking/reasoning for this message.
    pub thinking_enabled: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SendMessageResponse {
    pub message_id: String,
    pub conversation_id: String,
    pub response: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateConversationInput {
    pub title: Option<String>,
    pub space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConversationResponse {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub message_count: usize,
    pub created_at: String,
    pub updated_at: String,
}

/// Cross-domain summary of a recent conversation or agent session for the
/// search palette's browse mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentThread {
    pub id: String,
    /// "chat" | "agent"
    pub kind: String,
    pub title: String,
    /// Optional emoji prefix (from conversation metadata)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_emoji: Option<String>,
    /// Whether title generation is still pending (show spinner)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title_pending: Option<bool>,
    /// Human-readable workspace name (the space the thread lives in)
    pub workspace_name: String,
    /// Workspace id for navigation
    pub workspace_id: String,
    /// Number of messages in this thread
    pub message_count: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetMessagesInput {
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageResponse {
    pub id: String,
    pub conversation_id: String,
    pub role: String,
    pub content: String,
    pub created_at: String,
    /// Concatenated thinking-block text for assistant messages.
    /// Frontend renders via the <Reasoning> collapsible block.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<String>,
    /// Tool activity records for assistant messages — array of
    /// `{ tool, status, input, output, ... }` objects matching the
    /// frontend `ChatToolActivity` shape (deserialized JSON).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_activities: Option<serde_json::Value>,
    /// Model identifier used for the assistant turn, e.g. "deepseek-v4-flash".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    /// Original ordered ContentBlocks parsed from `messages.content`.
    /// `None` for legacy plain-text rows or rows that fail to parse.
    /// When `Some`, the frontend renders via NativeBlockRenderer for
    /// in-order display; when `None`, falls back to flat `content` +
    /// `reasoning` + `tool_activities`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_blocks: Option<Vec<crate::agent::types::ContentBlock>>,
}

// ─── Spaces ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSpaceInput {
    pub name: String,
    pub icon: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceResponse {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub created_at: String,
    pub updated_at: String,
}

// ─── LLM Config ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigInput {
    pub provider: String,
    pub model: String,
    pub api_key: String,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LlmConfigResponse {
    pub provider: String,
    pub model: String,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

// ─── Artifacts (Files) ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub children: Option<Vec<ArtifactNode>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadArtifactInput {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteArtifactInput {
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactContentResponse {
    pub path: String,
    pub content: String,
    pub size: u64,
}

// ─── Search ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchInput {
    pub query: String,
    pub scope: Option<String>, // "workspace" | "conversations" | "all"
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchResult {
    pub id: String,
    pub title: String,
    pub snippet: String,
    /// One of: "conversation" (title hit), "chat_message", "agent_turn", "file".
    pub source: String,
    /// The session/conversation id we should navigate to.
    pub source_id: String,
    /// Optional message id to scroll to inside the session. None for title-only hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    pub created_at: String,
}

// ─── Notifications ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NotificationItem {
    pub id: String,
    pub title: String,
    pub message: String,
    pub level: String,
    pub source: String,
    pub timestamp: String,
}

// ─── Memory ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySetInput {
    pub key: String,
    pub value: serde_json::Value,
    pub kind: Option<String>,
    pub namespace: Option<String>,
    pub space_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGetInput {
    pub key: String,
    pub namespace: Option<String>,
    pub space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchInput {
    pub query: String,
    pub namespace: Option<String>,
    pub space_id: Option<String>,
    pub kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryListInput {
    pub namespace: Option<String>,
    pub space_id: Option<String>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryClearInput {
    pub namespace: String,
    pub space_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBulkImportEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub kind: Option<String>,
    pub namespace: Option<String>,
    pub space_id: Option<String>,
    pub tags: Option<Vec<String>>,
    pub metadata: Option<serde_json::Value>,
    pub ttl_seconds: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBulkImportInput {
    pub entries: Vec<MemoryBulkImportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEntryResponse {
    pub id: String,
    pub key: String,
    pub value: serde_json::Value,
    pub kind: String,
    pub namespace: String,
    pub space_id: String,
    pub tags: Vec<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryBulkImportResponse {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryCountResponse {
    pub count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryClearResponse {
    pub deleted: usize,
}

// ─── MCP ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub enabled: bool,
    pub auto_approve: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpServerInput {
    pub id: Option<String>,
    pub name: String,
    pub description: String,
    pub command: String,
    pub args: Option<Vec<String>>,
    pub env: Option<std::collections::HashMap<String, String>>,
}

// ─── Skills ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillInfo {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillDetailResponse {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
    pub enabled: bool,
    pub category: String,
    pub keywords: Vec<String>,
    pub tags: Vec<String>,
    pub patterns: Vec<String>,
    pub parameters: Vec<SkillParamInfo>,
    pub prompt_length: usize,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillParamInfo {
    pub name: String,
    pub param_type: String,
    pub required: bool,
    pub description: String,
    pub default: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillToggleInput {
    pub name: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatchInput {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillMatchResult {
    pub name: String,
    pub score: u32,
    pub prompt_preview: String,
}

// ─── Channels ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInfo {
    pub id: String,
    pub name: String,
    pub channel_type: String,
    pub enabled: bool,
    pub webhook_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChannelInput {
    pub name: String,
    pub channel_type: String,
    pub webhook_url: Option<String>,
    pub config: Option<serde_json::Value>,
}

// ─── Providers ──────────────────────────────────────────────────────────

/// Built-in provider info returned to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderInfo {
    pub id: String,
    pub display_name: String,
    pub auth_type: String,
    pub default_base_url: String,
    pub default_api: String,
    pub service_category: String,
    pub geo_category: String,
    pub supports_models: bool,
}

/// Provider configuration sent from frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigInput {
    pub provider_id: String,
    pub display_name: String,
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub api: Option<String>,
}

/// Provider configuration stored on disk (response to frontend).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigResponse {
    pub provider_id: String,
    pub display_name: String,
    pub has_api_key: bool,
    pub base_url: Option<String>,
    pub api: Option<String>,
}

/// Configure provider with model selections.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderConfigureInput {
    #[serde(flatten)]
    pub config: ProviderConfigInput,
    pub model_ids: Vec<String>,
}

/// Model info returned to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: Option<u64>,
    pub max_tokens: Option<u64>,
    pub modality: String,
    pub reasoning: bool,
    pub supports_reasoning_effort: bool,
}

/// Model selection (active model reference).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSelectionInfo {
    pub provider_id: String,
    pub model_id: String,
}

/// Connection test result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestResultInfo {
    pub success: bool,
    pub message: String,
    pub latency_ms: Option<u64>,
    pub details: Option<String>,
}

/// List models input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListModelsInput {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

/// Test connection input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestConnectionInput {
    pub provider_id: String,
    pub base_url: String,
    pub api_key: Option<String>,
}

/// Role-based model config input.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetModelRoleInput {
    pub role: String,
    pub model_ref: String,
}

/// Model role config response.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelRoleConfigResponse {
    pub role: String,
    pub model_ref: Option<String>,
}

// ─── Enhanced Artifact Types ────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactTreeNodeResponse {
    pub path: String,
    pub name: String,
    pub is_dir: bool,
    pub parent_path: String,
    pub size_bytes: Option<u64>,
    pub mime_type: Option<String>,
    pub modified_at: Option<String>,
    pub children: Option<Vec<ArtifactTreeNodeResponse>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListArtifactTreeInput {
    pub space_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LoadArtifactChildrenInput {
    pub space_id: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArtifactInput {
    pub space_id: String,
    pub path: String,
    pub content: Option<String>,
    pub is_dir: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameArtifactInput {
    pub space_id: String,
    pub old_path: String,
    pub new_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveArtifactInput {
    pub space_id: String,
    pub src_path: String,
    pub dest_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectFileTypeResponse {
    pub mime_type: String,
    pub category: String,
}

// ─── File Change Event ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeEvent {
    pub space_id: String,
    pub change_type: String,
    pub path: String,
    pub old_path: Option<String>,
    pub is_dir: bool,
}

// ─── Conversation Star ──────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleStarInput {
    pub conversation_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToggleStarResponse {
    pub conversation_id: String,
    pub starred: bool,
}

// ─── Safety ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SafetyPolicyResponse {
    pub global_mode: String,
    pub tool_overrides: std::collections::HashMap<String, String>,
    pub auto_approved_tools: Vec<String>,
    pub blocked_tools: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetSafetyModeInput {
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SetToolOverrideInput {
    pub tool_name: String,
    pub mode: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolNameInput {
    pub tool_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssessCommandInput {
    pub command: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandRiskResponse {
    pub level: String,
    pub reasons: Vec<String>,
    pub suggested_action: String,
}

// ─── Tool Approval ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveToolCallInput {
    pub session_id: String,
    pub tool_id: String,
    pub approved: bool,
    pub always_allow: Option<bool>,
    /// Tool name needed to add to auto-approved whitelist when always_allow=true
    pub tool_name: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveToolCallResponse {
    pub success: bool,
}

// --- Memory Graph IPC Types ---

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphSearchInput {
    pub query: String,
    pub space_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphGetNodeInput {
    pub node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphListBootInput {
    pub space_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphManageBootInput {
    pub node_id: String,
    pub action: String, // "add" or "remove"
    pub space_id: Option<String>,
    pub priority: Option<i32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphTimelineInput {
    pub space_id: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphExplainRecallInput {
    pub query: String,
    pub space_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphCreateNodeInput {
    pub space_id: String,
    pub kind: String,
    pub title: String,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphUpdateNodeInput {
    pub node_id: String,
    pub title: Option<String>,
    pub kind: Option<String>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryGraphDeleteNodeInput {
    pub node_id: String,
}

// ─── Cost dashboard ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostRollup {
    /// `YYYY-MM-DD` (UTC).
    pub day: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCostRollup {
    pub model: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCostRollup {
    pub session_id: String,
    /// Joined session title from `agent_sessions`/`conversations`. Empty if unknown.
    pub title: String,
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cost_usd: f64,
    pub turn_count: i64,
    /// Most-recent record's created_at (epoch ms).
    pub last_used_at: i64,
}
