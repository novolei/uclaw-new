# openhuman Deepening · Slice B — Recency + Reinforcement Recall Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice B — make recall brain-like by weighting ranking with **recency** + **reinforcement (hotness)**, and adding the openhuman retrieval-feedback loop (recalled → hotter). Builds on A (load_context → recall_hybrid). **B2 (importance/salience in ranking) is DEFERRED** — see Decision.

## Problem

After Slice A, `load_context` ranks via `BucketSealAdapter::recall_hybrid` = **raw cosine** (semantic leg over summary embeddings) + FTS backfill, sorted by score desc (`merge_dedupe_budget`, `router.rs:214`). Two brain-like gaps:

1. **No recency weighting.** A 6-month-old summary and a yesterday summary with equal cosine rank equally. Memory should favor recent.
2. **No reinforcement / hotness.** Recall has zero side effects (`recall_semantic`/`recall_hybrid` are read-only; the only reinforcement-on-access precedent is the `skill_search` tool bumping `usage_count`). bucket_seal has NO access-count / hotness / last-recalled column on `mem_tree_summaries`. So a frequently-useful memory never rises; the openhuman "query_hits → hotness" retrieval-feedback loop is unported.

## Decision (approved 2026-06-03)

- **Scope = B1 (recency) + B3 (reinforcement/hotness).** Both are the "fresh + frequently-used memories rank higher" story.
- **B2 (importance/salience in ranking) DEFERRED.** Real architectural snag: `recall_semantic` ranks **summaries**, but Slice-C importance is per-**node_id**, and summaries over the `graph_facts` source tree carry no per-summary node_id link (only the FTS/chunk leg has `key=node_id`); + importance lives in a separate DB (cross-DB Rust join). And importance's primary payoff (archival) already shipped in C. Revisit as a later slice.
- **Recency lives inside `recall_semantic`** (affects all recall_hybrid callers — recency is a universal ranking improvement). **Reinforcement write-back lives in `load_context`** (NOT inside recall_hybrid) — only the agent's actual context-load reinforces; internal recall_hybrid callers (e.g. reflection's recall-before-memorize dedup gate) must NOT reinforce.

## Design

