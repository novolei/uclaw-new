# Step 2c — Agent gbrain Tools → Native + Graph-Viz Re-back Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Replace the agent's `mcp__gbrain__*` MCP tools with native `memory_*` page tools (memory_graph EntityPage + bucket_seal), delete the read-repoint mechanism + flag + the gbrain knowledge prompt, and re-back `DualNebulaView`'s graph from gbrain → a native `memory_entity_page_full_graph` cmd — so no agent tool and no graph view depends on the gbrain MCP server (2d then tears down the runtime).

**Architecture (Approach B):** Genuine native `Tool` impls cut the agent loop off gbrain (not a proxy shell). Order: add native tools (T1) → remove gbrain agent exposure + read-repoint + flag (T2) → remove the knowledge prompt (T3) → add native graph cmd (T4) → FE repoint + remove gbrain graph cmds (T5) → clean FE-dead gbrain_* cmds (T6) → verify + ship (T7). The compiler + grep gates are the guard.

**Tech Stack:** Rust (Tauri, rusqlite, async_trait), TypeScript/React (FE shim repoint), no new deps.

**Key facts (recon, file:line — read these references; mirror, don't reinvent):**
- **Tool trait**: `agent/tools/tool.rs:347` — required: `name()->&str`, `description()->&str`, `parameters_schema()->serde_json::Value`, `async execute(&self, params)->Result<ToolOutput,ToolError>`. Optional: `requires_approval()->ApprovalRequirement` (default `Never`), `effects()->ToolEffects` (default `write()`). `ToolOutput::success(text,ms)`/`error(text,ms)` at `tool.rs:21`. `ToolRegistry::register(t)` at `tool.rs:468`.
- **Stateful-tool exemplar**: `agent/tools/memu_tools.rs:26` (`MemuMemoryTool` holds `Arc<BucketSealAdapter>`, registered at `registry_build.rs:77` via `tools.register(MemuMemoryTool::new(Arc::clone(&state.bucket_seal_adapter)))`).
- **Store methods** (`memory_graph/store.rs`): `entity_page_put(space,slug,raw_markdown)->Result<MemoryNodeDetail,Error>` (:1533); `find_entity_page_by_slug(space,slug)->Result<Option<MemoryNodeDetail>,Error>` (:1353); `list_entity_pages(space, subkind_filter:Option<&str>, limit)` (:1394); `entity_page_search(space,query,limit)` (:1792); `list_all_edges()->Vec<MemoryEdge>` (:799); `list_all_nodes(limit)` (:526). `MemoryEdge` fields: `parent_node_id:Option<String>`, `child_node_id:String`, `relation_kind:MemoryRelationKind`. `MemoryNode`: `id`(UUID), `metadata`(JSON; slug at `$.slug`, subkind at `$.subkind`), `title`, `kind`.
- **bucket_seal**: `BucketSealAdapter::recall_hybrid(query, namespace:Option<&str>, max)` (`memory_bucket_seal/adapter.rs:200`). `crate::memory_graph::DEFAULT_SPACE_ID = "default"`.
- **write_page** (2b primitive, `memory_adapter/page_dual_write.rs:62`): `write_page(&Arc<MemoryGraphStore>, &Arc<dyn MemoryAdapter>, space, slug, md) -> anyhow::Result<()>`.
- **Read-repoint output formatting to mirror**: `mcp/gbrain_read_repoint.rs` `serve()` — get_page→page markdown, list_pages→`pages::list_all`, search→`pages::search_pages`, query→`recall_hybrid`. Reuse its text shapes.
- **gbrain agent surface**: allowlist `mcp/mod.rs:562-571` (`search,query,list_pages,think,get_page,put_page`); proxies built in `create_tool_proxies` (`mcp/mod.rs:2673`); `read_repoint` field on `McpToolProxy` (`:1815`), `GbrainProxyCfg{read,read_enabled}` (`:2726`), early-serve block in `execute` (`:1854-1862`).
- **flag**: `gbrain_read_repoint_enabled` — `memubot_config.rs:423/659/752` (+ serde tests near :1734); reads at `registry_build.rs:51`, `tauri_commands.rs:1662/11087/14957`.
- **knowledge prompt**: `agent/gbrain_prompt.rs` (whole file, `GbrainKnowledgeSection::render`); field `PromptBlocks.gbrain_knowledge` (`dispatcher/mod.rs:74`); `SystemPromptContext.gbrain_knowledge_block` + injection (`content_assembler.rs:258-261,371`); `set_gbrain_knowledge_block` (`content_assembler.rs:548`); 3 render sites in `tauri_commands.rs` (~2074, ~11268, ~15110).
- **graph viz**: `gbrain_full_graph` cmd (`tauri_commands.rs:1398`→`gbrain::browse::full_graph` `browse.rs:370`); `gbrain_traverse_graph` cmd (`:1347`→`browse::traverse_graph` `:298`, FE-unused). `KnowledgeGraph{nodes:[{slug,title,type(serde rename page_type)}],edges:[{from_slug,to_slug,link_type}]}` (`browse.rs:329-350`); assembler reference `assemble_graph` (`browse.rs:358`). FE: `ui/src/lib/gbrain-browse.ts` `gbrainFullGraph(limit)` (:320→`invoke('gbrain_full_graph')`), `gbrainTraverseGraph` (:317, unused). `MemoryModule.tsx:80` calls `gbrainFullGraph(150)`. `DualNebulaView.tsx` + `dual-nebula/buildUnifiedScene.ts` consume `KnowledgeGraph` (slug-keyed) — leave UNCHANGED.
- **2a precedent**: the `memory_entity_page_*` cmds (`tauri_commands.rs:7037-7276`) + the gbrain-browse.ts shim repoint pattern.

---

## Task 1: Native page tools (`memory_*`) — additive

**Files:**
- Create: `src-tauri/src/agent/tools/builtin/memory_pages.rs`
- Modify: `src-tauri/src/agent/tools/builtin/mod.rs` (add `pub mod memory_pages;`), `src-tauri/src/agent/tools/registry_build.rs` (register the 5 tools)

- [ ] **Step 1: Create the 5 tool structs + `Tool` impls.** Mirror `MemuMemoryTool` (`agent/tools/memu_tools.rs:26`). Each struct holds the handles it needs:
  - `MemoryPutPageTool { store: Arc<MemoryGraphStore>, adapter: Arc<dyn MemoryAdapter> }` — `name()="memory_put_page"`; `effects()=ToolEffects::write()`; `requires_approval()=Never`; params schema `{slug:string(required), content:string(required)}`; `execute` → `crate::memory_adapter::page_dual_write::write_page(&self.store, &self.adapter, crate::memory_graph::DEFAULT_SPACE_ID, slug, content).await` → `ToolOutput::success(format!("Saved page '{slug}'"), ms)` / on err `ToolOutput::error`. **Description** carries the usage guidance moved from the deleted prompt (see Step 2).
  - `MemoryGetPageTool { store }` — `name()="memory_get_page"`; `effects()=read()`; params `{slug:string(required)}`; `execute` → `store.find_entity_page_by_slug(DEFAULT_SPACE_ID, slug)?` → if `Some(detail)` return the active-version markdown (`detail.active_version...content`), else `ToolOutput::success("(no page '{slug}')")`.
  - `MemoryListPagesTool { store }` — `name()="memory_list_pages"`; `read()`; params `{limit:integer(optional, default 100)}`; `execute` → `store.list_entity_pages(DEFAULT_SPACE_ID, None, limit)` → format as a `- slug — title` list (mirror `gbrain_read_repoint::serve` list_pages text).
  - `MemorySearchPagesTool { store }` — `name()="memory_search_pages"`; `read()`; params `{query:string(required), limit:integer(optional, default 10)}`; `execute` → `store.entity_page_search(DEFAULT_SPACE_ID, query, limit)` → formatted hit list.
  - `MemoryQueryTool { adapter: Arc<dyn MemoryAdapter> }` — `name()="memory_query"`; `read()`; params `{query:string(required), limit:integer(optional, default 10)}`; `execute` → downcast/hold the concrete `Arc<BucketSealAdapter>` (recall_hybrid is concrete — see note) → `recall_hybrid(query, Some("pages"), limit)` → formatted results (mirror the read-repoint `query` text).
  > **NOTE (concrete adapter):** `recall_hybrid` is on the concrete `BucketSealAdapter`, not the `MemoryAdapter` trait (Step 1b/3b-2 precedent). `MemoryQueryTool` must hold `Arc<BucketSealAdapter>` (like `MemuMemoryTool`), NOT `Arc<dyn MemoryAdapter>`. `MemoryPutPageTool` needs `Arc<dyn MemoryAdapter>` for `write_page`'s shadow — pass `Arc::clone(&state.bucket_seal_adapter) as Arc<dyn MemoryAdapter>`.
- [ ] **Step 2: `memory_put_page` description = folded usage guidance.** Port the substance of `agent/gbrain_prompt.rs`'s `GBRAIN_INSTRUCTIONS` into the `MemoryPutPageTool::description()` (de-gbrained, renamed): when to save (new stable entities/facts/conclusions, "remember this"), slug format (kebab-case, namespaced), content format (YAML frontmatter + markdown, ≤500 words), and the negatives (no ephemeral content). Keep it tight (the description is read every turn).
- [ ] **Step 3: Register** in `registry_build.rs` (stateful path, near `MemuMemoryTool` at :77):
```rust
tools.register(MemoryPutPageTool::new(Arc::clone(&state.memory_graph_store), Arc::clone(&state.bucket_seal_adapter) as Arc<dyn crate::memory_adapter::MemoryAdapter>));
tools.register(MemoryGetPageTool::new(Arc::clone(&state.memory_graph_store)));
tools.register(MemoryListPagesTool::new(Arc::clone(&state.memory_graph_store)));
tools.register(MemorySearchPagesTool::new(Arc::clone(&state.memory_graph_store)));
tools.register(MemoryQueryTool::new(Arc::clone(&state.bucket_seal_adapter)));
```
  (gbrain proxies still register too — transient overlap is fine; T2 removes them.)
- [ ] **Step 4: Tests** in `memory_pages.rs` `#[cfg(test)]` — build an in-memory `MemoryGraphStore` + a fake/in-memory `MemoryAdapter`/`BucketSealAdapter` (reuse the `memory_adapter/page_dual_write.rs` test fixture: `Connection::open_in_memory` + `V4_MEMORY_GRAPH`+`V35_MEMORY_OS_PHASE_1`). Assert: `memory_put_page` creates an EntityPage (`find_entity_page_by_slug` Some); `memory_get_page` round-trips the markdown; `memory_list_pages`/`memory_search_pages` return the seeded page. (`memory_query` needs a real BucketSealAdapter — if a unit fixture is awkward, cover put/get/list/search and note query is covered by the read-repoint's existing tests / manual soak.)
- [ ] **Step 5: Build + clippy + test** — `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty); `cargo clippy --lib 2>&1 | grep "warning: "` (no new); `cargo test --lib agent::tools::builtin::memory_pages 2>&1 | tail` (green).
- [ ] **Step 6: Commit** — `feat(agent): native memory_* page tools (put/get/list/search/query) backed by EntityPage+bucket_seal (Step 2c)`

---

## Task 2: Remove gbrain agent tool exposure + read-repoint mechanism + flag

**Files:** `mcp/mod.rs`, `mcp/gbrain_read_repoint.rs` (delete), `memubot_config.rs`, `agent/tools/registry_build.rs`, `tauri_commands.rs`.

- [ ] **Step 1: Stop emitting gbrain proxies to the agent.** In `mcp/mod.rs`: remove gbrain from the agent tool allowlist (`:562-571`) so `create_tool_proxies` no longer yields `mcp__gbrain__*` tools to the agent registry. (Confirm the allowlist is what gates agent exposure; if proxies come from `all_tools()` filtered elsewhere, remove the gbrain entries at that seam. The gbrain MCP *server* stays connected at boot — 2d removes boot.)
- [ ] **Step 2: Delete the read-repoint mechanism.** Remove: the `read_repoint` field on `McpToolProxy` (`:1815`) + its constructor inits; the early-serve block in `McpToolProxy::execute` (`:1854-1862`); `GbrainProxyCfg` struct (`:2726`) + the `read`/`read_enabled` plumbing in `create_tool_proxies` (`:2710-2717`) + every `GbrainProxyCfg{...}` literal (`registry_build.rs:220-227`, `tauri_commands.rs:~14964`, the 2 test literals in `mcp/mod.rs`). Delete the whole file `mcp/gbrain_read_repoint.rs` + its `mod` declaration.
- [ ] **Step 3: Delete the flag.** Remove `gbrain_read_repoint_enabled` from `memubot_config.rs` (field `:659`, `default_gbrain_read_repoint_enabled` `:423`, default init `:752`, the serde tests near `:1734`) + every read (`registry_build.rs:51`, `tauri_commands.rs:1662/11087/14957`). Compiler enumerates.
- [ ] **Step 4: Build + clippy + test** — clean; `cargo test --lib mcp memubot_config 2>&1 | tail`.
- [ ] **Step 5: Commit** — `refactor(agent): drop gbrain MCP tool exposure to agent + read-repoint mechanism + gbrain_read_repoint_enabled flag (native memory_* tools cover it) (Step 2c)`

---

## Task 3: Remove the gbrain knowledge prompt

**Files:** `agent/gbrain_prompt.rs` (delete), `agent/dispatcher/mod.rs`, `agent/dispatcher/content_assembler.rs`, `tauri_commands.rs`, `agent/mod.rs` (or wherever `gbrain_prompt` is `mod`-declared).

- [ ] **Step 1: Delete the prompt + plumbing.** Delete `agent/gbrain_prompt.rs` + its `mod` decl. Remove `gbrain_knowledge` from `PromptBlocks` (`dispatcher/mod.rs:74`); remove `gbrain_knowledge_block` from `SystemPromptContext` + the injection in `content_assembler.rs:258-261` + the field set at `:371`; remove `set_gbrain_knowledge_block` (`content_assembler.rs:548`). Remove the 3 `GbrainKnowledgeSection::render(...)` + `set_gbrain_knowledge_block(...)` call sites in `tauri_commands.rs` (~2074, ~11268, ~15110). (The guidance now lives in `memory_put_page`'s description from T1.)
- [ ] **Step 2: Build + clippy + test** — clean; `cargo test --lib agent::dispatcher 2>&1 | tail`.
- [ ] **Step 3: Commit** — `refactor(agent): delete gbrain knowledge prompt block (guidance folded into memory_put_page description) (Step 2c)`

---

## Task 4: Native `memory_entity_page_full_graph` cmd + assembler — additive

**Files:** `memory_graph/store.rs` (or a small `memory_graph/graph_assemble.rs`), `tauri_commands.rs`, `main.rs`, `ipc.rs` (if an input/output DTO is needed).

- [ ] **Step 1: Assembler.** Add `entity_page_full_graph(&self, space_id: &str, limit: usize) -> Result<EntityKnowledgeGraph, Error>` on `MemoryGraphStore` (or a free fn taking `&self`). Logic: `list_entity_pages(space, None, limit)` → nodes; build `id -> slug` map (slug from `metadata.$.slug`, fallback `id`); `list_all_edges()` → keep only edges whose BOTH endpoints are in the map → map to `{from_slug, to_slug, link_type: relation_kind string}`; node `type` = `metadata.$.subkind` (fallback `"entity_page"`). Return a struct that **serializes to the same wire shape as gbrain's `KnowledgeGraph`**: `EntityKnowledgeGraph { nodes: Vec<{slug,title,type}>, edges: Vec<{from_slug,to_slug,link_type}> }` (use `#[serde(rename="type")]` on the node type field to match). Reference: `gbrain::browse::assemble_graph` (`browse.rs:358`).
- [ ] **Step 2: Tauri cmd** in `tauri_commands.rs`: `memory_entity_page_full_graph(state, space_id: Option<String>, limit: Option<u32>) -> Result<EntityKnowledgeGraph, String>` → `store.entity_page_full_graph(space_id.as_deref().unwrap_or(crate::memory_graph::DEFAULT_SPACE_ID), limit.unwrap_or(150) as usize).map_err(|e| e.to_string())`. Register in `main.rs` `invoke_handler!` (the two-edit rule — `uclaw-tauri-commands` skill).
- [ ] **Step 3: Test** (store unit, in `store.rs` tests or the assembler module): seed 3 EntityPages + 2 edges (one entity_page↔entity_page, one to a non-entity_page node) → `entity_page_full_graph` returns 3 slug-keyed nodes + 1 edge (the cross-kind edge excluded); no UUID leaks into `from_slug`/`to_slug`.
- [ ] **Step 4: Build + clippy + test** — clean; `cargo test --lib memory_graph 2>&1 | tail`.
- [ ] **Step 5: Commit** — `feat(memory): memory_entity_page_full_graph cmd (slug-keyed KnowledgeGraph from EntityPages, native) (Step 2c)`

---

## Task 5: FE repoint full_graph + remove gbrain graph cmds

**Files:** `ui/src/lib/gbrain-browse.ts`, `src-tauri/src/tauri_commands.rs`, `src-tauri/src/main.rs`, `src-tauri/src/gbrain/browse.rs`.

- [ ] **Step 1: FE repoint.** In `ui/src/lib/gbrain-browse.ts`, repoint `gbrainFullGraph(limit)` from `invoke('gbrain_full_graph', {limit})` → `invoke('memory_entity_page_full_graph', { spaceId: null, limit })` (keep the `KnowledgeGraph` TS DTO identical — verify the new cmd's JSON matches `{nodes:[{slug,title,type}],edges:[{from_slug,to_slug,link_type}]}`; check the Tauri arg-casing convention used elsewhere in this file, snake vs camel). Delete the unused `gbrainTraverseGraph` wrapper (`:317`). `DualNebulaView.tsx`/`buildUnifiedScene.ts`/`MemoryModule.tsx` UNCHANGED.
- [ ] **Step 2: Remove gbrain graph cmds.** Delete `gbrain_full_graph` (`tauri_commands.rs:1398`) + `gbrain_traverse_graph` (`:1347`) + their `main.rs` macro entries (`:1274,:1279`). Delete `gbrain::browse::full_graph` (`:370`) + `traverse_graph` (`:298`) + `assemble_graph` (`:358`) + `get_links` if now unused (grep first; keep `split_frontmatter`/`build_raw_markdown` — 2d).
- [ ] **Step 3: Verify** — `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty); `cd ui && npx tsc --noEmit 2>&1 | head` (delta empty vs baseline — `gbrainTraverseGraph` removal must have no remaining importers); `npm test -- --run 2>&1 | tail` (memory views green).
- [ ] **Step 4: Commit** — `refactor(memory): DualNebulaView graph re-backed to memory_entity_page_full_graph; remove gbrain_full_graph/traverse cmds (Step 2c)`

---

## Task 6: Clean remaining FE-dead gbrain_* Tauri cmds

**Files:** `tauri_commands.rs`, `main.rs`, `gbrain/browse.rs`.

- [ ] **Step 1: Grep for remaining FE callers** — `grep -rn "invoke('gbrain_\|invoke(\"gbrain_" ui/src` AND `grep -rn "gbrain_get_stats\|gbrain_get_backlinks\|gbrain_get_versions\|gbrain_revert_version\|gbrain_find_orphans\|gbrain_get_links\|gbrain_get_page\|gbrain_list_pages\|gbrain_search\|gbrain_query" ui/src src-tauri/src/tauri_commands.rs`. For each gbrain_* Tauri cmd: if NO FE caller remains (2a repointed WikiView to `memory_entity_page_*`) → delete the cmd + its `main.rs` macro entry + the underlying `gbrain::browse` fn if now unused. If a caller remains → repoint it to the `memory_entity_page_*` equivalent (T from 2a). If genuinely ambiguous/risky → leave + note in the commit body for 2d.
- [ ] **Step 2: Build + clippy** — `cargo build 2>&1 | grep -E "^error"` (empty); `cargo clippy --lib 2>&1 | grep "warning: "` (no new dead-code from removals); `cd ui && npx tsc --noEmit` (delta empty).
- [ ] **Step 3: Commit** — `refactor(memory): remove FE-dead gbrain_* Tauri cmds (superseded by memory_entity_page_* since 2a) (Step 2c)`

---

## Task 7: Whole-slice verification + ship

- [ ] **Step 1: Build + clippy + tests** — `cargo build` + `cargo clippy --lib` clean; `cargo test --lib agent::tools memory_graph mcp memubot_config 2>&1 | grep "test result:"` green; `cd ui && npx tsc --noEmit` delta empty + `npm test -- --run` memory views green.
- [ ] **Step 2: Grep gates (want empty):**
  - `grep -rn "mcp__gbrain__\|GbrainProxyCfg\|read_repoint\|gbrain_read_repoint" src-tauri/src` (no agent exposure / repoint mechanism).
  - `grep -rn "gbrain_read_repoint_enabled\|GbrainKnowledgeSection\|gbrain_knowledge" src-tauri/src` (flag + prompt gone).
  - `grep -rn "gbrain_full_graph\|gbrain_traverse_graph" src-tauri/src ui/src` (graph cmds gone).
  - Native present: `grep -rn "memory_put_page\|memory_entity_page_full_graph" src-tauri/src` (non-empty).
- [ ] **Step 3: Ship** — push → PR (Commits table T1-T6) → rebase-merge → sync parent main → worktree remove + branch cleanup → `npx gitnexus analyze` reindex.
- [ ] **Step 4: Post-merge soak (manual):** agent calls `memory_put_page` → page appears in WikiView (EntityPage); `memory_query`/`memory_search_pages` recall it; DualNebulaView "dual" tab renders the knowledge layer from `memory_entity_page_full_graph` — **no gbrain call in the log** for agent tools OR graph viz. (gbrain still boots — 2d removes it.)

---

## Self-Review

- **Spec coverage:** native tools (T1), remove gbrain agent exposure+repoint+flag (T2), remove prompt (T3), native graph cmd (T4), FE repoint + remove gbrain graph cmds (T5), clean FE-dead gbrain_* cmds (T6), verify+ship (T7). ✓ All spec §Design + §Finish-line items mapped.
- **Ordering keeps each commit compiling:** add native tools (T1, additive) → remove old agent path (T2/T3, native covers it) → add native graph cmd (T4, additive) → repoint FE + remove old graph cmds (T5) → clean stragglers (T6). ✓
- **Type consistency:** `EntityKnowledgeGraph` (T4) serializes to gbrain's `KnowledgeGraph` wire shape consumed by the FE (T5, unchanged DualNebulaView). `MemoryQueryTool` holds concrete `Arc<BucketSealAdapter>` (recall_hybrid is concrete); `MemoryPutPageTool` holds `Arc<dyn MemoryAdapter>` (write_page shadow). ✓
- **Two confirm-in-impl points (flagged):** (1) the exact seam that gates gbrain→agent exposure (allowlist vs `all_tools()` filter) — T2 Step 1; (2) FE Tauri arg-casing (spaceId/space_id) — T5 Step 1. Both call out "confirm by reading adjacent code."
- **Finish-line:** after T6, no agent tool / graph view / prompt / flag references gbrain; grep-gated (T7 Step 2). gbrain still boots (2d). ✓
- **No placeholders:** real signatures + file:line + the concrete-adapter note + the wire-shape-preservation detail.
