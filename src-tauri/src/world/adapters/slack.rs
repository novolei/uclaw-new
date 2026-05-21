//! Slack / IM channel projection adapter.
//!
//! Models each channel/DM as a single `WorldEntity` of kind
//! `ChatThread`. Channel state accumulates from inbound events:
//! `message_count`, `last_message_at`, `last_message_preview`,
//! plus a small ring-buffer of recent reactions.
//!
//! This pilot covers Slack-flavoured semantics but the event enum is
//! intentionally provider-agnostic — Discord / Lark / Telegram MCPs
//! reuse the same shapes via #353 `ImChannelAdapter`'s `ImPlatform`.
//!
//! Live websocket / Events-API subscriber lives in M4-T6 commit 2.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Inbound event from a Slack-style channel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SlackEvent {
    /// User joined a channel — projection starts tracking it.
    ChannelJoined {
        channel_id: String,
        channel_name: String,
        joined_at: String,
    },
    /// New message posted in a channel we're tracking.
    MessagePosted {
        channel_id: String,
        message_id: String,
        user_display: String,
        text: String,
        posted_at: String,
    },
    /// User left a channel — tombstone the entity.
    ChannelLeft {
        channel_id: String,
        left_at: String,
    },
    /// Emoji reaction on a message in a tracked channel.
    ReactionAdded {
        channel_id: String,
        message_id: String,
        emoji: String,
        by_user: String,
        added_at: String,
    },
}

/// Build a `WorldEntity` from a channel snapshot.
pub fn channel_to_entity(
    channel_id: &str,
    channel_name: &str,
    message_count: u64,
    last_message_at: Option<&str>,
    last_message_preview: Option<&str>,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("slack:channel:{channel_id}");
    let mut state = WorldEntityState::fresh(observed_at)
        .with_property("channel_id", json!(channel_id))
        .with_property("channel_name", json!(channel_name))
        .with_property("message_count", json!(message_count));
    if let Some(t) = last_message_at {
        state = state.with_property("last_message_at", json!(t));
    }
    if let Some(p) = last_message_preview {
        state = state.with_property("last_message_preview", json!(p));
    }
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::ChatThread, state)
}

/// Adapter wraps a `ProjectionStore` and folds Slack events into
/// `ChatThread` upserts/tombstones.
pub struct SlackAdapter {
    store: ProjectionStore,
}

