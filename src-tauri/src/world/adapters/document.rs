//! Document adapter — Google Docs / Notion / Office / Markdown files.
//!
//! Each doc becomes one `WorldEntity` of kind `Document`. State
//! carries source (gdoc / notion / office365 / markdown / other),
//! title, owner, last_modified_at, word_count, and a body preview.

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::world::entity::{EntityRef, WorldEntity, WorldEntityKind, WorldEntityState};
use crate::world::store::ProjectionStore;

/// Inbound document change event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DocEvent {
    /// New doc created or first observed.
    DocCreated {
        doc_id: String,
        source: String, // "gdoc" / "notion" / "office365" / "markdown"
        title: String,
        owner: String,
        body_preview: String,
        word_count: u64,
        observed_at: String,
    },
    /// Doc edited (title / body / owner change).
    DocUpdated {
        doc_id: String,
        source: String,
        title: String,
        owner: String,
        body_preview: String,
        word_count: u64,
        observed_at: String,
    },
    /// Doc removed.
    DocDeleted {
        doc_id: String,
        deleted_at: String,
    },
}

/// Build a `WorldEntity` for one document.
pub fn document_to_entity(
    doc_id: &str,
    source: &str,
    title: &str,
    owner: &str,
    body_preview: &str,
    word_count: u64,
    observed_at: &str,
) -> WorldEntity {
    let id = format!("doc:{source}:{doc_id}");
    let state = WorldEntityState::fresh(observed_at)
        .with_property("doc_id", json!(doc_id))
        .with_property("source", json!(source))
        .with_property("title", json!(title))
        .with_property("owner", json!(owner))
        .with_property("body_preview", json!(body_preview))
        .with_property("word_count", json!(word_count));
    WorldEntity::new(EntityRef::new(id), WorldEntityKind::Document, state)
}

pub struct DocumentAdapter {
    store: ProjectionStore,
}

impl DocumentAdapter {
    pub fn new(store: ProjectionStore) -> Self {
        Self { store }
    }

    /// Body previews can be huge — 240-byte UTF-8 safe cap.
    const MAX_PREVIEW: usize = 240;

    fn truncate(text: &str) -> String {
        let mut cut = Self::MAX_PREVIEW.min(text.len());
        while cut > 0 && !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text[..cut].to_string()
    }

