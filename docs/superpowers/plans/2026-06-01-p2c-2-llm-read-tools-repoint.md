# P2c-2 LLM gbrain READ Tools Repoint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Serve the four LLM gbrain read tools (`get_page`, `list_pages`, `search`, `query`) from the adapter `"pages"` namespace — intercept-and-replace at `McpToolProxy::execute` when `gbrain_read_repoint_enabled` is on — completing the LLM read repoint.

**Architecture:** A `gbrain_read_repoint::serve` dispatcher maps each read tool to the adapter (`pages::get_page`/`list_all`/`search_pages`, and `recall_hybrid` for `query`), formatting results as LLM-facing text. `McpToolProxy` gains a `read_repoint: Option<Arc<BucketSealAdapter>>` field (armed for the four tools when the flag is on); `execute` serves from it before calling gbrain. The proxy's gbrain params are consolidated into a `GbrainProxyCfg` struct.

**Tech Stack:** Rust, Tauri, the MCP `McpToolProxy`/`create_tool_proxies` machinery, `memory_adapter::pages` facade + `BucketSealAdapter::recall_hybrid`.

---

## Recon findings (complete — ground truth)

- `MemoryAdapter::list(namespace: Option<&str>, category: Option<&MemoryCategory>, session_id: Option<&str>) -> anyhow::Result<Vec<MemoryEntry>>` (`traits.rs:56`).
- `BucketSealAdapter::recall_hybrid(query: &str, namespace: Option<&str>, max_entries: usize) -> Vec<MemoryEntry>` (`memory_bucket_seal/adapter.rs:182`) — returns `Vec` (best-effort, NOT `Result`); concrete method, not on the trait.
- `MemoryEntry { id: String, key: String, content: String, namespace: Option<String>, category, timestamp, session_id, score: Option<f64> }` (`memory_adapter/types.rs:11`).
- `pages::Page { slug, title, page_type, body, tags }`; `pages::PageHit { slug, title, snippet }`; `pages::get_page(&Arc<dyn MemoryAdapter>, slug) -> Result<Option<Page>>`; `pages::search_pages(&Arc<dyn MemoryAdapter>, query, limit) -> Result<Vec<PageHit>>`; `const PAGES_NAMESPACE = "pages"` (`memory_adapter/pages.rs`).
- `McpToolProxy` (mcp.rs:1771) — after P2a-2 has field `dual_write_pages: Option<Arc<dyn MemoryAdapter>>`. Constructor `for_plugin` (~1925) sets it `None`. `create_tool_proxies(manager, locked, dual_write_adapter: Option<Arc<dyn MemoryAdapter>>, dual_write_enabled: bool)` (mcp.rs:2654); literal at ~2682; **callers:** `registry_build.rs:~224`, `tauri_commands.rs:~15008`, two test calls in mcp.rs (`grep -n "create_tool_proxies(" src-tauri/src/mcp.rs`).
- `execute(&self, params: serde_json::Value)` (mcp.rs:1830) — P2a-2 already reads `params` before it's moved into `JsonRpcRequest::call_tool`; the read-repoint serve also reads `&params` there.
- `src/mcp.rs` is a single file; a submodule `src/mcp/gbrain_read_repoint.rs` works via `pub mod gbrain_read_repoint;` in mcp.rs (Rust 2018 file+dir module style — confirm by building).
- `state.bucket_seal_adapter: Arc<BucketSealAdapter>` (concrete). `gbrain_read_repoint_enabled` flag lives on `MemoryOsConfig` (P2c-1); in `registry_build.rs` the P2c-1/P2a-2 config read is at fn scope (`gbrain_dual_write_enabled` tuple ~line 39 — add the read flag to that same read).

## Worktree setup

Worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on `claude/p2c-2-llm-read-tools-repoint` off `origin/main`. Fresh-build placeholders:
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/p2c-2-llm-read-tools-repoint
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
Baseline `cargo build` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/src/memory_adapter/pages.rs` | new `list_all` + test |
| `src-tauri/src/mcp/gbrain_read_repoint.rs` | **new** — `serve` dispatcher + 4 formatters + tests |
| `src-tauri/src/mcp.rs` | `pub mod gbrain_read_repoint;`; `read_repoint` field; `GbrainProxyCfg` + `create_tool_proxies` sig refactor; execute early-serve; `for_plugin` default; test-caller updates |
| `src-tauri/src/agent/tools/registry_build.rs` + `src-tauri/src/tauri_commands.rs` | build `GbrainProxyCfg` |
| `src-tauri/src/agent/gbrain_prompt.rs` | trim graph-specific framing |

---

### Task 1: `pages::list_all`

**Files:** Modify `src-tauri/src/memory_adapter/pages.rs` (helper + test in the existing `#[cfg(test)] mod tests`).

- [ ] **Step 1: Write the failing test**

In the test module (it has an `InMemoryAdapter` test double + `use super::*`):

```rust
#[tokio::test]
async fn list_all_returns_pages_excluding_markers() {
    let a = InMemoryAdapter::new();
    put_page(&a, &Page { slug: "p1".into(), title: "One".into(), page_type: "note".into(), body: "b1".into(), tags: vec![] }).await.unwrap();
    put_page(&a, &Page { slug: "__gbrain_pages_migrated_v2__".into(), title: "m".into(), page_type: "_migration_marker".into(), body: "".into(), tags: vec![] }).await.unwrap();
    let all = list_all(&a).await.unwrap();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].slug, "p1");
}
```

- [ ] **Step 2: Run → fail** — `cd src-tauri && cargo test --lib pages::tests::list_all 2>&1 | tail -8` → `cannot find function list_all`.

- [ ] **Step 3: Implement**

After `search_pages` in `pages.rs`:

```rust
/// List all pages in the `"pages"` namespace, excluding migration markers.
pub async fn list_all(adapter: &Arc<dyn MemoryAdapter>) -> anyhow::Result<Vec<Page>> {
    let entries = adapter.list(Some(PAGES_NAMESPACE), None, None).await?;
    Ok(entries
        .into_iter()
        .filter_map(|e| serde_json::from_str::<Page>(&e.content).ok())
        .filter(|p| p.page_type != "_migration_marker")
        .collect())
}
```

