# 阶段 4 PR8 — `memory_bucket_seal::tree_source` port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port openhuman's source-tree bucket-seal mechanics into `memory_bucket_seal/tree_source`. The cascade-seal flow turns admitted chunks into a hierarchy of sealed summary nodes per ingest source. Once this lands, PR9 can ship `BucketSealAdapter` as a real `MemoryAdapter` impl. This is "THE core" per the design spec.

**Architecture:** Faithful port of `openhuman/src/openhuman/memory/tree/tree_source/{mod, types, registry, store, bucket_seal, summariser/{mod, inert}}.rs`. PR5's `SCHEMA` constant in `memory_bucket_seal/store.rs` gets 3 new tables (`mem_tree_trees`, `mem_tree_summaries`, `mem_tree_buffers`). The `Summariser` trait + `InertSummariser` (concat + truncate fallback) lets the seal cascade run end-to-end without an LLM. `Embedder` from PR7 is called at seal time to populate `mem_tree_summaries.embedding` — `InertEmbedder` returns zeros in tests. `LabelStrategy::ExtractFromContent` is **dropped** (depends on extract which we deferred); only `UnionFromChildren` and `Empty` ship.

**Tech Stack:** Rust, `chrono`, `serde`, `rusqlite`, `anyhow`, `async-trait`, `tracing`. No new workspace deps.

---

## Source-of-truth references

