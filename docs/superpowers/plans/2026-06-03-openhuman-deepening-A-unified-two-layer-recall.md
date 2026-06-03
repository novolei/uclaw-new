# openhuman Deepening · Slice A — Unified Two-Layer Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the agent's per-turn recall (`load_context`) surface the rich memory_graph layer — by upgrading `load_context` to semantic `recall_hybrid`, projecting reflection-extracted facts into bucket_seal (best-effort, mirroring 2b's `write_page` dual-write), ensuring EntityPages are recalled, and backfilling existing recallable nodes.

**Architecture:** Path 1 — single recall surface. memory_graph stays authoritative; bucket_seal is the one recall surface. Read side: `load_context` uses the concrete `BucketSealAdapter::recall_hybrid` (semantic+FTS). Write side: reflection facts dual-write a best-effort bucket_seal projection. Backfill: one-time marker-gated migration. Compiler + tests are the guard.

**Tech Stack:** Rust (rusqlite, async_trait, tokio), in-process ONNX embeddings (bge-small/384). No new deps.

**Key facts (recon, file:line):**
- **`load_context`** (`memory_adapter/router.rs:261-283`): takes `adapters: &HashMap<String, Arc<dyn MemoryAdapter>>`, `default_backend: &str`, `query`, `budget`, `extra: Vec<MemoryEntry>`. Loops `sources=[default_backend]`, calls `ad.recall(query, 6, RecallOpts::default())` — **trait `recall` = FTS-only, namespace=None**. Then `merge_dedupe_budget` (sort by score desc, dedup by content, char-budget) + `format_entries` (`<memory_context>` block). Called from `tauri_commands.rs:1138` (where `state` + `state.bucket_seal_adapter` are in scope).
- **`recall_hybrid`** (`memory_bucket_seal/adapter.rs`): `recall_hybrid(query, namespace: Option<&str>, max) -> Vec<MemoryEntry>` — `recall_semantic` (cosine over `mem_tree_summaries` embeddings, dim-enforced 384, scan-capped) THEN FTS backfill (`recall`). Concrete on `BucketSealAdapter`, NOT on the `MemoryAdapter` trait.
- **`recall_semantic` namespace semantics**: `if let Some(ns) = namespace { filter }` — **namespace=None scans ALL summaries with embeddings across all trees/scopes**. NOTE: semantic leg ranks SEALED summaries (a freshly-stored chunk is FTS-recallable immediately, semantically-recallable only after the async seal cascade embeds it).
- **`BucketSealAdapter::store`** (`adapter.rs`): runs `score_chunk` (`memory_bucket_seal/score/mod.rs`) and **DROPS chunks below `DROP_THRESHOLD=0.3`** (`if result.kept { persist }`). So projecting a terse-but-vetted fact via `store` risks silent drop → need a keep-forced path.
- **Reflection** (`memory_graph/reflection.rs`): `ReflectionOrchestrator` HOLDS `bucket_seal_adapter: Arc<BucketSealAdapter>` (`:331/:339`) and already calls `recall_hybrid` (`:507`). `persist_items_to_graph` (`:135`, `pub fn`) writes memory_graph (MemoryNode+Version+Route+keywords) for taxonomy profile/event/knowledge/behavior/skill/tool. `MemoryExtractor.extract()` (`memory_graph/extractor.rs`) produces the items.
- **Pages projection** (2b): `memory_adapter/pages.rs::put_page` → `adapter.store(PAGES_NAMESPACE, slug, serde_json::to_string(page), Core, None)`. `write_page` (`page_dual_write.rs`) calls it. Page JSON includes `body` (markdown).
- **Migration template**: `memory_adapter/pages_to_entitypage_migration.rs` (2a) + the `app.rs` boot spawn (marker-gated, idempotent, best-effort).
- **`MemoryEntry`** fields (`memory_adapter/`): `id`, `key`, `content`, `namespace: Option<String>`, `category`, `timestamp`, `session_id`, `score: Option<f64>`.

---

## Task 1: Read side — `load_context` uses `recall_hybrid` (semantic + FTS)

**Files:** `memory_adapter/router.rs` (`load_context` + tests), `tauri_commands.rs:1138` (call site).

