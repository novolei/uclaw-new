//! P2c-2 — serve the gbrain LLM READ tools from the adapter `"pages"` namespace
//! when the read repoint is on. get_page/list_pages/search are faithful; query
//! degrades to `recall_hybrid` (gbrain graph params dropped per the convergence ADR).

use std::sync::Arc;
use serde_json::Value;

use crate::memory_adapter::{pages, MemoryAdapter};
use crate::memory_bucket_seal::BucketSealAdapter;

const SNIPPET_CHARS: usize = 200;

fn snippet(s: &str) -> String {
    s.chars().take(SNIPPET_CHARS).collect()
}

/// Serve a gbrain read tool from the adapter.
/// - `Some(Ok(text))`  — handled; `text` is the LLM-facing result
/// - `Some(Err(e))`    — handled but the adapter read failed
/// - `None`            — not a recognized read tool (caller falls through to gbrain)
pub async fn serve(
    adapter: &Arc<BucketSealAdapter>,
    tool: &str,
    params: &Value,
) -> Option<Result<String, anyhow::Error>> {
    let dyn_adapter: Arc<dyn MemoryAdapter> = adapter.clone();
    match tool {
        "get_page" => Some(serve_get_page(&dyn_adapter, params).await),
        "list_pages" => Some(serve_list_pages(&dyn_adapter).await),
        "search" => Some(serve_search(&dyn_adapter, params).await),
        "query" => Some(serve_query(adapter, params).await),
        _ => None,
    }
}

async fn serve_get_page(adapter: &Arc<dyn MemoryAdapter>, params: &Value) -> Result<String, anyhow::Error> {
    let slug = params.get("slug").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("get_page: missing 'slug' string argument"))?;
    match pages::get_page(adapter, slug).await? {
        Some(p) => Ok(p.body),
        None => Ok(format!("No page found for slug '{slug}'.")),
    }
}

async fn serve_list_pages(adapter: &Arc<dyn MemoryAdapter>) -> Result<String, anyhow::Error> {
    let all = pages::list_all(adapter).await?;
    if all.is_empty() {
        return Ok("No pages stored.".to_string());
    }
    Ok(all.iter()
        .map(|p| format!("{} — {} ({})", p.slug, p.title, p.page_type))
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn serve_search(adapter: &Arc<dyn MemoryAdapter>, params: &Value) -> Result<String, anyhow::Error> {
    let query = params.get("query").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("search: missing 'query' string argument"))?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let hits = pages::search_pages(adapter, query, limit).await?;
    if hits.is_empty() {
        return Ok(format!("No matches for '{query}'."));
    }
    Ok(hits.iter()
        .map(|h| format!("{} — {}\n  {}", h.slug, h.title, snippet(&h.snippet)))
        .collect::<Vec<_>>()
        .join("\n"))
}

async fn serve_query(adapter: &Arc<BucketSealAdapter>, params: &Value) -> Result<String, anyhow::Error> {
    let query = params.get("query").and_then(|v| v.as_str())
        .ok_or_else(|| anyhow::anyhow!("query: missing 'query' string argument"))?;
    let limit = params.get("limit").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
    let entries = adapter.recall_hybrid(query, Some("pages"), limit).await;
    if entries.is_empty() {
        return Ok(format!("No matches for '{query}'."));
    }
    Ok(entries.iter()
        .map(|e| {
            let score = e.score.map(|s| format!(" (score {s:.2})")).unwrap_or_default();
            format!("{}{}\n  {}", e.key, score, snippet(&e.content))
        })
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use crate::memory_bucket_seal::{
        BucketSealAdapter,
        store::BucketSealStore,
        score::embed::InertEmbedder,
        tree_source::InertSummariser,
    };
    use crate::memory_adapter::pages;

    // Build an in-memory BucketSealAdapter exactly as the adapter.rs tests do.
    fn make_adapter() -> (Arc<BucketSealAdapter>, tempfile::TempDir) {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(InertSummariser::new());
        let adapter = Arc::new(BucketSealAdapter::new(
            store,
            dir.path().join("content"),
            embedder,
            summariser,
        ));
        (adapter, dir)
    }

    async fn seed(a: &Arc<BucketSealAdapter>) {
        let dyn_a: Arc<dyn MemoryAdapter> = a.clone();
        pages::put_page(
            &dyn_a,
            &pages::Page {
                slug: "rust".into(),
                title: "Rust".into(),
                page_type: "note".into(),
                body: "Rust is a systems language".into(),
                tags: vec![],
            },
        )
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn get_page_present_and_absent() {
        let (a, _dir) = make_adapter();
        seed(&a).await;
        let got = serve(&a, "get_page", &serde_json::json!({"slug": "rust"}))
            .await
            .unwrap()
            .unwrap();
        assert!(got.contains("systems language"));
        let miss = serve(&a, "get_page", &serde_json::json!({"slug": "nope"}))
            .await
            .unwrap()
            .unwrap();
        assert!(miss.contains("No page found"));
    }

    #[tokio::test]
    async fn get_page_missing_arg_errs() {
        let (a, _dir) = make_adapter();
        assert!(serve(&a, "get_page", &serde_json::json!({}))
            .await
            .unwrap()
            .is_err());
    }

    #[tokio::test]
    async fn list_pages_lists_and_empty() {
        let (a, _dir) = make_adapter();
        let empty = serve(&a, "list_pages", &serde_json::json!({}))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(empty, "No pages stored.");
        seed(&a).await;
        let listed = serve(&a, "list_pages", &serde_json::json!({}))
            .await
            .unwrap()
            .unwrap();
        assert!(listed.contains("rust — Rust"));
    }

    #[tokio::test]
    async fn search_and_query_run_without_panic() {
        let (a, _dir) = make_adapter();
        seed(&a).await;
        let s = serve(&a, "search", &serde_json::json!({"query": "systems", "limit": 5}))
            .await
            .unwrap();
        assert!(s.is_ok());
        // query ignores graph params without panicking
        let q = serve(
            &a,
            "query",
            &serde_json::json!({"query": "rust", "expand": true, "salience": "high"}),
        )
        .await
        .unwrap();
        assert!(q.is_ok());
    }

    #[tokio::test]
    async fn unknown_tool_is_none() {
        let (a, _dir) = make_adapter();
        assert!(serve(&a, "put_page", &serde_json::json!({})).await.is_none());
    }
}
