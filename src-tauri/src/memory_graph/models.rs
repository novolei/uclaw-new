use serde::{Deserialize, Serialize};

// ─── 10 种分类 ───────────────────────────────────────────────────────
//
// `EntityPage` is the 10th variant added by Memory OS Foundation (Phase 1).
// It represents a per-entity, long-lived synthesis page (compiled-truth +
// timeline doctrine) and is the foundation for the AI Wiki view. The 9
// pre-existing variants are untouched and continue to behave identically.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryNodeKind {
    Boot,
    Identity,
    Value,
    UserProfile,
    Directive,
    Curated,
    Episode,
    Procedure,
    Reference,
    /// Per-entity compiled-truth + timeline page. See Memory OS Foundation
    /// spec §4.2.1 and `memory_graph::entity_page` for the metadata schema.
    EntityPage,
}

impl MemoryNodeKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "boot" => Self::Boot,
            "identity" => Self::Identity,
            "value" => Self::Value,
            "user_profile" => Self::UserProfile,
            "directive" => Self::Directive,
            "curated" => Self::Curated,
            "episode" => Self::Episode,
            "procedure" => Self::Procedure,
            "reference" => Self::Reference,
            "entity_page" => Self::EntityPage,
            _ => Self::Reference,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Boot => "boot",
            Self::Identity => "identity",
            Self::Value => "value",
            Self::UserProfile => "user_profile",
            Self::Directive => "directive",
            Self::Curated => "curated",
            Self::Episode => "episode",
            Self::Procedure => "procedure",
            Self::Reference => "reference",
            Self::EntityPage => "entity_page",
        }
    }
}

// ─── 版本状态 ────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryVersionStatus {
    Active,
    Deprecated,
    Orphaned,
}

impl MemoryVersionStatus {
    pub fn from_str(s: &str) -> Self {
        match s {
            "active" => Self::Active,
            "deprecated" => Self::Deprecated,
            "orphaned" => Self::Orphaned,
            _ => Self::Active,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Deprecated => "deprecated",
            Self::Orphaned => "orphaned",
        }
    }
}

// ─── 关系类型 ────────────────────────────────────────────────────────
//
// Memory OS Foundation Phase 2 adds 7 domain-specific typed-edge variants
// after the original 4 structural ones (`Contains/RelatesTo/Timeline/
// Trigger`). The new variants encode common entity-graph semantics
// (works_at / founded / etc.) and are populated by the zero-LLM auto-link
// post-hook (`memory_graph::auto_link`) when an `EntityPage` writes a
// reference like `[[entity:slug]]` in its compiled_truth.
//
// All 4 existing variants are untouched; from_str's fallback remains
// `RelatesTo` so on-disk rows written before Phase 2 deserialize
// identically.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationKind {
    // ─── Structural (V4 — pre-Phase-2) ────────────────────────────
    Contains,
    RelatesTo,
    Timeline,
    Trigger,
    // ─── Typed entity-graph edges (Phase 2 auto-link, gbrain shape) ─
    WorksAt,
    Founded,
    InvestedIn,
    Advises,
    Attended,
    /// `src` cites `dst` as the source it was derived from. Default
    /// inference for any edge whose destination is a `Reference` node.
    Source,
    /// Catch-all fallback when no other typed edge fits. The auto-link
    /// inference function (`auto_link::infer_link_type`) returns this
    /// when both `(src_kind, dst_kind)` and the context text fail to
    /// match any specific rule.
    Mentions,
}

impl MemoryRelationKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "contains" => Self::Contains,
            "relates_to" => Self::RelatesTo,
            "timeline" => Self::Timeline,
            "trigger" => Self::Trigger,
            "works_at" => Self::WorksAt,
            "founded" => Self::Founded,
            "invested_in" => Self::InvestedIn,
            "advises" => Self::Advises,
            "attended" => Self::Attended,
            "source" => Self::Source,
            "mentions" => Self::Mentions,
            _ => Self::RelatesTo,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::RelatesTo => "relates_to",
            Self::Timeline => "timeline",
            Self::Trigger => "trigger",
            Self::WorksAt => "works_at",
            Self::Founded => "founded",
            Self::InvestedIn => "invested_in",
            Self::Advises => "advises",
            Self::Attended => "attended",
            Self::Source => "source",
            Self::Mentions => "mentions",
        }
    }
}

// ─── 可见性 ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryVisibility {
    Private,
    Session,
    Shared,
}

impl MemoryVisibility {
    pub fn from_str(s: &str) -> Self {
        match s {
            "private" => Self::Private,
            "session" => Self::Session,
            "shared" => Self::Shared,
            _ => Self::Private,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Private => "private",
            Self::Session => "session",
            Self::Shared => "shared",
        }
    }
}

// ─── MemoryNode ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNode {
    pub id: String,
    pub space_id: String,
    pub kind: MemoryNodeKind,
    pub title: String,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── MemoryVersion ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryVersion {
    pub id: String,
    pub node_id: String,
    pub supersedes_version_id: Option<String>,
    pub status: MemoryVersionStatus,
    pub content: String,
    pub metadata: Option<serde_json::Value>,
    pub embedding_json: Option<String>,
    pub created_at: String,
}

// ─── MemoryEdge ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryEdge {
    pub id: String,
    pub space_id: String,
    pub parent_node_id: Option<String>,
    pub child_node_id: String,
    pub relation_kind: MemoryRelationKind,
    pub visibility: MemoryVisibility,
    pub priority: i32,
    pub trigger_text: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── MemoryRoute ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRoute {
    pub id: String,
    pub space_id: String,
    pub edge_id: Option<String>,
    pub node_id: String,
    pub domain: String,
    pub path: String,
    pub is_primary: bool,
    pub created_at: String,
    pub updated_at: String,
}

