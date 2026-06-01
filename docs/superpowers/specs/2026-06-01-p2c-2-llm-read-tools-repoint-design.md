# P2c-2 — LLM gbrain READ Tools Repoint → Adapter (gated) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P2**, sub-slice **P2c** (read repoint), second slice **P2c-2**. Follows **P2c-1** (passive recall + the `gbrain_read_repoint_enabled` flag + the v2 re-sync) and **P2a-2** (the `McpToolProxy::execute` write-intercept pattern this mirrors). The remaining slices are **P2c-3** (UI/IPC read commands) and **P2d** (retire gbrain).

## Problem

The LLM actively queries the knowledge base through four gbrain MCP read tools — `mcp__gbrain__{get_page, list_pages, search, query}` — which today route through `McpToolProxy::execute` to the gbrain server. P2c-1 repointed the *passive* recall injection; these *active* tool reads are the remaining LLM read path on gbrain. With the adapter `"pages"` namespace kept complete by P2b + the P2c-1 re-sync + P2a-1/P2a-2 dual-write, these reads can be served from the adapter, leaving gbrain as a store to retire in P2d.

## Decision (P2c-2 scope)

Intercept the four read tools at `McpToolProxy::execute` (the P2a-2 chokepoint) and, when armed + the `gbrain_read_repoint_enabled` flag is on, **serve the result from the adapter and return early** (intercept-and-replace, no gbrain fallback — gated rollback + the P2c-1 re-sync cover completeness). Three tools repoint faithfully; `query` repoints with **graceful degradation** (its core hybrid semantic maps to `recall_hybrid`; its graph parameters — `expand`/`salience`/`recency`/`since`/`until`/`source_id` — are dropped, consistent with the ADR retiring gbrain's graph features). The `gbrain_prompt` block is trimmed to drop now-inaccurate graph claims. gbrain remains the primary *store* until P2d; this is a read-only repoint.

Per-tool fidelity:

| Tool | gbrain | Adapter mapping | Fidelity |
|---|---|---|---|
| `get_page(slug)` | `PageDetail` | `pages::get_page` → `Page.body` | exact |
| `list_pages` | `PageSummary[]` | new `pages::list_all` (`adapter.list("pages")`, filter `_migration_marker`) | exact |
| `search(query, limit)` | FTS `SearchHit[]` | `pages::search_pages` (`adapter.recall`, namespace `pages`) | faithful |
| `query(query, …)` | hybrid semantic + graph | `BucketSealAdapter::recall_hybrid(q, Some("pages"), limit)`; graph params ignored | degraded (documented) |

Out of scope: P2c-3 (UI/IPC `gbrain_search`/`gbrain_get_page`); P2d (retire gbrain MCP/Bun/PGLite + delete the gated read/recall code). gbrain read-tool routing code is retained (gated), not deleted.

## Design

### §1 Mechanism + wiring consolidation

`McpToolProxy` (mcp.rs) gains a field:

```rust
    /// P2c-2 — `Some(adapter)` only for the gbrain read tools
    /// (get_page/list_pages/search/query) when `gbrain_read_repoint_enabled` is on.
    /// Concrete `BucketSealAdapter` so `query` can use `recall_hybrid` (not on the
    /// trait). `None` ⇒ the tool hits gbrain as before.
    read_repoint: Option<std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
```

In `execute`, **before** the gbrain JSON-RPC call:

```rust
    if let Some(adapter) = &self.read_repoint {
        if let Some(result) = crate::mcp::gbrain_read_repoint::serve(adapter, &self.tool_name, &params).await {
            let duration_ms = start.elapsed().as_millis() as u64;
            return Ok(match result {
                Ok(text) => crate::agent::tools::tool::ToolOutput::success(&text, duration_ms),
                Err(e) => crate::agent::tools::tool::ToolOutput::error(&format!("{e:#}"), duration_ms),
            });
        }
        // serve returned None (unrecognized tool) → fall through to gbrain (defensive)
    }
```

(`params` is read by reference here, before it is moved into `JsonRpcRequest::call_tool`. P2a-2's dual-write capture also reads `params` before the move; both reads precede it.)

**Wiring consolidation.** P2a-2 added `dual_write_adapter: Option<Arc<dyn MemoryAdapter>>` + `dual_write_enabled: bool` to `create_tool_proxies`. P2c-2 needs `read` + `read_enabled` too. Replace the positional params with one struct:

```rust
pub struct GbrainProxyCfg {
    pub dual_write: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    pub dual_write_enabled: bool,
    pub read: Option<std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
    pub read_enabled: bool,
}
pub fn create_tool_proxies(manager: &SharedMcpManager, locked: &McpManager, gbrain: GbrainProxyCfg) -> Vec<McpToolProxy>
```

In the `.map` literal: `dual_write_pages` arms as today (P2a-2); `read_repoint` arms as:

```rust
    read_repoint: if gbrain.read_enabled
        && tool.server_id == "gbrain"
        && matches!(tool.name.as_str(), "get_page" | "list_pages" | "search" | "query")
    {
        gbrain.read.clone()
    } else { None },
```

`McpToolProxy::for_plugin` sets both `dual_write_pages: None` and `read_repoint: None`.

### §2 Per-tool serve — `mcp/gbrain_read_repoint.rs` (new)

```rust
use std::sync::Arc;
use serde_json::Value;
use crate::memory_bucket_seal::BucketSealAdapter;
use crate::memory_adapter::{pages, MemoryAdapter};

/// Serve a gbrain read tool from the adapter. Returns:
/// - `Some(Ok(text))`  — handled; `text` is the LLM-facing result
/// - `Some(Err(e))`    — handled but the adapter read failed
/// - `None`            — tool name not a recognized read tool (caller falls through to gbrain)
pub async fn serve(adapter: &Arc<BucketSealAdapter>, tool: &str, params: &Value) -> Option<Result<String, anyhow::Error>> {
    let dyn_adapter: Arc<dyn MemoryAdapter> = adapter.clone();
    match tool {
        "get_page" => Some(serve_get_page(&dyn_adapter, params).await),
        "list_pages" => Some(serve_list_pages(&dyn_adapter).await),
        "search" => Some(serve_search(&dyn_adapter, params).await),
        "query" => Some(serve_query(adapter, params).await), // concrete — recall_hybrid
        _ => None,
    }
}
```

- **`serve_get_page`** — `slug = params["slug"].as_str()`; `pages::get_page` → `Some(page)` returns `page.body`; `None` returns `format!("No page found for slug '{slug}'.")` (Ok, not Err). Missing slug arg → `Err`.
- **`serve_list_pages`** — new `pages::list_all(&dyn_adapter)` → lines `"{slug} — {title} ({page_type})"`, joined; empty → `"No pages stored."`.
- **`serve_search`** — `q = params["query"]`, `limit = params["limit"].as_u64().unwrap_or(10)`; `pages::search_pages(&dyn_adapter, q, limit)` → `PageHit` lines `"{slug} — {title}\n  {snippet}"`; empty → `"No matches for '{q}'."`.
- **`serve_query`** — `q`, `limit`; `adapter.recall_hybrid(q, Some("pages"), limit).await` → ranked `MemoryEntry` lines `"{key} (score {score:.2})\n  {snippet}"` (snippet = first ~200 chars of content); empty → `"No matches for '{q}'."`. Graph params present in `params` are ignored.

New helper in `pages.rs`:

```rust
/// List all pages in the "pages" namespace (excluding migration markers).
pub async fn list_all(adapter: &Arc<dyn MemoryAdapter>) -> anyhow::Result<Vec<Page>> {
    let entries = adapter.list(PAGES_NAMESPACE, None).await?; // confirm list() signature
    Ok(entries.into_iter()
        .filter_map(|e| serde_json::from_str::<Page>(&e.content).ok())
        .filter(|p| p.page_type != "_migration_marker")
        .collect())
}
```

(The plan recons the exact `MemoryAdapter::list` signature — args + return — and the `PageHit`/`MemoryEntry` field names used for formatting.)

### §3 gbrain_prompt trim

`src-tauri/src/agent/gbrain_prompt.rs` — the block teaches the four read tools (still valid, adapter-backed) plus `put_page`. Trim the now-inaccurate graph framing: replace "wiki-style entity graph backed by PGlite" / entity-expand language with a neutral "persistent local knowledge base (pages)". Keep the *when-to-call* guidance for each tool. The block still renders only when the gbrain tools are visible.

### Data flow

```
LLM calls mcp__gbrain__{get_page|list_pages|search|query}
  → McpToolProxy::execute (read_repoint = Some(bucket_seal), flag on)
      gbrain_read_repoint::serve(adapter, tool, &params)
        get_page  → pages::get_page → body
        list_pages→ pages::list_all → slug/title/type lines
        search    → pages::search_pages → hit lines
        query     → recall_hybrid(Some("pages")) → ranked lines (graph params dropped)
      → ToolOutput::success(text)   [early return — gbrain NOT called]
  flag off / non-read tool ⇒ read_repoint None ⇒ unchanged gbrain path
```

## Error handling

Intercept-and-replace, no gbrain fallback. Adapter read error → `ToolOutput::error` (the LLM sees the failure rather than a silent gbrain mask). `get_page` miss / empty search/query → a normal "no results" **success** text. Missing required arg (`slug`/`query`) → `Err` → `ToolOutput::error`. `serve` returns `None` only for an unrecognized tool name → defensive fall-through to the gbrain path (won't happen for the four armed tools). Flag off or non-gbrain tool → `read_repoint == None` → unchanged.

## Testing

Unit, against an in-memory `BucketSealAdapter` (the P2b/pages tests already build one — reuse that constructor):

1. **`pages::list_all`** — returns stored pages, excludes `_migration_marker`.
2. **`serve_get_page`** — present slug → body; absent → "No page found"; missing `slug` arg → Err.
3. **`serve_list_pages`** — formats slug/title/type lines; marker excluded; empty → "No pages stored."
4. **`serve_search`** — formats hits; empty → "No matches".
5. **`serve_query`** — `recall_hybrid` ranked formatting; graph params in `params` ignored (no panic); empty → "No matches".
6. `cargo build` + `cargo test --lib mcp` + `--lib memory_adapter` + clippy clean; `create_tool_proxies` callers compile with `GbrainProxyCfg`.

(`McpToolProxy::execute`'s early-serve branch needs a live transport for the fall-through path so isn't unit-tested directly; the `serve` dispatcher + formatters are the testable seam.)

## Scope / files

| File | Change |
|---|---|
| `src-tauri/src/mcp.rs` | `read_repoint` field; `GbrainProxyCfg` struct + `create_tool_proxies` signature refactor (folds in P2a-2's params); execute early-serve branch; `for_plugin` default; 2 test-caller updates; `pub mod gbrain_read_repoint;` |
| `src-tauri/src/mcp/gbrain_read_repoint.rs` | **new** — `serve` + 4 per-tool formatters + tests |
| `src-tauri/src/memory_adapter/pages.rs` | **new** `list_all` + test |
| `src-tauri/src/agent/tools/registry_build.rs` | build `GbrainProxyCfg` (read = `Some(state.bucket_seal_adapter.clone())`, read_enabled = `gbrain_read_repoint_enabled`) |
| `src-tauri/src/tauri_commands.rs` (~15008) | build `GbrainProxyCfg` for the agent-teams registry |
| `src-tauri/src/agent/gbrain_prompt.rs` | trim graph-specific framing |

**Out of scope (later):** **P2c-3** UI/IPC read commands; **P2d** retire gbrain server/Bun/PGLite + delete gated read/recall code + the gbrain_prompt block.

## Risk

Medium. Touches the proxy dispatch chokepoint (shared with P2a-2), four tool semantics, and the system prompt. Gated by `gbrain_read_repoint_enabled` (default on, shared with P2c-1); rollback = flip false (read tools route to gbrain again; the routing code is retained). Intercept-and-replace means a read miss is not masked by gbrain — but P2c-1's re-sync made bucket_seal complete, so misses indicate genuine absence. New coupling: `McpToolProxy` holds the concrete `BucketSealAdapter` (the terminal store — acceptable; the `GbrainProxyCfg` refactor keeps the signature clean). `query` is a deliberate, documented semantic downgrade (graph features retiring per ADR). One branch, bisectable: mechanism + cfg refactor → serve module + `pages::list_all` → wire + prompt trim.
