# openhuman Deepening · Slice A — Unified Two-Layer Recall Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** "openhuman rich-memory deepening" — a multi-slice program to wire up + complete the brain-like memory machinery that the 2026-05-31 openhuman-primary convergence ported but left dormant/split/unwired. **Slice A is first** (highest functional value, no deps, unblocks B/G). Sibling slices (own specs later): B reinforcement+recency recall, C importance/decay loop, D spaced-repetition, E tool co-occurrence graph, F skill rich-store unification, G consolidation deepening.

## Problem

uClaw's per-turn memory injection (`memory_adapter/router.rs::load_context`) recalls **only the bucket_seal layer, and only via the trait `recall` method (FTS-only, `RecallOpts::default()` ⇒ namespace=None)** — no semantic recall at all. Two consequences:

1. **The rich layer is invisible to agent recall.** `memory_graph` holds the agent's richest memories — reflection-extracted facts (profile/event/knowledge from `MemoryExtractor`), EntityPages, proactive scenario outputs. The reflection pipeline (`memory_graph/reflection.rs::persist_items_to_graph`) writes **only to memory_graph**, never to bucket_seal, so reflection-derived facts never reach recall. The agent cannot recall what it learned about the user in prior sessions.
2. **Recall is FTS-only, not semantic.** `load_context` uses the FTS-only trait `recall` (`router.rs:~168`), not the concrete `BucketSealAdapter::recall_hybrid` (semantic cosine over summary embeddings + FTS backfill). Semantically-related-but-lexically-different memories are missed.

EntityPages are a partial exception: 2b's `write_page` already projects them into bucket_seal's `pages` namespace (`memory_adapter/pages.rs::put_page` → `adapter.store(PAGES_NAMESPACE, …)`), but `load_context`'s namespace=None FTS-only recall doesn't reliably surface them (JSON-blob chunks, no semantic leg).

This is the gating slice: importance scoring (C), reinforcement (B), and consolidation (G) only deliver value once the rich layer is actually recalled.

## Decision (approved 2026-06-03)

