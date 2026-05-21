//! Memory graph type definitions.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Coarse partition for organizing memory nodes. Open-ended via
/// `Custom(String)` so plugins can declare new namespaces without
/// touching the core enum.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "namespace", content = "subnamespace", rename_all = "snake_case")]
pub enum MemoryNamespace {
    /// Facts about the user (preferences, biographical, technical
    /// background).
    UserFacts,
    /// Notes about the user's projects (repos, tasks, deadlines).
    ProjectNotes,
    /// Per-conversation memory (volatile compared to the others).
    Conversation,
    /// Scratch space for ad-hoc agent reasoning. Subject to cleanup.
    Scratch,
    /// Plugin-defined namespace.
    Custom(String),
}

impl MemoryNamespace {
    pub fn id(&self) -> String {
        match self {
            Self::UserFacts => "user_facts".into(),
            Self::ProjectNotes => "project_notes".into(),
            Self::Conversation => "conversation".into(),
            Self::Scratch => "scratch".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}

/// Coarse classification of a node's payload.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subkind", rename_all = "snake_case")]
pub enum MemoryNodeKind {
    Entity,
    Fact,
    Decision,
    Preference,
    Custom(String),
}

impl MemoryNodeKind {
    pub fn id(&self) -> String {
        match self {
            Self::Entity => "entity".into(),
            Self::Fact => "fact".into(),
            Self::Decision => "decision".into(),
            Self::Preference => "preference".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}

/// One memory node. `id` is opaque per backend (gbrain uses page id,
/// future SurrealDB backend would use record id).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNode {
    pub id: String,
    pub namespace: MemoryNamespace,
    pub kind: MemoryNodeKind,
    /// Free-form body text. Vector embedding is the adapter's
    /// responsibility — not exposed here.
    pub body: String,
    /// String key→value tags. Adapters may also index these.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
    /// RFC 3339 timestamps.
    pub created_at: String,
    pub updated_at: String,
}

impl MemoryNode {
    pub fn new(
        id: impl Into<String>,
        namespace: MemoryNamespace,
        kind: MemoryNodeKind,
        body: impl Into<String>,
        ts: impl Into<String>,
    ) -> Self {
        let ts_s = ts.into();
        Self {
            id: id.into(),
            namespace,
            kind,
            body: body.into(),
            tags: BTreeMap::new(),
            created_at: ts_s.clone(),
            updated_at: ts_s,
        }
    }

    pub fn with_tag(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.tags.insert(key.into(), value.into());
        self
    }
}

/// How two memory nodes relate.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", content = "subkind", rename_all = "snake_case")]
pub enum MemoryEdgeKind {
    /// Generic association.
    Relates,
    /// Source contradicts target.
    Contradicts,
    /// Source supersedes target (target is older / no longer true).
    Supersedes,
    /// Source mentions target by name.
    Mentions,
    Custom(String),
}

impl MemoryEdgeKind {
    pub fn id(&self) -> String {
        match self {
            Self::Relates => "relates".into(),
            Self::Contradicts => "contradicts".into(),
            Self::Supersedes => "supersedes".into(),
            Self::Mentions => "mentions".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEdge {
    pub source_id: String,
    pub target_id: String,
    pub kind: MemoryEdgeKind,
    /// Optional confidence in [0.0, 1.0]. None = unscored.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f32>,
}

impl MemoryEdge {
    pub fn new(
        source_id: impl Into<String>,
        target_id: impl Into<String>,
        kind: MemoryEdgeKind,
    ) -> Self {
        Self {
            source_id: source_id.into(),
            target_id: target_id.into(),
            kind,
            weight: None,
        }
    }

    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = Some(weight);
        self
    }
}

/// Query against the memory graph.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQuery {
    pub text: String,
    /// Filter to one namespace. None = search all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub namespace: Option<MemoryNamespace>,
    /// Filter to one node kind. None = all kinds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<MemoryNodeKind>,
    /// Require ALL of these tag key+value matches to be present on
    /// the candidate node.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub require_tags: BTreeMap<String, String>,
    /// Maximum hits to return. 0 = adapter default.
    #[serde(default)]
    pub top_k: u32,
}

/// One ranked hit. `relevance` is adapter-defined (vector cosine,
/// BM25, etc.) but normalized to [0.0, 1.0].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryHit {
    pub node: MemoryNode,
    pub relevance: f32,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryQueryResult {
    pub hits: Vec<MemoryHit>,
    /// Total candidate count the adapter scanned (for diagnostic
    /// purposes — could be much larger than `hits.len()` after
    /// filtering + top-k).
    #[serde(default)]
    pub scanned: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts() -> &'static str {
        "2026-05-21T12:00:00Z"
    }

    // ── MemoryNamespace ────────────────────────────────────────────

    #[test]
    fn namespace_ids_distinct() {
        let ids: Vec<_> = [
            MemoryNamespace::UserFacts,
            MemoryNamespace::ProjectNotes,
            MemoryNamespace::Conversation,
            MemoryNamespace::Scratch,
            MemoryNamespace::Custom("plugin.weather".into()),
        ]
        .iter()
        .map(|n| n.id())
        .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
        assert!(MemoryNamespace::Custom("x".into()).id().starts_with("custom:"));
    }

    #[test]
    fn namespace_serde_tagged() {
        let v = serde_json::to_value(MemoryNamespace::UserFacts).unwrap();
        assert_eq!(v["namespace"], "user_facts");
        let v = serde_json::to_value(MemoryNamespace::Custom("x".into())).unwrap();
        assert_eq!(v["namespace"], "custom");
        assert_eq!(v["subnamespace"], "x");
    }

    // ── MemoryNodeKind ────────────────────────────────────────────

    #[test]
    fn node_kind_ids_distinct() {
        let ids: Vec<_> = [
            MemoryNodeKind::Entity,
            MemoryNodeKind::Fact,
            MemoryNodeKind::Decision,
            MemoryNodeKind::Preference,
            MemoryNodeKind::Custom("habit".into()),
        ]
        .iter()
        .map(|k| k.id())
        .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }

    // ── MemoryNode ────────────────────────────────────────────────

    #[test]
    fn node_new_sets_created_and_updated_to_same_ts() {
        let n = MemoryNode::new(
            "n1",
            MemoryNamespace::UserFacts,
            MemoryNodeKind::Fact,
            "user uses dvorak keyboard",
            ts(),
        );
        assert_eq!(n.created_at, ts());
        assert_eq!(n.updated_at, ts());
        assert!(n.tags.is_empty());
    }

    #[test]
    fn node_with_tag_chains() {
        let n = MemoryNode::new(
            "n1",
            MemoryNamespace::UserFacts,
            MemoryNodeKind::Fact,
            "body",
            ts(),
        )
        .with_tag("lang", "rust")
        .with_tag("level", "advanced");
        assert_eq!(n.tags.len(), 2);
        assert_eq!(n.tags.get("lang"), Some(&"rust".to_string()));
    }

    #[test]
    fn node_serde_camel_case_skips_empty_tags() {
        let n = MemoryNode::new(
            "n1",
            MemoryNamespace::UserFacts,
            MemoryNodeKind::Fact,
            "x",
            ts(),
        );
        let json = serde_json::to_string(&n).unwrap();
        assert!(!json.contains("tags"));
        assert!(json.contains("\"createdAt\":"));
        assert!(json.contains("\"updatedAt\":"));
    }

    // ── MemoryEdge ────────────────────────────────────────────────

    #[test]
    fn edge_new_no_weight() {
        let e = MemoryEdge::new("a", "b", MemoryEdgeKind::Relates);
        assert!(e.weight.is_none());
    }

    #[test]
    fn edge_with_weight() {
        let e = MemoryEdge::new("a", "b", MemoryEdgeKind::Supersedes).with_weight(0.9);
        assert_eq!(e.weight, Some(0.9));
    }

    #[test]
    fn edge_kind_ids_distinct() {
        let ids: Vec<_> = [
            MemoryEdgeKind::Relates,
            MemoryEdgeKind::Contradicts,
            MemoryEdgeKind::Supersedes,
            MemoryEdgeKind::Mentions,
            MemoryEdgeKind::Custom("derived_from".into()),
        ]
        .iter()
        .map(|k| k.id())
        .collect();
        let mut sorted = ids.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 5);
    }

    // ── MemoryQuery ──────────────────────────────────────────────

    #[test]
    fn query_default_skips_all_optionals() {
        let q = MemoryQuery::default();
        let json = serde_json::to_string(&q).unwrap();
        assert!(!json.contains("namespace"));
        assert!(!json.contains("kind"));
        assert!(!json.contains("requireTags"));
    }

    #[test]
    fn query_with_filters_roundtrip() {
        let mut q = MemoryQuery {
            text: "rust async".into(),
            namespace: Some(MemoryNamespace::ProjectNotes),
            kind: Some(MemoryNodeKind::Fact),
            require_tags: BTreeMap::new(),
            top_k: 5,
        };
        q.require_tags.insert("lang".into(), "rust".into());
        let json = serde_json::to_string(&q).unwrap();
        let back: MemoryQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(q, back);
    }

    // ── MemoryQueryResult ────────────────────────────────────────

    #[test]
    fn query_result_default_empty() {
        let r = MemoryQueryResult::default();
        assert!(r.hits.is_empty());
        assert_eq!(r.scanned, 0);
    }

    #[test]
    fn hit_serde_roundtrip() {
        let h = MemoryHit {
            node: MemoryNode::new(
                "n1",
                MemoryNamespace::UserFacts,
                MemoryNodeKind::Fact,
                "x",
                ts(),
            ),
            relevance: 0.87,
        };
        let json = serde_json::to_string(&h).unwrap();
        let back: MemoryHit = serde_json::from_str(&json).unwrap();
        assert_eq!(h, back);
    }
}
