//! P2a-1 — gated, best-effort dual-write of gbrain pages into the adapter
//! `"pages"` namespace. gbrain stays the PRIMARY write; the adapter copy is a
//! shadow that can never fail the primary. Convergence ADR Phase P2, sub-slice P2a-1.

use std::sync::Arc;

use crate::gbrain::browse::{self, GbrainError, PageDetail};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{pages, MemoryAdapter};

/// Pure map: a raw gbrain markdown page (frontmatter + body) → the adapter
/// `Page`. Mirrors P2b's `page_detail_to_page`: `body` is the full raw markdown
/// (the authoritative editable source); title/page_type/tags come from the YAML
/// frontmatter, with slug-fallback for the title.
pub(crate) fn markdown_to_page(slug: &str, markdown: &str) -> pages::Page {
    let (fm, _body) = browse::split_frontmatter(markdown);
    let title = fm
        .get("title")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| slug.to_string());
    let page_type = fm
        .get("page_type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let tags = fm
        .get("tags")
        .and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    pages::Page {
        slug: slug.to_string(),
        title,
        page_type,
        body: markdown.to_string(),
        tags,
    }
}

/// The adapter half of the dual-write, extracted so it is unit-testable without
/// the MCP call. Best-effort: an adapter error is logged and swallowed.
pub(crate) async fn shadow_write_page(
    adapter: &Arc<dyn MemoryAdapter>,
    slug: &str,
    markdown: &str,
) {
    let page = markdown_to_page(slug, markdown);
    if let Err(e) = pages::put_page(adapter, &page).await {
        tracing::warn!(slug, error = %e, "dual-write shadow to adapter pages failed (gbrain primary ok)");
    }
}

/// Write a page to gbrain (PRIMARY — its `Result` is returned unchanged), and —
/// when `dual_write_enabled` and a handle is present — ALSO shadow-write it to
/// the adapter `"pages"` namespace (best-effort, never fails the primary).
pub async fn dual_write_page(
    mcp: &SharedMcpManager,
    adapter: Option<&Arc<dyn MemoryAdapter>>,
    slug: &str,
    markdown: &str,
    dual_write_enabled: bool,
) -> Result<PageDetail, GbrainError> {
    let res = browse::put_page(mcp, slug, markdown).await;
    if dual_write_enabled {
        if let Some(a) = adapter {
            shadow_write_page(a, slug, markdown).await;
        }
    }
    res
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

    #[test]
    fn markdown_to_page_from_frontmatter() {
        let md = "---\ntitle: My Page\npage_type: note\ntags:\n  - x\n---\n\nhello";
        let p = markdown_to_page("a/b", md);
        assert_eq!(p.slug, "a/b");
        assert_eq!(p.title, "My Page");
        assert_eq!(p.page_type, "note");
        assert_eq!(p.tags, vec!["x".to_string()]);
        assert_eq!(p.body, md); // full raw markdown preserved
    }

    #[test]
    fn markdown_to_page_no_frontmatter_uses_slug_title() {
        let md = "plain body";
        let p = markdown_to_page("my-slug", md);
        assert_eq!(p.title, "my-slug");
        assert_eq!(p.page_type, "");
        assert!(p.tags.is_empty());
        assert_eq!(p.body, md);
    }

    #[tokio::test]
    async fn shadow_write_round_trips_into_pages_namespace() {
        let adapter = InMemoryAdapter::new();
        let md = "---\ntitle: T\n---\n\nbody";
        shadow_write_page(&adapter, "slug-1", md).await;
        let got = pages::get_page(&adapter, "slug-1").await.unwrap().expect("page present");
        assert_eq!(got.title, "T");
        assert_eq!(got.body, md);
    }
}
