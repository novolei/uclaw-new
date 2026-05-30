# 阶段 4 PR14 — GbrainAdapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wrap gbrain (uClaw's page-oriented knowledge-graph MCP server) as a `GbrainAdapter` behind the `MemoryAdapter` trait, reachable via the unified `memory.unified.*` IPC under `backend:"gbrain"`.

**Architecture:** A thin marshalling layer in `memory_adapter/gbrain.rs` that delegates to the existing, tested `gbrain::browse::{put_page, search, get_page, list_pages}` helpers (over the app's `SharedMcpManager`) and converts between the KV-shaped trait surface and gbrain's page model via pure functions. slug = `{namespace}/{key}`. delete/clear are graceful no-ops (gbrain has no delete tool).

**Tech Stack:** Rust, `async-trait`, `anyhow`, `serde_json`, `tracing`, uClaw's `gbrain::browse` + `SharedMcpManager` (`Arc<RwLock<McpManager>>`). No new deps.

---

## Source-of-truth references (verified during planning)

- `memory_adapter/traits.rs` — `MemoryAdapter` trait (8 methods). Signatures (confirm by reading): `fn name(&self) -> &str`; `async fn store(&self, namespace: &str, key: &str, content: &str, category: MemoryCategory, session_id: Option<&str>) -> anyhow::Result<()>`; `async fn recall(&self, query: &str, limit: usize, opts: RecallOpts<'_>) -> anyhow::Result<Vec<MemoryEntry>>`; `async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>>`; `async fn list(&self, namespace: Option<&str>, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>`; `async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool>`; `async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64>`; `async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>>`. **Verify the exact `list` category param type — PR9 used `Option<&MemoryCategory>`.**
- `memory_adapter/types.rs` — `MemoryEntry { id: String, key: String, content: String, namespace: Option<String>, category: MemoryCategory, timestamp: String, session_id: Option<String>, score: Option<f64> }`. `MemoryCategory { Core, Daily, Conversation, Custom(String) }`. `RecallOpts<'a> { namespace: Option<&'a str>, category: Option<MemoryCategory>, session_id: Option<&'a str>, min_score: Option<f64> }`. `NamespaceSummary { namespace: String, count: usize, last_updated: Option<String> }`.
- `memory_adapter/legacy_kv.rs` + `bucket_seal` adapter — reference adapter style (error mapping, test fixture conventions).
- `memory_adapter/mod.rs` — re-exports (line ~22-29); add `pub mod gbrain; pub use gbrain::GbrainAdapter;`.
- `gbrain/browse.rs` — the helpers the adapter wraps:
  - `pub async fn search(mcp: &SharedMcpManager, query: &str, limit: u32, offset: u32) -> Result<Vec<SearchHit>, GbrainError>`
  - `pub async fn get_page(mcp: &SharedMcpManager, slug: &str) -> Result<PageDetail, GbrainError>` (fuzzy:true)
  - `pub async fn list_pages(mcp: &SharedMcpManager, limit: u32, sort: Option<String>, page_type: Option<String>, tag: Option<String>, updated_after: Option<String>) -> Result<Vec<PageSummary>, GbrainError>`
  - `pub async fn put_page(mcp: &SharedMcpManager, slug: &str, content: &str) -> Result<PageDetail, GbrainError>`
  - `pub struct PageSummary { slug: String, title: String, page_type: String, updated_at: Option<String> }`
  - `pub struct PageDetail { slug, title, page_type, compiled_truth: String, frontmatter: serde_json::Value, created_at: Option<String>, updated_at: Option<String>, tags: Vec<String>, raw_markdown: String }`
  - `pub struct SearchHit { slug: String, title: String, snippet: String (serde rename chunk_text), similarity: f64 (serde rename score) }`
  - `pub enum GbrainError` with `pub fn to_command_string(&self) -> String`.
- `mcp.rs` — `pub type SharedMcpManager = Arc<RwLock<McpManager>>` (line 3090). `gbrain::browse::*` resolve the gbrain server internally (a fixed `"gbrain"` server id); the adapter only needs the `SharedMcpManager`.
- `app.rs` — `mcp_manager: SharedMcpManager` AppState field (line ~187, built ~594). Memory-adapter registry construction (`memory_adapters_map.insert(...)` for legacy_kv/legacy_steward/bucket_seal). `gbrain_mcp_id: Arc<Mutex<Option<String>>>` (line ~381) — informational; the adapter does NOT need it (browse:: resolves the server).

---

## CRITICAL design facts

1. **Marshalling layer only** — never reimplement MCP/CLI transport. Delegate to `gbrain::browse::*`. The adapter's testable logic is the pure conversion fns; the MCP round-trip is already covered by `gbrain::browse`'s own tests.
2. **slug = `{namespace}/{key}`**, split on the **first** `/` to recover `(namespace, key)`. A slug with no `/` → `(None, slug)` (e.g. an agent-written page). MemoryAdapter namespaces are flat strings so first-`/` split is unambiguous.
3. **delete/clear are graceful no-ops** — gbrain has no delete tool. `delete → Ok(false)`, `clear_namespace → Ok(0)`, each with a `tracing::warn!`. Never `Err` (a cross-backend clear must not hard-fail on gbrain).
4. **Errors map to `anyhow::Error`** via `GbrainError::to_command_string()`. An absent/disconnected gbrain → `Err` for read/write methods; callers (unified IPC, PR15 router) skip a backend that errors.
5. **Category/session round-trip** — `store` prepends a recoverable marker line to the page content; `get`/`list`/`recall` parse it back, defaulting to `Conversation` when absent. The marker must survive gbrain's `put_page`→`get_page` cycle (see adaptation #3 for which `PageDetail` field to read).
6. **Distinct slug space** — the adapter writes under `{namespace}/` prefixes, separate from the agent's own `put_page` slugs. No collision.

---

## File Structure

| File | New/Mod | Responsibility | LoC |
|---|---|---|---|
| `memory_adapter/gbrain.rs` | new | `GbrainAdapter` + 8 trait methods + pure marshalling fns + tests | ~420 (incl. ~150 tests) |
| `memory_adapter/mod.rs` | mod | `pub mod gbrain; pub use gbrain::GbrainAdapter;` | +2 |
| `app.rs` | mod | register `GbrainAdapter` in `memory_adapters_map` | +5 |

Est. ~280 source + ~150 tests.

---

## Adaptation responsibilities (verify before trusting the plan)

1. **Read `memory_adapter/traits.rs` + a reference impl (`legacy_kv.rs`)** before writing — match the exact 8 signatures (esp. `list`'s `category: Option<&MemoryCategory>` vs owned, and whether methods return `anyhow::Result`).
2. **`MemoryEntry` construction** — all 8 fields (`id, key, content, namespace, category, timestamp, session_id, score`). `timestamp` is an RFC3339 String — use the page's `updated_at` (already a String) directly, or `chrono::Utc::now().to_rfc3339()` when absent. `score` = `Some(hit.similarity)` for recall, `None` otherwise.
3. **Category/session marker round-trip (the one fragile detail)** — `build_page_body` prepends a marker line: `format!("<!-- uclaw-meta: category={}; session={} -->\n\n{}", cat_str, session.unwrap_or(""), content)`. On read, `parse_page_meta` inspects the page text's first line for `<!-- uclaw-meta: ... -->`. **Verify which `PageDetail` field preserves the body verbatim** — try `compiled_truth` first, fall back to `raw_markdown`; if neither preserves the HTML comment (gbrain may strip it), read category from `PageDetail.frontmatter` / `tags` instead, and default to `Conversation`. The PURE `parse_page_meta(text) -> (MemoryCategory, Option<String>, String)` is unit-tested against a constructed string (no gbrain), so the round-trip logic is verified regardless; the integration detail (which field) is resolved by the implementer reading `browse::put_page`/`get_page` behavior.
4. **`gbrain::browse` import path** — `crate::gbrain::browse::{search, get_page, list_pages, put_page, PageDetail, PageSummary, SearchHit, GbrainError}`.
5. **`SharedMcpManager` import** — `crate::mcp::SharedMcpManager`.
6. **`search` arg** — `search(mcp, query, limit as u32, 0)` (offset 0). `list_pages` — pass `limit=200, sort=None, page_type=None, tag=None, updated_after=None` and filter client-side by slug prefix.
7. **namespace filter in recall/list** — after fetching, keep only entries whose recovered namespace matches `opts.namespace` (recall) / the `namespace` arg (list). For `list(None, ...)` (no namespace), return all.
8. **`MemoryCategory` <-> string** — `Core→"core"`, `Daily→"daily"`, `Conversation→"conversation"`, `Custom(s)→"custom:{s}"`; parse inverse. Reuse the convention from the bucket_seal adapter's `build_tags`/`parse_tags` if it exists (check `memory_bucket_seal/adapter.rs`), else define locally.
9. **`min_score` filter** — recall drops hits with `similarity < opts.min_score` when set.
10. **`category` filter in recall/list** — when `opts.category`/`category` arg is set, keep only entries whose recovered category matches.
11. **Pre-commit hooks** — no `--no-verify`. PR14 touches neither `memory_graph::write` nor `dirs::home_dir`.

---

### Task 1: `gbrain.rs` — pure marshalling functions

**Files:**
- Create: `src-tauri/src/memory_adapter/gbrain.rs` (pure fns + tests at this step)

- [ ] **Step 1: Write the failing tests** for the pure fns

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::gbrain::browse::{PageDetail, PageSummary, SearchHit};

    #[test]
    fn slug_round_trips() {
        assert_eq!(slug_for("ns", "key"), "ns/key");
        assert_eq!(split_slug("ns/key"), (Some("ns".to_string()), "key".to_string()));
        // first-slash split: key may itself contain slashes
        assert_eq!(split_slug("ns/a/b"), (Some("ns".to_string()), "a/b".to_string()));
        // no slash → no namespace
        assert_eq!(split_slug("loose"), (None, "loose".to_string()));
    }

    #[test]
    fn build_and_parse_meta_round_trips() {
        let body = build_page_body("hello world", &MemoryCategory::Core, Some("sess1"));
        let (cat, session, content) = parse_page_meta(&body);
        assert!(matches!(cat, MemoryCategory::Core));
        assert_eq!(session.as_deref(), Some("sess1"));
        assert_eq!(content.trim(), "hello world");
    }

    #[test]
    fn parse_meta_defaults_when_absent() {
        let (cat, session, content) = parse_page_meta("just content, no marker");
        assert!(matches!(cat, MemoryCategory::Conversation));
        assert!(session.is_none());
        assert_eq!(content, "just content, no marker");
    }

    #[test]
    fn build_and_parse_custom_category() {
        let body = build_page_body("x", &MemoryCategory::Custom("notes".into()), None);
        let (cat, session, _) = parse_page_meta(&body);
        assert!(matches!(cat, MemoryCategory::Custom(ref s) if s == "notes"));
        assert!(session.is_none());
    }

    #[test]
    fn search_hit_becomes_entry() {
        let hit = SearchHit { slug: "ns/k".into(), title: "T".into(), snippet: "snip".into(), similarity: 0.9 };
        let e = search_hit_to_entry(&hit);
        assert_eq!(e.id, "ns/k");
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        assert_eq!(e.key, "k");
        assert_eq!(e.content, "snip");
        assert_eq!(e.score, Some(0.9));
    }

    #[test]
    fn page_detail_becomes_entry() {
        let p = PageDetail {
            slug: "ns/k".into(), title: "T".into(), page_type: "note".into(),
            compiled_truth: build_page_body("the body", &MemoryCategory::Daily, None),
            frontmatter: serde_json::json!({}), created_at: None,
            updated_at: Some("2026-05-30T00:00:00Z".into()), tags: vec![], raw_markdown: String::new(),
        };
        let e = page_detail_to_entry(&p);
        assert_eq!(e.id, "ns/k");
        assert_eq!(e.namespace.as_deref(), Some("ns"));
        assert_eq!(e.content.trim(), "the body");
        assert!(matches!(e.category, MemoryCategory::Daily));
        assert_eq!(e.timestamp, "2026-05-30T00:00:00Z");
    }

    #[test]
    fn summaries_group_by_first_segment() {
        let pages = vec![
            PageSummary { slug: "a/1".into(), title: "".into(), page_type: "".into(), updated_at: Some("2026-05-30T01:00:00Z".into()) },
            PageSummary { slug: "a/2".into(), title: "".into(), page_type: "".into(), updated_at: Some("2026-05-30T02:00:00Z".into()) },
            PageSummary { slug: "b/1".into(), title: "".into(), page_type: "".into(), updated_at: Some("2026-05-29T00:00:00Z".into()) },
        ];
        let mut s = summaries_from_pages(&pages);
        s.sort_by(|x, y| x.namespace.cmp(&y.namespace));
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].namespace, "a");
        assert_eq!(s[0].count, 2);
        assert_eq!(s[0].last_updated.as_deref(), Some("2026-05-30T02:00:00Z")); // latest in 'a'
        assert_eq!(s[1].namespace, "b");
        assert_eq!(s[1].count, 1);
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib memory_adapter::gbrain 2>&1 | tail`
Expected: compile error (fns not defined).

- [ ] **Step 3: Implement the pure fns + module header**

```rust
// SPDX-License-Identifier: Apache-2.0
//! `GbrainAdapter` — wraps gbrain (page-oriented knowledge-graph MCP server)
//! behind the `MemoryAdapter` trait. Marshalling layer over
//! `crate::gbrain::browse::*`; slug = `{namespace}/{key}`; delete/clear are
//! graceful no-ops (gbrain has no delete tool).

use async_trait::async_trait;
use chrono::Utc;

use crate::gbrain::browse::{self, PageDetail, PageSummary, SearchHit};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::traits::MemoryAdapter;
use crate::memory_adapter::types::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

const META_PREFIX: &str = "<!-- uclaw-meta:";

/// Build a gbrain page slug from a flat namespace + key.
fn slug_for(namespace: &str, key: &str) -> String {
    format!("{namespace}/{key}")
}

/// Recover (namespace, key) from a slug, splitting on the FIRST '/'.
/// No slash → (None, whole slug).
fn split_slug(slug: &str) -> (Option<String>, String) {
    match slug.split_once('/') {
        Some((ns, key)) => (Some(ns.to_string()), key.to_string()),
        None => (None, slug.to_string()),
    }
}

fn category_to_str(c: &MemoryCategory) -> String {
    match c {
        MemoryCategory::Core => "core".to_string(),
        MemoryCategory::Daily => "daily".to_string(),
        MemoryCategory::Conversation => "conversation".to_string(),
        MemoryCategory::Custom(s) => format!("custom:{s}"),
    }
}

fn category_from_str(s: &str) -> MemoryCategory {
    match s {
        "core" => MemoryCategory::Core,
        "daily" => MemoryCategory::Daily,
        "conversation" => MemoryCategory::Conversation,
        other => match other.strip_prefix("custom:") {
            Some(c) => MemoryCategory::Custom(c.to_string()),
            None => MemoryCategory::Conversation,
        },
    }
}

/// Prepend a recoverable meta marker to the page content.
fn build_page_body(content: &str, category: &MemoryCategory, session: Option<&str>) -> String {
    format!(
        "{META_PREFIX} category={}; session={} -->\n\n{}",
        category_to_str(category),
        session.unwrap_or(""),
        content
    )
}

/// Parse the meta marker (if present) off the first line; return
/// (category, session, stripped_content). Defaults to Conversation/None
/// when the marker is absent.
fn parse_page_meta(text: &str) -> (MemoryCategory, Option<String>, String) {
    let mut lines = text.lines();
    if let Some(first) = lines.clone().next() {
        if let Some(rest) = first.trim().strip_prefix(META_PREFIX) {
            // rest looks like ` category=core; session=abc -->`
            let inner = rest.trim_end_matches("-->").trim();
            let mut cat = MemoryCategory::Conversation;
            let mut session: Option<String> = None;
            for kv in inner.split(';') {
                let kv = kv.trim();
                if let Some(v) = kv.strip_prefix("category=") {
                    cat = category_from_str(v.trim());
                } else if let Some(v) = kv.strip_prefix("session=") {
                    let v = v.trim();
                    if !v.is_empty() {
                        session = Some(v.to_string());
                    }
                }
            }
            // Strip the marker line + one following blank line.
            let body: String = {
                let mut it = text.splitn(2, '\n');
                let _marker = it.next();
                it.next().unwrap_or("").trim_start_matches('\n').to_string()
            };
            return (cat, session, body);
        }
    }
    (MemoryCategory::Conversation, None, text.to_string())
}

fn search_hit_to_entry(hit: &SearchHit) -> MemoryEntry {
    let (namespace, key) = split_slug(&hit.slug);
    MemoryEntry {
        id: hit.slug.clone(),
        key,
        content: hit.snippet.clone(),
        namespace,
        category: MemoryCategory::Conversation, // search snippets carry no meta
        timestamp: Utc::now().to_rfc3339(),
        session_id: None,
        score: Some(hit.similarity),
    }
}

fn page_detail_to_entry(p: &PageDetail) -> MemoryEntry {
    let (namespace, key) = split_slug(&p.slug);
    // Prefer the field that preserves the body verbatim (see adaptation #3).
    let source = if !p.compiled_truth.is_empty() { &p.compiled_truth } else { &p.raw_markdown };
    let (category, session_id, content) = parse_page_meta(source);
    MemoryEntry {
        id: p.slug.clone(),
        key,
        content,
        namespace,
        category,
        timestamp: p.updated_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        session_id,
        score: None,
    }
}

fn page_summary_to_entry(s: &PageSummary) -> MemoryEntry {
    let (namespace, key) = split_slug(&s.slug);
    MemoryEntry {
        id: s.slug.clone(),
        key,
        content: s.title.clone(),
        namespace,
        category: MemoryCategory::Conversation,
        timestamp: s.updated_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339()),
        session_id: None,
        score: None,
    }
}

