# openhuman Deepening · Slice C — Importance / Decay / Archival Loop Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice C — wire the **archival half** of the decay loop + extend importance scoring to reflection facts. Independent of A (just-shipped) for compute, but COUPLES to A's bucket_seal projection on archival. Siblings: B (importance→recall ranking), D (spaced-repetition, reads C's scores), E/F/G.

## Problem

The importance COMPUTE loop is **already wired** (recon corrected a stale assumption): `batch_recompute_importance` runs in `proactive/service.rs:~1403` every `%360` ticks (~3h), gated by `importance_decay_enabled` (default true), populating `memory_importance_scores` (V44). But the **archival half is entirely absent**:

1. **`archive_pending_since` is never set** by any code (`memory_importance_scores.archive_pending_since` column + `idx_importance_scores_archive` partial index exist in V44, but zero `UPDATE … SET archive_pending_since` writes exist). `list_decay_candidates` (`importance_decay.rs:547`, reads `WHERE archive_pending_since IS NOT NULL`) therefore always returns empty.
2. **No archival action exists.** `MemoryNodeKind`/`MemoryVersionStatus` have no Archived variant; the only destructive op is `store.delete_node()` (hard delete). There is no soft-archive on `memory_nodes`.
3. **Reflection facts aren't even scored.** `DEFAULT_BATCH_KINDS` (`importance_decay.rs:417`) = `boot/identity/value/directive/curated/entity_page` — it **excludes** `reference/episode/user_profile`, which are exactly the high-volume reflection facts (+ Slice-A projections) where decay/forgetting matters most. So those facts accumulate forever, never scored, never forgotten.
4. **Slice-A coupling gap:** if a node is archived, its bucket_seal `graph_facts` projection (Slice A, `recall_projection.rs`) is not removed, so recall keeps surfacing archived facts.

So the agent's memory never forgets: low-value, uncited, un-recalled reflection facts pile up and stay recallable indefinitely.

## Decision (approved 2026-06-03)

- **Soft-archive (reversible).** Add `memory_nodes.archived_at` (migration V47); archival sets it + deletes the bucket_seal projection (recall-exclude). Data is never destroyed; restorable. "Dormant, not destroyed" — the brain-like forgetting model.
- **Extend importance scoring to reflection facts** (`reference/episode/user_profile`). Archive `reference/episode` (transient knowledge/events). **`user_profile` is scored but NOT auto-archived by default** (preferences are sticky — openhuman gives Identity a 90d half-life; forgetting a real preference after 30d non-use is wrong); a config flag (`importance_archive_user_profile`, default false) gates it.
- **Slice-A coupling:** on archive, `bucket_seal_adapter.delete(RECALL_PROJECTION_NAMESPACE, node_id)`; projection/backfill/reflection re-projection + importance recompute all SKIP archived nodes.

## Design — the archival loop (3 phases in the proactive tick)

All phases gated by `importance_decay_enabled`; best-effort (per-node failure warns + continues, never blocks the tick); mirror the existing `spawn_blocking` periodic-job pattern (e.g. `tier_escalator` at `proactive/service.rs:~1354`).