    pub async fn handle(&self, event: DocEvent) -> bool {
        match event {
            DocEvent::DocCreated {
                doc_id,
                source,
                title,
                owner,
                body_preview,
                word_count,
                observed_at,
            }
            | DocEvent::DocUpdated {
                doc_id,
                source,
                title,
                owner,
                body_preview,
                word_count,
                observed_at,
            } => {
                let preview = Self::truncate(&body_preview);
                let entity = document_to_entity(
                    &doc_id,
                    &source,
                    &title,
                    &owner,
                    &preview,
                    word_count,
                    &observed_at,
                );
                self.store.upsert(entity).await;
                true
            }
            DocEvent::DocDeleted {
                doc_id,
                deleted_at,
            } => {
                // We don't know which `source` the doc used at deletion
                // time. Try common sources to find a hit.
                let snap = self.store.snapshot();
                for source in &["gdoc", "notion", "office365", "markdown", "other"] {
                    let key = format!("doc:{source}:{doc_id}");
                    if snap.get(&WorldEntityKind::Document, &key).is_some() {
                        return self
                            .store
                            .tombstone(&WorldEntityKind::Document, &key, &deleted_at)
                            .await;
                    }
                }
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn created(id: &str, source: &str) -> DocEvent {
        DocEvent::DocCreated {
            doc_id: id.into(),
            source: source.into(),
            title: "Title".into(),
            owner: "alice".into(),
            body_preview: "Body text".into(),
            word_count: 42,
            observed_at: "t0".into(),
        }
    }

    // ── entity factory ─────────────────────────────────────────────

    #[test]
    fn entity_id_uses_doc_source_namespace() {
        let e = document_to_entity("d1", "gdoc", "Title", "alice", "preview", 10, "t0");
        assert_eq!(e.r#ref.id, "doc:gdoc:d1");
        assert_eq!(e.kind, WorldEntityKind::Document);
        assert_eq!(e.state.properties.get("source"), Some(&json!("gdoc")));
        assert_eq!(e.state.properties.get("word_count"), Some(&json!(10)));
    }

    // ── serde ──────────────────────────────────────────────────────

    #[test]
    fn event_serde_tag_snake_case() {
        let v = serde_json::to_value(created("d1", "gdoc")).unwrap();
        assert_eq!(v["kind"], "doc_created");
        let v = serde_json::to_value(DocEvent::DocDeleted {
            doc_id: "d1".into(),
            deleted_at: "t".into(),
        })
        .unwrap();
        assert_eq!(v["kind"], "doc_deleted");
    }

    // ── adapter: create ───────────────────────────────────────────

    #[tokio::test]
    async fn create_inserts_entity() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        adapter.handle(created("d1", "gdoc")).await;
        assert!(store
            .snapshot()
            .get(&WorldEntityKind::Document, "doc:gdoc:d1")
            .is_some());
    }

    #[tokio::test]
    async fn create_truncates_long_preview_utf8_safe() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        let big = "中文文档内容很长".repeat(200);
        adapter
            .handle(DocEvent::DocCreated {
                doc_id: "d1".into(),
                source: "notion".into(),
                title: "T".into(),
                owner: "o".into(),
                body_preview: big,
                word_count: 9999,
                observed_at: "t".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Document, "doc:notion:d1")
            .cloned()
            .unwrap();
        let p = e
            .state
            .properties
            .get("body_preview")
            .and_then(|v| v.as_str())
            .unwrap()
            .to_string();
        assert!(p.len() <= DocumentAdapter::MAX_PREVIEW);
        assert!(p.starts_with("中"));
    }

    // ── adapter: update ───────────────────────────────────────────

    #[tokio::test]
    async fn update_replaces_existing() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        adapter.handle(created("d1", "gdoc")).await;
        adapter
            .handle(DocEvent::DocUpdated {
                doc_id: "d1".into(),
                source: "gdoc".into(),
                title: "Renamed".into(),
                owner: "alice".into(),
                body_preview: "new body".into(),
                word_count: 100,
                observed_at: "t1".into(),
            })
            .await;
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Document, "doc:gdoc:d1")
            .cloned()
            .unwrap();
        assert_eq!(e.state.properties.get("title"), Some(&json!("Renamed")));
        assert_eq!(e.state.properties.get("word_count"), Some(&json!(100)));
    }

    // ── adapter: delete probes sources ────────────────────────────

    #[tokio::test]
    async fn delete_finds_doc_regardless_of_source() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        adapter.handle(created("d1", "notion")).await;
        // Delete event doesn't know source — adapter probes.
        let changed = adapter
            .handle(DocEvent::DocDeleted {
                doc_id: "d1".into(),
                deleted_at: "t1".into(),
            })
            .await;
        assert!(changed);
        let e = store
            .snapshot()
            .get(&WorldEntityKind::Document, "doc:notion:d1")
            .cloned()
            .unwrap();
        assert!(e.is_tombstoned());
    }

    #[tokio::test]
    async fn delete_unknown_returns_false() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        let changed = adapter
            .handle(DocEvent::DocDeleted {
                doc_id: "ghost".into(),
                deleted_at: "t".into(),
            })
            .await;
        assert!(!changed);
    }

    // ── same id different source coexists ─────────────────────────

    #[tokio::test]
    async fn same_doc_id_in_different_sources_are_separate() {
        let store = ProjectionStore::new("t0");
        let adapter = DocumentAdapter::new(store.clone());
        adapter.handle(created("shared", "gdoc")).await;
        adapter.handle(created("shared", "notion")).await;
        let snap = store.snapshot();
        assert_eq!(snap.entities.len(), 2);
        assert!(snap.get(&WorldEntityKind::Document, "doc:gdoc:shared").is_some());
        assert!(snap.get(&WorldEntityKind::Document, "doc:notion:shared").is_some());
    }
}
