// SPDX-License-Identifier: MIT
//! DEPRECATED (2026-05-31): retained for explicit-namespace back-compat only; see memory_adapter/mod.rs roster. New code must not route here.
//!
//! `LegacyKvAdapter` — wraps `crate::memory::MemoryStore` (legacy SQLite
//! KV + FTS) behind the `MemoryAdapter` trait.
//!
//! PR2 of 阶段 4. The legacy `MemoryStore` stays as-is; this adapter
//! just translates `MemoryAdapter` calls into the existing API and
//! converts the legacy `MemoryEntry` shape into the adapter shape.
//!
//! Sync→async: SQLite operations are fast; the impl runs the legacy
//! sync methods inline inside `async fn`s. If contention ever shows
//! up, swap to `tokio::task::spawn_blocking` per call.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;

use super::traits::MemoryAdapter;
use super::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

use crate::memory::{ListFilter, MemoryEntry as LegacyEntry, MemoryKind, MemoryStore, SetMemoryOpts};

const ADAPTER_NAME: &str = "legacy_kv";
const DEFAULT_SPACE_ID: &str = "global";

/// Wraps `crate::memory::MemoryStore` and exposes it through the
/// `MemoryAdapter` trait. The legacy store stays the source of truth;
/// this is purely a translation layer.
#[derive(Clone)]
pub struct LegacyKvAdapter {
    inner: Arc<MemoryStore>,
}

impl LegacyKvAdapter {
    pub fn new(inner: Arc<MemoryStore>) -> Self {
        Self { inner }
    }

    /// Convert legacy `MemoryEntry` to the trait's owned shape.
    fn convert_entry(legacy: LegacyEntry, score: Option<f64>) -> MemoryEntry {
        let category = match MemoryKind::from_str(&legacy.kind) {
            MemoryKind::Fact | MemoryKind::Preference => MemoryCategory::Core,
            MemoryKind::Context | MemoryKind::Note => MemoryCategory::Conversation,
            MemoryKind::Procedure => MemoryCategory::Custom("procedure".to_string()),
        };

        // Extract session_id from legacy "session:<id>" namespace convention.
        let session_id = legacy
            .namespace
            .strip_prefix("session:")
            .map(|s| s.to_string());

        let content = match &legacy.value {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };

        MemoryEntry {
            id: legacy.id,
            key: legacy.key,
            content,
            namespace: Some(legacy.namespace),
            category,
            timestamp: legacy.updated_at,
            session_id,
            score,
        }
    }

    fn category_to_kind(cat: &MemoryCategory) -> MemoryKind {
        match cat {
            MemoryCategory::Core => MemoryKind::Fact,
            MemoryCategory::Conversation => MemoryKind::Context,
            MemoryCategory::Daily => MemoryKind::Note,
            MemoryCategory::Custom(name) if name == "procedure" => MemoryKind::Procedure,
            MemoryCategory::Custom(_) => MemoryKind::Note,
        }
    }
}