- **Path 1 — single recall surface / project into bucket_seal.** memory_graph stays the rich/authoritative source; bucket_seal is the one recall surface (aligned with the two-layer ADR `2026-06-01-memory-two-layer-terminal-state.md`; do NOT re-introduce a second recall path — 2d just removed the gbrain leg).
- **Scope — focused:** project reflection-extracted **recallable facts** (knowledge/event/profile) into bucket_seal (mirroring 2b's `write_page` two-layer dual-write) + upgrade `load_context` to `recall_hybrid` (semantic+FTS) + fix EntityPage recall coverage + one-time backfill of existing recallable nodes. NOT in this slice: proactive scenario outputs (broader), all-node uniform projection (broadest), skills/tools (have facades), boot (already in every prompt).

## Design

### §1 Read side — `load_context` → `recall_hybrid` (semantic + FTS)

`load_context` (`router.rs`) currently calls the trait `ad.recall(query, limit, RecallOpts::default())` (FTS-only). Upgrade the bucket_seal path to use the concrete `BucketSealAdapter::recall_hybrid(query, namespace, max)` (semantic cosine over summary embeddings, dim-enforced 384, + FTS backfill). Because `recall_hybrid` is concrete (not on the `MemoryAdapter` trait — the FTS-only-trait gotcha), the plan resolves the seam: either (a) special-case the `bucket_seal` backend in `load_context` to downcast/hold the concrete `Arc<BucketSealAdapter>` (precedent: Step 1b / 3b-2 + the `memory_query` tool hold the concrete adapter), or (b) add a `recall_hybrid` default method to the trait. **Recommendation: (a)** — `load_context` already special-cases the default backend; holding `state.bucket_seal_adapter` (concrete) for the hybrid call is the established pattern and avoids widening the trait. Namespace coverage: the call must surface the projected reflection facts + the `pages` projection (§2/§3) — the plan pins the namespace strategy (a single recall namespace the projections share, vs a small fixed set of namespaces recalled in sequence; see §4).

### §2 Write side — reflection facts dual-write a bucket_seal projection

`memory_graph/reflection.rs::persist_items_to_graph` (called by `ReflectionOrchestrator.reflect()` after each chat turn; `MemoryExtractor.extract()` taxonomy = profile/event/knowledge/behavior/skill/tool) writes only memory_graph. Add a **best-effort bucket_seal projection** for the recallable kinds, mirroring 2b's `write_page`/`shadow_write_page` posture (memory_graph write is authoritative + propagates; the bucket_seal projection logs+swallows on failure, never blocks the graph write):

- **Recallable kinds projected:** knowledge (Reference/Curated), event (Episode), profile (UserProfile — non-boot). **Excluded:** boot (Identity/Value/Directive elevated — already injected into every system prompt; projecting would double-inject), skill (→ `skills` facade, separate `skill_search` recall), tool (→ `tool_stats` facade), behavior (personality — not per-turn recall content).
- **Projection form:** write each projected fact as a **first-class bucket_seal chunk** via `BucketSealAdapter.store(namespace, key, content, category, session_id)` so it flows through the normal seal/embedding/FTS path and is recalled by `recall_hybrid` like any chunk. The `content` = the fact's text (the version content); `key` = the memory_graph node id (idempotent — re-projection overwrites, no dup). Category/namespace per §4.
- **Score-gate bypass for vetted facts:** reflection facts are already LLM-vetted (passed the extractor + the recall-before-memorize score≥0.9 gate). bucket_seal's write-time score admission (`score/mod.rs`, `DROP_THRESHOLD=0.3`) could silently drop a terse-but-important fact. The plan decides: project with a forced-keep signal (e.g. an engagement/importance tag that lands it in DEFINITE_KEEP) OR via a store path that bypasses the admission gate. **Recommendation:** bypass the drop gate for reflection-fact projections (they're authoritative, not raw ingest) — confirm the exact store seam in the plan.

### §3 EntityPage recall coverage

EntityPages already project to bucket_seal `pages` (2b `write_page`). The gap is recall coverage, not projection. The §1 `recall_hybrid` upgrade + §4 namespace strategy must surface the `pages` namespace. If pages-as-JSON-blob-chunks recall poorly (the stored content is `serde_json::to_string(page)`, not the raw markdown body), the plan adjusts the page projection to store a recall-friendly text body (the markdown) rather than/alongside the JSON — confirm whether `pages::put_page` content needs a text-body field for FTS/embedding quality.

### §4 Namespace strategy (impl-confirm, resolved in plan)

`recall_hybrid(query, namespace: Option<&str>, max)` takes ONE namespace. The projections live in namespace(s): reflection facts (§2) + pages (§3). Options the plan pins:
- **(a) One unified recall namespace** (e.g. `"recall"` / the default chunk namespace): all recallable projections (reflection facts + pages) land there; `load_context` does one `recall_hybrid(query, Some("recall"), n)`. Simplest single call.
- **(b) namespace=None hybrid** (scan all): if `recall_semantic` with namespace=None covers all summary embeddings regardless of source namespace, one call surfaces everything (pages + facts + chat episodic). Verify `recall_semantic`/`recall` namespace=None semantics.
- **(c) Fixed small set** recalled in sequence (facts + pages + episodic), merged/budgeted.
**Recommendation:** (b) if namespace=None hybrid genuinely spans all content (most aligned with "one surface"); else (a). The plan confirms `recall_semantic` namespace=None behavior + picks.

### §5 Backfill — one-time projection of existing recallable nodes

Existing reflection/knowledge nodes in memory_graph (written before this slice) have no bucket_seal projection. Add a **marker-gated, idempotent, in-process boot migration** (mirror `pages_to_entitypage_migration.rs` / the 2a pages migration): iterate recallable memory_graph nodes (knowledge/event/profile kinds, active versions), project each into bucket_seal via the §2 projection if not already present (idempotent by node-id key). Best-effort, never blocks boot. Marker (e.g. `__recall_projection_backfill_v1__`) so it runs once.

