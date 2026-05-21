//! Typed request/response payloads for the 4 initial MCP-exposed
//! uClaw tools.

use serde::{Deserialize, Serialize};

/// Stable id for each MCP-exposed tool. Used by the rmcp wire-up to
/// route requests + by the M2-J UI to show "MCP server: 3 tools
/// connected".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerToolKind {
    ListThreads,
    ReadThread,
    StartAutomation,
    QueryMemory,
}

impl ServerToolKind {
    pub const ALL: [ServerToolKind; 4] = [
        Self::ListThreads,
        Self::ReadThread,
        Self::StartAutomation,
        Self::QueryMemory,
    ];

    /// Stable name for MCP `tools/list` advertisement.
    pub const fn name(self) -> &'static str {
        match self {
            Self::ListThreads => "uclaw_list_threads",
            Self::ReadThread => "uclaw_read_thread",
            Self::StartAutomation => "uclaw_start_automation",
            Self::QueryMemory => "uclaw_query_memory",
        }
    }
}

// ── list_threads ────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThreadsRequest {
    /// Optional substring filter on thread titles. Empty / None →
    /// return all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title_contains: Option<String>,
    /// Maximum threads to return. `0` = no limit.
    #[serde(default)]
    pub limit: u32,
}

/// Compact thread metadata. Body content lives in `ReadThread`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadSummary {
    pub id: String,
    pub title: String,
    pub message_count: u32,
    pub updated_at: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListThreadsResponse {
    pub threads: Vec<ThreadSummary>,
    pub truncated: bool,
}

// ── read_thread ────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadThreadRequest {
    pub thread_id: String,
    /// Optional cap on number of messages returned (most-recent first).
    /// `0` = no limit.
    #[serde(default)]
    pub max_messages: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReadThreadResponse {
    pub thread_id: String,
    pub title: String,
    /// JSON-encoded message list. Caller deserializes against the
    /// uClaw message schema. Kept as a string here so the server type
    /// doesn't pull in the full agent-message types.
    pub messages_json: String,
    pub message_count: u32,
}

// ── start_automation ───────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAutomationRequest {
    pub automation_id: String,
    /// JSON-encoded args blob passed to the automation. Empty when
    /// the automation takes no args.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args_json: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartAutomationResponse {
    /// uClaw task id assigned to this run.
    pub task_id: String,
    /// `"queued"` / `"running"` / `"rejected"` — initial state.
    pub status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<String>,
}

// ── query_memory ───────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryMemoryRequest {
    pub query: String,
    /// Top-K hits to return.
    #[serde(default)]
    pub top_k: u32,
    /// Optional namespace filter (e.g. "user_facts" / "project_notes").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueryMemoryResponse {
    /// Hits as JSON. Each element should be `{"id": "...", "snippet":
    /// "...", "score": 0.87}` but we keep the outer container open
    /// to let gbrain return richer payloads.
    pub hits_json: String,
    pub hit_count: u32,
}

// ── server config + auth ──────────────────────────────────────────

/// Hex-string auth token used by the MCP client to authenticate.
/// Stored as a newtype so we can swap to redacted display + zeroize
/// on drop in a follow-up without API churn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AuthToken(pub String);

impl AuthToken {
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }
    /// Length of the underlying string — useful for "is this a valid
    /// length token?" checks without leaking the value.
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Top-level server config, hydrated from `~/.uclaw/mcp_server.toml`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ServerConfig {
    /// `true` to actually bind the listening socket; default-disabled.
    pub enabled: bool,
    /// Bind address (e.g. `"127.0.0.1:9876"`).
    pub bind_addr: String,
    /// Tools the server should expose. Tools NOT in this list are
    /// hidden even if registered.
    #[serde(default)]
    pub exposed_tools: Vec<ServerToolKind>,
    /// Auth tokens accepted by the server. Empty → server refuses all
    /// requests (safe-by-default for misconfiguration).
    #[serde(default)]
    pub auth_tokens: Vec<AuthToken>,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            bind_addr: "127.0.0.1:0".into(),
            exposed_tools: Vec::new(),
            auth_tokens: Vec::new(),
        }
    }
}

impl ServerConfig {
    /// Default-on dev preset: bind to 127.0.0.1:9876, expose all 4
    /// tools, no auth tokens (so the server still refuses requests
    /// until the user adds one).
    pub fn dev_preset() -> Self {
        Self {
            enabled: true,
            bind_addr: "127.0.0.1:9876".into(),
            exposed_tools: ServerToolKind::ALL.to_vec(),
            auth_tokens: Vec::new(),
        }
    }

