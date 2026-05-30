# 阶段 4 PR13 — Durable Job Queue + Worker Pool + Scheduler Design Spec

**Status:** Approved design — pending user review gate before plan.
**Date:** 2026-05-30
**Position in 阶段 4 sequence:** PR13 of 15. Follows PR12 (model swap). Precedes PR14 (GbrainAdapter), PR15 (recall wiring).

---

## 1. Goal

Replace PR12's fire-and-forget `tokio::spawn(cascade_all_from)` interim with a **durable, SQLite-backed job queue** that survives crashes, retries failures, dedupes in-flight work, and drives a daily scheduler. Port openhuman's `jobs/` subsystem — adapted to uClaw's reality, where canonicalize/score/admit/buffer-append/topic-route are already synchronous (fast, no LLM), so only the **slow LLM work** needs queuing.

uClaw needs **3 job kinds** (not openhuman's 6):
- **`Seal`** — run the full cascade-seal for one tree (the slow LLM summarise + embed). Enqueued atomically by `store()`; replaces PR12's detached spawn.
- **`DigestDaily`** — run the cross-source end-of-day digest (PR11). Enqueued by the scheduler (and the existing manual IPC).
- **`FlushStale`** — walk buffers and enqueue `Seal` jobs for any that accumulated leaves but never crossed the token gate (low-volume trees). The completeness safety net.

**The durability win:** `store()` enqueues a `Seal` job in the **same transaction** as the buffer write, so a crash between them is impossible. A crashed worker mid-cascade leaves the job `running` with an expired lease; `recover_stale_locks` requeues it on the next startup/poll. No seal is ever lost (PR12's spawn could vanish on crash).

**Out of scope (deferred):**
- ExtractChunk / AppendBuffer / TopicRoute jobs — uClaw does these synchronously in `store()`; no queuing needed.
- GbrainAdapter → PR14. Semantic recall wiring → PR15.
- openhuman's `scheduler_gate` (Throttled/Paused modes) — uClaw has no such global mode; the LLM semaphore alone bounds concurrency.

---

## 2. Why this slice

Brainstorming chose "Full — Seal + DigestDaily + FlushStale + scheduler". The synchronous path (PR8-12) works; PR13 hardens the one async seam (PR12's detached cascade) into a durable queue and adds the automatic daily digest + stale-buffer recovery. The store/worker/dedupe/lease machinery is kind-agnostic and ports mostly whole; only 3 handlers + the scheduler tick are uClaw-specific.

---

## 3. Components

### 3.1 Schema — `mem_tree_jobs` (in `BucketSealStore` SCHEMA)

Added to the existing `chunks.db` SCHEMA constant (NOT uClaw's main `migrations.rs` — bucket_seal owns its own db, consistent with PR5-12). This co-location is what lets `store()` enqueue a follow-up job in the same tx as its side-effect.

```sql
CREATE TABLE IF NOT EXISTS mem_tree_jobs (
    id              TEXT PRIMARY KEY,
    kind            TEXT NOT NULL,            -- 'seal' | 'digest_daily' | 'flush_stale'
    payload_json    TEXT NOT NULL,
    dedupe_key      TEXT NOT NULL,
    status          TEXT NOT NULL,            -- 'ready'|'running'|'done'|'failed'|'cancelled'
    attempts        INTEGER NOT NULL DEFAULT 0,
    max_attempts    INTEGER NOT NULL,
    available_at_ms INTEGER NOT NULL,         -- earliest claim time (retry backoff)
    locked_until_ms INTEGER,                  -- lease expiry while running
    last_error      TEXT,
    created_at_ms   INTEGER NOT NULL,
    started_at_ms   INTEGER,
    completed_at_ms INTEGER
);

-- At-most-one-active per dedupe_key: enforces per-tree Seal serialisation +
-- per-day digest idempotency. Partial so terminal rows don't block re-enqueue.
CREATE UNIQUE INDEX IF NOT EXISTS idx_mem_tree_jobs_dedupe_active
    ON mem_tree_jobs(dedupe_key)
    WHERE status IN ('ready', 'running');

-- Claim scan: ready jobs whose available_at_ms has come due, oldest first.
CREATE INDEX IF NOT EXISTS idx_mem_tree_jobs_claim
    ON mem_tree_jobs(status, available_at_ms);
```

### 3.2 `jobs/types.rs`

```rust
pub enum JobKind { Seal, DigestDaily, FlushStale }   // as_str/parse, snake_case
pub enum JobStatus { Ready, Running, Done, Failed, Cancelled }  // + is_terminal()

impl JobKind {
    /// True for kinds that call the LLM summariser/embedder — these acquire
    /// the LLM concurrency permit before claiming. FlushStale is pure-SQL.
    pub fn is_llm_bound(&self) -> bool {
        matches!(self, JobKind::Seal | JobKind::DigestDaily)
    }
}

pub struct Job { id, kind, payload_json, dedupe_key, status, attempts, max_attempts,
                 available_at_ms, locked_until_ms, last_error, created_at_ms,
                 started_at_ms, completed_at_ms }

pub struct NewJob { id, kind, payload_json, dedupe_key, max_attempts: Option<u32> }
impl NewJob { fn seal(&SealPayload) -> Result<Self>; fn digest_daily(&DigestDailyPayload) -> Result<Self>; fn flush_stale(&FlushStalePayload) -> Result<Self> }

pub struct SealPayload { pub tree_id: String, pub from_level: u32 }
impl SealPayload { pub fn dedupe_key(&self) -> String { format!("seal:{}", self.tree_id) } }

pub struct DigestDailyPayload { pub date: String }  // "YYYY-MM-DD"
impl DigestDailyPayload { pub fn dedupe_key(&self) -> String { format!("digest:{}", self.date) } }

pub struct FlushStalePayload { pub date: String }  // bucketed per-day so reruns don't pile up
impl FlushStalePayload { pub fn dedupe_key(&self) -> String { format!("flush:{}", self.date) } }
```

- `dedupe_key` is the serialisation/idempotency lever: one active `Seal` per `tree_id`, one `DigestDaily` per date, one `FlushStale` per date.

### 3.3 `jobs/store.rs`

Kind-agnostic queue persistence. Ports openhuman's store ~whole, swapping `Config`/`with_connection` for `&BucketSealStore` + `lock_conn()`/`conn.transaction()` (the PR8 idiom).

```rust
pub const DEFAULT_LOCK_DURATION_MS: i64 = 5 * 60 * 1000;  // 5-min lease
pub const DEFAULT_MAX_ATTEMPTS: u32 = 5;

/// Enqueue (own tx). Idempotent on dedupe_key while an active row shares it.
pub fn enqueue(store: &BucketSealStore, job: &NewJob) -> Result<Option<String>>;
/// Enqueue inside a caller's tx — the atomic producer entry point used by
/// BucketSealAdapter.store() to commit a buffer write + its Seal job together.
pub fn enqueue_tx(tx: &rusqlite::Transaction<'_>, job: &NewJob) -> Result<Option<String>>;
/// Atomically claim the next due ready job: single UPDATE ... RETURNING.
pub fn claim_next(store: &BucketSealStore, lock_duration_ms: i64) -> Result<Option<Job>>;
/// Settle. Gated on (id, attempts/started) so a stale worker's settle is a no-op.
pub fn mark_done(store: &BucketSealStore, job: &Job) -> Result<()>;
/// Failure → retry with exponential backoff (min(base·2^attempts, cap)) until
/// max_attempts, then status='failed'.
pub fn mark_failed(store: &BucketSealStore, job: &Job, err: &str) -> Result<()>;
/// Voluntary requeue (e.g. transient unconfigured-provider) without burning an attempt.
pub fn mark_deferred(store: &BucketSealStore, job: &Job, retry_after_ms: i64) -> Result<()>;
/// Startup: any 'running' row whose lease expired → back to 'ready'.
pub fn recover_stale_locks(store: &BucketSealStore) -> Result<usize>;
pub fn get_job(store: &BucketSealStore, id: &str) -> Result<Option<Job>>;
pub fn count_by_status(store: &BucketSealStore) -> Result<Vec<(JobStatus, u64)>>;
```

- `claim_next` is the one statement `UPDATE mem_tree_jobs SET status='running', attempts=attempts+1, started_at_ms=?, locked_until_ms=? WHERE id=(SELECT id FROM mem_tree_jobs WHERE status='ready' AND available_at_ms<=? ORDER BY available_at_ms LIMIT 1) RETURNING ...`. SQLite serialises writes, so two workers can't claim the same row.

### 3.4 `jobs/handlers.rs`

```rust
pub async fn handle_job(
    store: &Arc<BucketSealStore>,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    job: &Job,
) -> Result<()> {
    match job.kind {
        JobKind::Seal       => handle_seal(...),        // load tree by payload.tree_id →
                                                        //   cascade_all_from(store, tree, from_level,
                                                        //     summariser, embedder, None, &LabelStrategy::Empty)
        JobKind::DigestDaily=> handle_digest(...),      // parse date → end_of_day_digest(store, date, summariser, embedder)
        JobKind::FlushStale => handle_flush(...),       // walk all trees' buffers → for each stale buffer
                                                        //   (Buffer::is_stale(now, DEFAULT_FLUSH_AGE_SECS)),
                                                        //   enqueue Seal{tree_id, from_level: buffer.level}
    }
}
```

- **`Seal`**: `get_tree(store, &payload.tree_id)?` (skip+done if the tree vanished) → `cascade_all_from`. One job = full multi-level cascade (uClaw's `cascade_all_from` already loops levels — no per-level follow-up jobs, simpler than openhuman).
- **`DigestDaily`**: `end_of_day_digest` is itself idempotent (PR11's `find_existing_daily`), so a duplicate digest job is a safe no-op (returns `Skipped`).
- **`FlushStale`**: enumerate trees (`list_trees_by_kind` × {Source, Topic, Global}), read each buffer level, and for any stale buffer enqueue a `Seal` (deduped on `tree_id`, so it merges with any pending real seal). Pure-SQL except the enqueues — not LLM-bound.

### 3.5 `jobs/worker.rs` — Stage-3 `ManagedService`

```rust
pub struct JobWorkerService {
    store: Arc<BucketSealStore>,
    summariser: Arc<dyn Summariser>,
    embedder: Arc<dyn Embedder>,
    worker_count: usize,         // e.g. 2
    llm_permits: Arc<Semaphore>, // e.g. 2 — bounds concurrent LLM-bound jobs
}

#[async_trait] impl ManagedService for JobWorkerService {
    fn name(&self) -> &str { "memory_jobs_worker" }
    async fn start(&self) -> Result<()> {
        recover_stale_locks(&self.store)?;        // requeue crashed leases
        for _ in 0..self.worker_count { tokio::spawn(worker_loop(...)); }
        Ok(())                                     // start() returns; loops run detached
    }
    async fn stop(&self) -> Result<()> { /* signal loops to exit */ }
}

// worker_loop: poll claim_next (LLM permit acquired BEFORE claim for is_llm_bound
// kinds so a flush job never holds a slot); on Some(job) → handle_job → mark_done/failed;
// on None → sleep(POLL_INTERVAL). Errors logged, never panic the loop.
pub async fn run_once(...) -> Result<bool>;       // claim+handle one job; for tests
```

- **Test seam**: `run_once` + a `drain_until_idle(store, summariser, embedder)` helper (in `jobs/testing.rs`) that loops `run_once` until no job is claimable — deterministic, no sleeps, for hermetic tests.

### 3.6 `jobs/scheduler.rs` — Stage-3 `ManagedService`

```rust
pub struct JobSchedulerService { store: Arc<BucketSealStore> }
#[async_trait] impl ManagedService for JobSchedulerService {
    fn name(&self) -> &str { "memory_jobs_scheduler" }
    async fn start(&self) -> Result<()> {
        tokio::spawn(async move { loop {
            enqueue_daily_jobs(&store);   // digest_daily(yesterday) + flush_stale(today)
            sleep(next_tick_duration()).await;  // sleep until next UTC 00:05
        }});
        Ok(())
    }
}
pub fn trigger_digest(store, date) -> Result<Option<String>>;          // manual enqueue
pub fn backfill_missing_digests(store, days_back) -> Result<Vec<String>>;
```

- Tick at UTC 00:05: `enqueue(digest_daily(yesterday))` + `enqueue(flush_stale(today))`. Both deduped, so a missed/duplicated tick is harmless.

### 3.7 `adapter.rs` change — `store()` enqueues instead of spawns

Replace PR12's per-chunk detached cascade with an atomic enqueue:

```rust
// Source (Phase A), inside the same tx that append_leaf_deferred uses — OR
// immediately after, via store-level enqueue. Preferred: enqueue_tx in the
// buffer-append tx so commit is atomic.
let gate_met = append_leaf_deferred(&self.store, &source_tree, &leaf)?;
if gate_met {
    enqueue(&self.store, &NewJob::seal(&SealPayload { tree_id: source_tree.id.clone(), from_level: 0 })?)?;
}
// Topic (Phase B): same, deduped on the topic tree_id.
```

- **Removes** the `tokio::spawn` + the per-tree `Arc<Mutex<()>>` resolution from `store()`. The dedupe index gives per-tree serialisation (one active Seal per tree); the worker runs the cascade.
- **Removes** `summariser`/`embedder` use from the hot path entirely — `store()` no longer needs them for sealing (the worker holds them). They stay adapter fields only for the digest IPC path / construction symmetry.
- **Atomicity nuance**: the cleanest form enqueues via `enqueue_tx` *inside* `append_leaf_deferred`'s transaction. Since `append_leaf_deferred` currently opens+commits its own tx internally, the plan will either (a) add an `append_leaf_deferred_tx` variant that takes a `&Transaction` and enqueues in the same tx, or (b) accept a tiny window by calling `enqueue` right after `append_leaf_deferred` returns (a crash in that window leaves the buffer written but no job — recovered by `FlushStale`). The plan picks (a) if the refactor is small, else (b) with the flush safety net documented. **(b) is acceptable** because FlushStale guarantees eventual sealing regardless.

### 3.8 `app.rs` wiring

Register both services in the Stage-3 block, after the bucket_seal adapter is built (so the store + summariser + embedder are available):

```rust
service_manager.register(Arc::new(JobWorkerService::new(
    bucket_seal_store.clone(), bucket_seal_summariser.clone(), bucket_seal_embedder.clone(),
))).await;
service_manager.register(Arc::new(JobSchedulerService::new(bucket_seal_store.clone()))).await;
```

- Requires keeping `Arc<BucketSealStore>` + the embedder + summariser handles reachable at Stage-3 (PR12 moved them into the adapter; PR13 keeps clones at boot, or exposes them via the concrete `bucket_seal_adapter` handle PR11 added).

### 3.9 IPC

- `memory_jobs_status() -> Vec<(String kind_or_status, u64 count)>` — ops/testing visibility into the queue. Defined in `tauri_commands.rs` + registered in `main.rs` `invoke_handler!`.
- `memory_global_digest_run` (PR11) — keep working. It can either stay synchronous (direct `run_global_digest`) OR enqueue a `digest_daily` job. **Decision: keep it synchronous** (it's a manual "run now and tell me the result" affordance; the scheduler covers the automatic path). No change to PR11's behavior.

---

## 4. The central subtlety: dedupe-as-serialisation + eventual consistency

The partial unique index means **at most one active `Seal` job per tree**. This replaces PR12's per-tree mutex and is the per-tree serialisation guarantee. Consequence to document explicitly:

> If a buffer crosses its seal threshold *while* that tree's `Seal` job is already `running`, the new `enqueue` is ignored (dedupe). The running `cascade_all_from` re-reads buffers and seals whatever is present at read time; leaves that land *after* its read but *before* completion won't seal until the **next** `store()` enqueues a fresh `Seal` (the prior job is now terminal, dedupe clear). In the rare case no further write arrives, **`FlushStale`** seals the lingering buffer once it ages past `DEFAULT_FLUSH_AGE_SECS`. Net: sealing is **eventually consistent**, never lost, never racing. Buffers are durable throughout.

---

## 5. Error handling & retry

| Failure | Behavior |
|---|---|
| Handler returns `Err` (LLM down, embed dim-mismatch, etc.) | `mark_failed` → exponential backoff requeue until `max_attempts` (5), then `status='failed'` (terminal, `last_error` recorded). |
| Worker crashes mid-job | Lease (`locked_until_ms`) expires; `recover_stale_locks` (startup + periodic) flips it back to `ready`. The `mark_done`/`mark_failed` claim-token gate makes a resurrected stale worker's late settle a no-op. |
| Unconfigured provider (no ingestion LLM) | Handler errors → `mark_failed` backoff. (Could `mark_deferred` instead to avoid burning attempts — plan decides; `mark_failed` is simpler and the daily scheduler re-enqueues digests anyway.) |
| Duplicate enqueue (same dedupe_key active) | `INSERT OR IGNORE` no-ops; `enqueue` returns `Ok(None)`. |
| Digest for an already-digested day | Handler runs `end_of_day_digest` which returns `Skipped` (PR11 idempotency) → `mark_done`. |

Guiding principle (unchanged from PR12): **memory is best-effort and never blocks/breaks the primary write.** The queue adds durability + retry on top.

---

## 6. Testing

| Area | Tests |
|---|---|
| `store.rs` | enqueue → claim → mark_done lifecycle; dedupe (second active enqueue → None); claim respects `available_at_ms`; mark_failed backoff + eventual `failed`; recover_stale_locks requeues an expired lease; claim-token gate (stale settle no-ops). ~7 tests. |
| `types.rs` | JobKind/JobStatus round-trip; dedupe_key formats; is_llm_bound. ~3 tests. |
| handlers | seal handler runs a cascade (seed a tree at threshold → drain → summary sealed); digest handler emits a daily (reuse PR11 fixture); flush handler enqueues Seal for a stale buffer. ~3 tests. |
| worker/`drain_until_idle` | enqueue N jobs → `drain_until_idle` → all `done`; a failing handler → `failed` after max_attempts. ~2 tests. |
| `adapter.rs` | `store()` enqueues a Seal job (assert a `seal:{tree_id}` row exists, status ready) instead of running inline; `store()` still returns fast. ~2 tests. |
| scheduler | `enqueue_daily_jobs` creates digest+flush rows; `trigger_digest` idempotent. ~2 tests. |

Hermetic: tests use `FakeLlmProvider`-backed summariser (PR12) + `InertEmbedder`, and `drain_until_idle` (no wall-clock sleeps). No live models, no real scheduler timing.

---

## 7. Scope boundaries (what PR13 does NOT do)

- **No ExtractChunk / AppendBuffer / TopicRoute jobs** — uClaw does these synchronously in `store()`.
- **No `scheduler_gate`** (openhuman's Throttled/Paused) — the LLM semaphore bounds concurrency.
- **No GbrainAdapter** (PR14), **no semantic recall wiring** (PR15).
- **No change to PR11's `memory_global_digest_run`** behavior (stays synchronous).
- **No main `migrations.rs` entry** — `mem_tree_jobs` lives in the bucket_seal `chunks.db` SCHEMA.
- **No removal of `append_leaf` / `cascade_all_from`** — the synchronous primitives stay (used by the seal handler + tests); only `store()`'s *call site* changes from spawn to enqueue.

---

## 8. File plan (preview — detailed in the implementation plan)

| File | New/Mod | Purpose |
|---|---|---|
| `memory_bucket_seal/jobs/mod.rs` | new | module surface + re-exports |
| `memory_bucket_seal/jobs/types.rs` | new | JobKind/JobStatus/Job/NewJob/payloads |
| `memory_bucket_seal/jobs/store.rs` | new | mem_tree_jobs persistence (enqueue/claim/mark/recover/count) |
| `memory_bucket_seal/jobs/handlers.rs` | new | seal / digest_daily / flush_stale handlers |
| `memory_bucket_seal/jobs/worker.rs` | new | `JobWorkerService` + worker_loop + run_once |
| `memory_bucket_seal/jobs/scheduler.rs` | new | `JobSchedulerService` + trigger/backfill |
| `memory_bucket_seal/jobs/testing.rs` | new | `drain_until_idle` |
| `memory_bucket_seal/store.rs` | mod | add `mem_tree_jobs` table + indexes to SCHEMA |
| `memory_bucket_seal/mod.rs` | mod | `pub mod jobs;` + re-exports |
| `memory_bucket_seal/adapter.rs` | mod | `store()` enqueue-instead-of-spawn; drop per-tree mutex from hot path |
| `app.rs` | mod | register worker + scheduler Stage-3 services |
| `tauri_commands.rs` + `main.rs` | mod | `memory_jobs_status` IPC + registration |

Est. ~1400 source + ~450 tests = ~1850 LoC.

---

## 9. Open adaptation questions (resolved at implementation time)

1. **`append_leaf_deferred_tx` variant?** — whether to refactor `append_leaf_deferred` to accept a `&Transaction` for a truly-atomic enqueue (§3.7 option a), or accept the tiny window with FlushStale recovery (option b). Plan decides based on the refactor size; (b) is acceptable.
2. **`ManagedService::stop` semantics** — how the worker/scheduler loops observe a shutdown signal (a `tokio_util::sync::CancellationToken` or an `AtomicBool`). Match the existing service pattern (read `services/power.rs`).
3. **Where the bucket_seal store/summariser/embedder handles live at Stage-3** — PR12 moved them into the adapter; PR13 needs them at service-registration time. Keep boot-time clones, or expose accessors on the concrete `bucket_seal_adapter` handle (PR11).
4. **Worker count + permit count** — start with `worker_count=2`, `llm_permits=2`. Tune later.
5. **Periodic `recover_stale_locks`** — startup-only (simplest) vs a periodic sweep in one worker. Startup-only is fine for PR13; note it.

---

## 10. Success criteria

- `store()` enqueues durable `Seal` jobs (atomic with the buffer write or with FlushStale backstop); no fire-and-forget spawn remains.
- A crash mid-cascade loses no seal — `recover_stale_locks` requeues it.
- The daily scheduler auto-enqueues digest + flush; the digest produces real cross-source summaries (via PR12's LLM summariser).
- `drain_until_idle` settles all jobs deterministically in tests; failing handlers reach `failed` after backoff.
- Per-tree serialisation holds via dedupe (no two concurrent Seal jobs for one tree).
- All existing `memory_bucket_seal` tests stay green; ~19 new tests pass. CI hermetic.
