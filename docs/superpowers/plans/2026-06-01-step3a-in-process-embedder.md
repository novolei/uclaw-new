# Step 3a — In-Process ONNX Embedder Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace bucket_seal's embedding provider (memU FastEmbed @ `localhost:7337`) with an in-process `OnnxEmbedder` (`ort` + `tokenizers`, bge-small-en-v1.5, 384-dim), removing Python from the embedding hot-path. Spec: `docs/superpowers/specs/2026-06-01-step3a-in-process-embedder-design.md`.

**Architecture:** A hand-rolled `ort` embedder mirroring `stt/openflow/onnx_inference.rs`: tokenize → ONNX run → **CLS-token pool + L2-normalize** (matching FastEmbed's bge pooling for vector-space compatibility with the 460 existing embeddings). Model downloaded on first use to `~/.uclaw/models/` (HF + mirror, mirroring the STT downloader). `build_embedder` returns it by default; `OpenAiCompatEmbedder` retained for remote endpoints.

**Tech Stack:** Rust, `ort = "=2.0.0-rc.10"` (already linked), `tokenizers` (new dep), `ndarray`, the `memory_bucket_seal::score::embed` module.

---

## Recon findings (complete — ground truth)

- **FastEmbed bge pooling = CLS + normalize** (`pyembed/.../fastembed/text/onnx_embedding.py:311-322`: `processed_embeddings = embeddings[:, 0]` then `normalize(...)`). bge-small-en-v1.5 ONNX outputs `last_hidden_state` `[batch, seq, 384]`; FastEmbed takes token 0 (CLS) + L2-normalizes. **Match exactly** for compatibility.
- **ort 2.0 pattern** (`stt/openflow/onnx_inference.rs`): `use ort::{session::{builder::GraphOptimizationLevel, Session}, value::{Tensor, TensorRef, Value}};`. Load: `Session::builder()?.with_optimization_level(...)?.commit_from_file(path)?`. Inputs: `Tensor::from_array((shape_tuple, vec))?`. Run: `session.run(ort::inputs!{ "name" => tensor, ... })?`. Extract: `outputs["last_hidden_state"].try_extract_array::<f32>()?` → `ndarray::ArrayD<f32>` → `.into_dimensionality::<ndarray::Ix3>()?`. `Session::run` needs `&mut` + is blocking → wrap in `tokio::task::spawn_blocking` + a `Mutex<Session>` (STT uses `blocking_lock()` inside spawn_blocking).
- **STT downloader** (`stt/openflow/downloader.rs`): dual-source (HF primary `https://huggingface.co/<repo>/resolve/main`, fallback `https://hf-mirror.com`), per-file list, progress callback, idempotent skip. Mirror its shape.
- **Embedder trait** (`score/embed/mod.rs:56`): `fn name(&self)->&'static str; fn dim(&self)->usize; async fn embed(&self, text:&str)->anyhow::Result<Vec<f32>>`. `EMBEDDING_DIM=1024` default const. `build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder>` (factory.rs:17).
- **`EmbeddingEndpointConfig::default()`**: `base_url="http://localhost:7337/v1"`, `model="llama-server:bge-small-en-v1.5"`, `dimensions=384` (memubot_config.rs:935). The factory routing keys off this default base_url.
- **ONNX input names:** bge-small is standard BERT → `input_ids`, `attention_mask`, `token_type_ids` (all i64). FastEmbed guards `if "attention_mask" in input_names` (onnx_text_model.py:88) + only sends `token_type_ids` if present — the embedder must inspect `session.inputs` and only feed names the model declares. Output name: `last_hidden_state` (confirm via the session's output metadata; some exports use `output_0` — read `session.outputs[0].name`).
- `tokenizers` is NOT a dep yet; `ndarray` + `half` are (via ort features). No bge ONNX present locally (download needed).

## Worktree

`/Users/ryanliu/Documents/uclaw-worktrees/step3a-in-process-embedder` on `claude/step3a-in-process-embedder` off `origin/main`. Placeholders: `mkdir -p src-tauri/{bunembed,pyembed,gbrain-source}; touch src-tauri/bunembed/bun src-tauri/pyembed/python; echo x > src-tauri/gbrain-source/placeholder.txt`. Baseline `cargo build` clean before Task 1.

## File structure / tasks

| Task | Files |
|---|---|
| 1 | `Cargo.toml` (+tokenizers); `score/embed/model_download.rs` (new) + `mod.rs` |
| 2 | `score/embed/onnx.rs` (new) — `OnnxEmbedder` + pooling + tests; `mod.rs` |
| 3 | `score/embed/factory.rs` — routing; tests |
| 4 | verify (+ gated parity / re-embed contingency note) |

---

### Task 1: add `tokenizers` + the model downloader

**Files:** `src-tauri/Cargo.toml`; create `src-tauri/src/memory_bucket_seal/score/embed/model_download.rs`; `score/embed/mod.rs` (`pub mod model_download;`).

- [ ] **Step 1: add the dep**

In `src-tauri/Cargo.toml` `[dependencies]`:
```toml
tokenizers = { version = "0.20", default-features = false, features = ["onig"] }
```
(Pick the version that resolves cleanly with the existing tree; `onig` covers BERT tokenization. If `onig` pulls C deps that conflict, use `default-features = false` + the `unstable_wasm`/`esaxx_fast`-free minimal set — the plan's implementer picks the minimal feature set that compiles. Run `cargo build` to confirm it resolves before proceeding.)

- [ ] **Step 2: downloader module**

`model_download.rs` — mirror `stt/openflow/downloader.rs` (read it first for the exact reqwest/progress/fallback shape):
```rust
//! Downloads bge-small-en-v1.5 ONNX + tokenizer into ~/.uclaw/models/bge-small-en-v1.5/
//! (HF primary, hf-mirror fallback). Idempotent: skips files already present.

use std::path::{Path, PathBuf};

const HF_BASE: &str = "https://huggingface.co/BAAI/bge-small-en-v1.5/resolve/main";
const MIRROR_BASE: &str = "https://hf-mirror.com/BAAI/bge-small-en-v1.5/resolve/main";
/// (remote_relpath, local_filename)
const FILES: &[(&str, &str)] = &[
    ("onnx/model.onnx", "model.onnx"),
    ("tokenizer.json", "tokenizer.json"),
    ("config.json", "config.json"),
    ("tokenizer_config.json", "tokenizer_config.json"),
    ("special_tokens_map.json", "special_tokens_map.json"),
];

pub fn model_dir(data_dir: &Path) -> PathBuf { data_dir.join("models").join("bge-small-en-v1.5") }

/// True if model.onnx + tokenizer.json already exist (the minimum to load).
pub fn is_present(dir: &Path) -> bool {
    dir.join("model.onnx").exists() && dir.join("tokenizer.json").exists()
}

/// Download any missing files (HF then mirror). Best-effort per-file; errors propagate
/// so the caller (lazy embed) can surface "model unavailable" and retry.
pub async fn ensure_model(dir: &Path) -> anyhow::Result<()> {
    if is_present(dir) { return Ok(()); }
    std::fs::create_dir_all(dir)?;
    for (rel, local) in FILES {
        let dest = dir.join(local);
        if dest.exists() { continue; }
        let bytes = fetch_with_fallback(rel).await?;   // reqwest GET HF then MIRROR
        std::fs::write(&dest, &bytes)?;
    }
    Ok(())
}
```
(Fill `fetch_with_fallback` per STT's downloader idiom — reqwest GET `{HF_BASE}/{rel}`, on error retry `{MIRROR_BASE}/{rel}`; stream to bytes. `config.json`/`tokenizer_config.json`/`special_tokens_map.json` are best-effort — `tokenizer.json` is self-contained for `Tokenizer::from_file`, so if those 404 it's fine; only `model.onnx` + `tokenizer.json` are required by `is_present`.)

- [ ] **Step 3: tests** (no network — path/URL construction only):
```rust
#[test]
fn model_dir_under_data() {
    let d = model_dir(std::path::Path::new("/tmp/uclaw"));
    assert!(d.ends_with("models/bge-small-en-v1.5"));
}
#[test]
fn is_present_requires_onnx_and_tokenizer() {
    let t = tempfile::tempdir().unwrap();
    assert!(!is_present(t.path()));
    std::fs::write(t.path().join("model.onnx"), b"x").unwrap();
    assert!(!is_present(t.path()));
    std::fs::write(t.path().join("tokenizer.json"), b"x").unwrap();
    assert!(is_present(t.path()));
}
```
- [ ] **Step 4:** `cargo build 2>&1 | grep -E "^error" | head` → empty (tokenizers resolves); `cargo test --lib model_download 2>&1 | tail` → green.
- [ ] **Step 5: commit** — `git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/memory_bucket_seal/score/embed/model_download.rs src-tauri/src/memory_bucket_seal/score/embed/mod.rs && git commit -m "feat(embed): tokenizers dep + bge-small model downloader (Step 3a)"`.

---

### Task 2: `OnnxEmbedder`

**Files:** create `src-tauri/src/memory_bucket_seal/score/embed/onnx.rs`; `mod.rs` (`pub mod onnx;`).

- [ ] **Step 1: the embedder** (mirror `onnx_inference.rs`'s ort usage):
```rust
//! In-process ONNX text embedder (bge-small-en-v1.5). CLS-token pooling + L2-normalize
//! to match FastEmbed (so vectors stay compatible with existing memU-produced embeddings).

use std::path::PathBuf;
use std::sync::Arc;
use anyhow::{anyhow, Result};
use async_trait::async_trait;
use ndarray::Axis;
use ort::{session::{builder::GraphOptimizationLevel, Session}, value::Tensor};
use tokenizers::Tokenizer;
use super::{Embedder, model_download};

pub struct OnnxEmbedder {
    dim: usize,
    model_dir: PathBuf,
    // lazily initialised on first embed
    inner: tokio::sync::Mutex<Option<Loaded>>,
}
struct Loaded { tokenizer: Tokenizer, session: Session }

impl OnnxEmbedder {
    pub fn new(model_dir: PathBuf, dim: usize) -> Self {
        Self { dim, model_dir, inner: tokio::sync::Mutex::new(None) }
    }
}

/// CLS pooling + L2 normalize over last_hidden_state [seq, hidden]. PURE — unit-testable.
fn cls_pool_normalize(last_hidden_state: &ndarray::ArrayView2<f32>) -> Vec<f32> {
    let cls = last_hidden_state.index_axis(Axis(0), 0); // token 0 = [CLS]
    let norm = cls.dot(&cls).sqrt();
    let inv = if norm > 0.0 { 1.0 / norm } else { 0.0 };
    cls.iter().map(|x| x * inv).collect()
}

#[async_trait]
impl Embedder for OnnxEmbedder {
    fn name(&self) -> &'static str { "onnx-bge-small" }
    fn dim(&self) -> usize { self.dim }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        // ensure model present (download once) + Session loaded (cached)
        {
            let mut guard = self.inner.lock().await;
            if guard.is_none() {
                model_download::ensure_model(&self.model_dir).await?;
                let dir = self.model_dir.clone();
                let loaded = tokio::task::spawn_blocking(move || -> Result<Loaded> {
                    let tokenizer = Tokenizer::from_file(dir.join("tokenizer.json")).map_err(|e| anyhow!("tokenizer: {e}"))?;
                    let session = Session::builder()?
                        .with_optimization_level(GraphOptimizationLevel::Level3)?
                        .commit_from_file(dir.join("model.onnx"))?;
                    Ok(Loaded { tokenizer, session })
                }).await??;
                *guard = Some(loaded);
            }
        }
        let text = text.to_string();
        let this_dim = self.dim;
        // run inference under the lock in a blocking thread (Session::run needs &mut)
        let inner = self.inner.clone(); // Mutex isn't Clone — instead reacquire below
        // NOTE: hold the lock across the blocking run via blocking_lock inside spawn_blocking:
        let guard_arc = self as *const _; // placeholder — see Step note: implement with an Arc<OnnxEmbedder> pattern like STT (self: &Arc<Self>)
        unimplemented!()
    }
}
```
> **Implementer note (resolve the borrow shape):** mirror STT exactly — make `embed` take `self: &Arc<Self>` is NOT possible on a trait method, so instead store `inner: Arc<tokio::sync::Mutex<Option<Loaded>>>`, `clone()` the `Arc` into the `spawn_blocking`, and use `inner.blocking_lock()` there (STT's pattern). Inside the blocking closure: tokenize → build `input_ids`/`attention_mask`/`token_type_ids` as `Tensor::from_array(([1, seq], i64_vec))` (only include `token_type_ids` if `session.inputs` declares it) → `session.run(ort::inputs!{ ... })?` → read the output (look up the `last_hidden_state` output name from `session.outputs`) → `try_extract_array::<f32>()?` → `into_dimensionality::<ndarray::Ix3>()?` → take `[0, .., ..]` view (`[seq, hidden]`) → `cls_pool_normalize(&view)` → assert `.len() == this_dim` → return. Replace the placeholder block above with this; the `cls_pool_normalize` fn + struct fields are the fixed contract.

- [ ] **Step 2: unit test the pooling (no model/network):**
```rust
#[test]
fn cls_pool_takes_token0_and_normalizes() {
    // last_hidden_state [seq=2, hidden=3]; token0 = [3,4,0] → norm 5 → [0.6,0.8,0]
    let a = ndarray::array![[3.0_f32, 4.0, 0.0], [9.0, 9.0, 9.0]];
    let v = cls_pool_normalize(&a.view());
    assert_eq!(v.len(), 3);
    assert!((v[0]-0.6).abs() < 1e-6 && (v[1]-0.8).abs() < 1e-6 && v[2].abs() < 1e-6);
}
```
- [ ] **Step 3: live embed test (gated `#[ignore]` — needs the model download):**
```rust
#[tokio::test]
#[ignore = "downloads bge-small (~130MB); run manually"]
async fn embed_real_text_384_and_self_cosine_1() {
    let dir = tempfile::tempdir().unwrap();
    let e = OnnxEmbedder::new(dir.path().to_path_buf(), 384);
    let v = e.embed("hello world").await.unwrap();
    assert_eq!(v.len(), 384);
    let dot: f32 = v.iter().map(|x| x*x).sum();
    assert!((dot - 1.0).abs() < 1e-3, "should be unit-normalized");
}
```
- [ ] **Step 4:** `cargo build 2>&1 | grep -E "^error" | head` empty; `cargo test --lib memory_bucket_seal::score::embed::onnx 2>&1 | tail` (the pooling test runs; the `#[ignore]` live test is skipped).
- [ ] **Step 5: commit** — `git add src-tauri/src/memory_bucket_seal/score/embed/onnx.rs src-tauri/src/memory_bucket_seal/score/embed/mod.rs && git commit -m "feat(embed): in-process OnnxEmbedder (bge-small, CLS-pool, lazy load) (Step 3a)"`.

---

### Task 3: factory routing

**Files:** `src-tauri/src/memory_bucket_seal/score/embed/factory.rs`.

- [ ] **Step 1:** route default → `OnnxEmbedder`:
```rust
pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder> {
    let dim = if cfg.dimensions == 0 { EMBEDDING_DIM } else { cfg.dimensions as usize };
    if cfg.base_url.trim().is_empty() {
        return Arc::new(InertEmbedder::with_dim(dim));
    }
    // The memU default endpoint → embed in-process (no Python / no 7337).
    if cfg.base_url.contains("localhost:7337") || cfg.base_url.contains("127.0.0.1:7337") {
        let data_dir = crate::uclaw_utils_home::data_dir(); // confirm the canonical data-dir helper used elsewhere
        let model_dir = crate::memory_bucket_seal::score::embed::model_download::model_dir(&data_dir);
        return Arc::new(OnnxEmbedder::new(model_dir, dim));
    }
    // explicit remote OpenAI-compatible endpoint → keep the network embedder
    Arc::new(OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model, cfg.embed_timeout_secs, dim))
}
```
> Confirm the canonical data-dir accessor (the repo forbids `dirs::home_dir` for `.uclaw` — use `uclaw_utils_home`/the established `data_dir()` helper; grep how STT's downloader resolves `~/.uclaw/models`). If `build_embedder` lacks a data-dir handle, thread it from the caller (`app.rs` builds the embedder with `memubot_config.embedding_endpoint` — it can pass `data_dir`); adjust the signature to `build_embedder(cfg, data_dir)` and update the `app.rs` call site.

- [ ] **Step 2: tests:**
```rust
#[test]
fn default_localhost_routes_to_onnx() {
    let cfg = EmbeddingEndpointConfig::default(); // localhost:7337
    let e = build_embedder(&cfg /*, data_dir */);
    assert_eq!(e.name(), "onnx-bge-small");
    assert_eq!(e.dim(), 384);
}
#[test]
fn remote_endpoint_routes_to_openai_compat() {
    let mut cfg = EmbeddingEndpointConfig::default();
    cfg.base_url = "https://api.example.com/v1".into();
    let e = build_embedder(&cfg /*, data_dir */);
    assert_eq!(e.name(), "openai-compat"); // confirm OpenAiCompatEmbedder::name()
}
#[test]
fn empty_base_url_routes_to_inert() {
    let mut cfg = EmbeddingEndpointConfig::default();
    cfg.base_url = "".into();
    assert_eq!(build_embedder(&cfg /*, data_dir */).name(), "inert"); // confirm InertEmbedder::name()
}
```
(Adjust for the data_dir signature. Confirm the existing `name()` strings for OpenAiCompat/Inert.)

- [ ] **Step 3:** `cargo build` empty; `cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail` green; update the `app.rs` `build_embedder(...)` call if the signature gained `data_dir`.
- [ ] **Step 4: commit** — `git add src-tauri/src/memory_bucket_seal/score/embed/factory.rs src-tauri/src/app.rs && git commit -m "feat(embed): build_embedder defaults to in-process OnnxEmbedder; retain OpenAiCompat for remote (Step 3a)"`.

---

### Task 4: whole-slice verification (+ parity / re-embed contingency)

- [ ] `cargo build 2>&1 | grep -E "^error" | head` → empty; `cargo clippy --lib 2>&1 | grep -E "^error" | head` → empty.
- [ ] `cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail` → green (pooling + factory + model_download; live embed `#[ignore]`).
- [ ] **Parity check (manual, before relying on existing vectors):** run the `#[ignore]` live embed for a fixed string; capture the vector; compare cosine vs a memU-FastEmbed reference vector for the same string (capture one from the running app's `/v1/embeddings` while memU still serves it). If cosine ≥ 0.999 → compatible, done. **If it diverges** → the CLS-pool/normalize or the ONNX export differs; either fix the pooling/inputs to match, OR run the **re-embed contingency**: a marker-gated boot pass (mirror P2b/P3 migrations) that re-embeds the existing `pages`/`skills`/`tool_stats` + summary embeddings with `OnnxEmbedder`. (Out of this plan unless parity fails — recorded so it's not a surprise.)
- [ ] `gitnexus_detect_changes()` before the PR.
- [ ] **Manual runtime (post-merge):** restart; first seal embed triggers the one-time model download (`~/.uclaw/models/bge-small-en-v1.5/`); confirm seals complete + no `localhost:7337` embedding traffic from bucket_seal.

## Adjacent-edit checklist (PR body)

- New dep `tokenizers`; new modules `onnx`/`model_download`.
- `build_embedder` signature may gain `data_dir` → `app.rs` call site updated.
- `OpenAiCompatEmbedder` + `InertEmbedder` retained (no behavior change for remote/empty configs).
- memU bridge boot / store / 7337 server UNCHANGED (Step 3b). This slice only changes which embedder bucket_seal uses by default.

## PR shape

Branch `claude/step3a-in-process-embedder`, one PR, `## Commits (bisectable)` table (Tasks 1–3 = 3 commits). Title: `feat(memory): Step 3a — in-process ONNX embedder (replace memU FastEmbed @7337)`. Body: in-process bge-small via ort+tokenizers (CLS pooling matches FastEmbed for vector compat); lazy model download; build_embedder defaults to it; OpenAiCompat retained for remote; memU store removal = Step 3b; parity verified (or re-embed contingency).

## Self-review notes

- **Spec coverage:** §1 embedder → Task 2; §2 downloader → Task 1; §3 factory → Task 3; parity/contingency → Task 4. CLS pooling (recon-confirmed) is the fixed contract. ✔
- **Type consistency:** `OnnxEmbedder::new(model_dir, dim)` + `cls_pool_normalize(&ArrayView2<f32>) -> Vec<f32>` + `Embedder{name,dim,embed}` consistent; `model_download::{model_dir,is_present,ensure_model}` used by factory + embedder. ✔
- **Bisectability:** T1 (dep+downloader, standalone) compiles; T2 (embedder, uses downloader) compiles; T3 (factory, uses embedder; updates app.rs caller atomically) compiles. ✔
- **Follow-the-recon items** (flagged): the `Arc<Mutex<Option<Loaded>>>` borrow shape in `embed` (Task 2 implementer-note — mirror STT's `blocking_lock` in `spawn_blocking`); the ONNX output name + optional `token_type_ids` (inspect `session.inputs/outputs`); the canonical `data_dir` helper + whether `build_embedder` needs it threaded (Task 3); `tokenizers` feature set that resolves; existing `name()` strings for OpenAiCompat/Inert (Task 3 tests). Each has explicit guidance.
