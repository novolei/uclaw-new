//! IM platform-agnostic types + adapter trait.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Known IM platforms. `Custom` allows plugin-defined platforms
/// without changing the core enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "platform", content = "subplatform", rename_all = "snake_case")]
pub enum ImPlatform {
    Slack,
    Discord,
    Telegram,
    Sms,
    WhatsApp,
    WeChat,
    Lark,
    Teams,
    Matrix,
    Custom(String),
}

impl ImPlatform {
    /// Stable id for table lookup / settings serialization.
    pub fn id(&self) -> String {
        match self {
            Self::Slack => "slack".into(),
            Self::Discord => "discord".into(),
            Self::Telegram => "telegram".into(),
            Self::Sms => "sms".into(),
            Self::WhatsApp => "whatsapp".into(),
            Self::WeChat => "wechat".into(),
            Self::Lark => "lark".into(),
            Self::Teams => "teams".into(),
            Self::Matrix => "matrix".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}

/// Channel / thread / DM pointer. Format of `id` is per-platform
/// (Slack `C01ABC`, Discord `1234567890`, Telegram `-100123`, ...).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImChannelRef {
    pub platform: ImPlatform,
    pub channel_id: String,
    /// Optional thread id within a channel (Slack thread_ts,
    /// Discord parent message id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    /// Display label for UI (e.g. `"#general"`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl ImChannelRef {
    pub fn channel(platform: ImPlatform, channel_id: impl Into<String>) -> Self {
        Self {
            platform,
            channel_id: channel_id.into(),
            thread_id: None,
            label: None,
        }
    }

    pub fn thread(
        platform: ImPlatform,
        channel_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) -> Self {
        Self {
            platform,
            channel_id: channel_id.into(),
            thread_id: Some(thread_id.into()),
            label: None,
        }
    }
}

/// Inbound message — the adapter assembles this from platform-
/// specific webhooks / polling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImMessage {
    /// Platform-specific message id (used to dedup if webhook fires twice).
    pub message_id: String,
    pub channel: ImChannelRef,
    /// Display name / handle of the sender. Adapters may also include
    /// stable user id in `sender_id`.
    pub sender_display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    pub text: String,
    /// RFC 3339 sent timestamp from the platform.
    pub sent_at: String,
    /// `true` when the platform tagged this message as edited.
    #[serde(default)]
    pub edited: bool,
    /// Attachment refs (URLs / blob refs) as JSON strings — adapters
    /// decide format. Empty for pure-text messages.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments_json: Vec<String>,
}

/// Outbound message — what we ask the adapter to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImOutbound {
    /// Plain text reply. Adapters that support markdown convert as needed.
    Text {
        channel: ImChannelRef,
        text: String,
    },
    /// Emoji / sticker reaction on an existing message.
    Reaction {
        channel: ImChannelRef,
        message_id: String,
        emoji: String,
    },
    /// "Typing…" indicator. Auto-clears after ~5s on most platforms.
    Typing { channel: ImChannelRef },
}

/// Outcome of an `ImChannelAdapter::send`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImSendResult {
    /// Platform-assigned id of the new message, when applicable
    /// (`None` for reactions / typing indicators).
    pub posted_message_id: Option<String>,
    /// Echo of the channel for diagnostic logs.
    pub channel: ImChannelRef,
}

/// Inbound event from the platform's push stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImEvent {
    MessageReceived(ImMessage),
    MessageEdited(ImMessage),
    MessageDeleted {
        channel: ImChannelRef,
        message_id: String,
    },
    ReactionAdded {
        channel: ImChannelRef,
        message_id: String,
        emoji: String,
        actor_display: String,
    },
}

/// Adapter contract. One impl per platform connector.
#[async_trait]
pub trait ImChannelAdapter: Send + Sync {
    /// Which platform this adapter serves.
    fn platform(&self) -> ImPlatform;

    /// Brief enumeration of channels the user has joined. Used by the
    /// UI to populate the channel picker.
    async fn list_channels(&self) -> Result<Vec<ImChannelRef>, ImAdapterError>;

    /// Fetch a window of recent messages from a channel.
    async fn fetch_recent(
        &self,
        channel: &ImChannelRef,
        max: u32,
    ) -> Result<Vec<ImMessage>, ImAdapterError>;

    /// Send an outbound message / reaction / typing indicator.
    async fn send(&self, outbound: &ImOutbound) -> Result<ImSendResult, ImAdapterError>;
}

/// Errors any adapter can surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImAdapterError {
    /// Adapter not authenticated for the requested operation.
    NotAuthenticated,
    /// Channel id doesn't exist or the user can't see it.
    ChannelNotFound(String),
    /// Platform returned a rate-limit error. Caller may retry.
    RateLimited { retry_after_secs: u32 },
    /// Transport / network failure.
    Transport(String),
    /// Anything else (kept open for adapter-specific cases).
    Other(String),
}

impl std::fmt::Display for ImAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAuthenticated => write!(f, "im adapter: not authenticated"),
            Self::ChannelNotFound(id) => write!(f, "im adapter: channel not found: {id}"),
            Self::RateLimited { retry_after_secs } => write!(
                f,
                "im adapter: rate limited (retry after {retry_after_secs}s)"
            ),
            Self::Transport(m) => write!(f, "im adapter: transport error: {m}"),
            Self::Other(m) => write!(f, "im adapter: {m}"),
        }
    }
}

impl std::error::Error for ImAdapterError {}

#[cfg(test)]
mod tests {
    use super::*;

    // ── ImPlatform ─────────────────────────────────────────────────

