//! P2b — one-time, non-destructive migration of gbrain knowledge pages into the
//! adapter "pages" namespace (convergence ADR, Phase P2). Reads gbrain via
//! `browse::*`, writes via the `pages` facade. gbrain is untouched + stays primary.
use std::sync::Arc;

use crate::gbrain::browse::{self, PageDetail};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{pages, MemoryAdapter};

// P2c-1 re-sync: bumped v1 → v2 so the boot migration runs one more full
// idempotent pass, backfilling any gbrain page not yet in bucket_seal before the
// passive-recall gbrain leg retires. Sets v2 on success → skips thereafter.
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v2__";
const LIST_ALL_LIMIT: u32 = 100_000;

/// Pure map: gbrain `PageDetail` → adapter `Page`. body = raw_markdown (authoritative
/// source), falling back to compiled_truth when raw is empty.
pub fn page_detail_to_page(p: &PageDetail) -> pages::Page {
    let body = if p.raw_markdown.trim().is_empty() {
        p.compiled_truth.clone()
    } else {
        p.raw_markdown.clone()
    };
    pages::Page {
        slug: p.slug.clone(),
        title: p.title.clone(),
        page_type: p.page_type.clone(),
        body,
        tags: p.tags.clone(),
    }
}

/// Returns `true` if the completion marker is already present in the adapter.
/// Extracted as a `pub(crate)` seam so it can be unit-tested without a live
/// gbrain MCP connection.
pub(crate) async fn already_migrated(adapter: &Arc<dyn MemoryAdapter>) -> bool {
    matches!(
        pages::get_page(adapter, MIGRATION_MARKER_SLUG).await,
        Ok(Some(_))
    )
}

/// Testable seam: write each detail into the adapter via the pages facade.
/// Returns (migrated_count, all_ok). all_ok=false if any put failed.
pub async fn apply_page_details(
    adapter: &Arc<dyn MemoryAdapter>,
    details: Vec<PageDetail>,
) -> (usize, bool) {
    let mut migrated = 0usize;
    let mut all_ok = true;
    for d in &details {
        let page = page_detail_to_page(d);
        match pages::put_page(adapter, &page).await {
            Ok(()) => migrated += 1,
            Err(e) => {
                tracing::warn!(
                    slug = %page.slug,
                    error = %e,
                    "gbrain page migration: put_page failed; skip"
                );
                all_ok = false;
            }
        }
    }
    (migrated, all_ok)
}

/// One-time idempotent non-destructive migration. Returns migrated count.
pub async fn migrate_gbrain_pages(
    mcp: &SharedMcpManager,
    adapter: &Arc<dyn MemoryAdapter>,
) -> usize {
    if already_migrated(adapter).await {
        tracing::debug!("gbrain page migration: completion marker present; skipping");
        return 0;
    }

    let summaries = match browse::list_pages(mcp, LIST_ALL_LIMIT, None, None, None, None).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(
                error = e.to_command_string(),
                "gbrain page migration: list_pages failed (gbrain absent?); skip"
            );
            return 0;
        }
    };

    if summaries.len() as u32 == LIST_ALL_LIMIT {
        tracing::warn!(
            limit = LIST_ALL_LIMIT,
            "gbrain page migration: list hit the limit — possible truncation"
        );
    }

    let mut details = Vec::with_capacity(summaries.len());
    let mut gets_ok = true;
    for s in &summaries {
        match browse::get_page(mcp, &s.slug).await {
            Ok(d) => details.push(d),
            Err(e) => {
                tracing::warn!(
                    slug = %s.slug,
                    error = e.to_command_string(),
                    "gbrain page migration: get_page failed; skip"
                );
                gets_ok = false;
            }
        }
    }

    let (migrated, apply_ok) = apply_page_details(adapter, details).await;

    let full = gets_ok && apply_ok && (summaries.len() as u32) < LIST_ALL_LIMIT;
    if full {
        let marker = pages::Page {
            slug: MIGRATION_MARKER_SLUG.into(),
            title: "gbrain pages migrated (P2b)".into(),
            page_type: "_migration_marker".into(),
            body: String::new(),
            tags: vec![],
        };
        let _ = pages::put_page(adapter, &marker).await;
    }

    tracing::info!(migrated, full, "gbrain page migration pass complete");
    migrated
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── Minimal in-process adapter (copied from memory_adapter/pages.rs tests) ──

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

    // ── Test helpers ──────────────────────────────────────────────────────────

    /// Build a minimal `PageDetail` for tests. `raw` is placed in `raw_markdown`;
    /// `compiled_truth` is always "compiled". All optional fields are filled with
    /// sensible defaults so future field additions compile without test churn.
    fn detail(slug: &str, title: &str, raw: &str) -> PageDetail {
        PageDetail {
            slug: slug.into(),
            title: title.into(),
            page_type: "note".into(),
            compiled_truth: "compiled".into(),
            frontmatter: serde_json::Value::Null,
            created_at: Some("2026-01-01T00:00:00Z".into()),
            updated_at: Some("2026-01-01T00:00:00Z".into()),
            tags: vec!["t".into()],
            raw_markdown: raw.into(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    #[test]
    fn page_detail_to_page_uses_raw_then_falls_back() {
        // Non-empty raw → body = raw
        let d = detail("slug1", "Title", "real raw content");
        let p = page_detail_to_page(&d);
        assert_eq!(p.body, "real raw content");

        // Whitespace-only raw → body = compiled_truth
        let d_blank = detail("slug2", "Title", "   ");
        let p_blank = page_detail_to_page(&d_blank);
        assert_eq!(p_blank.body, "compiled");
    }

    #[tokio::test]
    async fn apply_page_details_writes_and_reports_ok() {
        let adapter = InMemoryAdapter::new();
        let details = vec![
            detail("alpha", "Alpha", "body alpha"),
            detail("beta", "Beta", "body beta"),
        ];
        let (count, all_ok) = apply_page_details(&adapter, details).await;
        assert_eq!(count, 2);
        assert!(all_ok);

        let got_alpha = pages::get_page(&adapter, "alpha").await.unwrap();
        assert!(got_alpha.is_some());
        assert_eq!(got_alpha.unwrap().body, "body alpha");

        let got_beta = pages::get_page(&adapter, "beta").await.unwrap();
        assert!(got_beta.is_some());
        assert_eq!(got_beta.unwrap().slug, "beta");
    }

    #[tokio::test]
    async fn already_migrated_returns_true_when_marker_present() {
        let adapter = InMemoryAdapter::new();

        // Before marker: not migrated
        assert!(!already_migrated(&adapter).await);

        // Store the marker via pages facade
        let marker = pages::Page {
            slug: MIGRATION_MARKER_SLUG.into(),
            title: "gbrain pages migrated (P2b)".into(),
            page_type: "_migration_marker".into(),
            body: String::new(),
            tags: vec![],
        };
        pages::put_page(&adapter, &marker).await.unwrap();

        // After marker: already migrated
        assert!(already_migrated(&adapter).await);
    }

    /// The simple InMemoryAdapter stub cannot be made to fail a put, so the
    /// all_ok=false path from apply_page_details cannot be unit-tested here.
    /// The plumbing is exercised by the happy-path test above (all_ok=true path);
    /// the false branch is covered by code inspection + integration testing.
    #[tokio::test]
    async fn apply_empty_details_returns_zero_ok() {
        let adapter = InMemoryAdapter::new();
        let (count, all_ok) = apply_page_details(&adapter, vec![]).await;
        assert_eq!(count, 0);
        assert!(all_ok);
    }
}