/// One NamespaceSummary per first-segment prefix: count + latest updated_at.
fn summaries_from_pages(pages: &[PageSummary]) -> Vec<NamespaceSummary> {
    use std::collections::BTreeMap;
    let mut acc: BTreeMap<String, (usize, Option<String>)> = BTreeMap::new();
    for p in pages {
        let (ns, _key) = split_slug(&p.slug);
        let Some(ns) = ns else { continue }; // skip namespace-less (agent) pages
        let entry = acc.entry(ns).or_insert((0, None));
        entry.0 += 1;
        // Track the lexicographically-latest RFC3339 updated_at (string sort
        // is correct for RFC3339).
        if let Some(ts) = &p.updated_at {
            if entry.1.as_deref().map(|cur| ts.as_str() > cur).unwrap_or(true) {
                entry.1 = Some(ts.clone());
            }
        }
    }
    acc.into_iter()
        .map(|(namespace, (count, last_updated))| NamespaceSummary { namespace, count, last_updated })
        .collect()
}
```

(8 tests from Step 1 at the bottom.)

**Adaptation:** verify the `PageDetail`/`PageSummary`/`SearchHit` field names match (esp. `snippet`/`similarity` serde renames — the Rust field names are `snippet` and `similarity`). Adjust the test constructors if a field is missing/extra.

- [ ] **Step 4: Run the pure-fn tests**

Run: `cd src-tauri && cargo test --lib memory_adapter::gbrain 2>&1 | tail`
Expected: 8 passed. (The `MemoryAdapter` impl doesn't exist yet — fine; the pure fns + their tests compile standalone. If `unused` warnings fire for the marshalling fns, they'll be consumed by Task 2 — acceptable mid-task, but to keep the build clean you may add `#[allow(dead_code)]` temporarily OR proceed straight to Task 2 before committing. Prefer: implement Task 2 in the same session so nothing is dead.)

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_adapter/gbrain.rs
git commit -m "feat(memory_adapter): gbrain marshalling fns (slug/meta/entry conversions) (PR14.1 of 阶段 4)"
```

---

### Task 2: `GbrainAdapter` struct + `MemoryAdapter` impl

**Files:**
- Modify: `src-tauri/src/memory_adapter/gbrain.rs`
- Modify: `src-tauri/src/memory_adapter/mod.rs`

- [ ] **Step 1: Add the struct + impl** (above the `#[cfg(test)]` block)

