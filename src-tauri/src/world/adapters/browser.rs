//! Browser tab projection adapter.
//!
//! Translates Chrome MCP / browser-extension push events into
//! `WorldEntity`s of kind `BrowserPage`.
//!
//! This pilot ships the **inbound event shape** (`BrowserTabEvent`) +
//! the projector that turns events into entities + the wrapper
//! adapter that pushes into `ProjectionStore`.
//!
//! M4-T5 commit 2 will plug in the actual Chrome MCP websocket
//! consumer (the existing `mcp` module's chrome integration).

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Push event from the browser. One per tab state change.
///
/// `tab_id` is the browser-assigned tab id (Chrome `tabs.Tab.id`).
/// `window_id` lets the projection group tabs by window.
/// `is_active` is true when the tab is the active tab in its window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserTabEvent {
    TabOpened {
        tab_id: i64,
        window_id: i64,
        url: String,
        title: String,
        is_active: bool,
        opened_at: String,
    },
    TabUpdated {
        tab_id: i64,
        window_id: i64,
        url: String,
        title: String,
        is_active: bool,
        observed_at: String,
    },
    TabClosed {
        tab_id: i64,
        closed_at: String,
    },
    TabActivated {
        tab_id: i64,
        window_id: i64,
        observed_at: String,
    },
}

impl BrowserTabEvent {
    /// `true` for events that produce / update an entity. `TabClosed`
    /// produces a tombstone (separate code path).
    pub fn is_upsert(&self) -> bool {
        !matches!(self, Self::TabClosed { .. })
    }
}

/// Construct a `WorldEntity` from a tab observation.
pub fn tab_entity(
    tab_id: i64,
    window_id: i64,
    url: &str,
    title: &str,
    is_active: bool,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("browser:tab:{tab_id}");
    let state = WorldEntityState::fresh(observed_at)
        .with_property("tab_id", json!(tab_id))
        .with_property("window_id", json!(window_id))
        .with_property("url", json!(url))
        .with_property("title", json!(title))
        .with_property("is_active", json!(is_active));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::BrowserPage, state)
}

/// Adapter wraps a `ProjectionStore` and translates incoming events.
pub struct BrowserAdapter {
    store: ProjectionStore,
}

