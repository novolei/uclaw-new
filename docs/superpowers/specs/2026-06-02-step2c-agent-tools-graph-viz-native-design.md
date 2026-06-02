# Step 2c — Agent gbrain Tools → Native + Graph-Viz Re-back Design

**Date:** 2026-06-02
**Status:** Design (recon done; pending spec review → plan)
**Part of:** Step 2 (retire gbrain — the last external runtime, Bun+PGLite). Follows 2a (WikiView re-back, PR #644) and 2b (page-write reroute, PR #645). Precedes 2d (transport/Bun/PGLite/boot teardown). After 2c, **no agent tool and no graph view depends on the gbrain MCP server** — the only thing keeping gbrain wired is the boot/transport plumbing 2d removes.

## Problem

After 2a (WikiView reads memory_graph EntityPages) and 2b (all page-WRITE pipelines write `write_page` = EntityPage + bucket_seal; dual-write shadow leg + flag gone), gbrain still has two live consumers:

1. **The agent loop** sees gbrain as 6 MCP tools (`mcp__gbrain__{put_page,get_page,list_pages,search,query,think}`) via `McpToolProxy` (`mcp/mod.rs`). Of these, the 4 reads (`get_page`/`list_pages`/`search`/`query`) are already served from bucket_seal via the **read-repoint** (`mcp/gbrain_read_repoint.rs`, gated `gbrain_read_repoint_enabled`, default on) — but only as a proxy intercept; the gbrain MCP server is still connected to provide the tool *schemas*, and `put_page` + `think` still hit gbrain. A `GbrainKnowledgeSection` system-prompt block (`agent/gbrain_prompt.rs`) teaches the agent to call these gbrain tools.
2. **The graph viz** — `DualNebulaView`'s knowledge layer calls `gbrain_full_graph` (`tauri_commands.rs:1398` → `gbrain::browse::full_graph` → gbrain MCP). The `gbrain_traverse_graph` cmd (`:1347`) exists + is exported in the FE (`gbrain-browse.ts`) but is **never actually called** (a 2a-left stub).

So the agent loop and the graph viz both keep a hollow gbrain MCP dependency. 2c cuts both, so 2d can tear down the runtime.

## Decision (approved 2026-06-02)

- **Approach B (genuine native tools, cut gbrain out of the agent loop).** Replace the `mcp__gbrain__*` proxies with real native `Tool` impls backed by `MemoryGraphStore` (EntityPage) + `BucketSealAdapter`. Stop exposing gbrain MCP tools to the agent. The read-repoint *mechanism* (intercept + flag) is then dead and removed — reads go native directly.
- **Drop `think`** (YAGNI — LLM multi-round synthesis with no native equivalent; the agent already has `memory_query`/`memory_search_pages` recall + its own reasoning).
- **One spec, two parts:** Part A (agent tools + prompt), Part B (graph viz re-back). Plan slices into bisectable tasks.
- **Naming:** new native tools use the `memory_*` prefix (consistent with 2a's `memory_entity_page_*` Tauri cmds; drops the misleading `mcp__gbrain__*` framing).
- **Prompt:** delete `agent/gbrain_prompt.rs` + the `set_gbrain_knowledge_block` plumbing; fold the "when to save a page / slug format / ≤500 words" guidance into the `memory_put_page` tool **description** (self-describing tools, fewer system-prompt tokens, kernel-lean per the philosophy ADR).
- **Dead traverse stub:** remove `gbrain_traverse_graph` (cmd + FE wrapper) outright — no native traverse (DualNebulaView only uses `full_graph`).

## Design

### Part A — native agent page tools

Five native `Tool` impls (new file `agent/tools/builtin/memory_pages.rs`, mirroring `MemuMemoryTool`'s stateful pattern — each holds `Arc<MemoryGraphStore>` and/or `Arc<BucketSealAdapter>`), replacing the 6 gbrain proxies (minus `think`):

| New native tool | Backed by | Effects / approval |
|---|---|---|
| `memory_put_page` | `write_page(store, adapter, space, slug, md)` (2b primitive) | write; `ApprovalRequirement::Never` (matches gbrain's auto_approve) |
| `memory_get_page` | `store.find_entity_page_by_slug(space, slug)` | read |
| `memory_list_pages` | `store.list_entity_pages(space, None, limit)` | read |
| `memory_search_pages` | `store.entity_page_search(space, query, limit)` (FTS) | read |
| `memory_query` | `BucketSealAdapter::recall_hybrid(query, Some("pages"), limit)` (semantic+FTS) | read |

- **Space:** the agent loop today hard-codes `"default"` for page ops (see `tauri_commands.rs` comment at the old space sites + `DEFAULT_SPACE_ID`). Use `crate::memory_graph::DEFAULT_SPACE_ID` for these tools (2c keeps it simple; a session-space upgrade is a later concern). Confirm in the plan whether a session space is cleanly reachable from `SessionContext`; if so use it, else `DEFAULT_SPACE_ID`.
- **Registration:** register in `agent/tools/registry_build.rs` (stateful path, like `MemuMemoryTool` — `tools.register(MemoryPutPageTool::new(store.clone(), adapter.clone()))` etc.), using `state.memory_graph_store` + `state.bucket_seal_adapter`.
- **Output format:** each tool formats its result as text (markdown for get_page; a slug/title list for list_pages/search/query; a confirmation for put_page) — mirror what the gbrain tools returned so the agent's expectations don't shift. The plan pins the exact text shapes (reuse the `gbrain_read_repoint::serve` formatting where it already produced good text).
- **Result:** the agent gains an equivalent native page toolset; nothing it calls routes through the gbrain MCP server.

### Part A — remove the gbrain MCP tool exposure + read-repoint + prompt

- **Stop exposing gbrain MCP tools to the agent:** remove gbrain from the agent's MCP-proxy creation path / the gbrain tool allowlist (`mcp/mod.rs:562-571`) so `create_tool_proxies` no longer emits `mcp__gbrain__*` proxies into the agent `ToolRegistry`. (The gbrain MCP *server* may still be connected at boot — that's 2d; 2c just stops the agent from seeing its tools.)
- **Delete the read-repoint mechanism** (now dead — reads are native): `mcp/gbrain_read_repoint.rs`, the `read_repoint` field on `McpToolProxy`, `GbrainProxyCfg`, the early-serve block in `McpToolProxy::execute`, and the `gbrain_read_repoint_enabled` flag (`memubot_config.rs` field + default fn + default init + serde tests) + its read sites (`registry_build.rs:51`, `tauri_commands.rs:1662/11087/14957`). The compiler enumerates every site.
- **Delete the knowledge prompt:** `agent/gbrain_prompt.rs` (whole file), the `gbrain_knowledge` field on `PromptBlocks` (`dispatcher/mod.rs`) + `SystemPromptContext` (`content_assembler.rs`), `set_gbrain_knowledge_block`, the dynamic-context injection (`content_assembler.rs:258-261`), and the 3 `GbrainKnowledgeSection::render` call sites (`tauri_commands.rs` ~2074/11268/15110). Fold the put_page usage guidance into `memory_put_page`'s description.

### Part B — graph viz re-back

- **New native Tauri cmd** `memory_entity_page_full_graph(space_id: Option<String>, limit: Option<u32>) -> KnowledgeGraph` in `tauri_commands.rs` (register in `main.rs` invoke_handler). Returns the **same wire shape** gbrain returned so the FE needs zero render changes: `KnowledgeGraph { nodes: [{slug, title, type}], edges: [{from_slug, to_slug, link_type}] }`. Implementation (in `store.rs` or an assembler near it): `list_entity_pages(space, None, limit)` for nodes + `list_all_edges()` filtered to edges where both endpoints are `entity_page` nodes, doing the **UUID→slug translation server-side** (build an `id → metadata.slug` map; map `relation_kind` → `link_type` string; `subkind` → `type`). Mirror gbrain's `assemble_graph` shape. Reuse `crate::memory_graph::DEFAULT_SPACE_ID` default.
- **FE repoint** (mirror 2a's shim): in `ui/src/lib/gbrain-browse.ts`, repoint the `gbrainFullGraph(limit)` wrapper from `invoke('gbrain_full_graph')` → `invoke('memory_entity_page_full_graph', { spaceId: null, limit })`, keeping the `KnowledgeGraph` TS DTO identical. `DualNebulaView.tsx`, `buildUnifiedScene.ts`, `MemoryModule.tsx` unchanged.
- **Remove the dead traverse stub:** delete `gbrain_traverse_graph` (Tauri cmd `tauri_commands.rs:1347` + `main.rs` macro entry + `gbrain::browse::traverse_graph` if now unused) and the FE `gbrainTraverseGraph` wrapper (`gbrain-browse.ts`) — it is never called.
- **Remove `gbrain_full_graph`** (Tauri cmd + macro entry + `gbrain::browse::full_graph`/`get_links` if now unused) once the FE is repointed.

### Finish-line / removal (2c scope)

After Parts A+B, also clean what becomes FE-dead/agent-dead:
- The other `gbrain_*` Tauri cmds (`gbrain_get_stats`, `gbrain_get_backlinks`, `gbrain_get_versions`, `gbrain_revert_version`, `gbrain_find_orphans`, etc.): the plan greps `ui/src` for remaining callers. 2a repointed WikiView to `memory_entity_page_*`, so most are likely FE-dead → delete (cmd + macro + the `gbrain::browse` fn if unused). Any with a remaining caller → repoint to the `memory_entity_page_*` equivalent. Genuinely-ambiguous ones may defer to 2d (noted in the plan), but the goal is: **no `gbrain_*` Tauri cmd the FE still calls remains gbrain-backed.**
- Grep gate: no `mcp__gbrain__` tool exposure to the agent; no `gbrain_read_repoint`; no `GbrainKnowledgeSection`; no `gbrain_full_graph`/`gbrain_traverse_graph`; `gbrain_read_repoint_enabled` flag gone.

## Out of scope (2d)

`GbrainCliTransport`, `find_bun`/`find_gbrain_entry`, `ensure_bundled_gbrain_initialized`/`seed_bundled_gbrain` boot, `GbrainAdapter` (`memory_adapter/gbrain.rs`) + its `app.rs` registration, `bunembed/`, `gbrain-source/`, `setup-{bun,gbrain,init}` scripts, tauri.conf bun/gbrain entries, the `gbrain::browse` module remnants still used by 2d-scope code. After 2c the gbrain MCP server still *boots* but serves nothing — 2d removes the boot = TRUE zero external runtime (no Bun, no PGLite).

NOTE: keep `gbrain::browse::split_frontmatter` / `build_raw_markdown` (pure formatting utils reused by `page_dual_write.rs` / memorization) until 2d confirms their consumers; only remove the gbrain *network* fns (`full_graph`, `traverse_graph`, `get_links`, etc.) that become unused.

## Error handling

Native read tools: store/adapter errors → `ToolError`/`ToolOutput::error` (mirror the read-repoint's error formatting). `memory_put_page`: `write_page` is authoritative EntityPage + best-effort bucket_seal shadow (2b posture) — a write failure surfaces as a tool error. The graph cmd: store errors → `Result<_, String>` (standard Tauri cmd error).

## Testing

1. **Native tools (unit):** each `memory_*` tool over an in-memory `MemoryGraphStore` + fake `BucketSealAdapter` (the `page_dual_write.rs` test fixture pattern): `memory_put_page` creates an EntityPage + bucket_seal page; `memory_get_page` round-trips; `memory_list_pages`/`memory_search_pages`/`memory_query` return the seeded page.
2. **Graph cmd (unit):** seed N EntityPages + edges → `full_graph` assembler returns slug-keyed nodes + slug-resolved edges (no UUIDs leak); edges to non-entity_page nodes excluded.
3. **Removal gates (grep):** no `mcp__gbrain__` agent exposure, no `gbrain_read_repoint`, no `GbrainKnowledgeSection`, no `gbrain_read_repoint_enabled`, no `gbrain_full_graph`/`gbrain_traverse_graph`.
4. `cargo build` + `cargo clippy --lib` clean; `cargo test --lib agent::tools memory_graph mcp memubot_config` green; `cd ui && npx tsc --noEmit` delta empty + vitest for the memory views green.
5. **Manual soak:** agent saves a page via `memory_put_page` → appears in WikiView (EntityPage); recall via `memory_query`; the DualNebulaView "dual" tab renders the knowledge layer from `memory_entity_page_full_graph` (no gbrain call in the log).

## Scope / files

| File | Change |
|---|---|
| `agent/tools/builtin/memory_pages.rs` (new) | 5 native page `Tool` impls (put/get/list/search/query) |
| `agent/tools/registry_build.rs` | register the 5 native tools; drop gbrain proxy exposure + read-repoint flag read |
| `mcp/mod.rs` | drop gbrain tool allowlist/proxy exposure to agent; delete `read_repoint` field + `GbrainProxyCfg` + early-serve block |
| `mcp/gbrain_read_repoint.rs` | delete (whole file) |
| `agent/gbrain_prompt.rs` | delete (whole file) |
| `agent/dispatcher/mod.rs`, `agent/dispatcher/content_assembler.rs` | drop `gbrain_knowledge` field + `set_gbrain_knowledge_block` + injection |
| `memubot_config.rs` | drop `gbrain_read_repoint_enabled` field + default + serde tests |
| `memory_graph/store.rs` (or assembler) | `entity_page_full_graph` (slug-keyed KnowledgeGraph assembler) |
| `tauri_commands.rs` + `main.rs` | new `memory_entity_page_full_graph` cmd (+ macro); delete `gbrain_full_graph`/`gbrain_traverse_graph` + FE-dead `gbrain_*` cmds; drop 3 `GbrainKnowledgeSection::render` sites + flag reads |
| `ui/src/lib/gbrain-browse.ts` | repoint `gbrainFullGraph` → `memory_entity_page_full_graph`; delete `gbrainTraverseGraph` |
| `gbrain/browse.rs` | delete now-unused network fns (`full_graph`/`traverse_graph`/`get_links`); keep pure utils for 2d |

## Risk

Med. Part A is mostly mechanical (5 native tools mirror existing store/adapter calls; read-repoint deletion is compiler-guided) — the one judgment point is the tool output text shapes (pin them in the plan to avoid shifting agent behavior). Part B's only nuance is the server-side UUID→slug edge translation (well-bounded; gbrain's `assemble_graph` is the reference) keeping the FE wire shape identical (zero FE render change, like 2a). Finish-line clean: the flag + prompt + gbrain agent exposure + graph cmds all removed (grep-gated), not feature-flagged. gbrain still boots (2d teardown) but serves nothing.