(Confirm `MemoryEntry.content` holds the JSON(Page) — `put_page` stores `content = JSON(Page)`; `get_page` already `serde_json::from_str::<Page>` on it. The `InMemoryAdapter` test double's `list` must return stored entries for `Some("pages")` — verify it honors the namespace filter; if its `list` ignores args and returns all, the test still passes since only pages are stored.)

- [ ] **Step 4: Run → pass** — `cargo test --lib pages::tests::list_all 2>&1 | tail -8` → ok. `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 5: Commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/p2c-2-llm-read-tools-repoint
git add src-tauri/src/memory_adapter/pages.rs
git commit -m "feat(memory_adapter): pages::list_all (excludes migration markers) (P2c-2)"
```

---

### Task 2: `gbrain_read_repoint` serve module

**Files:** Create `src-tauri/src/mcp/gbrain_read_repoint.rs`; modify `src-tauri/src/mcp.rs` (add `pub mod gbrain_read_repoint;` near the other `mod`/top-level items).

- [ ] **Step 1: Create the module**

```rust
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
```

> Confirm field names against recon: `MemoryEntry.key`, `.content`, `.score: Option<f64>`; `PageHit.slug/.title/.snippet`; `Page.slug/.title/.page_type/.body`. If `score` isn't a field, drop the score formatting.

- [ ] **Step 2: Register the module** — in `src-tauri/src/mcp.rs`, add `pub mod gbrain_read_repoint;` (top-level, near other items).

- [ ] **Step 3: Add tests** (append to the new module):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::BucketSealAdapter;
    // reuse the in-memory BucketSealAdapter constructor used by other bucket_seal tests

    fn adapter() -> Arc<BucketSealAdapter> { /* same in-memory ctor as memory_bucket_seal tests */ }

    async fn seed(a: &Arc<BucketSealAdapter>) {
        let dyn_a: Arc<dyn MemoryAdapter> = a.clone();
        pages::put_page(&dyn_a, &pages::Page { slug: "rust".into(), title: "Rust".into(), page_type: "note".into(), body: "Rust is a systems language".into(), tags: vec![] }).await.unwrap();
    }

    #[tokio::test]
    async fn get_page_present_and_absent() {
        let a = adapter(); seed(&a).await;
        let got = serve(&a, "get_page", &serde_json::json!({"slug":"rust"})).await.unwrap().unwrap();
        assert!(got.contains("systems language"));
        let miss = serve(&a, "get_page", &serde_json::json!({"slug":"nope"})).await.unwrap().unwrap();
        assert!(miss.contains("No page found"));
    }

    #[tokio::test]
    async fn get_page_missing_arg_errs() {
        let a = adapter();
        assert!(serve(&a, "get_page", &serde_json::json!({})).await.unwrap().is_err());
    }

    #[tokio::test]
    async fn list_pages_lists_and_empty() {
        let a = adapter();
        let empty = serve(&a, "list_pages", &serde_json::json!({})).await.unwrap().unwrap();
        assert_eq!(empty, "No pages stored.");
        seed(&a).await;
        let listed = serve(&a, "list_pages", &serde_json::json!({})).await.unwrap().unwrap();
        assert!(listed.contains("rust — Rust"));
    }

    #[tokio::test]
    async fn search_and_query_run() {
        let a = adapter(); seed(&a).await;
        let s = serve(&a, "search", &serde_json::json!({"query":"systems","limit":5})).await.unwrap().unwrap();
        assert!(!s.is_empty());
        // query ignores graph params without panicking
        let q = serve(&a, "query", &serde_json::json!({"query":"rust","expand":true,"salience":"high"})).await.unwrap().unwrap();
        assert!(!q.is_empty());
    }

    #[tokio::test]
    async fn unknown_tool_is_none() {
        let a = adapter();
        assert!(serve(&a, "put_page", &serde_json::json!({})).await.is_none());
    }
}
```

> Use the EXACT in-memory `BucketSealAdapter` constructor the `memory_bucket_seal` tests use (Read `adapter.rs` tests, e.g. the `recall_hybrid_*` tests build one — copy that constructor). If `search`/`query` over the in-memory adapter return empty for a substring match, assert on the "No matches" branch instead — the point is no panic + correct dispatch, not FTS tuning.

- [ ] **Step 4: Test + build** — `cargo test --lib gbrain_read_repoint 2>&1 | tail -15` → green; `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/mcp/gbrain_read_repoint.rs src-tauri/src/mcp.rs
git commit -m "feat(mcp): gbrain_read_repoint::serve — adapter-backed read tool dispatcher + formatters (P2c-2)"
```

---

### Task 3: wire the proxy (field + GbrainProxyCfg + execute early-serve)

**Files:** `src-tauri/src/mcp.rs` (field, `GbrainProxyCfg`, `create_tool_proxies` sig + literal, execute branch, `for_plugin`, 2 test callers); `src-tauri/src/agent/tools/registry_build.rs`; `src-tauri/src/tauri_commands.rs:~15008`.

- [ ] **Step 1: Add the `read_repoint` field**

In `McpToolProxy` (after `dual_write_pages`):

```rust
    /// P2c-2 — `Some(adapter)` only for the gbrain read tools when
    /// `gbrain_read_repoint_enabled` is on; concrete `BucketSealAdapter` (query
    /// needs `recall_hybrid`). `None` ⇒ the tool hits gbrain as before.
    read_repoint: Option<std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