### §1 B1 — recency-decay weighting (no schema change)
In `recall_semantic` (`memory_bucket_seal/adapter.rs:~373`), after computing `cos`, multiply a recency-decay factor before pushing the scored entry. The summary row already carries `sealed_at_ms`, so age is free:
```
age_days = (now_ms - summary.sealed_at_ms) / 86_400_000
recency  = exp(-(age_days / recency_half_life_days))     // 1.0 fresh → →0 old; mirror recall.rs::time_decay_score
score    = cos * recency
```
(Use the `recall.rs::time_decay_score` shape — confirm exp vs Gaussian; pick exp-decay for a gentle long tail, or match recall.rs's Gaussian for consistency — the plan pins it.) `now_ms` is passed into / available in recall_semantic. No new columns. Affects every recall_hybrid caller (recency is universally desirable).

### §2 B3 — hotness columns (migration V58)
`mem_tree_summaries` gains two columns (ALTER, mirror V57's `archived_at` pattern; **plan reconfirms the next-free V-number against `db/migrations.rs` + open PRs — spec assumes V58**):
```sql
ALTER TABLE mem_tree_summaries ADD COLUMN recall_hit_count INTEGER NOT NULL DEFAULT 0;
ALTER TABLE mem_tree_summaries ADD COLUMN last_recalled_at_ms INTEGER;
```
(In the bucket_seal `chunks.db` schema — find its migration/`ensure`/`open` path in `memory_bucket_seal/store.rs`; bucket_seal may have its OWN schema-init (not the main `db/migrations.rs` runner) — apply the ALTERs there, idempotently. The plan pins where bucket_seal's schema is created + how it versions.)

### §3 B3 — hotness in ranking
`recall_semantic` already loads each summary row in its scan → read `recall_hit_count` from the same row (free) → multiply a log-scaled hotness factor into the score alongside recency:
```
final = cos
      * exp(-(age_days / recency_half_life_days))            // §1 recency
      * (1.0 + hotness_weight * ln(1.0 + recall_hit_count))   // §3 hotness
```
Log-scale + bounded `hotness_weight` keep a hot memory from dominating; recency counterbalances (old hot memories still decay).

### §4 B3 — reinforcement write-back (`load_context` seam)
- New `BucketSealAdapter::reinforce_recalled(&self, summary_ids: &[String], now_ms: i64) -> anyhow::Result<()>`: `UPDATE mem_tree_summaries SET recall_hit_count = recall_hit_count + 1, last_recalled_at_ms = ?now WHERE id IN (<ids>)`. Best-effort.
- Call it from `load_context` (`router.rs`) AFTER `recall_hybrid` returns, fire-and-forget: collect the summary ids of the SEMANTIC-leg entries (the ranking unit; FTS-leg chunks have no hotness column — out of scope) + `bucket_seal.reinforce_recalled(ids, now)`. load_context already holds the concrete `Arc<BucketSealAdapter>` (Slice A). **Only load_context reinforces** — recall_hybrid stays pure-read so dedup/other internal callers don't inflate hotness.
- Linkage: the semantic-leg `MemoryEntry.id` = the summary id (confirm `recall_semantic` sets `MemoryEntry.id` to the summary's id so reinforce can target it). If load_context can't distinguish semantic vs FTS entries by the time it has the Vec, recall_hybrid returns enough to identify summary ids (the plan pins how — e.g. semantic entries' ids are summary ids that exist in mem_tree_summaries; reinforce's `WHERE id IN` naturally no-ops on FTS chunk ids that aren't summary ids).

### §5 Runaway mitigation
The recall→hotter→recalled-more feedback is bounded by: (a) `ln(1+hits)` log-scale, (b) a small bounded `hotness_weight` (default ~0.3), (c) recency decay (a stale-but-hot memory still sinks). Time-decay of `recall_hit_count` itself is deferred (D/future). Note the risk + these mitigations in code comments.

### §6 Config (`MemoryOsRuntimeConfig` + `memubot_config.rs`)
- `recall_recency_half_life_days: f64` (default `30.0`) — recency decay rate.
- `recall_hotness_weight: f64` (default `0.3`; `0.0` disables hotness ranking).
- `recall_reinforcement_enabled: bool` (default `true`) — gates the write-back (recency ranking is always on; reinforcement is gateable). 
Mirror the `importance_decay_*` config pattern (field + serde default + test + MemoryOsRuntimeConfig threading).

## Data flow (after B)

```
load_context → recall_hybrid → recall_semantic:
    score = cosine * recency_decay(sealed_at) * (1 + w·ln(1+recall_hit_count))   // B1+B3 ranking
  → merge_dedupe_budget (score desc) → <memory_context>
load_context (after recall, if reinforcement_enabled): bucket_seal.reinforce_recalled(semantic summary_ids, now)  // B3 feedback
    → recall_hit_count++ , last_recalled_at_ms=now   (only the agent-context path; dedup/internal recall_hybrid does NOT reinforce)
```

## Out of scope

B2 importance/salience in ranking (summary↔node_id snag; later slice); time-decay of recall_hit_count (D/future); FTS-leg hotness (semantic leg only); spaced-repetition (D); the legacy `recall.rs` path (off the hot path since A's `unified_load_context_enabled`).

## Error handling

`reinforce_recalled` is best-effort: fire-and-forget from load_context, errors log+swallow, NEVER block the turn (mirror Slice A/2b projection posture). recency/hotness ranking is read-only + total-order-safe (a missing/0 hit_count → factor 1.0; a null sealed_at → treat age 0 / factor 1.0, no panic). Migration V58 additive (defaulted columns).

## Testing

1. **Recency**: two summaries, equal cosine, different `sealed_at_ms` → the fresher ranks higher in recall_hybrid output (deterministic `now_ms`).
2. **Hotness**: two summaries, equal cosine + sealed_at, different `recall_hit_count` → the hotter ranks higher.
3. **reinforce_recalled**: bumps `recall_hit_count` + sets `last_recalled_at_ms` for the given ids; no-ops on unknown ids.
4. **load_context reinforces, dedup does not**: load_context (reinforcement_enabled) → the recalled summaries' hit_count bumped; a direct recall_hybrid call (simulating reflection's dedup gate) → hit_count unchanged.
5. **Migration V58**: fresh bucket_seal db has the 2 columns defaulting 0/NULL.
6. `cargo build`/clippy clean; `cargo test --lib memory_bucket_seal memory_adapter::router 2>&1` green; the broad dependent-module run (the Slice-C lesson: any fixture building bucket_seal without V58 + reading the new columns) green.

## Scope / files

| File | Change |
|---|---|
| `memory_bucket_seal/store.rs` (or its schema-init) | V58 ALTERs: `recall_hit_count` + `last_recalled_at_ms` on `mem_tree_summaries` (idempotent, in bucket_seal's schema path) |
| `memory_bucket_seal/adapter.rs` | `recall_semantic` recency + hotness factors in scoring; `reinforce_recalled(summary_ids, now)` |
| `memory_adapter/router.rs` (`load_context`) | after recall_hybrid, fire-and-forget `reinforce_recalled` (gated) on the semantic summary ids |
| `memubot_config.rs` (+ MemoryOsRuntimeConfig) | `recall_recency_half_life_days` / `recall_hotness_weight` / `recall_reinforcement_enabled` |

## Risk

Med. B1 recency is self-contained (sealed_at exists, no schema). B3 needs a small migration + a fire-and-forget write-back + a same-row read in ranking. Main risks: (1) the recall→hotness feedback runaway — mitigated by log-scale + bounded weight + recency counterbalance; (2) the Slice-C fixture lesson — adding columns that ranking reads can break bucket_seal test fixtures that don't apply V58, so the plan routes fixtures through the canonical bucket_seal schema-init + the verify runs the broad dependent suite; (3) reinforce must be load_context-only (not recall_hybrid-internal) so dedup checks don't inflate hotness — pinned in §4. Bisectable: migration → recency ranking → hotness column+ranking → reinforce write-back+load_context wiring → config → verify. After B, recall favors fresh + frequently-used memories, and surfacing a memory reinforces it.