### §1 Phase 1 (exists, EXTEND kinds) — score reflection facts
Add `reference`, `episode`, `user_profile` to `DEFAULT_BATCH_KINDS` (`importance_decay.rs:417`) so `batch_recompute_importance` scores them. Keep boot/identity/value/directive/curated/entity_page. The batch is already limit-capped (`importance_decay_batch_size`, unscored-first/oldest order) — adding kinds just widens the rotation. `batch_recompute_importance` + `collect_node_importance_inputs` must SKIP `archived_at IS NOT NULL` nodes (don't re-score archived).

### §2 Phase 2 (new) — set / clear `archive_pending_since` (with hysteresis)
After the recompute, in the same `%360` tick:
- `mark_archive_pending(conn, threshold, now_ms)`: `UPDATE memory_importance_scores SET archive_pending_since = ?now WHERE importance < ?threshold AND archive_pending_since IS NULL` (only for archivable kinds — join `memory_nodes`, exclude boot/identity/value/directive; exclude user_profile unless `importance_archive_user_profile`).
- Hysteresis (clear): `UPDATE … SET archive_pending_since = NULL WHERE importance >= ?threshold AND archive_pending_since IS NOT NULL` — a node whose importance recovered (e.g. got cited/recalled) un-pends, never archived.
Returns counts (pended, cleared) for logging. `threshold` = `importance_archive_threshold` (default 0.3).

### §3 Phase 3 (new) — promote past-grace pending → soft-archive + un-project
`promote_archived(conn, store, adapter, grace_ms, now_ms)`: select rows where `archive_pending_since IS NOT NULL AND archive_pending_since < (now_ms - grace_ms)` (reuse/extend `list_decay_candidates`), filtered to archivable kinds. For each:
1. **Soft-archive**: `store.archive_node(node_id)` → `UPDATE memory_nodes SET archived_at = ?now WHERE id = ?` (the node + its versions stay; only the flag changes).
2. **Un-project**: `bucket_seal_adapter.delete(RECALL_PROJECTION_NAMESPACE, node_id)` (recall no longer surfaces it). Best-effort (log+swallow).
Returns count archived. `grace_ms` = `importance_archive_grace_days` (default 30) × 86_400_000.

### §4 Soft-archive mechanism (migration + store helpers)
- **Migration V47**: `ALTER TABLE memory_nodes ADD COLUMN archived_at INTEGER` (NULL = active; epoch-ms = archived). Partial index `CREATE INDEX idx_memory_nodes_archived ON memory_nodes(archived_at) WHERE archived_at IS NOT NULL` for the scan + an `archived_at IS NULL` predicate on hot reads. (Pick the next free V-number — confirm in the plan against `db/migrations.rs` + the active-migration registry; spec assumes V47.)
- **Store helpers** (`MemoryGraphStore`): `archive_node(id) -> Result<bool>` (set archived_at), `restore_node(id) -> Result<bool>` (clear archived_at). **Exclude archived from existing reads that feed recall/projection**: `list_nodes_by_kind` + `get_active_version`-driven projection paths + the Slice-A backfill (`recall_projection_backfill.rs`) + reflection live projection (`reflection.rs:~611`) must filter `archived_at IS NULL` (don't re-project/re-surface archived). Confirm in the plan which read methods need the filter (the recall-feeding ones; not necessarily every internal query).

### §5 Restore (reversibility)
- `restore_node(id)` (store) clears `archived_at` + the caller re-projects via `recall_projection::project_fact` (re-surface in recall). A thin Tauri cmd `memory_importance_restore(node_id)` (define + register in `main.rs` invoke_handler) wires it for the FE (UI later). The existing `memory_importance_list_candidates` cmd surfaces pending/archived candidates.

### §6 Config (`MemoryOsRuntimeConfig` + `memubot_config.rs`)
- `importance_archive_threshold: f64` (default 0.3) — below → pending.
- `importance_archive_grace_days: u32` (default 30) — pending duration before archive.
- `importance_archive_user_profile: bool` (default false) — include user_profile in auto-archival.
Gate everything under the existing `importance_decay_enabled`. Mirror the `importance_decay_*` field pattern (memubot_config.rs:~562/710 + the runtime config mapping).

## Data flow (after C)

```
%360 tick (importance_decay_enabled):
  Phase 1  batch_recompute_importance(kinds += reference/episode/user_profile, skip archived) → memory_importance_scores
  Phase 2  mark_archive_pending(threshold): sub-threshold non-pending → set archive_pending_since; recovered → clear (hysteresis)
  Phase 3  promote_archived(grace): pending > grace days → archive_node (memory_nodes.archived_at) + bucket_seal.delete("graph_facts", node_id)
recall (load_context recall_hybrid) → archived nodes' projections gone → not surfaced  ✓ forgetting
restore (manual cmd / importance recovery while pending) → archived_at cleared + re-project  ✓ reversible
```

## Out of scope (sibling slices)

importance as a recall-RANKING signal (Slice B — `recall.rs`/bucket_seal scoring); spaced-repetition scheduling (Slice D — reads C's now-complete importance scores, enrollment ≥0.6); archival UI; auto-restore-on-recovery after full archival (once archived, the node is skipped by recompute so it can't auto-recover — restore is manual; that's intended). boot/identity/value/directive archival (high-value, never auto-archived).

## Error handling

All phases best-effort: a per-node archive/un-project failure warns + continues (never blocks the tick or other nodes). The bucket_seal `delete` is best-effort (a failed un-project logs; the node is still soft-archived — recall might transiently surface it until the next projection-cleanup, acceptable). Migration V47 is additive (nullable column) — safe. `archive_node`/`restore_node` return `Ok(false)` for unknown ids (no panic).

## Testing

1. **Score extension:** `batch_recompute_importance` now scores a seeded `reference`/`episode`/`user_profile` node (was excluded before); skips an `archived_at`-set node.
2. **Phase 2 hysteresis:** a sub-threshold node gets `archive_pending_since` set; raising its importance ≥ threshold on the next pass clears it (no archive).
3. **Phase 3 archive + un-project:** a node pending > grace → `archived_at` set AND its `graph_facts` projection deleted from bucket_seal (assert recall no longer surfaces it). A pending-but-within-grace node is NOT archived.
4. **user_profile gate:** with `importance_archive_user_profile=false`, a sub-threshold past-grace user_profile node is NOT archived; with true, it is.
5. **Restore round-trip:** `restore_node` clears `archived_at` + re-projection makes it recallable again.
6. **Migration V47:** fresh DB has `memory_nodes.archived_at` + the partial index; existing rows default NULL (active).
7. `cargo build`/clippy clean; `cargo test --lib memory_graph::importance_decay proactive memory_adapter memubot_config db::migrations` green.

## Scope / files

| File | Change |
|---|---|
| `db/migrations.rs` | V47: `memory_nodes.archived_at` + partial index (next free V-number) |
| `memory_graph/store.rs` | `archive_node`/`restore_node`; `archived_at IS NULL` filter on recall/projection-feeding reads (`list_nodes_by_kind` etc.) |
| `memory_graph/importance_decay.rs` | extend `DEFAULT_BATCH_KINDS` (+reference/episode/user_profile); skip archived in recompute; `mark_archive_pending`; `promote_archived` (or the pending+promote helpers) |
| `proactive/service.rs` | wire Phase 2 + Phase 3 into the `%360` tick (after the existing Phase-1 recompute), best-effort spawn_blocking; pass bucket_seal_adapter + config |
| `memory_adapter/recall_projection_backfill.rs` + `memory_graph/reflection.rs` | skip archived nodes when projecting |
| `memubot_config.rs` (+ MemoryOsRuntimeConfig) | `importance_archive_threshold`/`_grace_days`/`_user_profile` config |
| `tauri_commands.rs` + `main.rs` | `memory_importance_restore(node_id)` cmd (+ invoke_handler) |

## Risk

Med. The compute loop already runs; C adds the archival completion + a small migration + config. Main risks: (1) over-forgetting — mitigated by the 0.3 threshold + 30d grace + hysteresis + user_profile-excluded-by-default + soft (reversible) archive; (2) the Slice-A un-projection coupling (must delete the graph_facts projection on archive, else archived facts still recalled — covered in §3 + tested in §3/Test 3); (3) the `archived_at` read-filter must cover the recall/projection-feeding paths without breaking unrelated queries (plan pins exactly which reads). Bisectable: migration+store helpers → score extension → pending/promote helpers → tick wiring → config+restore cmd → verify. After C, memory forgets (low-value reflection facts decay→archive→un-recalled) reversibly; D's spaced-rep enrollment gets complete importance scores.