Openhuman files this PR ports from (read fully before each task):
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/mod.rs` (33) — re-exports
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/types.rs` (252) — port verbatim
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/registry.rs` (230) — SLIM PORT (`&Config` → `&BucketSealStore`)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/store.rs` (522) — SLIM PORT (drop extract-coupled methods)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/bucket_seal.rs` (873) — SLIM PORT (drop `LabelStrategy::ExtractFromContent` variant + extract imports + `resolve_labels` extract arm)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/store_tests.rs` (227) — port relevant tests inline
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/bucket_seal_tests.rs` (661) — port relevant tests inline (drop tests that exercise `ExtractFromContent`)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/summariser/mod.rs` (153) — SLIM PORT (drop `build_summariser` factory — depends on openhuman `Config`; keep trait + `SummaryInput`/`SummaryOutput`/`SummaryContext`)
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/summariser/inert.rs` (181) — port verbatim

**DO NOT port** (entirely):
- `tree_source/flush.rs` (270 LoC — time-based stale buffer flush; defer to a follow-up small PR)
- `tree_source/source_file.rs` (213 LoC — Obsidian vault `.md` writer for trees; defer to a follow-up small PR)
- `tree_source/summariser/llm.rs` (665 LoC — LLM-driven summariser; defer to PR12 with jobs)
- All extract-coupled methods in `store.rs` (none ship)

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/store.rs` (modify, +60 lines) | Extend `SCHEMA` constant with 3 tables + 6 indexes + 2 FKs. NO new methods added — `tree_source::store` owns those. | +60 |
| `src-tauri/src/memory_bucket_seal/tree_source/types.rs` (new) | `Tree`, `SummaryNode`, `Buffer`, `TreeKind`, `TreeStatus` + constants (`INPUT_TOKEN_BUDGET`, `OUTPUT_TOKEN_BUDGET`, `SUMMARY_FANOUT`, `DEFAULT_FLUSH_AGE_SECS`) + 4 inline tests | ~260 |
| `src-tauri/src/memory_bucket_seal/tree_source/registry.rs` (new) | `get_or_create_source_tree(store, scope)` + `new_summary_id(level)` + tests | ~160 |
| `src-tauri/src/memory_bucket_seal/tree_source/store.rs` (new) | `insert_tree`, `get_tree_by_scope`, `get_tree`, `list_trees_by_kind`, `insert_summary`, `set_summary_embedding`, `get_summary_embedding`, `get_summary`, `list_summaries_at_level`, `count_summaries`, `get_buffer`, `get_buffer_conn` (tx variant), `upsert_buffer_tx` + tests | ~650 |
| `src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs` (new) | `LeafRef`, `LabelStrategy { UnionFromChildren, Empty }`, `append_leaf(store, tree, leaf, summariser, embedder, strategy)`, `append_leaf_deferred`, `cascade_seals`, `cascade_all_from`, `seal_one_level`, internal helpers + tests | ~750 |
| `src-tauri/src/memory_bucket_seal/tree_source/summariser/mod.rs` (new) | `Summariser` trait + `SummaryInput` + `SummaryOutput` + `SummaryContext`. NO `build_summariser` factory. | ~70 |
| `src-tauri/src/memory_bucket_seal/tree_source/summariser/inert.rs` (new) | `InertSummariser` concat + truncate fallback + tests | ~190 |
| `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (new) | Module declarations + re-exports | ~40 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +6 lines) | `pub mod tree_source;` + re-exports for `append_leaf`, `LabelStrategy`, `LeafRef`, `get_or_create_source_tree`, `InertSummariser`, `Summariser`, `Tree`, `SummaryNode`, `Buffer`, constants | +6 |

**LoC budget**: ~2120 source + ~450 tests = **~2570 LoC total**. Within "THE core" magnitude.

---

## Decisions Already Locked

- **Module path**: `memory_bucket_seal/tree_source/{mod, types, registry, store, bucket_seal}.rs` + `summariser/{mod, inert}.rs`. Nested directory because tree_source has multiple sub-modules.
- **Schema extension**: 3 new tables added to PR5's `SCHEMA` constant in `memory_bucket_seal/store.rs`. `BucketSealStore::ensure_schema()` already idempotent. `mem_tree_summaries.embedding BLOB DEFAULT NULL` column included from the start (greenfield).
- **`LabelStrategy` slim**: only `UnionFromChildren` and `Empty` variants ship. `ExtractFromContent(Arc<dyn EntityExtractor>)` is DROPPED — extract isn't ported. Source trees default to `LabelStrategy::Empty` in PR9's adapter wiring. PR8's tests cover both variants.
- **`Summariser` trait**: async `summarise(inputs, ctx) -> Result<SummaryOutput>` matching openhuman verbatim. `Send + Sync` bound. `InertSummariser` is the only impl shipped in PR8.
- **`build_summariser` factory dropped**: openhuman's factory reads `Config.memory_provider`, `Config.workload_*`, etc. We don't have that config. PR9's adapter constructs `Arc<InertSummariser>` directly until PR12 wires LLM.
- **`Embedder` integration**: `bucket_seal::seal_one_level` calls `embedder.embed(summary.content).await` BEFORE persisting the summary row. If embed fails, the seal aborts (summary not written, buffer not cleared — retry will succeed once embedder recovers). PR8 tests use `InertEmbedder` from PR7.
- **`append_leaf` is `async`**: it awaits both `summariser.summarise()` and `embedder.embed()`. Plus all SQL ops are sync (`rusqlite` is sync). The `async fn` is mostly an `.await` channel.
- **Store I/O pattern**: `tree_source::store` operates on `&BucketSealStore`. Uses `store.lock_conn()?` (made `pub(crate)` in PR7). For transactional ops, expose `_tx` / `_conn` variants taking `&Transaction` or `&Connection` so `bucket_seal` can batch buffer + summary writes in one tx.
- **`new_summary_id` shape**: `format!("summary:L{level}:{uuid}")` with `uuid::Uuid::new_v4()`. Faithful port from openhuman.
- **No AppState wiring, no IPC**: PR9 does that.
- **No new deps**: `async-trait` already in workspace (PR1). `uuid` already in workspace (PR3).

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

For each task:

1. **Re-read the openhuman source file you're porting** before implementing. The plan's structure is a guide; openhuman is the source of truth for algorithm/structure.

2. **Import path rewrites** (systematic):
   - `use crate::openhuman::memory::tree::types::{...}` → `use crate::memory_bucket_seal::types::{...}`
   - `use crate::openhuman::memory::tree::tree_source::*` → `use crate::memory_bucket_seal::tree_source::*`
   - `use crate::openhuman::memory::tree::score::{embed::*, ...}` → `use crate::memory_bucket_seal::score::{embed::*, ...}`
   - `use crate::openhuman::memory::tree::score::extract::{...}` → **REMOVE**
   - `use crate::openhuman::config::Config` → **REMOVE**; replace with `&BucketSealStore` parameter (+ `&Arc<dyn Summariser>` + `&Arc<dyn Embedder>` where bucket_seal needs them)

3. **`log::*` → `tracing::*`** with structured fields.

4. **`with_connection(config, |conn| { ... })` pattern** in openhuman becomes:
   ```rust
   let conn = store.lock_conn()?;
   // ... use conn ...
   ```
   For transactional ops:
   ```rust
   let mut conn = store.lock_conn()?;
   let tx = conn.transaction()?;
   // ... use tx ...
   tx.commit()?;
   ```

5. **`LabelStrategy` drop**: in `bucket_seal.rs`, REMOVE the `ExtractFromContent(Arc<dyn EntityExtractor>)` variant entirely. REMOVE the `Debug` impl arm for it. In `resolve_labels`, REMOVE the `LabelStrategy::ExtractFromContent(extractor) => { ... }` match arm (drop ~30 lines incl `canonicalise` import).

6. **`mem_tree_summaries.embedding BLOB`**: openhuman has this column added via an ALTER TABLE somewhere; for our greenfield port, include it in the CREATE TABLE statement directly (`embedding BLOB DEFAULT NULL`).

7. **Test fidelity**: openhuman has external `bucket_seal_tests.rs` + `store_tests.rs` files. uClaw convention is inline `#[cfg(test)]`. Port the cases that DON'T involve `ExtractFromContent`. Tests that build `Arc<dyn EntityExtractor>` mocks should be dropped or rewritten to use `LabelStrategy::Empty`.

8. **`uuid::Uuid::new_v4()` import**: PR3 added `uuid = { version = "1", features = ["v4", "serde"] }` to workspace deps. Verify with `grep -n uuid src-tauri/Cargo.toml` before using.

