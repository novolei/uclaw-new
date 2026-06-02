//! One-time, in-process migration: bucket_seal `pages` facade → memory_graph EntityPages.
//! Idempotent (sentinel EntityPage slug as marker). No gbrain dependency.
//!
//! Flow:
//!   1. Check whether the marker EntityPage slug exists in the graph — if so, skip.
//!   2. Call `pages::list_all` (excludes `_migration_marker` pages automatically).
//!   3. For each page, skip if an EntityPage with the same slug already exists.
//!   4. Write via `entity_page_put` (normalises `[[slug]]` + parses frontmatter).
//!   5. On full success, write the sentinel EntityPage marker so the next boot skips.

use std::sync::Arc;

use crate::memory_adapter::MemoryAdapter;
use crate::memory_graph::store::MemoryGraphStore;

/// Sentinel slug written as an EntityPage when the migration completes successfully.
/// Using a sentinel EntityPage mirrors how `find_entity_page_by_slug` can check
/// idempotency without needing a separate store — consistent with the memory_graph
/// EntityPage API (no external marker table required).
const MIGRATION_MARKER: &str = "__pages_to_entitypage_v1__";

/// One-time idempotent migration. Returns number of EntityPages written (0 on skip).
///
/// Never panics; all errors are logged at warn level and the page is skipped.
/// The marker is only written when the migration completes without any per-page errors,
/// matching the `gbrain_page_migration` all_ok guard.
pub async fn migrate_pages_to_entity_graph(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
) -> anyhow::Result<usize> {
    // Idempotency: marker EntityPage already present → skip entire migration.
    match store.find_entity_page_by_slug(space_id, MIGRATION_MARKER) {
        Ok(Some(_)) => {
            tracing::debug!("pages→EntityPage migration: marker present; skipping");
            return Ok(0);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(
                error = %e,
                "pages→EntityPage migration: marker check failed; proceeding anyway"
            );
        }
    }

    let pages = crate::memory_adapter::pages::list_all(adapter)
        .await
        .unwrap_or_default();

    let total = pages.len();
    let mut migrated = 0usize;
    let mut all_ok = true;

    for p in &pages {
        // Skip if an EntityPage with this slug was already created (e.g. by prior
        // reflection or a partial migration run).
        match store.find_entity_page_by_slug(space_id, &p.slug) {
            Ok(Some(_)) => continue,
            Ok(None) => {}
            Err(e) => {
                tracing::warn!(
                    slug = %p.slug,
                    error = %e,
                    "pages→EntityPage: slug-check failed; skipping"
                );
                all_ok = false;
                continue;
            }
        }

        match store.entity_page_put(space_id, &p.slug, &p.body) {
            Ok(_) => migrated += 1,
            Err(e) => {
                tracing::warn!(
                    slug = %p.slug,
                    error = %e,
                    "pages→EntityPage: put failed; skipping"
                );
                all_ok = false;
            }
        }
    }

    // Write the sentinel marker only when every page was processed without error.
    if all_ok {
        let marker_body = "migration marker — do not edit";
        match store.entity_page_put(space_id, MIGRATION_MARKER, marker_body) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "pages→EntityPage: failed to write marker; migration will re-run next boot"
                );
            }
        }
    }

    tracing::info!(
        migrated,
        total,
        all_ok,
        "pages→EntityPage migration complete"
    );
    Ok(migrated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
    use crate::memory_adapter::pages::{put_page, Page};
    use crate::memory_graph::store::MemoryGraphStore;

    // ── Minimal in-process MemoryAdapter (mirrors gbrain_page_migration tests) ──

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
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| {
                    if let Some(ns) = opts.namespace {
                        if e.namespace.as_deref() != Some(ns) {
                            return false;
                        }
                    }
                    let content_lower = e.content.to_lowercase();
                    terms.iter().any(|t| content_lower.contains(t.as_str()))
                })
                .cloned()
                .collect();
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

    // ── Helper: in-memory MemoryGraphStore (mirrors fresh_test_store in store.rs) ──

    fn make_store() -> Arc<MemoryGraphStore> {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1)
            .expect("V35 schema");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(std::sync::Mutex::new(conn))))
    }

    fn page(slug: &str, body: &str) -> Page {
        Page {
            slug: slug.into(),
            title: slug.into(),
            page_type: "note".into(),
            body: body.into(),
            tags: vec![],
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// Happy path: 2 pages in the adapter → both appear as EntityPages, migrated==2.
    #[tokio::test]
    async fn migrates_two_pages_and_writes_marker() {
        let store = make_store();
        let adapter = InMemoryAdapter::new();

        put_page(&adapter, &page("alpha", "# Alpha\nHello world.")).await.unwrap();
        put_page(&adapter, &page("beta", "# Beta\nFoo bar.")).await.unwrap();

        let n = migrate_pages_to_entity_graph(&store, &adapter, "default")
            .await
            .unwrap();
        assert_eq!(n, 2);

        // Both EntityPages should now exist.
        assert!(store.find_entity_page_by_slug("default", "alpha").unwrap().is_some());
        assert!(store.find_entity_page_by_slug("default", "beta").unwrap().is_some());

        // Marker EntityPage should exist.
        assert!(store.find_entity_page_by_slug("default", MIGRATION_MARKER).unwrap().is_some());
    }

    /// Idempotency: running migration a second time returns migrated==0 (marker skip).
    #[tokio::test]
    async fn second_run_is_skipped_via_marker() {
        let store = make_store();
        let adapter = InMemoryAdapter::new();

        put_page(&adapter, &page("gamma", "Some content.")).await.unwrap();

        let first = migrate_pages_to_entity_graph(&store, &adapter, "default")
            .await
            .unwrap();
        assert_eq!(first, 1);

        // Second run: marker present → 0 migrated.
        let second = migrate_pages_to_entity_graph(&store, &adapter, "default")
            .await
            .unwrap();
        assert_eq!(second, 0);
    }

    /// Per-page idempotency: if an EntityPage already exists for a slug,
    /// it is skipped (not double-written). migrated == only the new ones.
    #[tokio::test]
    async fn skips_slugs_already_in_entity_graph() {
        let store = make_store();
        let adapter = InMemoryAdapter::new();

        put_page(&adapter, &page("existing", "Pre-existing content.")).await.unwrap();
        put_page(&adapter, &page("new-one", "Fresh page.")).await.unwrap();

        // Pre-create "existing" in the graph.
        store
            .entity_page_put("default", "existing", "Pre-existing content.")
            .unwrap();

        let n = migrate_pages_to_entity_graph(&store, &adapter, "default")
            .await
            .unwrap();
        // Only "new-one" should be written; "existing" is skipped.
        assert_eq!(n, 1);
        assert!(store
            .find_entity_page_by_slug("default", "new-one")
            .unwrap()
            .is_some());
    }

    /// Empty adapter → migrated==0, marker is still written (all_ok=true).
    #[tokio::test]
    async fn empty_adapter_writes_marker_and_returns_zero() {
        let store = make_store();
        let adapter = InMemoryAdapter::new();

        let n = migrate_pages_to_entity_graph(&store, &adapter, "default")
            .await
            .unwrap();
        assert_eq!(n, 0);
        // Marker must be written even with zero pages.
        assert!(store
            .find_entity_page_by_slug("default", MIGRATION_MARKER)
            .unwrap()
            .is_some());
    }
}
