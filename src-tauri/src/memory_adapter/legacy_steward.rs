// SPDX-License-Identifier: MIT
//! DEPRECATED (2026-05-31): retained for explicit-namespace back-compat only; see memory_adapter/mod.rs roster. New code must not route here.
//!
//! `LegacyStewardAdapter` — wraps `crate::memory_graph::MemoryGraphStore`
//! (graph-shaped Steward memory) behind the `MemoryAdapter` trait.
//!
//! PR3 of 阶段 4. The store stays as-is — this adapter translates the
//! trait's flat `MemoryEntry` shape against the graph's `MemoryNode +
//! active MemoryVersion` pair. **Lossy by design**: edges, versions
//! older than active, importance scores, embeddings, and EntityPage
//! timelines don't map to the flat trait. The graph's full richness
//! stays reachable via `tauri_commands::memory_graph_*` IPCs.
//!
//! Writes still pass through `crate::memory_graph::enforce_freeze`
//! (warn-only by default). This adapter preserves the audit-confirmed
//! semantic; PR9+ BucketSealAdapter is where new writes default to
//! eventually.

use std::sync::Arc;

use async_trait::async_trait;

use super::traits::MemoryAdapter;
use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

use crate::memory_graph::models::{
    MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus,
};
use crate::memory_graph::store::MemoryGraphStore;

const ADAPTER_NAME: &str = "legacy_steward";
const DEFAULT_SPACE_ID: &str = "global";
const DEFAULT_LIST_LIMIT: usize = 200;

const CAT_PROCEDURE: &str = "procedure";
const CAT_BOOT: &str = "boot";
const CAT_REFERENCE: &str = "reference";
const CAT_ENTITY_PAGE: &str = "entity_page";

/// Wraps `crate::memory_graph::MemoryGraphStore`. Translates between
/// the trait's flat `MemoryEntry` and the graph's `(MemoryNode,
/// MemoryVersion)` pair.
#[derive(Clone)]
pub struct LegacyStewardAdapter {
    inner: Arc<MemoryGraphStore>,
}

impl LegacyStewardAdapter {
    pub fn new(inner: Arc<MemoryGraphStore>) -> Self {
        Self { inner }
    }

    /// Map `MemoryNodeKind` → `MemoryCategory` for the adapter view.
    fn kind_to_category(kind: MemoryNodeKind) -> MemoryCategory {
        match kind {
            MemoryNodeKind::Identity
            | MemoryNodeKind::Value
            | MemoryNodeKind::UserProfile
            | MemoryNodeKind::Directive => MemoryCategory::Core,
            MemoryNodeKind::Episode | MemoryNodeKind::Curated => {
                MemoryCategory::Conversation
            }
            MemoryNodeKind::Procedure => {
                MemoryCategory::Custom(CAT_PROCEDURE.to_string())
            }
            MemoryNodeKind::Boot => MemoryCategory::Custom(CAT_BOOT.to_string()),
            MemoryNodeKind::Reference => {
                MemoryCategory::Custom(CAT_REFERENCE.to_string())
            }
            MemoryNodeKind::EntityPage => {
                MemoryCategory::Custom(CAT_ENTITY_PAGE.to_string())
            }
        }
    }

    /// Inverse map for write-side category routing.
    fn category_to_kind(cat: &MemoryCategory) -> MemoryNodeKind {
        match cat {
            MemoryCategory::Core => MemoryNodeKind::Identity,
            MemoryCategory::Conversation => MemoryNodeKind::Episode,
            MemoryCategory::Daily => MemoryNodeKind::Episode,
            MemoryCategory::Custom(name) => match name.as_str() {
                CAT_PROCEDURE => MemoryNodeKind::Procedure,
                CAT_BOOT => MemoryNodeKind::Boot,
                CAT_REFERENCE => MemoryNodeKind::Reference,
                CAT_ENTITY_PAGE => MemoryNodeKind::EntityPage,
                _ => MemoryNodeKind::Curated,
            },
        }
    }