9. **FK enforcement**: PR5 sets `PRAGMA foreign_keys = ON`. Tests for `mem_tree_summaries`/`mem_tree_buffers` must insert a tree row first before referencing it.

10. **`approx_token_count` import**: `InertSummariser` uses this for truncation. PR5 already shipped it at `crate::memory_bucket_seal::types::approx_token_count`. Import path rewrite only.

11. **The `seal_one_level` function in `bucket_seal.rs`**: this is the heart of cascade-seal. Read openhuman's implementation FULLY. Key steps:
    1. Read buffer at `level`.
    2. Check `should_seal(buf)` — returns true if `token_sum >= INPUT_TOKEN_BUDGET` (L0) or `item_ids.len() >= SUMMARY_FANOUT` (L>=1).
    3. Build `SummaryInput`s from buffer items (look up chunk OR summary content via store).
    4. Build `SummaryContext` and call `summariser.summarise(inputs, ctx).await`.
    5. Compute `entities`/`topics` via `resolve_labels(strategy, inputs, summary.content).await` (only `UnionFromChildren` and `Empty` branches).
    6. Call `embedder.embed(summary.content).await` (PR8 integration).
    7. Build `SummaryNode { ..., embedding: Some(embedding) }`.
    8. In a single transaction: insert_summary + clear buffer + update tree's `max_level`/`root_id`/`last_sealed_at`.
    Adapt openhuman's implementation accordingly. If the openhuman code has `pack_checked(&embedding)?` to validate dimension before storing, port that too.

12. **`cascade_all_from` recursion**: after sealing level N, the new summary becomes an item in level N+1's buffer. Recurse until a level stays under budget. The return value is the list of summary ids that sealed during this call. Faithful port from openhuman.

13. **Pre-commit hooks**: same as previous PRs. Don't `--no-verify`.

---

