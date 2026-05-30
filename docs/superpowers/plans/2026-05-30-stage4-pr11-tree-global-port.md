# 阶段 4 PR11 — `memory_bucket_seal::tree_global` port (cross-source daily digest) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port openhuman's Global Activity Digest tree — a singleton per-workspace tree whose L0 nodes are end-of-day cross-source digests, sealing upward into weekly (L1) / monthly (L2) / yearly (L3) recaps via a **count-based** cascade (7→4→12). Ship the write path (`end_of_day_digest`), the count-based cascade (`append_daily_and_cascade`), and the read path (`recap`). Expose via two `BucketSealAdapter` inherent methods + one manual-trigger Tauri IPC command. The automatic end-of-day scheduler is explicitly PR12's job.

**Architecture:** New `tree_global/` subsystem mirrors PR8's `tree_source/` but with three differences: (1) the tree is a **singleton** (scope=`"global"`), (2) the cascade trigger is **count-based per level** (7 daily→1 weekly, 4 weekly→1 monthly, 12 monthly→1 yearly) instead of token-budget-gated, (3) **every level holds summary nodes** — even L0 (a daily digest is a `SummaryNode`, not a raw chunk leaf). Reuses PR8's storage (`mem_tree_trees`/`mem_tree_summaries`/`mem_tree_buffers` with `kind='global'`), the `Summariser`/`Embedder` traits, and every tx-level store helper. No schema changes.

**Tech Stack:** Rust, `chrono` (NaiveDate/Duration/day-bounds), `uuid`, `rusqlite`, `anyhow`, `async-trait`, `tracing`. Reuses every PR5-10 dep. No new workspace deps.

---

## Source-of-truth references

uClaw files PR11 builds on top of (PR1-10 already merged):
- `src-tauri/src/memory_bucket_seal/tree_source/types.rs` — `Tree`, `SummaryNode`, `Buffer`, `TreeKind::{Source, Topic, Global}`, `TreeStatus`. **`TreeKind::Global` already exists.**
- `src-tauri/src/memory_bucket_seal/tree_source/store.rs` — every helper PR11 needs ALREADY EXISTS:
  - `get_tree_by_scope(store, kind, scope) -> Result<Option<Tree>>`
  - `insert_tree(store, &tree) -> Result<()>`
  - `get_tree(store, id) -> Result<Option<Tree>>`
  - `list_trees_by_kind(store, kind) -> Result<Vec<Tree>>`
  - `get_buffer(store, tree_id, level) -> Result<Buffer>`
  - `get_buffer_conn(&tx, tree_id, level) -> Result<Buffer>`
  - `upsert_buffer_tx(&tx, &buffer) -> Result<()>`
  - `clear_buffer_tx(&tx, tree_id, level) -> Result<()>`
  - `insert_summary_tx(&tx, &node) -> Result<()>` (single-arg — NO `.md` staging in uClaw)
  - `get_summary(store, id) -> Result<Option<SummaryNode>>`
  - `list_summaries_at_level(store, tree_id, level) -> Result<Vec<SummaryNode>>` (store.rs:298)
  - `update_tree_after_seal_tx(&tx, tree_id, summary_id, level, now) -> Result<()>`
  - `refresh_last_sealed_tx(&tx, tree_id, now) -> Result<()>`
- `src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs` — **THE port template**. PR11's `seal.rs` mirrors `append_to_buffer` (lines 178-207) + `seal_one_level` (lines ~292-460) EXACTLY, changing only the seal trigger + hydration source. Read it in full before writing seal.rs.
- `src-tauri/src/memory_bucket_seal/tree_source/registry.rs` — `get_or_create_source_tree` + `new_summary_id(level)` + `is_unique_violation`. Mirror for the global registry.
- `src-tauri/src/memory_bucket_seal/tree_source/summariser/mod.rs` — `Summariser` trait, `SummaryInput`, `SummaryContext`, `SummaryOutput`.
- `src-tauri/src/memory_bucket_seal/score/embed/mod.rs` — `Embedder` trait (`embed(&self, text) -> Result<Vec<f32>>`).
- `src-tauri/src/memory_bucket_seal/adapter.rs` (PR9/PR10) — `BucketSealAdapter` holds private `store: Arc<BucketSealStore>`, `embedder: Arc<dyn Embedder>`, `summariser: Arc<dyn Summariser>`. PR11 adds two inherent methods.
- `src-tauri/src/app.rs` — PR9 builds `bucket_seal_adapter` as `Arc<dyn MemoryAdapter>` and inserts into the map. PR11 changes this to keep a concrete `Arc<BucketSealAdapter>` in a new AppState field too.
- `src-tauri/src/tauri_commands.rs` + `src-tauri/src/main.rs` — IPC command definition + `invoke_handler!` registration (per CLAUDE.md: both required).

**openhuman reference (read-only, for fidelity):** `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_global/{mod,registry,seal,digest,recap}.rs`. NOTE openhuman uses `Config` + `with_connection` + `content_store::stage_summary` + `build_embedder_from_config`. uClaw uses `BucketSealStore` + `lock_conn()` + NO `.md` staging + injected `Arc<dyn Embedder>`. Port the LOGIC, not the uClaw-absent dependencies.

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/tree_global/mod.rs` (new) | Module surface + threshold constants (`WEEKLY_SEAL_THRESHOLD=7`, `MONTHLY_SEAL_THRESHOLD=4`, `YEARLY_SEAL_THRESHOLD=12`, `GLOBAL_SCOPE="global"`, `GLOBAL_TOKEN_BUDGET=4_000`) + re-exports | ~60 |
| `src-tauri/src/memory_bucket_seal/tree_global/registry.rs` (new) | `get_or_create_global_tree(store)` singleton (scope="global") + 3 tests | ~110 |
| `src-tauri/src/memory_bucket_seal/tree_global/seal.rs` (new) | `append_daily_and_cascade` + count-based cascade (`should_seal` per level) + `seal_one_level` (mirrors PR8) + `hydrate_summary_inputs` + 3 tests | ~330 |
| `src-tauri/src/memory_bucket_seal/tree_global/digest.rs` (new) | `end_of_day_digest(store, day, summariser, embedder)` + `DigestOutcome` enum + `day_bounds_utc` + `find_existing_daily` + `pick_source_contribution` + 4 tests | ~380 |
| `src-tauri/src/memory_bucket_seal/tree_global/recap.rs` (new) | `recap(store, window)` + `RecapOutput` + `pick_level` + `pick_covering` + `assemble_recap` + 4 tests | ~280 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +3 lines) | `pub mod tree_global;` + re-exports `get_or_create_global_tree`, `end_of_day_digest`, `DigestOutcome`, `recap`, `RecapOutput` | +3 |
| `src-tauri/src/memory_bucket_seal/adapter.rs` (modify, +30 lines) | `BucketSealAdapter::run_global_digest(&self, day) -> Result<DigestOutcome>` + `global_recap(&self, window) -> Result<Option<RecapOutput>>` inherent methods + 2 tests | +50 |
| `src-tauri/src/app.rs` (modify, ~12 lines) | Build `bucket_seal_adapter` as concrete `Arc<BucketSealAdapter>`; store in new AppState field `bucket_seal_adapter`; clone into the `Arc<dyn MemoryAdapter>` map | ~12 |
| `src-tauri/src/tauri_commands.rs` (modify, +35 lines) | `#[tauri::command] memory_global_digest_run(state, day: Option<String>) -> Result<GlobalDigestResult, String>` + `GlobalDigestResult` serde struct | +35 |
| `src-tauri/src/main.rs` (modify, +1 line) | Register `memory_global_digest_run` in `invoke_handler!` | +1 |

**LoC budget**: ~1160 source + ~450 tests = **~1010 source / ~1600 total**. In-band with PR8 (2814) and PR9 (1004).

---

## Decisions Already Locked (no more questions)

