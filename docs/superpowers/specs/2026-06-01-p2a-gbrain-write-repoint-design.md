# P2a-1 — gbrain Write Repoint → Adapter (gated dual-write) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P2** (migrate gbrain knowledge → adapter + retire Bun/PGLite), sub-slice **P2a** (write repoint). Builds on **P1a** (`memory_adapter/pages.rs` facade) and **P2b** (`gbrain_page_migration.rs` — existing pages copied into the adapter). This slice is **P2a-1** (Rust call-site dual-write); **P2a-2** (LLM `mcp__gbrain__put_page` tool-write intercept) is a separate later slice.

## Problem

P2b copied the *existing* gbrain pages into the adapter `"pages"` namespace once. But gbrain remains the live write path, so every **new** page write (memorization, ingestion, the memory put-page IPC, the browser memory policy) lands only in gbrain — the adapter copy drifts stale immediately. Before the read repoint (P2c) can trust the adapter, new writes must also reach it.

The recon found there is **no single write chokepoint**: `gbrain::browse::put_page` is called from 4 distinct Rust subsystems (plus `GbrainAdapter::store` and the LLM MCP tool, both out of scope here), and **none of the 4 callers hold a bucket_seal adapter handle** — they are all mcp-only. So dual-write requires both a shared helper *and* threading the adapter handle into each subsystem.

## Decision (P2a-1 scope)

A shared, **gated, best-effort dual-write helper** wraps the existing `browse::put_page`: gbrain stays the **primary** write (its `Result` is returned unchanged — zero behavior change to the existing path), and — when the gate is on and a handle is available — the same page is **also** shadow-written to the adapter `"pages"` namespace (best-effort: an adapter error logs and is swallowed, never failing the primary). The bucket_seal adapter handle is threaded into the 4 Rust write sites. gbrain remains primary for both read and write throughout P2a; the adapter copy is not *read* until P2c, so eventual consistency during the transition window is acceptable and rollback is a config flip.

Out of scope: `GbrainAdapter::store` (unified memory already defaults to the `bucket_seal` backend, so unified writes already land there; dual-writing in the gbrain adapter would couple one adapter to another) — **excluded pending plan recon: the plan confirms no active caller routes a page write through `GbrainAdapter::store` to the gbrain backend; if one is found, it is folded in as site E rather than silently dropped**; the LLM `mcp__gbrain__put_page` tool write (P2a-2, a dispatch-level intercept, not a Rust call site); P2c read repoint; P2d retirement.

**Ordering constraint:** P2a-1 covers the Rust call sites only; the LLM tool write (the dominant write path) stays gbrain-only until **P2a-2**. So the adapter copy of LLM-authored pages drifts until P2a-2 lands. This is acceptable during the transition **only if P2a-2 lands before P2c (the read repoint)** — otherwise P2c would read an adapter missing LLM-authored pages. **P2a-2 MUST precede P2c.**

## Design

### §1 Core — helper + Page mapping + config gate

New module `src-tauri/src/memory_adapter/page_dual_write.rs`:

