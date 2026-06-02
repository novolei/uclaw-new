//! Native memory page tools for the agent loop.
//!
//! Five tools backed by `memory_graph` EntityPage + `bucket_seal`:
//! - `memory_put_page`    — upsert a page (write to EntityPage + shadow bucket_seal)
//! - `memory_get_page`    — retrieve a page by slug
//! - `memory_list_pages`  — list EntityPages in the default space
//! - `memory_search_pages`— FTS5 search over EntityPages
//! - `memory_query`       — hybrid semantic/FTS recall via bucket_seal `"pages"` namespace
//!
//! These are ADDITIVE in Step 2c: the existing `mcp__gbrain__*` proxies remain
//! until a later task removes them. Transient overlap is fine.

use std::sync::Arc;

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::json;
use tracing::{info, warn};

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolEffects, ToolError, ToolOutput};
use crate::memory_adapter::MemoryAdapter as _;
use crate::memory_bucket_seal::BucketSealAdapter;
use crate::memory_graph::entity_page::EntityPageMetadata;
use crate::memory_graph::store::MemoryGraphStore;
use crate::memory_graph::DEFAULT_SPACE_ID;

// ═══════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════

fn default_limit_100() -> usize {
    100
}

fn default_limit_10() -> usize {
    10
}

const SNIPPET_CHARS: usize = 200;

fn snippet(s: &str) -> String {
    s.chars().take(SNIPPET_CHARS).collect()
}

// ═══════════════════════════════════════════════════════════════════════
// 1. memory_put_page
// ═══════════════════════════════════════════════════════════════════════

/// Upsert a long-term knowledge page (EntityPage + bucket_seal shadow).
pub struct MemoryPutPageTool {
    store: Arc<MemoryGraphStore>,
    adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
}

impl MemoryPutPageTool {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
    ) -> Self {
        Self { store, adapter }
    }
}

#[derive(Debug, Deserialize)]
struct PutPageInput {
    slug: String,
    content: String,
}

#[async_trait]
impl Tool for MemoryPutPageTool {
    fn name(&self) -> &str {
        "memory_put_page"
    }

    fn description(&self) -> &str {
        "Save or update a page in long-term memory (persistent across conversations).

WHEN to call this tool:
- User introduces a new entity worth long-term retention (a person, company, project, concept, decision, claim)
- User explicitly asks you to remember (\"记住\", \"remember this\", \"save this\")
- Conversation surfaces a stable fact (e.g. \"GPT-5 released in 2026\")
- A multi-turn investigation reaches a conclusion worth preserving

Slug format: kebab-case English, namespaced when useful. Examples:
- `person/alice-wang`
- `project/uclaw`
- `decision/trigram-fts-2026-05`

Content format: YAML frontmatter (title, type, aliases, tags) + markdown body.
Keep bodies under 500 words; link to sub-pages via `[[other-slug]]` when content grows.

DO NOT use for ephemeral things (this turn's question, jokes, small talk,
general knowledge that doesn't reference the user's specific context).
After calling this tool, mention what you have recorded."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "Kebab-case identifier, namespaced when useful (e.g. person/alice-wang, project/uclaw)"
                },
                "content": {
                    "type": "string",
                    "description": "Full page content: optional YAML frontmatter (--- title: ... ---) followed by a markdown body"
                }
            },
            "required": ["slug", "content"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::write()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: PutPageInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("Failed to parse params: {e}")))?;

        info!(slug = %input.slug, "[memory_put_page] saving page");

