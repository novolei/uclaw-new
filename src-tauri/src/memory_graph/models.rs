use serde::{Deserialize, Serialize};

// ─── 9 种分类 ────────────────────────────────────────────────────────

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryRelationKind {
    Contains,
    RelatesTo,
    Timeline,
    Trigger,
}

impl MemoryRelationKind {
    pub fn from_str(s: &str) -> Self {
        match s {
            "contains" => Self::Contains,
            "relates_to" => Self::RelatesTo,
            "timeline" => Self::Timeline,
            "trigger" => Self::Trigger,
            _ => Self::RelatesTo,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Contains => "contains",
            Self::RelatesTo => "relates_to",
            Self::Timeline => "timeline",
            Self::Trigger => "trigger",
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

// ─── NodeDetail (聚合查询结果) ───────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryNodeDetail {
    pub node: MemoryNode,
    pub active_version: Option<MemoryVersion>,
    pub routes: Vec<MemoryRoute>,
    pub keywords: Vec<String>,
}