    #[test]
    fn platform_ids_distinct_including_custom() {
        let platforms = [
            ImPlatform::Slack,
            ImPlatform::Discord,
            ImPlatform::Telegram,
            ImPlatform::Sms,
            ImPlatform::WhatsApp,
            ImPlatform::WeChat,
            ImPlatform::Lark,
            ImPlatform::Teams,
            ImPlatform::Matrix,
            ImPlatform::Custom("signal".into()),
        ];
        let mut ids: Vec<_> = platforms.iter().map(|p| p.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 10);
        assert!(ImPlatform::Custom("x".into()).id().starts_with("custom:"));
    }

    #[test]
    fn platform_serde_tagged() {
        let v = serde_json::to_value(ImPlatform::Slack).unwrap();
        assert_eq!(v["platform"], "slack");
        let v = serde_json::to_value(ImPlatform::Custom("signal".into())).unwrap();
        assert_eq!(v["platform"], "custom");
        assert_eq!(v["subplatform"], "signal");
    }

    // ── ImChannelRef ───────────────────────────────────────────────

    #[test]
    fn channel_ref_factories() {
        let c = ImChannelRef::channel(ImPlatform::Slack, "C01");
        assert!(c.thread_id.is_none());
        let t = ImChannelRef::thread(ImPlatform::Slack, "C01", "1234.5678");
        assert_eq!(t.thread_id.as_deref(), Some("1234.5678"));
    }

    #[test]
    fn channel_ref_serde_skips_none_optional_fields() {
        let c = ImChannelRef::channel(ImPlatform::Discord, "abc");
        let json = serde_json::to_string(&c).unwrap();
        assert!(!json.contains("threadId"));
        assert!(!json.contains("label"));
    }

    // ── ImMessage ───────────────────────────────────────────────────

    #[test]
    fn im_message_roundtrip() {
        let m = ImMessage {
            message_id: "m1".into(),
            channel: ImChannelRef::channel(ImPlatform::Slack, "C01"),
            sender_display: "alice".into(),
            sender_id: Some("U01".into()),
            text: "hi".into(),
            sent_at: "2026-05-21T12:00:00Z".into(),
            edited: false,
            attachments_json: vec!["{\"kind\":\"file\"}".into()],
        };
        let json = serde_json::to_string(&m).unwrap();
        let back: ImMessage = serde_json::from_str(&json).unwrap();
        assert_eq!(m, back);
    }

    #[test]
    fn im_message_camel_case_keys() {
        let m = ImMessage {
            message_id: "m1".into(),
            channel: ImChannelRef::channel(ImPlatform::Slack, "C01"),
            sender_display: "a".into(),
            sender_id: None,
            text: "x".into(),
            sent_at: "t".into(),
            edited: false,
            attachments_json: vec![],
        };
        let v = serde_json::to_value(&m).unwrap();
        assert_eq!(v["messageId"], "m1");
        assert_eq!(v["senderDisplay"], "a");
        assert_eq!(v["sentAt"], "t");
        // None senderId skipped, empty attachments skipped, edited=false serialized.
        assert!(v.get("senderId").is_none());
        assert!(v.get("attachmentsJson").is_none());
        assert_eq!(v["edited"], false);
    }

    // ── ImOutbound ─────────────────────────────────────────────────

    #[test]
    fn outbound_serde_tag_kind() {
        let o = ImOutbound::Text {
            channel: ImChannelRef::channel(ImPlatform::Telegram, "@me"),
            text: "hello".into(),
        };
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["kind"], "text");
        assert_eq!(v["text"], "hello");

        let o = ImOutbound::Reaction {
            channel: ImChannelRef::channel(ImPlatform::Slack, "C01"),
            message_id: "m1".into(),
            emoji: "+1".into(),
        };
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["kind"], "reaction");
        assert_eq!(v["emoji"], "+1");

        let o = ImOutbound::Typing {
            channel: ImChannelRef::channel(ImPlatform::Slack, "C01"),
        };
        let v = serde_json::to_value(&o).unwrap();
        assert_eq!(v["kind"], "typing");
    }

    // ── ImEvent ────────────────────────────────────────────────────

    #[test]
    fn event_received_serde_roundtrip() {
        let msg = ImMessage {
            message_id: "m1".into(),
            channel: ImChannelRef::channel(ImPlatform::Slack, "C01"),
            sender_display: "a".into(),
            sender_id: None,
            text: "yo".into(),
            sent_at: "t".into(),
            edited: false,
            attachments_json: vec![],
        };
        let e = ImEvent::MessageReceived(msg.clone());
        let json = serde_json::to_string(&e).unwrap();
        let back: ImEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    #[test]
    fn event_deleted_carries_channel_and_msg_id() {
        let e = ImEvent::MessageDeleted {
            channel: ImChannelRef::channel(ImPlatform::Discord, "ch"),
            message_id: "m".into(),
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["kind"], "message_deleted");
        // serde `rename_all` on the enum applies to the variant TAG,
        // not to the inline struct field names — those stay snake_case
        // as written in Rust. `ImMessage` (a separate struct) keeps its
        // own `rename_all = "camelCase"` so its messageId field IS
        // camel-cased; this variant's fields are NOT.
        assert_eq!(v["message_id"], "m");
    }

    // ── ImAdapterError ─────────────────────────────────────────────

    #[test]
    fn adapter_error_display() {
        let cases = vec![
            (ImAdapterError::NotAuthenticated, "not authenticated"),
            (
                ImAdapterError::ChannelNotFound("C01".into()),
                "channel not found: C01",
            ),
            (
                ImAdapterError::RateLimited {
                    retry_after_secs: 60,
                },
                "retry after 60s",
            ),
            (
                ImAdapterError::Transport("dns failed".into()),
                "transport error: dns failed",
            ),
        ];
        for (e, contains) in cases {
            assert!(e.to_string().contains(contains), "missing in {}", e);
        }
    }
}