```rust
use std::sync::Arc;
use crate::gbrain::browse::{self};
use crate::gbrain::GbrainError;
use crate::mcp::SharedMcpManager;
use crate::memory_adapter::{MemoryAdapter, pages};

/// Pure map: a raw gbrain markdown page (frontmatter + body) → the adapter `Page`.
/// Mirrors P2b's `page_detail_to_page`: `body` is the full raw markdown (the
/// authoritative editable source); title/page_type/tags are parsed from the
/// YAML frontmatter, with slug-fallback for the title.
pub(crate) fn markdown_to_page(slug: &str, markdown: &str) -> pages::Page {
    let (fm, _body) = browse::split_frontmatter(markdown);
    let title = fm.get("title").and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(|s| s.to_string())
        .unwrap_or_else(|| slug.to_string());
    let page_type = fm.get("page_type").and_then(|v| v.as_str())
        .unwrap_or("").to_string();
    let tags = fm.get("tags").and_then(|v| v.as_array())
        .map(|a| a.iter().filter_map(|t| t.as_str().map(str::to_string)).collect())
        .unwrap_or_default();
    pages::Page { slug: slug.to_string(), title, page_type, body: markdown.to_string(), tags }
}

/// The adapter half of the dual-write, extracted so it is unit-testable without
/// the MCP call. Best-effort: an adapter error is logged and swallowed.
pub(crate) async fn shadow_write_page(adapter: &Arc<dyn MemoryAdapter>, slug: &str, markdown: &str) {
    let page = markdown_to_page(slug, markdown);
    if let Err(e) = pages::put_page(adapter, &page).await {
        tracing::warn!(slug, error = %e, "dual-write shadow to adapter pages failed (gbrain primary ok)");
    }
}

/// Write a page to gbrain (PRIMARY — its Result is returned unchanged), and —
/// when `dual_write_enabled` and a handle is present — ALSO shadow-write it to
/// the adapter `"pages"` namespace (best-effort, never fails the primary).
pub async fn dual_write_page(
    mcp: &SharedMcpManager,
    adapter: Option<&Arc<dyn MemoryAdapter>>,
    slug: &str,
    markdown: &str,
    dual_write_enabled: bool,
) -> Result<browse::PageDetail, GbrainError> {
    let res = browse::put_page(mcp, slug, markdown).await;
    if dual_write_enabled {
        if let Some(a) = adapter {
            shadow_write_page(a, slug, markdown).await;
        }
    }
    res
}
```

(`browse::put_page` returns `Result<PageDetail, GbrainError>` — it re-fetches the page after writing — so `dual_write_page` mirrors that return type. The adapter shadow write does not depend on this return shape.)

Declared `pub mod page_dual_write;` in `memory_adapter/mod.rs`.

New in `src-tauri/src/gbrain/browse.rs` — the inverse of the existing `build_raw_markdown`:

```rust
/// Split a raw markdown page into (frontmatter, body). A leading
/// `---\n…\n---\n` block is parsed as YAML → JSON value; if absent or the YAML
/// is malformed, returns (Value::Null, the_full_input) — never panics.
pub fn split_frontmatter(markdown: &str) -> (serde_json::Value, String) {
    // strip a leading "---\n" … "\n---\n" fence; serde_yml::from_str the inner
    // block → serde_json::Value; on any failure fall back to (Null, full input).
}
```

**Config gate** — in `src-tauri/src/memubot_config.rs`, on `MemoryOsConfig`:

```rust
#[serde(default = "default_gbrain_dual_write_pages_enabled")]
pub gbrain_dual_write_pages_enabled: bool,
// ...
fn default_gbrain_dual_write_pages_enabled() -> bool { true }
```

Default `true`: keeps the adapter synced through the transition (low-risk — the adapter copy is not read until P2c); rollback is setting it `false`.

### §2 Handle threading + call-site repoints

The bucket_seal adapter (`state.bucket_seal_adapter: Arc<BucketSealAdapter>`, cast `as Arc<dyn MemoryAdapter>`) threads into the 4 sites. Each site reads `cfg.gbrain_dual_write_pages_enabled` and passes the flag + `Some(adapter)` (or `None` where genuinely unavailable) into `dual_write_page`.

| Site | File(s) | Threading |
|---|---|---|
| **A** IPC put_page | `tauri_commands.rs:1538` | `state.bucket_seal_adapter` already in scope — cast + pass directly. Trivial. |
| **B** memorization | `memorization/service.rs:474,497` (+ `main.rs:248`) | Add `bucket_seal_adapter: Option<Arc<dyn MemoryAdapter>>` field + `set_bucket_seal_adapter` setter (mirrors `set_mcp_manager`); wire at `main.rs:248` beside the existing `mem_svc.set_mcp_manager(...)` call. |
| **C** memory policy | `memory_policy/targets/gbrain.rs:75` (+ `runtime_memory_policy.rs:81`) | `GbrainPolicyTarget::new(mcp)` → `new(mcp, adapter)`; the single caller at `runtime_memory_policy.rs:81` (`.map(GbrainPolicyTarget::new)`) threads the handle in. |
| **D** ingestion | `ingestion/merge.rs:59` (`write_entity`) (+ `ingestion/mod.rs:149` + ingestion entry plumbing) | Add an `adapter` param to `write_entity`; plumb the handle from the ingestion entry point through `ingestion/mod.rs` to the `merge::write_entity` call. Deepest thread. |

