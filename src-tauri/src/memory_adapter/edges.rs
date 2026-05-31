//! Thin graph-edge facade over `MemoryAdapter` (convergence ADR P1b).
//! Free functions — NOT trait methods — over the existing store/list. Undirected
//! (matches co-used-tools). No live wiring yet; P3 repoints tool_memory here.
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const EDGES_NAMESPACE: &str = "edges";

/// An undirected graph edge between two node ids, with a relation kind.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub kind: String,
}

/// Canonical undirected key: kind + endpoints sorted, so `relate(a,b,k)` and
/// `relate(b,a,k)` collide (idempotent symmetric dedup). `\u{1}` separates fields.
fn edge_key(a: &str, b: &str, kind: &str) -> String {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    format!("{kind}\u{1}{lo}\u{1}{hi}")
}

/// Create (or overwrite) an undirected edge. Idempotent on (from,to,kind) up to order.
pub async fn relate(adapter: &Arc<dyn MemoryAdapter>, from: &str, to: &str, kind: &str) -> anyhow::Result<()> {
    let edge = Edge { from: from.to_string(), to: to.to_string(), kind: kind.to_string() };
    let content = serde_json::to_string(&edge)?;
    adapter
        .store(EDGES_NAMESPACE, &edge_key(from, to, kind), &content, MemoryCategory::Core, None)
        .await
}

/// Neighbors of `node` (the other endpoint of each incident edge), optionally
/// filtered by `kind`. Deduped. List-scans the "edges" namespace (fine for the
/// small co-used graph; an index is a later optimization). Unparseable entries skipped.
pub async fn neighbors(adapter: &Arc<dyn MemoryAdapter>, node: &str, kind: Option<&str>) -> anyhow::Result<Vec<String>> {
    let entries = adapter.list(Some(EDGES_NAMESPACE), None, None).await?;
    let mut out = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for e in entries {
        let edge: Edge = match serde_json::from_str(&e.content) {
            Ok(ed) => ed,
            Err(_) => continue,
        };
        if let Some(k) = kind {
            if edge.kind != k { continue; }
        }
        let other = if edge.from == node {
            Some(edge.to)
        } else if edge.to == node {
            Some(edge.from)
        } else {
            None
        };
        if let Some(o) = other {
            if seen.insert(o.clone()) { out.push(o); }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── Minimal in-process adapter for tests ────────────────────────────

    /// Thread-safe in-memory `MemoryAdapter` used for unit tests.
    /// Stores entries in a HashMap; `list` honors the namespace filter.
    struct InMemoryAdapter {
        /// (namespace, key) → MemoryEntry
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Self {
            Self {
                store: Mutex::new(HashMap::new()),
            }
        }
    }

    #[async_trait]
    impl MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str {
            "in_memory_test"
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

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn relate_then_neighbors_is_symmetric() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        relate(&a, "x", "y", "relates_to").await.unwrap();
        assert_eq!(neighbors(&a, "x", None).await.unwrap(), vec!["y".to_string()]);
        assert_eq!(neighbors(&a, "y", None).await.unwrap(), vec!["x".to_string()]);
    }

    #[tokio::test]
    async fn relate_is_idempotent_and_order_insensitive() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        relate(&a, "x", "y", "relates_to").await.unwrap();
        relate(&a, "x", "y", "relates_to").await.unwrap();
        relate(&a, "y", "x", "relates_to").await.unwrap();
        assert_eq!(a.list(Some("edges"), None, None).await.unwrap().len(), 1);
        assert_eq!(neighbors(&a, "x", None).await.unwrap(), vec!["y".to_string()]);
    }

    #[tokio::test]
    async fn neighbors_dedupes_and_collects_multiple() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        relate(&a, "a", "b", "relates_to").await.unwrap();
        relate(&a, "a", "c", "relates_to").await.unwrap();
        let mut ns = neighbors(&a, "a", None).await.unwrap();
        ns.sort();
        assert_eq!(ns, vec!["b".to_string(), "c".to_string()]);
    }

    #[tokio::test]
    async fn neighbors_filters_by_kind() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        relate(&a, "a", "b", "relates_to").await.unwrap();
        relate(&a, "a", "c", "co_used").await.unwrap();
        assert_eq!(neighbors(&a, "a", Some("relates_to")).await.unwrap(), vec!["b".to_string()]);
    }

    #[tokio::test]
    async fn neighbors_unknown_node_is_empty() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        relate(&a, "a", "b", "relates_to").await.unwrap();
        assert!(neighbors(&a, "zzz", None).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn neighbors_skips_malformed_entries() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        a.store("edges", "junk", "not json", MemoryCategory::Core, None).await.unwrap();
        relate(&a, "a", "b", "relates_to").await.unwrap();
        assert_eq!(neighbors(&a, "a", None).await.unwrap(), vec!["b".to_string()]);
    }
}
