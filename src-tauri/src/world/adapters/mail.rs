//! Mail (Gmail / IMAP / Exchange) projection adapter.
//!
//! Each email message becomes one `WorldEntity` of kind `Email`.
//! State tracks `read` / `archived` / `tombstoned` (deleted) flags
//! plus subject/from/preview metadata so the agent can reason about
//! inbox state without fetching message bodies.
//!
//! Provider wire-up (Gmail OAuth, IMAP polling) lives in M4-T7 commit
//! 2 — this pilot covers the typed event → entity translation only.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Inbound mail event. `message_id` is the provider's stable id
/// (Gmail uses RFC 822 Message-ID; IMAP uses UID-VALIDITY + UID).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EmailEvent {
    /// New message arrived in `mailbox` (inbox / a label / a folder).
    MessageReceived {
        message_id: String,
        mailbox: String,
        from_address: String,
        subject: String,
        body_preview: String,
        received_at: String,
    },
    /// User opened the message (clears the unread badge).
    MessageRead {
        message_id: String,
        read_at: String,
    },
    /// User archived the message (still searchable, no longer in inbox).
    MessageArchived {
        message_id: String,
        archived_at: String,
    },
    /// User deleted the message — tombstone the entity.
    MessageDeleted {
        message_id: String,
        deleted_at: String,
    },
}

/// Build a `WorldEntity` for one email message.
pub fn email_to_entity(
    message_id: &str,
    mailbox: &str,
    from_address: &str,
    subject: &str,
    body_preview: &str,
    is_read: bool,
    is_archived: bool,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("email:msg:{message_id}");
    let state = WorldEntityState::fresh(observed_at)
        .with_property("message_id", json!(message_id))
        .with_property("mailbox", json!(mailbox))
        .with_property("from_address", json!(from_address))
        .with_property("subject", json!(subject))
        .with_property("body_preview", json!(body_preview))
        .with_property("is_read", json!(is_read))
        .with_property("is_archived", json!(is_archived));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::Email, state)
}

/// Mail adapter.
pub struct MailAdapter {
    store: ProjectionStore,
}