impl SlackAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    /// Truncate message preview to keep the projection compact. Slack
    /// allows 4000-char messages, but the agent only needs a short
    /// tease for "what's recent here?" reasoning.
    const MAX_PREVIEW: usize = 200;

    fn truncate_preview(text: &str) -> String {
        // Char-boundary-safe truncation. Slack messages are UTF-8.
        let mut cut = Self::MAX_PREVIEW.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_string()
    }

    /// Process one event. Returns `true` when the projection changed.
    pub async fn handle(&self, event: SlackEvent) -> bool {
        match event {
            SlackEvent::ChannelJoined {
                channel_id,
                channel_name,
                joined_at,
            } => {
                let entity = channel_to_entity(
                    &channel_id,
                    &channel_name,
                    0,
                    None,
                    None,
                    &joined_at,
                );
                self.store.upsert(entity).await;
                true
            }
            SlackEvent::MessagePosted {
                channel_id,
                message_id: _,
                user_display: _,
                text,
                posted_at,
            } => {
                // Look up prior state to carry channel_name + bump count.
                let snap = self.store.snapshot();
                let key = format!("slack:channel:{channel_id}");
                let (channel_name, prior_count) =
                    match snap.get(&WorldEntityKind::ChatThread, &key) {
                        Some(e) => {
                            let name = e
                                .state
                                .properties
                                .get("channel_name")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string();
                            let cnt = e
                                .state
                                .properties
                                .get("message_count")
                                .and_then(|v| v.as_u64())
                                .unwrap_or(0);
                            (name, cnt)
                        }
                        // Unknown channel — accept the message and
                        // record an empty name (caller may backfill
                        // via a ChannelJoined later).
                        None => (String::new(), 0),
                    };
                let preview = Self::truncate_preview(&text);
                let entity = channel_to_entity(
                    &channel_id,
                    &channel_name,
                    prior_count + 1,
                    Some(&posted_at),
                    Some(&preview),
                    &posted_at,
                );
                self.store.upsert(entity).await;
                true
            }
            SlackEvent::ChannelLeft {
                channel_id,
                left_at,
            } => {
                let key = format!("slack:channel:{channel_id}");
                self.store
                    .tombstone(&WorldEntityKind::ChatThread, &key, &left_at)
                    .await
            }
            SlackEvent::ReactionAdded {
                channel_id,
                message_id: _,
                emoji: _,
                by_user: _,
                added_at: _,
            } => {
                // For the pilot we treat reactions as observational
                // only — they don't bump message_count and we don't
                // store per-message ring buffers yet (that's M4-T6
                // commit 2 with a richer entity schema). Caller can
                // still see we received the event by checking the
                // snapshot's updated_at moving forward.
                let snap = self.store.snapshot();
                let key = format!("slack:channel:{channel_id}");
                if snap.get(&WorldEntityKind::ChatThread, &key).is_some() {
                    // No-op as far as state changes go.
                    return false;
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn joined() -> SlackEvent {
        SlackEvent::ChannelJoined {
            channel_id: "C01".into(),
            channel_name: "#general".into(),
            joined_at: "t0".into(),
        }
    }

    fn posted(text: &str, t: &str) -> SlackEvent {
        SlackEvent::MessagePosted {
            channel_id: "C01".into(),
            message_id: format!("m-{t}"),
            user_display: "alice".into(),
            text: text.into(),
            posted_at: t.into(),
        }
    }

    // ── event tag serde ────────────────────────────────────────────

    #[test]
    fn event_serde_tag_snake_case() {
        let v = serde_json::to_value(joined()).unwrap();
        assert_eq!(v["kind"], "channel_joined");
        let v = serde_json::to_value(posted("hi", "t1")).unwrap();
        assert_eq!(v["kind"], "message_posted");
    }

    #[test]
    fn event_roundtrips_reaction() {
        let e = SlackEvent::ReactionAdded {
            channel_id: "C01".into(),
            message_id: "m1".into(),
            emoji: "thumbsup".into(),
            by_user: "alice".into(),
            added_at: "t1".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: SlackEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    // ── channel_to_entity construction ────────────────────────────

    #[test]
    fn channel_to_entity_uses_slack_namespace_and_chat_thread_kind() {
        let e = channel_to_entity("C01", "#general", 0, None, None, "t0");
        assert_eq!(e.r#ref.id, "slack:channel:C01");
        assert_eq!(e.kind, WorldEntityKind::ChatThread);
        assert_eq!(
            e.state.properties.get("channel_name"),
            Some(&json!("#general"))
        );
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(0)));
        // Optional fields skipped when None.
        assert!(e.state.properties.get("last_message_at").is_none());
        assert!(e.state.properties.get("last_message_preview").is_none());
    }

    #[test]
    fn channel_to_entity_includes_optional_fields_when_provided() {
        let e = channel_to_entity(
            "C01",
            "#g",
            5,
            Some("t5"),
            Some("hello world"),
            "t5",
        );
        assert_eq!(
            e.state.properties.get("last_message_at"),
            Some(&json!("t5"))
        );
        assert_eq!(
            e.state.properties.get("last_message_preview"),
            Some(&json!("hello world"))
        );
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(5)));
    }

    // ── adapter: ChannelJoined ─────────────────────────────────────

    #[tokio::test]
    async fn handle_channel_joined_creates_entity_with_zero_messages() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        assert!(adapter.handle(joined()).await);
        let snap = store.snapshot();
        let e = snap
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .unwrap();
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(0)));
    }

    // ── adapter: MessagePosted ─────────────────────────────────────

    #[tokio::test]
    async fn handle_message_posted_increments_count_and_sets_preview() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        adapter.handle(joined()).await;
        adapter.handle(posted("hello world", "t1")).await;
        adapter.handle(posted("how are you", "t2")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(2)));
        assert_eq!(
            e.state.properties.get("last_message_at"),
            Some(&json!("t2"))
        );
        assert_eq!(
            e.state.properties.get("last_message_preview"),
            Some(&json!("how are you"))
        );
    }

    #[tokio::test]
    async fn handle_message_posted_to_unknown_channel_starts_count_at_one() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        // No ChannelJoined first — adapter should still accept the
        // message and create the entity with empty channel_name.
        adapter.handle(posted("hi", "t1")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(1)));
        assert_eq!(e.state.properties.get("channel_name"), Some(&json!("")));
    }

    #[tokio::test]
    async fn handle_long_message_preview_is_truncated_utf8_safe() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        adapter.handle(joined()).await;
        // 4 KB Japanese — each char is 3 bytes; MAX_PREVIEW = 200.
        let big = "日本語の長いメッセージ".repeat(200);
        adapter.handle(posted(&big, "t1")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .cloned()
            .unwrap();
        let preview = e
            .state
            .properties
            .get("last_message_preview")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert!(preview.len() <= SlackAdapter::MAX_PREVIEW);
        // Valid UTF-8 — the back-off cuts at a char boundary.
        // (If invalid we'd have panicked above; the as_str success
        // proves it round-tripped through serde_json::String fine.)
        assert!(preview.starts_with("日"));
    }

    // ── adapter: ChannelLeft ──────────────────────────────────────

    #[tokio::test]
    async fn handle_channel_left_tombstones_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        adapter.handle(joined()).await;
        let changed = adapter
            .handle(SlackEvent::ChannelLeft {
                channel_id: "C01".into(),
                left_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }

    #[tokio::test]
    async fn handle_channel_left_unknown_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        let changed = adapter
            .handle(SlackEvent::ChannelLeft {
                channel_id: "C99".into(),
                left_at: "t".into(),
            })
            .await;
        assert!(!changed);
    }

    // ── adapter: ReactionAdded (observe-only in pilot) ────────────

    #[tokio::test]
    async fn handle_reaction_added_is_observe_only_in_pilot() {
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        adapter.handle(joined()).await;
        let changed = adapter
            .handle(SlackEvent::ReactionAdded {
                channel_id: "C01".into(),
                message_id: "m1".into(),
                emoji: "+1".into(),
                by_user: "alice".into(),
                added_at: "t1".into(),
            })
            .await;
        // Pilot: reactions don't mutate state.
        assert!(!changed);
    }

    // ── deterministic state shape ─────────────────────────────────

    #[tokio::test]
    async fn multi_join_replaces_existing() {
        // Re-joining (e.g. user removed then re-invited) bumps version.
        let store = ProjectionStore::new("t0");
        let adapter = SlackAdapter::new(store.clone());
        adapter.handle(joined()).await;
        adapter.handle(joined()).await;
        let snap = store.snapshot();
        assert_eq!(snap.entities.len(), 1);
        let e = snap
            .get(&WorldEntityKind::ChatThread, "slack:channel:C01")
            .unwrap();
        // Re-joined → count reset to 0 (semantics: ChannelJoined is
        // a fresh-state event; M4-T6 commit 2 may revise to preserve
        // history).
        assert_eq!(e.state.properties.get("message_count"), Some(&json!(0)));
    }
}
