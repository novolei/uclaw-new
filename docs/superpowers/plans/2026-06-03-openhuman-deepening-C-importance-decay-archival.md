# openhuman Deepening · Slice C — Importance/Decay/Archival Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Complete the archival half of the decay loop — soft-archive (reversible) low-importance reflection facts after a grace period, un-projecting them from the bucket_seal recall surface, and extend importance scoring to the reflection-fact kinds that were never scored.

**Architecture:** The importance COMPUTE loop already runs in the proactive `%360` tick. C adds: (T1) `memory_nodes.archived_at` (migration V56) + archive/restore store helpers + an `archived_at IS NULL` filter on recall/projection-feeding reads; (T2) extend `DEFAULT_BATCH_KINDS` to reflection facts + skip archived in recompute; (T3) `mark_archive_pending` (hysteresis) + `select_archivable_past_grace` helpers; (T4) wire Phase 2/3 into the tick + config knobs; (T5) restore cmd. Soft-archive = set `archived_at` + delete the bucket_seal `graph_facts` projection (recall-exclude). Best-effort throughout.

**Tech Stack:** Rust (rusqlite migrations, async tokio tick, spawn_blocking for DB), bucket_seal adapter (async delete). No new deps.

**Key facts (recon, file:line):**
- **Migration**: highest is `V55_SESSION_TREE` → next free = **V56** (plan T1 re-confirms no open PR claimed V56; see *Active migration registry* in CONTEXT.md + the migration-discipline rule in CLAUDE.md). Migrations in `src/db/migrations.rs`; applied via the `run()` list.
- **`memory_nodes`**: no `archived_at` today; no Archived status. Hard delete only via `store.delete_node()`. `MemoryGraphStore` has `list_nodes_by_kind(space, kind, limit)` (`store.rs:~210`) + `get_active_version(node_id)` (`~:649`).
- **`importance_decay.rs`**: `DEFAULT_BATCH_KINDS` (`:417`) = boot/identity/value/directive/curated/entity_page (EXTEND). `batch_recompute_importance(conn, kinds, limit, now_ms) -> BatchRecomputeOutcome{recomputed,errored}` (`:457`). `collect_node_importance_inputs(conn, node_id)` (`:328`, the per-node reader). `upsert_importance_score` (`:263`, does NOT touch archive_pending_since). `list_decay_candidates(conn, space) -> Vec<ImportanceRow{node_id,title,importance,archive_pending_since,last_computed_at}>` (`:543`, reads `archive_pending_since IS NOT NULL`). compute_importance formula `:165`.
- **V44 `memory_importance_scores`** (`migrations.rs:~2308`): cols incl. `importance`, `archive_pending_since` (epoch-ms, NULL=not pending), `last_computed_at`; `idx_importance_scores_archive` partial index on `archive_pending_since IS NOT NULL`. `node_id` FK → memory_nodes ON DELETE CASCADE.
- **Tick hook** (`proactive/service.rs:~1404`): the `%360` block — `if refs.memory_os.importance_decay_enabled && batch_size>0 && tick%360==0 { let store=refs.memory_graph_store.clone(); spawn_blocking(move || { let conn=store.conn.lock()...; batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, batch_size, now_ms) }) }` — currently **fire-and-forget** (not awaited). `refs` (ProactiveStateRefs) has `memory_graph_store: Arc<MemoryGraphStore>`, `memory_os: MemoryOsRuntimeConfig`, `bucket_seal_adapter: Option<Arc<BucketSealAdapter>>` (`:422`).
- **bucket_seal un-project**: `BucketSealAdapter::delete(namespace, key) -> Result<bool>` (`memory_bucket_seal/adapter.rs:~682`, ASYNC). `recall_projection::RECALL_PROJECTION_NAMESPACE = "graph_facts"` + `project_fact(adapter, node_id, text)` (Slice A, `memory_adapter/recall_projection.rs`). Backfill `recall_projection_backfill.rs` + reflection live projection `memory_graph/reflection.rs:~611` (Slice A) must skip archived.
- **Config**: `MemoryOsRuntimeConfig.importance_decay_enabled/batch_size`; `memubot_config.rs:562/568` (fields), `:710/715` (defaults), + the serde tests `:1123-1167`. Mirror this pattern for the new knobs.