    /// `true` if `tool` is on the exposed list.
    pub fn exposes(&self, tool: ServerToolKind) -> bool {
        self.exposed_tools.contains(&tool)
    }

    /// `true` if `token` is in the accepted set.
    pub fn accepts(&self, token: &AuthToken) -> bool {
        self.auth_tokens.contains(token)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ServerToolKind ────────────────────────────────────────────

    #[test]
    fn all_4_tool_kinds_present_and_distinct() {
        assert_eq!(ServerToolKind::ALL.len(), 4);
        let mut names: Vec<_> = ServerToolKind::ALL.iter().map(|t| t.name()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), 4);
    }

    #[test]
    fn tool_names_use_uclaw_prefix() {
        for k in ServerToolKind::ALL {
            assert!(k.name().starts_with("uclaw_"));
        }
    }

    #[test]
    fn tool_kind_serde_snake_case() {
        let v = serde_json::to_value(ServerToolKind::ListThreads).unwrap();
        assert_eq!(v, serde_json::json!("list_threads"));
        let v = serde_json::to_value(ServerToolKind::StartAutomation).unwrap();
        assert_eq!(v, serde_json::json!("start_automation"));
    }

    // ── request/response serde ────────────────────────────────────

    #[test]
    fn list_threads_request_skips_none_filter() {
        let r = ListThreadsRequest::default();
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("titleContains"));
    }

    #[test]
    fn read_thread_response_camelcase_keys() {
        let r = ReadThreadResponse {
            thread_id: "t1".into(),
            title: "Hi".into(),
            messages_json: "[]".into(),
            message_count: 5,
        };
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["threadId"], "t1");
        assert_eq!(v["messagesJson"], "[]");
        assert_eq!(v["messageCount"], 5);
    }

    #[test]
    fn start_automation_response_rejected_includes_reason() {
        let r = StartAutomationResponse {
            task_id: "x".into(),
            status: "rejected".into(),
            reject_reason: Some("policy gate".into()),
        };
        let json = serde_json::to_string(&r).unwrap();
        assert!(json.contains("\"rejectReason\":\"policy gate\""));
    }

    #[test]
    fn query_memory_roundtrip() {
        let req = QueryMemoryRequest {
            query: "rust async".into(),
            top_k: 5,
            namespace: Some("project_notes".into()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: QueryMemoryRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(req, back);
    }

    // ── AuthToken ─────────────────────────────────────────────────

    #[test]
    fn auth_token_serializes_transparently() {
        let t = AuthToken::new("abc123");
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, "\"abc123\"");
        let back: AuthToken = serde_json::from_str(&json).unwrap();
        assert_eq!(t, back);
    }

    #[test]
    fn auth_token_len_and_empty() {
        let t = AuthToken::new("");
        assert!(t.is_empty());
        assert_eq!(t.len(), 0);
        let t = AuthToken::new("abc");
        assert_eq!(t.len(), 3);
        assert!(!t.is_empty());
    }

    // ── ServerConfig ──────────────────────────────────────────────

    #[test]
    fn default_config_disabled() {
        let c = ServerConfig::default();
        assert!(!c.enabled);
        assert_eq!(c.bind_addr, "127.0.0.1:0");
        assert!(c.exposed_tools.is_empty());
        assert!(c.auth_tokens.is_empty());
    }

    #[test]
    fn dev_preset_enables_all_4_tools_no_tokens() {
        let c = ServerConfig::dev_preset();
        assert!(c.enabled);
        assert_eq!(c.bind_addr, "127.0.0.1:9876");
        assert_eq!(c.exposed_tools.len(), 4);
        for t in ServerToolKind::ALL {
            assert!(c.exposes(t));
        }
        // No tokens → server refuses all requests (safe-by-default).
        assert!(c.auth_tokens.is_empty());
    }

    #[test]
    fn exposes_check() {
        let mut c = ServerConfig::default();
        c.exposed_tools = vec![ServerToolKind::ListThreads];
        assert!(c.exposes(ServerToolKind::ListThreads));
        assert!(!c.exposes(ServerToolKind::QueryMemory));
    }

    #[test]
    fn accepts_check() {
        let mut c = ServerConfig::default();
        c.auth_tokens = vec![AuthToken::new("good")];
        assert!(c.accepts(&AuthToken::new("good")));
        assert!(!c.accepts(&AuthToken::new("bad")));
        // Empty token list rejects everything.
        let empty = ServerConfig::default();
        assert!(!empty.accepts(&AuthToken::new("good")));
    }
}