- [ ] **Step 1: Add the concrete-adapter param.** Change `load_context`'s signature to accept the concrete bucket_seal adapter:
```rust
pub async fn load_context(
    adapters: &HashMap<String, std::sync::Arc<dyn MemoryAdapter>>,
    default_backend: &str,
    bucket_seal: Option<&std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>>,
    query: &str,
    budget: usize,
    extra: Vec<MemoryEntry>,
) -> String
```
- [ ] **Step 2: Use recall_hybrid for the bucket_seal backend.** Replace the `sources` loop body so that when `default_backend == "bucket_seal"` AND `bucket_seal` is `Some(bs)`, recall via the concrete hybrid (semantic+FTS, namespace=None spans all content); else fall back to the trait recall (legacy backends / tests):
```rust
let mut all = extra;
if default_backend == "bucket_seal" {
    if let Some(bs) = bucket_seal {
        let hits = bs.recall_hybrid(query, None, 6).await;   // semantic + FTS, all namespaces
        all.extend(hits);
    } else if let Some(ad) = adapters.get(default_backend) {
        match ad.recall(query, 6, RecallOpts::default()).await {
            Ok(mut h) => all.append(&mut h),
            Err(e) => tracing::debug!(backend = default_backend, error = %e, "load_context: recall failed; skipping"),
        }
    }
} else if let Some(ad) = adapters.get(default_backend) {
    match ad.recall(query, 6, RecallOpts::default()).await {
        Ok(mut h) => all.append(&mut h),
        Err(e) => tracing::debug!(backend = default_backend, error = %e, "load_context: recall failed; skipping"),
    }
}
format_entries(&merge_dedupe_budget(all, budget))
```
   (Confirm `recall_hybrid`'s exact return type — if it's `Vec<MemoryEntry>` use `extend`; if `anyhow::Result<Vec<_>>`, match + log on Err like the trait path. Read the signature.)