    /// Combine a node + its active version content into the flat trait shape.
    /// Returns an entry with empty content if no active version exists —
    /// the trait promises `content` is a String (non-Option).
    fn convert(node: MemoryNode, content: Option<String>) -> MemoryEntry {
        let category = Self::kind_to_category(node.kind);
        MemoryEntry {
            id: node.id,
            key: node.title,
            content: content.unwrap_or_default(),
            namespace: Some(node.space_id),
            category,
            timestamp: node.updated_at,
            session_id: None, // memory_graph doesn't model sessions
            score: None,
        }
    }

    fn hydrate(&self, node: MemoryNode) -> anyhow::Result<MemoryEntry> {
        let content = self
            .inner
            .get_active_version(&node.id)
            .map_err(|e| anyhow::anyhow!("legacy_steward::get_active_version: {}", e))?
            .map(|v: MemoryVersion| v.content);
        Ok(Self::convert(node, content))
    }
}

#[async_trait]
impl MemoryAdapter for LegacyStewardAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    /// Stores or updates an entry identified by `(namespace, key)`.
    ///
    /// Upsert semantics: if a node with the same `space_id` (namespace),
    /// `title` (key), and matching `kind` (derived from `category`) already
    /// exists, a new active version is appended with `supersedes_version_id`
    /// pointing at the previous active version. `get_active_version` orders
    /// by `created_at DESC`, so the newest version wins on reads. The prior
    /// active version is left as-is (not archived) — the store's ordering
    /// semantics make it unreachable for normal reads.
    ///
    /// Writes pass through `enforce_freeze` (warn-only). `namespace` maps to
    /// `space_id`; `session_id` is ignored (memory_graph doesn't model sessions).
    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        _session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let kind = Self::category_to_kind(&category);
        let now = chrono::Utc::now().to_rfc3339();

        // Upsert: check whether a node with this (space_id, title, kind) exists.
        let existing_nodes = self
            .inner
            .list_nodes_by_kind(namespace, kind, DEFAULT_LIST_LIMIT)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_nodes_by_kind: {}", e))?;
        let existing = existing_nodes.into_iter().find(|n| n.title == key);

        let node_id = if let Some(ref node) = existing {
            // Node already exists — reuse it.
            node.id.clone()
        } else {
            // New node — create it.
            let new_id = uuid::Uuid::new_v4().to_string();
            let node = MemoryNode {
                id: new_id.clone(),
                space_id: namespace.to_string(),
                kind,
                title: key.to_string(),
                metadata: None,
                created_at: now.clone(),
                updated_at: now.clone(),
            };
            self.inner
                .create_node(&node)
                .map_err(|e| anyhow::anyhow!("legacy_steward::create_node: {}", e))?;
            new_id
        };

        // For an existing node, chain the new version onto the current active one.
        let supersedes = if existing.is_some() {
            self.inner
                .get_active_version(&node_id)
                .map_err(|e| anyhow::anyhow!("legacy_steward::get_active_version: {}", e))?
                .map(|v| v.id)
        } else {
            None
        };

        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id,
            supersedes_version_id: supersedes,
            status: MemoryVersionStatus::Active,
            content: content.to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        self.inner
            .create_version(&version)
            .map_err(|e| anyhow::anyhow!("legacy_steward::create_version: {}", e))?;

        Ok(())
    }

    /// Simple title-substring search across all nodes in the namespace's
    /// space_id, with optional category + min_score filter. `min_score`
    /// is a no-op (no scoring at this layer); `session_id` is ignored.
    /// Real ranked recall is BucketSealAdapter's job (PR9+).
    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // List up to a generous bound, then filter client-side. memory_graph
        // doesn't expose FTS at the store level (MemoryRecallEngine is a
        // higher-layer construct not appropriate for a 1:1 adapter).
        let space_id = opts.namespace.unwrap_or(DEFAULT_SPACE_ID);
        let listing_limit = limit.saturating_mul(4).max(DEFAULT_LIST_LIMIT);
        let nodes = self
            .inner
            .list_recent_nodes(space_id, listing_limit)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_recent_nodes: {}", e))?;

        let q_lower = query.to_lowercase();
        let want_cat = opts.category.as_ref();
        let mut out: Vec<MemoryEntry> = Vec::new();
        for node in nodes {
            let cat = Self::kind_to_category(node.kind);
            if let Some(w) = want_cat {
                if w != &cat {
                    continue;
                }
            }
            let title_match = node.title.to_lowercase().contains(&q_lower);
            // Hydrate content for full-text fallback if title didn't hit.
            let entry = self.hydrate(node)?;
            let content_match = entry.content.to_lowercase().contains(&q_lower);
            if title_match || content_match {
                out.push(entry);
                if out.len() >= limit {
                    break;
                }
            }
        }
        Ok(out)
    }

    /// `(namespace, key)` lookup translates to "find the most-recent
    /// node in this space with this title". Returns `None` if not found.
    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        let nodes = self
            .inner
            .list_recent_nodes(namespace, DEFAULT_LIST_LIMIT)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_recent_nodes: {}", e))?;
        for node in nodes {
            if node.title == key {
                return self.hydrate(node).map(Some);
            }
        }
        Ok(None)
    }

    /// List nodes in a space, optionally filtered by category.
    /// `session_id` is ignored (graph has no session concept).
    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        _session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let space_id = namespace.unwrap_or(DEFAULT_SPACE_ID);
        let nodes = self
            .inner
            .list_recent_nodes(space_id, DEFAULT_LIST_LIMIT)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_recent_nodes: {}", e))?;
        // perf: N+1 hydration; tracked for bucket-seal port in stage 4 PR9+
        let mut out: Vec<MemoryEntry> = Vec::new();
        for node in nodes {
            let cat = Self::kind_to_category(node.kind);
            if let Some(want) = category {
                if want != &cat {
                    continue;
                }
            }
            out.push(self.hydrate(node)?);
        }
        Ok(out)
    }

    /// Find by title then delete the node (cascade rules in the store
    /// handle versions + edges).
    async fn delete(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<bool> {
        let nodes = self
            .inner
            .list_recent_nodes(namespace, DEFAULT_LIST_LIMIT)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_recent_nodes: {}", e))?;
        for node in nodes {
            if node.title == key {
                self.inner
                    .delete_node(&node.id)
                    .map_err(|e| {
                        anyhow::anyhow!("legacy_steward::delete_node: {}", e)
                    })?;
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Delete every node in the given space.
    async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
        let nodes = self
            .inner
            .list_recent_nodes(namespace, usize::MAX)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_recent_nodes: {}", e))?;
        let mut removed: u64 = 0;
        for node in nodes {
            self.inner
                .delete_node(&node.id)
                .map_err(|e| anyhow::anyhow!("legacy_steward::delete_node: {}", e))?;
            removed += 1;
        }
        Ok(removed)
    }

    /// memory_graph doesn't expose a per-space node-count cheaply.
    /// Compute by scanning all nodes once.
    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let all = self
            .inner
            .list_all_nodes(usize::MAX)
            .map_err(|e| anyhow::anyhow!("legacy_steward::list_all_nodes: {}", e))?;
        use std::collections::HashMap;
        let mut acc: HashMap<String, (usize, String)> = HashMap::new();
        for node in all {
            let entry = acc
                .entry(node.space_id.clone())
                .or_insert((0, node.updated_at.clone()));
            entry.0 += 1;
            if node.updated_at > entry.1 {
                entry.1 = node.updated_at;
            }
        }
        Ok(acc
            .into_iter()
            .map(|(namespace, (count, last_updated))| NamespaceSummary {
                namespace,
                count,
                last_updated: Some(last_updated),
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store.ensure_tables();
        Arc::new(store)
    }

    #[tokio::test]
    async fn name_is_legacy_steward() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        assert_eq!(adapter.name(), "legacy_steward");
    }

    #[tokio::test]
    async fn store_creates_node_plus_active_version() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "user.identity", "Ryan is an engineer.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let got = adapter.get("global", "user.identity").await.unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert_eq!(entry.key, "user.identity");
        assert_eq!(entry.content, "Ryan is an engineer.");
        assert_eq!(entry.category, MemoryCategory::Core);
        assert_eq!(entry.namespace.as_deref(), Some("global"));
    }

    #[tokio::test]
    async fn store_then_recall_by_title_substring() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "favorite_color", "blue", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("global", "favorite_food", "ramen", MemoryCategory::Core, None)
            .await
            .unwrap();
        let hits = adapter
            .recall("favorite", 10, RecallOpts {
                namespace: Some("global"),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(hits.len() >= 2);
        assert!(hits.iter().any(|e| e.key == "favorite_color"));
        assert!(hits.iter().any(|e| e.key == "favorite_food"));
    }

    #[tokio::test]
    async fn store_then_recall_by_content_substring() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "k1", "the quick brown fox", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("global", "k2", "lazy dog sleeps", MemoryCategory::Core, None)
            .await
            .unwrap();
        let hits = adapter
            .recall("quick", 10, RecallOpts {
                namespace: Some("global"),
                ..Default::default()
            })
            .await
            .unwrap();
        assert!(hits.iter().any(|e| e.key == "k1"));
        assert!(!hits.iter().any(|e| e.key == "k2"));
    }

    #[tokio::test]
    async fn list_filters_by_category() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "identity1", "core1", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("global", "episode1", "conv1", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        let cores = adapter
            .list(Some("global"), Some(&MemoryCategory::Core), None)
            .await
            .unwrap();
        assert!(cores.iter().all(|e| e.category == MemoryCategory::Core));
        assert!(cores.iter().any(|e| e.key == "identity1"));
        assert!(!cores.iter().any(|e| e.key == "episode1"));
    }

    #[tokio::test]
    async fn store_twice_same_key_is_upsert() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("ns", "k", "first_content", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns", "k", "second_content", MemoryCategory::Core, None)
            .await
            .unwrap();
        // get() returns the latest content.
        let got = adapter.get("ns", "k").await.unwrap();
        assert!(got.is_some());
        assert_eq!(got.unwrap().content, "second_content");
        // list() returns exactly one entry — no duplicate node was created.
        let all = adapter.list(Some("ns"), None, None).await.unwrap();
        assert_eq!(all.len(), 1, "upsert must not create a second node for the same key");
    }

    #[tokio::test]
    async fn recall_filters_by_category() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "core_key", "core content", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("global", "conv_key", "conversation content", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        // Recall with category filter — should return only Core entries.
        let hits = adapter
            .recall(
                "content",
                10,
                RecallOpts {
                    namespace: Some("global"),
                    category: Some(MemoryCategory::Core),
                    ..Default::default()
                },
            )
            .await
            .unwrap();
        assert!(hits.iter().any(|e| e.key == "core_key"), "Core entry must appear");
        assert!(!hits.iter().any(|e| e.key == "conv_key"), "Conversation entry must be filtered out");
        assert!(
            hits.iter().all(|e| e.category == MemoryCategory::Core),
            "all recalled entries must be Core category"
        );
    }

    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter
            .store("global", "k", "v", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert!(adapter.delete("global", "k").await.unwrap());
        assert!(!adapter.delete("global", "k").await.unwrap());
    }

    #[tokio::test]
    async fn clear_namespace_removes_only_space_entries() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter.store("global", "a", "1", MemoryCategory::Core, None).await.unwrap();
        adapter.store("global", "b", "2", MemoryCategory::Core, None).await.unwrap();
        adapter.store("other_space", "c", "3", MemoryCategory::Core, None).await.unwrap();
        let removed = adapter.clear_namespace("global").await.unwrap();
        assert_eq!(removed, 2);
        assert!(adapter.get("global", "a").await.unwrap().is_none());
        assert!(adapter.get("other_space", "c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn namespace_summaries_groups_by_space() {
        let adapter = LegacyStewardAdapter::new(fresh_store());
        adapter.store("global", "k1", "v", MemoryCategory::Core, None).await.unwrap();
        adapter.store("global", "k2", "v", MemoryCategory::Core, None).await.unwrap();
        adapter.store("other", "k3", "v", MemoryCategory::Core, None).await.unwrap();
        let mut summaries = adapter.namespace_summaries().await.unwrap();
        summaries.sort_by(|a, b| a.namespace.cmp(&b.namespace));
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].namespace, "global");
        assert_eq!(summaries[0].count, 2);
        assert_eq!(summaries[1].namespace, "other");
        assert_eq!(summaries[1].count, 1);
    }
}