```

- [ ] **Step 2: Add `GbrainProxyCfg` + refactor `create_tool_proxies`**

Define (near `create_tool_proxies`):

```rust
/// P2c-2 — gbrain dual-write (P2a-2) + read-repoint (P2c-2) config for proxy construction.
pub struct GbrainProxyCfg {
    pub dual_write: Option<std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    pub dual_write_enabled: bool,
    pub read: Option<std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
    pub read_enabled: bool,
}
```

Change the signature from `(manager, locked, dual_write_adapter, dual_write_enabled)` to `(manager: &SharedMcpManager, locked: &McpManager, gbrain: GbrainProxyCfg)`. In the `.map` literal, replace the P2a-2 `dual_write_pages: if dual_write_enabled && … {dual_write_adapter.clone()} else {None}` with `gbrain.dual_write_enabled` / `gbrain.dual_write.clone()`, and add:

```rust
                    read_repoint: if gbrain.read_enabled
                        && tool.server_id == "gbrain"
                        && matches!(tool.name.as_str(), "get_page" | "list_pages" | "search" | "query")
                    {
                        gbrain.read.clone()
                    } else { None },
```

- [ ] **Step 3: `for_plugin` default** — add `read_repoint: None,` to its `Self { … }` literal.

- [ ] **Step 4: execute early-serve branch**

In `execute`, after `params` is available and before the gbrain JSON-RPC call (alongside P2a-2's `dual` capture):

```rust
        if let Some(read_adapter) = &self.read_repoint {
            if let Some(result) = crate::mcp::gbrain_read_repoint::serve(read_adapter, &self.tool_name, &params).await {
                let duration_ms = start.elapsed().as_millis() as u64;
                return Ok(match result {
                    Ok(text) => crate::agent::tools::tool::ToolOutput::success(&text, duration_ms),
                    Err(e) => crate::agent::tools::tool::ToolOutput::error(&format!("{e:#}"), duration_ms),
                });
            }
        }
```

(Place it after `let start = …` so `start` is in scope. `params` is borrowed here, before P2a-2's `dual` capture / the move into the request — confirm ordering: read-serve returns early, so it must come before the gbrain call; it borrows `&params`, leaving `params` intact for the fall-through path.)

- [ ] **Step 5: Update all 4 callers**

`grep -n "create_tool_proxies(" src-tauri/src/` then:
- **mcp.rs tests (2):** `create_tool_proxies(&shared, &*locked, GbrainProxyCfg { dual_write: None, dual_write_enabled: false, read: None, read_enabled: false })`.
- **registry_build.rs ~224:** read the flag (the fn-scope config read ~line 39 already grabs `gbrain_dual_write_enabled`; extend it to also bind `gbrain_read_repoint_enabled`), then:
  ```rust
  let proxies = crate::mcp::McpManager::create_tool_proxies(
      &state.mcp_manager, &*mgr,
      crate::mcp::GbrainProxyCfg {
          dual_write: Some(std::sync::Arc::clone(&state.bucket_seal_adapter) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>),
          dual_write_enabled: gbrain_dual_write_enabled,
          read: Some(std::sync::Arc::clone(&state.bucket_seal_adapter)),
          read_enabled: gbrain_read_repoint_enabled,
      },
  );
  ```
- **tauri_commands.rs ~15008:** same `GbrainProxyCfg` build (read both flags from `state.memubot_config.read().await.memory_os` into `bool` locals before the `mgr` guard, mirroring the P2a-2 wiring there).

- [ ] **Step 6: Build + test + clippy** — `cargo build 2>&1 | grep -E "^error" | head` empty; `cargo test --lib mcp 2>&1 | grep "test result" | tail -1` pass (the pre-existing `browser::provider_execution` playwright-cli env failure is unrelated); `cargo clippy --lib 2>&1 | grep -E "^error" | head` empty.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/mcp.rs src-tauri/src/agent/tools/registry_build.rs src-tauri/src/tauri_commands.rs
git commit -m "feat(mcp): wire gbrain read-tool repoint into McpToolProxy::execute + GbrainProxyCfg (P2c-2)"
```

---

### Task 4: trim `gbrain_prompt`

**Files:** `src-tauri/src/agent/gbrain_prompt.rs`.

- [ ] **Step 1: Trim graph-specific framing**