- **Singleton tree**: scope = literal `"global"`, `kind = TreeKind::Global`. `get_or_create_global_tree` mirrors PR10's `get_or_create_topic_tree` but with the fixed scope.
- **Count-based cascade thresholds**: L0→L1 at 7 daily nodes, L1→L2 at 4 weekly, L2→L3 at 12 monthly. Levels ≥ 3 never seal (yearly is the top).
- **Every level holds summaries** — even L0. `hydrate_summary_inputs` always pulls from `mem_tree_summaries` (never chunks). This is the key difference from PR8 where L0 holds chunks.
- **Backlink at ALL levels** (unlike PR8 which skips L0): in the global tree, L0 daily nodes are global-owned summary nodes, so sealing L0→L1 backlinks the daily nodes' `parent_id`. PR8 skips L0 backlink because its L0 children are chunks; global has no such exception. Backlink every `buf.item_ids` whose `parent_id IS NULL`.
- **No `.md` staging**: uClaw's `insert_summary_tx(&tx, &node)` is single-arg (PR8 dropped openhuman's content_store staging). PR11 follows — DB row only. Drop openhuman's `stage_summary`/`SummaryComposeInput`/`SummaryTreeKind`/`slugify_source_id` entirely.
- **No entity indexing in seal**: PR8's `seal_one_level` does NOT call `index_summary_entity_ids_tx` (openhuman does). PR11 follows PR8 — skip it. Entities/topics still flow via union into `node.entities`/`node.topics`.
- **Embedding before tx**: mirror PR8 exactly — compute `embedder.embed(&output.content).await?` BEFORE opening the write tx so an embed failure aborts the seal cleanly. Store via `node.embedding = Some(embedding)`.
- **Digest representative-material priority** (port openhuman's CODE, which is 2-tier, not the 3-tier doc-comment): (1) the latest summary intersecting the day window (`time_range_start_ms < day_end AND time_range_end_ms >= day_start`, `ORDER BY level DESC, sealed_at_ms DESC LIMIT 1`); (2) else `source_tree.root_id`; (3) else skip that tree (returns None). A tree with no sealed summaries contributes nothing.
- **L0 preview limitation inherited**: `node.content` is the ≤500-char preview (PR8 deferred full-body read). The digest folds previews. openhuman reads full body via `content_read::read_summary_body` — uClaw has no such function yet, so use `node.content` (preview) directly. Document as a PR12 follow-up (full-body read when content_store::read lands). Prefix each contribution with `[{source_scope}]\n` for provenance.
- **Idempotency**: `end_of_day_digest` checks for an existing L0 node whose `time_range_start_ms == day_start_ms` and returns `DigestOutcome::Skipped` if found. `append_to_buffer` is idempotent on `(tree_id, level, item_id)` (skips if item already in buffer).
- **Recap level bands**: `< 2 days → L0`, `< 14 days → L1`, `< 60 days → L2`, `≥ 60 days → L3`. Walk DOWN from target level to 0 looking for material; report `level_used` = the level actually found. Returns `None` only when the global tree has zero sealed summaries.
- **Recap covering selection**: at each level, select summaries whose `[time_range_start, time_range_end]` overlaps `[now - window, now]`, sorted oldest→newest. If none overlap, fall back to the single latest-sealed summary at that level.
- **Trigger**: ONE manual Tauri IPC command `memory_global_digest_run(day: Option<String>)`. `day` is an ISO date string `"YYYY-MM-DD"`; `None` defaults to **yesterday** (`Utc::now().date_naive() - Duration::days(1)`). The automatic scheduler is PR12.
- **AppState concrete handle**: PR9 stored `bucket_seal_adapter` only as `Arc<dyn MemoryAdapter>` (in the map). PR11 adds a concrete `pub bucket_seal_adapter: Arc<BucketSealAdapter>` AppState field so the IPC command can call the inherent methods without `Any`-downcasting.
- **No new workspace deps**: chrono, uuid, rusqlite, anyhow, async-trait, tracing, serde all present.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

1. **Read PR8's `bucket_seal.rs` in full BEFORE writing `seal.rs`.** PR11's `append_to_buffer` and `seal_one_level` are near-verbatim ports of PR8's (lines 178-207 and ~292-460). Copy the uClaw idiom: `let mut conn = store.lock_conn()?; let tx = conn.transaction()?; ...; tx.commit()?;`. Use `store::refresh_last_sealed_tx` for the same-level branch (NOT a raw UPDATE).

2. **`should_seal` signature differs from PR8**: PR8's is `should_seal(buf: &Buffer) -> bool` (token OR fanout). PR11's is `should_seal(buf: &Buffer, level: u32) -> bool` (count threshold per level). Match by level: 0→7, 1→4, 2→12, _→false.

3. **Verify exact signatures of every `store::*` helper** before calling. Run `grep -nE "pub fn (get_tree_by_scope|insert_tree|get_tree|list_trees_by_kind|get_buffer|get_buffer_conn|upsert_buffer_tx|clear_buffer_tx|insert_summary_tx|get_summary|list_summaries_at_level|update_tree_after_seal_tx|refresh_last_sealed_tx)" src-tauri/src/memory_bucket_seal/tree_source/store.rs` and read each. The plan's call sites are approximate — adapt to reality.

4. **`new_summary_id(level)` location**: it's `pub fn` in `tree_source::registry`. Import as `crate::memory_bucket_seal::tree_source::registry::new_summary_id`. Verify it produces level-tagged ids.

5. **`is_unique_violation` for the registry**: PR10's `tree_topic/registry.rs` already replicated this private helper. Copy the same form into `tree_global/registry.rs` (it's `fn`, not `pub fn`, in tree_source — self-contained duplication is the established pattern; see PR10 review which accepted it).

6. **Singleton tree ID format**: `format!("{}:{}", TreeKind::Global.as_str(), Uuid::new_v4())` → `"global:<uuid>"`. Matches PR10's `topic:` and tree_source's `source:` convention.

7. **`SummaryContext` lifetime**: it's `SummaryContext<'a>` with `tree_id: &'a str`. Construct inline before each `summariser.summarise(&inputs, &ctx).await` call.

8. **`Summariser::summarise` signature**: read `summariser/mod.rs` — it's `async fn summarise(&self, inputs: &[SummaryInput], ctx: &SummaryContext) -> Result<SummaryOutput>`. The functions take `summariser: &Arc<dyn Summariser>` (matching PR8's append_leaf); call as `summariser.summarise(...)` (Arc derefs).

9. **`Embedder::embed` signature**: read `score/embed/mod.rs` — `async fn embed(&self, text: &str) -> Result<Vec<f32>>`. Functions take `embedder: &Arc<dyn Embedder>`.