---

## Task 1: Migration V56 (`archived_at`) + archive/restore helpers + archived read-filter

**Files:** `db/migrations.rs`, `memory_graph/store.rs`, CONTEXT.md (registry row).

- [ ] **Step 1: Confirm V56 is free.** `grep -rn "V56" src/db/migrations.rs` (none) + check no open PR claimed it (`git log --oneline -15` + the CONTEXT.md registry). If V56 is taken, use the next free integer + adjust all references below.
- [ ] **Step 2: Migration.** Add `pub const V56_MEMORY_NODE_ARCHIVED: &str = "...";` in `db/migrations.rs`:
```sql
ALTER TABLE memory_nodes ADD COLUMN archived_at INTEGER;
CREATE INDEX IF NOT EXISTS idx_memory_nodes_archived
    ON memory_nodes(archived_at) WHERE archived_at IS NOT NULL;
```
Register it in the `run()` migration list (after V55). Add a migration-applies test (mirror an existing one) asserting `archived_at` column exists + defaults NULL on existing rows. Add the registry row to CONTEXT.md.
- [ ] **Step 3: Store helpers** (`store.rs`): 
```rust
/// Soft-archive a node (set archived_at = now_ms). Reversible via restore_node.
pub fn archive_node(&self, node_id: &str, now_ms: i64) -> Result<bool, Error>  // UPDATE memory_nodes SET archived_at=?2 WHERE id=?1 AND archived_at IS NULL; Ok(rows>0)
/// Un-archive a node (clear archived_at).
pub fn restore_node(&self, node_id: &str) -> Result<bool, Error>  // UPDATE memory_nodes SET archived_at=NULL WHERE id=?1 AND archived_at IS NOT NULL; Ok(rows>0)
```
- [ ] **Step 4: Filter archived from recall/projection-feeding reads.** Add `AND archived_at IS NULL` to `list_nodes_by_kind` (`store.rs:~210`) — it feeds the Slice-A backfill + would otherwise re-project archived nodes. Grep the OTHER readers of memory_nodes that feed projection/recall (the Slice-A backfill `recall_projection_backfill.rs` uses `list_nodes_by_kind` → covered; reflection live projection at `reflection.rs:~611` projects freshly-created nodes which are never archived → no change needed, but confirm). Do NOT blanket-filter every memory_nodes query (e.g. WikiView/admin reads may want archived); only the recall/projection-feeding path. Report exactly which reads you filtered + why.
- [ ] **Step 5: Tests** — `archive_node` sets archived_at + returns true once / false on already-archived; `restore_node` clears it; `list_nodes_by_kind` excludes an archived node. (in `store.rs` tests, in-memory store.)
- [ ] **Step 6: Build + test + commit** — `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty); `cargo test --lib memory_graph::store db::migrations 2>&1 | tail`. Commit: `feat(memory): V56 memory_nodes.archived_at + archive/restore helpers + archived read-filter (openhuman-C)`

---

## Task 2: Extend importance scoring to reflection facts + skip archived

**Files:** `memory_graph/importance_decay.rs`.

- [ ] **Step 1: Extend `DEFAULT_BATCH_KINDS`** (`:417`) — append `"reference"`, `"episode"`, `"user_profile"` to the existing boot/identity/value/directive/curated/entity_page list. Update the const's doc comment (these high-volume reflection-fact kinds are now scored so they can decay/archive — openhuman-C).
- [ ] **Step 2: Skip archived in recompute.** In `batch_recompute_importance`'s node-selection SQL (`:457`) add `AND archived_at IS NULL` (don't re-score archived nodes). Confirm `collect_node_importance_inputs` (`:328`) joins memory_nodes — if it independently reads the node, ensure it's consistent (an archived node shouldn't be scored; the batch select gate is the primary guard).
- [ ] **Step 3: Test** — seed a `reference` + `episode` + `user_profile` node (active versions) → `batch_recompute_importance(conn, DEFAULT_BATCH_KINDS, 100, now)` scores them (a `memory_importance_scores` row appears, was excluded before); seed an `archived_at`-set node → it is NOT scored.
- [ ] **Step 4: Build + test + commit** — clean; `cargo test --lib memory_graph::importance_decay 2>&1 | tail`. Commit: `feat(memory): score reflection facts (reference/episode/user_profile) + skip archived in importance recompute (openhuman-C)`

---

## Task 3: Phase-2/3 helpers — mark_archive_pending (hysteresis) + select_archivable_past_grace

**Files:** `memory_graph/importance_decay.rs`.

- [ ] **Step 1: `mark_archive_pending`** — set/clear the pending clock:
```rust
/// Archivable kinds = transient knowledge/events (+ user_profile only when include_user_profile).
/// Never boot/identity/value/directive/curated/entity_page.
pub const ARCHIVABLE_KINDS: &[&str] = &["reference", "episode"];   // user_profile added conditionally
pub struct MarkPendingOutcome { pub pended: usize, pub cleared: usize }
pub fn mark_archive_pending(conn: &Connection, threshold: f64, now_ms: i64, include_user_profile: bool) -> Result<MarkPendingOutcome, Error>
```
Implementation: (a) `UPDATE memory_importance_scores SET archive_pending_since = ?now WHERE importance < ?threshold AND archive_pending_since IS NULL AND node_id IN (SELECT id FROM memory_nodes WHERE archived_at IS NULL AND kind IN (<archivable kinds, +user_profile if flag>))` → `pended` = changes(); (b) hysteresis clear: `UPDATE memory_importance_scores SET archive_pending_since = NULL WHERE importance >= ?threshold AND archive_pending_since IS NOT NULL` → `cleared` = changes(). Build the kind-IN list dynamically from `ARCHIVABLE_KINDS` (+ "user_profile" when `include_user_profile`).
- [ ] **Step 2: `select_archivable_past_grace`** — list node_ids whose pending exceeded the grace:
```rust
pub fn select_archivable_past_grace(conn: &Connection, grace_ms: i64, now_ms: i64, include_user_profile: bool) -> Result<Vec<String>, Error>
// SELECT s.node_id FROM memory_importance_scores s JOIN memory_nodes n ON n.id=s.node_id
// WHERE s.archive_pending_since IS NOT NULL AND s.archive_pending_since < (?now - ?grace)
//   AND n.archived_at IS NULL AND n.kind IN (<archivable kinds, +user_profile if flag>)
```
(The actual `archive_node` + bucket_seal un-project happen in the tick — T4 — because un-project is async. This fn is the sync selector.)
- [ ] **Step 3: Tests** — `mark_archive_pending`: a sub-threshold reference node gets pending set; raising importance ≥ threshold clears it; a sub-threshold user_profile is NOT pended when `include_user_profile=false`, IS when true; boot/curated never pended. `select_archivable_past_grace`: returns a node pending longer than grace, excludes within-grace + excludes archived. (in-memory conn, seed `memory_importance_scores` rows directly + memory_nodes.)
- [ ] **Step 4: Build + test + commit** — clean; `cargo test --lib memory_graph::importance_decay 2>&1 | tail`. Commit: `feat(memory): mark_archive_pending (hysteresis) + select_archivable_past_grace helpers (openhuman-C)`

---

## Task 4: Wire Phase 2/3 into the proactive tick + config knobs

**Files:** `memubot_config.rs` (+ `MemoryOsRuntimeConfig` mapping), `proactive/service.rs`.

- [ ] **Step 1: Config knobs.** In `memubot_config.rs` add to the memory_os config (mirror `importance_decay_*` at `:562/710` + serde tests `:1123`): `importance_archive_threshold: f64` (default `0.3`), `importance_archive_grace_days: u32` (default `30`), `importance_archive_user_profile: bool` (default `false`). Thread them into `MemoryOsRuntimeConfig` (the struct the proactive service reads — find where `importance_decay_enabled` is mapped into it + add the 3). Update the serde default tests.
- [ ] **Step 2: Wire Phase 2 + Phase 3 into the `%360` tick** (`proactive/service.rs:~1404`). After the existing `batch_recompute_importance` in the blocking closure, ALSO run Phase 2 (`mark_archive_pending`) + collect Phase 3 ids (`select_archivable_past_grace`) + `archive_node` each (all sync, inside the closure), and RETURN the archived node_ids from the closure. Then (async, after awaiting the spawn_blocking handle) un-project each. Concretely — convert the block from fire-and-forget to awaited, returning the ids:
```rust
if refs.memory_os.importance_decay_enabled && refs.memory_os.importance_decay_batch_size > 0
    && refs.tick_count.load(Ordering::SeqCst) % 360 == 0
{
    let store = refs.memory_graph_store.clone();
    let batch_size = refs.memory_os.importance_decay_batch_size as usize;
    let threshold = refs.memory_os.importance_archive_threshold;
    let grace_ms = refs.memory_os.importance_archive_grace_days as i64 * 86_400_000;
    let incl_profile = refs.memory_os.importance_archive_user_profile;
    let now_ms = chrono::Utc::now().timestamp_millis();
    let archived_ids: Vec<String> = tokio::task::spawn_blocking(move || {
        let conn = match store.conn.lock() { Ok(c) => c, Err(e) => { tracing::warn!(error=%e, "importance_decay: DB lock"); return Vec::new(); } };
        // Phase 1 (existing)
        let _ = crate::memory_graph::importance_decay::batch_recompute_importance(&conn, crate::memory_graph::importance_decay::DEFAULT_BATCH_KINDS, batch_size, now_ms);
        // Phase 2
        match crate::memory_graph::importance_decay::mark_archive_pending(&conn, threshold, now_ms, incl_profile) {
            Ok(o) => if o.pended>0 || o.cleared>0 { tracing::info!(pended=o.pended, cleared=o.cleared, "importance_decay: pending"); },
            Err(e) => tracing::warn!(error=%e, "importance_decay: mark_pending failed"),
        }
        // Phase 3 select + archive (sync)
        let ids = crate::memory_graph::importance_decay::select_archivable_past_grace(&conn, grace_ms, now_ms, incl_profile).unwrap_or_default();
        let mut archived = Vec::new();
        for id in ids {
            match store_archive(&conn, &id, now_ms) {  // inline UPDATE or call store.archive_node via a conn-taking helper
                Ok(true) => archived.push(id),
                _ => {}
            }
        }
        archived
    }).await.unwrap_or_default();
    // Phase 3 un-project (async, best-effort)
    if let Some(adapter) = &refs.bucket_seal_adapter {
        for id in &archived_ids {
            let _ = adapter.delete(crate::memory_adapter::recall_projection::RECALL_PROJECTION_NAMESPACE, id).await;
        }
        if !archived_ids.is_empty() { tracing::info!(archived = archived_ids.len(), "importance_decay: archived + un-projected"); }
    }
}
```
   NOTE: `archive_node` is a `&self` method on the store but the closure holds `conn` (the locked guard). Either add a conn-taking free helper `archive_node_conn(conn, id, now)` in store.rs/importance_decay.rs (cleanest — the UPDATE is one line) OR drop the lock + call `store.archive_node` per id (extra locking). Recommend a conn-taking helper used by both `store.archive_node` and here (DRY). Resolve in impl. Keep `restore_node` (T1) as the `&self` method for the cmd path.
- [ ] **Step 2b: Confirm the await doesn't stall ticks** — `tick_inner` is per-interval; awaiting the spawn_blocking handle within one tick is fine (bounded batch). Verify no deadlock (the conn lock is released when the closure returns, before the async deletes).
- [ ] **Step 3: Build + clippy + test** — clean; `cargo test --lib proactive memubot_config 2>&1 | tail`.
- [ ] **Step 4: Commit** — `feat(memory): wire decay archival (mark_pending + grace-promote + un-project) into proactive tick + archive config knobs (openhuman-C)`

---

## Task 5: Restore Tauri command

**Files:** `tauri_commands.rs`, `main.rs`.

- [ ] **Step 1: Cmd** — `memory_importance_restore(state, node_id: String) -> Result<bool, String>`: `state.memory_graph_store.restore_node(&node_id)` → if restored, re-project: read the node's active version content (`get_active_version`) + `recall_projection::project_fact(&state.bucket_seal_adapter, &node_id, &content)` (re-surface in recall). Return whether restored. Register in `main.rs` invoke_handler (two-edit rule).
- [ ] **Step 2: Test** — archive a node (archive_node) + delete its projection → `memory_importance_restore` clears archived_at + re-projects (recall surfaces it again). (Can be a store+adapter integration test if the cmd is thin; or test `restore_node` + a manual re-project.)
- [ ] **Step 3: Build + test + commit** — clean. Commit: `feat(memory): memory_importance_restore cmd (un-archive + re-project) (openhuman-C)`

---

## Task 6: Whole-slice verification + ship

- [ ] **Step 1:** `cargo build` + `cargo clippy --lib` clean; `cargo test --lib memory_graph::importance_decay memory_graph::store proactive memory_adapter memubot_config db::migrations 2>&1 | grep "test result:"` green.
- [ ] **Step 2: Integration sanity (test if feasible):** seed a sub-threshold reflection node + a projection → run mark_pending (now) → fast-forward `select_archivable_past_grace` with now = pending+grace+1 → archive + un-project → assert recall (load_context/recall_hybrid over graph_facts) no longer surfaces it; restore → surfaces again. (Use explicit `now_ms` args to avoid real-time waits.)
- [ ] **Step 3: Gates:** `grep -rn "archive_pending_since" src/` shows it's now SET (mark_archive_pending) not just read. `grep -rn "archived_at" src/memory_graph/store.rs` (helpers + filter present). Migration V56 in `run()`.
- [ ] **Step 4: Ship** — push → PR (Commits table T1-T5) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 5: Post-merge soak (manual):** with a low `importance_archive_grace_days` (or a seeded old pending), confirm a low-value reflection fact gets archived + drops out of `<memory_context>` recall; `memory_importance_restore` brings it back. user_profile facts are NOT auto-archived (default). Boot/identity facts never archived.

---

## Self-Review

- **Spec coverage:** §1 score-extension→T2; §2 mark-pending(hysteresis)→T3; §3 promote+un-project→T3(select)+T4(archive+delete); §4 soft-archive migration+helpers→T1; §5 restore→T5; §6 config→T4. ✓
- **Ordering compiles:** migration+helpers (T1) → score-extension (T2) → pending/promote helpers (T3) → tick wiring+config (T4, uses T1 archive_node + T3 helpers + bucket_seal) → restore cmd (T5). Each builds. ✓
- **Type consistency:** `archive_node(id, now_ms)->bool` / `restore_node(id)->bool`; `mark_archive_pending(conn, threshold, now_ms, incl)->MarkPendingOutcome{pended,cleared}`; `select_archivable_past_grace(conn, grace_ms, now_ms, incl)->Vec<String>`; `ARCHIVABLE_KINDS`; config `importance_archive_threshold/_grace_days/_user_profile`; `RECALL_PROJECTION_NAMESPACE` (Slice A). Consistent across tasks. ✓
- **No placeholders:** real SQL + signatures + the async/sync split (blocking closure returns ids → async un-project) + the conn-taking-helper note for archive inside the closure. Flagged impl points: exact V-number reconfirm (T1 Step1), which reads get the archived filter (T1 Step4 — recall/projection-feeding only), conn-helper vs &self for archive-in-closure (T4). ✓
- **Migration discipline:** V56 reconfirmed against registry + open PRs (T1 Step 1); CONTEXT.md registry row added. ✓
- **Finish-line:** after C, low-value reflection facts decay→pending→(grace)→soft-archive + un-projected from recall, reversibly (restore); user_profile sticky by default; importance scores now complete (D's enrollment prerequisite). ✓