The plan recons D's entry-point plumbing (how the ingestion entry obtains/holds the adapter) before writing the tasks.

### Data flow

```
write site (memorization / IPC / memory_policy / ingestion):
  dual_write_page(mcp, Some(adapter), slug, markdown, cfg.flag)
    → browse::put_page(mcp, slug, markdown)   [PRIMARY — gbrain PGLite, Result returned]
    → if flag && adapter: shadow_write_page    [best-effort — adapter "pages" namespace]
         markdown_to_page(slug, markdown) → pages::put_page(adapter)
         (error → warn! + swallow; primary unaffected)
  flag off OR adapter None → pure gbrain write, identical to today
```

## Error handling

gbrain write is primary and its `Result` propagates unchanged at every site — zero behavior change to the existing path. The adapter shadow write is best-effort: a `put_page` error logs a `warn!` and is swallowed (matches P2b's infallible posture). Gate off or `adapter == None` → pure gbrain write.

## Testing

All unit, no live gbrain (the MCP call to gbrain is not exercised — the pure pieces + the adapter half + the gate logic are):

1. **`split_frontmatter`** — md with a `---\n…\n---\n\n` fence → `(Value object, body)`; md with no fence → `(Value::Null, full input)`; malformed YAML → `(Value::Null, full input)` (no panic).
2. **`markdown_to_page`** — frontmatter with `title`/`page_type`/`tags` → all fields populated, `body == markdown`; missing frontmatter → `title == slug`, `page_type == ""`, `tags == []`.
3. **`shadow_write_page`** (the testable seam) — with an in-memory adapter: a page lands in the `"pages"` namespace with correct slug/title/body; `pages::get_page` round-trips it.
4. **`dual_write_page` gate logic** — exercised via `shadow_write_page` directly (the MCP-bound `browse::put_page` is not unit-testable): `enabled=false` path / `adapter=None` path perform no adapter write (assert namespace untouched); `enabled=true, Some(adapter)` writes.
5. `cargo test --lib memory_adapter` green; `cargo build` + clippy clean.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/page_dual_write.rs` | **new** — `dual_write_page` + `shadow_write_page` + `markdown_to_page` + tests |
| `gbrain/browse.rs` | **new** `split_frontmatter` (+ test) |
| `memory_adapter/mod.rs` | `pub mod page_dual_write;` |
| `memubot_config.rs` | `gbrain_dual_write_pages_enabled` (default true) + `default_*` fn |
| `tauri_commands.rs:1538` | site A — cast + pass adapter + flag |
| `memorization/service.rs` (+ `main.rs:248`) | site B — field + setter + wire |
| `memory_policy/targets/gbrain.rs` (+ `runtime_memory_policy.rs:81`) | site C — constructor param + caller |
| `ingestion/merge.rs` (+ `ingestion/mod.rs:149` + entry plumbing) | site D — `write_entity` param + plumbing |

**Out of scope (later P2 sub-slices):** `GbrainAdapter::store` (unified already → bucket_seal; *plan confirms no active page-write caller, else folds in as site E*); **P2a-2** LLM `mcp__gbrain__put_page` tool-write intercept (dispatch-level) — **MUST precede P2c**; **P2c** read repoint (chat recall + query/search + LLM tools → adapter); **P2d** retire gbrain MCP + Bun/PGLite + source + gbrain_prompt system-prompt block.

## Risk

Low-medium. Additive / non-destructive — gbrain stays the primary read+write path; the adapter receives a best-effort shadow copy not read until P2c. The gate defaults on; rollback is flipping `gbrain_dual_write_pages_enabled` to `false`. Blast radius is the 4 threaded handles (all compile-checked); the deepest is D's ingestion entry-point plumbing. One branch, one task per site, bisectable.