10. **Column names for raw SQL** (digest's `find_existing_daily` + `pick_source_contribution`): the table is `mem_tree_summaries`; columns `id`, `tree_id`, `level`, `time_range_start_ms`, `time_range_end_ms`, `sealed_at_ms`, `deleted`. Verified present. Use `store.lock_conn()?` then `conn.prepare(...)` / `conn.query_row(...).optional()`. Import `rusqlite::OptionalExtension` for `.optional()`.

11. **`day_bounds_utc(day: NaiveDate)`**: produce `[00:00 UTC, +24h)`. Use `day.and_hms_opt(0,0,0)` → `Utc.from_local_datetime(&naive).single()` → `(start, start + Duration::days(1))`. Handle the `None` cases with `anyhow::bail!`.

12. **`pick_source_contribution` content prefix**: prefix the contribution content with `format!("[{}]\n{}", source_tree.scope, node.content)` so the daily digest preserves per-source provenance. Use `node.content` (preview) — uClaw has no `read_summary_body`.

13. **Digest L0 node is NOT backlinked to children**: the daily node's `child_ids` are source-tree summary ids (cross-source references owned by their source trees). Do NOT `UPDATE ... SET parent_id` on them. Only the count-cascade (`seal_one_level`) backlinks, and only the global-owned daily/weekly/monthly nodes it folds.

14. **`BucketSealAdapter` field access**: the two new inherent methods (`run_global_digest`, `global_recap`) live in an `impl BucketSealAdapter { ... }` block (NOT the trait impl). They read `self.store`, `self.summariser`, `self.embedder` (all private fields, accessible within the same module). Pass `&self.store`, `&self.summariser`, `&self.embedder` to the free functions.

15. **AppState wiring change**: in `app.rs`, PR9 has roughly:
    ```rust
    let bucket_seal_adapter = std::sync::Arc::new(
        crate::memory_bucket_seal::BucketSealAdapter::new(...)
    ) as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>;
    memory_adapters_map.insert(bucket_seal_adapter.name().to_string(), bucket_seal_adapter);
    ```
    Change to build the concrete Arc first, keep it, and clone into the map:
    ```rust
    let bucket_seal_adapter = std::sync::Arc::new(
        crate::memory_bucket_seal::BucketSealAdapter::new(...)
    );
    memory_adapters_map.insert(
        bucket_seal_adapter.name().to_string(),
        bucket_seal_adapter.clone() as std::sync::Arc<dyn crate::memory_adapter::MemoryAdapter>,
    );
    ```
    Then add `bucket_seal_adapter` to the `AppState { ... }` struct literal and declare `pub bucket_seal_adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>` in the struct definition.

16. **IPC command + registration (CLAUDE.md adjacent-edit rule)**: define `memory_global_digest_run` in `tauri_commands.rs` AND register it in the `invoke_handler!` macro in `main.rs`. Forgetting the macro entry compiles fine but fails at runtime. Call it out in the commit body.

17. **`memory_global_digest_run` day parsing**: parse `Option<String>` as `chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")`. `None` → `Utc::now().date_naive() - Duration::days(1)`. Map parse errors to `Err(format!(...))`.

18. **Pre-commit hooks**: same as previous PRs. Don't `--no-verify`. Note: the pre-commit hook blocks `memory_graph::write` and `dirs::home_dir` for `.uclaw` — PR11 touches neither.

---

### Task 1: `tree_global/mod.rs` + `tree_global/registry.rs`

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_global/mod.rs`
- Create: `src-tauri/src/memory_bucket_seal/tree_global/registry.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs`

- [ ] **Step 1: Write `tree_global/mod.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Global Activity Digest tree (Phase 3b — openhuman port).
//!
//! A singleton cross-source recap structure: one tree per workspace, built
//! end-of-day from the source trees' current material so a question like
//! "what did I do in the last 7 days?" resolves with one summary hop.
//! Unlike source trees (whose L0 holds raw chunk leaves), the global tree's
//! L0 already holds synthesised **daily** summaries — each a fold of the
//! day's activity across every active source tree.
//!
//! Level conventions (time-axis aligned, not token-driven):
//!   - L0 = one node per **day** (emitted by [`digest::end_of_day_digest`])
//!   - L1 = one node per **week** (~7 daily leaves)
//!   - L2 = one node per **month** (~4 weekly nodes)
//!   - L3 = one node per **year** (~12 monthly nodes)
//!
//! Reuses Phase 3a storage (`mem_tree_trees`/`mem_tree_summaries`/
//! `mem_tree_buffers` with `kind='global'`) and the `Summariser`/`Embedder`
//! traits. The count-based seal trigger replaces the source tree's
//! token-budget gate.

pub mod digest;
pub mod recap;
pub mod registry;
pub mod seal;

pub use digest::{end_of_day_digest, DigestOutcome};
pub use recap::{recap, RecapOutput};
pub use registry::get_or_create_global_tree;

/// Number of L0 (daily) nodes that seal into one L1 (weekly) node.
pub const WEEKLY_SEAL_THRESHOLD: usize = 7;

/// Number of L1 (weekly) nodes that seal into one L2 (monthly) node.
pub const MONTHLY_SEAL_THRESHOLD: usize = 4;

/// Number of L2 (monthly) nodes that seal into one L3 (yearly) node.
pub const YEARLY_SEAL_THRESHOLD: usize = 12;

/// Literal scope used for the singleton global tree.
pub const GLOBAL_SCOPE: &str = "global";

/// Token budget passed into the summariser for global-tree seals. The
/// token-based seal trigger is disabled on the global tree (count/time
/// trigger instead), so this is purely a ceiling on the summariser's
/// output length at each level.
pub const GLOBAL_TOKEN_BUDGET: u32 = 4_000;
```

- [ ] **Step 2: Write `tree_global/registry.rs`**

Mirror PR10's `tree_topic/registry.rs` exactly, with `TreeKind::Global` + fixed scope. Read `tree_topic/registry.rs` first for the precise idempotency + `is_unique_violation` form.

```rust
// SPDX-License-Identifier: Apache-2.0
//! Singleton registry for the global activity digest tree (Phase 3b).
//!
//! Unlike source trees (one per source_id) or topic trees (one per entity),
//! the global tree is a true singleton per workspace — scope is the literal
//! string `"global"`. Lookup + race-recovery otherwise mirror
//! `tree_source::registry::get_or_create_source_tree`.

use anyhow::Result;
use chrono::Utc;
use uuid::Uuid;

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::GLOBAL_SCOPE;
use crate::memory_bucket_seal::tree_source::store as tree_store;
use crate::memory_bucket_seal::tree_source::types::{Tree, TreeKind, TreeStatus};

/// Return the workspace's singleton global tree, creating it lazily on
/// first call. Safe to call repeatedly — subsequent calls short-circuit to
/// the existing row.
pub fn get_or_create_global_tree(store: &BucketSealStore) -> Result<Tree> {
    if let Some(existing) = tree_store::get_tree_by_scope(store, TreeKind::Global, GLOBAL_SCOPE)? {
        return Ok(existing);
    }

    let tree = Tree {
        id: format!("{}:{}", TreeKind::Global.as_str(), Uuid::new_v4()),
        kind: TreeKind::Global,
        scope: GLOBAL_SCOPE.to_string(),
        root_id: None,
        max_level: 0,
        status: TreeStatus::Active,
        created_at: Utc::now(),
        last_sealed_at: None,
    };
    match tree_store::insert_tree(store, &tree) {
        Ok(()) => Ok(tree),
        Err(err) if is_unique_violation(&err) => tree_store::get_tree_by_scope(
            store,
            TreeKind::Global,
            GLOBAL_SCOPE,
        )?
        .ok_or_else(|| {
            anyhow::anyhow!("UNIQUE violation on global-tree insert but no row found on re-query")
        }),
        Err(err) => Err(err),
    }
}

/// True when `err` wraps a SQLite UNIQUE constraint violation. Duplicated
/// from `tree_source::registry` (private there) to keep this module
/// self-contained — same shape as the `tree_topic` copy.
fn is_unique_violation(err: &anyhow::Error) -> bool {
    if let Some(rusqlite::Error::SqliteFailure(sqlite_err, _)) =
        err.downcast_ref::<rusqlite::Error>()
    {
        return sqlite_err.code == rusqlite::ErrorCode::ConstraintViolation;
    }
    format!("{err:#}").contains("UNIQUE constraint failed")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn creates_singleton_global_tree() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_global_tree(&store).unwrap();
        assert_eq!(tree.scope, "global");
        assert_eq!(tree.kind, TreeKind::Global);
        assert_eq!(tree.status, TreeStatus::Active);
        assert!(tree.id.starts_with("global:"));
    }

    #[test]
    fn idempotent_returns_same_tree() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_global_tree(&store).unwrap();
        let t2 = get_or_create_global_tree(&store).unwrap();
        assert_eq!(t1.id, t2.id);
    }

    #[test]
    fn global_distinct_from_source_and_topic_same_scope() {
        let (store, _dir) = fresh_store();
        let global = get_or_create_global_tree(&store).unwrap();
        let source = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(
            &store, "global",
        )
        .unwrap();
        assert_ne!(global.id, source.id);
        assert_eq!(global.kind, TreeKind::Global);
        assert_eq!(source.kind, TreeKind::Source);
    }
}
```

- [ ] **Step 3: Add `pub mod tree_global;` + re-exports to `memory_bucket_seal/mod.rs`**

```rust
pub mod tree_global;

pub use tree_global::{end_of_day_digest, get_or_create_global_tree, recap, DigestOutcome, RecapOutput};
```

(Alphabetical placement with existing `pub mod`/`pub use`. NOTE: digest/recap won't exist until Tasks 3-4 — so add `pub mod tree_global;` now but defer the `pub use` of `end_of_day_digest`/`recap`/`DigestOutcome`/`RecapOutput` until Task 4, OR add them now and accept that the module won't compile until Task 3/4 land. Recommended: add only `pub use tree_global::get_or_create_global_tree;` in Task 1; extend the re-export in Task 4. The `tree_global/mod.rs` itself references `digest`/`recap` — so to compile Task 1 alone, temporarily comment out the `pub mod digest; pub mod recap;` + their re-exports in `tree_global/mod.rs`, OR write stub digest.rs/recap.rs. SIMPLEST: write Tasks 1-4 as a unit and commit once they all compile. The implementer may collapse Tasks 1-4 into fewer commits if intermediate states don't compile — mirror PR9's combined-commit decision.)

- [ ] **Step 4: Build + test (if compilable in isolation)**

If you stubbed digest/recap or deferred their `pub mod`, run:
Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_global::registry 2>&1 | tail -10`
Expected: 3 passed.

