# Step 3a — In-Process ONNX Embedder (replace memU FastEmbed @ 7337) Design

**Date:** 2026-06-01
**Status:** Design (approved in brainstorming; pending spec review)
**Part of:** Memory two-layer finish-line (ADR `2026-06-01-memory-two-layer-terminal-state.md`), **Step 3** (remove memU), sub-slice **Step 3a** (the embedding provider). Step 3b (remove the memU *store* — episodic/recall/reflection → bucket_seal, then bridge teardown) is a separate later slice.

## Problem

bucket_seal embeds via `OpenAiCompatEmbedder` → an OpenAI-compatible endpoint at `localhost:7337/v1` → uClaw's `local_api` server → memU's **Python FastEmbed** bridge (model `BAAI/bge-small-en-v1.5`, 384-dim). This couples the memory hot-path (every seal-summary + recall query embed) to a Python subprocess + a TCP port, and surfaced the `:7337` port-collision fragility (a stale instance holding the port). The two-layer terminal state wants **zero external runtimes**. `ort`/ONNX Runtime is already statically linked, and STT already loads ONNX models via it — so embedding can run **in-process**, removing the Python dependency from the embedding hot-path and unblocking the full memU-store removal (3b).

## Decision (Step 3a scope)

Add an in-process `OnnxEmbedder` (hand-rolled `ort` + `tokenizers`, mirroring the STT engine's load/run pattern) that produces `bge-small-en-v1.5` 384-dim embeddings, and make `build_embedder` return it by default (when the config points at the memU default endpoint). The embedding **dimension stays config-driven** (the `Embedder::dim()` fix). `OpenAiCompatEmbedder` is retained for users who configure a *real* remote endpoint; `InertEmbedder` stays the test/no-op fallback. The memU *store* (episodic/recall/reflection) and its bridge boot are **out of scope** (Step 3b) — but bucket_seal's embedding path stops hitting Python/7337.

Out of scope: removing the memU bridge boot, `memu.db`, the `local_api` 7337 server, and the memU vector-recall/reflection legs (all Step 3b).

## Design

### §1 `OnnxEmbedder` (new `src-tauri/src/memory_bucket_seal/score/embed/onnx.rs`)

Mirrors `stt/openflow/engine.rs` (ort `Session::run` is blocking + `&mut` → `tokio::task::spawn_blocking` + a `Mutex<Session>`):

```rust
pub struct OnnxEmbedder {
    tokenizer: tokenizers::Tokenizer,
    session: tokio::sync::Mutex<ort::session::Session>,
    dim: usize,
    model_dir: std::path::PathBuf,
}
```

- **Lazy load-on-first-use:** the embedder is cheap to construct (records `model_dir` + `dim`); the model + tokenizer load lazily on first `embed` (download if absent — §2 — then build the `Session` once, cache it). No binary bloat; no zero-vector fallback baked into seals; the durable seal-job queue tolerates the first slow embed.
- **`embed(text)`** (in `spawn_blocking`): `tokenizer.encode(text, true)` → `input_ids` / `attention_mask` / `token_type_ids` (i64 tensors, batch=1) → `session.run(inputs!{...})` → `last_hidden_state` `[1, seq, 384]` → **pool + L2-normalize** → `Vec<f32>` of length `dim`.
- **`name()`** = `"onnx-bge-small"`; **`dim()`** = the configured dim (384 default).

**Critical compatibility detail.** The 460 existing embeddings (70 pages + 248 skills + 142 tool_stats) were produced by memU's FastEmbed for `bge-small-en-v1.5`. For old and new vectors to share one space (so recall on mixed data doesn't degrade), `OnnxEmbedder` **must match FastEmbed's pooling for bge** — FastEmbed uses the **`[CLS]` token (first token) of `last_hidden_state` + L2-normalize** for bge models (NOT mean-pooling). The plan confirms FastEmbed's exact bge pooling + ONNX input names against the model card and matches them. (Implementation note: bge-small uses standard BERT inputs `input_ids`/`attention_mask`/`token_type_ids`; output `last_hidden_state`; pooling = `[:, 0, :]` then normalize.)

### §2 Model downloader (mirror `stt/openflow/downloader.rs`)