Read the block. Replace the opening framing — e.g. "gbrain is a wiki-style entity graph backed by PGlite" — with a neutral line such as: "You have a persistent local knowledge base (pages) that survives across conversations and restarts. Use it PROACTIVELY:". Remove any entity-graph / expand-specific claims. **Keep** the `put_page` guidance and the when-to-call `query`/`search`/`list_pages`/`get_page` bullets (they remain valid). Do not change tool names.

- [ ] **Step 2: Build + any prompt test** — `cargo build 2>&1 | grep -E "^error" | head` empty; if `gbrain_prompt` has render tests, `cargo test --lib gbrain_prompt 2>&1 | tail -8` → adjust any test asserting the removed phrasing.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/agent/gbrain_prompt.rs
git commit -m "docs(agent): trim gbrain_prompt graph-specific framing (reads now adapter-backed) (P2c-2)"
```

---

### Task 5: Whole-slice verification

- [ ] **Step 1:** `cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 2:** `cargo test --lib gbrain_read_repoint 2>&1 | grep "test result"`; `cargo test --lib pages 2>&1 | grep "test result"`; `cargo test --lib mcp 2>&1 | grep "test result" | tail -1` → green (modulo the known browser env failure).
- [ ] **Step 3:** `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] **Step 4:** confirm arming: `grep -n "read_repoint" src-tauri/src/mcp.rs` (field + arming `if` + execute branch + for_plugin None); `grep -rn "create_tool_proxies(" src-tauri/src/` → all callers pass `GbrainProxyCfg`.
- [ ] **Step 5:** `gitnexus_detect_changes()` per CLAUDE.md before the PR.

## Adjacent-edit checklist (PR body)

- **`create_tool_proxies` signature** refactored to `GbrainProxyCfg` (folds in P2a-2's two params) → all 4 callers updated (registry_build, tauri_commands, 2 mcp tests).
- **New `McpToolProxy` field** `read_repoint` → both constructors set it (`create_tool_proxies` literal + `for_plugin`).
- New `src/mcp/gbrain_read_repoint.rs` submodule under the `src/mcp.rs` file module.
- No migration, no new Tauri command, no config change (reuses P2c-1's `gbrain_read_repoint_enabled`).

## PR shape

One branch `claude/p2c-2-llm-read-tools-repoint`, one PR with a `## Commits (bisectable)` table (Tasks 1–4 = 4 commits). Title: `feat(memory): P2c-2 — LLM gbrain read-tools repoint to adapter (gated)`. Body: 4 read tools served from adapter pages when `gbrain_read_repoint_enabled`; get_page/list_pages/search faithful, query degraded (graph params dropped per ADR); intercept-and-replace, rollback via flag; gbrain read code retained (gated); P2c-3 (UI) + P2d (retire) later.

## Self-review notes

- **Spec coverage:** §1 mechanism+cfg → Task 3; §2 serve+list_all → Tasks 1+2; §3 prompt → Task 4, error handling → Task 2/3 (Ok/Err/None + ToolOutput mapping), testing → Tasks 1/2 + Task 5. ✔
- **Type consistency:** `read_repoint: Option<Arc<BucketSealAdapter>>` identical across field/cfg/arming/execute; `serve(&Arc<BucketSealAdapter>, &str, &Value) -> Option<Result<String>>` matches the execute call + tests; `list_all(&Arc<dyn MemoryAdapter>) -> Result<Vec<Page>>` matches `serve_list_pages`. ✔
- **Bisectability:** Task 1 (pages::list_all, used by its test) compiles; Task 2 (serve module, uses list_all, `pub` + tests) compiles; Task 3 (field+cfg+execute uses serve, all callers updated) compiles; Task 4 (prompt) compiles. ✔
- **Follow-the-recon items** (flagged): in-memory `BucketSealAdapter` constructor name (Task 2 tests); `MemoryEntry.score` presence (Task 2 — drop if absent); the `src/mcp/` submodule build (Task 2 Step 2); exact `gbrain_prompt` phrasing (Task 4). Each has concrete guidance.