- [ ] **Step 5: Commit** (or defer to a combined commit after Task 4)

```bash
git add src-tauri/src/memory_bucket_seal/tree_global/ src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): tree_global mod + singleton registry (PR11.1 of 阶段 4)"
```

---

### Task 2: `tree_global/seal.rs` — count-based cascade

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_global/seal.rs`

- [ ] **Step 1: Read PR8's `bucket_seal.rs` `append_to_buffer` + `seal_one_level` in full.** The port is near-verbatim.

- [ ] **Step 2: Write `seal.rs`**

Key structure (adapt store-helper calls to verified signatures):

```rust
// SPDX-License-Identifier: Apache-2.0
//! Count-based cascade-seal for the global activity digest tree (Phase 3b).
//!
//! Trigger is count-based, not token-based: seal L0→L1 at 7 daily nodes,
//! L1→L2 at 4 weekly, L2→L3 at 12 monthly. Reuses PR8's storage primitives
//! without the token-budget gate. Every global buffer level holds summary
//! ids (even L0 — a daily digest is a SummaryNode), so hydration always
//! reads `mem_tree_summaries`.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::{
    GLOBAL_TOKEN_BUDGET, MONTHLY_SEAL_THRESHOLD, WEEKLY_SEAL_THRESHOLD, YEARLY_SEAL_THRESHOLD,
};
use crate::memory_bucket_seal::tree_source::registry::new_summary_id;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::summariser::{
    Summariser, SummaryContext, SummaryInput,
};
use crate::memory_bucket_seal::tree_source::types::{Buffer, SummaryNode, Tree, TreeKind};

const MAX_CASCADE_DEPTH: u32 = 32;