### Task 1: Schema extension — 3 new tables

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/store.rs` (extend `SCHEMA` constant)

- [ ] **Step 1: Extend `SCHEMA` constant**

In `src-tauri/src/memory_bucket_seal/store.rs`, find the existing `SCHEMA` constant. After the existing tables (`mem_tree_chunks`, `mem_tree_score`), append:

```sql
CREATE TABLE IF NOT EXISTS mem_tree_trees (
    id                     TEXT PRIMARY KEY,
    kind                   TEXT NOT NULL,
    scope                  TEXT NOT NULL,
    root_id                TEXT,
    max_level              INTEGER NOT NULL DEFAULT 0,
    status                 TEXT NOT NULL DEFAULT 'active',
    created_at_ms          INTEGER NOT NULL,
    last_sealed_at_ms      INTEGER
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_mem_tree_trees_kind_scope
    ON mem_tree_trees(kind, scope);
CREATE INDEX IF NOT EXISTS idx_mem_tree_trees_status
    ON mem_tree_trees(status);

CREATE TABLE IF NOT EXISTS mem_tree_summaries (
    id                     TEXT PRIMARY KEY,
    tree_id                TEXT NOT NULL,
    tree_kind              TEXT NOT NULL,
    level                  INTEGER NOT NULL,
    parent_id              TEXT,
    child_ids_json         TEXT NOT NULL DEFAULT '[]',
    content                TEXT NOT NULL,
    token_count            INTEGER NOT NULL,
    entities_json          TEXT NOT NULL DEFAULT '[]',
    topics_json            TEXT NOT NULL DEFAULT '[]',
    time_range_start_ms    INTEGER NOT NULL,
    time_range_end_ms      INTEGER NOT NULL,
    score                  REAL NOT NULL DEFAULT 0.0,
    sealed_at_ms           INTEGER NOT NULL,
    deleted                INTEGER NOT NULL DEFAULT 0,
    embedding              BLOB,
    FOREIGN KEY (tree_id) REFERENCES mem_tree_trees(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_tree_level
    ON mem_tree_summaries(tree_id, level);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_parent
    ON mem_tree_summaries(parent_id);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_sealed_at
    ON mem_tree_summaries(sealed_at_ms);
CREATE INDEX IF NOT EXISTS idx_mem_tree_summaries_deleted
    ON mem_tree_summaries(deleted);

CREATE TABLE IF NOT EXISTS mem_tree_buffers (
    tree_id                TEXT NOT NULL,
    level                  INTEGER NOT NULL,
    item_ids_json          TEXT NOT NULL DEFAULT '[]',
    token_sum              INTEGER NOT NULL DEFAULT 0,
    oldest_at_ms           INTEGER,
    updated_at_ms          INTEGER NOT NULL,
    PRIMARY KEY (tree_id, level),
    FOREIGN KEY (tree_id) REFERENCES mem_tree_trees(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_buffers_oldest
    ON mem_tree_buffers(oldest_at_ms);
```

- [ ] **Step 2: Verify build + idempotent schema**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::store::tests::ensure_schema_is_idempotent 2>&1 | tail`
Expected: 1 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/store.rs
git commit -m "feat(memory_bucket_seal): mem_tree_trees + summaries + buffers schema (PR8.1 of 阶段 4)"
```

---

### Task 2: `tree_source/types.rs` + module skeleton

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (skeleton only at this step)
- Create: `src-tauri/src/memory_bucket_seal/tree_source/types.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (add `pub mod tree_source;`)

- [ ] **Step 1: Skeleton `tree_source/mod.rs`**

```rust
//! Source-tree bucket-seal mechanics (openhuman port — Phase 3a).
//!
//! Lifts admitted chunks into a hierarchy of sealed summary nodes, one tree
//! per ingest source. Public surface at PR8:
//! - [`registry::get_or_create_source_tree`] — idempotent tree lookup
//! - [`bucket_seal::append_leaf`] — push a chunk into its tree, cascade-seal on budget
//! - [`summariser::inert::InertSummariser`] — deterministic fallback summariser
//!
//! Defers: `flush.rs` (time-based seal), `source_file.rs` (Obsidian vault output),
//! `summariser/llm.rs` (LLM-driven summariser, PR12+).

pub mod types;
// Other modules land in Tasks 3-6.
```

- [ ] **Step 2: Port `types.rs` verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/types.rs` in full. Port verbatim — only import path rewrites needed:
- `use crate::openhuman::memory::tree::...` — there are NO such imports in this file (it's pure types + chrono + serde). No edits needed.

The file ships:
- `TreeKind` enum (Source/Topic/Global) + `as_str`/`parse`
- `TreeStatus` enum (Active/Archived) + `as_str`/`parse`
- `Tree` struct
- `SummaryNode` struct (note: `embedding: Option<Vec<f32>>` field with `#[serde(default)]`)
- `Buffer` struct + `empty`/`is_empty`/`is_stale` methods
- Constants: `INPUT_TOKEN_BUDGET = 50_000`, `OUTPUT_TOKEN_BUDGET = 5_000`, `SUMMARY_FANOUT = 10`, `DEFAULT_FLUSH_AGE_SECS = 7*24*60*60`
- 4 inline tests (tree_kind_round_trip, tree_status_round_trip, empty_buffer_is_not_stale, stale_buffer_detected)

- [ ] **Step 3: Update `memory_bucket_seal/mod.rs`**

Add `pub mod tree_source;` to the existing `pub mod` block (alphabetical, near other `pub mod` declarations).

- [ ] **Step 4: Build + run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::types 2>&1 | tail -10`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/ src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): tree_source types + module skeleton (PR8.2 of 阶段 4)"
```

---

### Task 3: `tree_source/summariser/` — trait + InertSummariser

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/summariser/mod.rs`
- Create: `src-tauri/src/memory_bucket_seal/tree_source/summariser/inert.rs`
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (add `pub mod summariser;`)

- [ ] **Step 1: Port `summariser/mod.rs` slim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/summariser/mod.rs` in full. Port:
- `SummaryInput` struct (id, content, token_count, entities, topics, time_range_start, time_range_end, score)
- `SummaryContext<'a>` struct (tree_id, tree_kind, target_level, token_budget)
- `SummaryOutput` struct (content, token_count, entities, topics)
- `Summariser` trait (async `summarise(inputs, ctx) -> Result<SummaryOutput>` with `Send + Sync` bound)
- `pub mod inert;`

**DO NOT** port:
- `pub mod llm;` declaration
- `pub fn build_summariser(config: &Config) -> Arc<dyn Summariser>` factory
- All `use crate::openhuman::config::*` and `use crate::openhuman::memory::tree::chat::*` imports

Top of file:

```rust
//! Summariser trait + fallback (openhuman port — Phase 3a).
//!
//! Folds N buffered items into one sealed summary. PR8 ships an
//! [`InertSummariser`] that concatenates contributions and truncates to budget
//! — enough to make the tree mechanics observable end-to-end without an LLM.
//! [`LlmSummariser`] is deferred to PR12 with the jobs worker pool.

use anyhow::Result;
use async_trait::async_trait;
use chrono::{DateTime, Utc};

use crate::memory_bucket_seal::tree_source::types::TreeKind;

pub mod inert;

// SummaryInput, SummaryContext, SummaryOutput, Summariser trait — port verbatim
```

- [ ] **Step 2: Port `summariser/inert.rs` verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/summariser/inert.rs` in full. Port verbatim with import rewrites:
- `use crate::openhuman::memory::tree::tree_source::summariser::{...}` → `use crate::memory_bucket_seal::tree_source::summariser::{...}`
- `use crate::openhuman::memory::tree::types::approx_token_count;` → `use crate::memory_bucket_seal::types::approx_token_count;`

Port the inline tests verbatim.

- [ ] **Step 3: Wire `pub mod summariser;` into `tree_source/mod.rs`**

```rust
pub mod summariser;
```

- [ ] **Step 4: Build + run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::summariser 2>&1 | tail -15`
Expected: openhuman's inert tests (~4-6 tests) all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/
git commit -m "feat(memory_bucket_seal): Summariser trait + InertSummariser (PR8.3 of 阶段 4)"
```

---

### Task 4: `tree_source/store.rs` — SQLite I/O

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/store.rs`
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (add `pub mod store;`)

- [ ] **Step 1: Port slim store from openhuman**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/store.rs` in full (522 LoC). Port the following functions, adapting `&Config` → `&BucketSealStore` and `with_connection(config, |conn| { ... })` → `let conn = store.lock_conn()?;`:

**Tree I/O:**
- `pub fn insert_tree(store: &BucketSealStore, tree: &Tree) -> Result<()>`
- `pub fn get_tree_by_scope(store: &BucketSealStore, kind: TreeKind, scope: &str) -> Result<Option<Tree>>`
- `pub fn get_tree(store: &BucketSealStore, id: &str) -> Result<Option<Tree>>`
- `pub fn list_trees_by_kind(store: &BucketSealStore, kind: TreeKind) -> Result<Vec<Tree>>`
- `pub fn update_tree_after_seal_tx(tx: &Transaction, tree_id: &str, max_level: u32, root_id: &str, last_sealed_at: DateTime<Utc>) -> Result<()>` (or similar — verify the name in openhuman)

**Summary I/O:**
- `pub(crate) fn insert_summary_tx(tx: &Transaction, summary: &SummaryNode) -> Result<()>` (transactional variant — bucket_seal batches with buffer clear)
- `pub fn get_summary(store: &BucketSealStore, id: &str) -> Result<Option<SummaryNode>>`
- `pub fn list_summaries_at_level(store: &BucketSealStore, tree_id: &str, level: u32) -> Result<Vec<SummaryNode>>`
- `pub fn count_summaries(store: &BucketSealStore, tree_id: &str) -> Result<u64>`
- `pub fn set_summary_embedding(store: &BucketSealStore, summary_id: &str, embedding: &[f32]) -> Result<()>` (PR12 will use this; PR8 sets it via `insert_summary_tx`)
- `pub fn get_summary_embedding(store: &BucketSealStore, summary_id: &str) -> Result<Option<Vec<f32>>>`

**Buffer I/O:**
- `pub fn get_buffer(store: &BucketSealStore, tree_id: &str, level: u32) -> Result<Buffer>`
- `pub(crate) fn get_buffer_conn(conn: &Connection, tree_id: &str, level: u32) -> Result<Buffer>` (used by bucket_seal in a tx)
- `pub(crate) fn upsert_buffer_tx(tx: &Transaction, buf: &Buffer) -> Result<()>`
- `pub(crate) fn clear_buffer_tx(tx: &Transaction, tree_id: &str, level: u32) -> Result<()>`

**Drop (don't port)**:
- Any methods returning `EntityIndexRow` / `EntityHit` (extract-coupled, PR8+ scope)
- Any methods that take an `EntityExtractor` parameter

**Tests inline** (port from `store_tests.rs` minus extract-coupled cases): ~8-10 tests covering insert/get round-trips, buffer upsert idempotency, list_summaries ordering, FK enforcement.

Key row hydration: `mem_tree_summaries.embedding BLOB` ↔ `Vec<f32>` via PR7's `pack_checked` / `unpack_embedding` from `crate::memory_bucket_seal::score::embed::{pack_checked, unpack_embedding}`. NULL blob → `None`. Non-NULL → unpack and validate dimension.

- [ ] **Step 2: Add `pub mod store;` to `tree_source/mod.rs`**

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::store 2>&1 | tail -15`
Expected: ~8-10 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/store.rs src-tauri/src/memory_bucket_seal/tree_source/mod.rs
git commit -m "feat(memory_bucket_seal): tree_source SQLite I/O (PR8.4 of 阶段 4)"
```

---

### Task 5: `tree_source/registry.rs` — tree lookup

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/registry.rs`
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (add `pub mod registry;`)

- [ ] **Step 1: Port `registry.rs` slim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/registry.rs` in full (230 LoC). Port:
- `pub fn get_or_create_source_tree(store: &BucketSealStore, scope: &str) -> Result<Tree>`:
  - Try `store::get_tree_by_scope(store, TreeKind::Source, scope)` first.
  - If `Some(t)`, return.
  - If `None`, build a new `Tree` with id `format!("tree:source:{}", uuid::Uuid::new_v4())`, `kind: Source`, scope, `root_id: None`, `max_level: 0`, `status: Active`, `created_at: Utc::now()`, `last_sealed_at: None`. Call `store::insert_tree(store, &t)`. Return.
- `pub fn new_summary_id(level: u32) -> String` — format `"summary:L{level}:{uuid_v4_simple}"`.

Drop openhuman's `&Config` → use `&BucketSealStore`.

Port the relevant tests inline (~4-6 tests: idempotent create, distinct scopes get distinct trees, new_summary_id format).

- [ ] **Step 2: Add `pub mod registry;`**

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::registry 2>&1 | tail -10`
Expected: ~4-6 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/registry.rs src-tauri/src/memory_bucket_seal/tree_source/mod.rs
git commit -m "feat(memory_bucket_seal): tree_source registry (PR8.5 of 阶段 4)"
```

---

### Task 6: `tree_source/bucket_seal.rs` — append_leaf + cascade-seal

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs`
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs` (add `pub mod bucket_seal;`)

- [ ] **Step 1: Port `bucket_seal.rs` slim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/tree_source/bucket_seal.rs` in full (873 LoC). Port:

**Public surface:**
- `LabelStrategy` enum with ONLY `UnionFromChildren` and `Empty` variants (DROP `ExtractFromContent`)
- `LeafRef` struct (chunk_id, token_count, timestamp, content, entities, topics, score)
- `pub async fn append_leaf(store: &BucketSealStore, tree: &Tree, leaf: &LeafRef, summariser: &Arc<dyn Summariser>, embedder: &Arc<dyn Embedder>, strategy: &LabelStrategy) -> Result<Vec<String>>`
- `pub fn append_leaf_deferred(store: &BucketSealStore, tree: &Tree, leaf: &LeafRef) -> Result<bool>` (returns true if L0 buffer crossed threshold and a follow-up seal job is needed)
- `pub async fn cascade_all_from(store: &BucketSealStore, tree: &Tree, start_level: u32, summariser: &Arc<dyn Summariser>, embedder: &Arc<dyn Embedder>, force_now: Option<DateTime<Utc>>, strategy: &LabelStrategy) -> Result<Vec<String>>`

**Private helpers:**
- `fn append_to_buffer(store, tree_id, level, item_id, token_delta, item_ts) -> Result<()>` (transactional, idempotent on `(tree_id, level, item_id)`)
- `fn should_seal(buf: &Buffer, level: u32) -> bool` — `level == 0 ? token_sum >= INPUT_TOKEN_BUDGET : item_ids.len() >= SUMMARY_FANOUT as usize`
- `async fn seal_one_level(store, tree, level, summariser, embedder, strategy) -> Result<Option<String>>` — the heart of cascade-seal (see Adaptation Responsibility #11)
- `async fn resolve_labels(strategy: &LabelStrategy, inputs: &[SummaryInput], summary_content: &str) -> Result<(Vec<String>, Vec<String>)>` — match arms ONLY for `UnionFromChildren` and `Empty`

**Drop entirely:**
- `LabelStrategy::ExtractFromContent(Arc<dyn EntityExtractor>)` variant
- The `ExtractFromContent` arm in `resolve_labels`
- The `Debug` impl arm for `ExtractFromContent`
- All `use crate::openhuman::memory::tree::score::extract::*` imports
- All `use crate::openhuman::memory::tree::score::resolver::*` imports (canonicalise was from there)

**`seal_one_level` algorithm** (faithful port):
1. Lock conn + begin tx.
2. Read buffer at `level`.
3. If `!should_seal(buf, level) && !force_now`, return `Ok(None)`.
4. Hydrate `inputs: Vec<SummaryInput>` from buffer item_ids (look up chunks at L0, summaries at L>=1).
5. Compute `time_range_start = min(inputs.time_range_start)`, `time_range_end = max(inputs.time_range_end)`, `score = max(inputs.score)`.
6. Commit tx (release lock for the async work).
7. Build `SummaryContext { tree_id: &tree.id, tree_kind: tree.kind, target_level: level + 1, token_budget: OUTPUT_TOKEN_BUDGET }`.
8. Call `summariser.summarise(&inputs, &ctx).await` → `SummaryOutput`.
9. Call `resolve_labels(strategy, &inputs, &summary_output.content).await` → `(entities, topics)`.
10. Call `embedder.embed(&summary_output.content).await` → `embedding`. Validate via `pack_checked(&embedding)` (or store as raw `Vec<f32>` until insert_summary_tx packs it).
11. Build `SummaryNode { id: new_summary_id(level + 1), tree_id, tree_kind, level: level + 1, parent_id: None, child_ids: buf.item_ids.clone(), content: summary_output.content, token_count: summary_output.token_count, entities, topics, time_range_start, time_range_end, score, sealed_at: Utc::now(), deleted: false, embedding: Some(embedding) }`.
12. Acquire conn + begin tx again. Insert summary, clear buffer at `level`, update tree's `max_level`/`root_id`/`last_sealed_at`. Commit.
13. Return `Ok(Some(summary.id))`.

**`cascade_all_from` algorithm:**
1. Set `current_level = start_level`. `sealed_ids = vec![]`.
2. Loop:
   a. Call `seal_one_level(... current_level ...)`.
   b. If returns `Some(id)`, push `id` to `sealed_ids`. Append the new summary to buffer at `current_level + 1` via `append_to_buffer`. Increment `current_level`.
   c. If returns `None`, break.
3. Return `Ok(sealed_ids)`.

**`append_leaf` algorithm:**
1. Call `append_to_buffer(store, &tree.id, 0, &leaf.chunk_id, leaf.token_count as i64, leaf.timestamp)`.
2. Return `cascade_all_from(store, tree, 0, summariser, embedder, None, strategy).await`.

**Tests inline** (port from `bucket_seal_tests.rs` minus extract-coupled cases): ~12-15 tests covering:
- append_leaf to empty tree (L0 buffer accumulates)
- append_leaf triggers L0→L1 seal when token_sum crosses INPUT_TOKEN_BUDGET
- cascade-seal L0→L1→L2 when buffer at each level fills
- append_leaf_deferred returns `should_seal` flag correctly
- append_to_buffer is idempotent on (tree_id, level, item_id)
- Cascade-seal updates tree.root_id and tree.max_level
- Cascade-seal updates tree.last_sealed_at
- `LabelStrategy::Empty` produces summaries with empty entities/topics
- `LabelStrategy::UnionFromChildren` unions labels from inputs
- Embedder failure aborts the seal (use a custom failing embedder for this test — or assert that InertEmbedder produces a 1024-zero embedding)
- Each test fixture builds a fresh store + InertSummariser + InertEmbedder

- [ ] **Step 2: Add `pub mod bucket_seal;`**

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::bucket_seal 2>&1 | tail -20`
Expected: ~12-15 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs src-tauri/src/memory_bucket_seal/tree_source/mod.rs
git commit -m "feat(memory_bucket_seal): append_leaf + cascade-seal (PR8.6 of 阶段 4)"
```

---

### Task 7: Module wiring + re-exports

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/mod.rs`
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs`

- [ ] **Step 1: Finalize `tree_source/mod.rs`**

```rust
//! Source-tree bucket-seal mechanics (openhuman port — Phase 3a).
//!
//! Public surface at PR8:
//! - [`registry::get_or_create_source_tree`] — idempotent tree lookup
//! - [`bucket_seal::append_leaf`] — push a chunk into its tree, cascade-seal on budget
//! - [`summariser::inert::InertSummariser`] — deterministic fallback summariser
//!
//! Deferred to follow-up PRs:
//! - `flush.rs` (time-based stale buffer seal)
//! - `source_file.rs` (Obsidian vault .md writer for trees)
//! - `summariser/llm.rs` (LLM-driven summariser, PR12)

pub mod bucket_seal;
pub mod registry;
pub mod store;
pub mod summariser;
pub mod types;

pub use bucket_seal::{append_leaf, append_leaf_deferred, LabelStrategy, LeafRef};
pub use registry::get_or_create_source_tree;
pub use store::{get_summary_embedding, set_summary_embedding};
pub use summariser::{inert::InertSummariser, Summariser};
pub use types::{
    Buffer, SummaryNode, Tree, TreeKind, TreeStatus, INPUT_TOKEN_BUDGET, OUTPUT_TOKEN_BUDGET,
    SUMMARY_FANOUT,
};
```

- [ ] **Step 2: Update `memory_bucket_seal/mod.rs`**

Add `pub mod tree_source;` to the existing `pub mod` block. Add to the existing `pub use` block:

```rust
pub use tree_source::{
    append_leaf, get_or_create_source_tree, Buffer, InertSummariser, LabelStrategy, LeafRef,
    Summariser, SummaryNode, Tree, TreeKind, TreeStatus, INPUT_TOKEN_BUDGET, OUTPUT_TOKEN_BUDGET,
    SUMMARY_FANOUT,
};
```

- [ ] **Step 3: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: 115 (PR7 baseline) + ~30 new = ~145 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/mod.rs src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): wire tree_source re-exports (PR8.7 of 阶段 4)"
```

---

### Task 8: End-to-end integration test

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (append to existing `#[cfg(test)]` block)

- [ ] **Step 1: Add e2e test — chat batch → tree → seal**

Append to the `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn end_to_end_chat_batch_to_l1_seal() {
        use crate::memory_bucket_seal::canonicalize::chat::{canonicalise, ChatBatch, ChatMessage};
        use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
        use crate::memory_bucket_seal::score::embed::InertEmbedder;
        use crate::memory_bucket_seal::store::BucketSealStore;
        use crate::memory_bucket_seal::tree_source::{
            append_leaf, get_or_create_source_tree, store as ts_store, InertSummariser,
            LabelStrategy, LeafRef, INPUT_TOKEN_BUDGET,
        };
        use chrono::{TimeZone, Utc};
        use std::sync::Arc;
        use tempfile::TempDir;

        // 1. Set up a tree + summariser + embedder.
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();

        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(InertSummariser::new());
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());

        // 2. Build enough leaves to cross INPUT_TOKEN_BUDGET (50k tokens) so L0 seals.
        // Each leaf is ~5000 tokens — 11 leaves = 55k, triggering one L0→L1 seal.
        let mut sealed_ids: Vec<String> = Vec::new();
        let chunks_per_leaf = 5000_u32;
        for i in 0..11 {
            let ts = Utc.timestamp_millis_opt(1_700_000_000_000 + i as i64 * 1000).unwrap();
            let leaf = LeafRef {
                chunk_id: format!("chunk_{i:02}"),
                token_count: chunks_per_leaf,
                timestamp: ts,
                content: format!("Leaf {i} content."),
                entities: vec![],
                topics: vec![],
                score: 0.8,
            };
            let result = append_leaf(&store, &tree, &leaf, &summariser, &embedder, &LabelStrategy::Empty).await.unwrap();
            sealed_ids.extend(result);
        }

        // 3. At least one L1 summary must have been emitted.
        assert!(!sealed_ids.is_empty(), "cascade-seal should fire at least one summary");
        assert_eq!(ts_store::count_summaries(&store, &tree.id).unwrap(), sealed_ids.len() as u64);

        // 4. The summary should be at level 1 (one level above the L0 leaves).
        let l1 = ts_store::list_summaries_at_level(&store, &tree.id, 1).unwrap();
        assert_eq!(l1.len(), sealed_ids.len(), "summaries should land at L1");

        // 5. Each summary should have embedding populated (InertEmbedder always returns Some).
        for s in &l1 {
            assert!(s.embedding.is_some(), "PR8 summaries must have embedding populated");
            assert_eq!(s.embedding.as_ref().unwrap().len(), 1024);
        }

        // 6. tree.last_sealed_at should be set; tree.max_level should be at least 1.
        let refreshed = ts_store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert!(refreshed.last_sealed_at.is_some());
        assert!(refreshed.max_level >= 1);
        assert!(refreshed.root_id.is_some());
    }
```

Adapt: if the implementer chose slightly different test fixture patterns or types, adjust the imports. The test must exercise: `get_or_create_source_tree` → `append_leaf` ≥11 times → at least one L1 seal fires → `mem_tree_summaries` has rows with `embedding` populated → tree's `root_id`/`max_level`/`last_sealed_at` updated.

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tests::end_to_end 2>&1 | tail -15`
Expected: 3 passed (PR6's e2e + PR7's e2e + new PR8 e2e).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "test(memory_bucket_seal): end-to-end chat → tree → L1 seal (PR8.8 of 阶段 4)"
```

---

### Task 9: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~145+ passed.

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive (baseline + ~30 new from PR8).

- [ ] **Step 3: Clippy on PR8 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep "tree_source" | head -20`
Expected: zero hits.

- [ ] **Step 4: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr8-tree-source-port && git diff main -- src-tauri/Cargo.toml`
Expected: empty (no new workspace deps).

- [ ] **Step 5: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nrE "TODO|FIXME|XXX" src/memory_bucket_seal/tree_source/`
Expected: zero hits.

- [ ] **Step 6: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR8 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| `types` (TreeKind/TreeStatus round-trip, Buffer empty/stale) | 4 | `tree_source::types::tests` |
| `summariser/mod` + `summariser/inert` (concat, truncate, empty) | ~6 | `tree_source::summariser::*::tests` |
| `store` (insert + get round-trip, buffer upsert, count, FK enforcement) | ~10 | `tree_source::store::tests` |
| `registry` (idempotent create, distinct scopes, new_summary_id format) | ~4 | `tree_source::registry::tests` |
| `bucket_seal` (append_leaf, cascade-seal L0→L1, cascade L0→L1→L2, append_to_buffer idempotent, LabelStrategy::Empty + UnionFromChildren, append_leaf_deferred) | ~15 | `tree_source::bucket_seal::tests` |
| End-to-end chat → tree → L1 seal | 1 | `memory_bucket_seal::tests` |
| **Total new tests** | **~40** | — |
| **PR7 tests preserved** | 115 | (unchanged) |
| **Module total** | **~155** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B from brainstorming → types + registry + store + bucket_seal + summariser/{trait, inert}. Schema extended with 3 new tables.
- ✅ **Scope check**: NO `flush.rs`, NO `source_file.rs`, NO `summariser/llm.rs`. NO `LabelStrategy::ExtractFromContent`. NO extract-coupled store methods. NO AppState wiring. NO IPC.
- ✅ **Faithful port**: `Tree`/`SummaryNode`/`Buffer` shapes match openhuman bit-for-bit. Constants (`INPUT_TOKEN_BUDGET = 50_000`, `SUMMARY_FANOUT = 10`) verbatim. `seal_one_level` algorithm matches the openhuman flow (read buffer → hydrate inputs → summarise → resolve labels → embed → insert + clear in one tx).
- ✅ **Embedder integration**: `seal_one_level` calls `embedder.embed()` before persisting. `InertEmbedder` in tests. `mem_tree_summaries.embedding BLOB` populated on every new seal.
- ✅ **FK discipline**: `mem_tree_summaries.tree_id → mem_tree_trees.id` and `mem_tree_buffers.tree_id → mem_tree_trees.id` FKs included. Tests insert tree rows before referencing them.
- ✅ **Bisectability**: 9 task commits (schema / types / summariser / store / registry / bucket_seal / wiring / e2e / cleanup). Each compiles standalone except the wiring step requires its dependencies.
- ✅ **No new deps**: `async-trait` (PR1), `uuid` (PR3), `chrono`/`serde`/`rusqlite` (workspace) all already present.
- ✅ **`tracing::*` discipline**: no `log::*` slips.
- ✅ **Test fidelity**: openhuman test names preserved where possible; extract-coupled tests dropped or rewritten to use `LabelStrategy::Empty`.
- ✅ **PR9 readiness**: after PR8 lands, `BucketSealAdapter` (PR9) can wrap this with: `recall` = FTS over `mem_tree_chunks` scoped by namespace, `store` = chunker + score + `append_leaf`. All the machinery is in place.
