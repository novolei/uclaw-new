# Step 3b-2 — Recall Repoint (memU read legs → bucket_seal) Design

**Date:** 2026-06-01
**Status:** Design (Decision C = Option B approved in brainstorming; pending spec review → plan)
**Part of:** Memory two-layer finish-line (ADR `2026-06-01-memory-two-layer-terminal-state.md`), Step 3 (remove memU), sub-slice **3b-2**. Follows 3b-1 (embedder finish, PR #638). Precedes 3b-3 (memorize/extraction) and 3b-4 (teardown).

## Problem

Three consumers still READ from memU's store. After this slice, memU's read/recall role is gone; only its write/extraction role (3b-3) and the bridge (3b-4) remain.

1. **`MemuMemoryTool`** (`agent/tools/memu_tools.rs`) — the agent's long-term-memory tool. Three paths: a skill-ranking fast path (already on the bucket_seal adapter), a "list all memory" path (`client.list_items`), and a standard retrieve path (`client.retrieve_with_context`).
2. **`MemuTodosTool`** (`agent/tools/memu_tools.rs:538`) — `client.retrieve_with_context(query, Some(&["event","knowledge"]), 20, false)` + a todo post-filter.
3. **memory_graph passive recall L3 vector leg** (`memory_graph/recall.rs:1100-1291`) — fuses an FTS leg with a `memu.retrieve` leg by `node_id` (RRF/weighted).

## Decision C (approved): Option B — bucket_seal as a separate semantic section

The memU vector leg fuses by **memory_graph `node_id`**; bucket_seal `MemoryEntry` carries its own tree-summary id, not a memory_graph node_id, so it **cannot** be fused into the node_id-keyed RRF (rejected: ID-mapping hack = architectural debt). Instead: **drop** the memU leg from L3 (L3 → FTS-over-memory_graph-nodes only, with the existing `fts_fallback_limit_multiplier` applied unconditionally), and **add bucket_seal recall as its own semantic section** in the recall prompt. This realizes the two-layer design — memory_graph supplies FTS + graph over its rich nodes; bucket_seal supplies the semantic layer — and keeps passive recall semantically capable (rejected: Option A drop-leg-entirely, which regresses passive semantic recall). The two agent tools repoint to bucket_seal regardless of Decision C.

## Key facts (recon-confirmed)

- `MemoryAdapter` trait (`memory_adapter/traits.rs:19`): `recall(query, namespace, limit)`, `list(namespace, limit)`, `get`, `store`, `delete`, `clear_namespace`, `namespace_summaries`. The `BucketSealAdapter` implements it; **verify `BucketSealAdapter::recall` delegates to `recall_hybrid`** (semantic + FTS backfill) so callers get hybrid quality via the trait — if it only does one leg, callers use the concrete `recall_hybrid(query, namespace: Option<&str>, max) -> Vec<MemoryEntry>` instead.
- `recall_hybrid` returns `Vec<MemoryEntry>` directly (best-effort, no `Result`). `MemoryEntry` (`memory_adapter/types.rs`): `id`, `key`, `content`, `namespace`, `category: MemoryCategory`, `timestamp` (RFC3339), `session_id`, `score: Option<f64>`.
- **The bucket_seal adapter is ALREADY wired into the tools** (`registry_build.rs:79-83` passes `state.bucket_seal_adapter` to `register_memu_tools`; `MemuMemoryTool` already holds `skill_adapter` + uses it). No new wiring for Part 1 — only repoint the call paths + add the adapter to `MemuTodosTool`.
- `MemoryRecallEngine` (`recall.rs:335`): `new(store, memu_client, config)`. Constructed at 4 sites: `tauri_commands.rs:2314`, `proactive/proactive_recall.rs:262` + `:395`, `proactive/hybrid_search.rs:281`. `state.bucket_seal_adapter` is in scope at each.
- `format_recall_for_prompt` (`recall.rs:472-831`): per-section `<memory_context>` with cascading token budget; appending a section before `</memory_context>` is trivial.
- L3 FTS already has a no-memU graceful path (`fts_limit` expands by `fts_fallback_limit_multiplier` when `memu_client` is None — `recall.rs:1106-1112`).

## Design

### Part 1 — agent tools → bucket_seal (finish-line: drop their memU client field)

- **`MemuMemoryTool`**:
  - skill-ranking path: unchanged (already adapter).
  - "list all" path: `adapter.list(Some(&space_id), list_limit)` instead of `client.list_items`.
  - standard retrieve path: `adapter.recall(&query, Some(&space_id), retrieve_limit)` instead of `client.retrieve_with_context`.
  - Make the adapter a REQUIRED field (`Arc<dyn MemoryAdapter>`, not `Option`); **drop the `client: Option<Arc<MemUClient>>` field** (no remaining use). The 15s wall-clock cap stays as a cheap safeguard (bucket_seal is local; keep it harmless).
- **`MemuTodosTool`**: `adapter.recall(query, Some(&space_id), 20)` + the content-based todo filter (bucket_seal `MemoryEntry` has no `categories`; filter on `content.contains("todo"/"待办")`). Add an adapter field; drop the client field.
- **Output mapping** (keep the agent-facing JSON shape): `content ← entry.content`; `type ← entry.category` (enum → string); `relevance ← entry.score.unwrap_or(0.0)`; `categories ← [entry.category]` (single-element); `created_at ← entry.timestamp`.
- **`register_memu_tools`** (`memu_tools.rs:697`): signature drops `memu_client`, takes the required adapter for BOTH tools; `registry_build.rs` updates the call.

### Part 2 — passive recall: drop L3 memU leg + add a bucket_seal semantic section

- **`MemoryRecallEngine`**: replace `memu_client: Option<Arc<MemUClient>>` with `bucket_seal_adapter: Option<Arc<dyn MemoryAdapter>>` (the engine has no other memU use after the L3 leg is removed — **verify** by grepping `memu` in `recall.rs`; if confirmed, drop the memU field entirely). Update the 4 construction sites to pass `state.bucket_seal_adapter` (as `Option<Arc<dyn MemoryAdapter>>`).
- **L3 `layer_relevant`**: remove the `memu.retrieve` call, `vector_rank_map`, and the vector side of the RRF/weighted fusion. L3 becomes FTS-over-memory_graph-nodes ranked by FTS score; apply `fts_fallback_limit_multiplier` unconditionally (the old "no memU" branch becomes the only branch). Phase-5 EntityPage/backlink boosts stay.
- **New semantic leg + section**: add `semantic_summaries: Vec<MemoryEntry>` to `MemoryRecallPlan`. A new async leg calls `bucket_seal_adapter.recall(user_input, namespace, semantic_limit)` (namespace = the recall space_id or None — decide in the plan). `format_recall_for_prompt` renders a new `## 语义摘要 (bucket_seal)` / `## Semantic Summaries` section (own token-budget allocation, CJK-aware), deduped against other sections by content where cheap.
- **Config**: a `semantic_limit` (reuse `seed_limit` or add a field) + the section's budget weight in `MemoryRecallConfig`. Keep changes minimal; reuse existing budget machinery.

### Data flow

```
passive recall (build_recall_plan):
  L1 boot, L2 triggered  (memory_graph, unchanged)
  L3 relevant            → FTS over memory_graph nodes ONLY (memU leg removed)
  L4 expanded, L5 recent (memory_graph, unchanged)
  NEW semantic leg       → bucket_seal_adapter.recall(query, ns, limit) → MemoryEntry[]
  format_recall_for_prompt → existing sections + "## 语义摘要 (bucket_seal)"

agent tool MemuMemoryTool/MemuTodosTool → bucket_seal adapter.recall/list (no memU)
```

## Error handling

bucket_seal recall is best-effort (`recall_hybrid` returns `Vec`, never errors); an empty result → the semantic section is omitted (same posture as today's empty L3). Tool paths: empty recall → the existing "no memories" degradation. No memU timeout path remains for these consumers.

## Testing

1. **Tool output shape** (unit): feed a fake bucket_seal adapter returning known `MemoryEntry`s; assert MemuMemoryTool retrieve/list + MemuTodosTool produce the expected JSON (content/type/relevance/categories/created_at) and the todo content-filter works.
2. **L3 FTS-only** (unit): with the memU leg removed, L3 returns FTS-ranked memory_graph nodes; `fts_limit` uses the multiplier path; Phase-5 boosts still apply.
3. **Semantic section render** (unit): a plan with `semantic_summaries` populated renders the new section within budget; empty → section omitted.
4. **Engine construction**: all 4 sites compile with the adapter; a no-adapter (None) engine still produces a plan (semantic section just empty).
5. `cargo build` + clippy clean; targeted tests green; **gate:** no `memu`/`MemUClient`/`retrieve`/`list_items` references remain in `recall.rs` or the two tools.

## Scope / files

| File | Change |
|---|---|
| `agent/tools/memu_tools.rs` | Both tools → adapter `recall`/`list`; drop client fields; `register_memu_tools` signature |
| `agent/tools/registry_build.rs` | Update `register_memu_tools` call (adapter for both, drop memu_client) |
| `memory_graph/recall.rs` | Engine field memU→bucket_seal adapter; L3 drop memU leg; new semantic leg + plan field + prompt section + config |
| `tauri_commands.rs:2314`, `proactive/proactive_recall.rs:262,395`, `proactive/hybrid_search.rs:281` | Pass `state.bucket_seal_adapter` to `MemoryRecallEngine::new` |

**Out of scope:** the memU write/extraction paths (memorize/create_item in MemorizationService/ReflectionEngine/ProactiveService) — Step 3b-3. The `MemUClient`/bridge — Step 3b-4. `MemUClient::retrieve`/`list_items` methods stay defined (deleted in 3b-4) but lose their app callers here.

## Risk

Medium. The agent-tool repoint is low-risk (adapter wired, shape compatible, local = faster). The passive-recall change is the judgment call: dropping L3's semantic fusion is mitigated by the new bucket_seal section (which is denser/curated) + L4 graph expansion. Net semantic coverage should be comparable-or-better, but the *surface* differs (bucket_seal tree summaries vs memory_graph raw nodes) — validate with a few synonym-query smoke checks post-merge. Bisectable: tools first (Part 1), then engine wiring, then L3 drop, then the new section.
