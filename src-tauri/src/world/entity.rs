//! `WorldEntity` — the typed model of one thing in the external world.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Coarse kind of entity. Drives which adapter owns the entity and
/// which observation patterns are valid.
///
/// Open-ended via `Other(String)` — plugins can declare new entity
/// kinds without changing the core enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subkind", rename_all = "snake_case")]
pub enum WorldEntityKind {
    /// Local filesystem item (file or directory).
    File,
    /// VCS object (commit, branch, PR, issue).
    GitObject,
    /// Chat thread or channel.
    ChatThread,
    /// Email message or thread.
    Email,
    /// Calendar event.
    CalendarEvent,
    /// Browser page / tab.
    BrowserPage,
    /// Document in a spreadsheet/wordprocessor/slides app.
    Document,
    /// Database row / table / dataset.
    Dataset,
    /// Anything plugin-defined. Subkind string carries the
    /// plugin-specific category.
    Other(String),
}

impl WorldEntityKind {
    /// Stable string id for serialization + table lookup.
    pub fn id(&self) -> String {
        match self {
            Self::File => "file".into(),
            Self::GitObject => "git_object".into(),
            Self::ChatThread => "chat_thread".into(),
            Self::Email => "email".into(),
            Self::CalendarEvent => "calendar_event".into(),
            Self::BrowserPage => "browser_page".into(),
            Self::Document => "document".into(),
            Self::Dataset => "dataset".into(),
            Self::Other(sub) => format!("other:{sub}"),
        }
    }
}

/// Opaque reference — stable id + display label. Mirrors the shape of
/// `runtime::contracts::ContextRef` but lives in the world layer so
/// world entities don't need a runtime-contracts dep.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityRef {
    /// Globally-stable id within `kind`. Format is adapter-defined
    /// (e.g. `"file:/Users/me/a.txt"`, `"gh:owner/repo/issues/42"`).
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

impl EntityRef {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: None,
        }
    }

    pub fn labeled(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: Some(label.into()),
        }
    }
}

/// Stateful snapshot of the entity. Generic by design — adapters
/// project concrete state shapes into the JSON payload, projection
/// consumers read by entity kind + path key.
///
/// `properties` is a sorted map (deterministic serialization) keyed
/// on adapter-defined paths like `"size"`, `"modified_at"`,
/// `"status"`. `version` increments on every observed change.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntityState {
    pub version: u64,
    /// RFC 3339 timestamp of the most recent observation.
    pub observed_at: String,
    /// Adapter-specific properties.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, serde_json::Value>,
    /// `true` if the adapter believes the entity has been deleted.
    /// Soft-deleted entities stay in the projection (callers may
    /// need to detect "this was here yesterday and is now gone").
    #[serde(default)]
    pub tombstoned: bool,
}

impl WorldEntityState {
    pub fn fresh(observed_at: impl Into<String>) -> Self {
        Self {
            version: 1,
            observed_at: observed_at.into(),
            properties: BTreeMap::new(),
            tombstoned: false,
        }
    }

    pub fn with_property(
        mut self,
        key: impl Into<String>,
        value: serde_json::Value,
    ) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    pub fn tombstoned(mut self, when: impl Into<String>) -> Self {
        self.tombstoned = true;
        self.observed_at = when.into();
        self.version += 1;
        self
    }
}

/// Full entity record. The projection store keys on `(kind, ref.id)`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorldEntity {
    pub r#ref: EntityRef,
    pub kind: WorldEntityKind,
    pub state: WorldEntityState,
}