```rust
/// gbrain wrapped as a `MemoryAdapter`. Holds a clone of the app's MCP
/// manager; all ops delegate to `crate::gbrain::browse::*`.
pub struct GbrainAdapter {
    mcp: SharedMcpManager,
}

impl GbrainAdapter {
    pub fn new(mcp: SharedMcpManager) -> Self {
        Self { mcp }
    }
}

#[async_trait]
impl MemoryAdapter for GbrainAdapter {
    fn name(&self) -> &str {
        "gbrain"
    }

    async fn store(
        &self,
        namespace: &str,
        key: &str,
        content: &str,
        category: MemoryCategory,
        session_id: Option<&str>,
    ) -> anyhow::Result<()> {
        let slug = slug_for(namespace, key);
        let body = build_page_body(content, &category, session_id);
        browse::put_page(&self.mcp, &slug, &body)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain put_page: {}", e.to_command_string()))?;
        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let hits = browse::search(&self.mcp, query, limit as u32, 0)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain search: {}", e.to_command_string()))?;
        let mut out = Vec::new();
        for hit in &hits {
            let entry = search_hit_to_entry(hit);
            if let Some(ns) = opts.namespace {
                if entry.namespace.as_deref() != Some(ns) {
                    continue;
                }
            }
            if let Some(min) = opts.min_score {
                if entry.score.map(|s| s < min).unwrap_or(false) {
                    continue;
                }
            }
            if let Some(cat) = &opts.category {
                if &entry.category != cat {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn get(&self, namespace: &str, key: &str) -> anyhow::Result<Option<MemoryEntry>> {
        let slug = slug_for(namespace, key);
        match browse::get_page(&self.mcp, &slug).await {
            Ok(page) => Ok(Some(page_detail_to_entry(&page))),
            Err(e) => {
                // page-not-found → None; other errors → propagate.
                let msg = e.to_command_string();
                if msg.contains("page_not_found") || msg.contains("not found") {
                    Ok(None)
                } else {
                    Err(anyhow::anyhow!("gbrain get_page: {}", msg))
                }
            }
        }
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        let pages = browse::list_pages(&self.mcp, 200, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain list_pages: {}", e.to_command_string()))?;
        let mut out = Vec::new();
        for p in &pages {
            let entry = page_summary_to_entry(p);
            if let Some(ns) = namespace {
                if entry.namespace.as_deref() != Some(ns) {
                    continue;
                }
            }
            // category/session come from page meta, not the summary — list
            // returns summaries (no body), so category/session filters here
            // are best-effort: a summary has no meta, so only namespace
            // filtering is reliable. Skip category/session filters for list
            // (documented limitation: list yields summaries without meta).
            let _ = (category, session_id);
            out.push(entry);
        }
        Ok(out)
    }

    async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
        tracing::warn!(
            namespace = %namespace, key = %key,
            "gbrain has no delete tool — delete is a no-op"
        );
        Ok(false)
    }

    async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
        tracing::warn!(
            namespace = %namespace,
            "gbrain has no delete tool — clear_namespace is a no-op"
        );
        Ok(0)
    }

    async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
        let pages = browse::list_pages(&self.mcp, 200, None, None, None, None)
            .await
            .map_err(|e| anyhow::anyhow!("gbrain list_pages: {}", e.to_command_string()))?;
        Ok(summaries_from_pages(&pages))
    }
}
```

