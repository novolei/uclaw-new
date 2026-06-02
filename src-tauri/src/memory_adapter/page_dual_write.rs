//! Two-layer page write primitive. `write_page` is the authoritative path:
//! it writes to `memory_graph` EntityPage (rich, WikiView) and shadow-writes
//! to the `bucket_seal` adapter `"pages"` namespace (best-effort recall). The
//! gbrain dual-write shadow mechanism was removed in Step 2b.

use std::sync::Arc;

use crate::gbrain::browse;
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

/// The adapter half of the two-layer write, extracted so it is unit-testable
/// without going through the full `write_page` call. Best-effort: an adapter
/// error is logged and swallowed.
///
/// `caller` is a short tag included in the warning log for attribution.
pub(crate) async fn shadow_write_page(
    adapter: &Arc<dyn MemoryAdapter>,
    slug: &str,
    markdown: &str,
    caller: &str,
) {
    let page = markdown_to_page(slug, markdown);
    if let Err(e) = pages::put_page(adapter, &page).await {
        tracing::warn!(slug, caller, error = %e, "shadow_write_page to adapter pages failed (authoritative write ok)");
    }
}

/// Write a page to BOTH layers: memory_graph EntityPage (rich, WikiView) +
/// bucket_seal `pages` (recall projection). Replaces the gbrain dual-write.
pub async fn write_page(
    store: &Arc<crate::memory_graph::store::MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    slug: &str,
    markdown: &str,
) -> anyhow::Result<()> {
    store.entity_page_put(space_id, slug, markdown)?;                      // authoritative
    shadow_write_page(adapter, slug, markdown, "write_page").await;        // best-effort bucket_seal
    Ok(())
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
        shadow_write_page(&adapter, "slug-1", md, "test").await;
        let got = pages::get_page(&adapter, "slug-1").await.unwrap().expect("page present");
        assert_eq!(got.title, "T");
        assert_eq!(got.body, md);
    }

    /// Build an in-memory MemoryGraphStore suitable for use in page_dual_write tests.
    /// Mirrors `fresh_test_store()` in memory_graph/store.rs (not importable here).
    fn build_test_store() -> Arc<crate::memory_graph::store::MemoryGraphStore> {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).expect("V35 schema");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(crate::memory_graph::store::MemoryGraphStore::new(
            Arc::new(Mutex::new(conn)),
        ))
    }

    #[tokio::test]
    async fn write_page_creates_entity_page_and_bucket_seal_entry() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();
        let md = "---\ntitle: T\n---\n\nbody";

        write_page(&store, &adapter, "default", "test-slug", md)
            .await
            .expect("write_page should succeed");

        // EntityPage was created in memory_graph
        let ep = store
            .find_entity_page_by_slug("default", "test-slug")
            .expect("find_entity_page_by_slug should not error")
            .expect("EntityPage should exist");
        assert_eq!(ep.node.space_id, "default");
        // slug is in metadata; confirm the active version contains the expected body
        let content = ep
            .active_version
            .as_ref()
            .expect("active_version should exist")
            .content
            .clone();
        assert!(
            content.contains("body"),
            "active version content should contain 'body', got: {content}"
        );

        // bucket_seal page was written
        let page = pages::get_page(&adapter, "test-slug")
            .await
            .expect("get_page should not error")
            .expect("bucket_seal page should exist");
        assert_eq!(page.body, md);
    }

    #[tokio::test]
    async fn write_page_upsert_creates_new_version() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();
        let md1 = "---\ntitle: T\n---\n\nfirst body";
        let md2 = "---\ntitle: T\n---\n\nsecond body";

        write_page(&store, &adapter, "default", "upsert-slug", md1)
            .await
            .expect("first write_page should succeed");

        write_page(&store, &adapter, "default", "upsert-slug", md2)
            .await
            .expect("second write_page should succeed");

        // EntityPage still exists (not deleted)
        let ep = store
            .find_entity_page_by_slug("default", "upsert-slug")
            .expect("find_entity_page_by_slug should not error")
            .expect("EntityPage should exist after upsert");

        // The active version should reflect the new body
        let active_content = ep
            .active_version
            .as_ref()
            .expect("active_version should exist")
            .content
            .clone();
        assert!(
            active_content.contains("second body"),
            "active version should contain second body, got: {active_content}"
        );

        // At least 2 versions exist (initial + upsert)
        let versions = store
            .entity_page_versions(&ep.node.id)
            .expect("entity_page_versions should not error");
        assert!(
            versions.len() >= 2,
            "expected at least 2 versions, got {}",
            versions.len()
        );

        // bucket_seal reflects the latest write
        let page = pages::get_page(&adapter, "upsert-slug")
            .await
            .expect("get_page should not error")
            .expect("bucket_seal page should exist after upsert");
        assert_eq!(page.body, md2);
    }
}
