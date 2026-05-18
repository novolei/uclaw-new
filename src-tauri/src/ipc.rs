use serde::{Deserialize, Serialize};
use serde::de::Deserializer;

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_budget_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSettingsInput {
    pub language: Option<String>,
    pub theme: Option<String>,
    /// When provided, sets the monthly budget. To CLEAR the budget, send `Some(None)` —
    /// represented on the wire as `monthlyBudgetUsd: null`. To leave unchanged, omit the
    /// field entirely.
    #[serde(default, deserialize_with = "deserialize_option_option_f64")]
    pub monthly_budget_usd: Option<Option<f64>>,
}

fn deserialize_option_option_f64<'de, D>(d: D) -> Result<Option<Option<f64>>, D::Error>
where D: Deserializer<'de>
{
    Ok(Some(Option::<f64>::deserialize(d)?))
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
    /// User-selected system prompt ID to use for this message.
    /// Falls back to the global default prompt when None.
    pub prompt_id: Option<String>,
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

// ─── System Prompts ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptDto {
    pub id: String,
    pub name: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_builtin: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort_order: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptConfigDto {
    pub prompts: Vec<SystemPromptDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_prompt_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub append_date_time_and_user_name: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptCreateInput {
    pub name: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptUpdateInput {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// A single version snapshot of a system prompt.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemPromptVersionDto {
    pub id: String,
    pub prompt_id: String,
    pub name: String,
    pub content: String,
    pub created_at: i64,
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
    pub path: Option<String>,
    pub attached_dirs: Vec<String>,
    pub sort_order: i64,
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
    /// One of: "conversation" (title hit), "chat_message", "agent_turn",
    /// "agent_message", "file".
    pub source: String,
    /// The session/conversation id we should navigate to.
    pub source_id: String,
    /// Optional message id to scroll to inside the session. None for
    /// title-only hits.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message_id: Option<String>,
    /// Workspace this hit originated in. Resolved server-side via
    /// `agent_sessions.space_id` (agent domain) or
    /// `conversations.workspace_id` (chat domain). `None` only when the
    /// row hasn't been re-homed yet (legacy data) — the frontend treats
    /// `None` as 'default'.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
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
    pub transport_type: crate::mcp::TransportType,
    pub command: String,
    pub args: Vec<String>,
    pub env: Option<std::collections::HashMap<String, String>>,
    pub url: Option<String>,
    pub enabled: bool,
    pub auto_approve: bool,
    pub error_message: Option<String>,
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
    pub transport_type: Option<crate::mcp::TransportType>,
    pub url: Option<String>,
    pub auto_approve: Option<bool>,
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
    /// Disk-tier provenance ("bundled" / "user" / "project") — drives
    /// the Settings UI's tier badge and the Fork affordance (only
    /// `bundled` skills offer Fork to my skills).
    #[serde(default = "default_provenance")]
    pub provenance: crate::skills::SkillProvenance,
}

fn default_provenance() -> crate::skills::SkillProvenance {
    // Legacy callers without provenance default to Project — safest
    // assumption since both Bundled and User imply specific paths the
    // legacy caller couldn't have known.
    crate::skills::SkillProvenance::Project
}

/// Aggregate view returned by `get_workspace_capabilities` — what skills and
/// MCP servers are available to a given workspace.
///
/// Skills and MCP servers are app-global today (no per-workspace scoping),
/// so the `slug` arg on the IPC is currently ignored. The shape is shared
/// with the frontend `getWorkspaceCapabilities` bridge in `tauri-bridge.ts`
/// and with `mention-suggestions.tsx` / `LeftSidebar.tsx`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCapabilities {
    pub mcp_servers: Vec<McpServerInfo>,
    pub skills: Vec<SkillInfo>,
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
    /// Path approval: "once" | "session" | "deny" (only set when kind=="path")
    #[serde(default)]
    pub path_scope: Option<String>,
    /// Path approval: which absolute paths to grant (only when path_scope="session")
    #[serde(default)]
    pub paths: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApproveToolCallResponse {
    pub success: bool,
}

// ─── Memory Graph IPC Types ---

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
pub struct MemoryGraphQuickCaptureInput {
    pub content: String,
    pub title: Option<String>,
    pub space_id: Option<String>,
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
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

// ─── EntityPage IPC types (Memory OS Foundation Phase 1) ───────────────
//
// Wire shape for the `memory_entity_page_*` Tauri commands. The Rust-side
// EntityPageMetadata schema is fully expressed by these DTOs, but for IPC
// we keep `metadata` as a raw `serde_json::Value` to mirror the existing
// Memory Graph command style (see MemoryGraphCreateNodeInput) and to keep
// the frontend free to extend the schema without round-trip Rust changes.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageCreateInput {
    /// Workspace scope. Defaults to `"default"` if omitted.
    pub space_id: Option<String>,
    /// Stable handle; lowercased + trimmed before storage.
    pub slug: String,
    pub title: String,
    /// Initial compiled_truth markdown body (lands in
    /// `memory_versions.content` as the active version).
    pub compiled_truth: String,
    /// Optional EntityPageMetadata-shaped JSON. Unknown fields are
    /// tolerated (forward-compat). Schema documented in
    /// `memory_graph::entity_page::EntityPageMetadata`.
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageGetInput {
    pub node_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageFindBySlugInput {
    pub space_id: Option<String>,
    pub slug: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageListInput {
    pub space_id: Option<String>,
    /// Optional metadata.subkind filter (`"entity"`, `"concept"`, ...).
    pub subkind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageAppendTimelineInput {
    pub node_id: String,
    /// One timeline entry; matches
    /// `memory_graph::entity_page::TimelineEntry`. Date is `YYYY-MM-DD`.
    pub date: String,
    pub text: String,
    pub source_node_id: Option<String>,
    pub source_session_id: Option<String>,
}

// ─── Wiki Artifacts IPC types (Memory OS Foundation Phase 3) ───────────
//
// Wire types for the `memory_wiki_*` Tauri commands. Read shape is a
// `WikiArtifactDto` mirroring the `wiki_artifacts` row; the frontend
// renders `content` as markdown.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiGetInput {
    pub space_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiRegenerateInput {
    pub space_id: Option<String>,
    /// Which artifact to regenerate. `"index"` is free; `"overview"`
    /// calls the configured `WikiSynthesizer` (stub in Phase 3, real
    /// LLM in a follow-up). Default behaviour when omitted: regenerate
    /// `"index"` only (the cheap path).
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiArtifactDto {
    pub id: String,
    pub space_id: String,
    pub kind: String,
    pub content: String,
    pub generated_at: i64,
    pub source_node_ids: Vec<String>,
    pub llm_model: Option<String>,
    pub token_cost: i64,
}

// ─── Health Findings IPC types (Memory OS Foundation Phase 4) ──────────
//
// Read DTO for `memory_health_findings` rows + input shapes for the
// three new `memory_health_*` commands.

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthListInput {
    pub space_id: Option<String>,
    /// When true, include rows with `dismissed=1`. Default false —
    /// the frontend usually only shows the active queue.
    pub include_dismissed: Option<bool>,
    /// Optional `check_kind` filter (e.g. `"orphan"`). When None,
    /// returns every kind.
    pub check_kind: Option<String>,
    pub limit: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthDismissInput {
    pub finding_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthRunNowInput {
    pub space_id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LintRunNowInput {
    pub space_id: Option<String>,
}

/// Phase 6.3 — manual synth trigger from the WikiView "Synthesize now"
/// button. Single required field is the EntityPage's node id.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityPageSynthesizeNowInput {
    pub node_id: String,
}

/// Phase 7.1 — `memory_wiki_export` input. `brainRoot` is optional;
/// when absent the backend resolves the default
/// `~/Documents/workground/brain/`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiExportInput {
    pub space_id: Option<String>,
    pub brain_root: Option<String>,
}

/// Phase 7.2 — `memory_wiki_sync_from_disk` input. Mirrors the export
/// input shape; reuses the same brain_root resolution.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WikiSyncInput {
    pub space_id: Option<String>,
    pub brain_root: Option<String>,
}

// ─── Memory OS Sprint 1.10 — learning IPC ──────────────────────────────

/// Manual trigger for the stability rebuild. Returns the
/// `RebuildOutcome` shape (snake_case via serde default).
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningRebuildNowInput {
    /// Reserved for future per-space scoping. Currently ignored
    /// (the facet store is global).
    pub space_id: Option<String>,
}

/// Filter for the list endpoint. Empty filters return everything.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningListFacetsInput {
    /// "identity" / "style" / "tooling" / "veto" / "goal" / "channel"
    /// or None for all classes.
    pub class: Option<String>,
    /// "active" / "provisional" / "candidate" / "forgotten" or None
    /// for all states.
    pub state: Option<String>,
}

/// Camel-case mirror of `FacetSnapshot` for the wire. State / class
/// are strings here so frontend doesn't need to mirror the Rust enums.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FacetDto {
    pub facet_id: String,
    pub class: String,
    pub name: String,
    pub value: String,
    pub state: String,
    pub stability: f64,
    pub evidence_count: u32,
    pub last_seen_at_ms: i64,
}

/// User-driven forget: flag a facet as Forgotten regardless of its
/// current stability. The next rebuild may resurface it if evidence
/// accumulates again — this is "dismiss for now", not "tombstone".
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningDismissFacetInput {
    pub facet_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthFindingDto {
    pub id: String,
    pub space_id: String,
    pub severity: String,
    pub check_kind: String,
    pub subject: String,
    pub payload_json: Option<String>,
    pub is_lint: bool,
    pub dismissed: bool,
    pub discovered_at: i64,
    pub dismissed_at: Option<i64>,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCostRollup {
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_icon: String,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetThresholdPayload {
    /// 80 or 100. The threshold percentage that was crossed on this turn.
    pub threshold: u8,
    /// Current monthly total in USD (after this turn's cost).
    pub current: f64,
    /// Configured monthly budget in USD.
    pub budget: f64,
}

// ─── Permission rules ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionRule {
    pub id: String,
    /// "session" | "pattern"
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub tool_name: String,
    /// For scope='pattern': the arg/command prefix to match. None for session scope.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    /// "allow" | "block" | "ask"
    pub mode: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionAuditEntry {
    pub id: String,
    pub session_id: String,
    pub tool_name: String,
    pub args_hash: String,
    /// "auto_approve" | "user_approve" | "user_deny" | "blocked"
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreatePermissionRuleInput {
    pub scope: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    pub tool_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub mode: String,
}

// ─── Default prompts (for Settings → 提示词 read-only preview) ─────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DefaultPromptsResponse {
    pub baseline: String,
    pub mode_ask: String,
    pub mode_accept_edits: String,
    pub mode_plan: String,
    pub mode_bypass: String,
}

// ─── ask_user ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserOption {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserQuestion {
    pub question: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<String>,
    #[serde(default)]
    pub multi_select: bool,
    #[serde(default)]
    pub options: Vec<AskUserOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AskUserRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub questions: Vec<AskUserQuestion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondAskUserInput {
    pub request_id: String,
    pub answers: serde_json::Map<String, serde_json::Value>,
}

// ─── exit_plan_mode ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExitPlanRequestPayload {
    pub request_id: String,
    pub session_id: String,
    pub plan: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_prompts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RespondExitPlanInput {
    pub request_id: String,
    /// "accept_and_auto" | "accept_keep_plan" | "reject"
    pub decision: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feedback: Option<String>,
    /// For accept_keep_plan + accept_and_auto, the original allowed_prompts
    /// list (frontend echoes them back since backend's pending registry
    /// doesn't store the request body).
    #[serde(default)]
    pub allowed_prompts: Vec<String>,
    /// Echo of session_id (frontend already knows it, simpler than backend
    /// stashing it).
    pub session_id: String,
}

// ─── Fragment / Daily Summary IPC Types ────────────────────────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListFragmentsInput {
    pub space_id: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentItem {
    pub id: String,
    pub title: Option<String>,
    pub content: String,
    pub source: String,
    pub tags: Vec<String>,
    pub subtype: Option<String>,
    pub created_at: i64,
    pub review_status: Option<ReviewStatus>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewStatus {
    pub review_count: i32,
    pub next_review_at: Option<i64>,
    pub completed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FragmentSearchHit {
    pub id: String,
    pub title: Option<String>,
    pub snippet: String,
    pub tags: Vec<String>,
    pub subtype: Option<String>,
    pub source: String,
    pub created_at: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailySummaryItem {
    pub id: String,
    pub summary_date: String,
    pub content: String,
    pub fragment_count: i32,
    pub fragment_ids: Vec<String>,
    pub created_at: i64,
}

#[cfg(test)]
mod workspace_capabilities_tests {
    use super::*;
    use serde_json::json;

    /// The frontend `getWorkspaceCapabilities` bridge in `tauri-bridge.ts`
    /// expects `{ mcpServers, skills }` (camelCase) — the empty case must
    /// serialize cleanly so the LeftSidebar count badge and the
    /// `mention-suggestions.tsx` dropdown render placeholders, not crash.
    #[test]
    fn empty_serializes_to_camelcase_arrays() {
        let payload = WorkspaceCapabilities { mcp_servers: vec![], skills: vec![] };
        let actual = serde_json::to_value(&payload).unwrap();
        assert_eq!(actual, json!({"mcpServers": [], "skills": []}));
    }

    /// Round-trip: serialize a populated payload, parse it back into the
    /// same struct, verify the round-trip is lossless. Catches any future
    /// frontend/backend drift if someone renames a field.
    #[test]
    fn populated_round_trips_through_camelcase() {
        let payload = WorkspaceCapabilities {
            mcp_servers: vec![McpServerInfo {
                id: "srv-1".into(),
                name: "github".into(),
                description: "GitHub MCP".into(),
                transport_type: crate::mcp::TransportType::Stdio,
                command: "uvx".into(),
                args: vec!["mcp-server-github".into()],
                env: None,
                url: None,
                enabled: true,
                auto_approve: false,
                error_message: None,
                status: "connected".into(),
            }],
            skills: vec![SkillInfo {
                name: "tdd".into(),
                version: "0.1.0".into(),
                description: "Test-driven development".into(),
                author: "uclaw".into(),
                enabled: true,
                category: "process".into(),
                provenance: crate::skills::SkillProvenance::Bundled,
            }],
        };
        let json_str = serde_json::to_string(&payload).unwrap();
        let parsed: WorkspaceCapabilities = serde_json::from_str(&json_str).unwrap();

        assert_eq!(parsed.mcp_servers.len(), 1);
        assert_eq!(parsed.mcp_servers[0].name, "github");
        assert_eq!(parsed.mcp_servers[0].auto_approve, false);
        assert_eq!(parsed.skills.len(), 1);
        assert_eq!(parsed.skills[0].name, "tdd");
        // mcpServers + autoApprove (not mcp_servers / auto_approve) is the
        // frontend contract — assert the on-the-wire shape directly.
        assert!(json_str.contains("\"mcpServers\""), "expected camelCase mcpServers in JSON: {}", json_str);
        assert!(json_str.contains("\"autoApprove\""), "expected camelCase autoApprove in JSON: {}", json_str);
        assert!(!json_str.contains("\"mcp_servers\""), "snake_case leaked: {}", json_str);
    }
}

#[cfg(test)]
mod ask_user_tests {
    use super::*;

    #[test]
    fn ask_user_question_accepts_camel_case_multiselect() {
        // Schema advertises multiSelect — LLM sends multiSelect — must deserialize.
        let json = serde_json::json!({
            "question": "Q1?",
            "multiSelect": false,
            "options": [{"label": "A"}],
        });
        let q: AskUserQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
    }

    #[test]
    fn ask_user_question_defaults_multiselect_when_absent() {
        // Some LLM calls omit multiSelect entirely — default to false
        // rather than fail the whole tool call.
        let json = serde_json::json!({
            "question": "Q1?",
            "options": [{"label": "A"}],
        });
        let q: AskUserQuestion = serde_json::from_value(json).unwrap();
        assert!(!q.multi_select);
    }
}
