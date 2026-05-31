//! One-time migration of legacy memory_graph Episode nodes into the
//! MemoryAdapter (`proactive:episode:{space}` namespace). Idempotent +
//! infallible: logs and skips on any error; never panics or blocks boot.
//!
//! **Idempotency strategy (option b — namespace sentinel):** before
//! migrating a space, we call `adapter.list(Some(&ns), None, None)` on
//! the target namespace.  If any entry already exists we skip that space
//! entirely — the presence of migrated data IS the idempotency marker.
//! No config-file write is needed; the sentinel survives restarts as long
//! as the bucket-seal DB is intact (same durability guarantee as the
//! migrated data itself).
//!
//! Old memory_graph nodes are retained (frozen read-only legacy); the
//! adapter is the new source of truth for proactive task episodes.
//!
//! Content field names MUST match what `task_memory::entry_to_similar_task`
//! reads: `title`, `task_type`, `status`, `solution_summary`,
//! `files_changed`, `recorded_at`, `keywords`.

use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory};
use crate::memory_graph::models::{MemoryNode, MemoryNodeKind};
use crate::memory_graph::store::MemoryGraphStore;

// ─── Content helper ──────────────────────────────────────────────────────────

/// Convert a legacy `MemoryNode` (Episode kind) into the JSON content string
/// expected by `task_memory::entry_to_similar_task`.
///
/// Extracted as a pure function so it can be unit-tested without a real
/// DB or adapter.
pub fn node_to_content(node: &MemoryNode) -> String {
    let m = node.metadata.as_ref();
    let get_str = |k: &str| {
        m.and_then(|x| x.get(k))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let files: Vec<String> = m
        .and_then(|x| x.get("files_changed"))
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_default();

    serde_json::json!({
        // ── Fields read by entry_to_similar_task ──────────────────────
        "title":            node.title,
        "task_type":        get_str("task_type").unwrap_or_else(|| "unknown".into()),
        "status":           get_str("status").unwrap_or_else(|| "unknown".into()),
        "solution_summary": get_str("solution_summary"),
        "files_changed":    files,
        "recorded_at":      node.created_at,
        "keywords":         Vec::<String>::new(),
        // ── Provenance marker ─────────────────────────────────────────
        "legacy_migrated": true,
    })
    .to_string()
}

// ─── Migration entry point ───────────────────────────────────────────────────

/// Migrate all Episode nodes for the given `spaces` into the adapter
/// namespace `proactive:episode:{space}`.
///
/// Returns the total count of nodes successfully stored.  Each space is
/// checked for existing entries first (namespace-sentinel idempotency):
/// if the target namespace is non-empty the space is skipped.  Any
/// individual error is logged and skipped; the function never panics.
pub async fn migrate_episodes(
    graph: &MemoryGraphStore,
    adapter: &Arc<dyn MemoryAdapter>,
    spaces: &[String],
) -> usize {
    let mut migrated = 0usize;

    for space_id in spaces {
        let ns = format!("proactive:episode:{space_id}");

        // ── Idempotency check (option b — namespace sentinel) ────────
        // If there are already entries in the target namespace, this
        // space has been migrated in a prior run; skip it.
        match adapter.list(Some(&ns), None, None).await {
            Ok(existing) if !existing.is_empty() => {
                tracing::debug!(
                    space = %space_id,
                    existing = existing.len(),
                    "episode migration: namespace already populated, skipping"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    space = %space_id,
                    error = %e,
                    "episode migration: idempotency check failed; will attempt migration anyway"
                );
            }
            Ok(_) => {} // empty — proceed with migration
        }

        // ── List legacy Episode nodes ─────────────────────────────────
        let nodes = match graph.list_nodes_by_kind(space_id, MemoryNodeKind::Episode, 100_000) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    space = %space_id,
                    error = %e,
                    "episode migration: list_nodes_by_kind failed; skipping space"
                );
                continue;
            }
        };

        if nodes.is_empty() {
            tracing::debug!(space = %space_id, "episode migration: no Episode nodes found");
            continue;
        }

        // ── Store each node in the adapter ────────────────────────────
        for node in &nodes {
            let content = node_to_content(node);
            if let Err(e) = adapter
                .store(&ns, &node.id, &content, MemoryCategory::Core, None)
                .await
            {
                tracing::warn!(
                    node = %node.id,
                    space = %space_id,
                    error = %e,
                    "episode migration: store failed; skipping node"
                );
                continue;
            }
            migrated += 1;
        }

        tracing::info!(
            space = %space_id,
            count = nodes.len(),
            "episode migration: space migrated"
        );
    }

    migrated
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
    use crate::memory_graph::models::MemoryNodeKind;

    // ── Minimal in-memory adapter for tests ──────────────────────────────

    struct InMemoryAdapter {
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Arc<dyn MemoryAdapter> {
            Arc::new(Self {
                store: Mutex::new(HashMap::new()),
            })
        }
    }

    #[async_trait]
    impl MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str {
            "in_memory_migration_test"
        }

        async fn store(
            &self,
            namespace: &str,
            key: &str,
            content: &str,
            category: MemoryCategory,
            session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = MemoryEntry {
                id: key.to_string(),
                key: key.to_string(),
                content: content.to_string(),
                namespace: Some(namespace.to_string()),
                category,
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: session_id.map(String::from),
                score: None,
            };
            self.store
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), entry);
            Ok(())
        }

        async fn recall(
            &self,
            _query: &str,
            _limit: usize,
            _opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(
            &self,
            namespace: &str,
            key: &str,
        ) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn list(
            &self,
            namespace: Option<&str>,
            _category: Option<&MemoryCategory>,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| match namespace {
                    Some(ns) => e.namespace.as_deref() == Some(ns),
                    None => true,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(out)
        }

        async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
            let removed = self
                .store
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()))
                .is_some();
            Ok(removed)
        }

        async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
            let mut store = self.store.lock().unwrap();
            let before = store.len();
            store.retain(|(ns, _), _| ns != namespace);
            Ok((before - store.len()) as u64)
        }

        async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
            Ok(Vec::new())
        }
    }

    // ── node_to_content shape tests ────────────────────────────────────────

    /// Verify `node_to_content` produces every field that
    /// `entry_to_similar_task` reads.
    #[test]
    fn node_to_content_produces_correct_field_names() {
        let node = MemoryNode {
            id: "ep-001".into(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Episode,
            title: "Fix the pool leak".into(),
            metadata: Some(serde_json::json!({
                "task_type": "debugging",
                "status": "success",
                "solution_summary": "Added health check",
                "files_changed": ["src/db/pool.rs"],
            })),
            created_at: "2026-05-31T00:00:00Z".into(),
            updated_at: "2026-05-31T00:00:00Z".into(),
        };

        let content = node_to_content(&node);
        let v: serde_json::Value = serde_json::from_str(&content).expect("valid JSON");

        // ── Required by entry_to_similar_task ───────────────────────
        assert_eq!(v["title"].as_str().unwrap(), "Fix the pool leak");
        assert_eq!(v["task_type"].as_str().unwrap(), "debugging");
        assert_eq!(v["status"].as_str().unwrap(), "success");
        assert_eq!(
            v["solution_summary"].as_str().unwrap(),
            "Added health check"
        );
        assert_eq!(
            v["files_changed"].as_array().unwrap(),
            &[serde_json::json!("src/db/pool.rs")]
        );
        assert_eq!(v["recorded_at"].as_str().unwrap(), "2026-05-31T00:00:00Z");
        assert!(v["keywords"].as_array().unwrap().is_empty());
        // ── Provenance ───────────────────────────────────────────────
        assert_eq!(v["legacy_migrated"].as_bool().unwrap(), true);
    }

    #[test]
    fn node_to_content_uses_defaults_when_metadata_absent() {
        let node = MemoryNode {
            id: "ep-002".into(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Episode,
            title: "Some task".into(),
            metadata: None,
            created_at: "2026-01-01T00:00:00Z".into(),
            updated_at: "2026-01-01T00:00:00Z".into(),
        };

        let content = node_to_content(&node);
        let v: serde_json::Value = serde_json::from_str(&content).unwrap();

        assert_eq!(v["task_type"].as_str().unwrap(), "unknown");
        assert_eq!(v["status"].as_str().unwrap(), "unknown");
        assert!(v["solution_summary"].is_null());
        assert!(v["files_changed"].as_array().unwrap().is_empty());
    }

    // ── migrate_episodes integration tests ────────────────────────────────

    /// Build a temp MemoryGraphStore with two Episode nodes, run
    /// `migrate_episodes`, assert it returns 2 and the adapter holds the
    /// entries with correct titles.
    #[tokio::test]
    async fn migrate_episodes_migrates_two_nodes() {
        // Allow writes for migration tests (suppress freeze guard warnings).
        // Safety: tests run in isolated processes; the env var is only
        // visible within this test binary.
        std::env::set_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES", "1");

        // ── Build in-process MemoryGraphStore ───────────────────────
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory DB");
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let graph = MemoryGraphStore::new(conn);
        graph.ensure_tables();

        // Insert two Episode nodes directly
        let nodes = [
            MemoryNode {
                id: "ep-a".into(),
                space_id: "default".into(),
                kind: MemoryNodeKind::Episode,
                title: "Episode A".into(),
                metadata: Some(serde_json::json!({
                    "task_type": "debugging",
                    "status": "success",
                })),
                created_at: "2026-05-01T00:00:00Z".into(),
                updated_at: "2026-05-01T00:00:00Z".into(),
            },
            MemoryNode {
                id: "ep-b".into(),
                space_id: "default".into(),
                kind: MemoryNodeKind::Episode,
                title: "Episode B".into(),
                metadata: Some(serde_json::json!({
                    "task_type": "code_generation",
                    "status": "partial",
                    "files_changed": ["src/lib.rs"],
                })),
                created_at: "2026-05-02T00:00:00Z".into(),
                updated_at: "2026-05-02T00:00:00Z".into(),
            },
        ];
        for node in &nodes {
            graph.create_node(node).expect("create_node");
        }

        // ── Run migration ────────────────────────────────────────────
        let adapter = InMemoryAdapter::new();
        let spaces = vec!["default".to_string()];
        let count = migrate_episodes(&graph, &adapter, &spaces).await;

        assert_eq!(count, 2, "should migrate exactly 2 nodes");

        // ── Verify adapter contents ───────────────────────────────────
        let ns = "proactive:episode:default";
        let mut entries = adapter.list(Some(ns), None, None).await.unwrap();
        assert_eq!(entries.len(), 2, "adapter should hold 2 entries");

        entries.sort_by(|a, b| a.id.cmp(&b.id));

        let va: serde_json::Value = serde_json::from_str(&entries[0].content).unwrap();
        let vb: serde_json::Value = serde_json::from_str(&entries[1].content).unwrap();

        assert_eq!(va["title"].as_str().unwrap(), "Episode A");
        assert_eq!(vb["title"].as_str().unwrap(), "Episode B");
        assert_eq!(
            vb["files_changed"].as_array().unwrap(),
            &[serde_json::json!("src/lib.rs")]
        );
    }

    /// Running `migrate_episodes` twice must NOT double-insert entries.
    #[tokio::test]
    async fn migrate_episodes_is_idempotent() {
        std::env::set_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES", "1");

        let conn = rusqlite::Connection::open_in_memory().expect("in-memory DB");
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let graph = MemoryGraphStore::new(conn);
        graph.ensure_tables();

        let node = MemoryNode {
            id: "ep-idem".into(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Episode,
            title: "Idempotent episode".into(),
            metadata: None,
            created_at: "2026-05-10T00:00:00Z".into(),
            updated_at: "2026-05-10T00:00:00Z".into(),
        };
        graph.create_node(&node).expect("create_node");

        let adapter = InMemoryAdapter::new();
        let spaces = vec!["default".to_string()];

        // First run
        let first = migrate_episodes(&graph, &adapter, &spaces).await;
        assert_eq!(first, 1);

        // Second run — namespace sentinel fires, migration skipped
        let second = migrate_episodes(&graph, &adapter, &spaces).await;
        assert_eq!(second, 0, "second run should migrate 0 (already done)");

        // Adapter should still have exactly 1 entry
        let entries = adapter
            .list(Some("proactive:episode:default"), None, None)
            .await
            .unwrap();
        assert_eq!(entries.len(), 1);
    }
}
