# P1a — Page-Knowledge Adapter Layer Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Add a thin, additive page-typed facade (`Page`/`PageHit` + `put_page`/`get_page`/`search_pages`) over the existing `MemoryAdapter` methods, under a `"pages"` namespace — unblocking P2's gbrain migration. No trait change, no live wiring, no gbrain change.

**Architecture:** A new `memory_adapter/pages.rs` module: free functions over `Arc<dyn MemoryAdapter>` that serialize a `Page` into `MemoryEntry.content` (JSON) keyed by slug in the `"pages"` namespace, mapping to the adapter's `store`/`get`/`recall`. Pure capability + unit tests; nothing calls it yet (P2 will).

**Tech Stack:** Rust, `serde_json`, existing `MemoryAdapter` trait. No new deps. Spec: `docs/superpowers/specs/2026-05-31-p1a-page-knowledge-adapter-layer-design.md`.

---

## Source-of-truth references (verified)

- `memory_adapter/traits.rs`: `async fn store(&self, namespace: &str, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>`; `async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>>`; `async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>>`.
- `MemoryEntry { id, key, content, namespace, category, timestamp, session_id, score }`; `RecallOpts<'a> { namespace, category, session_id, min_score }` (derives `Default`); `MemoryCategory::Core` valid.
- A reusable in-memory `MemoryAdapter` test stub already exists at `proactive/task_memory.rs:464` (`InMemoryAdapter` — a HashMap-backed full impl) — use it as the TEMPLATE for the pages test stub (the pages test module needs its own copy; do not cross-import a `#[cfg(test)]` type).
- `memory_adapter/mod.rs` — module declarations + re-exports (mirror the existing `pub mod` + `pub use` style for `pages`).

---

## CRITICAL facts

1. **Facade, not trait** — `pages.rs` holds free `async fn`s over `&Arc<dyn MemoryAdapter>`; do NOT add methods to the `MemoryAdapter` trait.
2. **No live wiring** — nothing in the production path calls these in P1a. P2 repoints gbrain to them. This slice is purely additive (a new module + tests).
3. **Robust deserialization** — a stored `"pages"` entry whose content isn't valid `Page` JSON → `get_page` returns `None`, `search_pages` skips it. Never panic.
4. **Field alignment** — `Page` = the core subset of gbrain's `PageDetail` (slug/title/page_type/body/tags) so P2's `PageDetail`↔`Page` map is mechanical.
5. **Pre-commit hooks** — no `--no-verify`.

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `memory_adapter/pages.rs` | **new** — `Page`/`PageHit` + `put_page`/`get_page`/`search_pages` + in-memory test stub + tests | ~70 src + ~90 test |
| `memory_adapter/mod.rs` | `pub mod pages;` + `pub use pages::{Page, PageHit, put_page, get_page, search_pages};` | +2 |

---

## Tasks

### Task 1: `pages.rs` facade + tests (TDD)

**Files:** Create `src-tauri/src/agent/.../memory_adapter/pages.rs` (path: `src-tauri/src/memory_adapter/pages.rs`); modify `src-tauri/src/memory_adapter/mod.rs`.

- [ ] **Step 1: Declare the module.** In `memory_adapter/mod.rs`, add `pub mod pages;` (next to the other `pub mod`s) + `pub use pages::{Page, PageHit, put_page, get_page, search_pages};`.

- [ ] **Step 2: Write `pages.rs` with the facade + a `#[cfg(test)]` in-memory stub + failing tests.** Source (above the test module):
```rust
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
```
Tests (in `#[cfg(test)] mod tests`): copy the `InMemoryAdapter` stub from `proactive/task_memory.rs:464` (a HashMap<(namespace,key), MemoryEntry>-backed `impl MemoryAdapter` — `store` inserts, `get` looks up, `recall` returns entries in the namespace whose content contains the query substring; `list`/`delete`/`clear_namespace`/`namespace_summaries` minimal). Then:
```rust
fn page(slug: &str, title: &str, body: &str) -> Page {
    Page { slug: slug.into(), title: title.into(), page_type: "note".into(), body: body.into(), tags: vec!["t".into()] }
}
#[tokio::test]
async fn put_then_get_round_trips_all_fields() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    let p = page("intro", "Intro", "hello world");
    put_page(&a, &p).await.unwrap();
    assert_eq!(get_page(&a, "intro").await.unwrap(), Some(p));
}
#[tokio::test]
async fn get_absent_is_none() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    assert_eq!(get_page(&a, "nope").await.unwrap(), None);
}
#[tokio::test]
async fn get_malformed_content_is_none() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    a.store("pages", "bad", "not json", MemoryCategory::Core, None).await.unwrap();
    assert_eq!(get_page(&a, "bad").await.unwrap(), None);
}
#[tokio::test]
async fn search_returns_hits_with_truncated_snippet() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    put_page(&a, &page("a", "Alpha", &"x".repeat(500))).await.unwrap();
    let hits = search_pages(&a, "x", 10).await.unwrap();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].slug, "a");
    assert!(hits[0].snippet.chars().count() <= 200);
}
#[test]
fn page_serde_round_trip_and_defaults() {
    let json = r#"{"slug":"s","title":"T","body":"b"}"#; // no page_type/tags
    let p: Page = serde_json::from_str(json).unwrap();
    assert_eq!(p.page_type, "");
    assert!(p.tags.is_empty());
}
```
(Adjust the `InMemoryAdapter` stub's `recall` to match how the real stub matches queries — substring on `content` is fine for these tests; ensure the namespace filter is honored. If the task_memory stub's `recall` signature/behavior differs, adapt minimally.)

- [ ] **Step 3: Run → red→green.** `cd src-tauri && cargo test --lib memory_adapter::pages 2>&1 | tail`.

- [ ] **Step 4: Commit.**
```bash
git add src-tauri/src/memory_adapter/pages.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory): page-knowledge adapter facade (Page + put/get/search) — convergence P1a"
```

### Task 2: Verification

- [ ] `cd src-tauri && cargo test --lib memory_adapter::pages 2>&1 | tail` (5 tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib memory_adapter 2>&1 | tail -3` (broader memory_adapter green — no regression to existing adapter/router/bucket_seal tests).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter/pages" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Additive-only confirm:** `grep -rn "pages::put_page\|pages::get_page\|pages::search_pages" src-tauri/src | grep -v "memory_adapter/pages.rs\|mod.rs"` → empty (nothing wires it yet; P2's job).

---

## Self-Review

- ✅ **Spec coverage:** `Page`/`PageHit` + `put_page`/`get_page`/`search_pages` facade (Task 1) + verification incl. additive-only confirm (Task 2). gbrain richness / P1b / P1c / P2 wiring explicitly out of scope.
- ✅ **Placeholder scan:** full facade code + full test code; the in-memory stub is a copy-from-named-template instruction with a concrete behavior contract.
- ✅ **Type consistency:** `Page { slug, title, page_type, body, tags }`; `PageHit { slug, title, snippet }`; `put_page(&Arc<dyn MemoryAdapter>, &Page) -> Result<()>`; `get_page(...) -> Result<Option<Page>>`; `search_pages(..., query, limit) -> Result<Vec<PageHit>>`; matches `store`/`get`/`recall` signatures verified in traits.rs.
- ✅ **Risk-scaled:** lowest — pure additive facade, no trait change, no live wiring, no gbrain change; one module + tests. The only judgment (the `Page` field subset) is documented as gbrain-`PageDetail`-aligned for P2.
- Decisions: facade over trait; `"pages"` namespace; body-truncated snippet; robust-deserialize→None/skip; no live wiring (P2).