## Data flow (after A)

```
chat turn ─→ ReflectionOrchestrator ─→ MemoryExtractor.extract()
   └─→ persist_items_to_graph()
         ├─→ memory_graph MemoryNode+Version           (authoritative, unchanged)
         └─→ [NEW] project recallable kinds ─→ bucket_seal chunk   (best-effort recall projection)
next turn ─→ load_context ─→ [CHANGED] recall_hybrid (semantic+FTS over bucket_seal)
         └─→ now surfaces: reflection facts + EntityPages + chat episodic   (gap closed)
boot (once) ─→ [NEW] backfill: existing recallable memory_graph nodes ─→ bucket_seal projections
```

## Out of scope (sibling slices / later)

Reinforcement on access + recency/salience weighting in ranking (B); importance/decay/archival loop (C); spaced repetition (D); tool co-occurrence graph (E); skill rich-store unification (F); consolidation dedup/merge/link + spreading activation (G); proactive scenario output projection + all-node uniform projection (broader scope). EntityPage authoring/UI unchanged.

## Error handling

memory_graph write = authoritative (propagates). bucket_seal projection = best-effort (log + swallow; a failed projection never blocks the graph write or the turn) — mirrors 2b `write_page`. `load_context` recall failure already logs + skips (`router.rs:~168`); the `recall_hybrid` upgrade keeps that posture.

## Testing

1. **Projection unit:** project a knowledge fact → `recall_hybrid(query, ns, n)` returns it (in-memory bucket_seal fixture, like the page_dual_write tests).
2. **load_context recall_hybrid:** with a seeded bucket_seal, `load_context` surfaces a semantically-related fact that FTS-only would miss (proves the semantic upgrade).
3. **Reflection → recall integration:** a chat turn whose extracted fact, on the NEXT turn, appears in `load_context`'s `<memory_context>` block.
4. **EntityPage recall:** a `write_page`'d EntityPage is surfaced by `load_context` (coverage fix).
5. **Backfill:** seed recallable memory_graph nodes (no projection) → run the migration → they're in bucket_seal + recallable; re-run = no-op (marker).
6. `cargo build`/clippy clean; `cargo test --lib memory_adapter memory_graph::reflection memory_bucket_seal` green.

## Scope / files

| File | Change |
|---|---|
| `memory_adapter/router.rs` (`load_context`) | trait `recall` (FTS-only) → concrete `BucketSealAdapter::recall_hybrid` (semantic+FTS); namespace strategy (§4) |
| `memory_graph/reflection.rs` (`persist_items_to_graph`) | + best-effort bucket_seal projection for recallable kinds (knowledge/event/profile) |
| `memory_adapter/recall_projection.rs` (new, or in pages.rs/reflection.rs) | the projection helper (node/fact → bucket_seal chunk, idempotent by node-id, score-gate bypass) |
| `memory_adapter/pages.rs` (maybe) | page projection stores a recall-friendly text body if JSON-blob recalls poorly (§3) |
| backfill migration (new, mirror `pages_to_entitypage_migration.rs`) + `app.rs` boot spawn | one-time marker-gated projection of existing recallable nodes |

## Risk

Med. The read-side `recall_hybrid` upgrade is the highest-leverage change (also fixes the latent "no semantic recall" bug) — its risk is the concrete-adapter seam in `load_context` (precedented). The write-side projection mirrors 2b's proven `write_page` two-layer pattern. Backfill mirrors 2a's proven migration. Main impl-confirm points (flagged, resolved in plan): the recall_hybrid seam in load_context (§1), the namespace strategy (§4), the score-gate bypass (§2), and pages recall-body (§3). Bisectable: read-side upgrade → write-side projection → backfill → verify. After A, the agent recalls its rich memories; B/C/G become worth doing.
