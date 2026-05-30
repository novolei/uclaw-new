# 阶段 4 PR15 — Recall Routing into `effective_system_prompt` Design Spec

**Status:** Approved design (brainstorm forks locked) — proceeding to plan per "go".
**Date:** 2026-05-30
**Position in 阶段 4 sequence:** PR15 of 15 — **the closing slice**. Follows PR14 (GbrainAdapter). Completes the memory program.

---

## 1. Goal

Wire bucket_seal **hybrid recall** (semantic cosine over real summary embeddings + FTS5 over raw chunks) into the agent's per-turn system-prompt assembly — the first time the whole 阶段 4 memory engine (3 cascade-sealing trees + durable job queue + real LLM summaries + real embeddings) actually feeds the live agent. This is where the **PR12 embeddings finally get queried** (and the 1024-dim endpoint requirement becomes user-visible).

**The integration is additive, not greenfield.** The prompt *already* injects recalled memory at the chat/agent send sites (`tauri_commands.rs:2171+`) via the legacy `MemoryRecallEngine` (over `memory_graph_store` + memU + the session store), set through `delegate.set_memory_context(...)`. PR15 **adds a bucket_seal contribution** to that injection — a `## Relevant Memory (bucket-seal)` section appended to the memory context — without touching the working legacy/memU/session paths.

**Brainstorm decisions:**
- **Recall method = hybrid**: semantic summary rerank (embed query → cosine vs `mem_tree_summaries.embedding`) + FTS5 chunk candidates, merged/deduped/budgeted. Summaries first (dense, curated), chunk fragments as backfill.
- **Scope = bucket_seal only**: the primary engine. gbrain/legacy stay reachable via explicit IPC but don't auto-inject here.

**Out of scope:**
- No removal/change of the legacy graph/memU/session recall injection (it stays; bucket_seal is additive).
- No multi-backend fan-out (gbrain/legacy not auto-injected).
- No new IPC (recall is internal to prompt assembly; the unified `memory.unified.*` IPC from PR4 already exposes recall for external callers).

---

## 2. Why this slice closes 阶段 4

The engine was fully built (PR1-14) but never *queried by the live agent*. PR15 connects it: a per-turn semantic+keyword recall over bucket_seal, injected into the system prompt. After this, the loop is closed — chat/agent writes flow into the trees (PR8-13), and the agent reads them back via recall (PR15). The embeddings PR12 made real are finally used for retrieval.

---

## 3. Components

### 3.1 `BucketSealAdapter::recall_semantic` — `memory_bucket_seal/adapter.rs`

A new inherent (or trait-adjacent) method: semantic recall over summary embeddings.

```rust
impl BucketSealAdapter {
    /// Semantic recall: embed the query, cosine-rank summary embeddings,
    /// return the top-`limit` summaries as MemoryEntries. Summaries are the
    /// dense, curated recall unit (raw chunks come from FTS via `recall`).
    pub async fn recall_semantic(
        &self,
        query: &str,
        limit: usize,
        namespace: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        // 1. embed(query) via self.embedder  (1024-dim; errors if endpoint absent)
        // 2. list summaries (optionally namespace-scoped via their tree's scope)
        //    that have a non-NULL embedding
        // 3. cosine_similarity(query_vec, summary_vec) per summary
        // 4. sort desc, take `limit`, map SummaryNode → MemoryEntry
        //    (content=summary.content, score=Some(cosine), timestamp=sealed_at)
    }
}
```

- Reuses `score::embed::{cosine_similarity, unpack_embedding}` (PR7) + the adapter's existing `embedder`.
- Reads summaries via a store query (e.g. a new `list_summaries_with_embeddings(store, namespace_filter)` in `tree_source::store`, or iterate `list_summaries_at_level` across levels + `get_summary_embedding`). The plan picks the simplest correct read.
- Namespace scoping: a summary belongs to a tree (`tree_id` → `Tree.scope`); filter by matching the source-tree scope to `namespace` when provided. (Summaries from topic/global trees have entity/`"global"` scopes — included only when `namespace` is None, or matched explicitly.)
- Embed failure (no 1024-dim endpoint) → `Err`; the caller treats recall as best-effort (empty contribution, never blocks the turn).

### 3.2 Hybrid recall helper

