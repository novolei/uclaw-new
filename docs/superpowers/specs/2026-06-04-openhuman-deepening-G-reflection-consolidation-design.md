# openhuman Deepening · Slice G — Reflection Consolidation (dedup/merge/link) Design

**Date:** 2026-06-04
**Status:** Design (recon done; approved → spec → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice G — stop the reflection engine from minting a brand-new isolated node for every extracted fact. Add **dedup → merge → light link** so repeated/near-duplicate facts fold into existing nodes (with version history) and co-extracted facts connect into the graph. **G2 (multi-hop / spreading-activation recall) is already implemented — out of scope.**

## Problem

`memory_graph/reflection.rs::persist_items_to_graph` (runs after every chat turn) loops the extracted items and **unconditionally mints fresh UUIDs** per item (`let node_id = uuid::Uuid::new_v4()` with NO dedup/merge check), then `store.create_node`. Every reflection output → a new isolated node. Consequences: (a) **bloat** — the same fact ("user likes X") re-stated across turns creates N duplicate nodes; (b) **fragmentation** — facts are never linked, so the graph stays sparse and the (already-working) multi-hop recall has few edges to traverse among reflection facts. By contrast `skill_parser` (Slice-E era) and importance/spaced-rep all dedup before write — reflection is the lone create-only path.

**Recon correction:** the program tracker listed G2 as "spreading-activation / multi-hop recall vs current 1-hop `edges::neighbors`". That assumption is stale: memory_graph recall ALREADY does multi-hop spreading activation — `recall.rs::layer_expanded` → `store.rs::graph_propagation_search` (BFS, default `max_depth=2`, decay `0.6`, per-relation weights, priority boost). The "1-hop `edges::neighbors`" is the bucket_seal KV facade (tool co-occurrence, removed in Slice E), not the recall path. So **G = G1 only** (dedup/merge/link); G2 needs no work (an optional `memory_edges.weight` column is deferred — current BFS uses hardcoded relation weights + `priority`).

## Decision (approved 2026-06-04)

- **G1 only.** dedup + merge + light link in reflection. G2 confirmed already-implemented; `memory_edges.weight` column polish deferred.
- **Reuse the proven dedup machinery** from `skill_parser`: D1 normalize-title exact match + D2 bigram-Jaccard fuzzy (CJK-aware) + upgrade-existing (supersede version, reuse node id). Extract these helpers to a **shared module** so reflection and skills share one implementation.
- **Light link**: facts co-extracted in one reflect() turn are related pairwise via `memory_edges` `relation_kind='co_extracted'`, so isolated reflection facts join the graph (and the existing multi-hop recall can traverse them).
- **Dedup always-on** (no config gate) — it is strictly-better and the merge keeps version history (auditable; the superseded version is retained, not deleted). A kill-switch can be added later if needed.
- **No migration** (reuses memory_nodes/versions/keywords/edges).

## Design

### §1 Shared dedup module — `memory_graph/text_dedup.rs`
Extract from `skill_parser.rs` (make `pub`, move, re-export from skill_parser to avoid churn at its call sites): `normalize_title_for_dedup`, `title_bigrams`, `word_bigrams`, `jaccard_similarity`, `cjk_char_ratio`, `tokenize_mixed`, `is_cjk_char`, `FUZZY_DEDUP_THRESHOLD`. `skill_parser` switches to `use crate::memory_graph::text_dedup::*`. Pure functions, no behavior change — covered by skill_parser's existing tests + new unit tests in the shared module.

### §2 Dedup-before-create in `persist_items_to_graph`
For each extracted item, BEFORE minting a new node, dedup against existing reflection-fact nodes of the **same kind** in the space (kinds: `Reference`/`Episode`/`UserProfile`/`Directive` per `map_memu_type_to_kind`; **exclude `Procedure`** — that's skills, dedup'd separately, to avoid cross-contaminating the two stores). The node's `title` (the fact summary) is the dedup key.

1. **D1 exact:** `normalize_title_for_dedup(summary)` → `store.find_fact_by_normalized_title(space, kinds, normalized)` (new store helper, mirrors `find_learned_skill_by_normalized_title` but filters to the reflection kinds, NOT Procedure). Hit → `upgrade_existing_fact`.
2. **D2 fuzzy:** if no D1 hit and `normalized.chars().count() >= 4`: load candidate reflection facts (`store.list_recent_facts_by_kinds(space, kinds, 500)` — new/existing helper), bigram-Jaccard over normalized titles with the CJK-aware threshold (0.65 CJK / 0.75 ASCII). Best match ≥ threshold → `upgrade_existing_fact`.
3. **else** → the existing create path (new UUID node + version + route + keywords).

`PersistedFact.node_id` returned is the existing node's id on a merge (so the downstream `project_fact` recall projection upserts by content-hash — unchanged content no-ops, changed content updates).

### §3 `upgrade_existing_fact` (merge)
Mirror `upgrade_existing_skill`:
- Deprecate the node's current active version (`store.deprecate_version`).
- Create a new active version with the new fact content (supersession chain preserved → history/auditability).
- Bump `metadata.reinforced_count` (new counter; default 0 → 1, 2, …) and refresh `updated_at`.
- Merge keywords additively (new keywords inserted, existing kept).
Returns the existing node id.

### §4 Light link (co_extracted edges)
After the per-item dedup/persist loop in `reflect()`, collect the resulting node ids for the turn (both newly-created and merged-into). If ≥2, relate them pairwise via `store.create_edge` with `relation_kind='co_extracted'` (idempotent: skip if an edge already exists between the pair — a store check or rely on a dedup helper). This connects facts that surfaced together so the existing `graph_propagation_search` BFS can reach them. Bounded: if a turn yields many facts, cap pairs (e.g. only link the first K=5 to avoid O(n²) blowup) — plan pins K.

## Data flow (after G)

```
chat turn → reflect() → extractor → items
  per item: D1 normalize-title exact / D2 bigram-Jaccard fuzzy (same-kind, non-Procedure)
            hit → upgrade_existing_fact (supersede version, reinforced_count++, merge keywords) [reuse node id]
            miss → create new node (as today)
  after loop: relate the turn's fact nodes pairwise (memory_edges 'co_extracted', capped, idempotent)
  → project_fact (recall projection; content-hash idempotent — unchanged)
later recall: graph_propagation_search (ALREADY multi-hop) now traverses the co_extracted links
```

## Out of scope

G2 spreading-activation / multi-hop recall (already implemented); `memory_edges.weight` column (deferred — BFS uses relation weights + priority today); cross-store dedup with Procedure/skills (reflection dedups its own kinds only); a config gate (always-on); semantic (embedding-cosine) dedup beyond bigram-Jaccard (future — the bigram approach matches skill_parser's proven bar).

## Error handling

Dedup is best-effort within reflection (already a background spawn): a dedup-query error logs + falls through to the create path (never lose a fact). `upgrade_existing_fact` errors log + fall back to create. Link creation is best-effort (per-pair error logs + continues). All reads null-safe. No migration.

## Testing

1. **Shared helpers**: `text_dedup` unit tests (normalize, bigram, jaccard, cjk ratio) — port a few from skill_parser; skill_parser's existing tests still pass against the re-exported helpers.
2. **D1 exact dedup**: persist a fact "User likes fish."; persist "user likes fish" (case/punct variant) → ONE node, `reinforced_count==1`, two versions (one deprecated).
3. **D2 fuzzy dedup**: persist "User prefers Rust for backend"; persist "User prefers Rust for the backend" (≥ threshold) → merged into one node.
4. **No false-merge**: two genuinely different facts (low similarity) → two nodes.
5. **Kind isolation**: a Reference fact and a Procedure (skill) with identical title → NOT merged (different stores).
6. **Link**: a turn yielding 3 facts → 3 `co_extracted` edges (pairwise, capped); re-running the same turn doesn't duplicate edges.
7. **G2 still works**: `graph_propagation_search` from a seed reaches a co_extracted-linked fact (multi-hop intact).
8. `cargo build`/clippy clean; `cargo test --lib` for `memory_graph::reflection`, `memory_graph::text_dedup`, `proactive::skill_parser` (shared-helper refactor), + broad dependent run.

## Scope / files

| File | Change |
|---|---|
| `memory_graph/text_dedup.rs` (new) | extract shared dedup helpers from skill_parser |
| `proactive/skill_parser.rs` | `use` the shared helpers (remove the local copies) |
| `memory_graph/reflection.rs` | dedup-before-create in `persist_items_to_graph`; `upgrade_existing_fact`; pairwise `co_extracted` linking in `reflect()` |
| `memory_graph/store.rs` | `find_fact_by_normalized_title` / `list_recent_facts_by_kinds` (reflection-kind variants) + edge-exists check if needed |

## Risk

Low–Med. Pure logic reusing a proven pattern; no schema, no config, no new external surface. Main risks: (1) **kind scoping** — dedup must filter to reflection kinds and EXCLUDE Procedure so skills and facts don't cross-merge (pinned in §2); (2) **false-merge** from too-loose fuzzy threshold — reuse skill_parser's tuned thresholds + a no-false-merge test; (3) the shared-helper extraction must not change skill_parser behavior (re-export + run its tests); (4) O(n²) linking on large turns (capped); (5) merge must preserve history (supersede, not delete) for reversibility/audit. Bisectable: extract helpers → dedup+merge → linking → verify. After G, reflection consolidates repeated facts into single versioned nodes and connects co-occurring facts, so the memory graph densifies and the existing multi-hop recall has real structure to traverse.
