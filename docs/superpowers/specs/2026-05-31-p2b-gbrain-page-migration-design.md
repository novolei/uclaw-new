# P2b — gbrain Page Migration → Adapter (non-destructive) Design

**Date:** 2026-05-31
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P2** (migrate gbrain knowledge → adapter + retire Bun/PGLite), first sub-slice **P2b** (data migration). Builds on **P1a** (`memory_adapter/pages.rs` facade). Mirrors the proactive-episode migration (sub-project C / `proactive/memory_migration.rs`).

## Problem

The convergence ADR retires the external gbrain (Bun + PGLite at `~/.uclaw/gbrain`) in favor of the adapter. gbrain holds real user knowledge pages. Before any read/write repointing (P2a/P2c) or retirement (P2d), the existing pages must be copied into the adapter's `"pages"` namespace **non-destructively** (gbrain stays primary + live during P2b), so no knowledge is orphaned.

## Decision (P2b scope)

A one-time, idempotent, **non-destructive** migration: read existing gbrain pages (via `gbrain::browse::list_pages` + `get_page` while gbrain runs) → map `PageDetail` → `pages::Page` → `pages::put_page` into the adapter. gbrain's PGLite is untouched and remains the live read/write path. Idempotency via the namespace-sentinel pattern (the `"pages"` namespace is empty until P2b — nothing writes it yet). Fire-and-forget at startup, infallible, gbrain-unavailable → no-op. No read/write repoint, no retirement (later sub-slices).

## Design

### New module `src-tauri/src/memory_adapter/gbrain_page_migration.rs`

