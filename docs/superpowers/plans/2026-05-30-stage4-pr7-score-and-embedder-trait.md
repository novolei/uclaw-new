# 阶段 4 PR7 — `memory_bucket_seal` score + Embedder trait port Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port openhuman's score subsystem (signals + admission gate + score table) AND the `Embedder` trait + `InertEmbedder` no-op impl. Defer the real Ollama backend (~975 LoC of HTTP+JSON) to PR12 where it lands naturally with the jobs worker pool. Defer entity extraction (`extract/`, ~1700 LoC) permanently — uClaw's LLM stack differs from openhuman, and the score admission gate works without entity_density (defaults to 0).

**Architecture:** Faithful port of `openhuman/src/openhuman/memory/tree/score/{signals/*, mod, store, embed/{mod, inert}}` into nested `memory_bucket_seal/score/`. PR5's `SCHEMA` constant in `memory_bucket_seal/store.rs` is extended with the `mem_tree_score` table + indexes. `score_chunk` is the slim orchestrator: signals → combine → admission decision → ScoreRow. No LLM call. No embedder call (PR12 wires that). `Embedder` trait + helpers (cosine_similarity, pack/unpack, EMBEDDING_DIM) ship so PR12 has a contract to fulfill.

**Tech Stack:** Rust, `serde`, `chrono`, `rusqlite`, `anyhow`, `async-trait` (for `Embedder` trait), `tracing`. No new workspace deps.

---

## Source-of-truth references

Openhuman files this PR ports from (read fully before each task):
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/types.rs` (66 LoC) — `ScoreSignals` + `SignalWeights`. Port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/ops.rs` (182 LoC) — `compute` + `combine` + `combine_cheap_only` + `entity_density_score`. **PARTIAL PORT**: skip the entity_density-from-ExtractedEntities path; provide a no-extract `compute` that defaults `entity_density = 0.0` and `llm_importance = 0.0`.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/token_count.rs` (84 LoC) — port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/unique_words.rs` (87 LoC) — port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/metadata_weight.rs` (49 LoC) — port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/source_weight.rs` (110 LoC) — port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/interaction.rs` (105 LoC) — port verbatim.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/mod.rs` (20 LoC) — re-exports.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/mod.rs` (431 LoC) — **SLIM PORT**: drop the `ScoringConfig.extractor`/`llm_extractor` fields; drop the `extracted`/`canonical_entities` fields on `ScoreResult`; drop `score_chunk`'s LLM-band integration. Keep: constants (`DEFAULT_DROP_THRESHOLD`, `DEFAULT_DEFINITE_KEEP`, `DEFAULT_DEFINITE_DROP`), slim `ScoringConfig { weights, drop_threshold }`, slim `ScoreResult { chunk_id, total, signals, kept, drop_reason }`, slim `score_chunk(chunk, config) -> ScoreResult` (sync, no LLM).
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/store.rs` (464 LoC) — **SLIM PORT**: keep only `ScoreRow` struct + `upsert_score` + `get_score` + `count_scores`. DROP all `entity_index_*` / `index_entity*` / `lookup_entity` / `EntityHit` / `list_entity_ids_for_node` (PR8+ territory).
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/embed/mod.rs` (235 LoC) — port full: `EMBEDDING_DIM = 1024`, `Embedder` trait, `cosine_similarity`, `pack_embedding`, `unpack_embedding`, `pack_checked`, `decode_optional_blob`, all 10 inline tests.
- `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/embed/inert.rs` (64 LoC) — port full: `InertEmbedder` ZST + 3 tests.

**DO NOT port** (entirely):
- `score/resolver.rs` (265 LoC — `CanonicalEntity` + `canonicalise` are extract-only concerns)
- `score/extract/` (1713 LoC across 6 files — entity extraction, defer permanently)
- `embed/cloud.rs` (101 LoC — defer to PR12)
- `embed/factory.rs` (255 LoC — defer to PR12)
- `embed/ollama.rs` (320 LoC — defer to PR12)
- `score/store.rs`'s entity_index methods (~250 LoC of the 464 total)

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/score/signals/types.rs` (new) | `ScoreSignals` + `SignalWeights` structs | ~70 |
| `src-tauri/src/memory_bucket_seal/score/signals/ops.rs` (new) | `compute_cheap` + `combine` + `combine_cheap_only` (no extract input) | ~140 |
| `src-tauri/src/memory_bucket_seal/score/signals/token_count.rs` (new) | Token count → [0,1] signal + tests | ~85 |
| `src-tauri/src/memory_bucket_seal/score/signals/unique_words.rs` (new) | Lexical diversity → [0,1] signal + tests | ~90 |
| `src-tauri/src/memory_bucket_seal/score/signals/metadata_weight.rs` (new) | Owner/tags weight → [0,1] signal + tests | ~50 |
| `src-tauri/src/memory_bucket_seal/score/signals/source_weight.rs` (new) | Per-SourceKind base weight + tests | ~115 |
| `src-tauri/src/memory_bucket_seal/score/signals/interaction.rs` (new) | Per-chunk interaction signal stub (always 0.0 in PR7 — no UI hook yet) + tests | ~110 |
| `src-tauri/src/memory_bucket_seal/score/signals/mod.rs` (new) | Re-exports | ~25 |
| `src-tauri/src/memory_bucket_seal/score/mod.rs` (new) | `DEFAULT_*` thresholds, slim `ScoringConfig`, slim `ScoreResult`, slim `score_chunk(chunk, config)` | ~150 |
| `src-tauri/src/memory_bucket_seal/score/store.rs` (new) | `ScoreRow` + `upsert_score` + `get_score` + `count_scores` (operate on the existing `BucketSealStore` connection) | ~260 |
| `src-tauri/src/memory_bucket_seal/score/embed/mod.rs` (new) | `EMBEDDING_DIM = 1024`, `Embedder` trait, cosine_similarity, pack/unpack helpers + 10 tests | ~240 |
| `src-tauri/src/memory_bucket_seal/score/embed/inert.rs` (new) | `InertEmbedder` ZST + 3 tests | ~65 |
| `src-tauri/src/memory_bucket_seal/store.rs` (modify, +30 lines) | Extend `SCHEMA` constant with `mem_tree_score` table + 2 indexes. NO new methods added — score::store owns those. | +30 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +6 lines) | `pub mod score;` + re-exports for `Embedder`, `InertEmbedder`, `EMBEDDING_DIM`, `ScoreResult`, `ScoringConfig`, `score_chunk` | +6 |
| `src-tauri/Cargo.toml` (modify, +1 line) | Add `async-trait = "0.1"` to `[dependencies]` IF NOT already present — see Adaptation Responsibility #2. | +1 (or 0) |