impl WorldEntity {
    pub fn new(r#ref: EntityRef, kind: WorldEntityKind, state: WorldEntityState) -> Self {
        Self { r#ref, kind, state }
    }

    pub fn is_tombstoned(&self) -> bool {
        self.state.tombstoned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ── WorldEntityKind ─────────────────────────────────────────────

    #[test]
    fn kind_id_strings_distinct() {
        let ids = vec![
            WorldEntityKind::File.id(),
            WorldEntityKind::GitObject.id(),
            WorldEntityKind::ChatThread.id(),
            WorldEntityKind::Email.id(),
            WorldEntityKind::CalendarEvent.id(),
            WorldEntityKind::BrowserPage.id(),
            WorldEntityKind::Document.id(),
            WorldEntityKind::Dataset.id(),
            WorldEntityKind::Other("plugin.x".into()).id(),
        ];
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 9);
    }

    #[test]
    fn other_kind_carries_subkind_in_id() {
        let k = WorldEntityKind::Other("plugin.weather".into());
        assert_eq!(k.id(), "other:plugin.weather");
    }

    #[test]
    fn kind_serde_snake_case_with_subkind_content() {
        // Variants without payload serialize as just kind tag.
        let v = serde_json::to_value(WorldEntityKind::File).unwrap();
        assert_eq!(v["kind"], "file");
        // Other carries its subkind in "subkind" field.
        let v = serde_json::to_value(WorldEntityKind::Other("plugin.x".into())).unwrap();
        assert_eq!(v["kind"], "other");
        assert_eq!(v["subkind"], "plugin.x");
    }

    // ── EntityRef ──────────────────────────────────────────────────

    #[test]
    fn entity_ref_factories() {
        let a = EntityRef::new("file:/tmp/a");
        assert_eq!(a.id, "file:/tmp/a");
        assert!(a.label.is_none());
        let b = EntityRef::labeled("file:/tmp/b", "B");
        assert_eq!(b.label.as_deref(), Some("B"));
    }

    #[test]
    fn entity_ref_serde_skips_label_when_none() {
        let r = EntityRef::new("file:/x");
        let json = serde_json::to_string(&r).unwrap();
        assert!(!json.contains("label"));
    }

    // ── WorldEntityState ───────────────────────────────────────────

    #[test]
    fn state_fresh_v1_no_tombstone() {
        let s = WorldEntityState::fresh("2026-05-21T00:00:00Z");
        assert_eq!(s.version, 1);
        assert!(!s.tombstoned);
        assert!(s.properties.is_empty());
    }

    #[test]
    fn state_with_property_chains() {
        let s = WorldEntityState::fresh("t0")
            .with_property("size", json!(1024))
            .with_property("modified_at", json!("t1"));
        assert_eq!(s.properties.len(), 2);
        assert_eq!(s.properties.get("size"), Some(&json!(1024)));
    }

    #[test]
    fn state_tombstoned_bumps_version_and_marks_flag() {
        let s = WorldEntityState::fresh("t0").tombstoned("t9");
        assert!(s.tombstoned);
        assert_eq!(s.version, 2);
        assert_eq!(s.observed_at, "t9");
    }

    #[test]
    fn state_serde_skips_empty_properties() {
        let s = WorldEntityState::fresh("t0");
        let json = serde_json::to_string(&s).unwrap();
        assert!(!json.contains("properties"));
    }

    // ── WorldEntity ────────────────────────────────────────────────

    #[test]
    fn entity_is_tombstoned_passthrough() {
        let e = WorldEntity::new(
            EntityRef::new("file:/x"),
            WorldEntityKind::File,
            WorldEntityState::fresh("t0"),
        );
        assert!(!e.is_tombstoned());
        let e = WorldEntity::new(
            EntityRef::new("file:/x"),
            WorldEntityKind::File,
            WorldEntityState::fresh("t0").tombstoned("t1"),
        );
        assert!(e.is_tombstoned());
    }

    #[test]
    fn entity_serde_roundtrip() {
        let e = WorldEntity::new(
            EntityRef::labeled("gh:org/repo/issues/42", "Issue 42"),
            WorldEntityKind::GitObject,
            WorldEntityState::fresh("2026-05-21T12:00:00Z")
                .with_property("title", json!("Bug"))
                .with_property("status", json!("open")),
        );
        let json = serde_json::to_string(&e).unwrap();
        // camelCase
        assert!(json.contains("\"observedAt\":"));
        let back: WorldEntity = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