// ─── MemoryKeyword ───────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryKeyword {
    pub id: String,
    pub space_id: String,
    pub node_id: String,
    pub keyword: String,
    pub created_at: String,
}

// ─── Graph Propagation Result ───────────────────────────────────────

/// 图传播搜索结果节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphPropagationNode {
    pub node_id: String,
    /// 传播得分（0.0 - 1.0），基于关系权重和衰减
    pub score: f32,
    /// 距离种子节点的跳数
    pub depth: usize,
}

// ─── NodeDetail (聚合查询结果) ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNodeDetail {
    pub node: MemoryNode,
    pub active_version: Option<MemoryVersion>,
    pub routes: Vec<MemoryRoute>,
    pub keywords: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_kind_round_trip_all_variants() {
        for kind in [
            MemoryNodeKind::Boot,
            MemoryNodeKind::Identity,
            MemoryNodeKind::Value,
            MemoryNodeKind::UserProfile,
            MemoryNodeKind::Directive,
            MemoryNodeKind::Curated,
            MemoryNodeKind::Episode,
            MemoryNodeKind::Procedure,
            MemoryNodeKind::Reference,
            MemoryNodeKind::EntityPage,
        ] {
            let s = kind.as_str();
            let parsed = MemoryNodeKind::from_str(s);
            assert_eq!(parsed, kind, "round-trip failed for {s}");
        }
    }

    #[test]
    fn entity_page_variant_is_addressable() {
        // Ensure the new variant has the expected wire name and is reachable
        // through both serde and the manual `from_str` / `as_str` helpers.
        assert_eq!(MemoryNodeKind::EntityPage.as_str(), "entity_page");
        assert_eq!(
            MemoryNodeKind::from_str("entity_page"),
            MemoryNodeKind::EntityPage
        );
        let json = serde_json::to_string(&MemoryNodeKind::EntityPage).unwrap();
        assert_eq!(json, "\"entity_page\"");
        let parsed: MemoryNodeKind = serde_json::from_str("\"entity_page\"").unwrap();
        assert_eq!(parsed, MemoryNodeKind::EntityPage);
    }

    #[test]
    fn unknown_kind_falls_back_to_reference() {
        // Forward-compatibility: stale on-disk rows with unknown kind strings
        // must not panic the reader.
        assert_eq!(MemoryNodeKind::from_str("some_future_kind"), MemoryNodeKind::Reference);
        assert_eq!(MemoryNodeKind::from_str(""), MemoryNodeKind::Reference);
    }

    #[test]
    fn relation_kind_round_trip_all_variants() {
        // 4 structural + 7 Phase 2 typed = 11 total. Each must round-trip
        // through (as_str, from_str) without loss.
        for kind in [
            MemoryRelationKind::Contains,
            MemoryRelationKind::RelatesTo,
            MemoryRelationKind::Timeline,
            MemoryRelationKind::Trigger,
            MemoryRelationKind::WorksAt,
            MemoryRelationKind::Founded,
            MemoryRelationKind::InvestedIn,
            MemoryRelationKind::Advises,
            MemoryRelationKind::Attended,
            MemoryRelationKind::Source,
            MemoryRelationKind::Mentions,
        ] {
            let s = kind.as_str();
            let parsed = MemoryRelationKind::from_str(s);
            assert_eq!(parsed, kind, "round-trip failed for {s}");
        }
    }

    #[test]
    fn relation_kind_typed_variants_serde_to_snake_case() {
        // Wire-name contract — these strings end up in memory_edges.relation_kind.
        let cases = [
            (MemoryRelationKind::WorksAt, "works_at"),
            (MemoryRelationKind::Founded, "founded"),
            (MemoryRelationKind::InvestedIn, "invested_in"),
            (MemoryRelationKind::Advises, "advises"),
            (MemoryRelationKind::Attended, "attended"),
            (MemoryRelationKind::Source, "source"),
            (MemoryRelationKind::Mentions, "mentions"),
        ];
        for (variant, expected) in cases {
            assert_eq!(variant.as_str(), expected);
            assert_eq!(MemoryRelationKind::from_str(expected), variant);
            let json = serde_json::to_string(&variant).unwrap();
            assert_eq!(json, format!("\"{}\"", expected));
        }
    }

    #[test]
    fn relation_kind_unknown_falls_back_to_relates_to() {
        // Forward-compat: future variants on disk should not panic the reader.
        assert_eq!(
            MemoryRelationKind::from_str("some_future_edge_kind"),
            MemoryRelationKind::RelatesTo
        );
        assert_eq!(MemoryRelationKind::from_str(""), MemoryRelationKind::RelatesTo);
    }

    #[test]
    fn relation_kind_existing_strings_unchanged_for_backcompat() {
        // V1-V33 rows on disk use these 4 strings — they must keep parsing
        // to the same variants after Phase 2 expansion.
        assert_eq!(MemoryRelationKind::from_str("contains"), MemoryRelationKind::Contains);
        assert_eq!(MemoryRelationKind::from_str("relates_to"), MemoryRelationKind::RelatesTo);
        assert_eq!(MemoryRelationKind::from_str("timeline"), MemoryRelationKind::Timeline);
        assert_eq!(MemoryRelationKind::from_str("trigger"), MemoryRelationKind::Trigger);
    }
}