- [ ] **Step 3: Update the call site** (`tauri_commands.rs:1138`): pass `Some(&state.bucket_seal_adapter)` as the new `bucket_seal` arg. Update the stale comment at `:1129` ("+ gbrain") to note bucket_seal hybrid recall.
- [ ] **Step 4: Update load_context tests** (`router.rs` tests, ~:285+): existing tests call `load_context(...)` without the new param — add `None` for `bucket_seal` (they use stub legacy adapters → trait-recall path, behavior unchanged). Add ONE new test: build an in-memory `BucketSealAdapter` (mirror the `memory_bucket_seal` test fixtures), store a chunk whose content is semantically related but lexically different from the query, seal/embed it (or assert the FTS leg surfaces a lexical match if sealing is async in tests), call `load_context(&map, "bucket_seal", Some(&bs), query, 8000, vec![])` → the `<memory_context>` block contains the stored content. (If embedding/sealing in a unit test is heavy, assert the FTS leg surfaces a lexical-overlap fact — the semantic leg is covered by `memory_bucket_seal`'s own recall_hybrid tests; note this.)
- [ ] **Step 5: Build + clippy + test.** `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty); `cargo clippy --lib 2>&1 | grep "warning: "` (no new); `cargo test --lib memory_adapter::router 2>&1 | tail`.
- [ ] **Step 6: Commit.** `feat(memory): load_context uses bucket_seal recall_hybrid (semantic+FTS) instead of FTS-only trait recall (openhuman-A)`

---

## Task 2: Write side — recall projection helper + forced-keep store + reflection dual-write

**Files:** `memory_bucket_seal/adapter.rs` (forced-keep store), `memory_adapter/recall_projection.rs` (new helper), `memory_graph/reflection.rs` (wire projection).

- [ ] **Step 1: Forced-keep store on BucketSealAdapter.** Read `BucketSealAdapter::store` (the score-gated path). Add a sibling method that stores a chunk WITHOUT the `score_chunk` drop gate (authoritative projections are pre-vetted):
```rust
/// Store a pre-vetted authoritative chunk, bypassing the score-admission drop
/// gate (used for memory_graph→bucket_seal recall projections — the content is
/// already LLM-extracted + recall-gated, so it must not be silently dropped).
pub async fn store_kept(
    &self,
    namespace: &str,
    key: &str,
    content: &str,
    category: MemoryCategory,
    session_id: Option<&str>,
) -> anyhow::Result<()>
```
   Implement by mirroring `store` but forcing `kept = true` (skip the `result.kept` filter / the DROP_THRESHOLD check) — the chunk + entity index + (eventual) seal cascade run as normal. Confirm the exact internal seam by reading `store`.
- [ ] **Step 2: Projection helper.** Create `memory_adapter/recall_projection.rs`:
```rust
//! memory_graph → bucket_seal recall projection (openhuman-A). The rich layer
//! (memory_graph) is authoritative; this projects recallable facts into the
//! bucket_seal recall surface so `load_context`'s recall_hybrid surfaces them.
use std::sync::Arc;
use crate::memory_bucket_seal::BucketSealAdapter;
use crate::memory_adapter::MemoryCategory;

/// Namespace for projected memory_graph facts. recall_hybrid(query, None, n)
/// spans it (namespace=None scans all).
pub const RECALL_PROJECTION_NAMESPACE: &str = "graph_facts";

/// Project one recallable fact (idempotent by node_id key → re-projection
/// overwrites, no dup). Best-effort: logs + swallows (never blocks the
/// authoritative memory_graph write). `text` = the fact's version content.
pub async fn project_fact(
    adapter: &Arc<BucketSealAdapter>,
    node_id: &str,
    text: &str,
) {
    if let Err(e) = adapter
        .store_kept(RECALL_PROJECTION_NAMESPACE, node_id, text, MemoryCategory::Core, None)
        .await
    {
        tracing::warn!(node_id, error = %e, "recall_projection: project_fact failed (memory_graph authoritative ok)");
    }
}
```
   Add `pub mod recall_projection;` to `memory_adapter/mod.rs`.
- [ ] **Step 3: Wire into reflection.** In `memory_graph/reflection.rs`, after `persist_items_to_graph` runs in `ReflectionOrchestrator.reflect()`, project the **recallable kinds** (knowledge/event/profile) of the just-persisted items. Read `persist_items_to_graph` to get the persisted nodes' `(node_id, content, kind)` (return them if it doesn't already, or iterate the extracted items + their created node ids). For each item whose kind ∈ {knowledge (Reference/Curated), event (Episode), profile (UserProfile, non-boot)} call `crate::memory_adapter::recall_projection::project_fact(&self.bucket_seal_adapter, &node_id, &content).await`. EXCLUDE: boot-elevated nodes (already in every prompt), skill (skills facade), tool (tool_stats facade), behavior (personality). (If `persist_items_to_graph` is a free fn without the adapter, do the projection in the orchestrator method that owns `self.bucket_seal_adapter`, after the persist call returns the node ids.)
- [ ] **Step 4: Test.** In `recall_projection.rs` (or reflection tests): in-memory `BucketSealAdapter` fixture → `project_fact(&bs, "node-1", "Alice prefers terse answers")` → `bs.recall_hybrid("Alice answer style", None, 5)` (or the FTS leg `recall` with namespace=Some("graph_facts")) returns the projected text. Assert idempotency: projecting the same node_id twice → one entry (overwrite). If sealing/embedding is async-heavy in unit tests, assert via the FTS/`recall` leg over the `graph_facts` namespace.
- [ ] **Step 5: Build + clippy + test.** clean; `cargo test --lib memory_adapter::recall_projection memory_graph::reflection memory_bucket_seal 2>&1 | tail`.
- [ ] **Step 6: Commit.** `feat(memory): project reflection facts (knowledge/event/profile) into bucket_seal recall surface, best-effort (openhuman-A)`

---

## Task 3: EntityPage recall coverage

**Files:** `memory_adapter/pages.rs` (conditional body fix), test in `pages.rs` or `router.rs`.

- [ ] **Step 1: Verify pages surface.** Add a test: build an in-memory `BucketSealAdapter`, `pages::put_page(&adapter, &page)` (page with a markdown `body` containing a distinctive term), then `load_context(&map, "bucket_seal", Some(&bs), <term from body>, 8000, vec![])` → the block contains the page content. (Pages are stored as `serde_json::to_string(page)` chunks in the `pages` namespace; recall_hybrid namespace=None FTS leg should match the term inside the JSON.)
- [ ] **Step 2: If recall quality is poor (JSON-blob noise), fix the page projection.** ONLY if Step 1 shows the term isn't matched well: change `pages::put_page` to store the page via `BucketSealAdapter::store_kept` with the **markdown `body`** as the chunk `content` (recall-friendly text), keeping the structured JSON in a metadata field or a separate get-path. (The WikiView read path uses memory_graph EntityPages now — 2a — so the bucket_seal `pages` blob is recall-only; storing the body as content is safe. Confirm no reader depends on the JSON-blob shape of the `pages` chunk via `pages::get_page`.) If Step 1 passes, SKIP this step + note pages already recall fine.
- [ ] **Step 3: Build + clippy + test.** clean; `cargo test --lib memory_adapter::pages 2>&1 | tail`.
- [ ] **Step 4: Commit.** `test(memory): EntityPage recall coverage via load_context recall_hybrid (+ body-as-content if needed) (openhuman-A)`

---

## Task 4: Backfill — project existing recallable memory_graph nodes

**Files:** `memory_adapter/recall_projection_backfill.rs` (new, mirror `pages_to_entitypage_migration.rs`), `app.rs` (boot spawn).

- [ ] **Step 1: Backfill fn.** Create `recall_projection_backfill.rs` mirroring `pages_to_entitypage_migration.rs`: marker-gated (`__recall_projection_backfill_v1__` — store the marker in bucket_seal, e.g. via `get`/`store_kept` on a sentinel key, like the pages migration's marker), idempotent, best-effort. Logic: short-circuit if marker present; else iterate recallable memory_graph nodes (kinds knowledge/event/profile — use `MemoryGraphStore` list/query for active versions of those kinds), for each call `recall_projection::project_fact(&adapter, &node_id, &content)`; set marker at end.
```rust
pub async fn backfill_recall_projections(
    store: &Arc<crate::memory_graph::store::MemoryGraphStore>,
    adapter: &Arc<crate::memory_bucket_seal::BucketSealAdapter>,
) -> anyhow::Result<usize>   // returns count projected
```
   (Read `pages_to_entitypage_migration.rs` for the marker pattern + how it lists nodes. Use the MemoryGraphStore method that lists nodes by kind + active version content — find it; if none exists, the recallable-kinds query is a small read.)
- [ ] **Step 2: Boot spawn.** In `app.rs`, near the existing `pages_to_entitypage` migration spawn, add a fire-and-forget `tauri::async_runtime::spawn` calling `backfill_recall_projections(&memory_graph_store, &bucket_seal_adapter)`; log the count; never block boot (best-effort, like the pages migration).
- [ ] **Step 3: Test.** In-memory MemoryGraphStore + BucketSealAdapter: seed 2 recallable nodes (knowledge/event) + 1 excluded (boot) → `backfill_recall_projections` returns 2, the 2 are recallable via `recall_hybrid`/`recall` over `graph_facts`, boot one is NOT; re-run → returns 0 (marker short-circuit).
- [ ] **Step 4: Build + clippy + test.** clean; `cargo test --lib memory_adapter::recall_projection_backfill 2>&1 | tail`.
- [ ] **Step 5: Commit.** `feat(memory): one-time backfill of existing recallable memory_graph nodes into bucket_seal (openhuman-A)`

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1:** `cargo build` + `cargo clippy --lib` clean; `cargo test --lib memory_adapter memory_graph::reflection memory_bucket_seal 2>&1 | grep "test result:"` green.
- [ ] **Step 2: Integration sanity (manual or test):** simulate reflection persisting a knowledge fact → confirm a subsequent `load_context` with a related query surfaces it (the read+write paths connect). If feasible as a `#[tokio::test]`, add it; else document the manual soak.
- [ ] **Step 3: Gates:** `grep -n "recall_hybrid" src/memory_adapter/router.rs` (load_context now uses it); `grep -rn "RECALL_PROJECTION_NAMESPACE\|project_fact\|store_kept\|backfill_recall_projections" src/` (all wired). load_context no longer FTS-only for bucket_seal.
- [ ] **Step 4: Ship** — push → PR (Commits table T1-T4) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 5: Post-merge soak (manual):** have a substantive conversation that teaches the agent a fact (e.g. a preference) → in a LATER turn, ask something where that fact is relevant → confirm the fact appears in the agent's `<memory_context>` (reflection→projection→recall round-trip works). Confirm WikiView pages + chat episodic still recall.

---

## Self-Review

- **Spec coverage:** §1 read-side recall_hybrid → T1; §2 write-side projection (+ forced-keep) → T2; §3 EntityPage coverage → T3; §5 backfill → T4; verify → T5. ✓ (§4 namespace strategy resolved = namespace=None, baked into T1.)
- **Ordering compiles:** read-side upgrade (T1, additive param+None default) → write-side projection (T2) → page coverage (T3) → backfill (T4). Each builds; recall surfaces projections once both T1+T2 land. ✓
- **Type consistency:** `load_context(..., bucket_seal: Option<&Arc<BucketSealAdapter>>, ...)`; `store_kept(ns,key,content,cat,sid)`; `project_fact(&Arc<BucketSealAdapter>, node_id, text)`; `RECALL_PROJECTION_NAMESPACE="graph_facts"`; `backfill_recall_projections(store, adapter)->usize`. Used consistently. ✓
- **No placeholders:** real signatures + file:line + resolved seams (recall_hybrid concrete-adapter param; namespace=None spans all; store_kept bypasses the drop gate; reflection already holds the adapter). The 2 conditional points (T1 Step 4 semantic-vs-FTS test if sealing is async-heavy; T3 Step 2 body-fix only if JSON recall is poor) are explicit conditionals, not vague. ✓
- **Finish-line:** after A, `load_context` semantically recalls bucket_seal incl. projected reflection facts + EntityPages; reflection facts dual-write; existing nodes backfilled. Unblocks B/C/G. ✓