Merges semantic summaries + FTS chunks into one ranked, deduped, budgeted list. Lives where it's cleanest — a free fn near the integration site or a thin `BucketSealAdapter::recall_hybrid`:

```rust
/// Hybrid: semantic summaries (dense) first, then FTS chunk hits (keyword)
/// as backfill. Dedup by entry id; cap to `max_entries`.
async fn recall_hybrid(adapter: &BucketSealAdapter, query: &str, namespace: Option<&str>, max_entries: usize)
    -> Vec<MemoryEntry>
```

- Semantic via `recall_semantic`; FTS via the existing `recall(query, limit, opts)` (PR9).
- Both are best-effort: if one errs, use the other; if both err, return empty.
- Dedup: a chunk whose content is already covered by a returned summary is lower priority; simple id-based dedup + summaries-first ordering is sufficient (no fuzzy overlap detection).

### 3.3 Format helper — `agent/memory_recall_block.rs` (new) or inline

```rust
pub const BUCKET_SEAL_RECALL_MARKER: &str = "## Relevant Memory (bucket-seal)";

/// Render recalled entries into a prompt block. Returns None when empty.
pub fn render_bucket_seal_recall(entries: &[MemoryEntry], token_budget: usize) -> Option<String>;
```

- Each entry → a labelled fragment (e.g. `- [{score:.2} · {namespace}] {content}`), summaries first.
- Respects `token_budget` (greedy fill, stop when exceeded — reuse an existing token estimator or chars/4).
- Marker const for tests/log-matching (mirrors `GBRAIN_SECTION_MARKER`).

### 3.4 Wiring — the prompt-build sites (`tauri_commands.rs`)

At the existing "Memory Recall Integration" block(s) (chat send ~2171; agent send paths ~11365/15188 where `set_memory_context`/`set_gbrain_knowledge_block` are called), after the legacy `memory_ctx` is assembled, ADD:

```rust
// PR15: bucket_seal hybrid recall contribution (additive — does not replace
// the legacy graph/memU/session recall above).
if !input.content.trim().is_empty() {
    if let Some(bs) = state.memory_adapters.get("bucket_seal") {
        // downcast to BucketSealAdapter for recall_semantic/recall_hybrid,
        // OR use state.bucket_seal_adapter (the concrete handle added in PR13).
        let entries = recall_hybrid(&state.bucket_seal_adapter, &input.content, None, 6).await;
        if let Some(block) = render_bucket_seal_recall(&entries, 1500) {
            delegate.append_memory_context(&format!("\n\n{block}"));
        }
    }
}
```

