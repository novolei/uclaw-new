# P2b — gbrain Page Migration → Adapter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** One-time, idempotent, **non-destructive** migration of existing gbrain knowledge pages into the adapter's `"pages"` namespace (via the P1a facade), so P2's later read/write repoint + retirement have the data in place. gbrain stays primary + live throughout.

**Architecture:** A new `memory_adapter/gbrain_page_migration.rs`: a pure `page_detail_to_page` map + a testable `apply_page_details(adapter, Vec<PageDetail>)` seam + `migrate_gbrain_pages(mcp, adapter)` orchestrator (completion-marker idempotency → `list_pages` high-limit → per-slug `get_page` → `pages::put_page` → marker on full success). Fired at startup (fire-and-forget, gated by the marker, infallible). Mirrors the proactive-episode migration.

**Tech Stack:** Rust, `gbrain::browse`, `memory_adapter::pages` (P1a), `serde_json`. No new deps. Spec: `docs/superpowers/specs/2026-05-31-p2b-gbrain-page-migration-design.md`.

---

## Source-of-truth references (verified)

- `gbrain/browse.rs`: `async fn list_pages(mcp: &SharedMcpManager, limit: u32, sort: Option<String>, page_type: Option<String>, tag: Option<String>, updated_after: Option<String>) -> Result<Vec<PageSummary>, GbrainError>` (234) — **limit-based, no offset/cursor**. `async fn get_page(mcp, slug: &str) -> Result<PageDetail, GbrainError>` (252, `fuzzy: true`). `PageSummary { slug, title, page_type, updated_at }`; `PageDetail { slug, title, page_type, compiled_truth, frontmatter, created_at, updated_at, tags, raw_markdown }`.
- `memory_adapter/pages.rs` (P1a): `Page { slug, title, page_type, body, tags }`; `pub async fn put_page(&Arc<dyn MemoryAdapter>, &Page) -> anyhow::Result<()>`; `pub async fn get_page(&Arc<dyn MemoryAdapter>, slug) -> anyhow::Result<Option<Page>>`. (Use these — the facade, not the gbrain `get_page`.)
- `app.rs`: `bucket_seal_adapter: Arc<BucketSealAdapter>` built at **1086**; `mcp_manager: SharedMcpManager` (`Arc<RwLock<McpManager>>`) at 602; an existing `tauri::async_runtime::spawn` fire-and-forget block at ~649 (template). `GbrainAdapter::new(mcp_manager.clone())` at 1105 (so gbrain reads are wired post-1086).
- `memory_adapter/mod.rs`: `pub mod` declarations (add `gbrain_page_migration`).

NOTE: `pages::get_page` and the gbrain `browse::get_page` share the name `get_page` — qualify both (`pages::get_page` vs `browse::get_page`) to avoid confusion.

---

## CRITICAL facts

1. **Non-destructive** — only READ gbrain (`browse::list_pages`/`get_page`) + WRITE the adapter (`pages::put_page`). Never write/delete gbrain. gbrain stays primary + live.
2. **Completion-marker idempotency** — skip only if the marker page (`__gbrain_pages_migrated_v1__`) is present (set ONLY after a fully-successful pass). A partial pass leaves no marker → re-runs next boot (`put_page` idempotent by slug → harmless re-copy). Do NOT use "namespace non-empty" (a partial migration would falsely mark done).
3. **Read ALL pages** — `list_pages` is limit-based; pass a high limit (`100_000`) and **log a warning if the returned count == the limit** (possible truncation) — no silent cap.
4. **Infallible + boot-safe** — gbrain absent (list error) → no-op (0); per-page get/put error → log + skip + (no marker). Never panics or blocks boot.
5. **Additive** — no read/write repoint, no retirement (P2a/P2c/P2d). gbrain untouched.
6. **Pre-commit hooks** — no `--no-verify`.

---

## File Structure

| File | Change | LoC |
|---|---|---|
| `memory_adapter/gbrain_page_migration.rs` | **new** — `page_detail_to_page` + `apply_page_details` + `migrate_gbrain_pages` + tests | ~90 src + ~110 test |
| `memory_adapter/mod.rs` | `pub mod gbrain_page_migration;` | +1 |
| `app.rs` | startup fire-and-forget migration spawn (after `bucket_seal_adapter`, gated by marker) | ~+10 |

---

## Tasks

### Task 1: the migration module (pure map + testable seam + orchestrator) + tests

**Files:** Create `src-tauri/src/memory_adapter/gbrain_page_migration.rs`; modify `memory_adapter/mod.rs`.

- [ ] **Step 1: Declare the module.** In `memory_adapter/mod.rs`, add `pub mod gbrain_page_migration;`.

