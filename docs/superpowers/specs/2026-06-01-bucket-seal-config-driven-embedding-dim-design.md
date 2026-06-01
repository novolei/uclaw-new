# bucket_seal — Config-Driven Embedding Dimension Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Type:** Independent storage-layer bugfix (not part of the memory-store convergence ADR, though surfaced by it).

## Problem

Seal jobs fail with `embedding dimension mismatch: got 384, expected 1024 (dimension)`. Root cause: a hard contradiction between two parts of the codebase.

- `EmbeddingEndpointConfig::default()` (memubot_config.rs:935): `base_url = "http://localhost:7337/v1"`, `model = "llama-server:bge-small-en-v1.5"`, `dimensions = 384`, `fastembed_model = "BAAI/bge-small-en-v1.5"` → a real, reachable **384-dim** endpoint.
- `memory_bucket_seal` hard-codes `EMBEDDING_DIM = 1024` (score/embed/mod.rs:49, designed for `bge-m3`). `build_embedder` always builds the real `OpenAiCompatEmbedder` (base_url+model are set by default); its per-embed guard validates the returned vector against `EMBEDDING_DIM` (1024) — explicitly **NOT** `cfg.dimensions` (openai_compat.rs:4-7 comment: a known, punted mismatch).

Every seal-time `embed summary` therefore returns 384 and is rejected by the 1024 guard. The P2/P3 convergence work pushed real content into bucket_seal, triggering topic-tree cascade seals that embed summaries, surfacing this latent contradiction at scale.

## Decision

Make the embedding dimension **config-driven**, sourced from `EmbeddingEndpointConfig.dimensions`, via the **Embedder** as the single source of truth (`fn dim(&self)`). The store/adapter/score/verification consult the embedder's dim instead of the hard-coded const; the blob layer becomes dim-lenient on read (skip mismatched-dim vectors rather than hard-erroring) so a dim change degrades gracefully and re-seal repopulates. `EMBEDDING_DIM` is retained as a **default** (`DEFAULT_EMBEDDING_DIM = 1024`) for the no-arg `InertEmbedder` + tests + the factory fallback. No store schema change, no explicit reset migration (seals have been failing → ≈zero stored embeddings; lenient read covers any stragglers).

## Design

### §1 `dim()` on the Embedder (source of truth)

`score/embed/mod.rs`:
- Add to the `Embedder` trait: `fn dim(&self) -> usize;`.
- `pub const EMBEDDING_DIM: usize = 1024;` → keep, document as the **default** (rename references conceptually; the constant name may stay `EMBEDDING_DIM` to avoid churning the 40 test sites, with an updated doc-comment "default dim; runtime dim comes from the Embedder").

`score/embed/openai_compat.rs`:
- `struct OpenAiCompatEmbedder { …, dim: usize }`; `new(base_url, model, timeout_secs, dim)`; `fn dim(&self) -> usize { self.dim }`; the guard calls `parse_embedding_response(body, self.dim)` (already parameterised by `expected_dim` — just pass `self.dim`).

`score/embed/inert.rs`:
- `struct InertEmbedder { dim: usize }`; `InertEmbedder::new()` defaults `dim = EMBEDDING_DIM` (test/back-compat); add `InertEmbedder::with_dim(dim)`; `embed` returns `vec![0.0; self.dim]`; `fn dim(&self) -> usize { self.dim }`.

`score/embed/factory.rs`:
- `build_embedder(cfg)`: pass `cfg.dimensions as usize` into `OpenAiCompatEmbedder::new(...)`; the Inert fallback uses `InertEmbedder::with_dim(cfg.dimensions as usize)` (falling back to `EMBEDDING_DIM` when `cfg.dimensions == 0`).

### §2 Use sites read the runtime dim