**LoC budget**: ~1400 source + ~300 tests = **~1700 LoC total**. Within PR5/PR6 magnitude.

---

## Decisions Already Locked

- **Module path**: `memory_bucket_seal/score/{mod, store}.rs` + `memory_bucket_seal/score/signals/*.rs` + `memory_bucket_seal/score/embed/{mod, inert}.rs`. Nested directory because score/ has sub-modules (signals/, embed/).
- **Schema strategy**: extend PR5's `SCHEMA` constant in `memory_bucket_seal/store.rs` to include `mem_tree_score` table. `BucketSealStore::ensure_schema()` already idempotent — old DBs get the new tables on next connect. Score::store does NOT define a separate schema.
- **No mem_tree_chunks `embedding BLOB` column yet**: PR7 ships the `Embedder` trait + helpers (pack_embedding/unpack_embedding/decode_optional_blob) but **does not persist embeddings**. PR12 (jobs) adds the schema column + actually invokes `Embedder::embed()` and stores results. PR7's helpers are tested standalone via the embed/mod.rs inline tests.
- **`Embedder` trait signature**: `async fn embed(&self, text: &str) -> Result<Vec<f32>>` returning a `Vec<f32>` of length `EMBEDDING_DIM = 1024`. `fn name(&self) -> &'static str` for diagnostics. Trait is `Send + Sync`. Verbatim from openhuman.
- **InertEmbedder is the only impl** in PR7. Returns `vec![0.0; EMBEDDING_DIM]` for every input. Used by tests today; PR12 swaps in OllamaEmbedder.
- **score::mod slim**: drops `extractor`/`llm_extractor`/`ExtractedEntities`/`CanonicalEntity` references. `ScoreResult` has 5 fields (id, total, signals, kept, drop_reason). `ScoringConfig` has 2 fields (weights, drop_threshold). `score_chunk` is `sync` — no `.await`.
- **signals::ops::compute_cheap signature**: `pub fn compute_cheap(chunk: &Chunk) -> ScoreSignals`. Sets `entity_density = 0.0` and `llm_importance = 0.0`. The full `compute(chunk, extracted)` from openhuman is NOT ported — defer when entity extraction lands.
- **interaction signal**: openhuman's `interaction.rs` has a stub that reads from a "user interaction recorder" (UI-driven). uClaw has no such recorder yet — port the file but the function always returns 0.0 with a `tracing::debug!` note. PR12+ wires real interaction tracking if/when needed.
- **`async-trait` dep**: required for the `Embedder` trait. Likely already in workspace (PR1's `MemoryAdapter` trait uses it). Verify and skip if present — see Adaptation Responsibility #2.
- **No AppState wiring**: PR9 wires. Score lives at the data layer.
- **No IPC, no Tauri commands**.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

For each task:

1. **Re-read the openhuman source file you're porting** before implementing. The plan's structure assumes openhuman's surface; the implementer verifies against the actual file.

2. **Verify `async-trait` is in workspace `Cargo.toml`**: PR1's `MemoryAdapter` trait at `src-tauri/src/memory_adapter/traits.rs` uses `#[async_trait]`. Run `grep -n "async-trait\|async_trait" src-tauri/Cargo.toml` — if present, use it; if not, add `async-trait = "0.1"` to `[dependencies]`. The plan's LoC budget includes this 1-line possible addition.

3. **Import rewrites** (systematic edit):
   - `use crate::openhuman::memory::tree::types::{...}` → `use crate::memory_bucket_seal::types::{...}`
   - `use crate::openhuman::memory::tree::score::extract::{...}` → **REMOVE** (we don't port extract)
   - `use crate::openhuman::memory::tree::score::signals::*` → `use crate::memory_bucket_seal::score::signals::*`
   - `use crate::openhuman::config::Config` → drop entirely. Score::store does NOT take a `Config`; it takes a `&BucketSealStore` (or for slim ops, a `&Connection` / `&Transaction` mirroring PR5's `upsert_staged_chunks` pattern).

4. **`log::*` → `tracing::*`** with structured fields throughout.

5. **`futures_util::future::try_join_all` import**: openhuman's `score::mod` uses this for parallel signal computation. We're slim-porting — signals compute sequentially in our `score_chunk`. **DROP** this import. If openhuman's `compute_cheap`-equivalent uses parallel-await, replace with sequential sync calls.

6. **`signals::ops::entity_density_score(token_count, ExtractedEntities)` is the extract-coupled signal**. **DO NOT port this function**. Instead, in `signals::ops`, expose:
   - `pub fn compute_cheap(chunk: &Chunk) -> ScoreSignals` — runs all cheap signals (token_count, unique_words, metadata_weight, source_weight, interaction) and zeros out `entity_density` + `llm_importance`.
   - `pub fn combine(signals: &ScoreSignals, w: &SignalWeights) -> f32` — port verbatim (the weight-combining formula doesn't need extract).
   - `pub fn combine_cheap_only(signals: &ScoreSignals, w: &SignalWeights) -> f32` — port verbatim.

7. **`score::store` does NOT use a `Config` — it operates on the existing `BucketSealStore` connection**. Pattern to follow:
   ```rust
   // score/store.rs
   use crate::memory_bucket_seal::store::BucketSealStore;
   pub fn upsert_score(store: &BucketSealStore, row: &ScoreRow) -> Result<()> { ... }
   ```
   The implementer may need to expose a small accessor on `BucketSealStore` (e.g., `pub(crate) fn lock_conn(...)` already exists from PR5 review fixes — verify and reuse). If `lock_conn` is private (`fn lock_conn`), make it `pub(crate)` so `score::store` can use it. This is a 1-character edit.

8. **`mem_tree_score` table FOREIGN KEY**: `chunk_id` references `mem_tree_chunks(id)`. PR5 sets `PRAGMA foreign_keys = ON` in `BucketSealStore::open()`. Including the FK is the faithful-port choice — but be aware that tests must insert a chunk before upserting its score, or the FK will fire. Test fixtures should chain stage_chunks + upsert_staged_chunks BEFORE upsert_score (mirroring openhuman tests).

9. **Test fidelity**: openhuman's `signals/*.rs` files each have inline tests. Port them all verbatim. `score/store_tests.rs` and `score/mod_tests.rs` are external test files at the score module root — port the relevant cases (drop entity_index test cases) and inline them into the corresponding `.rs` files (uClaw convention is inline `#[cfg(test)]` blocks, not external `_tests.rs` files). The PR5 + PR6 ports established this convention.

10. **`Embedder::embed` is async-but-uncalled in PR7**. The trait is defined and `InertEmbedder` implements it. PR12 wires the first caller (jobs::embed_pending_chunks or similar). Tests in `embed/inert.rs` exercise the trait via `#[tokio::test]`.

11. **`EMBEDDING_DIM = 1024`**: openhuman comments call this a `bge-m3` dimension. Keep verbatim — PR12's Ollama wiring will target bge-m3. The constant is hard-coded and the trait validates output length.

12. **Pre-commit hooks**: same as PR5/PR6. After each task commit, hooks run. Fix the underlying issue if any fails.

---

### Task 1: Schema extension — `mem_tree_score` table

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/store.rs` (extend `SCHEMA` constant)
- Modify: `src-tauri/src/memory_bucket_seal/store.rs` (expose `lock_conn` as `pub(crate)` if private)

- [ ] **Step 1: Extend `SCHEMA` constant**

In `src-tauri/src/memory_bucket_seal/store.rs`, find the existing `SCHEMA` constant (it currently contains `mem_tree_chunks` + indexes). Append the score table block before the closing `";`:

```sql
CREATE TABLE IF NOT EXISTS mem_tree_score (
    chunk_id               TEXT PRIMARY KEY,
    total                  REAL NOT NULL,
    token_count_signal     REAL NOT NULL,
    unique_words_signal    REAL NOT NULL,
    metadata_weight        REAL NOT NULL,
    source_weight          REAL NOT NULL,
    interaction_weight     REAL NOT NULL,
    entity_density         REAL NOT NULL,
    llm_importance         REAL NOT NULL DEFAULT 0.0,
    dropped                INTEGER NOT NULL DEFAULT 0,
    reason                 TEXT,
    computed_at_ms         INTEGER NOT NULL,
    FOREIGN KEY (chunk_id) REFERENCES mem_tree_chunks(id)
);

CREATE INDEX IF NOT EXISTS idx_mem_tree_score_total
    ON mem_tree_score(total);
CREATE INDEX IF NOT EXISTS idx_mem_tree_score_dropped
    ON mem_tree_score(dropped);
```

- [ ] **Step 2: Verify `lock_conn` visibility**

Run: `grep -n "fn lock_conn" src-tauri/src/memory_bucket_seal/store.rs`

If the function is declared as `fn lock_conn(...)` (private), change it to `pub(crate) fn lock_conn(...)`. If already `pub(crate)`, skip.

- [ ] **Step 3: Verify build + schema idempotency**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::store::tests::ensure_schema_is_idempotent 2>&1 | tail`
Expected: 1 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/store.rs
git commit -m "feat(memory_bucket_seal): mem_tree_score schema + expose lock_conn (PR7.1 of 阶段 4)"
```

---

### Task 2: `signals/` — pure scoring signals

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/score/signals/types.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/token_count.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/unique_words.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/metadata_weight.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/source_weight.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/interaction.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/ops.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/signals/mod.rs`

- [ ] **Step 1: Port `types.rs` from openhuman verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/types.rs` in full. Port verbatim — no import changes needed (it only uses `serde`).

- [ ] **Step 2: Port the 5 signal files verbatim**

For each of `token_count.rs`, `unique_words.rs`, `metadata_weight.rs`, `source_weight.rs`, `interaction.rs`:
- Read openhuman's file in full.
- Port verbatim with the only edit being:
  - `use crate::openhuman::memory::tree::types::{...}` → `use crate::memory_bucket_seal::types::{...}`
  - `log::*` → `tracing::*` (rare; signals are mostly pure math)
- Each file has inline tests; port them verbatim.

**Special note on `interaction.rs`**: openhuman's version reads from a global interaction recorder. uClaw doesn't have one. **The public function `interaction_signal(chunk: &Chunk) -> f32` always returns 0.0 in PR7**, with a one-line `tracing::debug!` indicating the stub. The function signature and tests stay; the inner body becomes a 1-line `0.0` return. The test that asserts "no recorded interactions → 0.0" still passes; other tests (if any) that pre-populate a recorder must be skipped or replaced with the trivial pass.

Adaptation responsibility: read the openhuman `interaction.rs` carefully. If it has multiple tests, port only the ones that exercise the zero-interaction path. Comment-mark any skipped test with `// SKIP: PR7 has no interaction recorder; revisit in a future PR.` (One short line, not a block.)

- [ ] **Step 3: Port `ops.rs` with slim `compute_cheap`**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/signals/ops.rs` in full. Port `compute` (rename to `compute_cheap`), `combine`, `combine_cheap_only` verbatim, BUT:

- DROP `entity_density_score(token_count, ExtractedEntities)` (extract-coupled).
- The renamed `compute_cheap(chunk: &Chunk) -> ScoreSignals` skips entity_density (always 0.0) and llm_importance (always 0.0). All other signal fields are populated by calling the per-signal functions.
- All openhuman imports of `ExtractedEntities` are dropped.

Skeleton:

```rust
use crate::memory_bucket_seal::score::signals::{
    interaction, metadata_weight, source_weight, token_count, unique_words,
};
use crate::memory_bucket_seal::score::signals::types::{ScoreSignals, SignalWeights};
use crate::memory_bucket_seal::types::Chunk;

/// Compute the cheap (no-LLM, no-extract) signal bundle for a chunk.
///
/// `entity_density` and `llm_importance` default to 0.0 — they're populated
/// when entity extraction and LLM rating are wired in (post-PR7).
pub fn compute_cheap(chunk: &Chunk) -> ScoreSignals {
    ScoreSignals {
        token_count: token_count::token_count_signal(chunk),
        unique_words: unique_words::unique_words_signal(chunk),
        metadata_weight: metadata_weight::metadata_weight_signal(chunk),
        source_weight: source_weight::source_weight_signal(chunk),
        interaction: interaction::interaction_signal(chunk),
        entity_density: 0.0,
        llm_importance: 0.0,
    }
}

/// Combine all signals into a final total. Faithful port of openhuman's
/// weighted-sum-with-clamp formula.
pub fn combine(signals: &ScoreSignals, w: &SignalWeights) -> f32 {
    // Port openhuman's combine() body verbatim.
}

/// Combine only the cheap signals (skips `entity_density` and `llm_importance`).
/// Used in PR7 since extract isn't wired. Faithful port from openhuman.
pub fn combine_cheap_only(signals: &ScoreSignals, w: &SignalWeights) -> f32 {
    // Port openhuman's combine_cheap_only() body verbatim.
}

// Port openhuman's inline tests for combine/combine_cheap_only verbatim.
// Drop tests that exercise entity_density.
```

**Adaptation responsibility**: function names in openhuman's signal files. Check the actual function names exposed by `token_count.rs`, `unique_words.rs`, etc., before writing the `compute_cheap` body. They may be named differently than `token_count_signal` etc. — port what's there.

- [ ] **Step 4: Port `mod.rs` re-exports**

```rust
//! Per-signal computations for the score admission gate.
//!
//! Faithful port of `openhuman::memory::tree::score::signals` minus the
//! `entity_density_score(extracted)` path (extract isn't wired in uClaw yet).

pub mod interaction;
pub mod metadata_weight;
pub mod ops;
pub mod source_weight;
pub mod token_count;
pub mod types;
pub mod unique_words;

pub use ops::{combine, combine_cheap_only, compute_cheap};
pub use types::{ScoreSignals, SignalWeights};
```

- [ ] **Step 5: Update `memory_bucket_seal/mod.rs`**

Add `pub mod score;` to the `pub mod` block. (The `score/mod.rs` file lands in Task 3, but adding the declaration here now will fail the build until then; defer the `pub mod score;` line to Task 3.)

For Task 2, the signals files compile standalone because they only use `types.rs` (PR5). No mod-level declaration in `memory_bucket_seal/mod.rs` yet — but `score/signals/` exists as a directory. **To make Task 2 standalone**: add a stub `src-tauri/src/memory_bucket_seal/score/mod.rs` that just declares `pub mod signals;` (5 LoC). Task 3 will fill in the rest.

- [ ] **Step 6: Build + run signal tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::signals 2>&1 | tail -15`
Expected: all signal-module tests pass. Count depends on how many openhuman ships per file — expect ~15-20.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/
git commit -m "feat(memory_bucket_seal): score signals (PR7.2 of 阶段 4)"
```

---

### Task 3: `score/store.rs` — score table I/O

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/score/store.rs`

- [ ] **Step 1: Port slim store from openhuman**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/store.rs` in full. Port ONLY:
- `ScoreRow` struct (likely fields: chunk_id, total, signal breakdown, dropped, reason, computed_at_ms)
- `upsert_score(store: &BucketSealStore, row: &ScoreRow) -> Result<()>`
- `get_score(store: &BucketSealStore, chunk_id: &str) -> Result<Option<ScoreRow>>`
- `count_scores(store: &BucketSealStore) -> Result<u64>`

**DROP**:
- `index_entity` / `index_entities` / `clear_entity_index_for_node` / `lookup_entity` / `EntityHit` / `list_entity_ids_for_node` / `count_entity_index` (all entity-index methods — PR8+ territory)

Adapt:
- Replace `&Config` parameter with `&BucketSealStore` everywhere.
- Use `BucketSealStore::lock_conn()` to get the connection guard (made `pub(crate)` in Task 1).
- Use `tracing::*` for logging.
- Return `anyhow::Result<...>` (same pattern as PR5/PR6).

- [ ] **Step 2: Inline tests**

Port the relevant tests from openhuman's `score/store_tests.rs` (~207 LoC). Keep only tests that exercise score row CRUD; drop entity-index tests. Inline them in `score/store.rs` under `#[cfg(test)] mod tests`.

Test fixture pattern (must respect the FK):
```rust
fn fresh_store() -> (BucketSealStore, TempDir) { ... }

fn seed_chunk(store: &BucketSealStore, dir: &Path, id: &str) {
    // Stage + upsert a real chunk first so the FK is satisfied
    let chunk = Chunk { id: id.to_string(), ..sample_chunk() };
    let staged = stage_chunks(dir, &[chunk]).unwrap();
    store.upsert_staged_chunks(&staged).unwrap();
}
```

- [ ] **Step 3: Update `score/mod.rs`**

In the `score/mod.rs` stub from Task 2, add `pub mod store;`.

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::store 2>&1 | tail -10`
Expected: ~5-7 passed (upsert_then_get, get_missing, count, dropped-flag round-trip, FK enforcement, etc.).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/store.rs src-tauri/src/memory_bucket_seal/score/mod.rs
git commit -m "feat(memory_bucket_seal): score::store row I/O (PR7.3 of 阶段 4)"
```

---

### Task 4: `score/mod.rs` — slim orchestrator

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/score/mod.rs` (replace the stub with the real orchestrator)

- [ ] **Step 1: Port slim `score::mod` from openhuman**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/mod.rs` in full. Port the slim version:

```rust
//! Phase 2: scoring / admission pipeline for bucket-seal.
//!
//! Faithful port of `openhuman::memory::tree::score` slimmed to the
//! cheap-signals-only path. Drops `ScoringConfig.extractor`, drops the
//! LLM band integration, drops `extracted` and `canonical_entities` on
//! `ScoreResult`. Entity extraction lands in a separate future PR.

pub mod embed;
pub mod signals;
pub mod store;

use serde::{Deserialize, Serialize};

use crate::memory_bucket_seal::types::Chunk;
use crate::memory_bucket_seal::score::signals::{
    compute_cheap, combine_cheap_only, ScoreSignals, SignalWeights,
};

/// Default drop threshold. Chunks with `total < DEFAULT_DROP_THRESHOLD`
/// are tombstoned and never reach the L0 buffer (PR8). Faithful port from openhuman.
pub const DEFAULT_DROP_THRESHOLD: f32 = 0.3;

/// Pre-LLM definite-keep band. Currently unused (no LLM extractor in PR7) —
/// preserved so PR8+ can wire the LLM band without changing the public surface.
pub const DEFAULT_DEFINITE_KEEP: f32 = 0.85;

/// Pre-LLM definite-drop band. Currently unused (no LLM extractor in PR7).
pub const DEFAULT_DEFINITE_DROP: f32 = 0.15;

/// Whole outcome of [`score_chunk`].
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreResult {
    pub chunk_id: String,
    pub total: f32,
    pub signals: ScoreSignals,
    pub kept: bool,
    pub drop_reason: Option<String>,
}

/// Configuration for [`score_chunk`].
#[derive(Clone, Debug)]
pub struct ScoringConfig {
    pub weights: SignalWeights,
    pub drop_threshold: f32,
}

impl Default for ScoringConfig {
    fn default() -> Self {
        Self {
            weights: SignalWeights::default(),
            drop_threshold: DEFAULT_DROP_THRESHOLD,
        }
    }
}

/// Score a chunk via the cheap-signals path (no LLM, no extract).
///
/// Returns a `ScoreResult` with the combined `total` and an admission
/// decision (`kept`). The orchestrator in PR8 will call this before
/// appending the chunk to the L0 buffer; dropped chunks are tombstoned
/// in the score table but never enter the buffer.
pub fn score_chunk(chunk: &Chunk, config: &ScoringConfig) -> ScoreResult {
    let signals = compute_cheap(chunk);
    let total = combine_cheap_only(&signals, &config.weights);
    let kept = total >= config.drop_threshold;
    let drop_reason = if kept {
        None
    } else {
        Some(format!(
            "cheap-signals total {:.3} below drop_threshold {:.3}",
            total, config.drop_threshold
        ))
    };
    ScoreResult {
        chunk_id: chunk.id.clone(),
        total,
        signals,
        kept,
        drop_reason,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    // Port the cases from openhuman's mod_tests.rs that exercise the
    // cheap-signals path. Drop tests that involve ExtractedEntities or LLM
    // band logic. Expect ~4-6 tests:
    // - score_chunk_admits_substantive_content
    // - score_chunk_drops_trivial_content
    // - score_chunk_includes_drop_reason
    // - score_chunk_uses_config_threshold
    // - ScoringConfig::default_uses_default_threshold
}
```

The implementer reads openhuman's `mod.rs` + `mod_tests.rs` to identify the cheap-path-only tests and ports them.

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score 2>&1 | tail -15`
Expected: all score-module tests pass (signals + store + mod).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/mod.rs
git commit -m "feat(memory_bucket_seal): score_chunk slim orchestrator (PR7.4 of 阶段 4)"
```

---

### Task 5: `embed/mod.rs` — Embedder trait + helpers

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/score/embed/mod.rs`

- [ ] **Step 1: Verify `async-trait` is in workspace deps**

Run: `grep -n "async-trait\|async_trait" src-tauri/Cargo.toml`

If present: continue.
If absent: add `async-trait = "0.1"` under `[dependencies]` in `src-tauri/Cargo.toml`. (PR1 likely already pulled it in for `MemoryAdapter` — verify with `grep -n "#\[async_trait" src-tauri/src/memory_adapter/traits.rs`.)

- [ ] **Step 2: Port `embed/mod.rs` from openhuman verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/embed/mod.rs` (235 LoC). Port full, with:
- The `pub mod cloud; pub mod factory; pub mod ollama;` declarations: **REMOVE**. Only `pub mod inert;` is ported in PR7.
- The `pub use cloud::CloudEmbedder; pub use factory::build_embedder_from_config; pub use ollama::OllamaEmbedder;` re-exports: **REMOVE**. Keep only `pub use inert::InertEmbedder;`.
- All 10 inline tests (cosine_*, pack_unpack_round_trip, unpack_wrong_byte_count_errors, unpack_wrong_dim_errors, pack_checked_rejects_wrong_dim, etc.): port verbatim. They don't touch network, deterministic.

The trait definition:

```rust
#[async_trait]
pub trait Embedder: Send + Sync {
    fn name(&self) -> &'static str;
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
}
```

Verbatim with openhuman. `EMBEDDING_DIM = 1024` constant verbatim.

- [ ] **Step 3: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail -15`
Expected: 10 passed (the openhuman tests).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/
git commit -m "feat(memory_bucket_seal): Embedder trait + cosine/pack helpers (PR7.5 of 阶段 4)"
```

---

### Task 6: `embed/inert.rs` — no-op embedder

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/score/embed/inert.rs`

- [ ] **Step 1: Port `inert.rs` verbatim**

Read `/Users/ryanliu/Documents/openhuman/src/openhuman/memory/tree/score/embed/inert.rs` (64 LoC). Port full:

```rust
//! Deterministic zero-vector embedder for tests.

use anyhow::Result;
use async_trait::async_trait;

use super::{Embedder, EMBEDDING_DIM};

#[derive(Clone, Copy, Debug, Default)]
pub struct InertEmbedder;

impl InertEmbedder {
    pub fn new() -> Self { Self }
}

#[async_trait]
impl Embedder for InertEmbedder {
    fn name(&self) -> &'static str { "inert" }
    async fn embed(&self, _text: &str) -> Result<Vec<f32>> {
        Ok(vec![0.0; EMBEDDING_DIM])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn returns_zero_vector_of_embedding_dim() {
        let v = InertEmbedder::new().embed("anything").await.unwrap();
        assert_eq!(v.len(), EMBEDDING_DIM);
        assert!(v.iter().all(|f| *f == 0.0));
    }

    #[tokio::test]
    async fn name_is_inert() {
        assert_eq!(InertEmbedder::new().name(), "inert");
    }

    #[tokio::test]
    async fn empty_input_still_returns_full_vector() {
        let v = InertEmbedder::new().embed("").await.unwrap();
        assert_eq!(v.len(), EMBEDDING_DIM);
    }
}
```

- [ ] **Step 2: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed::inert 2>&1 | tail -10`
Expected: 3 passed.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/inert.rs
git commit -m "feat(memory_bucket_seal): InertEmbedder no-op impl (PR7.6 of 阶段 4)"
```

---

### Task 7: Module wiring + re-exports

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs`
- Modify: `src-tauri/src/memory_bucket_seal/score/mod.rs`

- [ ] **Step 1: Verify `memory_bucket_seal/score/mod.rs` declarations**

Open `src-tauri/src/memory_bucket_seal/score/mod.rs`. It should have:

```rust
pub mod embed;
pub mod signals;
pub mod store;
```

at the top. If any are missing (Task 2-6 may have added them inconsistently), normalize the list.

- [ ] **Step 2: Add re-exports in `memory_bucket_seal/mod.rs`**

In `src-tauri/src/memory_bucket_seal/mod.rs`, add:

```rust
pub mod score;
```

Add to the `pub use` block:

```rust
pub use score::embed::{Embedder, InertEmbedder, EMBEDDING_DIM};
pub use score::{score_chunk, ScoreResult, ScoringConfig, DEFAULT_DROP_THRESHOLD};
```

(Adjust paths if Tasks 2-4 chose slightly different export names.)

- [ ] **Step 3: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: 63 (PR6 baseline) + ~30 new = ~93 passed.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs src-tauri/src/memory_bucket_seal/score/mod.rs
git commit -m "feat(memory_bucket_seal): wire score + embed re-exports (PR7.7 of 阶段 4)"
```

---

### Task 8: End-to-end integration test

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/mod.rs` (append to existing `#[cfg(test)]` block)

- [ ] **Step 1: Add e2e test exercising PR5+PR6+PR7**

Append to the `#[cfg(test)] mod tests` block at the bottom of `mod.rs`:

```rust
    #[test]
    fn end_to_end_chat_batch_to_score_admission() {
        use crate::memory_bucket_seal::canonicalize::chat::{canonicalise, ChatBatch, ChatMessage};
        use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
        use crate::memory_bucket_seal::score::store::{upsert_score, get_score, ScoreRow};
        use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
        use crate::memory_bucket_seal::store::BucketSealStore;
        use chrono::{TimeZone, Utc};
        use tempfile::TempDir;

        // 1. Build a chat batch
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        let batch = ChatBatch {
            platform: "slack".to_string(),
            channel_label: "eng".to_string(),
            messages: vec![
                ChatMessage {
                    author: "alice".to_string(),
                    timestamp: ts,
                    text: "Detailed technical message about the migration plan with sufficient signal density to clear the admission threshold.".to_string(),
                    source_ref: None,
                },
            ],
        };

        // 2. Canonicalise → chunk
        let canonical = canonicalise("slack:#eng", "alice", &[], batch).unwrap().unwrap();
        let chunker_input = ChunkerInput {
            source_kind: canonical.metadata.source_kind,
            source_id: canonical.metadata.source_id.clone(),
            markdown: canonical.markdown.clone(),
            metadata: canonical.metadata.clone(),
        };
        let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
        assert_eq!(chunks.len(), 1);

        // 3. Score
        let result = score_chunk(&chunks[0], &ScoringConfig::default());
        // A reasonable chat message should clear the threshold
        // (don't pin the exact value — signals are tuned per uClaw)
        assert_eq!(result.chunk_id, chunks[0].id);
        // entity_density is 0 in PR7 (no extract)
        assert_eq!(result.signals.entity_density, 0.0);
        // llm_importance is 0 in PR7 (no LLM)
        assert_eq!(result.signals.llm_importance, 0.0);

        // 4. Stage chunks + persist score
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();

        let staged = stage_chunks(dir.path(), &chunks).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();

        let row = ScoreRow {
            chunk_id: result.chunk_id.clone(),
            total: result.total,
            signals: result.signals.clone(),
            dropped: !result.kept,
            reason: result.drop_reason.clone(),
            computed_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        upsert_score(&store, &row).unwrap();

        // 5. Round-trip via get_score
        let got = get_score(&store, &result.chunk_id).unwrap().expect("score should round-trip");
        assert_eq!(got.chunk_id, result.chunk_id);
        assert!((got.total - result.total).abs() < 1e-6);
    }
```

Adaptation: `ScoreRow`'s field shape may differ from this skeleton (openhuman may flatten signals into named columns rather than a nested struct). Adjust the test to match the actual `ScoreRow` defined in Task 3.

- [ ] **Step 2: Run test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tests::end_to_end_chat 2>&1 | tail -15`
Expected: 2 passed (PR6's e2e + new PR7 e2e).

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "test(memory_bucket_seal): end-to-end chat → score admission (PR7.8 of 阶段 4)"
```

---

### Task 9: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~93+ passed.

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive (baseline + ~30 new from PR7).

- [ ] **Step 3: Clippy on PR7 files**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep "score\|embed" | head -20`
Expected: zero hits on `memory_bucket_seal/score/*`.

- [ ] **Step 4: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr7-score-and-embedder-trait && git diff main -- src-tauri/Cargo.toml`
Expected: either empty (`async-trait` already present from PR1) or a single 1-line addition for `async-trait`.

- [ ] **Step 5: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nrE "TODO|FIXME|XXX" src/memory_bucket_seal/score/`
Expected: zero hits (or only the `// SKIP: PR7 has no interaction recorder` markers from Task 2 — those are explicit and acceptable).

- [ ] **Step 6: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR7 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| `signals/types` derives + defaults | ~2 | `signals::types::tests` |
| `signals/token_count` boundary cases | ~3 | `signals::token_count::tests` |
| `signals/unique_words` boundary cases | ~3 | `signals::unique_words::tests` |
| `signals/metadata_weight` boundary cases | ~2 | `signals::metadata_weight::tests` |
| `signals/source_weight` per-kind cases | ~4 | `signals::source_weight::tests` |
| `signals/interaction` zero-recorder cases | ~2 | `signals::interaction::tests` |
| `signals/ops` combine + compute_cheap | ~4 | `signals::ops::tests` |
| `score::store` upsert/get/count + FK | ~5 | `score::store::tests` |
| `score::mod::score_chunk` admit/drop cases | ~4 | `score::tests` |
| `embed/mod` cosine + pack/unpack helpers | 10 | `embed::tests` |
| `embed/inert` zero-vector cases | 3 | `embed::inert::tests` |
| End-to-end chat → score → SQL | 1 | `mod::tests` |
| **Total new tests** | **~43** | — |
| **PR6 tests preserved** | 63 | (unchanged) |
| **Module total** | **~106** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B from brainstorming → signals + score::{mod, store} + Embedder trait + InertEmbedder. NO extract, NO Ollama, NO factory. Schema extends mem_tree_score table only (no mem_tree_chunks.embedding column yet — PR12 adds that).
- ✅ **Scope check**: NO `resolver.rs`, NO `extract/`, NO `embed/cloud`, NO `embed/factory`, NO `embed/ollama`. NO score::store entity_index methods. NO mem_tree_chunks ALTER. NO AppState wiring. NO IPC.
- ✅ **Type fidelity**: `ScoreSignals` + `SignalWeights` fields match openhuman bit-for-bit (8 + 7 fields respectively, including `llm_importance` which defaults to 0). `Embedder` trait signature matches verbatim. `EMBEDDING_DIM = 1024`.
- ✅ **No placeholders**: every step shows actual code patterns. Adaptation responsibilities enumerated.
- ✅ **FK discipline**: `mem_tree_score.chunk_id → mem_tree_chunks.id` FK is included. Tests insert chunks before scores.
- ✅ **Bisectability**: 9 task commits (schema / signals / score-store / score-mod / embed-trait / embed-inert / wiring / e2e / cleanup). Each compiles standalone.
- ✅ **No new deps** beyond a possible 1-line `async-trait` addition (which is likely already present from PR1).
- ✅ **`tracing::*` discipline**: no `log::*` slips.
- ✅ **Test fidelity**: openhuman test names preserved where possible; tests using `ExtractedEntities` either dropped or noted with `// SKIP: PR7 has no interaction recorder`.
- ✅ **No scope creep**: extract is permanently out of scope for PR7. PR9's BucketSealAdapter does not need it. PR8's tree_source does not need it (entity_density = 0 is fine for admission).
