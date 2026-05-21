//! Calendar (Google Calendar / iCloud / Outlook) projection adapter.
//!
//! Each scheduled event becomes one `WorldEntity` of kind
//! `CalendarEvent`. State carries title / start / end / location /
//! attendees plus a `cancelled` flag.
//!
//! Provider wire-up (Google Calendar API, CalDAV) lives in M4-T7
//! commit 2.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Inbound calendar change event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CalendarChangeEvent {
    /// New event scheduled.
    EventScheduled {
        event_id: String,
        title: String,
        start_at: String,
        end_at: String,
        location: Option<String>,
        attendees: Vec<String>,
        observed_at: String,
    },
    /// Event modified — title / time / attendees changed.
    EventUpdated {
        event_id: String,
        title: String,
        start_at: String,
        end_at: String,
        location: Option<String>,
        attendees: Vec<String>,
        observed_at: String,
    },
    /// Event cancelled (still in calendar history but flagged).
    EventCancelled {
        event_id: String,
        cancelled_at: String,
    },
    /// Event fully deleted from the calendar — tombstone.
    EventDeleted {
        event_id: String,
        deleted_at: String,
    },
}

/// Build a calendar `WorldEntity`.
pub fn calendar_event_to_entity(
    event_id: &str,
    title: &str,
    start_at: &str,
    end_at: &str,
    location: Option<&str>,
    attendees: &[String],
    cancelled: bool,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("calendar:event:{event_id}");
    let mut state = WorldEntityState::fresh(observed_at)
        .with_property("event_id", json!(event_id))
        .with_property("title", json!(title))
        .with_property("start_at", json!(start_at))
        .with_property("end_at", json!(end_at))
        .with_property("attendees", json!(attendees))
        .with_property("cancelled", json!(cancelled));
    if let Some(loc) = location {
        state = state.with_property("location", json!(loc));
    }
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::CalendarEvent, state)
}

pub struct CalendarAdapter {
    store: ProjectionStore,
}