/// Append one L0 (daily) summary id to the global tree's L0 buffer, then
/// cascade-seal upward if count thresholds are crossed. The caller
/// (`digest::end_of_day_digest`) has already inserted the daily node into
/// `mem_tree_summaries`; this only does buffer accounting + cascade.
pub async fn append_daily_and_cascade(
    store: &BucketSealStore,
    tree: &Tree,
    daily_summary: &SummaryNode,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<Vec<String>> {
    append_to_buffer(
        store,
        &tree.id,
        0,
        &daily_summary.id,
        daily_summary.token_count as i64,
        daily_summary.time_range_start,
    )?;
    cascade_seals(store, tree, summariser, embedder).await
}

/// Transactionally append a summary id to the buffer at (tree_id, level).
/// Idempotent on (tree_id, level, item_id). Mirrors PR8's append_to_buffer.
fn append_to_buffer(
    store: &BucketSealStore,
    tree_id: &str,
    level: u32,
    item_id: &str,
    token_delta: i64,
    item_ts: DateTime<Utc>,
) -> Result<()> {
    let mut conn = store.lock_conn()?;
    let tx = conn.transaction()?;
    let mut buf = store::get_buffer_conn(&tx, tree_id, level)?;
    if buf.item_ids.iter().any(|existing| existing == item_id) {
        return Ok(());
    }
    buf.item_ids.push(item_id.to_string());
    buf.token_sum = buf.token_sum.saturating_add(token_delta);
    buf.oldest_at = match buf.oldest_at {
        Some(existing) => Some(existing.min(item_ts)),
        None => Some(item_ts),
    };
    store::upsert_buffer_tx(&tx, &buf)?;
    tx.commit()?;
    Ok(())
}

async fn cascade_seals(
    store: &BucketSealStore,
    tree: &Tree,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<Vec<String>> {
    let mut sealed_ids: Vec<String> = Vec::new();
    let mut level: u32 = 0;
    for _ in 0..MAX_CASCADE_DEPTH {
        let buf = store::get_buffer(store, &tree.id, level)?;
        if !should_seal(&buf, level) {
            break;
        }
        let summary_id = seal_one_level(store, tree, &buf, summariser, embedder).await?;
        sealed_ids.push(summary_id);
        level += 1;
    }
    Ok(sealed_ids)
}

/// Count-based threshold per level. 0→7 (weekly), 1→4 (monthly), 2→12
/// (yearly). Levels ≥ 3 never seal — yearly is the top.
fn should_seal(buf: &Buffer, level: u32) -> bool {
    let threshold = match level {
        0 => WEEKLY_SEAL_THRESHOLD,
        1 => MONTHLY_SEAL_THRESHOLD,
        2 => YEARLY_SEAL_THRESHOLD,
        _ => return false,
    };
    !buf.is_empty() && buf.item_ids.len() >= threshold
}

async fn seal_one_level(
    store: &BucketSealStore,
    tree: &Tree,
    buf: &Buffer,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<String> {
    let level = buf.level;
    let target_level = level + 1;

    let inputs = hydrate_summary_inputs(store, &buf.item_ids)?;
    if inputs.is_empty() {
        anyhow::bail!(
            "[tree_global::seal] refused to seal empty buffer tree_id={} level={}",
            tree.id,
            level
        );
    }

    let time_range_start = inputs.iter().map(|i| i.time_range_start).min().unwrap_or_else(Utc::now);
    let time_range_end = inputs.iter().map(|i| i.time_range_end).max().unwrap_or_else(Utc::now);
    let score = inputs.iter().map(|i| i.score).fold(f32::NEG_INFINITY, f32::max).max(0.0);

    let ctx = SummaryContext {
        tree_id: &tree.id,
        tree_kind: TreeKind::Global,
        target_level,
        token_budget: GLOBAL_TOKEN_BUDGET,
    };
    let output = summariser
        .summarise(&inputs, &ctx)
        .await
        .context("summariser failed during global seal")?;

    // Union entity/topic labels from already-labeled inputs — global is a
    // sink; no second-pass extractor.
    let mut entities_set: BTreeSet<String> = BTreeSet::new();
    let mut topics_set: BTreeSet<String> = BTreeSet::new();
    for inp in &inputs {
        entities_set.extend(inp.entities.iter().cloned());
        topics_set.extend(inp.topics.iter().cloned());
    }

    // Embed BEFORE the write tx so an embed error aborts the seal cleanly.
    let embedding = embedder
        .embed(&output.content)
        .await
        .with_context(|| format!("embed global summary tree_id={} level={}", tree.id, level))?;

    let now = Utc::now();
    let summary_id = new_summary_id(target_level);
    let node = SummaryNode {
        id: summary_id.clone(),
        tree_id: tree.id.clone(),
        tree_kind: TreeKind::Global,
        level: target_level,
        parent_id: None,
        child_ids: buf.item_ids.clone(),
        content: output.content,
        token_count: output.token_count,
        entities: entities_set.into_iter().collect(),
        topics: topics_set.into_iter().collect(),
        time_range_start,
        time_range_end,
        score,
        sealed_at: now,
        deleted: false,
        embedding: Some(embedding),
    };

    {
        let mut conn = store.lock_conn()?;
        let tx = conn.transaction()?;

        let current_max: u32 = tx
            .query_row(
                "SELECT max_level FROM mem_tree_trees WHERE id = ?1",
                rusqlite::params![&tree.id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n.max(0) as u32)
            .context("read current max_level for global tree")?;

        store::insert_summary_tx(&tx, &node)?;

        // Backlink children → new parent. In the global tree EVERY level
        // (incl. L0) holds global-owned summary nodes, so always backlink.
        for child_id in &node.child_ids {
            tx.execute(
                "UPDATE mem_tree_summaries SET parent_id = ?1 WHERE id = ?2 AND parent_id IS NULL",
                rusqlite::params![&summary_id, child_id],
            )
            .context("backlink global summary to parent")?;
        }

        store::clear_buffer_tx(&tx, &tree.id, level)?;

        let mut parent = store::get_buffer_conn(&tx, &tree.id, target_level)?;
        parent.item_ids.push(summary_id.clone());
        parent.token_sum = parent.token_sum.saturating_add(node.token_count as i64);
        parent.oldest_at = match parent.oldest_at {
            Some(existing) => Some(existing.min(time_range_start)),
            None => Some(time_range_start),
        };
        store::upsert_buffer_tx(&tx, &parent)?;

        if target_level > current_max {
            store::update_tree_after_seal_tx(&tx, &tree.id, &summary_id, target_level, now)?;
        } else {
            store::refresh_last_sealed_tx(&tx, &tree.id, now)?;
        }

        tx.commit()?;
    }

    Ok(summary_id)
}

/// Hydrate summary rows for buffer ids. Global buffers at every level hold
/// summary ids, so always pull from `mem_tree_summaries`.
fn hydrate_summary_inputs(store: &BucketSealStore, summary_ids: &[String]) -> Result<Vec<SummaryInput>> {
    let mut out = Vec::with_capacity(summary_ids.len());
    for id in summary_ids {
        let Some(node) = store::get_summary(store, id)? else {
            tracing::warn!(summary_id = %id, "[tree_global::seal] hydrate: missing summary — skipping");
            continue;
        };
        out.push(SummaryInput {
            id: node.id.clone(),
            content: node.content.clone(),
            token_count: node.token_count,
            entities: node.entities.clone(),
            topics: node.topics.clone(),
            time_range_start: node.time_range_start,
            time_range_end: node.time_range_end,
            score: node.score,
        });
    }
    Ok(out)
}
```

- [ ] **Step 3: Add 3 inline tests** (mirror openhuman's seal tests, adapted to uClaw fixtures)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
    use crate::memory_bucket_seal::tree_source::summariser::InertSummariser;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    fn mk_daily(id: &str, tree_id: &str, day_ms: i64) -> SummaryNode {
        let ts = Utc.timestamp_millis_opt(day_ms).single().unwrap();
        SummaryNode {
            id: id.to_string(),
            tree_id: tree_id.to_string(),
            tree_kind: TreeKind::Global,
            level: 0,
            parent_id: None,
            child_ids: vec![],
            content: format!("daily digest {id}"),
            token_count: 200,
            entities: vec![],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.5,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        }
    }

    fn insert_daily(store: &BucketSealStore, node: &SummaryNode) {
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        store::insert_summary_tx(&tx, node).unwrap();
        tx.commit().unwrap();
    }

    #[tokio::test]
    async fn below_threshold_does_not_seal() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        for i in 0..3 {
            let node = mk_daily(&format!("summary:L0:day{i}"), &tree.id, 1_700_000_000_000 + i);
            insert_daily(&store, &node);
            let sealed = append_daily_and_cascade(&store, &tree, &node, &s, &e).await.unwrap();
            assert!(sealed.is_empty());
        }
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids.len(), 3);
    }

    #[tokio::test]
    async fn crossing_weekly_threshold_seals_l1() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        for i in 0..WEEKLY_SEAL_THRESHOLD {
            let node = mk_daily(&format!("summary:L0:day{i}"), &tree.id, 1_700_000_000_000 + i as i64);
            insert_daily(&store, &node);
            let sealed = append_daily_and_cascade(&store, &tree, &node, &s, &e).await.unwrap();
            if i + 1 < WEEKLY_SEAL_THRESHOLD {
                assert!(sealed.is_empty());
            } else {
                assert_eq!(sealed.len(), 1);
            }
        }
        let l0 = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert!(l0.is_empty());
        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert_eq!(l1.item_ids.len(), 1);
        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 1);
        let weekly = store::get_summary(&store, &l1.item_ids[0]).unwrap().unwrap();
        assert_eq!(weekly.level, 1);
        assert_eq!(weekly.tree_kind, TreeKind::Global);
        assert_eq!(weekly.child_ids.len(), WEEKLY_SEAL_THRESHOLD);
    }

    #[tokio::test]
    async fn append_is_idempotent_on_retry() {
        let (store, s, e, _dir) = fresh();
        let tree = get_or_create_global_tree(&store).unwrap();
        let node = mk_daily("summary:L0:dayA", &tree.id, 1_700_000_000_000);
        insert_daily(&store, &node);
        append_daily_and_cascade(&store, &tree, &node, &s, &e).await.unwrap();
        append_daily_and_cascade(&store, &tree, &node, &s, &e).await.unwrap();
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids.len(), 1);
        assert_eq!(buf.token_sum, 200);
    }
}
```

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_global::seal 2>&1 | tail -15`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_global/seal.rs
git commit -m "feat(memory_bucket_seal): tree_global count-based cascade seal (PR11.2 of 阶段 4)"
```

---

### Task 3: `tree_global/digest.rs` — end-of-day builder

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_global/digest.rs`

- [ ] **Step 1: Write `digest.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! End-of-day digest builder for the global activity tree (Phase 3b).
//!
//! Once per calendar day: walk every active source tree, collect the
//! summary material covering that day, fold it into one cross-source recap,
//! persist as an L0 node in the singleton global tree, then cascade-seal.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, NaiveDate, TimeZone, Utc};
use rusqlite::OptionalExtension;

use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
use crate::memory_bucket_seal::tree_global::seal::append_daily_and_cascade;
use crate::memory_bucket_seal::tree_global::GLOBAL_TOKEN_BUDGET;
use crate::memory_bucket_seal::tree_source::registry::new_summary_id;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::summariser::{Summariser, SummaryContext, SummaryInput};
use crate::memory_bucket_seal::tree_source::types::{SummaryNode, Tree, TreeKind};

/// Outcome of one `end_of_day_digest` call.
#[derive(Debug, Clone)]
pub enum DigestOutcome {
    /// Emitted one L0 daily node; `sealed_ids` lists any L1/L2/L3 nodes
    /// that sealed during the cascade.
    Emitted {
        daily_id: String,
        source_count: usize,
        sealed_ids: Vec<String>,
    },
    /// No source tree had material for the day — nothing written.
    EmptyDay,
    /// An L0 node already exists for the day (re-run) — nothing written.
    Skipped { existing_id: String },
}

/// Run an end-of-day digest for `day` (UTC calendar date). Appends one L0
/// node to the global tree + cascade-seals if thresholds cross.
pub async fn end_of_day_digest(
    store: &BucketSealStore,
    day: NaiveDate,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
) -> Result<DigestOutcome> {
    let (day_start, day_end) = day_bounds_utc(day)?;
    let global = get_or_create_global_tree(store)?;

    if let Some(existing) = find_existing_daily(store, &global.id, day_start)? {
        return Ok(DigestOutcome::Skipped { existing_id: existing.id });
    }

    let source_trees = store::list_trees_by_kind(store, TreeKind::Source)?;
    let mut inputs: Vec<SummaryInput> = Vec::with_capacity(source_trees.len());
    for source_tree in &source_trees {
        if let Some(inp) = pick_source_contribution(store, source_tree, day_start, day_end)? {
            inputs.push(inp);
        }
    }

    if inputs.is_empty() {
        return Ok(DigestOutcome::EmptyDay);
    }

    let ctx = SummaryContext {
        tree_id: &global.id,
        tree_kind: TreeKind::Global,
        target_level: 0,
        token_budget: GLOBAL_TOKEN_BUDGET,
    };
    let output = summariser
        .summarise(&inputs, &ctx)
        .await
        .context("summariser failed during end-of-day digest")?;

    let score = inputs.iter().map(|i| i.score).fold(f32::NEG_INFINITY, f32::max).max(0.0);

    let embedding = embedder
        .embed(&output.content)
        .await
        .context("embed daily summary during end_of_day_digest")?;

    let mut entities_set: BTreeSet<String> = BTreeSet::new();
    let mut topics_set: BTreeSet<String> = BTreeSet::new();
    for inp in &inputs {
        entities_set.extend(inp.entities.iter().cloned());
        topics_set.extend(inp.topics.iter().cloned());
    }

    let now = Utc::now();
    let daily_id = new_summary_id(0);
    let daily = SummaryNode {
        id: daily_id.clone(),
        tree_id: global.id.clone(),
        tree_kind: TreeKind::Global,
        level: 0,
        parent_id: None,
        child_ids: inputs.iter().map(|i| i.id.clone()).collect(),
        content: output.content,
        token_count: output.token_count,
        entities: entities_set.into_iter().collect(),
        topics: topics_set.into_iter().collect(),
        time_range_start: day_start,
        time_range_end: day_end,
        score,
        sealed_at: now,
        deleted: false,
        embedding: Some(embedding),
    };

    // Persist the daily node. Do NOT backlink child_ids — those are
    // source-tree summary references owned by their source trees, not the
    // global tree.
    {
        let mut conn = store.lock_conn()?;
        let tx = conn.transaction()?;
        store::insert_summary_tx(&tx, &daily)?;
        tx.commit()?;
    }

    let sealed_ids = append_daily_and_cascade(store, &global, &daily, summariser, embedder).await?;

    Ok(DigestOutcome::Emitted {
        daily_id: daily.id,
        source_count: inputs.len(),
        sealed_ids,
    })
}

/// [00:00, +24h) UTC bounds for a calendar day.
fn day_bounds_utc(day: NaiveDate) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let start_naive = day
        .and_hms_opt(0, 0, 0)
        .ok_or_else(|| anyhow::anyhow!("invalid day {day} — failed to build 00:00 timestamp"))?;
    let start = Utc
        .from_local_datetime(&start_naive)
        .single()
        .ok_or_else(|| anyhow::anyhow!("non-unique UTC time for day {day}"))?;
    Ok((start, start + Duration::days(1)))
}

/// Existing L0 daily node for this day, matched on time_range_start_ms.
fn find_existing_daily(
    store: &BucketSealStore,
    global_tree_id: &str,
    day_start: DateTime<Utc>,
) -> Result<Option<SummaryNode>> {
    let start_ms = day_start.timestamp_millis();
    let opt_id: Option<String> = {
        let conn = store.lock_conn()?;
        conn.query_row(
            "SELECT id FROM mem_tree_summaries
              WHERE tree_id = ?1 AND level = 0 AND time_range_start_ms = ?2 AND deleted = 0
              LIMIT 1",
            rusqlite::params![global_tree_id, start_ms],
            |r| r.get::<_, String>(0),
        )
        .optional()
        .context("query for existing daily node")?
    };
    match opt_id {
        Some(id) => store::get_summary(store, &id),
        None => Ok(None),
    }
}

/// Best single contribution from one source tree for the target day.
/// Priority: (1) latest summary intersecting the day window; (2) else the
/// tree's current root summary; (3) else None (no sealed summaries yet).
fn pick_source_contribution(
    store: &BucketSealStore,
    source_tree: &Tree,
    day_start: DateTime<Utc>,
    day_end: DateTime<Utc>,
) -> Result<Option<SummaryInput>> {
    let start_ms = day_start.timestamp_millis();
    let end_ms = day_end.timestamp_millis();
    let intersecting_id: Option<String> = {
        let conn = store.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id FROM mem_tree_summaries
              WHERE tree_id = ?1 AND deleted = 0
                AND time_range_start_ms < ?3 AND time_range_end_ms >= ?2
              ORDER BY level DESC, sealed_at_ms DESC
              LIMIT 1",
        )?;
        stmt.query_row(rusqlite::params![&source_tree.id, start_ms, end_ms], |r| {
            r.get::<_, String>(0)
        })
        .optional()
        .context("query intersecting source summary")?
    };

    let chosen_id = intersecting_id.or_else(|| source_tree.root_id.clone());
    let Some(id) = chosen_id else { return Ok(None) };

    let Some(node) = store::get_summary(store, &id)? else {
        return Ok(None);
    };

    Ok(Some(SummaryInput {
        id: node.id.clone(),
        // node.content is a ≤500-char preview (PR8). Full-body read lands
        // in PR12 with content_store::read. Prefix scope for provenance.
        content: format!("[{}]\n{}", source_tree.scope, node.content),
        token_count: node.token_count,
        entities: node.entities,
        topics: node.topics,
        time_range_start: node.time_range_start,
        time_range_end: node.time_range_end,
        score: node.score,
    }))
}
```

- [ ] **Step 2: Add 4 inline tests** — empty-day no-op, populated-day emits L0, idempotent skip on re-run, multi-source fold. Use a fixture that seeds source-tree L1 summaries (via PR8's `append_leaf` with large-token chunks to force an L1 seal, OR directly insert source summaries via `insert_summary_tx` + `update_tree_after_seal_tx` to set root_id). The directly-insert path is simpler and avoids depending on chunk staging:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::summariser::InertSummariser;
    use crate::memory_bucket_seal::tree_source::types::TreeStatus;
    use chrono::TimeZone;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    /// Seed a source tree with one L1 summary covering `day`, set as its root.
    fn seed_source_with_summary(store: &BucketSealStore, scope: &str, day: NaiveDate) -> String {
        let tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(store, scope).unwrap();
        let ts = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let summary_id = format!("summary:L1:{scope}");
        let node = SummaryNode {
            id: summary_id.clone(),
            tree_id: tree.id.clone(),
            tree_kind: TreeKind::Source,
            level: 1,
            parent_id: None,
            child_ids: vec![],
            content: format!("source summary for {scope}"),
            token_count: 300,
            entities: vec![format!("Entity_{scope}")],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.7,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        };
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        store::insert_summary_tx(&tx, &node).unwrap();
        store::update_tree_after_seal_tx(&tx, &tree.id, &summary_id, 1, ts).unwrap();
        tx.commit().unwrap();
        summary_id
    }

    #[tokio::test]
    async fn empty_day_is_noop() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(outcome, DigestOutcome::EmptyDay));
    }

    #[tokio::test]
    async fn populated_day_emits_l0_daily() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#eng", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        match outcome {
            DigestOutcome::Emitted { source_count, .. } => assert_eq!(source_count, 1),
            other => panic!("expected Emitted, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn rerun_same_day_skips() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#eng", day);
        end_of_day_digest(&store, day, &s, &e).await.unwrap();
        let second = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(second, DigestOutcome::Skipped { .. }));
    }

    #[tokio::test]
    async fn multi_source_fold_counts_all() {
        let (store, s, e, _dir) = fresh();
        let day = NaiveDate::from_ymd_opt(2026, 5, 1).unwrap();
        seed_source_with_summary(&store, "slack:#a", day);
        seed_source_with_summary(&store, "slack:#b", day);
        seed_source_with_summary(&store, "email:gmail", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        match outcome {
            DigestOutcome::Emitted { source_count, .. } => assert_eq!(source_count, 3),
            other => panic!("expected Emitted, got {other:?}"),
        }
    }
}
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_global::digest 2>&1 | tail -15`
Expected: 4 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_global/digest.rs
git commit -m "feat(memory_bucket_seal): tree_global end-of-day digest builder (PR11.3 of 阶段 4)"
```

---

### Task 4: `tree_global/recap.rs` — read-side level picker

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_global/recap.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (extend re-export now that digest+recap exist)

- [ ] **Step 1: Write `recap.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Window-scoped recap retrieval for the global activity tree (Phase 3b).
//!
//! Given a duration, pick the tree level matching the time axis and return
//! the covering summaries. Falls back DOWN when the chosen level hasn't
//! sealed yet, reporting the actual `level_used`.

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_global::registry::get_or_create_global_tree;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::types::SummaryNode;

/// Aggregated recap returned to the caller.
#[derive(Debug, Clone)]
pub struct RecapOutput {
    pub content: String,
    pub time_range: (DateTime<Utc>, DateTime<Utc>),
    pub level_used: u32,
    pub summary_ids: Vec<String>,
}

/// Recap for the given window, or `None` if no global summaries have sealed.
pub async fn recap(store: &BucketSealStore, window: Duration) -> Result<Option<RecapOutput>> {
    let target_level = pick_level(window);
    let global = get_or_create_global_tree(store)?;
    let now = Utc::now();
    let window_start = now - window;

    for level in (0..=target_level).rev() {
        let all_at_level = store::list_summaries_at_level(store, &global.id, level)?;
        if all_at_level.is_empty() {
            continue;
        }
        let covering = pick_covering(&all_at_level, window_start, now);
        if covering.is_empty() {
            continue;
        }
        return Ok(Some(assemble_recap(&covering, level)));
    }
    Ok(None)
}

/// Map a window duration to a level. <2d→0, <14d→1, <60d→2, ≥60d→3.
pub fn pick_level(window: Duration) -> u32 {
    if window < Duration::days(2) {
        0
    } else if window < Duration::days(14) {
        1
    } else if window < Duration::days(60) {
        2
    } else {
        3
    }
}

/// Summaries at a level overlapping [window_start, now], oldest→newest.
/// Falls back to the single latest-sealed when none overlap.
fn pick_covering<'a>(
    summaries: &'a [SummaryNode],
    window_start: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Vec<&'a SummaryNode> {
    let mut overlapping: Vec<&SummaryNode> = summaries
        .iter()
        .filter(|s| s.time_range_end >= window_start && s.time_range_start <= now)
        .collect();
    overlapping.sort_by_key(|s| s.time_range_start);
    if overlapping.is_empty() {
        if let Some(latest) = summaries.iter().max_by_key(|s| s.sealed_at) {
            return vec![latest];
        }
    }
    overlapping
}

fn assemble_recap(covering: &[&SummaryNode], level: u32) -> RecapOutput {
    let mut parts = Vec::with_capacity(covering.len());
    let mut summary_ids = Vec::with_capacity(covering.len());
    for s in covering {
        parts.push(format!(
            "[{} → {}]\n{}",
            s.time_range_start.to_rfc3339(),
            s.time_range_end.to_rfc3339(),
            s.content
        ));
        summary_ids.push(s.id.clone());
    }
    let time_start = covering.iter().map(|s| s.time_range_start).min().unwrap_or_else(Utc::now);
    let time_end = covering.iter().map(|s| s.time_range_end).max().unwrap_or_else(Utc::now);
    RecapOutput {
        content: parts.join("\n\n"),
        time_range: (time_start, time_end),
        level_used: level,
        summary_ids,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
    use crate::memory_bucket_seal::tree_global::digest::{end_of_day_digest, DigestOutcome};
    use crate::memory_bucket_seal::tree_source::store as tstore;
    use crate::memory_bucket_seal::tree_source::summariser::{InertSummariser, Summariser};
    use crate::memory_bucket_seal::tree_source::types::{SummaryNode, TreeKind};
    use chrono::NaiveDate;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn fresh() -> (BucketSealStore, Arc<dyn Summariser>, Arc<dyn Embedder>, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        let s: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let e: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        (store, s, e, dir)
    }

    fn seed_source_summary(store: &BucketSealStore, scope: &str, day: NaiveDate) {
        let tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(store, scope).unwrap();
        let ts = day.and_hms_opt(12, 0, 0).unwrap().and_utc();
        let node = SummaryNode {
            id: format!("summary:L1:{scope}"),
            tree_id: tree.id.clone(),
            tree_kind: TreeKind::Source,
            level: 1,
            parent_id: None,
            child_ids: vec![],
            content: format!("src {scope}"),
            token_count: 300,
            entities: vec![],
            topics: vec![],
            time_range_start: ts,
            time_range_end: ts,
            score: 0.5,
            sealed_at: ts,
            deleted: false,
            embedding: None,
        };
        let mut conn = store.lock_conn().unwrap();
        let tx = conn.transaction().unwrap();
        tstore::insert_summary_tx(&tx, &node).unwrap();
        tstore::update_tree_after_seal_tx(&tx, &tree.id, &node.id, 1, ts).unwrap();
        tx.commit().unwrap();
    }

    #[test]
    fn pick_level_matches_thresholds() {
        assert_eq!(pick_level(Duration::hours(1)), 0);
        assert_eq!(pick_level(Duration::days(1)), 0);
        assert_eq!(pick_level(Duration::days(2)), 1);
        assert_eq!(pick_level(Duration::days(13)), 1);
        assert_eq!(pick_level(Duration::days(14)), 2);
        assert_eq!(pick_level(Duration::days(59)), 2);
        assert_eq!(pick_level(Duration::days(60)), 3);
        assert_eq!(pick_level(Duration::days(365)), 3);
    }

    #[tokio::test]
    async fn recap_on_empty_tree_returns_none() {
        let (store, _s, _e, _dir) = fresh();
        let out = recap(&store, Duration::days(7)).await.unwrap();
        assert!(out.is_none());
    }

    #[tokio::test]
    async fn recap_one_day_returns_latest_l0() {
        let (store, s, e, _dir) = fresh();
        let day = Utc::now().date_naive();
        seed_source_summary(&store, "slack:#eng", day);
        let outcome = end_of_day_digest(&store, day, &s, &e).await.unwrap();
        assert!(matches!(outcome, DigestOutcome::Emitted { .. }));
        let r = recap(&store, Duration::hours(24)).await.unwrap().expect("recap");
        assert_eq!(r.level_used, 0);
        assert_eq!(r.summary_ids.len(), 1);
        assert!(!r.content.is_empty());
    }

    #[tokio::test]
    async fn recap_weekly_window_falls_back_to_l0_when_no_l1() {
        let (store, s, e, _dir) = fresh();
        let today = Utc::now().date_naive();
        for i in 0..3 {
            let day = today - Duration::days(2 - i);
            seed_source_summary(&store, &format!("slack:#d{i}"), day);
            end_of_day_digest(&store, day, &s, &e).await.unwrap();
        }
        let r = recap(&store, Duration::days(7)).await.unwrap().expect("fallback recap");
        assert_eq!(r.level_used, 0);
        assert_eq!(r.summary_ids.len(), 3);
    }
}
```

- [ ] **Step 2: Extend `memory_bucket_seal/mod.rs` re-export**

Now that digest + recap exist, ensure the full re-export line is present:

```rust
pub use tree_global::{end_of_day_digest, get_or_create_global_tree, recap, DigestOutcome, RecapOutput};
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_global 2>&1 | tail -15`
Expected: ~14 passed (3 registry + 3 seal + 4 digest + 4 recap).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_global/recap.rs src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): tree_global recap level-picker (PR11.4 of 阶段 4)"
```

---

### Task 5: Adapter methods + AppState handle + IPC command

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`
- Modify: `src-tauri/src/app.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Modify: `src-tauri/src/main.rs`

- [ ] **Step 1: Add inherent methods to `BucketSealAdapter`**

In `adapter.rs`, add a new (or extend the existing) `impl BucketSealAdapter { ... }` inherent block (NOT the `#[async_trait] impl MemoryAdapter` block):

```rust
impl BucketSealAdapter {
    /// Run an end-of-day cross-source digest for `day`, appending one L0
    /// node to the global tree and cascade-sealing if thresholds cross.
    pub async fn run_global_digest(
        &self,
        day: chrono::NaiveDate,
    ) -> anyhow::Result<crate::memory_bucket_seal::DigestOutcome> {
        crate::memory_bucket_seal::end_of_day_digest(
            &self.store,
            day,
            &self.summariser,
            &self.embedder,
        )
        .await
    }

    /// Return a window-scoped recap from the global activity tree.
    pub async fn global_recap(
        &self,
        window: chrono::Duration,
    ) -> anyhow::Result<Option<crate::memory_bucket_seal::RecapOutput>> {
        crate::memory_bucket_seal::recap(&self.store, window).await
    }
}
```

- [ ] **Step 2: Add 2 adapter tests**

```rust
    #[tokio::test]
    async fn run_global_digest_via_adapter() {
        let (adapter, _dir) = fresh_adapter();
        // Store something so a source tree + summary exists. Use a large
        // body to push a chunk through admission.
        adapter
            .store("slack:#eng", "k1", "Alice shipped Project Phoenix today with substantial detail and signal.", MemoryCategory::Core, None)
            .await
            .unwrap();
        // The source tree exists but may have no sealed L1 (single small
        // chunk stays in L0 buffer). Digest skips trees with no root summary,
        // so this likely yields EmptyDay — that's a valid outcome. Assert it
        // doesn't error.
        let day = chrono::Utc::now().date_naive();
        let outcome = adapter.run_global_digest(day).await.unwrap();
        // Either EmptyDay (no sealed source summary) or Emitted — both ok.
        let _ = outcome;
    }

    #[tokio::test]
    async fn global_recap_empty_is_none() {
        let (adapter, _dir) = fresh_adapter();
        let r = adapter.global_recap(chrono::Duration::days(7)).await.unwrap();
        assert!(r.is_none());
    }
```

- [ ] **Step 3: AppState concrete handle in `app.rs`**

Read `app.rs` around the PR9 bucket_seal wiring + the `AppState` struct definition + the `AppState { ... }` construction literal. Apply adaptation #15:
- Build `bucket_seal_adapter` as concrete `Arc<BucketSealAdapter>` (drop the `as Arc<dyn MemoryAdapter>` cast on the `let`).
- Insert `bucket_seal_adapter.clone() as Arc<dyn MemoryAdapter>` into the map.
- Add `pub bucket_seal_adapter: std::sync::Arc<crate::memory_bucket_seal::BucketSealAdapter>` to the `AppState` struct definition.
- Add `bucket_seal_adapter` to the `AppState { ... }` construction literal.

- [ ] **Step 4: IPC command in `tauri_commands.rs`**

```rust
/// Result of a manual global-digest run (PR11). The automatic scheduler
/// lands in PR12 (jobs subsystem).
#[derive(serde::Serialize)]
pub struct GlobalDigestResult {
    pub outcome: String,
    pub daily_id: Option<String>,
    pub source_count: usize,
    pub sealed_ids: Vec<String>,
}

/// Manually run the end-of-day cross-source digest for `day`
/// (ISO "YYYY-MM-DD"; `None` → yesterday). Routes through the bucket_seal
/// adapter's global activity tree.
#[tauri::command]
pub async fn memory_global_digest_run(
    state: tauri::State<'_, crate::app::AppState>,
    day: Option<String>,
) -> Result<GlobalDigestResult, String> {
    let day = match day {
        Some(s) => chrono::NaiveDate::parse_from_str(&s, "%Y-%m-%d")
            .map_err(|e| format!("invalid day '{s}': {e}"))?,
        None => chrono::Utc::now().date_naive() - chrono::Duration::days(1),
    };

    let outcome = state
        .bucket_seal_adapter
        .run_global_digest(day)
        .await
        .map_err(|e| format!("global digest failed: {e:#}"))?;

    use crate::memory_bucket_seal::DigestOutcome;
    let result = match outcome {
        DigestOutcome::Emitted { daily_id, source_count, sealed_ids } => GlobalDigestResult {
            outcome: "emitted".to_string(),
            daily_id: Some(daily_id),
            source_count,
            sealed_ids,
        },
        DigestOutcome::EmptyDay => GlobalDigestResult {
            outcome: "empty_day".to_string(),
            daily_id: None,
            source_count: 0,
            sealed_ids: vec![],
        },
        DigestOutcome::Skipped { existing_id } => GlobalDigestResult {
            outcome: "skipped".to_string(),
            daily_id: Some(existing_id),
            source_count: 0,
            sealed_ids: vec![],
        },
    };
    Ok(result)
}
```

Verify the exact `AppState` import path + the existing command style in `tauri_commands.rs` (some commands may use `State<'_, AppState>` with a local `use`). Match the surrounding convention.

- [ ] **Step 5: Register in `main.rs` `invoke_handler!`**

Add `memory_global_digest_run` to the `tauri::generate_handler![...]` list (the `invoke_handler!` macro). Find the existing `memory_*` commands and add adjacent.

- [ ] **Step 6: Build + test**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter 2>&1 | tail -15`
Expected: PR9+PR10 adapter tests + 2 new = all pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs src-tauri/src/app.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "feat(app): wire BucketSealAdapter global-digest methods + memory_global_digest_run IPC (PR11.5 of 阶段 4)

Adjacent edits (per CLAUDE.md): new IPC command defined in tauri_commands.rs
AND registered in invoke_handler! in main.rs. AppState gains a concrete
Arc<BucketSealAdapter> handle alongside the Arc<dyn MemoryAdapter> map entry."
```

---

### Task 6: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~218+ passed (202 PR10 baseline + ~16 new: 3 registry + 3 seal + 4 digest + 4 recap + 2 adapter).

- [ ] **Step 2: Full backend build (IPC wiring touches main.rs — must compile end to end)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors. (A `--lib` build won't catch a missing `invoke_handler!` arg mismatch — the full build does.)

- [ ] **Step 3: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive over PR10 baseline; pre-existing failures elsewhere unchanged.

- [ ] **Step 4: Clippy**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "tree_global|adapter\.rs|tauri_commands\.rs|app\.rs" | head -20`
Expected: zero hits attributable to PR11.

- [ ] **Step 5: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr11-tree-global-port && git diff main -- src-tauri/Cargo.toml`
Expected: empty.

- [ ] **Step 6: IPC registration sanity**

Run: `grep -n "memory_global_digest_run" src-tauri/src/main.rs src-tauri/src/tauri_commands.rs`
Expected: present in BOTH files (definition + registration).

- [ ] **Step 7: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -rnE "TODO|FIXME|XXX" src/memory_bucket_seal/tree_global/`
Expected: zero hits.

- [ ] **Step 8: If verification surfaces cleanups, apply + commit**

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR11 cleanup pass"
```

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| Global registry (singleton create, idempotent, distinct-from-source) | 3 | `tree_global::registry::tests` |
| Count-based seal (below-threshold no-seal, weekly seal at 7, idempotent retry) | 3 | `tree_global::seal::tests` |
| Digest (empty-day no-op, populated emits L0, idempotent skip, multi-source fold) | 4 | `tree_global::digest::tests` |
| Recap (pick_level thresholds, empty→None, 1-day→L0, weekly fallback→L0) | 4 | `tree_global::recap::tests` |
| Adapter inherent methods (run_global_digest, global_recap empty→None) | 2 | `memory_bucket_seal::adapter::tests` |
| **Total new tests** | **16** | — |
| **PR10 tests preserved** | 202 | (unchanged) |
| **Module total** | **~218** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Full port — registry + seal + digest + recap + adapter methods + manual IPC trigger. All from the locked brainstorming decisions.
- ✅ **Scope check**: NO automatic scheduler (PR12). NO `.md` content staging (uClaw dropped it in PR8). NO entity-indexing-in-seal (PR8 doesn't either). NO schema migrations (TreeKind::Global + kind column already exist). NO recall-by-global wiring into effective_system_prompt (PR15).
- ✅ **Faithful where it matters**: count-based cascade (7/4/12), 2-tier representative-material pick, level-band recap with downward fallback, idempotent digest + buffer append.
- ✅ **uClaw idiom**: `lock_conn()` + `conn.transaction()` (not openhuman's `with_connection`/`unchecked_transaction`); single-arg `insert_summary_tx`; `refresh_last_sealed_tx` for same-level; embedding-before-tx — all mirror PR8's `bucket_seal.rs`.
- ✅ **Backlink difference documented**: global backlinks ALL levels (every level holds global-owned summaries), unlike PR8 which skips L0 (chunks). The digest L0 node itself is NOT backlinked (its children are cross-source references).
- ✅ **Adjacent edits called out**: IPC command in tauri_commands.rs + registration in main.rs invoke_handler! (CLAUDE.md rule). AppState gains concrete Arc<BucketSealAdapter>.
- ✅ **No placeholders**: every step has actual code or exact paths.
- ✅ **Bisectability**: 5 task commits (mod+registry / seal / digest / recap / wiring). Implementer may combine 1-4 if intermediate states don't compile (mod.rs references digest/recap before they exist) — same call as PR9.
- ✅ **No new workspace deps**: chrono/uuid/rusqlite/anyhow/async-trait/tracing/serde all present.
- ✅ **Full build in verification**: Task 6 runs `cargo build` (not just `--lib`) because the invoke_handler! wiring must compile end-to-end.
- ✅ **L0 preview limitation inherited**: digest folds previews (PR8 deferred full-body read). Documented as PR12 follow-up. Provenance prefix `[scope]\n` preserved.
- ✅ **PR12 prep**: the jobs subsystem will wire `run_global_digest` to a daily scheduler + swap InertSummariser/InertEmbedder for Ollama/LLM (no change needed in tree_global — the Arc<dyn> injection already abstracts it).
