# bucket_seal Config-Driven Embedding Dimension Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make bucket_seal's embedding dimension config-driven (from `EmbeddingEndpointConfig.dimensions`) via `Embedder::dim()`, fixing the `got 384, expected 1024` seal-job failures so the default bge-small/384 endpoint seals successfully.

**Architecture:** The `Embedder` becomes the single source of truth for dimension (`fn dim()`); both embedders carry a `dim` field set by `build_embedder` from `cfg.dimensions`. Adapter/seal/synthetic sites read `embedder.dim()`; the blob layer goes dim-lenient (recall skips mismatched-dim rows). `EMBEDDING_DIM=1024` is retained as a default for `InertEmbedder::new()`/tests/fallback.

**Tech Stack:** Rust, `memory_bucket_seal::score::embed` (Embedder trait + OpenAiCompat/Inert + factory), the seal/recall pipeline.

---

## Recon findings (complete — ground truth)

- `Embedder` trait (score/embed/mod.rs:54): `fn name(&self)`, `async fn embed(&self, text) -> Result<Vec<f32>>`. Add `fn dim(&self) -> usize`.
- `OpenAiCompatEmbedder` (openai_compat.rs): `struct { base_url, model, client }`; `new(base_url, model, timeout_secs)`; its `embed` calls **`parse_embedding_response(&text_body, EMBEDDING_DIM)` at openai_compat.rs:96** ← the 1024 guard. `parse_embedding_response(body, expected_dim)` is already dim-parameterised (tests call it with `3`).
- `InertEmbedder` (inert.rs): unit struct `pub struct InertEmbedder;`, `new() -> Self`, `embed` returns `vec![0.0; EMBEDDING_DIM]`. Used across many tests as `InertEmbedder::new()`.
- `build_embedder(cfg)` (factory.rs:14): builds `OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model, cfg.embed_timeout_secs)` when base_url+model set, else `InertEmbedder::new()`. `cfg.dimensions: u32` is available here, currently unused.
- Blob layer (mod.rs): `unpack`/decode hard-errors `if floats.len() != EMBEDDING_DIM` (~116); `pack_checked(v)` errors `if v.len() != EMBEDDING_DIM` (~129).
- `EMBEDDING_DIM: usize = 1024` (mod.rs:49) — keep as default; ~40 references are mostly tests.
- **Recon in T1:** `grep -rn "impl Embedder for" src-tauri/src/` — every impl needs `dim()` (likely only OpenAiCompat + Inert; check for test mocks). **Recon in T2:** the recall semantic leg in `adapter.rs` (where stored embeddings are decoded + cosine'd — add the `len == embedder.dim()` filter); the seal-summary verification (mod.rs:~444); the synthetic keyword-embeddings (adapter.rs:1306/1323/1365).
- Construction (app.rs:1080) unchanged (build_embedder already gets `memubot_config.embedding_endpoint`); remove the stale comment at app.rs:1062.

## Worktree setup

Worktree under `/Users/ryanliu/Documents/uclaw-worktrees/` on `claude/bucket-seal-config-embed-dim` off `origin/main`. Placeholders:
```bash
WT=/Users/ryanliu/Documents/uclaw-worktrees/bucket-seal-config-embed-dim
mkdir -p "$WT/src-tauri/bunembed" "$WT/src-tauri/pyembed" "$WT/src-tauri/gbrain-source"
touch "$WT/src-tauri/bunembed/bun" "$WT/src-tauri/pyembed/python"
echo x > "$WT/src-tauri/gbrain-source/placeholder.txt"
```
Baseline `cargo build` clean before Task 1.

## File structure

| File | Responsibility |
|---|---|
| `score/embed/mod.rs` | `Embedder::dim()`; lenient `unpack`/`pack_checked`; doc EMBEDDING_DIM as default |
| `score/embed/openai_compat.rs` | `dim` field + `new(..,dim)` + `dim()` + guard `self.dim` |
| `score/embed/inert.rs` | `dim` field + `with_dim` + `new()` default + `dim()` + `embed` uses `self.dim` |
| `score/embed/factory.rs` | pass `cfg.dimensions` into both embedders |
| `adapter.rs` | synthetic embeddings + seal verification → `self.embedder.dim()`; recall dim-filter |
| `mod.rs` (seal) | summary-embedding verification → `embedder.dim()` |
| `app.rs` | remove stale 1062 comment |

---

### Task 1: `Embedder::dim()` + config-driven embedders + factory

**Files:** `score/embed/mod.rs`, `openai_compat.rs`, `inert.rs`, `factory.rs` (+ any other `impl Embedder`).

- [ ] **Step 1: Add `dim()` to the trait**

In `mod.rs` `Embedder` trait, add:
```rust
    /// The fixed embedding dimension this embedder produces. Sourced from
    /// `EmbeddingEndpointConfig.dimensions` at construction; defaults to
    /// `EMBEDDING_DIM` for the no-arg `InertEmbedder`.
    fn dim(&self) -> usize;
```
Update the trait doc comment (lines ~52-61) to say the dimension is the embedder's `dim()` (runtime), with `EMBEDDING_DIM` as the default, rather than "MUST produce exactly EMBEDDING_DIM".

- [ ] **Step 2: `OpenAiCompatEmbedder` carries `dim`**

```rust
pub struct OpenAiCompatEmbedder { base_url: String, model: String, client: reqwest::Client, dim: usize }
// new(base_url, model, timeout_secs, dim: usize) { Self { …, dim } }
fn dim(&self) -> usize { self.dim }
```
At openai_compat.rs:96 change `parse_embedding_response(&text_body, EMBEDDING_DIM)` → `parse_embedding_response(&text_body, self.dim)`. (Keep `parse_embedding_response`'s `expected_dim` param + its tests calling it with `3` — unchanged.)

- [ ] **Step 3: `InertEmbedder` carries `dim`**

```rust
pub struct InertEmbedder { dim: usize }
impl InertEmbedder {
    pub fn new() -> Self { Self { dim: EMBEDDING_DIM } }
    pub fn with_dim(dim: usize) -> Self { Self { dim } }
}
// embed: Ok(vec![0.0; self.dim])
// dim(&self) -> usize { self.dim }
```
If `InertEmbedder` derives or is constructed as a bare unit struct `InertEmbedder` anywhere (`grep -rn "InertEmbedder" src-tauri/src/`), those become `InertEmbedder::new()` (the field makes bare construction invalid). Update such sites (likely none — most use `::new()`).

- [ ] **Step 4: factory passes `cfg.dimensions`**

In `factory.rs`:
```rust
let dim = if cfg.dimensions == 0 { EMBEDDING_DIM } else { cfg.dimensions as usize };
// real: OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model, cfg.embed_timeout_secs, dim)
// inert fallback: InertEmbedder::with_dim(dim)
```
(Import `EMBEDDING_DIM` in factory.rs if needed.)

- [ ] **Step 5: update any other `impl Embedder`**

`grep -rn "impl Embedder for" src-tauri/src/` — add `fn dim(&self)` to each (test mocks return a fixed dim). 

- [ ] **Step 6: tests + build**

Add to the embed tests:
```rust
#[tokio::test]
async fn inert_with_dim_returns_that_many_zeros() {
    let e = InertEmbedder::with_dim(384);
    assert_eq!(e.dim(), 384);
    assert_eq!(e.embed("x").await.unwrap().len(), 384);
    assert_eq!(InertEmbedder::new().dim(), EMBEDDING_DIM);
}
#[test]
fn parse_embedding_response_honors_dim() {
    let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
    assert!(parse_embedding_response(body, 3).is_ok());
    assert!(parse_embedding_response(body, 4).is_err());
}
```
Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail -15` → green. `cargo build 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 7: commit**

```bash
cd /Users/ryanliu/Documents/uclaw-worktrees/bucket-seal-config-embed-dim
git add src-tauri/src/memory_bucket_seal/score/embed/mod.rs src-tauri/src/memory_bucket_seal/score/embed/openai_compat.rs src-tauri/src/memory_bucket_seal/score/embed/inert.rs src-tauri/src/memory_bucket_seal/score/embed/factory.rs
git commit -m "feat(bucket_seal): Embedder::dim() — config-driven embedding dimension (default EMBEDDING_DIM) (embed-dim fix)"
```

---

### Task 2: use-site repoint — lenient blob + embedder.dim() + recall filter

**Files:** `score/embed/mod.rs` (unpack/pack_checked), `adapter.rs` (synthetic + verification + recall filter), seal `mod.rs` verification, `app.rs` (comment).

- [ ] **Step 1: lenient blob decode**

In `mod.rs` `unpack` (~116): REMOVE the `if floats.len() != EMBEDDING_DIM { bail! }` block. Keep any 4-byte-alignment validation above it (a non-multiple-of-4 blob is still an error). `pack_checked` (~129): make dim-agnostic — drop the `v.len() != EMBEDDING_DIM` check (the embedder already guarantees its output dim), OR rename usage so write-time packing just packs. (If `pack_checked` callers rely on the error, replace the check with a 4-byte-alignment/non-empty guard.) Confirm callers via `grep -rn "pack_checked\|unpack" src-tauri/src/memory_bucket_seal/`.

- [ ] **Step 2: synthetic embeddings + seal verification → `embedder.dim()`**

In `adapter.rs`, the synthetic keyword-hot vectors at 1306/1323/1365 (`vec![0.0f32; EMBEDDING_DIM]`) → `vec![0.0f32; self.embedder.dim()]`. The seal-summary verification (search `EMBEDDING_DIM` in adapter.rs + seal `mod.rs:~444` "embedding dimension must match") → compare against `embedder.dim()` (the seal path has the embedder; thread it / use `self.embedder.dim()`).

- [ ] **Step 3: recall dim-filter**

Recon the recall semantic leg (`recall_semantic` in adapter.rs — where stored embedding blobs are unpacked + cosine'd against the query embedding). After unpacking each candidate, **skip rows whose `len != self.embedder.dim()`** before cosine (with a `tracing::debug!(skipped, dim) ` count of stale-dim rows). The query embedding comes from `self.embedder` so its dim is current; mismatched stored rows are stale → skip.

- [ ] **Step 4: remove the stale comment**

`app.rs:1062` — delete the "EMBEDDING_DIM (1024); the default 384-dim endpoint will log a warn and fall back gracefully" comment (it no longer applies; the endpoint now seals correctly).

- [ ] **Step 5: tests**

```rust
// in adapter.rs tests (or wherever recall_semantic is tested):
// seed two stored summaries — one with embedding length == adapter.embedder.dim(), one with a different length —
// assert recall scores only the matching-dim one (the stale-dim row is skipped, no panic/err).
```
Add a decode test: a `384*4`-byte blob → 384 floats (lenient `unpack`); a 3-byte (misaligned) blob → error.

- [ ] **Step 6: build + test + clippy**

`cargo build 2>&1 | grep -E "^error" | head` → empty. `cargo test --lib memory_bucket_seal 2>&1 | grep "test result" | tail` → green. `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.

- [ ] **Step 7: commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/mod.rs src-tauri/src/memory_bucket_seal/adapter.rs src-tauri/src/memory_bucket_seal/mod.rs src-tauri/src/app.rs
git commit -m "feat(bucket_seal): dim-lenient blob decode + recall dim-filter + embedder.dim() at use sites (embed-dim fix)"
```

---

### Task 3: Whole-slice verification

- [ ] `cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] `cargo test --lib memory_bucket_seal 2>&1 | grep "test result" | tail` → green (incl. the new dim tests).
- [ ] `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] confirm no production hardcode remains: `grep -rn "EMBEDDING_DIM" src-tauri/src/memory_bucket_seal/ | grep -v test` shows only the const definition + the InertEmbedder default + factory fallback (no guard/decode/synthetic uses it directly anymore).
- [ ] `gitnexus_detect_changes()` before the PR.
- [ ] **Manual sanity (note in PR):** with the default config (bge-small/384) + the 384 endpoint up, seal jobs should no longer log `embedding dimension mismatch` (the user can confirm post-merge; not automatable here).

## Adjacent-edit checklist (PR body)

- `Embedder` trait gained `dim()` → all impls updated (OpenAiCompat, Inert, any mocks).
- `OpenAiCompatEmbedder::new` + `InertEmbedder` signatures changed → factory + all test constructors updated.
- No schema change, no data migration (lenient decode + recall filter handle any stale-dim rows; re-seal repopulates).
- `EMBEDDING_DIM` retained as the default constant.

## PR shape

One branch `claude/bucket-seal-config-embed-dim`, PR with a `## Commits (bisectable)` table (Tasks 1–2 = 2 commits). Title: `fix(bucket_seal): config-driven embedding dimension (resolve 384-vs-1024 seal mismatch)`. Body: root cause (default bge-small/384 vs hardcoded 1024); `Embedder::dim()` from `cfg.dimensions`; lenient decode + recall dim-filter; no schema/migration; swapping models later just works via config.

## Self-review notes

- **Spec coverage:** §1 dim() seam → Task 1; §2 use sites + lenient decode + recall filter → Task 2; testing → Tasks 1/2/3; stale-data handling → Task 2 recall filter. ✔
- **Type consistency:** `dim()` on the trait + both impls + factory `cfg.dimensions as usize`; `parse_embedding_response(_, self.dim)`; synthetic `vec![0.0; self.embedder.dim()]`. ✔
- **Bisectability:** Task 1 (trait+embedders+factory; blob still uses EMBEDDING_DIM = unchanged) compiles; Task 2 (lenient decode + use sites) compiles. ✔
- **Follow-the-recon items** (flagged): other `impl Embedder` (Task 1 Step 5); `pack_checked`/`unpack` callers (Task 2 Step 1); the exact `recall_semantic` decode+cosine site for the filter (Task 2 Step 3); the seal verification site (Task 2 Step 2); bare `InertEmbedder` constructions (Task 1 Step 3). Each has a grep + concrete guidance.