impl MailAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    /// Preview length cap. Plain-text email previews are usually
    /// fine in 240 bytes; HTML-stripped previews can run long.
    const MAX_PREVIEW: usize = 240;

    fn truncate_preview(text: &str) -> String {
        let mut cut = Self::MAX_PREVIEW.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_string()
    }

    /// Process one event. Returns `true` when the projection changed.
    pub async fn handle(&self, event: EmailEvent) -> bool {
        match event {
            EmailEvent::MessageReceived {
                message_id,
                mailbox,
                from_address,
                subject,
                body_preview,
                received_at,
            } => {
                let preview = Self::truncate_preview(&body_preview);
                let entity = email_to_entity(
                    &message_id,
                    &mailbox,
                    &from_address,
                    &subject,
                    &preview,
                    false, // is_read
                    false, // is_archived
                    &received_at,
                );
                self.store.upsert(entity).await;
                true
            }
            EmailEvent::MessageRead {
                message_id,
                read_at,
            } => self.flip_flag(&message_id, "is_read", true, &read_at).await,
            EmailEvent::MessageArchived {
                message_id,
                archived_at,
            } => {
                self.flip_flag(&message_id, "is_archived", true, &archived_at)
                    .await
            }
            EmailEvent::MessageDeleted {
                message_id,
                deleted_at,
            } => {
                let key = format!("email:msg:{message_id}");
                self.store
                    .tombstone(&WorldEntityKind::Email, &key, &deleted_at)
                    .await
            }
        }
    }

    async fn flip_flag(
        &self,
        message_id: &str,
        flag: &str,
        value: bool,
        observed_at: &str,
    ) -> bool {
        let snap = self.store.snapshot();
        let key = format!("email:msg:{message_id}");
        let entity = match snap.get(&WorldEntityKind::Email, &key) {
            Some(e) => e.clone(),
            None => return false, // unknown message id — no-op
        };
        // Re-construct entity preserving prior fields.
        let mailbox = entity
            .state
            .properties
            .get("mailbox")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let from_addr = entity
            .state
            .properties
            .get("from_address")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let subject = entity
            .state
            .properties
            .get("subject")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let preview = entity
            .state
            .properties
            .get("body_preview")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let was_read = entity
            .state
            .properties
            .get("is_read")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let was_archived = entity
            .state
            .properties
            .get("is_archived")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let (is_read, is_archived) = match flag {
            "is_read" => (value, was_archived),
            "is_archived" => (was_read, value),
            _ => (was_read, was_archived),
        };
        let new_entity = email_to_entity(
            message_id,
            &mailbox,
            &from_addr,
            &subject,
            &preview,
            is_read,
            is_archived,
            observed_at,
        );
        self.store.upsert(new_entity).await;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn received(msg_id: &str, t: &str) -> EmailEvent {
        EmailEvent::MessageReceived {
            message_id: msg_id.into(),
            mailbox: "inbox".into(),
            from_address: "alice@example.com".into(),
            subject: "Hi".into(),
            body_preview: "Hello world".into(),
            received_at: t.into(),
        }
    }

    // ── event serde ────────────────────────────────────────────────

    #[test]
    fn event_tag_snake_case() {
        let v = serde_json::to_value(received("m1", "t0")).unwrap();
        assert_eq!(v["kind"], "message_received");
        let v = serde_json::to_value(EmailEvent::MessageDeleted {
            message_id: "m1".into(),
            deleted_at: "t".into(),
        })
        .unwrap();
        assert_eq!(v["kind"], "message_deleted");
    }

    #[test]
    fn event_roundtrips_read() {
        let e = EmailEvent::MessageRead {
            message_id: "m1".into(),
            read_at: "t1".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: EmailEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }

    // ── entity factory ─────────────────────────────────────────────

    #[test]
    fn entity_id_uses_email_namespace() {
        let e = email_to_entity("m1", "inbox", "a@b.com", "Hi", "...", false, false, "t");
        assert_eq!(e.r#ref.id, "email:msg:m1");
        assert_eq!(e.kind, WorldEntityKind::Email);
        assert_eq!(e.state.properties.get("is_read"), Some(&json!(false)));
        assert_eq!(e.state.properties.get("is_archived"), Some(&json!(false)));
    }

    // ── adapter: receive ──────────────────────────────────────────

    #[tokio::test]
    async fn receive_creates_unread_unarchived_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        adapter.handle(received("m1", "t0")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Email, "email:msg:m1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("is_read"), Some(&json!(false)));
        assert_eq!(e.state.properties.get("is_archived"), Some(&json!(false)));
        assert_eq!(e.state.properties.get("subject"), Some(&json!("Hi")));
    }

    #[tokio::test]
    async fn receive_truncates_long_preview_utf8_safe() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        let big = "你好世界，这是一封很长的邮件。".repeat(100);
        adapter
            .handle(EmailEvent::MessageReceived {
                message_id: "m1".into(),
                mailbox: "inbox".into(),
                from_address: "x@y".into(),
                subject: "Long".into(),
                body_preview: big,
                received_at: "t".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Email, "email:msg:m1")
            .cloned()
            .unwrap();
        let preview = e
            .state
            .properties
            .get("body_preview")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert!(preview.len() <= MailAdapter::MAX_PREVIEW);
        assert!(preview.starts_with("你"));
    }

    // ── adapter: read flag flip ────────────────────────────────────

    #[tokio::test]
    async fn read_flips_is_read_preserves_other_fields() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        adapter.handle(received("m1", "t0")).await;
        adapter
            .handle(EmailEvent::MessageRead {
                message_id: "m1".into(),
                read_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Email, "email:msg:m1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("is_read"), Some(&json!(true)));
        assert_eq!(e.state.properties.get("is_archived"), Some(&json!(false)));
        // Subject etc preserved.
        assert_eq!(e.state.properties.get("subject"), Some(&json!("Hi")));
    }

    #[tokio::test]
    async fn read_unknown_message_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        let changed = adapter
            .handle(EmailEvent::MessageRead {
                message_id: "nope".into(),
                read_at: "t".into(),
            })
            .await;
        assert!(!changed);
    }

    // ── adapter: archive ──────────────────────────────────────────

    #[tokio::test]
    async fn archive_flips_is_archived_keeps_read() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        adapter.handle(received("m1", "t0")).await;
        adapter
            .handle(EmailEvent::MessageRead {
                message_id: "m1".into(),
                read_at: "t1".into(),
            })
            .await;
        adapter
            .handle(EmailEvent::MessageArchived {
                message_id: "m1".into(),
                archived_at: "t2".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Email, "email:msg:m1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("is_read"), Some(&json!(true)));
        assert_eq!(e.state.properties.get("is_archived"), Some(&json!(true)));
    }

    // ── adapter: delete ───────────────────────────────────────────

    #[tokio::test]
    async fn delete_tombstones_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = MailAdapter::new(store.clone());
        adapter.handle(received("m1", "t0")).await;
        let changed = adapter
            .handle(EmailEvent::MessageDeleted {
                message_id: "m1".into(),
                deleted_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Email, "email:msg:m1")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }
}