**Adaptation:** confirm the `list` signature's category param (`Option<&MemoryCategory>` per PR9). The `list` category/session filtering is documented as best-effort (summaries lack body meta) — if the spec reviewer wants category filtering on `list`, the implementer could `get_page` each summary to recover meta, but that's N round-trips — NOT worth it; namespace-prefix filtering is the reliable contract. Keep the `let _ = (category, session_id);` to silence unused warnings, OR remove the params' usage cleanly.

- [ ] **Step 2: Wire `memory_adapter/mod.rs`**

```rust
pub mod gbrain;
pub use gbrain::GbrainAdapter;
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors (the marshalling fns are now consumed by the impl — no dead-code warnings).

Run: `cd src-tauri && cargo test --lib memory_adapter::gbrain 2>&1 | tail`
Expected: 8 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_adapter/gbrain.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory_adapter): GbrainAdapter MemoryAdapter impl (PR14.2 of 阶段 4)"
```

---

### Task 3: `app.rs` registry registration

**Files:**
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Register the adapter** in the `memory_adapters_map` construction block (where legacy_kv / legacy_steward / bucket_seal are inserted)

```rust
// GbrainAdapter (PR14): wraps the gbrain knowledge-graph MCP server. Always
// registered; methods return Err when gbrain isn't seeded (callers skip it).
let gbrain_adapter = std::sync::Arc::new(
    crate::memory_adapter::GbrainAdapter::new(mcp_manager.clone()),
) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
memory_adapters_map.insert(gbrain_adapter.name().to_string(), gbrain_adapter);
```

