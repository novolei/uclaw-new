# Step 2b — Page-Write Reroute (gbrain → EntityPage + bucket_seal) Design

**Date:** 2026-06-02
**Status:** Design (recon done; pending spec review → plan)
**Part of:** Step 2 (retire gbrain). Follows 2a (WikiView re-back, PR #644). Closes 2a's transient gap: after 2a, WikiView reads memory_graph EntityPages, but the page-WRITE pipelines still write gbrain (so agent/extractor-created pages didn't show in WikiView). 2b reroutes those writes → **EntityPage (rich) + bucket_seal `pages` (recall projection)** — the two-layer split — and drops the gbrain write leg + flag. Precedes 2c (agent `mcp__gbrain__*` tools + graph viz) and 2d (boot/Bun/PGLite teardown).

## Problem

Page writes currently go to gbrain (primary) + bucket_seal (shadow) via `memory_adapter::page_dual_write::dual_write_page(mcp, adapter, slug, markdown, gbrain_dual_write_pages_enabled)`. Three pipelines write pages:
1. **chat_extractor** (proactive): `agent/dispatcher/turn_runner.rs:169` calls `mgr.call_tool("gbrain", "put_page", {slug, content})` directly (only `mgr` in scope).
2. **Scheme-A draft ingestion**: `memorization/service.rs` `ingest_draft_file` → `dual_write_page(...)`.
3. **memory_policy** `GbrainWrite` action → the Gbrain policy target → `dual_write_page`.

2a added the write API: `MemoryGraphStore::entity_page_put(space, slug, raw_markdown)` (rich EntityPage, normalizes `[[slug]]`, runs auto-link) + `memory_adapter::pages::put_page(adapter, &Page)` (bucket_seal recall projection). The `gbrain_put_page` Tauri command is now likely dead (2a repointed the FE `gbrainPutPage` → `memory_entity_page_put`).

## Decision (clear — the two-layer model)

Page writes go to **both**: memory_graph EntityPage (the rich/wiki layer WikiView reads) + bucket_seal `pages` (the FTS/semantic recall projection the agent reads). Drop the gbrain `browse::put_page` leg + the `gbrain_dual_write_pages_enabled` flag. This mirrors 2a's read model (WikiView ← EntityPages; agent recall ← bucket_seal pages) and the overall two-layer architecture.

## Design

### §1 Unified `write_page` (replace `dual_write_page`)

In `memory_adapter/page_dual_write.rs` (rename concept; keep file): replace
```rust
dual_write_page(mcp, adapter, slug, markdown, dual_enabled) -> gbrain + bucket_seal shadow
```
with
```rust
pub fn write_page(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    slug: &str,
    markdown: &str,
) -> anyhow::Result<()> {
    store.entity_page_put(space_id, slug, markdown)?;          // rich EntityPage (+auto-link, [[slug]] normalize)
    let page = markdown_to_page(slug, markdown);               // existing helper
    // bucket_seal shadow (recall projection) — best-effort, never fails the EntityPage write:
    let _ = pages::put_page(adapter, &page).await; /* or shadow_write_page */
    Ok(())
}
```
No `mcp` param, no gbrain, no flag. (`markdown_to_page`/`shadow_write_page` already exist in `page_dual_write.rs` — reuse.) Drop the `browse::put_page` call.

### §2 Reroute the 3 write sites

- **chat_extractor dispatcher** (`turn_runner.rs:169`): thread `memory_graph_store: Arc<MemoryGraphStore>` + the bucket_seal adapter into the `ChatDelegate`/dispatcher (mirror how an existing dep reaches it — e.g. how the gene retriever / mcp_mgr are injected; the AppState has both). Replace the `mgr.call_tool("gbrain","put_page", ...)` block with `write_page(&store, &adapter, space, &proposal.slug, &proposal.content)`. Keep the confidence gate + cost tag + chip event.
- **Scheme-A `ingest_draft_file`** (`memorization/service.rs`): MemorizationService gained a bucket_seal adapter (Step 3b-3) — confirm it also has/can-get the `memory_graph_store` (it likely does — reflection writes go through it). Replace the `dual_write_page(...)` call with `write_page(...)`.
- **memory_policy `GbrainWrite`** (`memory_policy/targets/gbrain.rs` + the `GbrainWrite` action in `types.rs`): repoint the target to `write_page` (EntityPage + bucket_seal) OR retire the `GbrainWrite` policy action if nothing emits it after the above. The plan resolves which (recon the policy router's emit sites).

### §3 Drop the flag + dead command

- Remove `gbrain_dual_write_pages_enabled` (`memubot_config.rs` field + `default_*` + the `MemoryOsConfig` default + the serde tests) and every read: `tauri_commands.rs:1390/14950`, `main.rs:247` (`set_dual_write_pages_enabled` + the setter itself), `app.rs:1096`. (Finish-line: the gbrain-dual-write transition flag is obsolete once writes are EntityPage+bucket_seal.)
- **`gbrain_put_page` Tauri command** (`tauri_commands.rs:1380` + macro `main.rs:1285`): confirm no FE/Rust caller remains (2a repointed the FE). If dead → delete it + macro entry. If a caller remains → repoint to `write_page`.
- Drop `gbrain::browse::put_page` if now unused (else leave for 2c — the agent put_page tool may still use it).

### Data flow (after 2b)

```
chat_extractor proposal ─┐
Scheme-A draft           ├─→ write_page(store, adapter, space, slug, md)
memory_policy GbrainWrite ┘         ├─→ memory_graph EntityPage (WikiView reads — 2a)
                                     └─→ bucket_seal pages (agent recall reads)
(no page WRITE hits gbrain; WikiView now shows extractor/draft pages too — 2a gap closed)
```

## Out of scope (2c / 2d)

The agent's explicit `mcp__gbrain__put_page` TOOL + the `mcp__gbrain__*` read tools + `gbrain_prompt` (2c); GbrainCliTransport / Bun / PGLite / gbrain-source / bundle / boot (2d). gbrain stays alive: its read tools still serve (via the read-repoint → bucket_seal), and the agent put_page tool still exists until 2c. After 2b, the proactive/draft/policy PIPELINES no longer write gbrain — only the agent's explicit tool might (2c removes it).

## Error handling

`entity_page_put` is the authoritative write (Result — propagate). The bucket_seal `pages` shadow is best-effort (log + swallow — a failed recall projection never blocks the EntityPage). Mirrors today's shadow posture.

## Testing

1. **`write_page` unit**: fixture store + fake adapter → creates an EntityPage (find_by_slug Some) AND a bucket_seal page (pages::get_page Some); slug re-write → new EntityPage version.
2. **Dispatcher**: with a mock store/adapter, a high-confidence proposal → `write_page` creates the EntityPage (assert), no gbrain call.
3. **Flag removal**: config deserializes without `gbrain_dual_write_pages_enabled` (serde default-tolerant); no read sites remain (`grep` gate).
4. `cargo build` + clippy clean; `cargo test --lib memory_adapter memorization proactive memory_graph` green.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/page_dual_write.rs` | `dual_write_page` → `write_page(store, adapter, space, slug, md)`; drop gbrain leg + mcp param |
| `agent/dispatcher/turn_runner.rs` (+ dispatcher struct/ctor) | thread store+adapter; chat_extractor put_page → `write_page` |
| `memorization/service.rs` | `ingest_draft_file` dual_write → `write_page` |
| `memory_policy/targets/gbrain.rs` + `types.rs` | repoint GbrainWrite → `write_page`, or retire the action |
| `memubot_config.rs` + `main.rs` + `tauri_commands.rs` + `app.rs` | drop `gbrain_dual_write_pages_enabled` flag + setter + reads; delete dead `gbrain_put_page` cmd |

## Risk

Med. The main implementation risk is **threading `memory_graph_store` + the bucket_seal adapter into the dispatcher** (`turn_runner.rs`) — mirror an existing injected dep; if genuinely hard, the plan flags it. Dropping the flag is finish-line-clean (the compiler catches every read). The memory_policy `GbrainWrite` decision (repoint vs retire) is resolved in the plan after reconning its emit sites. Bisectable: `write_page` + flag removal → dispatcher reroute → Scheme-A/policy reroute → verify. gbrain stays alive (no boot/tool changes here).
