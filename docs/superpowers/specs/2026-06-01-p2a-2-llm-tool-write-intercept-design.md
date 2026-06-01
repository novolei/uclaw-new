# P2a-2 — LLM `mcp__gbrain__put_page` Write Intercept → Adapter (gated dual-write) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory-store convergence (ADR `2026-05-31-memory-store-convergence-openhuman-primary.md`), Phase **P2** (migrate gbrain knowledge → adapter + retire Bun/PGLite), sub-slice **P2a** (write repoint). This is **P2a-2** — the dispatch-level intercept of the LLM's `mcp__gbrain__put_page` tool write. Builds on **P2a-1** (`memory_adapter::page_dual_write::shadow_write_page`, the config flag `gbrain_dual_write_pages_enabled`) and **P2b** (existing pages copied into the adapter).

## Problem

P2a-1 dual-writes every **Rust** call site that writes a gbrain page (memorization, ingestion, the put-page IPC, the memory-policy target). But the **dominant** write path is the LLM itself calling the `mcp__gbrain__put_page` tool — the agent actively saving knowledge. That write is **not** a Rust call site: it is dispatched through the MCP tool proxy to the gbrain server, so P2a-1 does not cover it, and the adapter copy drifts stale for all LLM-authored pages.

This is the **last write-side piece**. The convergence ordering constraint (from the P2a-1 spec) is firm: **P2a-2 MUST land before P2c** (the read repoint), otherwise P2c would read an adapter missing LLM-authored pages.

## Decision (P2a-2 scope)

Intercept at the single dispatch chokepoint — `McpToolProxy::execute` (`src-tauri/src/mcp.rs`). The LLM's `mcp__gbrain__put_page` call routes to the `McpToolProxy` whose `(server_id, tool_name) == ("gbrain", "put_page")`. After that proxy's call to gbrain **succeeds**, best-effort shadow-write the same page into the adapter `"pages"` namespace, reusing P2a-1's `shadow_write_page`. gbrain stays the **primary** read+write path (its `execute` result is returned unchanged). Gated by the existing `MemoryOsConfig::gbrain_dual_write_pages_enabled` (default on). Only the gbrain put_page proxy is armed; every other MCP tool's path is byte-identical.

Out of scope: P2c read repoint; P2d retirement; any other MCP tool's dual-write (none needed).

## Design

### §1 Interception in `McpToolProxy::execute`

`McpToolProxy` (mcp.rs:1771) already holds `server_id`, `tool_name`, `manager`, `auto_approve`, etc. Add one field:

```rust
    /// P2a-2 — `Some(adapter)` ONLY for the (gbrain, put_page) proxy when the
    /// dual-write flag is on; `None` for every other proxy. Folds the gate +
    /// handle into one field: `None` ⇒ no dual-write. Snapshotted at proxy
    /// construction (proxies rebuild each turn → fresh flag), like `auto_approve`.
    dual_write_pages: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
```

A pure, unit-testable helper (free fn in mcp.rs):

```rust
/// Extract (slug, content) from a gbrain `put_page` arguments object.
/// Returns None if either field is absent or not a string.
fn parse_put_page_args(params: &serde_json::Value) -> Option<(String, String)> {
    let slug = params.get("slug")?.as_str()?.to_string();
    let content = params.get("content")?.as_str()?.to_string();
    Some((slug, content))
}
```

`execute(&self, params: serde_json::Value)` moves `params` into `JsonRpcRequest::call_tool(req_id, &self.tool_name, params)`, so the args must be captured **before** that move:

```rust
    // P2a-2 — capture dual-write inputs before `params` is moved into the request.
    let dual = self
        .dual_write_pages
        .as_ref()
        .and_then(|a| parse_put_page_args(&params).map(|(s, c)| (a.clone(), s, c)));
```

The existing result match ends in the genuine-success arm `Ok(call_result)` with `!call_result.is_error` (the `ToolOutput::success(&text, …)` branch). Immediately before returning that success, shadow-write:

```rust
    if let Some((adapter, slug, content)) = dual {
        crate::memory_adapter::page_dual_write::shadow_write_page(&adapter, &slug, &content).await;
    }
```

`shadow_write_page` (P2a-1, `pub(crate)`, reachable from mcp.rs — same crate) maps the markdown → `pages::Page` (body = raw markdown, title/page_type/tags from frontmatter) and best-effort `pages::put_page` (logs + swallows errors). The dual-write fires **only** in the success branch — never on `call_result.is_error` (gbrain rejected) or on transport/protocol errors — so the adapter never receives a page gbrain itself refused.

### §2 Wiring (`create_tool_proxies` + `registry_build`)

`McpManager::create_tool_proxies` (mcp.rs:2654) gains two params:

```rust
    pub fn create_tool_proxies(
        manager: &SharedMcpManager,
        locked: &McpManager,
        dual_write_adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>>,
        dual_write_enabled: bool,
    ) -> Vec<McpToolProxy> {
```