Download `bge-small-en-v1.5` artifacts on first use to `~/.uclaw/models/bge-small-en-v1.5/`:
- `model.onnx` (the ONNX export — from `BAAI/bge-small-en-v1.5` `onnx/model.onnx`, ~130 MB) + `tokenizer.json` (+ any required `config.json`/`special_tokens_map.json`).
- Primary `https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main/...`; **fallback `https://hf-mirror.com/...`** (same dual-source + progress-callback shape as STT's downloader). Idempotent (skip present files via a size/hash check, as STT does).

### §3 `build_embedder` routing (`score/embed/factory.rs`)

```
build_embedder(cfg):
  dim = if cfg.dimensions == 0 { EMBEDDING_DIM } else { cfg.dimensions as usize }
  if cfg.base_url is empty           → InertEmbedder::with_dim(dim)           // unchanged no-op
  else if cfg.base_url is the memU default (localhost:7337)  → OnnxEmbedder (in-process, model_dir under ~/.uclaw/models, dim)   // NEW default
  else                               → OpenAiCompatEmbedder(base_url, model, timeout, dim)   // explicit remote endpoint retained
```

(The "is the memU default" check keys off the default `base_url`/`model` from `EmbeddingEndpointConfig::default()` — `localhost:7337` + `llama-server:bge-small-en-v1.5`. A user who sets a different `base_url` keeps the remote OpenAI-compatible path. Net: default installs embed in-process; power users can still point at a remote endpoint.)

### Data flow

```
seal/recall embed → build_embedder(cfg) → OnnxEmbedder (default)
  first embed: ensure model_dir (download bge-small onnx + tokenizer if absent) → load Session (cached)
  per embed (spawn_blocking): tokenize → ort run → CLS pool → L2-normalize → 384-dim
  (no Python, no 7337, in-process)
remote-endpoint users (non-default base_url) → OpenAiCompatEmbedder (unchanged)
```

## Error handling

First-embed download failure (offline / HF + mirror both down) → `embed` returns `Err` → the seal job fails + **retries** via the durable job queue (same posture as today's transient embed failures) — no zero vectors persisted. Session-load failure → `Err` + retry. A dimension mismatch from the model (shouldn't happen — fixed model) → the existing `Embedder::dim()` guard. `InertEmbedder` covers the no-endpoint config.

## Testing

1. **Pooling math** (unit, no network): feed a small fixture `last_hidden_state` array → assert CLS-pool + L2-normalize produces the expected unit vector; assert `dim()==384`.
2. **Pooling parity** (the compatibility guarantee): for one fixed string, assert the `OnnxEmbedder` vector is within a tight cosine threshold (≥0.999) of a **stored reference vector captured from memU's FastEmbed** for the same string (checked into the test as a fixture). If this fails → the embedder isn't space-compatible → trigger the re-embed contingency (below).
3. **Factory routing:** default config → `OnnxEmbedder`; non-default `base_url` → `OpenAiCompatEmbedder`; empty → `InertEmbedder`; all honor `cfg.dimensions`.
4. **Live integration** (gated / ignored by default — needs the model download): construct `OnnxEmbedder`, embed two strings, assert 384-dim + that semantically-related strings cosine higher than unrelated ones.
5. `cargo build` + clippy clean; `cargo test --lib memory_bucket_seal::score::embed` green.

## Scope / files

| File | Change |
|---|---|
| `score/embed/onnx.rs` (new) | `OnnxEmbedder` + lazy load + tokenize/infer/CLS-pool/normalize + tests |
| `score/embed/model_download.rs` (new, or extend STT's) | bge-small ONNX + tokenizer download (HF + mirror) |
| `score/embed/factory.rs` | route default → `OnnxEmbedder`; retain OpenAiCompat (remote) + Inert |
| `score/embed/mod.rs` | `pub mod onnx;` (+ download mod) |
| `src-tauri/Cargo.toml` | add `tokenizers` (+ `hf-hub`/reqwest reuse for download if needed) |

**Out of scope (Step 3b):** memU bridge boot teardown, `memu.db`, `local_api` 7337 server, the memU vector-recall leg in `memory_graph/recall.rs`, the reflection memU mapping, `restart_memu_bridge`, memU diagnostics — all the *store* side. After 3b, Python is fully gone.

## Risk

Medium. Two real risks, both mitigated:
1. **Vector-space compatibility** with the 460 existing memU-FastEmbed vectors — addressed by matching FastEmbed's CLS pooling + a parity test (≥0.999 cosine vs a captured reference). **Contingency if parity fails:** a one-time background re-embed of the existing `pages`/`skills`/`tool_stats`/summary embeddings with the new embedder (a marker-gated boot pass like the P2b/P3 migrations) — recorded here so it's not a surprise; only run if the parity test shows divergence.
2. **New `tokenizers` dep + ONNX inference correctness** — mirrors the proven STT `ort` pattern; pooling unit-tested; the model is a fixed, well-known export.

Lazy first-embed download = one-time latency (tolerated by the durable seal queue). This slice removes Python from the embedding hot-path + the 7337 embedding coupling; the memU *store* removal (3b) completes the "zero external runtimes" goal. Bisectable: embedder + downloader → factory routing.