- Uses `state.bucket_seal_adapter` (the concrete `Arc<BucketSealAdapter>` PR13 added to AppState) — no trait downcast needed.
- `append_memory_context` (existing on the assembler) appends to the memory_ctx that `set_memory_context` seeds — so bucket_seal recall rides alongside the legacy recall in the same `<memory>` region of the prompt. (If `set_memory_context` hasn't been called yet at that site, `append_memory_context` creates it — verified safe per the assembler impl.)
- Best-effort: `recall_hybrid` swallows errors → empty → no block appended. Never blocks the turn.
- Gated on non-empty `input.content`.

### 3.5 No new AppState/IPC

`state.bucket_seal_adapter` (PR13) + `state.memory_adapters` (PR2+) already exist. No new fields, no new commands.

---

## 4. Data flow (after PR15)

```text
chat/agent send (tauri_commands)
  ├─ [existing] legacy MemoryRecallEngine(graph + memU) → memory_ctx → set_memory_context
  ├─ [existing] session-store + browser-task memory → appended
  └─ [PR15] recall_hybrid(bucket_seal, last_user_text):
        ├─ recall_semantic: embed(query) → cosine vs mem_tree_summaries.embedding → top-K summaries
        ├─ recall (FTS5): mem_tree_chunks_fts MATCH → chunk hits
        └─ merge/dedup/budget → render "## Relevant Memory (bucket-seal)" → append_memory_context
  → effective_system_prompt assembles all blocks → LLM turn
```

---

## 5. Error handling

| Failure | Behavior |
|---|---|
| Embedder absent / not 1024-dim | `recall_semantic` → Err; hybrid falls back to FTS-only; if FTS also empty → no block. The turn proceeds. |
| FTS recall error | hybrid uses semantic only. |
| Both error / empty | no bucket_seal block appended (legacy recall still injected). |
| Empty user text | recall skipped entirely. |

Consistent with the program-wide principle: **memory is best-effort and never blocks/breaks the turn.**

---

## 6. Testing

| Area | Tests |
|---|---|
| `recall_semantic` | seed summaries with known embeddings (via `set_summary_embedding`) + an `InertEmbedder` returning a fixed vector → assert cosine ordering + top-K cut + namespace filter. With `InertEmbedder` (zeros), cosine is 0 for all → assert it returns up-to-limit without panic (degenerate but safe). For real ordering, use a tiny fake embedder returning distinct vectors per text. ~3 tests. |
| `recall_hybrid` | semantic + FTS merge: dedup by id, summaries-first ordering, both-empty → empty, one-errs → other used. ~3 tests. |
| `render_bucket_seal_recall` | marker present, entries formatted, empty → None, budget truncation. ~3 tests. |
| Integration (light) | the wiring is in `tauri_commands` (Tauri State) — hard to unit-test without the harness; covered by the adapter/format unit tests + a manual verification note. The recall_hybrid + render fns are pure-enough to test directly. |

Hermetic — fake/inert embedder, in-memory `BucketSealStore`, no live models. ~9 new tests.

---

## 7. Scope boundaries

- **Additive only** — the legacy graph/memU/session recall injection is unchanged. PR15 appends a bucket_seal section.
- **bucket_seal only** — no gbrain/legacy auto-injection in the prompt path.
- **No new IPC, no new AppState fields** — reuses `bucket_seal_adapter` + existing assembler block API.
- **No change to BucketSealAdapter's trait `recall`** (FTS5 stays); `recall_semantic`/`recall_hybrid` are additive methods.
- **Summary embeddings are the semantic unit** — chunks have no embeddings; semantic recall is over summaries. Recent un-sealed chunks are reachable only via FTS (acceptable — they haven't accumulated into a summary yet).

---

## 8. File plan (preview — detailed in the implementation plan)

| File | New/Mod | Purpose |
|---|---|---|
| `memory_bucket_seal/adapter.rs` | mod | `recall_semantic` + `recall_hybrid` methods + tests |
| `memory_bucket_seal/tree_source/store.rs` | mod (maybe) | `list_summaries_with_embeddings` read helper (if not derivable from existing fns) |
| `agent/memory_recall_block.rs` | new | `render_bucket_seal_recall` + marker + tests |
| `agent/mod.rs` | mod | `pub mod memory_recall_block;` |
| `tauri_commands.rs` | mod | wire recall_hybrid + append at the chat/agent send sites |

Est. ~350 source + ~180 tests.

---

## 9. Open adaptation questions (resolved at implementation time)

1. **Summary read for semantic recall** — whether `tree_source::store` already exposes a "list all summaries with embeddings" path, or the adapter iterates `list_summaries_at_level` across `0..=max_level` per tree + `get_summary_embedding`. Pick the simplest correct read; add a focused helper if needed.
2. **Namespace → summary scoping** — a summary's namespace is its tree's `scope`. For `namespace=None` (the prompt path passes None — recall across all source/topic/global), no filter. Confirm the source-tree scope equals the MemoryAdapter namespace used at store time (PR9 used the namespace as the source-tree scope).
3. **Which send sites get the wiring** — the chat send (`~2171`) definitely; confirm the agent send path(s) (`~11365`, `~15188`) and whether a shared helper avoids 3× duplication. Prefer a small private fn `append_bucket_seal_recall(state, delegate, query).await` called at each site.
4. **Token estimator** — reuse the chars/4 helper from PR12's summariser or an existing util.
5. **Concrete-handle access** — use `state.bucket_seal_adapter` (PR13) directly; no downcast.

---

## 10. Success criteria

- A per-turn `## Relevant Memory (bucket-seal)` block appears in the system prompt when bucket_seal has relevant summaries/chunks for the user's query.
- Semantic recall embeds the query + cosine-ranks real summary embeddings (PR12) — embeddings are finally queried; a 1024-dim endpoint makes them rank meaningfully, an absent endpoint degrades to FTS-only.
- The legacy graph/memU/session recall injection is unchanged (additive).
- Best-effort: recall errors never block the turn.
- All existing tests stay green; ~9 new tests pass. CI hermetic.
- **阶段 4 closed**: the memory engine reads back into the live agent loop.
