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

/// Where a communication turn came from. This is intentionally broader than
/// IM so the same close-loop contract can carry desktop chat, automation
/// escalations, and plugin-defined transports.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "origin", content = "detail", rename_all = "snake_case")]
pub enum MessageFlowOrigin {
    Desktop,
    Im(ImPlatform),
    Automation,
    System,
    Plugin(String),
}

/// The app-level destination selected for a communication turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "target", rename_all = "snake_case")]
pub enum MessageFlowTarget {
    /// Route to the normal uClaw app session/chat runtime.
    AppSession { session_id: String },
    /// Route to an automation trigger/spec matcher.
    AutomationTrigger {
        space_id: String,
        channel_instance_id: String,
    },
    /// Publish as a notification only; no agent turn is expected.
    Notification,
}

/// Where close-loop output should go after the app handles a turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sink", rename_all = "snake_case")]
pub enum CloseLoopSink {
    /// Return output into an IM channel/thread. `reply_to_message_id`
    /// is platform-specific and optional because some transports only know
    /// the channel, not the precise parent message.
    Im {
        channel: ImChannelRef,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to_message_id: Option<String>,
    },
    /// Return output to a local desktop conversation.
    Desktop { conversation_id: String },
    /// Fire-and-forget: useful for one-way notifications or audit-only flows.
    None,
}

/// Capability exposure for one communication turn. Transports can keep this
/// compact and policy code can translate it into actual tool/profile exposure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageCapabilityProfile {
    /// If empty, no non-core tools are exposed to the caller by default.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_allowlist: Vec<String>,
    /// Whether runtime-discovered MCP tools may be exposed for this turn.
    #[serde(default)]
    pub mcp_enabled: bool,
    /// Whether read-only skill discovery/loading is available.
    #[serde(default = "default_skills_enabled")]
    pub skills_enabled: bool,
    /// Whether skill authoring/installing tools may be exposed.
    #[serde(default)]
    pub skill_write_enabled: bool,
}

fn default_skills_enabled() -> bool {
    true
}

impl Default for MessageCapabilityProfile {
    fn default() -> Self {
        Self {
            tool_allowlist: Vec::new(),
            mcp_enabled: false,
            skills_enabled: true,
            skill_write_enabled: false,
        }
    }
}

/// A normalized communication envelope. Adapters convert platform-specific
/// events into this shape before the app chooses a route. This keeps the
/// transport layer pluggable and lets IM act as one global communication
/// protocol rather than a Feishu/automation-specific path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MessageFlowEnvelope {
    /// Stable id for dedup/audit. For IM this is usually the platform message id.
    pub flow_id: String,
    pub origin: MessageFlowOrigin,
    pub target: MessageFlowTarget,
    pub sink: CloseLoopSink,
    pub text: String,
    pub sender_display: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sender_id: Option<String>,
    pub capability_profile: MessageCapabilityProfile,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl MessageFlowEnvelope {
    pub fn from_im_message(
        message: ImMessage,
        target: MessageFlowTarget,
        capability_profile: MessageCapabilityProfile,
    ) -> Self {
        Self {
            flow_id: message.message_id.clone(),
            origin: MessageFlowOrigin::Im(message.channel.platform.clone()),
            target,
            sink: CloseLoopSink::Im {
                channel: message.channel,
                reply_to_message_id: Some(message.message_id),
            },
            text: message.text,
            sender_display: message.sender_display,
            sender_id: message.sender_id,
            capability_profile,
            metadata: serde_json::json!({
                "sentAt": message.sent_at,
                "edited": message.edited,
                "attachmentsJson": message.attachments_json,
            }),
        }
    }
}

/// Outbound message — what we ask the adapter to send.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImOutbound {
    /// Plain text reply. Adapters that support markdown convert as needed.
    Text { channel: ImChannelRef, text: String },
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

    // ── MessageFlowEnvelope ────────────────────────────────────────

    #[test]
    fn message_flow_from_im_message_preserves_sink_and_policy() {
        let m = ImMessage {
            message_id: "m1".into(),
            channel: ImChannelRef::thread(ImPlatform::Lark, "oc_1", "om_parent"),
            sender_display: "alice".into(),
            sender_id: Some("ou_1".into()),
            text: "help".into(),
            sent_at: "2026-05-26T12:00:00Z".into(),
            edited: false,
            attachments_json: vec!["{\"kind\":\"image\"}".into()],
        };
        let profile = MessageCapabilityProfile {
            tool_allowlist: vec!["skill_search".into(), "load_skill".into()],
            mcp_enabled: false,
            skills_enabled: true,
            skill_write_enabled: false,
        };
        let envelope = MessageFlowEnvelope::from_im_message(
            m,
            MessageFlowTarget::AppSession {
                session_id: "sess1".into(),
            },
            profile.clone(),
        );

        assert_eq!(envelope.flow_id, "m1");
        assert_eq!(envelope.origin, MessageFlowOrigin::Im(ImPlatform::Lark));
        assert_eq!(envelope.text, "help");
        assert_eq!(envelope.capability_profile, profile);
        assert_eq!(envelope.metadata["sentAt"], "2026-05-26T12:00:00Z");
        assert_eq!(
            envelope.metadata["attachmentsJson"][0],
            "{\"kind\":\"image\"}"
        );
        match envelope.sink {
            CloseLoopSink::Im {
                channel,
                reply_to_message_id,
            } => {
                assert_eq!(channel.platform, ImPlatform::Lark);
                assert_eq!(channel.channel_id, "oc_1");
                assert_eq!(channel.thread_id.as_deref(), Some("om_parent"));
                assert_eq!(reply_to_message_id.as_deref(), Some("m1"));
            }
            other => panic!("expected IM sink, got {:?}", other),
        }
    }

    #[test]
    fn capability_profile_defaults_to_skills_without_mcp_or_writes() {
        let profile = MessageCapabilityProfile::default();
        assert!(profile.tool_allowlist.is_empty());
        assert!(!profile.mcp_enabled);
        assert!(profile.skills_enabled);
        assert!(!profile.skill_write_enabled);

        let json = serde_json::to_string(&profile).unwrap();
        let back: MessageCapabilityProfile = serde_json::from_str(&json).unwrap();
        assert_eq!(profile, back);
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
