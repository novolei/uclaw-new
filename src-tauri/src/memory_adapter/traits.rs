//! The `MemoryAdapter` trait — backend-agnostic contract for memory stores.
//!
//! Mirrors openhuman's `Memory` trait from
//! `src/openhuman/memory/traits.rs`. Adapters wrap concrete backends
//! (bucket-seal, legacy KV, legacy Steward graph, gbrain MCP, memU) and
//! present them through this single shape so callers don't need to know
//! which store is underneath.

use async_trait::async_trait;

use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

/// The core trait for memory storage and retrieval.
///
/// Any persistence backend (SQLite, in-process KV, vector DB, MCP-wrapped
/// remote, etc.) should implement this trait to be used within the
/// uClaw memory subsystem.
#[async_trait]
pub trait MemoryAdapter: Send + Sync {
    /// Returns the name of the memory backend (e.g. `"bucket_seal"`,
    /// `"legacy_kv"`, `"gbrain"`). Used as the key in
    /// `AppState.memory_adapters`.
    fn name(&self) -> &str;

    /// Stores a new memory entry or updates an existing one.
    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()>;

    /// Recalls memories matching a query string using keyword or
    /// semantic search.
    ///
    /// Namespace is passed via `opts.namespace`; `None` uses the
    /// backend's legacy default namespace.
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Retrieves a specific memory entry by exact `(namespace, key)`.
    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>>;

    /// Lists memory entries, optionally scoped by namespace, category,
    /// session.
    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>>;

    /// Removes the entry at `(namespace, key)`. Returns `true` if an
    /// entry existed and was removed, `false` if nothing matched.
    async fn delete(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<bool>;

    /// Clears every entry in a namespace. Returns the number of entries
    /// removed.
    async fn clear_namespace(
        &self,
        namespace: &str,
    ) -> anyhow::Result<u64>;

    /// Returns a summary row for every namespace the backend knows
    /// about, used by namespace-discovery UI affordances.
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>>;
}