#[async_trait]
impl MemoryAdapter for LegacyKvAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let effective_namespace = match session_id {
            Some(sid) => format!("session:{}", sid),
            None => namespace.to_string(),
        };
        let opts = SetMemoryOpts {
            space_id: DEFAULT_SPACE_ID.to_string(),
            namespace: effective_namespace,
            key: key.to_string(),
            value: serde_json::Value::String(content.to_string()),
            kind: Self::category_to_kind(&category),
            tags: Vec::new(),
            metadata: None,
            ttl_seconds: None,
        };
        self.inner
            .set_full(opts)
            .map(|_| ())
            .map_err(|e| anyhow::anyhow!("legacy_kv::store: {}", e))
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // `MemoryStore::search` returns Vec<LegacyEntry> without scores;
        // for PR2 we don't expose a score (real ranking is for
        // BucketSealAdapter to provide). Filter by category client-side
        // since legacy `kind` is a free-form string per row.
        let hits = self.inner.search(query, opts.namespace, limit);
        let mut out = Vec::with_capacity(hits.len());
        for h in hits.into_iter() {
            let entry = Self::convert_entry(h, None);
            if let Some(cat) = opts.category.as_ref() {
                if &entry.category != cat {
                    continue;
                }
            }
            if let Some(sid) = opts.session_id {
                if entry.session_id.as_deref() != Some(sid) {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn get(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<Option<MemoryEntry>> {
        Ok(self
            .inner
            .get(key, namespace)
            .map(|legacy| Self::convert_entry(legacy, None)))
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let effective_namespace = match (namespace, session_id) {
            (_, Some(sid)) => Some(format!("session:{}", sid)),
            (Some(ns), None) => Some(ns.to_string()),
            (None, None) => None,
        };
        let kind_filter = category.map(|c| Self::category_to_kind(c).as_str().to_string());
        let filter = ListFilter {
            space_id: None,
            namespace: effective_namespace,
            kind: kind_filter,
            tag: None,
            limit: None,
            offset: None,
        };
        Ok(self
            .inner
            .list_filtered(&filter)
            .into_iter()
            .map(|legacy| Self::convert_entry(legacy, None))
            .collect())
    }

    async fn delete(
        &self,
        namespace: &str,
        key: &str,
    ) -> anyhow::Result<bool> {
        Ok(self.inner.delete(key, namespace))
    }

    async fn clear_namespace(
        &self,
        namespace: &str,
    ) -> anyhow::Result<u64> {
        let removed = self.inner.clear_namespace(namespace, None);
        Ok(removed as u64)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let namespaces = self.inner.list_namespaces(None);
        let now = Utc::now().to_rfc3339();
        let mut out = Vec::with_capacity(namespaces.len());
        for ns in namespaces {
            let filter = ListFilter {
                space_id: None,
                namespace: Some(ns.clone()),
                kind: None,
                tag: None,
                limit: None,
                offset: None,
            };
            let count = self.inner.count(&filter);
            out.push(NamespaceSummary {
                namespace: ns,
                count,
                // Legacy MemoryStore doesn't expose per-namespace
                // last_updated cheaply; report current time as a
                // placeholder (the trait field is `last_updated: Option`).
                // PR9+ BucketSealAdapter will provide accurate values.
                last_updated: Some(now.clone()),
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryStore> {
        let conn = Connection::open_in_memory().unwrap();
        let store = MemoryStore::new(Arc::new(Mutex::new(conn)));
        // ensure_table returns () — no .unwrap() needed
        store.ensure_table();
        Arc::new(store)
    }

    #[tokio::test]
    async fn name_is_legacy_kv() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        assert_eq!(adapter.name(), "legacy_kv");
    }

    #[tokio::test]
    async fn store_and_get_round_trip() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store(
                "global",
                "favorite_color",
                "blue",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        let got = adapter.get("global", "favorite_color").await.unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert_eq!(entry.key, "favorite_color");
        assert_eq!(entry.content, "blue");
        assert_eq!(entry.category, MemoryCategory::Core);
        assert_eq!(entry.namespace.as_deref(), Some("global"));
    }

    #[tokio::test]
    async fn session_id_routes_to_session_namespace() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store(
                "ignored",
                "current_task",
                "fix the bug",
                MemoryCategory::Conversation,
                Some("sess-42"),
            )
            .await
            .unwrap();
        // The store should NOT find it under "ignored"
        assert!(adapter.get("ignored", "current_task").await.unwrap().is_none());
        // But it SHOULD find it under "session:sess-42"
        let got = adapter
            .get("session:sess-42", "current_task")
            .await
            .unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert_eq!(entry.session_id.as_deref(), Some("sess-42"));
    }

    #[tokio::test]
    async fn list_filters_by_category() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "a", "fact1", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns", "b", "note1", MemoryCategory::Conversation, None)
            .await
            .unwrap();

        let cores = adapter
            .list(Some("ns"), Some(&MemoryCategory::Core), None)
            .await
            .unwrap();
        assert_eq!(cores.len(), 1);
        assert_eq!(cores[0].key, "a");
    }

    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "k", "v", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert!(adapter.delete("ns", "k").await.unwrap());
        assert!(!adapter.delete("ns", "k").await.unwrap());
    }

    #[tokio::test]
    async fn clear_namespace_removes_entries() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter.store("ns", "a", "1", MemoryCategory::Core, None).await.unwrap();
        adapter.store("ns", "b", "2", MemoryCategory::Core, None).await.unwrap();
        adapter.store("other", "c", "3", MemoryCategory::Core, None).await.unwrap();
        let removed = adapter.clear_namespace("ns").await.unwrap();
        assert_eq!(removed, 2);
        assert!(adapter.get("ns", "a").await.unwrap().is_none());
        assert!(adapter.get("other", "c").await.unwrap().is_some());
    }

    #[tokio::test]
    async fn recall_finds_by_content() {
        let adapter = LegacyKvAdapter::new(fresh_store());
        adapter
            .store("ns", "k1", "the quick brown fox", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns", "k2", "lazy dog sleeps", MemoryCategory::Core, None)
            .await
            .unwrap();
        let hits = adapter
            .recall("quick", 10, RecallOpts { namespace: Some("ns"), ..Default::default() })
            .await
            .unwrap();
        assert!(hits.iter().any(|e| e.key == "k1"));
    }
}
