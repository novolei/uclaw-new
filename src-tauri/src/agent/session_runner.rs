//! Shared Agent-session runner contract.
//!
//! This module is the boundary we extract toward from `send_agent_message`.
//! IM close-loop, desktop Agent chat, automation-triggered chat sessions, and
//! future plugin transports should enter the app through this shape instead of
//! each transport building a private agent loop.

use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::channels::types::{ReplyHandle, StreamingHandle};
use crate::im_channels::{
    CloseLoopSink, MessageCapabilityProfile, MessageFlowEnvelope, MessageFlowOrigin,
};

/// Origin metadata for one global Agent-session turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum ChatRunSource {
    Desktop,
    Im {
        origin: MessageFlowOrigin,
        flow_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        sender_id: Option<String>,
    },
    AutomationSpec {
        spec_id: String,
        identity_key: String,
    },
    Plugin {
        plugin_id: String,
        flow_id: String,
    },
}

/// Serializable sink descriptor used for audit, projection, and tests.
///
/// Runtime-only objects such as `ReplyHandle` live in [`ChatRunReplySink`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "sink", rename_all = "snake_case")]
pub enum ChatRunSinkDescriptor {
    Desktop {
        conversation_id: String,
    },
    Im {
        platform_id: String,
        channel_id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        thread_id: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply_to_message_id: Option<String>,
    },
    None,
}

impl From<&CloseLoopSink> for ChatRunSinkDescriptor {
    fn from(sink: &CloseLoopSink) -> Self {
        match sink {
            CloseLoopSink::Desktop { conversation_id } => Self::Desktop {
                conversation_id: conversation_id.clone(),
            },
            CloseLoopSink::Im {
                channel,
                reply_to_message_id,
            } => Self::Im {
                platform_id: channel.platform.id(),
                channel_id: channel.channel_id.clone(),
                thread_id: channel.thread_id.clone(),
                reply_to_message_id: reply_to_message_id.clone(),
            },
            CloseLoopSink::None => Self::None,
        }
    }
}

/// Runtime sink used by the extracted runner implementation.
#[derive(Clone)]
pub enum ChatRunReplySink {
    DesktopEvents,
    Im {
        reply_handle: Arc<ReplyHandle>,
        streaming_handle: Option<Arc<dyn StreamingHandle>>,
    },
    None,
}

/// Input for a single global Agent-session turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRunInput {
    pub session_id: String,
    pub user_message: String,
    pub source: ChatRunSource,
    pub sink: ChatRunSinkDescriptor,
    pub capability_profile: MessageCapabilityProfile,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub metadata: serde_json::Value,
}

impl ChatRunInput {
    pub fn from_message_flow(envelope: MessageFlowEnvelope, session_id: String) -> Self {
        Self {
            session_id,
            user_message: envelope.text,
            source: ChatRunSource::Im {
                origin: envelope.origin,
                flow_id: envelope.flow_id,
                sender_id: envelope.sender_id,
            },
            sink: ChatRunSinkDescriptor::from(&envelope.sink),
            capability_profile: envelope.capability_profile,
            prompt_id: None,
            model_id: None,
            workspace_id: None,
            metadata: envelope.metadata,
        }
    }
}

/// Output from a completed Agent-session turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatRunOutcome {
    pub session_id: String,
    pub response_text: String,
    pub assistant_message_id: Option<String>,
    pub sink: ChatRunSinkDescriptor,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::im_channels::{ImChannelRef, ImMessage, ImPlatform, MessageFlowTarget};

    #[test]
    fn sink_descriptor_from_im_close_loop_sink() {
        let sink = CloseLoopSink::Im {
            channel: ImChannelRef::thread(ImPlatform::Lark, "oc_1", "om_parent"),
            reply_to_message_id: Some("om_reply".into()),
        };
        let descriptor = ChatRunSinkDescriptor::from(&sink);

        assert_eq!(
            descriptor,
            ChatRunSinkDescriptor::Im {
                platform_id: "lark".into(),
                channel_id: "oc_1".into(),
                thread_id: Some("om_parent".into()),
                reply_to_message_id: Some("om_reply".into()),
            }
        );
    }

    #[test]
    fn chat_run_input_from_message_flow_uses_app_session_target() {
        let message = ImMessage {
            message_id: "om_1".into(),
            channel: ImChannelRef::channel(ImPlatform::Lark, "oc_1"),
            sender_display: "Alice".into(),
            sender_id: Some("ou_1".into()),
            text: "hello".into(),
            sent_at: "2026-05-26T12:00:00Z".into(),
            edited: false,
            attachments_json: Vec::new(),
        };
        let envelope = MessageFlowEnvelope::from_im_message(
            message,
            MessageFlowTarget::AppSession {
                session_id: "sess1".into(),
            },
            MessageCapabilityProfile::default(),
        );
        let input = ChatRunInput::from_message_flow(envelope, "sess1".into());

        assert_eq!(input.session_id, "sess1");
        assert_eq!(input.user_message, "hello");
        assert_eq!(
            input.sink,
            ChatRunSinkDescriptor::Im {
                platform_id: "lark".into(),
                channel_id: "oc_1".into(),
                thread_id: None,
                reply_to_message_id: Some("om_1".into()),
            }
        );
        assert!(matches!(
            input.source,
            ChatRunSource::Im {
                origin: MessageFlowOrigin::Im(ImPlatform::Lark),
                flow_id,
                sender_id: Some(_),
            } if flow_id == "om_1"
        ));
    }
}
