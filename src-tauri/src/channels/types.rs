//! Core types for the IM channel framework.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// All supported IM/notify channel types.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ImChannelType {
    WecomBot,
    WechatIlink,
    Email,
    Dingtalk,
    Feishu,
    Webhook,
}

impl ImChannelType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WecomBot     => "wecom_bot",
            Self::WechatIlink  => "wechat_ilink",
            Self::Email        => "email",
            Self::Dingtalk     => "dingtalk",
            Self::Feishu       => "feishu",
            Self::Webhook      => "webhook",
        }
    }

    pub fn is_bidirectional(&self) -> bool {
        matches!(self, Self::WecomBot | Self::WechatIlink)
    }
}

impl std::fmt::Display for ImChannelType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Per-user permission policy for guests (non-owner senders).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GuestPolicy {
    /// Tool names guests are allowed to trigger (empty = all allowed).
    pub tool_allowlist: Vec<String>,
    /// Whether MCP tools are enabled for guests.
    pub mcp_enabled: bool,
}

/// One configured IM channel instance (DB row + deserialized fields).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImChannelInstanceConfig {
    pub id: String,
    pub space_id: String,
    pub channel_type: ImChannelType,
    pub name: String,
    /// Non-sensitive config (endpoint, bot_id, etc.)
    pub config: serde_json::Value,
    /// Sensitive credentials (api_key, secret, password, etc.)
    pub credentials: serde_json::Value,
    pub enabled: bool,
    pub streaming: bool,
    pub reply_scope: String,
    pub permission_enabled: bool,
    /// chat_id whitelist — only these senders can use this channel.
    pub owners: Vec<String>,
    pub guest_policy: GuestPolicy,
}

/// Unified inbound message from any bidirectional channel.
#[derive(Debug, Clone)]
pub struct InboundMessage {
    pub instance_id: String,
    /// User identifier (WeChat openid, WeCom userid, etc.)
    pub chat_id: String,
    pub sender_name: Option<String>,
    pub text: String,
    pub timestamp: i64,
    /// Channel-specific context passed through to ReplyHandle.
    /// iLink: `{"context_token": "..."}`.
    /// WeCom: `{"req_id": "...", "expires_at": <unix_ms>}`.
    pub channel_ctx: Option<serde_json::Value>,
}

/// Unified outbound reply handle — abstracts over all channel types.
#[derive(Clone)]
pub struct ReplyHandle {
    pub sender: Arc<dyn ImChannelSender>,
    pub chat_id: String,
    pub channel_ctx: Option<serde_json::Value>,
}

impl ReplyHandle {
    pub async fn send(&self, text: &str) -> Result<(), String> {
        self.sender.send_text(&self.chat_id, text, self.channel_ctx.as_ref()).await
    }
}

/// Unified outbound sender trait — implemented by each channel backend.
#[async_trait::async_trait]
pub trait ImChannelSender: Send + Sync {
    async fn send_text(
        &self,
        chat_id: &str,
        text: &str,
        ctx: Option<&serde_json::Value>,
    ) -> Result<(), String>;

    /// True for WeCom — enables streaming token updates.
    fn supports_streaming(&self) -> bool {
        false
    }
}

/// Streaming reply handle — only WeCom Bot supports real streaming.
/// Other channels ignore the update() calls and deliver on finish().
#[async_trait::async_trait]
pub trait StreamingHandle: Send + Sync {
    /// Send a partial update. May be called multiple times before finish().
    async fn update(&self, partial: &str) -> anyhow::Result<()>;
    /// Mark the stream complete with the final full text.
    async fn finish(&self, final_text: &str) -> anyhow::Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn im_channel_type_roundtrips_json() {
        let t = ImChannelType::WecomBot;
        let json = serde_json::to_string(&t).unwrap();
        assert_eq!(json, r#""wecom_bot""#);
        let back: ImChannelType = serde_json::from_str(&json).unwrap();
        assert_eq!(back, ImChannelType::WecomBot);
    }

    #[test]
    fn inbound_message_channel_ctx_is_optional() {
        let msg = InboundMessage {
            instance_id: "i1".into(),
            chat_id: "u1".into(),
            sender_name: None,
            text: "hello".into(),
            timestamp: 0,
            channel_ctx: None,
        };
        assert!(msg.channel_ctx.is_none());

        let msg_with_ctx = InboundMessage {
            channel_ctx: Some(serde_json::json!({"context_token": "abc"})),
            ..msg
        };
        assert!(msg_with_ctx.channel_ctx.is_some());
    }

    #[test]
    fn guest_policy_default_is_permissive() {
        let gp = GuestPolicy::default();
        assert!(gp.tool_allowlist.is_empty());
        assert!(!gp.mcp_enabled);
    }

    #[test]
    fn streaming_handle_is_object_safe() {
        fn _accepts(_: &dyn StreamingHandle) {}
    }
}