- [ ] **Step 2: Write the module + failing tests.** Source:
```rust
//! P2b — one-time, non-destructive migration of gbrain knowledge pages into the
//! adapter "pages" namespace (convergence ADR, Phase P2). Reads gbrain via
//! `browse::*`, writes via the `pages` facade. gbrain is untouched + stays primary.
use std::sync::Arc;

use crate::gbrain::browse::{self, PageDetail};
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{pages, MemoryAdapter};

/// Reserved slug for the completion marker (set ONLY after a fully-successful pass).
const MIGRATION_MARKER_SLUG: &str = "__gbrain_pages_migrated_v1__";
/// High limit to read all gbrain pages (list_pages is limit-based, no cursor).
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

/// Testable seam: write each detail into the adapter via the pages facade.
/// Returns (migrated_count, all_ok). all_ok=false if any put failed.
pub async fn apply_page_details(adapter: &Arc<dyn MemoryAdapter>, details: Vec<PageDetail>) -> (usize, bool) {
    let mut migrated = 0usize;
    let mut all_ok = true;
    for d in &details {
        let page = page_detail_to_page(d);
        match pages::put_page(adapter, &page).await {
            Ok(()) => migrated += 1,
            Err(e) => { tracing::warn!(slug=%page.slug, error=%e, "gbrain page migration: put_page failed; skip"); all_ok = false; }
        }
    }
    (migrated, all_ok)
}

/// One-time idempotent non-destructive migration. Returns migrated count.
pub async fn migrate_gbrain_pages(mcp: &SharedMcpManager, adapter: &Arc<dyn MemoryAdapter>) -> usize {
    // Idempotency: completion marker present ⇒ a prior pass fully succeeded.
    if matches!(pages::get_page(adapter, MIGRATION_MARKER_SLUG).await, Ok(Some(_))) {
        tracing::debug!("gbrain page migration: completion marker present; skipping");
        return 0;
    }
    let summaries = match browse::list_pages(mcp, LIST_ALL_LIMIT, None, None, None, None).await {
        Ok(s) => s,
        Err(e) => { tracing::warn!(error=%e, "gbrain page migration: list_pages failed (gbrain absent?); skip"); return 0; }
    };
    if summaries.len() as u32 == LIST_ALL_LIMIT {
        tracing::warn!(limit = LIST_ALL_LIMIT, "gbrain page migration: list hit the limit — possible truncation; some pages may not migrate this pass");
    }
    // Gather details (gbrain reads).
    let mut details = Vec::with_capacity(summaries.len());
    let mut gets_ok = true;
    for s in &summaries {
        match browse::get_page(mcp, &s.slug).await {
            Ok(d) => details.push(d),
            Err(e) => { tracing::warn!(slug=%s.slug, error=%e, "gbrain page migration: get_page failed; skip"); gets_ok = false; }
        }
    }
    let (migrated, apply_ok) = apply_page_details(adapter, details).await;
    // Marker ONLY on a fully-successful pass (no get errors, no put errors, no truncation).
    let full = gets_ok && apply_ok && (summaries.len() as u32) < LIST_ALL_LIMIT;
    if full {
        let marker = pages::Page {
            slug: MIGRATION_MARKER_SLUG.to_string(),
            title: "gbrain pages migrated (P2b)".to_string(),
            page_type: "_migration_marker".to_string(),
            body: String::new(),
            tags: vec![],
        };
        let _ = pages::put_page(adapter, &marker).await;
    }
    tracing::info!(migrated, full, "gbrain page migration pass complete");
    migrated
}
```
Tests (`#[cfg(test)] mod tests`): copy the in-memory `MemoryAdapter` stub from `memory_adapter/pages.rs`; build `PageDetail` fixtures (match the struct's fields). Then:
```rust
#[test]
fn page_detail_to_page_uses_raw_markdown_then_falls_back() {
    let mut d = detail("s", "T", "raw body");      // helper: PageDetail with raw_markdown="raw body", compiled_truth="compiled"
    assert_eq!(page_detail_to_page(&d).body, "raw body");
    d.raw_markdown = "   ".into();
    assert_eq!(page_detail_to_page(&d).body, "compiled");
}
#[tokio::test]
async fn apply_page_details_writes_pages_and_reports_all_ok() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    let (n, ok) = apply_page_details(&a, vec![detail("a","A","x"), detail("b","B","y")]).await;
    assert_eq!(n, 2); assert!(ok);
    assert_eq!(pages::get_page(&a, "a").await.unwrap().unwrap().title, "A");
}
#[tokio::test]
async fn migrate_skips_when_marker_present() {
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    // pre-store the marker (simulate a prior complete pass)
    pages::put_page(&a, &pages::Page{ slug: "__gbrain_pages_migrated_v1__".into(), title:"".into(), page_type:"_migration_marker".into(), body:"".into(), tags:vec![] }).await.unwrap();
    // No mcp call should happen; migrate returns 0 (use a SharedMcpManager that would error if called, or assert via the marker-short-circuit path — see note).
    // Simplest: assert the marker-present branch via a direct unit on the guard if migrate is hard to call without mcp; otherwise gate.
}
#[tokio::test]
async fn migrate_partial_apply_leaves_no_marker() {
    // apply_page_details with one put forced to fail (e.g. an adapter stub whose store errors on a sentinel slug)
    // → all_ok=false → no marker written → a re-run would proceed. Assert get_page(marker) is None after a partial apply path.
    let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
    // (Construct the partial via apply_page_details + manual marker logic, or a stub that fails one put.)
}
```
(The `migrate_gbrain_pages` orchestrator calls `browse::list_pages`/`get_page` which need a live MCP — so unit-test the **testable seams** directly: `page_detail_to_page` (pure) + `apply_page_details` (in-memory adapter) + the marker guard. For the marker-skip, you can test the guard by pre-storing the marker and calling `migrate_gbrain_pages` with an MCP manager that's empty/offline → it should return 0 via the marker branch BEFORE any list call; if constructing an MCP manager in a test is heavy, instead extract the marker check into a tiny `pub(crate) async fn already_migrated(adapter) -> bool` and unit-test THAT + assert `migrate` calls it first. Pick the cleaner; the non-negotiable: the pure map + apply seam + marker logic are unit-tested without a live gbrain.) Run red→green: `cargo test --lib memory_adapter::gbrain_page_migration 2>&1 | tail`.

- [ ] **Step 3: Commit.**
```bash
git add src-tauri/src/memory_adapter/gbrain_page_migration.rs src-tauri/src/memory_adapter/mod.rs
git commit -m "feat(memory): gbrain page migration module (non-destructive, completion-marker idempotent) — P2b"
```

### Task 2: startup wiring (`app.rs`)

- [ ] **Step 1: RECON** `app.rs` around the `bucket_seal_adapter` build (1086) + the existing spawn block (~649) — confirm `mcp_manager` + `bucket_seal_adapter` are both in scope + cloneable there. The spawn must run AFTER `bucket_seal_adapter` (1086) exists.

- [ ] **Step 2: Wire** (after `bucket_seal_adapter` is built, mirroring the existing fire-and-forget spawn):
```rust
    // P2b — non-destructive one-time migration of gbrain pages into the adapter
    // "pages" namespace. Marker-gated + infallible; gbrain stays primary. Fire-and-forget.
    {
        let adapter = bucket_seal_adapter.clone() as Arc<dyn crate::memory_adapter::MemoryAdapter>;
        let mcp = mcp_manager.clone();
        tauri::async_runtime::spawn(async move {
            let n = crate::memory_adapter::gbrain_page_migration::migrate_gbrain_pages(&mcp, &adapter).await;
            tracing::info!(migrated = n, "P2b: gbrain page migration spawn complete");
        });
    }
```
(If gbrain hasn't finished `ensure_bundled_gbrain_initialized` yet when this runs, `list_pages` errors → no-op → the marker isn't set → it retries on the next boot. That's acceptable; no strict ordering needed. FLAG if you place it differently.)

- [ ] **Step 3: Build + commit.** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`; `git commit -am "feat(app): fire P2b gbrain page migration at startup (marker-gated, non-blocking)"`

### Task 3: Verification

- [ ] `cd src-tauri && cargo test --lib memory_adapter::gbrain_page_migration 2>&1 | tail` (pure map + apply + marker tests pass).
- [ ] `cargo build 2>&1 | grep -E "^error"` (clean).
- [ ] `cargo test --lib memory_adapter 2>&1 | tail -3` (broader green).
- [ ] `cargo clippy --lib -- -D warnings 2>&1 | grep -E "gbrain_page_migration|app\.rs" | head` (clean).
- [ ] `git diff main -- src-tauri/Cargo.toml` (empty).
- [ ] **Non-destructive confirm:** the module only calls `browse::list_pages`/`browse::get_page` (reads) + `pages::put_page` (adapter write) — NO `browse::put_page`/gbrain writes/deletes. `grep -nE "browse::put_page|create_|delete" src-tauri/src/memory_adapter/gbrain_page_migration.rs` → empty.
- [ ] **Idempotency:** marker-present → `migrate` returns 0 (no list); partial pass → no marker (re-runnable).

---

## Self-Review

- ✅ **Spec coverage:** `page_detail_to_page` + `apply_page_details` (testable seam) + `migrate_gbrain_pages` (marker idempotency + read-all + per-page skip) (Task 1); startup spawn (Task 2); verification incl. non-destructive + idempotency (Task 3). P2a/P2c/P2d explicitly out of scope.
- ✅ **Placeholder scan:** full module code; the test notes give a concrete seam (`page_detail_to_page`/`apply_page_details` unit-testable; the orchestrator's gbrain-I/O tested via the marker guard / a gated path) — not vague.
- ✅ **Type consistency:** `page_detail_to_page(&PageDetail) -> pages::Page`; `apply_page_details(&Arc<dyn MemoryAdapter>, Vec<PageDetail>) -> (usize, bool)`; `migrate_gbrain_pages(&SharedMcpManager, &Arc<dyn MemoryAdapter>) -> usize`; `browse::list_pages(mcp, u32, None×4)`; `pages::put_page`/`pages::get_page` per P1a.
- ✅ **Risk-scaled:** non-destructive (gbrain untouched), infallible, marker-gated (partial-safe re-run), read-all-with-truncation-log. The only gbrain mutation is NONE. One branch, bisectable.
- Decisions: completion-marker idempotency (not namespace-non-empty); body=raw_markdown→compiled_truth fallback; high-limit list + truncation warn; testable seam = pure map + apply; non-blocking startup spawn; gbrain stays primary (no repoint/retire).