- **Adapter** (holds `embedder: Arc<dyn Embedder>`): the synthetic keyword-embeddings (adapter.rs:1306/1323/1365 `vec![0.0; EMBEDDING_DIM]`) → `vec![0.0; self.embedder.dim()]`; the seal-summary verification (mod.rs:444 "embedding dimension must match EMBEDDING_DIM") → compare against `embedder.dim()` (thread the embedder/dim to that check — it's on the seal path which has the embedder).
- **Blob layer dim-lenient on read:** `unpack`/decode (mod.rs:116/130) currently hard-errors `if floats.len() != EMBEDDING_DIM`. Change to: decode the f32 blob, validate only 4-byte alignment (a corrupt/truncated blob is still an error); do **not** reject on a specific dim. **Recall** (the semantic leg) filters candidate vectors to `len == embedder.dim()` before cosine (a mismatched-dim row → skip, with a `tracing::debug!` count). Re-seal repopulates at the current dim.
- `app.rs:1062`: remove the stale "EMBEDDING_DIM (1024); the default 384-dim endpoint will log a warn" comment (no longer true).

### Data flow

```
boot: build_embedder(cfg) → OpenAiCompatEmbedder{ dim: cfg.dimensions=384 }
seal: embed summary → endpoint returns 384 → guard expected_dim=self.dim(384) → OK → pack → store
recall: decode blobs (lenient) → keep rows where len == embedder.dim() → cosine
dim change later (e.g. cfg→1024): old 384 rows skipped on recall (logged); re-seal writes 1024; no crash
```

## Error handling

The per-embed guard still hard-fails on a *wrong* dim (got 385 when dim=384) — a genuine endpoint bug stays loud. A *stale* stored row (len != current dim) is skipped on recall (debug-logged count), not fatal. A truncated/misaligned blob remains a decode error. The factory's Inert fallback covers an unconfigured endpoint.

## Testing

1. `OpenAiCompatEmbedder` with `dim=384`: `parse_embedding_response` accepts a 384-vec, rejects 385 / 1024.
2. `InertEmbedder::with_dim(384).embed(..)` → 384 zeros; `dim()==384`; `InertEmbedder::new()` → 1024 (default).
3. `build_embedder` maps `cfg.dimensions` → `embedder.dim()` (384 default config; 1024 when cfg says 1024; fallback when 0).
4. Recall skips a stored row whose `len != embedder.dim()` (seed two rows of different dims; only the matching one is scored).
5. Decode: a 384*4-byte blob decodes to 384 floats; a misaligned blob errors.
6. Existing const-based tests (Inert default 1024) still pass; `cargo build` + `cargo test --lib memory_bucket_seal` + clippy clean.

## Scope / files

| File | Change |
|---|---|
| `score/embed/mod.rs` | `Embedder::dim()`; `EMBEDDING_DIM` doc → "default"; lenient `unpack` (alignment-only) |
| `score/embed/openai_compat.rs` | `dim` field + `new(.., dim)` + `dim()` + guard uses `self.dim` |
| `score/embed/inert.rs` | `dim` field + `with_dim` + `new()` default + `dim()` + `embed` uses `self.dim` |
| `score/embed/factory.rs` | pass `cfg.dimensions` into both embedders (fallback to default when 0) |
| `adapter.rs` | synthetic embeddings + seal-summary verification → `self.embedder.dim()` |
| `mod.rs` / seal pipeline | summary-embedding verification → `embedder.dim()`; recall dim-filter |
| `app.rs` | remove stale 1062 comment |

**Out of scope:** changing the default model (this fix makes *whatever* `cfg.dimensions` says work — the default stays bge-small/384, which now functions); a store-recorded-dim meta table with auto-reset (the A2 alternative — not needed given lenient read); re-embedding historical content beyond what re-seal naturally does.

## Risk

Medium. Touches the embedding/score/blob layer, but the seam (`embedder.dim()`) is clean and the change is largely const→method. No schema change, no data migration (lenient read + likely-empty embedding store). The one behavioral subtlety is the lenient decode + recall dim-filter (covered by tests). After this, the default bge-small/384 endpoint seals successfully, and swapping to any other-dim model (e.g. bge-m3/1024) just works via `cfg.dimensions` without further code changes. Bisectable: trait+embedders+factory → adapter/verification sites → lenient decode + recall filter.
