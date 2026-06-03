# openhuman Deepening · Slice B — Recency + Reinforcement Recall Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:subagent-driven-development. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make `recall_hybrid` ranking favor fresh + frequently-recalled memories — add a recency-decay factor and a log-scaled hotness factor to the semantic-leg score, plus a fire-and-forget reinforcement write-back (recalled summaries get hotter) wired only into the agent-context `load_context` path.

**Architecture:** B1 recency lives in `recall_semantic` (uses the existing `sealed_at`; no schema). B3 hotness adds two columns to bucket_seal's `mem_tree_summaries` (via its OWN `ensure_schema`, idempotent ALTER — bucket_seal is NOT in the main `db/migrations.rs` V-runner), read into ranking + bumped by a new `reinforce_recalled` called from `load_context` after recall (NOT inside recall_hybrid, so dedup/internal callers don't inflate hotness). B2 (importance/salience) is deferred.

**Tech Stack:** Rust (rusqlite, bucket_seal `chunks.db`), config via `MemoryOsConfig`→adapter fields + `MemoryOsRuntimeConfig`. No new deps. No main-DB migration (bucket_seal has its own schema).

**Key facts (recon, file:line):**
- **bucket_seal schema** = `BucketSealStore::ensure_schema()` (`memory_bucket_seal/store.rs:228`), pure `CREATE TABLE/INDEX IF NOT EXISTS` (idempotent, applied lazily). `mem_tree_summaries` CREATE at `store.rs:90`. **No user_version/V-runner** — adding columns to the existing table = idempotent `ALTER TABLE ... ADD COLUMN` in `ensure_schema` with duplicate-column-error tolerance (SQLite errors "duplicate column name" on re-run; catch+swallow). **No main-DB V-number needed.**
- **`recall_semantic`** (`memory_bucket_seal/adapter.rs:~316-401`): scans `ts::list_trees_by_kind` → `ts::list_summaries_at_level(&self.store, &tree.id, level)` → per summary `node`: `cos = cosine_similarity(&qvec, emb)` → `scored.push((cos, MemoryEntry { id: node.id, key: node.id, content, namespace: tree.scope, timestamp: node.sealed_at.to_rfc3339(), score: Some(cos as f64) }))`. Then sort by cos desc, truncate to `limit`. `node.sealed_at` is a `chrono::DateTime` on the summary struct. `self.recall_max_scan` is an adapter field set at boot from `MemoryOsConfig` — MIRROR this for the new config fields.
- **Summary struct** (`memory_bucket_seal/types.rs:~216`, `pub id: String` + `sealed_at`, `score`, `embedding`, etc.): B adds `recall_hit_count: i64` (+ `last_recalled_at_ms: Option<i64>`). The summary read fn (`list_summaries_at_level` / the row→struct mapper) + the insert fn (`insert_summary_tx`) must map the new columns.
- **`recall_hybrid`** (`adapter.rs:~407`): semantic + FTS backfill, dedup by id. **Stays pure-read.**
- **`load_context`** (`memory_adapter/router.rs:~261`): Slice A — when `default_backend=="bucket_seal"` + `Some(bucket_seal)`, calls `bs.recall_hybrid(query, None, 6)`. Holds the concrete `Arc<BucketSealAdapter>`. This is where reinforce fires (after recall). `MemoryEntry.id` of semantic entries = summary id; FTS entries' id = chunk id → `reinforce_recalled`'s `WHERE id IN (...)` over `mem_tree_summaries` naturally no-ops on chunk ids (pass ALL recalled ids, only summaries match).
- **reflection recall-before-memorize** (`memory_graph/reflection.rs:~507`): calls `recall_hybrid` directly (dedup gate) — must NOT reinforce (that's why reinforce is in load_context, not recall_hybrid).
- **config**: `memubot_config.rs` memory_os fields + defaults + serde tests; `MemoryOsRuntimeConfig` (proactive/shared); the adapter gets `recall_max_scan` from `MemoryOsConfig` at construction (find the BucketSealAdapter build site — `app.rs` / a builder).
- **recency reference**: `memory_graph/recall.rs::time_decay_score` (`:49`, Gaussian `exp(-(age/half_life)^2)`). B uses simple exp-decay `exp(-age_days/half_life)` (gentler long tail) — pin in T3.

---

## Task 1: bucket_seal hotness columns + Summary struct + `reinforce_recalled`

**Files:** `memory_bucket_seal/store.rs` (ensure_schema ALTERs), `memory_bucket_seal/types.rs` (Summary struct), the summary read/insert fns (grep `list_summaries_at_level` / `insert_summary_tx` — likely `store.rs` or a `tree_store.rs`/`ts` module), `memory_bucket_seal/adapter.rs` (`reinforce_recalled`).

- [ ] **Step 1: Columns (idempotent ALTER in ensure_schema).** In `BucketSealStore::ensure_schema` (`store.rs:228`), after the `CREATE TABLE ... mem_tree_summaries` batch, add idempotent column adds:
```rust
// B3 hotness (openhuman-B): added via ALTER for existing DBs. CREATE TABLE IF NOT
// EXISTS won't add columns to a pre-existing table, so ALTER + swallow the
// "duplicate column name" error on re-run (bucket_seal has no version table).
for stmt in [
    "ALTER TABLE mem_tree_summaries ADD COLUMN recall_hit_count INTEGER NOT NULL DEFAULT 0",
    "ALTER TABLE mem_tree_summaries ADD COLUMN last_recalled_at_ms INTEGER",
] {
    if let Err(e) = conn.execute(stmt, []) {
        let msg = e.to_string();
        if !msg.contains("duplicate column name") { return Err(e.into()); }
    }
}
```
(Adapt to ensure_schema's exact conn handle + error type. Confirm fresh-DB path: the CREATE TABLE doesn't include these columns, so the ALTER adds them on both fresh and existing DBs — OR add them to the CREATE TABLE too AND keep the ALTER for existing DBs; the ALTER-only-with-swallow approach works for both since a fresh table lacks them. Simplest: ALTER-with-swallow for both.)
- [ ] **Step 2: Summary struct.** Add `pub recall_hit_count: i64` + `pub last_recalled_at_ms: Option<i64>` to the in-memory summary struct (`types.rs:~216`). Update the row→struct mapper in `list_summaries_at_level` (+ any other summary SELECT) to read the 2 columns (default 0/None if the SELECT predates them — but after Step 1 they always exist). Update `insert_summary_tx` to write them (new summaries: `recall_hit_count=0, last_recalled_at_ms=NULL`). Add the columns to the SELECT/INSERT column lists.
- [ ] **Step 3: `reinforce_recalled`** on `BucketSealAdapter` (`adapter.rs`):
```rust
/// Reinforcement-on-access (openhuman-B): bump recall_hit_count + last_recalled_at_ms
/// for the given summary ids. Best-effort; chunk ids (FTS leg) that aren't summary
/// ids simply don't match. Called ONLY from load_context (the agent-context path) —
/// NOT from recall_hybrid, so dedup/internal recalls don't inflate hotness.
pub async fn reinforce_recalled(&self, summary_ids: &[String], now_ms: i64) -> anyhow::Result<()> {
    if summary_ids.is_empty() { return Ok(()); }
    let conn = self.store.conn.lock()... ;   // mirror the lock idiom used by other adapter writes
    let placeholders = std::iter::repeat("?").take(summary_ids.len()).collect::<Vec<_>>().join(",");
    let sql = format!("UPDATE mem_tree_summaries SET recall_hit_count = recall_hit_count + 1, last_recalled_at_ms = ? WHERE id IN ({})", placeholders);
    let mut params: Vec<&dyn rusqlite::ToSql> = vec![&now_ms];
    for id in summary_ids { params.push(id); }
    conn.execute(&sql, params.as_slice())?;
    Ok(())
}
```
(Adapt the conn-lock + ToSql binding to the adapter's actual pattern — read an existing adapter write method. `now_ms` bound first, then the ids.)
- [ ] **Step 4: Tests** (in `adapter.rs` / store tests, full bucket_seal fixture via `ensure_schema`): insert 2 summaries → `reinforce_recalled(&["s1".into()], now)` → `s1.recall_hit_count==1` + `last_recalled_at_ms==now`, `s2` unchanged; calling again → `s1==2`; an unknown id → no-op (no error). `ensure_schema` idempotent (run twice, no error). Round-trip: insert_summary then list → the 2 new fields present (default 0/None).
- [ ] **Step 5: Build + test + commit** — `cd src-tauri && cargo build 2>&1 | grep -E "^error"` (empty); `cargo test --lib memory_bucket_seal 2>&1 | tail`. Commit: `feat(memory): bucket_seal mem_tree_summaries hotness columns + reinforce_recalled (openhuman-B)`

---

## Task 2: config knobs (recency / hotness / reinforcement)

**Files:** `memubot_config.rs` (+ serde tests), `MemoryOsRuntimeConfig` (proactive/shared), the BucketSealAdapter construction site (thread the 2 ranking knobs as adapter fields like `recall_max_scan`).

- [ ] **Step 1: memubot_config fields.** Add to the memory_os config (mirror `importance_decay_*` / `recall_*` pattern — field + `#[serde(default="...")]` + default fn):
  - `recall_recency_half_life_days: f64` (default `30.0`)
  - `recall_hotness_weight: f64` (default `0.3`)
  - `recall_reinforcement_enabled: bool` (default `true`)
  Update defaults + the serde default tests.
- [ ] **Step 2: Thread the 2 ranking knobs into BucketSealAdapter.** Find where the adapter is built with `recall_max_scan` from `MemoryOsConfig` (grep `recall_max_scan` — the builder / `app.rs`). Add `recency_half_life_days: f64` + `hotness_weight: f64` adapter fields, set from the config at construction (mirror recall_max_scan exactly). Default-safe if config absent (30.0 / 0.3).
- [ ] **Step 3: Thread `recall_reinforcement_enabled` into `MemoryOsRuntimeConfig`** (the struct the proactive/load_context path reads — find where `unified_load_context_enabled` or `importance_decay_enabled` lives; add the field + all constructors: from_memubot_config / for_tests / Default).
- [ ] **Step 4: Build + test + commit** — clean; `cargo test --lib memubot_config 2>&1 | tail`. Commit: `feat(config): recall recency/hotness/reinforcement knobs (openhuman-B)`

---

## Task 3: recency + hotness in `recall_semantic` ranking

**Files:** `memory_bucket_seal/adapter.rs` (`recall_semantic`).

- [ ] **Step 1: Apply recency + hotness to the score.** In `recall_semantic` (`adapter.rs:~373`), replace `score: Some(cos as f64)` with an enriched score. Compute `now_ms` once at fn top (`chrono::Utc::now().timestamp_millis()`). Per summary:
```rust
let age_days = ((now_ms - node.sealed_at.timestamp_millis()).max(0) as f64) / 86_400_000.0;
let recency = (-(age_days / self.recency_half_life_days)).exp();          // exp-decay, 1.0 fresh→0 old
let hotness = 1.0 + self.hotness_weight * ((1.0 + node.recall_hit_count as f64).ln());  // log-scaled
let final_score = (cos as f64) * recency * hotness;
```
Use `final_score` for both the sort key and `MemoryEntry.score`. Guard: if `recency_half_life_days <= 0.0` skip recency (factor 1.0); `node.sealed_at` always present. (Keep the existing cosine + the sort-by-score-desc + truncate; only the per-entry score changes. The `scored` vec sorts by `final_score` now.)
- [ ] **Step 2: Tests** (full bucket_seal fixture):
  - Recency: two summaries, IDENTICAL embedding (equal cosine to the query) but different `sealed_at` (one now, one 90 days ago) → recall_hybrid/recall_semantic ranks the fresher first. (Use a `FakeVecEmbedder` like the existing `recall_semantic_ranks_by_cosine` test; set sealed_at explicitly.)
  - Hotness: two summaries, equal cosine + equal sealed_at, different `recall_hit_count` (0 vs 10) → the hotter ranks first.
  - hotness_weight=0 → hotness factor is 1.0 (no effect), recency still applies.
- [ ] **Step 3: Build + clippy + test + commit** — clean; `cargo test --lib memory_bucket_seal::adapter 2>&1 | tail`. Commit: `feat(memory): recency-decay + log-scaled hotness in recall_semantic ranking (openhuman-B)`

---

## Task 4: reinforce write-back wired into `load_context`

**Files:** `memory_adapter/router.rs` (`load_context`), its caller `tauri_commands.rs:~1138` (pass the reinforcement gate if needed).

- [ ] **Step 1: Wire reinforce into load_context.** In `load_context` (`router.rs`), after the `bucket_seal` `recall_hybrid` call returns `hits` (the bucket_seal branch from Slice A), when reinforcement is enabled, fire-and-forget bump the recalled summary ids:
```rust
// (inside the `if use_hybrid` arm, after `let hits = bs.recall_hybrid(...).await;`)
if reinforcement_enabled {
    let ids: Vec<String> = hits.iter().map(|e| e.id.clone()).collect();
    let now_ms = chrono::Utc::now().timestamp_millis();
    if let Err(e) = bs.reinforce_recalled(&ids, now_ms).await {
        tracing::debug!(error = %e, "load_context: reinforce_recalled failed (best-effort)");
    }
    all.extend(hits);
} else {
    all.extend(hits);
}
```
   `load_context` needs the `reinforcement_enabled` bool — add it as a param (like Slice A added `bucket_seal`), OR read it from a config the caller passes. Cleanest: add `reinforce: bool` param to `load_context`; the caller (`tauri_commands.rs:1138`) passes `state.memubot_config...memory_os.recall_reinforcement_enabled` (or the MemoryOsRuntimeConfig field). Update the load_context tests to pass `false` (no reinforce in the legacy/stub tests). **reinforce_recalled's `WHERE id IN` no-ops on FTS chunk ids — passing all hit ids is correct (only summary ids match).**
- [ ] **Step 2: Confirm reflection's recall-before-memorize does NOT reinforce.** It calls `recall_hybrid` directly (`reflection.rs:~507`), not `load_context` → it never calls `reinforce_recalled`. Verify by grep (`reinforce_recalled` only referenced in load_context + tests). No code change — just confirm + note.
- [ ] **Step 3: Test** — `load_context(..., reinforce=true)` over a bucket_seal with a recallable summary → after the call, that summary's `recall_hit_count` is bumped; `load_context(..., reinforce=false)` → not bumped; a direct `recall_hybrid` call (simulating reflection) → not bumped. (Full bucket_seal fixture; assert the column post-call.)
- [ ] **Step 4: Build + clippy + test + commit** — clean; `cargo test --lib memory_adapter::router memory_bucket_seal 2>&1 | tail`. Commit: `feat(memory): reinforce recalled summaries from load_context (agent-context only, best-effort) (openhuman-B)`

---

## Task 5: Whole-slice verification + ship

- [ ] **Step 1:** `cargo build` + `cargo clippy --lib` clean; `cargo test --lib memory_bucket_seal memory_adapter proactive memubot_config 2>&1 | grep "test result:"` green. **Broad dependent run (Slice-C lesson):** any test fixture building a bucket_seal store + reading summaries must tolerate the new columns — since they're added in `ensure_schema` (which all bucket_seal fixtures call), this should be automatic, but RUN the broad suite to confirm (no `no such column: recall_hit_count`).
- [ ] **Step 2: Integration sanity (test):** seed two equal-cosine summaries (fresh vs old) → recall ranks fresh first; reinforce the old one 10× → it now out-ranks via hotness (recency vs hotness interplay). Deterministic `now_ms`.
- [ ] **Step 3: Gates:** `grep -rn "recall_hit_count\|reinforce_recalled\|recency_half_life\|hotness_weight" src/` (wired). `recall_semantic` score includes recency+hotness. reinforce only in load_context.
- [ ] **Step 4: Ship** — push → PR (Commits table T1-T4) → rebase-merge → sync → cleanup → reindex.
- [ ] **Step 5: Post-merge soak (manual):** ask the agent something → confirm recently-learned/recalled facts rank higher in `<memory_context>` over time; a repeatedly-relevant fact gets reinforced (recall_hit_count climbs) and stays surfaced; an old never-recalled fact sinks.

---

## Self-Review

- **Spec coverage:** §1 recency→T3; §2 hotness columns→T1; §3 hotness ranking→T3; §4 reinforce write-back→T1(method)+T4(load_context wiring); §6 config→T2. ✓ (§B2 deferred — not in plan.)
- **Ordering compiles:** schema+struct+reinforce_recalled (T1) → config (T2) → recency+hotness ranking (T3, uses T1 columns + T2 adapter knobs) → load_context reinforce (T4, uses T1 method + T2 gate). Each builds. ✓
- **Type consistency:** `recall_hit_count: i64` / `last_recalled_at_ms: Option<i64>` (struct + columns); `reinforce_recalled(&[String], i64)`; adapter fields `recency_half_life_days`/`hotness_weight: f64`; config `recall_recency_half_life_days`/`recall_hotness_weight`/`recall_reinforcement_enabled`; `MemoryEntry.id` = summary id (the reinforce target). Consistent. ✓
- **No placeholders:** real SQL + the idempotent-ALTER mechanism (bucket_seal has no V-runner — corrected from the spec's "V58") + the scoring formula + the pass-all-ids/WHERE-IN-no-op insight + reinforce-only-in-load_context. Flagged impl points: ensure_schema conn/error idiom (T1), adapter-config threading mirror recall_max_scan (T2), exp-vs-Gaussian recency (T3 — chose exp), the summary read/insert mapper location (T1). ✓
- **Slice-C lesson applied:** new summary columns are added in `ensure_schema` (which every bucket_seal fixture calls) so fixtures don't break; T5 still runs the broad suite to confirm. ✓
- **Finish-line:** after B, recall favors fresh + frequently-recalled memories; surfacing a memory in agent context reinforces it (load_context only); dedup/internal recalls don't. ✓