        match crate::memory_adapter::page_dual_write::write_page(
            &self.store,
            &self.adapter,
            DEFAULT_SPACE_ID,
            &input.slug,
            &input.content,
        )
        .await
        {
            Ok(()) => {
                let ms = start.elapsed().as_millis() as u64;
                Ok(ToolOutput::success(
                    &format!("Saved page '{}' to long-term memory.", input.slug),
                    ms,
                ))
            }
            Err(e) => {
                warn!(slug = %input.slug, error = %e, "[memory_put_page] failed");
                let ms = start.elapsed().as_millis() as u64;
                Ok(ToolOutput::error(
                    &format!("Failed to save page '{}': {e}", input.slug),
                    ms,
                ))
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 2. memory_get_page
// ═══════════════════════════════════════════════════════════════════════

/// Retrieve a page from long-term memory by its slug.
pub struct MemoryGetPageTool {
    store: Arc<MemoryGraphStore>,
}

impl MemoryGetPageTool {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct GetPageInput {
    slug: String,
}

#[async_trait]
impl Tool for MemoryGetPageTool {
    fn name(&self) -> &str {
        "memory_get_page"
    }

    fn description(&self) -> &str {
        "Retrieve a specific long-term memory page by its slug. Use when you know the exact slug of the page you need."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "slug": {
                    "type": "string",
                    "description": "The exact slug of the page to retrieve (e.g. person/alice-wang)"
                }
            },
            "required": ["slug"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::read()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: GetPageInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("Failed to parse params: {e}")))?;

        info!(slug = %input.slug, "[memory_get_page] fetching page");

        let result = self
            .store
            .find_entity_page_by_slug(DEFAULT_SPACE_ID, &input.slug);
        let ms = start.elapsed().as_millis() as u64;
        match result {
            Ok(Some(detail)) => {
                let content = detail
                    .active_version
                    .as_ref()
                    .map(|v| v.content.clone())
                    .unwrap_or_else(|| {
                        format!("(page '{}' exists but has no active version)", input.slug)
                    });
                Ok(ToolOutput::success(&content, ms))
            }
            Ok(None) => Ok(ToolOutput::success(
                &format!("(no page found for slug '{}')", input.slug),
                ms,
            )),
            Err(e) => {
                warn!(slug = %input.slug, error = %e, "[memory_get_page] error");
                Ok(ToolOutput::error(
                    &format!("Error retrieving page '{}': {e}", input.slug),
                    ms,
                ))
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 3. memory_list_pages
// ═══════════════════════════════════════════════════════════════════════

/// List all pages stored in long-term memory.
pub struct MemoryListPagesTool {
    store: Arc<MemoryGraphStore>,
}

impl MemoryListPagesTool {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct ListPagesInput {
    #[serde(default = "default_limit_100")]
    limit: usize,
}

#[async_trait]
impl Tool for MemoryListPagesTool {
    fn name(&self) -> &str {
        "memory_list_pages"
    }

    fn description(&self) -> &str {
        "List all pages stored in long-term memory. Use when the user asks what is currently stored, or when you need to check whether a page exists before searching."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of pages to return (default 100)",
                    "default": 100
                }
            }
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::read()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: ListPagesInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("Failed to parse params: {e}")))?;

        let limit = input.limit.clamp(1, 500);
        info!(limit, "[memory_list_pages] listing pages");

        let result = self.store.list_entity_pages(DEFAULT_SPACE_ID, None, limit);
        let ms = start.elapsed().as_millis() as u64;
        match result {
            Ok(pages) => {
                if pages.is_empty() {
                    Ok(ToolOutput::success("(no pages)", ms))
                } else {
                    // Mirror gbrain_read_repoint list_pages format: slug — title (page_type)
                    let lines: Vec<String> = pages
                        .iter()
                        .map(|detail| {
                            let meta =
                                EntityPageMetadata::from_optional(&detail.node.metadata);
                            let slug = meta
                                .canonical_slug()
                                .map(String::from)
                                .unwrap_or_else(|| detail.node.id.clone());
                            let subkind = meta.subkind.as_deref();
                            match subkind {
                                Some(k) => format!("{} — {} ({})", slug, detail.node.title, k),
                                None => format!("{} — {}", slug, detail.node.title),
                            }
                        })
                        .collect();
                    Ok(ToolOutput::success(&lines.join("\n"), ms))
                }
            }
            Err(e) => {
                warn!(error = %e, "[memory_list_pages] error");
                Ok(ToolOutput::error(&format!("Error listing pages: {e}"), ms))
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 4. memory_search_pages
// ═══════════════════════════════════════════════════════════════════════

/// Full-text search over long-term memory pages.
pub struct MemorySearchPagesTool {
    store: Arc<MemoryGraphStore>,
}

impl MemorySearchPagesTool {
    pub fn new(store: Arc<MemoryGraphStore>) -> Self {
        Self { store }
    }
}

#[derive(Debug, Deserialize)]
struct SearchPagesInput {
    query: String,
    #[serde(default = "default_limit_10")]
    limit: usize,
}

#[async_trait]
impl Tool for MemorySearchPagesTool {
    fn name(&self) -> &str {
        "memory_search_pages"
    }

    fn description(&self) -> &str {
        "Full-text search over long-term memory pages. Use when looking for pages matching a keyword or phrase. Returns slug, title, and a content snippet for each hit."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Search query text (FTS5, supports keyword and phrase matching)"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 10)",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::read()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: SearchPagesInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("Failed to parse params: {e}")))?;

        let limit = input.limit.clamp(1, 50);
        info!(query = %input.query, limit, "[memory_search_pages] searching");

        let result = self
            .store
            .entity_page_search(DEFAULT_SPACE_ID, &input.query, limit);
        let ms = start.elapsed().as_millis() as u64;
        match result {
            Ok(hits) => {
                if hits.is_empty() {
                    Ok(ToolOutput::success(
                        &format!("No matches for '{}'.", input.query),
                        ms,
                    ))
                } else {
                    // Mirror gbrain_read_repoint search format: slug — title\n  snippet
                    let lines: Vec<String> = hits
                        .iter()
                        .map(|h| {
                            format!("{} — {}\n  {}", h.slug, h.title, snippet(&h.snippet))
                        })
                        .collect();
                    Ok(ToolOutput::success(&lines.join("\n"), ms))
                }
            }
            Err(e) => {
                warn!(query = %input.query, error = %e, "[memory_search_pages] error");
                Ok(ToolOutput::error(
                    &format!("Error searching pages for '{}': {e}", input.query),
                    ms,
                ))
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 5. memory_query
// ═══════════════════════════════════════════════════════════════════════

/// Semantic/hybrid recall from the `"pages"` namespace of bucket_seal.
pub struct MemoryQueryTool {
    adapter: Arc<BucketSealAdapter>,
}

impl MemoryQueryTool {
    pub fn new(adapter: Arc<BucketSealAdapter>) -> Self {
        Self { adapter }
    }
}

#[derive(Debug, Deserialize)]
struct QueryInput {
    query: String,
    #[serde(default = "default_limit_10")]
    limit: usize,
}

#[async_trait]
impl Tool for MemoryQueryTool {
    fn name(&self) -> &str {
        "memory_query"
    }

    fn description(&self) -> &str {
        "Semantic hybrid recall over long-term memory pages (dense + FTS). Use when the user asks 'do you remember', 'what did we say about', or when a new question echoes a prior session topic. Returns ranked hits with relevance scores."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Natural-language query describing what you want to recall"
                },
                "limit": {
                    "type": "integer",
                    "description": "Maximum number of results to return (default 10)",
                    "default": 10
                }
            },
            "required": ["query"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    fn effects(&self) -> ToolEffects {
        ToolEffects::read()
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let input: QueryInput = serde_json::from_value(params)
            .map_err(|e| ToolError::InvalidParams(format!("Failed to parse params: {e}")))?;

        let limit = input.limit.clamp(1, 50);
        info!(query = %input.query, limit, "[memory_query] hybrid recall");

        let entries = self
            .adapter
            .recall_hybrid(&input.query, Some("pages"), limit)
            .await;

        let ms = start.elapsed().as_millis() as u64;
        if entries.is_empty() {
            Ok(ToolOutput::success(
                &format!("No matches for '{}'.", input.query),
                ms,
            ))
        } else {
            // Mirror gbrain_read_repoint query format: key (score)\n  snippet
            let lines: Vec<String> = entries
                .iter()
                .map(|e| {
                    let score = e
                        .score
                        .map(|s| format!(" (score {s:.2})"))
                        .unwrap_or_default();
                    format!("{}{}\n  {}", e.key, score, snippet(&e.content))
                })
                .collect();
            Ok(ToolOutput::success(&lines.join("\n"), ms))
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════
// Tests
// ═══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::*;
    use crate::agent::tools::tool::Tool;
    use crate::memory_adapter::page_dual_write;
    use crate::memory_graph::store::MemoryGraphStore;

    // ── Test store fixture ───────────────────────────────────────────────

    fn build_test_store() -> Arc<MemoryGraphStore> {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1)
            .expect("V35 schema");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    // ── InMemoryAdapter (reused from page_dual_write tests pattern) ──────

    use async_trait::async_trait;
    use std::collections::HashMap;

    struct InMemoryAdapter {
        store: Mutex<HashMap<(String, String), crate::memory_adapter::MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Arc<dyn crate::memory_adapter::MemoryAdapter> {
            Arc::new(Self {
                store: Mutex::new(HashMap::new()),
            })
        }
    }

    #[async_trait]
    impl crate::memory_adapter::MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str {
            "in_memory_test"
        }

        async fn store(
            &self,
            namespace: &str,
            key: &str,
            content: &str,
            category: crate::memory_adapter::MemoryCategory,
            session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = crate::memory_adapter::MemoryEntry {
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
            opts: crate::memory_adapter::RecallOpts<'_>,
        ) -> anyhow::Result<Vec<crate::memory_adapter::MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<crate::memory_adapter::MemoryEntry> = store
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
        ) -> anyhow::Result<Option<crate::memory_adapter::MemoryEntry>> {
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
            _category: Option<&crate::memory_adapter::MemoryCategory>,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<crate::memory_adapter::MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let mut out: Vec<crate::memory_adapter::MemoryEntry> = store
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

        async fn namespace_summaries(
            &self,
        ) -> anyhow::Result<Vec<crate::memory_adapter::NamespaceSummary>> {
            Ok(Vec::new())
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────

    const TEST_SLUG: &str = "person/test-alice";
    const TEST_MD: &str =
        "---\ntitle: Alice Test\npage_type: person\n---\n\nAlice works at Acme Corp.";

    #[tokio::test]
    async fn memory_put_page_creates_entity_page() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();
        let tool = MemoryPutPageTool::new(Arc::clone(&store), adapter);

        let output = tool
            .execute(json!({"slug": TEST_SLUG, "content": TEST_MD}))
            .await
            .expect("execute should not return Err");

        // ToolOutput should be success
        assert_eq!(
            output.result["ok"],
            json!(true),
            "expected ok=true, got: {:?}",
            output.result
        );
        assert!(
            output
                .result
                .get("content")
                .and_then(|v| v.as_str())
                .map(|s| s.contains(TEST_SLUG))
                .unwrap_or(false),
            "success message should mention slug"
        );

        // EntityPage was created in the store
        let ep = store
            .find_entity_page_by_slug(DEFAULT_SPACE_ID, TEST_SLUG)
            .expect("find should not error")
            .expect("EntityPage should exist after put");
        assert_eq!(ep.node.space_id, DEFAULT_SPACE_ID);
        let content = ep
            .active_version
            .as_ref()
            .expect("active_version should exist")
            .content
            .clone();
        assert!(content.contains("Acme"), "content should round-trip");
    }

    #[tokio::test]
    async fn memory_get_page_round_trips_content() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();

        // Seed via write_page primitive (same as MemoryPutPageTool calls)
        page_dual_write::write_page(&store, &adapter, DEFAULT_SPACE_ID, TEST_SLUG, TEST_MD)
            .await
            .expect("write_page should succeed");

        let get_tool = MemoryGetPageTool::new(Arc::clone(&store));
        let output = get_tool
            .execute(json!({"slug": TEST_SLUG}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        assert!(body.contains("Acme"), "returned content should contain body text");
    }

    #[tokio::test]
    async fn memory_get_page_returns_not_found_message() {
        let store = build_test_store();
        let tool = MemoryGetPageTool::new(store);

        let output = tool
            .execute(json!({"slug": "nonexistent/slug"}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        assert!(
            body.contains("no page found"),
            "should indicate page not found, got: {body}"
        );
    }

    #[tokio::test]
    async fn memory_list_pages_shows_seeded_page() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();

        page_dual_write::write_page(&store, &adapter, DEFAULT_SPACE_ID, TEST_SLUG, TEST_MD)
            .await
            .expect("seed should succeed");

        let list_tool = MemoryListPagesTool::new(Arc::clone(&store));
        let output = list_tool
            .execute(json!({}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        // Should contain "person/test-alice" in the listing
        assert!(
            body.contains("person/test-alice") || body.contains("Alice Test"),
            "listing should include the seeded page, got: {body}"
        );
    }

    #[tokio::test]
    async fn memory_list_pages_returns_no_pages_message_when_empty() {
        let store = build_test_store();
        let tool = MemoryListPagesTool::new(store);

        let output = tool
            .execute(json!({}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        assert_eq!(body, "(no pages)");
    }

    #[tokio::test]
    async fn memory_search_pages_finds_seeded_page() {
        let store = build_test_store();
        let adapter = InMemoryAdapter::new();

        page_dual_write::write_page(&store, &adapter, DEFAULT_SPACE_ID, TEST_SLUG, TEST_MD)
            .await
            .expect("seed should succeed");

        let search_tool = MemorySearchPagesTool::new(Arc::clone(&store));
        let output = search_tool
            .execute(json!({"query": "Acme", "limit": 5}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        // Should find the page (FTS5 search)
        assert!(
            body.contains("Alice") || body.contains("Acme") || body.contains("person/test-alice"),
            "search should find the seeded page, got: {body}"
        );
    }

    #[tokio::test]
    async fn memory_search_pages_returns_no_matches_when_empty() {
        let store = build_test_store();
        let tool = MemorySearchPagesTool::new(store);

        let output = tool
            .execute(json!({"query": "nonexistent term"}))
            .await
            .expect("execute should not return Err");

        assert_eq!(output.result["ok"], json!(true));
        let body = output.result["content"]
            .as_str()
            .expect("content should be a string");
        assert!(
            body.contains("No matches"),
            "should say no matches, got: {body}"
        );
    }

    // Note: memory_query (MemoryQueryTool) is backed by the CONCRETE
    // BucketSealAdapter which requires a full tempdir + embedder + summariser
    // fixture. That tool's recall_hybrid path is covered by the existing
    // gbrain_read_repoint::serve_query tests and the memu_tools integration
    // tests (both use BucketSealAdapter in-process). Duplicating that heavy
    // fixture here adds no new coverage, so memory_query is skipped in this
    // unit suite. End-to-end coverage comes from manual soak testing.
}