impl CalendarAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    pub async fn handle(&self, event: CalendarChangeEvent) -> bool {
        match event {
            CalendarChangeEvent::EventScheduled {
                event_id,
                title,
                start_at,
                end_at,
                location,
                attendees,
                observed_at,
            }
            | CalendarChangeEvent::EventUpdated {
                event_id,
                title,
                start_at,
                end_at,
                location,
                attendees,
                observed_at,
            } => {
                let entity = calendar_event_to_entity(
                    &event_id,
                    &title,
                    &start_at,
                    &end_at,
                    location.as_deref(),
                    &attendees,
                    false,
                    &observed_at,
                );
                self.store.upsert(entity).await;
                true
            }
            CalendarChangeEvent::EventCancelled {
                event_id,
                cancelled_at,
            } => {
                // Cancellation = re-upsert with cancelled=true,
                // preserving title/time. If the event was unknown,
                // create a minimal cancelled stub.
                let snap = self.store.snapshot();
                let key = format!("calendar:event:{event_id}");
                let (title, start, end, location, attendees) =
                    match snap.get(&WorldEntityKind::CalendarEvent, &key) {
                        Some(e) => (
                            e.state
                                .properties
                                .get("title")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            e.state
                                .properties
                                .get("start_at")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            e.state
                                .properties
                                .get("end_at")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                .to_string(),
                            e.state
                                .properties
                                .get("location")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string()),
                            e.state
                                .properties
                                .get("attendees")
                                .and_then(|v| v.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|a| a.as_str().map(String::from))
                                        .collect::<Vec<_>>()
                                })
                                .unwrap_or_default(),
                        ),
                        None => (
                            String::new(),
                            String::new(),
                            String::new(),
                            None,
                            Vec::new(),
                        ),
                    };
                let entity = calendar_event_to_entity(
                    &event_id,
                    &title,
                    &start,
                    &end,
                    location.as_deref(),
                    &attendees,
                    true, // cancelled
                    &cancelled_at,
                );
                self.store.upsert(entity).await;
                true
            }
            CalendarChangeEvent::EventDeleted {
                event_id,
                deleted_at,
            } => {
                let key = format!("calendar:event:{event_id}");
                self.store
                    .tombstone(&WorldEntityKind::CalendarEvent, &key, &deleted_at)
                    .await
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scheduled(id: &str, title: &str) -> CalendarChangeEvent {
        CalendarChangeEvent::EventScheduled {
            event_id: id.into(),
            title: title.into(),
            start_at: "2026-05-21T10:00:00Z".into(),
            end_at: "2026-05-21T11:00:00Z".into(),
            location: Some("Zoom".into()),
            attendees: vec!["alice@x".into(), "bob@y".into()],
            observed_at: "t0".into(),
        }
    }

    // ── entity factory ─────────────────────────────────────────────

    #[test]
    fn entity_id_uses_calendar_namespace() {
        let e = calendar_event_to_entity(
            "e1",
            "Standup",
            "10:00",
            "10:30",
            Some("HQ"),
            &["a@x".into()],
            false,
            "t",
        );
        assert_eq!(e.r#ref.id, "calendar:event:e1");
        assert_eq!(e.kind, WorldEntityKind::CalendarEvent);
        assert_eq!(e.state.properties.get("title"), Some(&json!("Standup")));
        assert_eq!(e.state.properties.get("location"), Some(&json!("HQ")));
        assert_eq!(e.state.properties.get("cancelled"), Some(&json!(false)));
    }

    #[test]
    fn entity_skips_location_when_none() {
        let e = calendar_event_to_entity(
            "e1",
            "T",
            "10:00",
            "10:30",
            None,
            &[],
            false,
            "t",
        );
        assert!(e.state.properties.get("location").is_none());
    }

    // ── serde ──────────────────────────────────────────────────────

    #[test]
    fn event_serde_tag_snake_case() {
        let v = serde_json::to_value(scheduled("e1", "X")).unwrap();
        assert_eq!(v["kind"], "event_scheduled");
        let v = serde_json::to_value(CalendarChangeEvent::EventCancelled {
            event_id: "e1".into(),
            cancelled_at: "t".into(),
        })
        .unwrap();
        assert_eq!(v["kind"], "event_cancelled");
    }

    // ── adapter: schedule ──────────────────────────────────────────

    #[tokio::test]
    async fn schedule_creates_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        assert!(adapter.handle(scheduled("e1", "Standup")).await);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:e1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("title"), Some(&json!("Standup")));
        assert_eq!(e.state.properties.get("cancelled"), Some(&json!(false)));
    }

    // ── adapter: update ────────────────────────────────────────────

    #[tokio::test]
    async fn update_replaces_existing() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        adapter.handle(scheduled("e1", "Original")).await;
        let upd = CalendarChangeEvent::EventUpdated {
            event_id: "e1".into(),
            title: "Renamed".into(),
            start_at: "2026-05-21T10:00:00Z".into(),
            end_at: "2026-05-21T12:00:00Z".into(),
            location: None,
            attendees: vec!["alice@x".into()],
            observed_at: "t1".into(),
        };
        adapter.handle(upd).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:e1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("title"), Some(&json!("Renamed")));
        assert!(e.state.properties.get("location").is_none());
    }

    // ── adapter: cancel ────────────────────────────────────────────

    #[tokio::test]
    async fn cancel_sets_flag_preserves_title() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        adapter.handle(scheduled("e1", "Important")).await;
        adapter
            .handle(CalendarChangeEvent::EventCancelled {
                event_id: "e1".into(),
                cancelled_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:e1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("cancelled"), Some(&json!(true)));
        assert_eq!(e.state.properties.get("title"), Some(&json!("Important")));
        // Not tombstoned — cancelled events stay in projection.
        assert!(!e.is_tombstoned());
    }

    #[tokio::test]
    async fn cancel_unknown_creates_minimal_stub() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        adapter
            .handle(CalendarChangeEvent::EventCancelled {
                event_id: "ghost".into(),
                cancelled_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:ghost")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("cancelled"), Some(&json!(true)));
        assert_eq!(e.state.properties.get("title"), Some(&json!("")));
    }

    // ── adapter: delete ────────────────────────────────────────────

    #[tokio::test]
    async fn delete_tombstones_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        adapter.handle(scheduled("e1", "X")).await;
        let changed = adapter
            .handle(CalendarChangeEvent::EventDeleted {
                event_id: "e1".into(),
                deleted_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:e1")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }

    #[tokio::test]
    async fn delete_unknown_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        let changed = adapter
            .handle(CalendarChangeEvent::EventDeleted {
                event_id: "nope".into(),
                deleted_at: "t".into(),
            })
            .await;
        assert!(!changed);
    }

    #[tokio::test]
    async fn attendees_array_round_trips_through_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = CalendarAdapter::new(store.clone());
        adapter.handle(scheduled("e1", "Meeting")).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::CalendarEvent, "calendar:event:e1")
            .cloned()
            .unwrap();
        let attendees = e
            .state
            .properties
            .get("attendees")
            .and_then(|v| v.as_array())
            .unwrap()
            .clone();
        assert_eq!(attendees.len(), 2);
    }
}