**Adaptation:** verify `mcp_manager` is in scope at the registry-construction point (it's built ~line 594, before the adapter map). If the adapter map is built BEFORE `mcp_manager`, move the gbrain registration to just after `mcp_manager` is available, or reorder. Confirm `name()` resolves (the dyn upcast may need the trait in scope — it's already used for the other adapters there).

- [ ] **Step 2: Full build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/app.rs
git commit -m "feat(app): register GbrainAdapter in the memory-adapter registry (PR14.3 of 阶段 4)"
```

---

### Task 4: Verification

- [ ] **Step 1: gbrain adapter tests**

Run: `cd src-tauri && cargo test --lib memory_adapter::gbrain 2>&1 | tail`
Expected: 8 passed.

- [ ] **Step 2: Full memory_adapter module**

Run: `cd src-tauri && cargo test --lib memory_adapter 2>&1 | tail -10`
Expected: existing memory_adapter tests + 8 new pass.

- [ ] **Step 3: Full backend build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 4: Broader regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive; pre-existing failures unchanged.

- [ ] **Step 5: Clippy**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "memory_adapter/gbrain|app\.rs" | head`
Expected: zero PR14-attributable hits.

- [ ] **Step 6: Cargo audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr14-gbrain-adapter && git diff main -- src-tauri/Cargo.toml`
Expected: empty.

- [ ] **Step 7: Registration sanity**

Run: `grep -n "GbrainAdapter" src-tauri/src/app.rs src-tauri/src/memory_adapter/mod.rs`
Expected: registered in app.rs + re-exported in mod.rs.

- [ ] **Step 8: If cleanups surface, apply + commit**

```bash
git add -A
git commit -m "chore(memory_adapter): PR14 cleanup pass"
```

---

## Test plan summary

| Test | Count | Module |
|---|---|---|
| slug round-trip (build/split, multi-slash, no-slash) | 1 | `memory_adapter::gbrain::tests` |
| meta build+parse round-trip + defaults + custom category | 3 | same |
| search_hit_to_entry | 1 | same |
| page_detail_to_entry | 1 | same |
| summaries_from_pages grouping | 1 | same |
| (page_summary_to_entry covered via summaries / add 1 if desired) | 1 | same |
| **Total new** | **~8** | — |

All pure-fn tests — no live gbrain, no MCP server, hermetic.

---

## Self-Review

**1. Spec coverage:**
- §2 mapping table → Task 2 (8 methods) ✅
- §3.1 GbrainAdapter struct → Task 2 ✅
- §3.2 pure marshalling fns → Task 1 ✅ (slug_for, split_slug, search_hit_to_entry, page_detail_to_entry, page_summary_to_entry, summaries_from_pages, build_page_body + parse_page_meta)
- §3.3 availability/error (always register, Err→skippable, delete/clear no-op) → Task 2 (delete/clear) + Task 3 (register) ✅
- §3.4 app wiring → Task 3 ✅
- §3.5 mod re-export → Task 2 ✅
- §4 error handling (page-not-found→None, no-op delete) → Task 2 ✅
- §5 testing (pure-fn, hermetic) → Task 1 ✅
- §6 scope boundaries (no agent put_page change, no delete tool, no router wiring, no default change) → respected (PR14 only adds the adapter + registration) ✅

**2. Placeholder scan:** No TBD/TODO. The category/session round-trip field choice (compiled_truth vs raw_markdown vs frontmatter) is a concrete verify-at-impl instruction (adaptation #3) with a coded default, not a placeholder. The `list` category/session filter is an explicit documented limitation (summaries lack meta), not a gap.

**3. Type consistency:** `slug_for(ns, key) -> String`, `split_slug(slug) -> (Option<String>, String)`, `build_page_body(content, &MemoryCategory, Option<&str>) -> String`, `parse_page_meta(text) -> (MemoryCategory, Option<String>, String)`, `search_hit_to_entry(&SearchHit) -> MemoryEntry`, `page_detail_to_entry(&PageDetail) -> MemoryEntry`, `page_summary_to_entry(&PageSummary) -> MemoryEntry`, `summaries_from_pages(&[PageSummary]) -> Vec<NamespaceSummary>` — consistent between Task 1 definitions, Task 1 tests, and Task 2 call sites. `MemoryEntry` 8-field construction consistent. `browse::{search(_,_,u32,u32), get_page, list_pages(_,u32,..), put_page}` match the verified signatures.

**Documented decisions:**
1. `list` applies only namespace-prefix filtering (summaries carry no body meta; category/session filters would need N `get_page` round-trips — not worth it). Documented limitation.
2. Category/session survive via a `<!-- uclaw-meta: ... -->` marker line; the read-back field (compiled_truth/raw_markdown) is verified at impl, defaulting to `Conversation` if the marker doesn't survive.
3. `get` maps page-not-found → `Ok(None)`; other gbrain errors → `Err`.
