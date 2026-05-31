# P1a — Page-Knowledge Adapter Layer (thin facade) Design

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P1** (MemoryAdapter capability growth), first slice **P1a** (page-knowledge). Unblocks **P2** (migrate gbrain knowledge → adapter).

## Problem

The convergence ADR makes openhuman/bucket_seal the terminal primary store; **P2** will migrate gbrain's page-knowledge (`put_page`/`query`/`search`) off the external Bun/PGLite gbrain onto the adapter. But the `MemoryAdapter` (store/recall/get/list/delete/clear/namespace_summaries) + bucket_seal (a chunk-consolidation tree) have **no page abstraction**. gbrain pages are `(slug, title, page_type, body, tags, …)` with auto-link/compiled_truth/frontmatter richness.

## Decision (P1a scope)

Build a **thin, additive page-typed facade** over the existing `MemoryAdapter` methods — NOT a rebuild of gbrain's auto-link/wiki pipeline (that richness is re-derived-or-dropped in P2). A `Page` model covering gbrain's core fields, with `put_page`/`get_page`/`search_pages` mapping to `store`/`get`/`recall` under a `"pages"` namespace. **No trait change, no live wiring, no gbrain change** (P2 repoints gbrain to this facade later). Pure new capability + unit tests. Lowest-risk first slice; unblocks P2.

## Design

### New module `src-tauri/src/memory_adapter/pages.rs`

```rust
use std::sync::Arc;
use crate::memory_adapter::{MemoryAdapter, MemoryCategory, RecallOpts};

const PAGES_NAMESPACE: &str = "pages";

/// A knowledge page — the core subset of gbrain's PageDetail that the thin
/// adapter layer persists (auto-link/compiled_truth/frontmatter are NOT modeled
/// here; that gbrain-pipeline richness is re-derived or dropped in P2).
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

/// A page search hit (mirrors gbrain's SearchHit shape so P2 can repoint cleanly).
#[derive(Debug, Clone, PartialEq)]
pub struct PageHit {
    pub slug: String,
    pub title: String,
    pub snippet: String,
}

/// Store/overwrite a page. Content = JSON(Page); key = slug; namespace = "pages".
pub async fn put_page(adapter: &Arc<dyn MemoryAdapter>, page: &Page) -> anyhow::Result<()> {
    let content = serde_json::to_string(page)?;
    adapter.store(PAGES_NAMESPACE, &page.slug, &content, MemoryCategory::Core, None).await
}

/// Fetch a page by slug. None if absent or unparseable.
pub async fn get_page(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<Option<Page>> {
    match adapter.get(PAGES_NAMESPACE, slug).await? {
        Some(entry) => Ok(serde_json::from_str::<Page>(&entry.content).ok()),
        None => Ok(None),
    }
}

/// Search pages by query; returns hits (snippet = body truncated). Unparseable entries skipped.
pub async fn search_pages(adapter: &Arc<dyn MemoryAdapter>, query: &str, limit: usize) -> anyhow::Result<Vec<PageHit>> {
    let opts = RecallOpts { namespace: Some(PAGES_NAMESPACE), ..Default::default() };
    let entries = adapter.recall(query, limit, opts).await?;
    Ok(entries.into_iter().filter_map(|e| {
        let page: Page = serde_json::from_str(&e.content).ok()?;
        let snippet: String = page.body.chars().take(200).collect();
        Some(PageHit { slug: page.slug, title: page.title, snippet })
    }).collect())
}
```

Declared `pub mod pages;` in `memory_adapter/mod.rs`; `Page`/`PageHit`/the fns re-exported as convenient.

- **Facade, not trait**: works over any `Arc<dyn MemoryAdapter>` via the existing methods → no `MemoryAdapter` trait change, no per-backend impl; defaults to whatever adapter the caller passes (bucket_seal in production). Keeps the trait atomic.
- **Field alignment**: `Page` = the core subset of gbrain's `PageDetail` (slug/title/page_type/body←raw_markdown/tags). P2 maps gbrain `PageDetail`↔`Page` and repoints `put_page`/`query`/`search` to this facade.
- **`get_page` exact-key**; `search_pages` content-search via `recall` (FTS on bucket_seal). `get`/`recall` are the existing adapter methods.

## Data flow

```
put_page(adapter, Page{slug,title,page_type,body,tags})
  → adapter.store("pages", slug, JSON(Page), Core, None)
get_page(adapter, slug) → adapter.get("pages", slug) → deserialize → Some(Page)/None
search_pages(adapter, q, n) → adapter.recall(q, n, ns="pages") → deserialize → Vec<PageHit>
```

(Not invoked by any live path in P1a. P2 wires gbrain's read/write to it.)

## Error handling

`put_page`/`get_page`/`search_pages` propagate the adapter's `anyhow::Result`. Deserialization of a malformed stored page → treated as absent (`get_page` → None) / skipped (`search_pages`), never panics.

## Testing

1. `put_page` then `get_page` round-trips all `Page` fields (slug/title/page_type/body/tags) — via an in-memory adapter test stub (reuse the `InMemoryAdapter`/stub used by router/task_memory tests).
2. `get_page` on an absent slug → `None`.
3. `get_page` on a stored entry with non-`Page` JSON content → `None` (robust).
4. `search_pages` over several stored pages → `PageHit`s with truncated snippet; query matching by content.
5. `Page` serde round-trip (serialize → deserialize equal); `#[serde(default)]` lets older content without `page_type`/`tags` parse.
6. `cargo test --lib memory_adapter::pages` + build clean + clippy clean; `Cargo.toml` unchanged.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/pages.rs` | **new** — `Page`/`PageHit` + `put_page`/`get_page`/`search_pages` facade + tests |
| `memory_adapter/mod.rs` | `pub mod pages;` + re-exports |

**Out of scope (later phases/slices):** gbrain auto-link / compiled_truth / frontmatter / wiki-synth (gbrain-pipeline richness); **P1b** graph edges (tool_memory); **P1c** versioning + keyword-index + ranking (skill_parser); **P2** the gbrain data migration + repointing + retiring Bun/PGLite. No live wiring in P1a.

## Risk

Low. Pure additive facade over existing, tested adapter methods; no trait change, no live-path wiring, no gbrain change. One branch, bisectable. The only judgment is the `Page` field subset — chosen to match gbrain's `PageDetail` core so P2's repoint is mechanical.
