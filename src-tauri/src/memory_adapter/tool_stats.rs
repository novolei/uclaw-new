//! P3-edges — per-tool usage stats facade over `MemoryAdapter`. Stores the raw
//! ToolNodeStats accumulator (derived ToolStats fields are computed by the caller).
//! Space-qualified key in the "tool_stats" namespace. Mirrors skills.rs.

use std::collections::HashMap;
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const TOOL_STATS_NAMESPACE: &str = "tool_stats";

fn stats_key(space: &str, tool_name: &str) -> String {
    format!("{space}\u{1}{tool_name}")
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct ToolStatsRecord {
    pub space: String,
    pub tool_name: String,
    #[serde(default)]
    pub total_uses: u64,
    #[serde(default)]
    pub success_count: u64,
    #[serde(default)]
    pub failure_count: u64,
    #[serde(default)]
    pub total_latency_ms: u64,
    #[serde(default)]
    pub output_sizes: Vec<u64>,
    #[serde(default)]
    pub parameter_fingerprints: HashMap<String, u64>,
    #[serde(default)]
    pub last_used_at: String,
}

pub async fn put_stats(adapter: &Arc<dyn MemoryAdapter>, rec: &ToolStatsRecord) -> anyhow::Result<()> {
    let content = serde_json::to_string(rec)?;
    adapter
        .store(
            TOOL_STATS_NAMESPACE,
            &stats_key(&rec.space, &rec.tool_name),
            &content,
            MemoryCategory::Core,
            None,
        )
        .await
}

pub async fn get_stats(
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    tool_name: &str,
) -> anyhow::Result<Option<ToolStatsRecord>> {
    match adapter.get(TOOL_STATS_NAMESPACE, &stats_key(space_id, tool_name)).await? {
        Some(e) => Ok(serde_json::from_str::<ToolStatsRecord>(&e.content).ok()),
        None => Ok(None),
    }
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
        fn new() -> Arc<dyn MemoryAdapter> {
            Arc::new(Self {
                store: Mutex::new(HashMap::new()),
            })
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

    // ── Test helpers ─────────────────────────────────────────────────────

    fn rec(space: &str, tool: &str, uses: u64) -> ToolStatsRecord {
        ToolStatsRecord {
            space: space.into(),
            tool_name: tool.into(),
            total_uses: uses,
            success_count: uses,
            failure_count: 0,
            total_latency_ms: 10 * uses,
            output_sizes: vec![],
            parameter_fingerprints: HashMap::new(),
            last_used_at: "t".into(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn put_get_round_trips() {
        let a = InMemoryAdapter::new();
        put_stats(&a, &rec("sp", "write_file", 3)).await.unwrap();
        let got = get_stats(&a, "sp", "write_file").await.unwrap().unwrap();
        assert_eq!((got.total_uses, got.success_count), (3, 3));
    }

    #[tokio::test]
    async fn space_isolation_and_absent() {
        let a = InMemoryAdapter::new();
        put_stats(&a, &rec("s1", "t", 1)).await.unwrap();
        put_stats(&a, &rec("s2", "t", 9)).await.unwrap();
        assert_eq!(get_stats(&a, "s1", "t").await.unwrap().unwrap().total_uses, 1);
        assert_eq!(get_stats(&a, "s2", "t").await.unwrap().unwrap().total_uses, 9);
        assert!(get_stats(&a, "s1", "absent").await.unwrap().is_none());
    }
}
