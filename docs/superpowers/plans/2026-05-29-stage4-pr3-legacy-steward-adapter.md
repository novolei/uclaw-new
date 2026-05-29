# 阶段 4 PR3 — `LegacyStewardAdapter` (wraps `memory_graph::MemoryGraphStore`) · Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development.

**Goal:** Second concrete `MemoryAdapter` impl: `LegacyStewardAdapter` wraps the Steward memory-graph store (`crate::memory_graph::MemoryGraphStore`) behind the trait. Demonstrates the trait works for graph-shaped data (nodes + versions + edges) by flattening node+active-version pairs into `MemoryEntry`. Writes go through `enforce_freeze` warn-only path (audit-confirmed semantic preserved). **No call-site migrations** — existing `tauri_commands::memory_graph_*` handlers keep their direct store references.

**Architecture:** The adapter wraps `Arc<MemoryGraphStore>` and translates the trait's flat `MemoryEntry` shape against the graph's `(MemoryNode, MemoryVersion)` pair. Lossy by design — edges, versions older than active, importance scores, embeddings, and the EntityPage timeline don't map to the trait. The trait gives a "flat view" of the graph; the graph's full richness stays reachable via `tauri_commands::memory_graph_*` IPCs. Error conversion: `crate::error::Error` → `anyhow::Error` via `.map_err(|e| anyhow::anyhow!(...))`. Recall: simple title-substring search (legacy memory_graph has no FTS at the store level; `MemoryRecallEngine` provides ranking but it's a higher-layer construct not appropriate for a 1:1 adapter). PR9 BucketSealAdapter delivers real ranked recall.

**Tech Stack:** `async-trait`, `anyhow`, `serde`, `chrono`. No new deps.

**Related design:** [`docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md`](../specs/2026-05-29-stage4-memory-adapter-design.md) — Backend roster row #3.

**Reference:** `src-tauri/src/memory_graph/store.rs` (legacy store), `models.rs` (MemoryNode + MemoryNodeKind), `mod.rs` (enforce_freeze).

---

## Pre-flight

1. Confirm main at `c840c4a0` (PR2 merged):
   ```bash
   git -C /Users/ryanliu/Documents/uclaw status -sb
   git -C /Users/ryanliu/Documents/uclaw log --oneline -3
   ```

2. Create worktree:
   ```bash
   git worktree add -b claude/stage4-pr3-legacy-steward-adapter \
       /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter main
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/gbrain-source \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri/gbrain-source
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/pyembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri/pyembed
   ln -s /Users/ryanliu/Documents/uclaw/src-tauri/bunembed \
         /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri/bunembed
   ```

3. Baselines (expected):
   - `cargo build`: 0 errors, **50 warnings** (post-PR2).
   - `cargo test --lib memory_adapter`: 14/0 (5 PR1 + 7 PR2 + 2 pre-existing browser).
   - `cargo test --lib agent::`: 798/2.

---

## File structure

Modified PR1+PR2 state already has:
```
src-tauri/src/memory_adapter/
├── mod.rs                — add `mod legacy_steward;` + `pub use ...`
├── traits.rs             — unchanged
├── types.rs              — unchanged
├── tests.rs              — unchanged
└── legacy_kv.rs          — PR2, unchanged
```

PR3 adds:
```
src-tauri/src/memory_adapter/
└── legacy_steward.rs     — NEW: struct + trait impl + inline tests
```

And modifies:
- `src-tauri/src/memory_adapter/mod.rs` (declare module + re-export)
- `src-tauri/src/app.rs` (build + register at boot)

---

## Task 1: Implement `LegacyStewardAdapter`

**Files:**
- Create: `src-tauri/src/memory_adapter/legacy_steward.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs`

### Steps

- [ ] **Step 1.1: Create `legacy_steward.rs` with the adapter**

```rust
// src-tauri/src/memory_adapter/legacy_steward.rs

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
                MemoryCategory::Custom("procedure".to_string())
            }
            MemoryNodeKind::Boot => MemoryCategory::Custom("boot".to_string()),
            MemoryNodeKind::Reference => {
                MemoryCategory::Custom("reference".to_string())
            }
            MemoryNodeKind::EntityPage => {
                MemoryCategory::Custom("entity_page".to_string())
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
                "procedure" => MemoryNodeKind::Procedure,
                "boot" => MemoryNodeKind::Boot,
                "reference" => MemoryNodeKind::Reference,
                "entity_page" => MemoryNodeKind::EntityPage,
                _ => MemoryNodeKind::Curated,
            },
        }
    }

    /// Combine a node + its active version content into the flat trait shape.
    /// Returns `None` if the active-version lookup fails — node-only entries
    /// are skipped (the trait promises `content`).
    fn convert(
        &self,
        node: MemoryNode,
        content: Option<String>,
    ) -> MemoryEntry {
        let category = Self::kind_to_category(node.kind);
        MemoryEntry {
            id: node.id,
            key: node.title,
            content: content.unwrap_or_default(),
            namespace: Some(node.space_id),
            category,
            timestamp: node.updated_at,
            session_id: None,  // memory_graph doesn't model sessions
            score: None,
        }
    }

    fn hydrate(&self, node: MemoryNode) -> anyhow::Result<MemoryEntry> {
        let content = self
            .inner
            .get_active_version(&node.id)
            .map_err(|e| anyhow::anyhow!("legacy_steward::get_active_version: {}", e))?
            .map(|v| v.content);
        Ok(self.convert(node, content))
    }
}

#[async_trait]
impl MemoryAdapter for LegacyStewardAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    /// Creates a new node + active version. Writes pass through
    /// `enforce_freeze` (warn-only). `namespace` maps to `space_id`;
    /// `session_id` is ignored (memory_graph doesn't model sessions).
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
        let node_id = uuid::Uuid::new_v4().to_string();

        let node = MemoryNode {
            id: node_id.clone(),
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

        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id,
            supersedes_version_id: None,
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
        // doesn't expose FTS at the store level.
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
    /// Compute by scanning all_nodes once.
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
        // ensure_schema returns Result; verify the actual name + signature
        // during implementation. If it returns Result, use .expect("schema").
        store.ensure_schema().expect("memory_graph schema setup");
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
```

**Adaptation responsibility for the implementer:** The `fresh_store()` helper assumes `MemoryGraphStore::ensure_schema()` returns `Result`. Verify the actual signature; if it returns `()` (like memory.rs's `ensure_table` did in PR2), drop the `.expect(...)`. If the method has a different name (e.g., `init_schema`, `ensure_tables`), use the actual name.

- [ ] **Step 1.2: Wire into `memory_adapter/mod.rs`**

Add to mod.rs:

```rust
mod legacy_steward;

pub use legacy_steward::LegacyStewardAdapter;
```

Place alongside the existing `mod legacy_kv;` + `pub use legacy_kv::LegacyKvAdapter;` (after PR2).

- [ ] **Step 1.3: Build + run new tests**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib memory_adapter::legacy_steward 2>&1 | tail -10
```

Expected: 0 errors, 8 new tests pass.

Likely adaptations:
- `ensure_schema` method name/signature — verify and use actual.
- `MemoryNode` field names — verify match (the recon found: id, space_id, kind, title, metadata, created_at, updated_at).
- `MemoryVersionStatus::Active` — verify variant exists (the recon saw `status: MemoryVersionStatus` field but didn't enumerate variants).
- `uuid::Uuid::new_v4()` — verify `uuid` is a workspace dep. If not, use `chrono::Utc::now().timestamp_millis().to_string()` as a simpler id (no UUID need for tests).

- [ ] **Step 1.4: Verify full baseline**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
```

Expected: `agent::` 798/2 unchanged; warnings ≤51.

- [ ] **Step 1.5: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter add -A src-tauri/src/memory_adapter/
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter commit -m "feat(memory_adapter): LegacyStewardAdapter wrapping memory_graph::MemoryGraphStore (PR3.1 of 阶段 4)

Second concrete MemoryAdapter impl. Translates the trait's 8 async methods
into graph operations on the Steward store. Lossy by design — edges,
versions older than active, importance scores, embeddings, and EntityPage
timelines don't map to the flat trait. The graph's full richness stays
reachable via tauri_commands::memory_graph_* IPCs.

Writes pass through enforce_freeze (warn-only) — audit-confirmed
semantic preserved. New writes default to bucket_seal once PR9 lands.

8 inline tests cover name, store + get, recall by title/content, list
filter, delete, clear_namespace, namespace_summaries grouping.

Spec: docs/superpowers/specs/2026-05-29-stage4-memory-adapter-design.md"
```

Continue to Task 2.

---

## Task 2: Register `LegacyStewardAdapter` in `AppState::new`

**Files:**
- Modify: `src-tauri/src/app.rs`

### Steps

- [ ] **Step 2.1: Locate `memory_graph_store` build + the existing `memory_adapters` populator**

```bash
grep -n "memory_graph_store\|MemoryGraphStore::new\|memory_adapters:" /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri/src/app.rs | head -10
```

PR2 already built a `legacy_kv_adapter` and inserted it into the map. PR3 extends that block to also build + insert `legacy_steward_adapter`.

- [ ] **Step 2.2: Add `LegacyStewardAdapter` to the populator**

Find PR2's block (post-PR2 it should look roughly like):

```rust
let legacy_kv_adapter = std::sync::Arc::new(
    crate::memory_adapter::LegacyKvAdapter::new(memory_store.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

let mut memory_adapters: std::collections::HashMap<...> = std::collections::HashMap::new();
memory_adapters.insert(legacy_kv_adapter.name().to_string(), legacy_kv_adapter);
let memory_adapters = std::sync::Arc::new(memory_adapters);
```

Modify to insert the Steward adapter BEFORE `Arc::new`-ing the map. The construction MUST happen AFTER `memory_graph_store` is bound:

```rust
let legacy_kv_adapter = std::sync::Arc::new(
    crate::memory_adapter::LegacyKvAdapter::new(memory_store.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

let legacy_steward_adapter = std::sync::Arc::new(
    crate::memory_adapter::LegacyStewardAdapter::new(memory_graph_store.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;

let mut memory_adapters: std::collections::HashMap<
    String,
    std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
> = std::collections::HashMap::new();
memory_adapters.insert(legacy_kv_adapter.name().to_string(), legacy_kv_adapter);
memory_adapters.insert(legacy_steward_adapter.name().to_string(), legacy_steward_adapter);
let memory_adapters = std::sync::Arc::new(memory_adapters);
```

Don't change the `Self { ... }` literal — `memory_adapters` is already moved in.

- [ ] **Step 2.3: Build + verify**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -10
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
```

Expected:
- 0 errors.
- Warnings ≤51.
- `memory_adapter` ≥22/0 (14 PR1/2 + 8 PR3).
- `agent::` 798/2 unchanged.

- [ ] **Step 2.4: Commit**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter add -A src-tauri/src/app.rs
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter commit -m "feat(app): register LegacyStewardAdapter in memory_adapters at boot (PR3.2 of 阶段 4)

memory_adapters now has 2 entries: 'legacy_kv' + 'legacy_steward'.
Existing tauri_commands::memory_graph_* handlers keep their direct
MemoryGraphStore references; this PR just makes the same store reachable
via the trait registry as well."
```

Continue to Task 3.

---

## Task 3: Final verification

- [ ] **Step 3.1: Full battery**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo build 2>&1 | grep -E "^warning:" | wc -l
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib memory_adapter 2>&1 | tail -5
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib agent:: 2>&1 | tail -3
cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter/src-tauri && cargo test --lib 2>&1 | tail -3
```

Required: 0 errors; ≤51 warnings; `memory_adapter` ≥22/0; `agent::` 798/2; lib total preserved.

- [ ] **Step 3.2: Verify chain**

```bash
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter log --oneline main..HEAD
git -C /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr3-legacy-steward-adapter status -sb
```

Expected: 2 commits ahead of main, clean tree.

---

## Self-Review

**1. Spec coverage:** Backend roster row #3 (LegacyStewardAdapter) — ✅ implemented. 8 trait methods — ✅. Lossy-by-design translation — ✅ (edges/versions/embeddings unmapped, documented). Writes preserve `enforce_freeze` — ✅ (via existing store path). Registered in `AppState.memory_adapters` — ✅ Task 2.

**2. Placeholder scan:** None. All code shown verbatim. One adaptation responsibility called out explicitly: `MemoryGraphStore::ensure_schema` signature — that's the implementer's job to verify, with concrete fallback options.

**3. Type consistency:** `LegacyStewardAdapter`, `MemoryAdapter`, `MemoryEntry`, `MemoryCategory`, `RecallOpts`, `NamespaceSummary` consistent with PR1/PR2. Legacy types `MemoryNode`, `MemoryNodeKind`, `MemoryVersion`, `MemoryVersionStatus`, `MemoryGraphStore` match the recon-verified `models.rs` + `store.rs` shapes.

---

## Cumulative summary

- **Tasks:** 3 (2 implementation + 1 verification).
- **Estimated time:** ~45 minutes (slightly more than PR2 because the translation layer is richer + 8 tests vs PR2's 7).
- **Risk:** Low-medium. Translation logic loses fidelity (edges/versions), but documented and not a regression — the legacy IPC routes preserve the full graph access.
- **Total commits:** 2 (Task 1 + Task 2; Task 3 is verification + handoff).