```rust
use std::sync::Arc;
use crate::gbrain::browse::{self, PageDetail};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{MemoryAdapter, pages};

/// Pure map: a gbrain PageDetail → the adapter `Page`. body = raw_markdown
/// (the authoritative source), falling back to compiled_truth when empty.
fn page_detail_to_page(p: &PageDetail) -> pages::Page {
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

/// Completion marker: a sentinel page stored ONLY after a fully-successful pass.
/// Its presence (not "pages namespace non-empty") is the idempotency signal, so a
/// PARTIAL migration does not falsely mark itself done.
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v1__";

/// One-time, idempotent, non-destructive migration of gbrain pages into the
/// adapter "pages" namespace. Returns the count migrated. Best-effort: a missing
/// gbrain (list error) or a per-page error logs + skips; never panics or blocks boot.
/// Idempotency = the completion marker (NOT namespace-non-empty), so a partial
/// migration re-runs on the next boot until it completes (put_page is idempotent →
/// re-copying already-migrated pages is harmless).
pub async fn migrate_gbrain_pages(mcp: &SharedMcpManager, adapter: &Arc<dyn MemoryAdapter>) -> usize {
    // Idempotency: skip only if the completion marker is present (a prior pass
    // fully succeeded). A partial prior run left no marker → re-run.
    if matches!(pages::get_page(adapter, MIGRATION_MARKER_SLUG).await, Ok(Some(_))) {
        tracing::debug!("gbrain page migration: completion marker present; skipping");
        return 0;
    }
    // Read all gbrain pages (paginate to completion — see plan; do NOT silently truncate).
    let summaries = match browse::list_pages(mcp, /* limit/offset per recon */).await {
        Ok(s) => s,
        Err(e) => { tracing::warn!(error=%e, "gbrain page migration: list_pages failed (gbrain absent?); skip"); return 0; }
    };
    let mut migrated = 0usize;
    let mut all_ok = true;
    for summary in summaries {
        let detail = match browse::get_page(mcp, &summary.slug).await {
            Ok(d) => d,
            Err(e) => { tracing::warn!(slug=%summary.slug, error=%e, "gbrain page migration: get_page failed; skip"); all_ok = false; continue; }
        };
        let page = page_detail_to_page(&detail);
        if let Err(e) = pages::put_page(adapter, &page).await {
            tracing::warn!(slug=%page.slug, error=%e, "gbrain page migration: put_page failed; skip");
            all_ok = false;
            continue;
        }
        migrated += 1;
    }
    // Set the completion marker ONLY on a fully-successful pass (no per-page errors).
    // Partial passes leave no marker → the next boot retries the unmigrated pages.
    if all_ok {
        let marker = pages::Page {
            slug: MIGRATION_MARKER_SLUG.to_string(),
            title: "gbrain pages migrated (P2b)".to_string(),
            page_type: "_migration_marker".to_string(),
            body: String::new(),
            tags: vec![],
        };
        let _ = pages::put_page(adapter, &marker).await;
    }
    tracing::info!(migrated, all_ok, "gbrain page migration pass complete");
    migrated
}
```
(The marker page uses a reserved `__…__` slug + `_migration_marker` page_type so downstream page consumers can filter it out; P2c's read path should skip `page_type == "_migration_marker"`.)

Declared `pub mod gbrain_page_migration;` in `memory_adapter/mod.rs`.

### Startup wiring (`app.rs`)

After the `bucket_seal` adapter + `mcp_manager` are built, fire-and-forget (the boot idiom — `tauri::async_runtime::spawn`, as the proactive episode migration + checkpoint prune do):
```rust
{
    let adapter = bucket_seal_adapter.clone() as Arc<dyn MemoryAdapter>;
    let mcp = mcp_manager.clone();
    tauri::async_runtime::spawn(async move {
        let n = crate::memory_adapter::gbrain_page_migration::migrate_gbrain_pages(&mcp, &adapter).await;
        tracing::info!(migrated = n, "P2b: gbrain page migration spawn complete");
    });
}
```
Runs AFTER the gbrain MCP server has had a chance to initialize (the migration's `list_pages` failing → no-op, so ordering is not strict; but place it after `ensure_bundled_gbrain_initialized` is kicked off, or accept the sentinel/retry-on-next-boot semantics — recon the boot order in the plan). Non-blocking; sentinel makes re-runs safe.

## Data flow

```
startup (if completion marker absent) → spawn:
  browse::list_pages(mcp) → [PageSummary]   (paginated to completion)
  per slug: browse::get_page → PageDetail → page_detail_to_page → pages::put_page(adapter)
  → on fully-successful pass: write completion marker
  → adapter "pages" namespace populated (copy of gbrain knowledge); gbrain PGLite untouched
  (partial pass → no marker → next boot retries unmigrated pages; put_page idempotent)
```

## Error handling

Best-effort + boot-safe: pages-list error / gbrain absent → no-op (0); a per-page get/put error → logged + skipped. Never panics, never blocks boot. Re-run safe via the sentinel.

## Testing

1. **Pure mapping** (`page_detail_to_page`): a `PageDetail` with `raw_markdown` → `Page` with all fields + body=raw_markdown; empty `raw_markdown` → body=compiled_truth fallback.
2. **Idempotency (completion marker):** with the marker page pre-stored (`pages::put_page` of `__gbrain_pages_migrated_v1__`), `migrate_gbrain_pages` returns 0 without reading gbrain. A `"pages"` namespace that is non-empty but lacks the marker (a simulated partial prior run) does NOT skip — it re-runs.
3. **Migration loop:** with a mock/stub `browse` (or a gated integration test if a live gbrain is available) returning 2 page summaries + details → the adapter `"pages"` namespace ends with 2 pages with correct slug/title/body; gbrain-list error → 0.
   - Since `browse::list_pages`/`get_page` hit the MCP manager, the loop is best covered by (a) the pure-mapping unit test + (b) the sentinel test with an in-memory adapter; a full live read is a gated integration test (skip if gbrain absent). The plan decides the testable seam (e.g. extract the slug-iteration so it can be fed a `Vec<PageDetail>`).
4. `cargo test --lib memory_adapter` net green; build + clippy clean; `Cargo.toml` unchanged.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/gbrain_page_migration.rs` | **new** — `page_detail_to_page` + `migrate_gbrain_pages` + tests |
| `memory_adapter/mod.rs` | `pub mod gbrain_page_migration;` |
| `app.rs` | startup fire-and-forget migration spawn (gated by the sentinel) |

**Out of scope (later P2 sub-slices):** P2a write repoint (memorization/IPC/LLM-tool writes → adapter); P2c read repoint (chat recall + query/search + LLM tools → adapter); P2d retire gbrain MCP + Bun/PGLite + source + the gbrain_prompt system-prompt block. gbrain stays primary + live through P2b.

## Risk

Low-medium. Non-destructive (gbrain untouched, adapter gets a copy); infallible (gbrain-absent / per-page errors skip, never block boot). **Idempotency = a completion marker set only after a fully-successful pass** — so a partial/failed migration leaves no marker and **re-runs on the next boot until complete** (`put_page` is idempotent by slug → re-copying already-migrated pages is harmless), closing the partial-migration gap a naive "namespace non-empty" sentinel would have. The one remaining consideration is **`list_pages` pagination** — the migration must read ALL pages, not a default-limited first page (the plan recons `list_pages`'s args and paginates to completion, with a `log` if any cap is hit — no silent truncation). gbrain remains the live read/write path throughout, so the migration is fully recoverable. One branch, bisectable.
