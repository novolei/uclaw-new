//! Thin page-knowledge facade over `MemoryAdapter` (convergence ADR P1a).
//! Free functions — NOT a trait method — so they work over any adapter via the
//! existing store/get/recall. No live wiring yet; P2 repoints gbrain here.
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory, RecallOpts};

const PAGES_NAMESPACE: &str = "pages";

/// A knowledge page — the core subset of gbrain's `PageDetail` the adapter layer
/// persists. (auto-link / compiled_truth / frontmatter richness is NOT modeled
/// here — re-derived or dropped in P2.)
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Page {
    pub slug: String,
    pub title: String,
    #[serde(default)]
    pub page_type: String,
    pub body: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

/// A page search hit (mirrors gbrain's `SearchHit` shape for a clean P2 repoint).
#[derive(Debug, Clone, PartialEq)]
pub struct PageHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,
}

/// Store/overwrite a page. content = JSON(Page), key = slug, namespace = "pages".
pub async fn put_page(adapter: &Arc<dyn MemoryAdapter>, page: &Page) -> anyhow::Result<()> {
    let content = serde_json::to_string(page)?;
    adapter
        .store(PAGES_NAMESPACE, &page.slug, &content, MemoryCategory::Core, None)
        .await
}

/// Fetch a page by slug. `None` if absent or content isn't a valid `Page`.
pub async fn get_page(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<Option<Page>> {
    match adapter.get(PAGES_NAMESPACE, slug).await? {
        Some(entry) => Ok(serde_json::from_str::<Page>(&entry.content).ok()),
        None => Ok(None),
    }
}

/// Search pages by query; `snippet` = body truncated to 200 chars. Unparseable entries skipped.
pub async fn search_pages(
    adapter: &Arc<dyn MemoryAdapter>,
    query: &str,
    limit: usize,
) -> anyhow::Result<Vec<PageHit>> {
    let opts = RecallOpts { namespace: Some(PAGES_NAMESPACE), ..Default::default() };
    let entries = adapter.recall(query, limit, opts).await?;
    Ok(entries
        .into_iter()
        .filter_map(|e| {
            let page: Page = serde_json::from_str(&e.content).ok()?;
            let snippet: String = page.body.chars().take(200).collect();
            Some(PageHit { slug: page.slug, title: page.title, snippet })
        })
        .collect())
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
    /// Stores entries in a HashMap; `recall` does namespace-scoped substring match.
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
            query: &str,
            limit: usize,
            opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            // Split query on whitespace so that any individual term can match
            // (mirrors FTS5 OR semantics for the in-memory test adapter).
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| {
                    // Namespace filter
                    if let Some(ns) = opts.namespace {
                        if e.namespace.as_deref() != Some(ns) {
                            return false;
                        }
                    }
                    // Any term matches anywhere in content
                    let content_lower = e.content.to_lowercase();
                    terms.iter().any(|t| content_lower.contains(t.as_str()))
                })
                .cloned()
                .collect();
            // Stable ordering for deterministic tests
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out.truncate(limit);
            Ok(out)
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

    fn page(slug: &str, title: &str, body: &str) -> Page {
        Page {
            slug: slug.into(),
            title: title.into(),
            page_type: "note".into(),
            body: body.into(),
            tags: vec!["t".into()],
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn put_then_get_round_trips_all_fields() {
        let a: Arc<dyn MemoryAdapter> = InMemoryAdapter::new();
        let p = page("intro", "Intro", "hello world");
        put_page(&a, &p).await.unwrap();
        assert_eq!(get_page(&a, "intro").await.unwrap(), Some(p));
    }

    #[tokio::test]
    async fn get_absent_is_none() {
        let a: Arc<dyn MemoryAdapter> = InMemoryAdapter::new();
        assert_eq!(get_page(&a, "nope").await.unwrap(), None);
    }

    #[tokio::test]
    async fn get_malformed_content_is_none() {
        let a: Arc<dyn MemoryAdapter> = InMemoryAdapter::new();
        a.store("pages", "bad", "not json", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert_eq!(get_page(&a, "bad").await.unwrap(), None);
    }

    #[tokio::test]
    async fn search_returns_hits_with_truncated_snippet() {
        let a: Arc<dyn MemoryAdapter> = InMemoryAdapter::new();
        put_page(&a, &page("a", "Alpha", &"x".repeat(500))).await.unwrap();
        let hits = search_pages(&a, "x", 10).await.unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].slug, "a");
        assert!(hits[0].snippet.chars().count() <= 200);
    }

    #[test]
    fn page_serde_round_trip_and_defaults() {
        let json = r#"{"slug":"s","title":"T","body":"b"}"#;
        let p: Page = serde_json::from_str(json).unwrap();
        assert_eq!(p.page_type, "");
        assert!(p.tags.is_empty());
    }
}
