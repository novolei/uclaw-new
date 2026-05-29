//! Owned types used by the `MemoryAdapter` trait.
//!
//! Ported from openhuman's `src/openhuman/memory/traits.rs` (MemoryEntry,
//! MemoryCategory, RecallOpts, NamespaceSummary). The shape is identical
//! so any future port-from-source reads with no friction.

use serde::{Deserialize, Serialize};

/// Represents a single stored memory entry with associated metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    /// Unique identifier for the memory entry (usually a UUID).
    pub id: String,
    /// The key or title associated with this memory.
    pub key: String,
    /// The actual content or value of the memory.
    pub content: String,
    /// Optional namespace for logical separation of memories.
    #[serde(default)]
    pub namespace: Option<String>,
    /// The organizational category this memory belongs to.
    pub category: MemoryCategory,
    /// ISO 8601 formatted timestamp of when the memory was created or last updated.
    pub timestamp: String,
    /// Optional session ID if this memory is scoped to a specific interaction.
    pub session_id: Option<String>,
    /// Optional relevance or confidence score, typically from 0.0 to 1.0.
    pub score: Option<f64>,
}

/// Categories used to organize and filter memories by their nature and lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryCategory {
    /// Long-term foundational facts, user preferences, and permanent decisions.
    Core,
    /// Temporal logs reflecting daily activities or ephemeral state.
    Daily,
    /// Contextual information derived from and relevant to active conversations.
    Conversation,
    /// A user-defined or system-defined custom category.
    Custom(String),
}

impl std::fmt::Display for MemoryCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Core => write!(f, "core"),
            Self::Daily => write!(f, "daily"),
            Self::Conversation => write!(f, "conversation"),
            Self::Custom(name) => write!(f, "{name}"),
        }
    }
}

/// Optional filters for `MemoryAdapter::recall`.
///
/// All fields default to `None`. `namespace = None` uses the backend's
/// legacy default namespace. Pass `Some("namespace")` to scope the query
/// to a specific namespace.
#[derive(Debug, Default, Clone)]
pub struct RecallOpts<'a> {
    pub namespace: Option<&'a str>,
    pub category: Option<MemoryCategory>,
    pub session_id: Option<&'a str>,
    pub min_score: Option<f64>,
}

/// Summary row returned by `MemoryAdapter::namespace_summaries`, used for
/// agent-side namespace discovery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamespaceSummary {
    pub namespace: String,
    pub count: usize,
    /// RFC3339 timestamp of most recent `updated_at` in the namespace, if any.
    pub last_updated: Option<String>,
}
