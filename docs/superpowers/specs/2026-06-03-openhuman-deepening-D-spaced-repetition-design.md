# openhuman Deepening · Slice D — Spaced Repetition Scheduling Design

**Date:** 2026-06-03
**Status:** Design (recon done; pending spec review → plan)
**Part of:** openhuman rich-memory deepening ([[project-openhuman-deepening]]). Slice D — schedule the **dormant SM-2 ladder** (`memory_graph/spaced_repetition.rs`, V45, built but never invoked) so important memories get periodically re-consolidated. Closes the **importance → spaced-repetition** loop by consuming Slice C's `memory_importance_scores`. **Zero LLM, zero migration.**

## Problem

`memory_graph/spaced_repetition.rs` is a complete SM-2 state machine — `enroll_node`, `record_pass`/`record_fail`, `due_now`, `set_enabled`, `INTERVAL_LADDER_DAYS=[1,3,7,14,30,90]`, `ENROLLMENT_IMPORTANCE_THRESHOLD=0.6`, backed by table `spaced_repetition_state` (V45) with a partial due-index — but it has **zero production callers** (only 16 in-module tests; `pi-convergence-gap-audit` flags it dormant). Nothing enrolls nodes, nothing scans for due reviews. Meanwhile Slice C now recomputes `memory_importance_scores.importance` (0..1) every `%360` tick (~3h) for `reference/episode/user_profile` kinds and archives the low-value tail. The two halves never meet: importance is computed but never used to keep valuable memories *consolidated*; the SR ladder exists but is never *driven*.

## Decision (approved 2026-06-03)

Three forks were resolved with the user:

1. **Review semantics = importance-as-grader (zero LLM).** The old core spec (`2026-05-18-agent-memory-os-engines-design.md` §4.12.3) defined "review" as an **LLM re-validation** ("is this still valid given the recent timeline?") → pass/fail. That overlaps heavily with the existing drift-detection + dream cycle and costs one LLM call per due node. Instead, **the importance score IS the grader**: on review, `importance >= threshold` ⇒ **pass** (advance the ladder + reinforce); `importance < threshold` (or archived / no score) ⇒ **drop** (un-enroll via `set_enabled(false)`, leaving C's archival to handle the node). The LLM re-validation is explicitly **deferred to a later D2 slice**; this slice does not add an LLM call.

2. **Reinforce-on-pass = re-project into the recall layer + reuse B's hotness bump.** When a node passes, re-project it into the `graph_facts` recall surface via Slice A's `recall_projection::project_fact` (content-hash idempotent — brings a node back if C had un-projected it, refreshes otherwise) and best-effort `BucketSealAdapter::reinforce_recalled` (Slice B). Both reuse already-built A/B pipelines; no new recall path.

3. **Enrollment scope = all of C's high-value kinds.** Enroll `reference/episode/user_profile` nodes (the kinds C scores) with `importance >= threshold` — the broadest "everything important gets periodically re-consolidated", aligned with C's scoring scope. (The old spec's EntityPage-only scope was the conservative fallback, not chosen.)

**No migration.** V45's `spaced_repetition_state` table + `idx_spaced_rep_due` partial index already exist. D is the only slice in the A–G program that adds zero schema.

## Design

### §1 Enrollment scan — `select_enrollable`

New reader in `memory_graph/spaced_repetition.rs` (or `store.rs` — plan pins the home; spaced_repetition.rs keeps SR cohesive):

```sql
SELECT n.id
FROM memory_nodes n
JOIN memory_importance_scores s ON s.node_id = n.id
LEFT JOIN spaced_repetition_state sr ON sr.node_id = n.id
WHERE n.kind IN (<kinds>)
  AND n.archived_at IS NULL              -- Slice C V57 column
  AND s.importance >= ?threshold
  AND sr.node_id IS NULL                 -- not already enrolled
ORDER BY s.importance DESC
LIMIT ?limit
```

`fn select_enrollable(conn, kinds: &[&str], threshold: f64, limit: usize) -> rusqlite::Result<Vec<String>>`. Returns the highest-importance unenrolled nodes. The `kinds` list is the lowercase memory-node kind strings matching C's `DEFAULT_BATCH_KINDS` recall-projectable subset (`reference`, `episode`, `user_profile` — confirm exact kind spellings against `memory_nodes.kind` storage in the plan). Each returned id → `enroll_node(conn, id, now_ms)` (idempotent upsert; preserves counters on re-enroll).

### §2 Tick orchestrator — `run_spaced_repetition_blocking`

New blocking orchestrator in `spaced_repetition.rs`, mirroring Slice C's `importance_decay::run_decay_archival_blocking(conn, ...)` shape (synchronous, takes `&Connection`, returns ids for the async half):

```rust
pub struct SpacedRepetitionReinforce {
    pub node_id: String,
    pub text: String,    // node content to re-project
    pub kind: String,    // for is_recallable_memu_type gating in the async half
}

pub struct SpacedRepetitionOutcome {
    pub enrolled: usize,
    pub passed: usize,
    pub dropped: usize,
    pub reinforce: Vec<SpacedRepetitionReinforce>,  // passed nodes to re-project async
}

pub fn run_spaced_repetition_blocking(
    conn: &Connection,
    kinds: &[&str],
    threshold: f64,
    batch_size: usize,
    now_ms: i64,
) -> SpacedRepetitionOutcome
```

Logic:
1. **Enroll:** `select_enrollable(conn, kinds, threshold, batch_size)` → `enroll_node` each; count.
2. **Review due:** `due_now(conn, now_ms, batch_size)` → for each due `node_id`:
   - Read current importance: a reader returning `Option<f64>` (e.g. `importance_decay::get_importance(conn, node_id)` — add if absent; `SELECT importance FROM memory_importance_scores WHERE node_id=?`).
   - Read the node (existing store getter) to check `archived_at` + fetch `text`/`kind`.
   - **Drop** if node missing, `archived_at IS NOT NULL`, importance `None`, or `< threshold` → `set_enabled(conn, node_id, false)`; count `dropped`.
   - **Pass** otherwise → `record_pass(conn, node_id, now_ms)` (advance ladder); push `SpacedRepetitionReinforce{node_id, text, kind}`; count `passed`.

All DB ops best-effort: a per-node error logs + continues (one bad node never aborts the batch). `record_fail` stays available but is **not used in prod** in this slice (drop = `set_enabled(false)`, avoiding daily re-fail churn on a node that's no longer important; if it later re-crosses threshold, §1's idempotent re-enroll picks it back up with counters preserved).

### §3 Proactive tick wiring (the scheduling seam)

In `proactive/service.rs`, add a job **after** Slice C's `%360` importance block (so it runs in the same cadence as importance refresh — every importance recompute is immediately followed by an SR scan, which is the literal "importance → spaced-repetition" coupling). Mirror C's `spawn_blocking` + async-half pattern:

```rust
// openhuman-D — Spaced-repetition scan. Importance-as-grader, zero LLM.
// Runs every 360 ticks (~3h), right after the importance recompute/archival
// block so it reads fresh scores. SQL-only; batch bounded.
if refs.memory_os.spaced_repetition_enabled
    && refs.memory_os.spaced_repetition_batch_size > 0
    && refs.tick_count.load(Ordering::SeqCst) % 360 == 0
{
    let store = refs.memory_graph_store.clone();
    let batch = refs.memory_os.spaced_repetition_batch_size as usize;
    let threshold = refs.memory_os.spaced_repetition_importance_threshold;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let outcome = tokio::task::spawn_blocking(move || {
        let conn = match store.conn.lock() {
            Ok(c) => c,
            Err(_) => return SpacedRepetitionOutcome::default(),
        };
        crate::memory_graph::spaced_repetition::run_spaced_repetition_blocking(
            &conn, SR_KINDS, threshold, batch, now_ms,
        )
    }).await.unwrap_or_default();

    // async half (conn dropped) — re-project passed nodes into the recall surface
    if let Some(adapter) = &refs.bucket_seal_adapter {
        for r in &outcome.reinforce {
            // project_fact is content-hash idempotent + gated by is_recallable_memu_type
            let _ = crate::memory_adapter::recall_projection::project_fact(
                adapter, &r.node_id, &r.text,
            ).await;  // best-effort; debug-log on err
            let _ = adapter.reinforce_recalled(&[r.node_id.clone()], now_ms).await; // B; no-op if not yet a summary
        }
    }
}
```

`SR_KINDS` = the recall-projectable high-value kinds const (shared with / derived from C's set). `SpacedRepetitionOutcome` derives `Default`. Exact `refs.*` field names + the node getter signature are pinned in the plan against the live `proactive/service.rs`.

### §4 Config (`memubot_config.rs` + `MemoryOsRuntimeConfig`)

Mirror C's `importance_archive_*` serde pattern exactly (field + doc comment + `#[serde(default = "fn")]` + default fn + MemoryOsRuntimeConfig threading + integration test):

- `spaced_repetition_enabled: bool` (default **true** — actually closes the loop; reversible via flag).
- `spaced_repetition_batch_size: u32` (default **50** — enrollment + due are SQL-only, single-digit ms; `0` disables).
- `spaced_repetition_importance_threshold: f64` (default **0.6** — matches `ENROLLMENT_IMPORTANCE_THRESHOLD`; gates both enrollment and the pass/drop decision).

### §5 Reinforce semantics + runaway notes

- **Re-project** keeps an important memory present + fresh in the `graph_facts` recall surface; if C had un-projected it during a transient importance dip that later recovered, a passing review re-projects it. Idempotent (content-hash) → no duplication.
- **Hotness bump** (`reinforce_recalled`) only takes effect once the projected fact has sealed into a `mem_tree_summaries` summary (its `WHERE id IN` no-ops on raw FTS chunk ids) — acceptable; it's a best-effort reuse of B's pipeline, not a correctness dependency.
- **No runaway:** the ladder *lengthens* on pass (1→3→7→14→30→90 days), so a stable important memory is reviewed *less* over time, not more. Reinforcement here cannot inflate importance (importance is computed by C from citations/edges/recency/status, not from review count) — so there is no review→importance→review positive feedback. Hotness can rise via B, but that's bounded by B's log-scale + recency counterbalance.

## Data flow (after D)

```
proactive tick %360:
  [C] recompute importance + archive low-value tail
  [D] run_spaced_repetition_blocking:
        enroll: importance>=0.6 & high-value kind & !archived & !enrolled → enroll_node
        due_now → per node: importance>=0.6 ? record_pass (ladder++) : set_enabled(false)
      → reinforce[] (passed nodes)
  [D async] per passed node: project_fact (re-consolidate into recall) + reinforce_recalled (hotness)
```

## Out of scope

LLM re-validation review (deferred to D2 — the `record_fail` + `review_queue_items` path stays unused); time-decay of `recall_hit_count` (future); any new migration (V45 suffices); a manual enroll/stats Tauri command (autonomous loop; add later if observability needs it); EntityPage-specific review (covered by the kind-based scope).

## Error handling

Whole job is best-effort and never blocks the turn: DB-lock failure → empty `Default` outcome; per-node DB error → log + continue; the async re-project/reinforce half is fire-and-forget with debug-log-on-error (mirror Slice A/B/C posture). All reads are null-safe (missing importance → drop, not panic).

## Testing

1. **`select_enrollable`**: high-importance unenrolled node returned; already-enrolled excluded; archived excluded; below-threshold excluded; wrong-kind excluded; respects `limit` + ordering by importance desc.
2. **Orchestrator enroll**: a fresh high-importance node → after `run_spaced_repetition_blocking`, it has a `spaced_repetition_state` row (`enabled=1`, `interval_idx=0`); `outcome.enrolled` counts it.
3. **Orchestrator pass**: an enrolled, due, importance≥threshold node → `record_pass` advanced its `interval_idx`; it appears in `outcome.reinforce` with correct text/kind; `outcome.passed` counts it.
4. **Orchestrator drop (low importance)**: an enrolled, due, importance<threshold node → `enabled=0` after; NOT in `reinforce`; `outcome.dropped` counts it.
5. **Orchestrator drop (archived)**: an enrolled, due, archived node → `enabled=0`; not reinforced.
6. **Config**: `MemoryOsRuntimeConfig` deserializes the 3 knobs with documented defaults; missing keys fall back via `#[serde(default)]`.
7. `cargo build`/clippy clean; `cargo test --lib memory_graph::spaced_repetition memory_graph::importance_decay memubot proactive memory_adapter` green; **the broad dependent-module run** (the Slice-C lesson — any fixture that builds memory_graph and now reads `memory_importance_scores`/`spaced_repetition_state` via the new join) green. Fixtures route through the canonical `db::migrations::run` (V45 already there).

## Scope / files

| File | Change |
|---|---|
| `memory_graph/spaced_repetition.rs` | `select_enrollable(conn, kinds, threshold, limit)`; `run_spaced_repetition_blocking(...) -> SpacedRepetitionOutcome` (+ `SpacedRepetitionReinforce`, `SpacedRepetitionOutcome: Default`) |
| `memory_graph/importance_decay.rs` (or store) | `get_importance(conn, node_id) -> Option<f64>` reader if not already present |
| `memubot_config.rs` (+ `MemoryOsRuntimeConfig`) | `spaced_repetition_enabled` / `_batch_size` / `_importance_threshold` + defaults |
| `proactive/service.rs` | `%360` SR job after the importance block: `spawn_blocking(run_spaced_repetition_blocking)` + async re-project/reinforce half |

## Risk

Low–Med. No migration, no LLM, no new recall path — pure wiring of an already-tested module + a small SQL join + reuse of A/B/C's project/reinforce pipelines. Main risks: (1) the **Slice-C fixture lesson** — the new `memory_importance_scores ⋈ spaced_repetition_state` join can break fixtures built without those tables, so the verify runs the broad dependent suite and fixtures go through `db::migrations::run`; (2) **kind-string mismatch** — `select_enrollable`'s `kind IN (...)` must use the exact `memory_nodes.kind` spellings (plan pins them against the live enum/storage); (3) **tick field drift** — `refs.*` names + the node getter signature must match the live `proactive/service.rs` (plan pins against current source, like C did). Bisectable: enroll query → orchestrator → config → tick wiring → verify. After D, every importance refresh re-consolidates the still-valuable memories on a lengthening ladder and lets the faded ones fall out to C's archival — the importance→spaced-repetition loop is closed, at zero LLM cost.