In its `.map(|tool| McpToolProxy { … })`, arm only the gbrain put_page proxy:

```rust
        dual_write_pages: if dual_write_enabled
            && tool.server_id == "gbrain"
            && tool.name == "put_page"
        {
            dual_write_adapter.clone()
        } else {
            None
        },
```

The caller `registry_build.rs` (~line 222, already in `AppState` scope) reads the flag + passes the handle:

```rust
        let dual_enabled = state
            .memubot_config
            .read()
            .await
            .memory_os
            .gbrain_dual_write_pages_enabled;
        let dual_adapter: Option<Arc<dyn crate::memory_adapter::MemoryAdapter>> =
            Some(state.bucket_seal_adapter.clone());
        let proxies = crate::mcp::McpManager::create_tool_proxies(
            &state.mcp_manager,
            &*mgr,
            dual_adapter,
            dual_enabled,
        );
```

(`state.bucket_seal_adapter.clone()` assigned to the explicitly-typed `Option<Arc<dyn MemoryAdapter>>` triggers the Arc unsized coercion — `Arc::clone(&x)` does not; this matches the P2a-1 finding. Confirm the runtime config-read idiom against the existing `unified_load_context_enabled` read, same as P2a-1 site A.)

The other `McpToolProxy` constructor — `McpToolProxy::new` (~mcp.rs:1917, plugin-declared tools) — sets `dual_write_pages: None` (plugin tools are never gbrain).

### Data flow

```
LLM calls mcp__gbrain__put_page {slug, content}
  → McpToolProxy::execute (server_id=gbrain, tool_name=put_page, dual_write_pages=Some(adapter))
      capture (adapter, slug, content) from params  [before params is moved]
      → JsonRpcRequest::call_tool → gbrain server     [PRIMARY — result returned unchanged]
      → on Ok(call_result) && !is_error:
            shadow_write_page(adapter, slug, content)  [best-effort adapter "pages"; errors swallowed]
      → return ToolOutput::success(text)               [unchanged]
  flag off ⇒ dual_write_pages None ⇒ no dual-write; any other tool ⇒ None ⇒ byte-identical path
```

## Error handling

gbrain is primary: `execute`'s `Result`/`ToolOutput` is returned unchanged at every path. The adapter shadow write is best-effort (P2a-1: logs `warn!` + swallows). It fires only on genuine gbrain success; on `call_result.is_error` or transport/protocol error it is skipped. If the LLM omitted `slug`/`content` or sent non-strings, `parse_put_page_args` → `None` → no-op (no panic). Flag off or non-gbrain tool → `dual_write_pages == None` → no dual-write.

## Testing

Unit (no live MCP transport — `execute` itself is not unit-testable without a server; the pure pieces + P2a-1's already-tested `shadow_write_page` cover the logic):

1. **`parse_put_page_args`** — valid `{slug, content}` → `Some((slug, content))`; missing `slug` → `None`; missing `content` → `None`; `slug`/`content` non-string (number/object) → `None`; empty object → `None`.
2. Build + clippy clean; `cargo test --lib mcp` green (existing MCP tests still pass — the new param + field must not break them; any test constructing `McpToolProxy` directly or calling `create_tool_proxies` is updated to pass `None`/`(None, false)`).

## Scope / files

| File | Change |
|---|---|
| `src-tauri/src/mcp.rs` | `dual_write_pages` field on `McpToolProxy`; `parse_put_page_args` helper (+ tests); capture-before-move + success-branch `shadow_write_page` in `execute`; two new params on `create_tool_proxies` + the arming `if`; `McpToolProxy::new` defaults the field to `None` |
| `src-tauri/src/agent/tools/registry_build.rs` | read flag + pass `Some(adapter)` + flag into `create_tool_proxies` |

**Out of scope (later P2 sub-slices):** **P2c** read repoint (chat recall + query/search + LLM tools → adapter); **P2d** retire gbrain MCP + Bun/PGLite + source + gbrain_prompt system-prompt block.

## Risk

Low. Additive / non-destructive — gbrain stays the primary read+write path; the adapter receives a best-effort shadow copy (not read until P2c). Gate defaults on; rollback = flip `gbrain_dual_write_pages_enabled` false. Single chokepoint; **only the gbrain put_page proxy is armed** — every other MCP tool's `execute` is byte-identical (the new field is `None`). Blast radius: the `create_tool_proxies` signature change (callers: registry_build + any test) and the new `McpToolProxy` field (both constructors updated). One branch, bisectable.

**Known minor (noted, not blocking):** if gbrain canonicalizes the slug server-side, the adapter stores under the LLM's *raw* slug → possible slug divergence between the two stores — the same property as P2a-1's Rust call sites (which also pass the raw slug). Acceptable for a best-effort transition copy; a future P2b-style re-sync (or P2c's reconciliation) heals it. **With P2a-2, the write side is fully covered — P2c (read repoint) is unblocked.**