impl BrowserAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    /// Process one event. Upsert / tombstone as appropriate. Returns
    /// `true` when the projection changed.
    pub async fn handle(&self, event: BrowserTabEvent) -> bool {
        match event {
            BrowserTabEvent::TabOpened {
                tab_id,
                window_id,
                url,
                title,
                is_active,
                opened_at,
            } => {
                let entity = tab_entity(tab_id, window_id, &url, &title, is_active, &opened_at);
                self.store.upsert(entity).await;
                true
            }
            BrowserTabEvent::TabUpdated {
                tab_id,
                window_id,
                url,
                title,
                is_active,
                observed_at,
            } => {
                let entity = tab_entity(tab_id, window_id, &url, &title, is_active, &observed_at);
                self.store.upsert(entity).await;
                true
            }
            BrowserTabEvent::TabClosed {
                tab_id,
                closed_at,
            } => {
                let id = format!("browser:tab:{tab_id}");
                self.store
                    .tombstone(&WorldEntityKind::BrowserPage, &id, &closed_at)
                    .await
            }
            BrowserTabEvent::TabActivated {
                tab_id,
                window_id,
                observed_at,
            } => {
                // Activate = re-upsert with is_active true. We don't
                // know the URL/title from a pure activate event, so we
                // pull the prior snapshot to get them.
                let snap = self.store.snapshot();
                let id = format!("browser:tab:{tab_id}");
                let prior = snap.get(&WorldEntityKind::BrowserPage, &id);
                let (url, title) = match prior {
                    Some(p) => (
                        p.state
                            .properties
                            .get("url")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                        p.state
                            .properties
                            .get("title")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            .to_string(),
                    ),
                    None => (String::new(), String::new()),
                };
                let entity = tab_entity(tab_id, window_id, &url, &title, true, &observed_at);
                self.store.upsert(entity).await;
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opened_event() -> BrowserTabEvent {
        BrowserTabEvent::TabOpened {
            tab_id: 42,
            window_id: 1,
            url: "https://example.com".into(),
            title: "Example".into(),
            is_active: true,
            opened_at: "t0".into(),
        }
    }

    // ── event classification ──────────────────────────────────────

    #[test]
    fn is_upsert_for_open_update_activate_not_close() {
        assert!(opened_event().is_upsert());
        assert!(BrowserTabEvent::TabUpdated {
            tab_id: 1,
            window_id: 1,
            url: String::new(),
            title: String::new(),
            is_active: false,
            observed_at: "t".into(),
        }
        .is_upsert());
        assert!(BrowserTabEvent::TabActivated {
            tab_id: 1,
            window_id: 1,
            observed_at: "t".into(),
        }
        .is_upsert());
        assert!(!BrowserTabEvent::TabClosed {
            tab_id: 1,
            closed_at: "t".into(),
        }
        .is_upsert());
    }

    // ── tab_entity construction ───────────────────────────────────

    #[test]
    fn tab_entity_id_uses_browser_tab_namespace() {
        let e = tab_entity(7, 1, "http://x", "X", true, "t0");
        assert_eq!(e.r#ref.id, "browser:tab:7");
        assert_eq!(e.kind, WorldEntityKind::BrowserPage);
        assert_eq!(e.state.properties.get("url"), Some(&json!("http://x")));
        assert_eq!(e.state.properties.get("is_active"), Some(&json!(true)));
    }

    // ── adapter handle ────────────────────────────────────────────

    #[tokio::test]
    async fn handle_open_upserts_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        let changed = adapter.handle(opened_event()).await;
        assert!(changed);
        let snap = store.snapshot();
        let e = snap
            .get(&WorldEntityKind::BrowserPage, "browser:tab:42")
            .unwrap();
        assert_eq!(e.state.properties.get("title"), Some(&json!("Example")));
    }

    #[tokio::test]
    async fn handle_update_replaces_existing() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        adapter.handle(opened_event()).await;
        let upd = BrowserTabEvent::TabUpdated {
            tab_id: 42,
            window_id: 1,
            url: "https://new.example.com".into(),
            title: "New".into(),
            is_active: true,
            observed_at: "t1".into(),
        };
        adapter.handle(upd).await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::BrowserPage, "browser:tab:42")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("title"), Some(&json!("New")));
        assert_eq!(
            e.state.properties.get("url"),
            Some(&json!("https://new.example.com"))
        );
    }

    #[tokio::test]
    async fn handle_close_tombstones() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        adapter.handle(opened_event()).await;
        let changed = adapter
            .handle(BrowserTabEvent::TabClosed {
                tab_id: 42,
                closed_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::BrowserPage, "browser:tab:42")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }

    #[tokio::test]
    async fn handle_close_on_unknown_tab_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        let changed = adapter
            .handle(BrowserTabEvent::TabClosed {
                tab_id: 999,
                closed_at: "t1".into(),
            })
            .await;
        assert!(!changed);
    }

    #[tokio::test]
    async fn handle_activate_carries_prior_url_and_title() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        adapter.handle(opened_event()).await;
        adapter
            .handle(BrowserTabEvent::TabActivated {
                tab_id: 42,
                window_id: 1,
                observed_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::BrowserPage, "browser:tab:42")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("url"), Some(&json!("https://example.com")));
        assert_eq!(e.state.properties.get("title"), Some(&json!("Example")));
        assert_eq!(e.state.properties.get("is_active"), Some(&json!(true)));
    }

    #[tokio::test]
    async fn handle_activate_on_unknown_tab_creates_with_empty_url() {
        let store = ProjectionStore::new("t0");
        let adapter = BrowserAdapter::new(store.clone());
        adapter
            .handle(BrowserTabEvent::TabActivated {
                tab_id: 999,
                window_id: 1,
                observed_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::BrowserPage, "browser:tab:999")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("url"), Some(&json!("")));
        assert_eq!(e.state.properties.get("is_active"), Some(&json!(true)));
    }

    // ── serde tag snake_case ─────────────────────────────────────

    #[test]
    fn event_serde_tag_snake_case() {
        let v = serde_json::to_value(opened_event()).unwrap();
        assert_eq!(v["kind"], "tab_opened");
    }

    #[test]
    fn event_roundtrips_closed() {
        let e = BrowserTabEvent::TabClosed {
            tab_id: 1,
            closed_at: "t".into(),
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: BrowserTabEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
