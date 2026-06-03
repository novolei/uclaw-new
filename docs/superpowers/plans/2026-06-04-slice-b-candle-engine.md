# Slice B — candle Local Inference Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Run MiniCPM5-1B Q4_K_M in-process via candle `quantized_llama`, expose it as an OpenAI-compatible `:7337/v1/chat/completions` endpoint (streaming + non-stream), and register it as a built-in provider `local/minicpm5-1b` so the existing 模型分配 dropdown (wired live by Slice A) can target it.

**Architecture:** A new `src-tauri/src/local_llm/` module mirrors the existing ONNX embedder's "Mutex-behind lazy-load + lifetime cache" shape (`memory_bucket_seal/score/embed/onnx.rs`). The candle model + tokenizer live behind a `tokio::Mutex<Option<Loaded>>`, lazy-loaded on first request (no 688 MB at startup), generation serialized behind the lock. The existing `LocalApiService` (`local_api/`) gains the chat-completions route, exactly mirroring how `/v1/embeddings` drives the `OnnxEmbedder`. Integration is **B2** (local HTTP), so the agent's existing OpenAI-compatible HTTP client and Slice A's role routing work unchanged.

**Tech Stack:** Rust, candle 0.10 (`candle-core` / `candle-transformers::models::quantized_llama` / `candle-nn`, `metal` feature on macOS, CPU fallback), `tokenizers 0.20` (already present), `axum 0.8` (SSE built-in), `tokio-stream` (already present), `reqwest` (Slice C does the downloading — B only reads from disk).

---

## Boundary with adjacent slices (read before starting)

- **Slice A (shipped, PR #652, main `739dc426`)** is the downstream consumer. It added `ProviderConfigs::resolve_role_llm` + `get_utility_llm_config()` / `get_summarizer_llm_config()` returning `ResolvedLlmConfig { provider_id, model_id, api_key, base_url, api_type }` (`providers/types.rs:273`, `providers/service.rs:166-210`). When a role points at `local/minicpm5-1b`, resolution yields `base_url=http://localhost:7337/v1`, `api_type=OpenAiCompletions`. **B does not touch role routing** — it only provides the endpoint + registers the provider so the dropdown can select it.
- **Slice C (next, not this PR)** owns the downloader and `model_manager.rs`. **Cache-path contract B must honor:** `~/.uclaw/models/minicpm5-1b/` containing `MiniCPM5-1B-Q4_K_M.gguf` + `tokenizer.json`. **B never downloads** — if files are absent, B returns a structured "model not ready" error. **Note for C (write in C's plan):** candle's quantized loader reads an *external* `tokenizer.json` (it does NOT use the GGUF's embedded tokenizer), so C must fetch `tokenizer.json` from the base repo `openbmb/MiniCPM5-1B` (the GGUF repo `openbmb/MiniCPM5-1B-GGUF` may lack it).
- **Degradation contract to A:** model missing / not-loaded / load-failed → structured OpenAI error body with HTTP **503** and `code: "model_not_ready"`. Request-time fallback to the cloud active model is the **consumer's** responsibility, not B's. Slice A as shipped does *config-level* resolution only; whether a 503 at request time triggers a runtime cloud fallback is **out of B's scope** — flag it in the PR body as a follow-up to verify in the role-routing call path.

## Model facts (verified at plan time)

- MiniCPM5-1B is a standard `LlamaForCausalLM` → candle `quantized_llama::ModelWeights::from_gguf` loads its GGUF directly, no custom arch module.
- Special tokens (from `openbmb/MiniCPM5-1B/tokenizer_config.json`): `bos=<s>`, `eos=</s>`, ChatML role markers `<|im_start|>` (130072) / `<|im_end|>` (130073). The repo ships **no** `chat_template` field, so we render the canonical ChatML layout and lock it with a gated smoke test (a wrong template makes `"2+2="` not contain `"4"`).
- Stop tokens for generation: both `</s>` and `<|im_end|>` (the assistant turn terminator in ChatML).

## candle 0.10 API facts (verified at plan time)

- `ModelWeights::from_gguf(ct: gguf_file::Content, reader: &mut R, device: &Device) -> Result<Self>` where `R: Seek + Read` (use `std::fs::File`).
- `ModelWeights::forward(&mut self, x: &Tensor, index_pos: usize) -> Result<Tensor>` — returns last-position logits `[1, vocab]`; `index_pos == 0` **resets** the per-layer KV cache, `index_pos > 0` concatenates.
- `ModelWeights::clear_kv_cache(&mut self)` — public; call it at the start of every generation as belt-and-suspenders.
- `LogitsProcessor::from_sampling(seed: u64, sampling: Sampling)` with `Sampling::{ArgMax, All{temperature}, TopK{k,temperature}, TopP{p,temperature}, TopKThenTopP{k,p,temperature}}`; `processor.sample(&logits) -> Result<u32>`.
- `candle_transformers::utils::apply_repeat_penalty(&logits, penalty: f32, &ctx_tokens) -> Result<Tensor>`.
- Device: `Device::new_metal(0)` (macOS), fallback `Device::Cpu`.

## File structure

| File | Responsibility |
|---|---|
| `src-tauri/Cargo.toml` | add candle deps (metal feature on macOS) |
| `src-tauri/src/lib.rs` | `pub mod local_llm;` declaration |
| `src-tauri/src/local_llm/mod.rs` | module wiring + cache-path helpers + `LocalLlmEngine` lifecycle (`load`/`unload`/`warmup`/`is_ready`/`generate`/`generate_stream`) + `NotReady` error |
| `src-tauri/src/local_llm/chat_template.rs` | pure ChatML renderer (`render_chatml`) + unit tests |
| `src-tauri/src/local_llm/engine.rs` | candle load + sampling-config mapping + generation loop + `TokenStream` UTF-8-safe decoder |
| `src-tauri/src/local_api/routes.rs` | `/v1/chat/completions` wire types + non-stream handler + SSE streaming handler |
| `src-tauri/src/local_api/server.rs` | thread the engine into `ApiState` / `LocalApiService` |
| `src-tauri/src/main.rs` | construct + inject the engine at `[Stage 3]` |
| `src-tauri/src/providers/registry.rs` | register the `local/minicpm5-1b` built-in provider |

All new `.rs` files start with `// SPDX-License-Identifier: Apache-2.0` (repo pre-commit hook enforces this).

---

## Task 1: Dependencies + module skeleton + cache-path helper

**Files:**
- Modify: `src-tauri/Cargo.toml` (`[dependencies]` + a macOS target table)
- Modify: `src-tauri/src/lib.rs` (add `pub mod local_llm;` near `pub mod local_api;` at line 63)
- Create: `src-tauri/src/local_llm/mod.rs`
- Create: `src-tauri/src/local_llm/engine.rs` (stub for now)
- Create: `src-tauri/src/local_llm/chat_template.rs` (stub for now)

- [ ] **Step 1: Add candle deps to `Cargo.toml`**

In the `[dependencies]` section (near `ort` at line 65) add:

```toml
# Local LLM inference (Slice B) — pure-Rust candle, no C++ runtime.
# quantized_llama loads MiniCPM5-1B Q4_K_M GGUF directly (standard Llama arch).
candle-core = "0.10"
candle-transformers = "0.10"
candle-nn = "0.10"
```

Then add a macOS target table so candle gets the Metal backend on the primary platform while staying CPU-only elsewhere (cargo unions target-specific features additively with the base dep):

```toml
# Metal acceleration for candle on macOS (the primary platform). On other OSes
# candle stays CPU-only via the base [dependencies] entry above.
[target.'cfg(target_os = "macos")'.dependencies]
candle-core = { version = "0.10", features = ["metal"] }
candle-transformers = { version = "0.10", features = ["metal"] }
```

- [ ] **Step 2: Declare the module in `lib.rs`**

After `pub mod local_api;` (line 63) add:

```rust
pub mod local_llm;
```

- [ ] **Step 3: Create the module file `src-tauri/src/local_llm/mod.rs` with the cache-path helper**

```rust
// SPDX-License-Identifier: Apache-2.0
//! In-process local LLM inference (MiniCPM5-1B via candle quantized_llama).
//!
//! Mirrors the ONNX embedder (`memory_bucket_seal/score/embed/onnx.rs`):
//! model + tokenizer live behind a `tokio::Mutex<Option<Loaded>>`, lazy-loaded
//! on first request (no 688 MB at startup), generation serialized behind the
//! lock. Exposed over HTTP by `LocalApiService` at `:7337/v1/chat/completions`.
//!
//! Cache-path contract with Slice C: model files live under
//! `<data_dir>/models/minicpm5-1b/`. Slice B only READS them; Slice C downloads.

use std::path::{Path, PathBuf};

pub mod chat_template;
pub mod engine;

/// The default model identifier as registered with the provider registry.
pub const MODEL_ID: &str = "minicpm5-1b";

/// GGUF filename for the default quant (Q4_K_M). Slice C writes this path.
pub const GGUF_FILENAME: &str = "MiniCPM5-1B-Q4_K_M.gguf";

/// External tokenizer (candle does NOT use the GGUF-embedded tokenizer).
pub const TOKENIZER_FILENAME: &str = "tokenizer.json";

/// Resolve the model cache directory under the uClaw data dir
/// (e.g. `~/.uclaw/models/minicpm5-1b/`).
pub fn model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("models").join(MODEL_ID)
}

/// `(gguf_path, tokenizer_path)` inside the model dir.
pub fn model_paths(data_dir: &Path) -> (PathBuf, PathBuf) {
    let dir = model_dir(data_dir);
    (dir.join(GGUF_FILENAME), dir.join(TOKENIZER_FILENAME))
}

/// True when both required files exist on disk (does NOT mean loaded).
pub fn is_present(data_dir: &Path) -> bool {
    let (gguf, tok) = model_paths(data_dir);
    gguf.exists() && tok.exists()
}

#[cfg(test)]
mod path_tests {
    use super::*;

    #[test]
    fn model_dir_under_data() {
        let d = model_dir(Path::new("/tmp/uclaw"));
        assert!(d.ends_with("models/minicpm5-1b"), "got {d:?}");
    }

    #[test]
    fn model_paths_name_the_two_files() {
        let (g, t) = model_paths(Path::new("/tmp/uclaw"));
        assert!(g.ends_with("models/minicpm5-1b/MiniCPM5-1B-Q4_K_M.gguf"));
        assert!(t.ends_with("models/minicpm5-1b/tokenizer.json"));
    }

    #[test]
    fn is_present_requires_both_files() {
        let tmp = tempfile::tempdir().unwrap();
        let data = tmp.path();
        let dir = model_dir(data);
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!is_present(data));
        std::fs::write(dir.join(GGUF_FILENAME), b"x").unwrap();
        assert!(!is_present(data));
        std::fs::write(dir.join(TOKENIZER_FILENAME), b"x").unwrap();
        assert!(is_present(data));
    }
}
```

- [ ] **Step 4: Create stub `src-tauri/src/local_llm/engine.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! candle quantized_llama load + generation. Filled in Tasks 3–4.
```

- [ ] **Step 5: Create stub `src-tauri/src/local_llm/chat_template.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! MiniCPM ChatML prompt rendering. Filled in Task 2.
```

- [ ] **Step 6: Build (candle's first compile is slow — expect minutes)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines (empty output).

- [ ] **Step 7: Run the path unit tests**

Run: `cd src-tauri && cargo test --lib local_llm::path_tests 2>&1 | tail -20`
Expected: `test result: ok. 3 passed`.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/lib.rs src-tauri/src/local_llm/
git commit -m "feat(local_llm): add candle deps + module skeleton + cache-path helper

Slice B Task 1. candle-core/transformers/nn (metal feature on macOS, CPU
fallback elsewhere). New src/local_llm/ with model cache-path helpers honoring
the Slice C contract (~/.uclaw/models/minicpm5-1b/). No model load yet."
```

---

## Task 2: ChatML prompt template (pure, fully unit-tested)

**Files:**
- Modify: `src-tauri/src/local_llm/chat_template.rs`

- [ ] **Step 1: Write the failing tests**

Replace the stub body with the tests first:

```rust
// SPDX-License-Identifier: Apache-2.0
//! MiniCPM ChatML prompt rendering — pure functions, no model/tokenizer.
//!
//! MiniCPM5-1B ships no `chat_template` in its tokenizer_config; it uses the
//! canonical ChatML layout with `<|im_start|>`/`<|im_end|>` role markers
//! (verified from openbmb/MiniCPM5-1B/tokenizer_config.json). We render that
//! layout and lock it here; a wrong template surfaces as garbage in the gated
//! engine smoke test.

/// One chat message in role/content form (matches the OpenAI wire shape).
#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage { role: role.into(), content: content.into() }
    }

    #[test]
    fn single_user_turn_opens_assistant() {
        let out = render_chatml(&[msg("user", "hi")]);
        assert_eq!(
            out,
            "<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn system_then_user() {
        let out = render_chatml(&[msg("system", "You are clawby."), msg("user", "2+2=")]);
        assert_eq!(
            out,
            "<|im_start|>system\nYou are clawby.<|im_end|>\n\
             <|im_start|>user\n2+2=<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn multi_turn_includes_prior_assistant() {
        let out = render_chatml(&[
            msg("user", "hi"),
            msg("assistant", "hello!"),
            msg("user", "bye"),
        ]);
        assert_eq!(
            out,
            "<|im_start|>user\nhi<|im_end|>\n\
             <|im_start|>assistant\nhello!<|im_end|>\n\
             <|im_start|>user\nbye<|im_end|>\n\
             <|im_start|>assistant\n"
        );
    }

    #[test]
    fn unknown_role_passes_through_verbatim() {
        // Don't silently drop/remap unknown roles — render them as-is so
        // misconfiguration is visible rather than hidden.
        let out = render_chatml(&[msg("tool", "result=4")]);
        assert_eq!(
            out,
            "<|im_start|>tool\nresult=4<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn empty_messages_still_opens_assistant() {
        assert_eq!(render_chatml(&[]), "<|im_start|>assistant\n");
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cd src-tauri && cargo test --lib local_llm::chat_template 2>&1 | tail -20`
Expected: compile error — `render_chatml` not found.

- [ ] **Step 3: Implement `render_chatml`**

Insert above the `#[cfg(test)]` block:

```rust
/// Render messages into MiniCPM ChatML and open the assistant turn.
///
/// Layout (per role): `<|im_start|>{role}\n{content}<|im_end|>\n`, then a final
/// `<|im_start|>assistant\n` to prompt generation. The `<s>` BOS is added by the
/// tokenizer at encode time (`add_special_tokens = true`), not here.
pub fn render_chatml(messages: &[ChatMessage]) -> String {
    let mut out = String::new();
    for m in messages {
        out.push_str("<|im_start|>");
        out.push_str(&m.role);
        out.push('\n');
        out.push_str(&m.content);
        out.push_str("<|im_end|>\n");
    }
    out.push_str("<|im_start|>assistant\n");
    out
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib local_llm::chat_template 2>&1 | tail -20`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/local_llm/chat_template.rs
git commit -m "feat(local_llm): MiniCPM ChatML prompt renderer (pure, unit-tested)

Slice B Task 2. render_chatml builds <|im_start|>/<|im_end|> turns and opens
the assistant turn. BOS left to the tokenizer (add_special_tokens=true)."
```

---

## Task 3: Sampling-config mapping + generation params + errors (pure, unit-tested)

**Files:**
- Modify: `src-tauri/src/local_llm/engine.rs`

- [ ] **Step 1: Write the failing tests**

Replace the engine stub with the types, the `NotReady`/`EngineError` enum, the pure sampling mapper, and tests:

```rust
// SPDX-License-Identifier: Apache-2.0
//! candle quantized_llama load + generation for MiniCPM5-1B.
//!
//! Mirrors the ONNX embedder's two-phase pattern: async lock to lazy-load,
//! then `spawn_blocking` + `blocking_lock` for the (synchronous, &mut) forward
//! loop. Generation is serialized behind the lock by construction.

use candle_transformers::generation::{LogitsProcessor, Sampling};

/// Generation parameters (mapped from the OpenAI request, with defaults).
#[derive(Debug, Clone)]
pub struct GenParams {
    pub temperature: f64,
    pub top_p: Option<f64>,
    pub top_k: Option<usize>,
    pub repeat_penalty: f32,
    /// How many recent tokens the repeat penalty considers.
    pub repeat_last_n: usize,
    pub max_tokens: usize,
    pub seed: u64,
    /// Extra stop strings beyond the built-in EOS / `<|im_end|>`.
    pub stop: Vec<String>,
}

impl Default for GenParams {
    fn default() -> Self {
        Self {
            temperature: 0.7,
            top_p: Some(0.9),
            top_k: None,
            repeat_penalty: 1.1,
            repeat_last_n: 64,
            max_tokens: 512,
            seed: 299792458,
            stop: Vec::new(),
        }
    }
}

/// Why a generation could not run / produce output.
#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    /// Model files missing on disk, or load not yet attempted/succeeded.
    /// Maps to HTTP 503 `model_not_ready` so the caller can fall back to cloud.
    #[error("model not ready: {0}")]
    NotReady(String),
    /// Load failed (corrupt GGUF, OOM, device init). Also a fall-back signal.
    #[error("model load failed: {0}")]
    LoadFailed(String),
    /// Inference-time failure (forward / sampling / decode).
    #[error("generation failed: {0}")]
    Generation(String),
}

/// Build a candle `Sampling` from params. `temperature <= 0` ⇒ greedy ArgMax
/// (deterministic), matching the candle quantized example's convention.
pub fn build_sampling(p: &GenParams) -> Sampling {
    if p.temperature <= 0.0 {
        return Sampling::ArgMax;
    }
    let t = p.temperature;
    match (p.top_k, p.top_p) {
        (None, None) => Sampling::All { temperature: t },
        (Some(k), None) => Sampling::TopK { k, temperature: t },
        (None, Some(pp)) => Sampling::TopP { p: pp, temperature: t },
        (Some(k), Some(pp)) => Sampling::TopKThenTopP { k, p: pp, temperature: t },
    }
}

/// Construct the candle logits processor for these params.
pub fn build_logits_processor(p: &GenParams) -> LogitsProcessor {
    LogitsProcessor::from_sampling(p.seed, build_sampling(p))
}

#[cfg(test)]
mod sampling_tests {
    use super::*;

    #[test]
    fn zero_temperature_is_argmax() {
        let p = GenParams { temperature: 0.0, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::ArgMax));
    }

    #[test]
    fn temp_only_is_all() {
        let p = GenParams { temperature: 0.8, top_p: None, top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::All { .. }));
    }

    #[test]
    fn temp_and_top_p_is_top_p() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: None, ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopP { .. }));
    }

    #[test]
    fn top_k_and_top_p_is_combined() {
        let p = GenParams { temperature: 0.8, top_p: Some(0.9), top_k: Some(40), ..Default::default() };
        assert!(matches!(build_sampling(&p), Sampling::TopKThenTopP { .. }));
    }

    #[test]
    fn defaults_are_sane() {
        let p = GenParams::default();
        assert_eq!(p.max_tokens, 512);
        assert_eq!(p.repeat_last_n, 64);
        assert!((p.repeat_penalty - 1.1).abs() < 1e-6);
    }
}
```

- [ ] **Step 2: Ensure `thiserror` is available**

Run: `cd src-tauri && grep -n '^thiserror' Cargo.toml`
Expected: a `thiserror = ...` line. If absent, add `thiserror = "1"` to `[dependencies]` in this commit.

- [ ] **Step 3: Run tests to verify they fail then pass**

Run: `cd src-tauri && cargo test --lib local_llm::engine::sampling_tests 2>&1 | tail -20`
Expected: `test result: ok. 5 passed`.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/local_llm/engine.rs src-tauri/Cargo.toml src-tauri/Cargo.lock
git commit -m "feat(local_llm): GenParams + Sampling mapping + EngineError (pure)

Slice B Task 3. temperature/top-p/top-k → candle Sampling; <=0 temp ⇒ ArgMax.
EngineError::NotReady is the structured 503 signal to the role router."
```

---

## Task 4: candle load + generation loop + UTF-8-safe token decoder (gated smoke test)

**Files:**
- Modify: `src-tauri/src/local_llm/engine.rs`

- [ ] **Step 1: Add the `Loaded` state, device selection, loader, `TokenStream`, and the blocking generation core**

Append to `engine.rs` (above the `#[cfg(test)]` blocks):

```rust
use std::io::Seek;
use std::path::Path;

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use candle_transformers::models::quantized_llama::ModelWeights;
use tokenizers::Tokenizer;

/// A loaded model + tokenizer + device. Lives behind the Mutex in `mod.rs`.
pub struct Loaded {
    pub model: ModelWeights,
    pub tokenizer: Tokenizer,
    pub device: Device,
    /// Token ids that terminate generation (EOS `</s>` and `<|im_end|>`).
    pub stop_ids: Vec<u32>,
}

/// Pick Metal on macOS, else CPU. Metal init failure falls back to CPU.
pub fn select_device() -> Device {
    #[cfg(target_os = "macos")]
    {
        match Device::new_metal(0) {
            Ok(d) => {
                tracing::info!("[local_llm] using Metal device");
                return d;
            }
            Err(e) => tracing::warn!("[local_llm] Metal init failed ({e}); CPU fallback"),
        }
    }
    tracing::info!("[local_llm] using CPU device");
    Device::Cpu
}

/// Load the GGUF + external tokenizer from disk. Blocking (file IO + GPU upload);
/// callers run it under `spawn_blocking`. Caller guarantees files exist.
pub fn load(gguf_path: &Path, tokenizer_path: &Path, device: Device) -> Result<Loaded, EngineError> {
    let mut file = std::fs::File::open(gguf_path)
        .map_err(|e| EngineError::LoadFailed(format!("open gguf: {e}")))?;
    let content = gguf_file::Content::read(&mut file)
        .map_err(|e| EngineError::LoadFailed(format!("read gguf: {e}")))?;
    // from_gguf needs the reader positioned to read tensor data after the header.
    file.rewind()
        .map_err(|e| EngineError::LoadFailed(format!("rewind gguf: {e}")))?;
    let content = {
        // Re-read content after rewind so the reader offset matches what
        // from_gguf expects (it reads the header then streams tensors).
        let mut f = std::fs::File::open(gguf_path)
            .map_err(|e| EngineError::LoadFailed(format!("reopen gguf: {e}")))?;
        let c = gguf_file::Content::read(&mut f)
            .map_err(|e| EngineError::LoadFailed(format!("reread gguf: {e}")))?;
        let model = ModelWeights::from_gguf(c, &mut f, &device)
            .map_err(|e| EngineError::LoadFailed(format!("from_gguf: {e}")))?;
        let _ = content; // header already validated above
        model
    };
    let model = content;
    let tokenizer = Tokenizer::from_file(tokenizer_path)
        .map_err(|e| EngineError::LoadFailed(format!("tokenizer: {e}")))?;

    // Resolve stop ids: </s> and <|im_end|>. Missing markers are skipped.
    let mut stop_ids = Vec::new();
    for tok in ["</s>", "<|im_end|>"] {
        if let Some(id) = tokenizer.token_to_id(tok) {
            stop_ids.push(id);
        }
    }
    if stop_ids.is_empty() {
        return Err(EngineError::LoadFailed(
            "tokenizer has neither </s> nor <|im_end|>".into(),
        ));
    }

    Ok(Loaded { model, tokenizer, device, stop_ids })
}

/// UTF-8-safe incremental decoder (mirrors candle's TokenOutputStream). Emits a
/// text delta only when appending the new token yields complete UTF-8 — critical
/// for multi-byte scripts (Chinese) where one char spans several tokens.
struct TokenStream<'a> {
    tokenizer: &'a Tokenizer,
    tokens: Vec<u32>,
    prev_index: usize,
    current_index: usize,
}

impl<'a> TokenStream<'a> {
    fn new(tokenizer: &'a Tokenizer) -> Self {
        Self { tokenizer, tokens: Vec::new(), prev_index: 0, current_index: 0 }
    }

    fn decode(&self, ids: &[u32]) -> Result<String, EngineError> {
        self.tokenizer
            .decode(ids, true)
            .map_err(|e| EngineError::Generation(format!("decode: {e}")))
    }

    /// Push a token; return the newly-complete text delta (or None if the
    /// pending bytes don't yet form a complete suffix).
    fn push(&mut self, token: u32) -> Result<Option<String>, EngineError> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            self.decode(&self.tokens[self.prev_index..self.current_index])?
        };
        self.tokens.push(token);
        let text = self.decode(&self.tokens[self.prev_index..])?;
        if text.len() > prev_text.len() && text.chars().last().map_or(false, |c| !c.is_control()) {
            let delta = text[prev_text.len()..].to_string();
            self.prev_index = self.current_index;
            self.current_index = self.tokens.len();
            Ok(Some(delta))
        } else {
            Ok(None)
        }
    }

    /// Flush any remaining buffered text at end of generation.
    fn finish(&self) -> Result<String, EngineError> {
        let prev_text = if self.tokens.is_empty() {
            String::new()
        } else {
            self.decode(&self.tokens[self.prev_index..self.current_index])?
        };
        let text = self.decode(&self.tokens[self.prev_index..])?;
        if text.len() > prev_text.len() {
            Ok(text[prev_text.len()..].to_string())
        } else {
            Ok(String::new())
        }
    }
}

/// Why generation stopped — surfaced as OpenAI `finish_reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
}

impl FinishReason {
    pub fn as_str(&self) -> &'static str {
        match self {
            FinishReason::Stop => "stop",
            FinishReason::Length => "length",
        }
    }
}

/// Run a full generation against a loaded model, invoking `on_delta` for each
/// text fragment. Synchronous + `&mut` (KV cache); callers run under
/// `spawn_blocking` while holding the model lock. Returns the finish reason and
/// completion token count.
pub fn generate(
    loaded: &mut Loaded,
    prompt: &str,
    params: &GenParams,
    mut on_delta: impl FnMut(&str),
) -> Result<(FinishReason, usize), EngineError> {
    loaded.model.clear_kv_cache();

    let enc = loaded
        .tokenizer
        .encode(prompt, true)
        .map_err(|e| EngineError::Generation(format!("encode: {e}")))?;
    let prompt_tokens: Vec<u32> = enc.get_ids().to_vec();
    if prompt_tokens.is_empty() {
        return Err(EngineError::Generation("empty prompt after tokenize".into()));
    }

    let mut logits_processor = build_logits_processor(params);
    let mut all_tokens: Vec<u32> = Vec::new();
    let mut stream = TokenStream::new(&loaded.tokenizer);
    let mut accumulated = String::new();

    // Prefill: feed the whole prompt at index_pos 0 (resets KV cache).
    let input = Tensor::new(prompt_tokens.as_slice(), &loaded.device)
        .and_then(|t| t.unsqueeze(0))
        .map_err(|e| EngineError::Generation(format!("prompt tensor: {e}")))?;
    let mut logits = loaded
        .model
        .forward(&input, 0)
        .map_err(|e| EngineError::Generation(format!("prefill forward: {e}")))?
        .squeeze(0)
        .map_err(|e| EngineError::Generation(format!("squeeze: {e}")))?;

    let mut finish = FinishReason::Length;
    for step in 0..params.max_tokens {
        // Repeat penalty over recent tokens.
        if params.repeat_penalty != 1.0 && !all_tokens.is_empty() {
            let start = all_tokens.len().saturating_sub(params.repeat_last_n);
            logits = candle_transformers::utils::apply_repeat_penalty(
                &logits,
                params.repeat_penalty,
                &all_tokens[start..],
            )
            .map_err(|e| EngineError::Generation(format!("repeat penalty: {e}")))?;
        }

        let next = logits_processor
            .sample(&logits)
            .map_err(|e| EngineError::Generation(format!("sample: {e}")))?;

        if loaded.stop_ids.contains(&next) {
            finish = FinishReason::Stop;
            break;
        }

        all_tokens.push(next);
        if let Some(delta) = stream.push(next)? {
            accumulated.push_str(&delta);
            // Honor user stop strings on the accumulated text.
            if let Some(cut) = first_stop_hit(&accumulated, &params.stop) {
                let keep = &accumulated[..cut];
                // Emit only the not-yet-emitted portion up to the cut.
                let already = accumulated.len() - delta.len();
                if cut > already {
                    on_delta(&keep[already..]);
                }
                finish = FinishReason::Stop;
                break;
            }
            on_delta(&delta);
        }

        // Decode next step: single token at the advancing position.
        let input = Tensor::new(&[next], &loaded.device)
            .and_then(|t| t.unsqueeze(0))
            .map_err(|e| EngineError::Generation(format!("step tensor: {e}")))?;
        logits = loaded
            .model
            .forward(&input, prompt_tokens.len() + step)
            .map_err(|e| EngineError::Generation(format!("step forward: {e}")))?
            .squeeze(0)
            .map_err(|e| EngineError::Generation(format!("squeeze: {e}")))?;
    }

    // Flush any trailing buffered text (skip if a stop string already cut us off).
    if finish == FinishReason::Length {
        let tail = stream.finish()?;
        if !tail.is_empty() {
            on_delta(&tail);
        }
    }

    Ok((finish, all_tokens.len()))
}

/// Index of the first user-stop-string occurrence in `text`, or None.
fn first_stop_hit(text: &str, stops: &[String]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for s in stops {
        if s.is_empty() {
            continue;
        }
        if let Some(i) = text.find(s.as_str()) {
            best = Some(best.map_or(i, |b| b.min(i)));
        }
    }
    best
}
```

> **Implementer note:** The double-read in `load` exists because `gguf_file::Content::read` advances the reader past the header and `from_gguf` then streams the tensor data from the reader's current position. The cleanest form is a single read + `from_gguf(content, &mut file, &device)` reusing the *same* `file` handle (as the candle example does). Simplify to that single-read form if it compiles cleanly against the resolved candle 0.10 API; the smoke test in Step 4 is the correctness gate either way.

- [ ] **Step 2: Add a pure unit test for `first_stop_hit` + a gated smoke test**

Append a test module to `engine.rs`:

```rust
#[cfg(test)]
mod stop_tests {
    use super::*;

    #[test]
    fn finds_earliest_stop() {
        assert_eq!(first_stop_hit("abcSTOPdef", &["STOP".into()]), Some(3));
        assert_eq!(
            first_stop_hit("aXbYc", &["Y".into(), "X".into()]),
            Some(1),
            "earliest match wins"
        );
        assert_eq!(first_stop_hit("nomatch", &["zzz".into()]), None);
        assert_eq!(first_stop_hit("anything", &[]), None);
    }
}

#[cfg(test)]
mod gated_engine_tests {
    use super::*;
    use crate::local_llm;

    /// Model-backed smoke test. Skips (does NOT fail) when the 688 MB model is
    /// absent — mirrors the embedder's `#[ignore]` live tests so CI isn't blocked.
    /// Run locally after Slice C downloads the model, or place files manually.
    #[test]
    fn smoke_generate_two_plus_two() {
        let data_dir = dirs_home_uclaw();
        if !local_llm::is_present(&data_dir) {
            eprintln!("[skip] MiniCPM model not present under {data_dir:?}");
            return;
        }
        let (gguf, tok) = local_llm::model_paths(&data_dir);
        let device = select_device();
        let mut loaded = load(&gguf, &tok, device).expect("load");
        let prompt = local_llm::chat_template::render_chatml(&[
            local_llm::chat_template::ChatMessage { role: "user".into(), content: "2+2=".into() },
        ]);
        let params = GenParams { temperature: 0.0, max_tokens: 16, ..Default::default() };
        let mut out = String::new();
        let (reason, n) = generate(&mut loaded, &prompt, &params, |d| out.push_str(d)).expect("gen");
        eprintln!("smoke out={out:?} reason={reason:?} tokens={n}");
        assert!(out.contains('4'), "expected '4' in {out:?}");
    }

    /// Resolve `~/.uclaw` without the banned `dirs::home_dir` (repo pre-commit
    /// hook blocks it). Uses HOME directly for the test only.
    fn dirs_home_uclaw() -> std::path::PathBuf {
        let home = std::env::var("HOME").expect("HOME set");
        std::path::Path::new(&home).join(".uclaw")
    }
}
```

- [ ] **Step 3: Build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines.

- [ ] **Step 4: Run tests (gated test skips without the model)**

Run: `cd src-tauri && cargo test --lib local_llm::engine 2>&1 | tail -20`
Expected: `stop_tests` pass; `smoke_generate_two_plus_two` prints `[skip]` and passes (model absent in CI). If the model is present locally, it prints `smoke out=...` containing `4`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/local_llm/engine.rs
git commit -m "feat(local_llm): candle load + generation loop + UTF-8 token stream

Slice B Task 4. quantized_llama from_gguf + Metal/CPU select; prefill + KV-cache
step loop; repeat penalty; EOS/<|im_end|> + user stop strings; incremental
UTF-8-safe decode (Chinese-safe). Smoke gen test gated on model presence."
```

---

## Task 5: `LocalLlmEngine` lifecycle (lazy-load, warmup, is_ready, not-ready error)

**Files:**
- Modify: `src-tauri/src/local_llm/mod.rs`

- [ ] **Step 1: Write the failing tests (not-ready states are testable without a model)**

Append to `mod.rs`:

```rust
#[cfg(test)]
mod engine_lifecycle_tests {
    use super::*;

    #[tokio::test]
    async fn not_ready_when_files_absent() {
        let tmp = tempfile::tempdir().unwrap();
        let eng = LocalLlmEngine::new(tmp.path().to_path_buf());
        assert!(!eng.is_ready().await);
        let err = eng
            .generate("<|im_start|>user\nhi<|im_end|>\n<|im_start|>assistant\n", &engine::GenParams::default(), |_| {})
            .await
            .expect_err("absent model must error");
        assert!(matches!(err, engine::EngineError::NotReady(_)), "got {err:?}");
    }

    #[tokio::test]
    async fn warmup_on_absent_model_reports_not_ready() {
        let tmp = tempfile::tempdir().unwrap();
        let eng = LocalLlmEngine::new(tmp.path().to_path_buf());
        let err = eng.warmup().await.expect_err("absent model warmup must error");
        assert!(matches!(err, engine::EngineError::NotReady(_)));
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib local_llm::engine_lifecycle 2>&1 | tail -20`
Expected: compile error — `LocalLlmEngine` not found.

- [ ] **Step 3: Implement `LocalLlmEngine`**

Insert into `mod.rs` (above the test modules, after the path helpers):

```rust
use std::sync::Arc;

use engine::{EngineError, FinishReason, GenParams, Loaded};

/// Long-lived in-process MiniCPM engine. Model + tokenizer are lazy-loaded on
/// first use and cached behind a `tokio::Mutex`; generation is serialized by the
/// lock. Mirrors `OnnxEmbedder`'s two-phase (async-load / blocking-infer) shape.
pub struct LocalLlmEngine {
    data_dir: PathBuf,
    inner: Arc<tokio::sync::Mutex<Option<Loaded>>>,
}

impl LocalLlmEngine {
    pub fn new(data_dir: PathBuf) -> Self {
        Self { data_dir, inner: Arc::new(tokio::sync::Mutex::new(None)) }
    }

    /// True iff the model is currently loaded in memory.
    pub async fn is_ready(&self) -> bool {
        self.inner.lock().await.is_some()
    }

    /// True iff the model files exist on disk (Slice C has downloaded them).
    pub fn is_present(&self) -> bool {
        is_present(&self.data_dir)
    }

    /// Ensure the model is loaded; `NotReady` if files are absent. Idempotent.
    async fn ensure_loaded(&self) -> Result<(), EngineError> {
        let mut guard = self.inner.lock().await;
        if guard.is_some() {
            return Ok(());
        }
        if !is_present(&self.data_dir) {
            return Err(EngineError::NotReady(format!(
                "model files missing under {:?}",
                model_dir(&self.data_dir)
            )));
        }
        let (gguf, tok) = model_paths(&self.data_dir);
        let loaded = tokio::task::spawn_blocking(move || {
            let device = engine::select_device();
            engine::load(&gguf, &tok, device)
        })
        .await
        .map_err(|e| EngineError::LoadFailed(format!("spawn_blocking join: {e}")))??;
        *guard = Some(loaded);
        tracing::info!("[local_llm] model loaded");
        Ok(())
    }

    /// Load + run a 1-token forward to JIT Metal kernels / page in weights.
    pub async fn warmup(&self) -> Result<(), EngineError> {
        self.ensure_loaded().await?;
        let params = GenParams { max_tokens: 1, temperature: 0.0, ..Default::default() };
        let inner = self.inner.clone();
        tokio::task::spawn_blocking(move || -> Result<(), EngineError> {
            let mut guard = inner.blocking_lock();
            let loaded = guard.as_mut().ok_or_else(|| EngineError::NotReady("unloaded".into()))?;
            engine::generate(loaded, "<|im_start|>assistant\n", &params, |_| {})?;
            Ok(())
        })
        .await
        .map_err(|e| EngineError::Generation(format!("spawn_blocking join (warmup): {e}")))??;
        tracing::info!("[local_llm] warmup complete");
        Ok(())
    }

    /// Drop the loaded model, freeing ~688 MB.
    pub async fn unload(&self) {
        *self.inner.lock().await = None;
        tracing::info!("[local_llm] model unloaded");
    }

    /// Generate to completion, invoking `on_delta` per text fragment. Serialized
    /// behind the model lock. `NotReady` when files are absent.
    pub async fn generate(
        &self,
        prompt: &str,
        params: &GenParams,
        mut on_delta: impl FnMut(&str) + Send + 'static,
    ) -> Result<(FinishReason, usize), EngineError> {
        self.ensure_loaded().await?;
        let inner = self.inner.clone();
        let prompt = prompt.to_string();
        let params = params.clone();
        tokio::task::spawn_blocking(move || -> Result<(FinishReason, usize), EngineError> {
            let mut guard = inner.blocking_lock();
            let loaded = guard.as_mut().ok_or_else(|| EngineError::NotReady("unloaded".into()))?;
            engine::generate(loaded, &prompt, &params, |d| on_delta(d))
        })
        .await
        .map_err(|e| EngineError::Generation(format!("spawn_blocking join (gen): {e}")))?
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cd src-tauri && cargo test --lib local_llm::engine_lifecycle 2>&1 | tail -20`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/local_llm/mod.rs
git commit -m "feat(local_llm): LocalLlmEngine lifecycle (lazy-load/warmup/unload)

Slice B Task 5. Mutex-behind lazy load mirroring OnnxEmbedder; warmup runs a
1-token forward; generate serializes behind the lock. NotReady when model files
absent — the structured fall-back signal for the role router."
```

---

## Task 6: `/v1/chat/completions` wire types + non-stream handler (TDD serde + 503)

**Files:**
- Modify: `src-tauri/src/local_api/routes.rs`
- Modify: `src-tauri/src/local_api/mod.rs` (export nothing new; route added in `create_router`)

- [ ] **Step 1: Add the engine to `ApiState`**

In `routes.rs`, extend `ApiState` (currently lines 13-21):

```rust
pub struct ApiState {
    pub start_time: std::time::Instant,
    pub embedder: Arc<dyn crate::memory_bucket_seal::Embedder>,
    /// In-process MiniCPM engine backing `/v1/chat/completions` (Slice B).
    pub local_llm: Arc<crate::local_llm::LocalLlmEngine>,
}
```

- [ ] **Step 2: Add the route to `create_router`**

In `create_router` (after the `/v1/embeddings` line 48) add:

```rust
        .route("/v1/chat/completions", post(chat_completions))
```

- [ ] **Step 3: Write the failing tests**

Append a test module to `routes.rs`:

```rust
#[cfg(test)]
mod chat_completions_tests {
    use super::*;
    use axum::extract::State;
    use crate::memory_bucket_seal::InertEmbedder;

    fn state_with_absent_model() -> Arc<ApiState> {
        let tmp = tempfile::tempdir().unwrap();
        // Leak the tempdir so the path stays valid for the test's lifetime.
        let path = tmp.keep();
        Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(path)),
        })
    }

    #[test]
    fn request_deserializes_messages_and_flags() {
        let req: ChatCompletionsRequest = serde_json::from_str(
            r#"{"model":"local/minicpm5-1b","messages":[{"role":"user","content":"hi"}],"stream":true,"temperature":0.5,"max_tokens":10}"#,
        )
        .unwrap();
        assert_eq!(req.messages.len(), 1);
        assert_eq!(req.messages[0].role, "user");
        assert!(req.stream);
        assert_eq!(req.max_tokens, Some(10));
    }

    #[test]
    fn stop_field_accepts_string_or_array() {
        let one: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[],"stop":"END"}"#).unwrap();
        assert_eq!(one.stop_strings(), vec!["END".to_string()]);
        let many: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[],"stop":["A","B"]}"#).unwrap();
        assert_eq!(many.stop_strings(), vec!["A".to_string(), "B".to_string()]);
        let none: ChatCompletionsRequest =
            serde_json::from_str(r#"{"messages":[]}"#).unwrap();
        assert!(none.stop_strings().is_empty());
    }

    #[test]
    fn params_mapping_applies_request_overrides() {
        let req: ChatCompletionsRequest = serde_json::from_str(
            r#"{"messages":[],"temperature":0.2,"top_p":0.5,"max_tokens":7}"#,
        )
        .unwrap();
        let p = req.to_gen_params();
        assert!((p.temperature - 0.2).abs() < 1e-9);
        assert_eq!(p.top_p, Some(0.5));
        assert_eq!(p.max_tokens, 7);
    }

    #[tokio::test]
    async fn non_stream_returns_503_when_model_absent() {
        let state = state_with_absent_model();
        let req = ChatCompletionsRequest {
            model: Some("local/minicpm5-1b".into()),
            messages: vec![ChatMessageDto { role: "user".into(), content: "hi".into() }],
            stream: false,
            temperature: None,
            top_p: None,
            top_k: None,
            max_tokens: None,
            stop: None,
        };
        let result = chat_completions(State(state), Json(req)).await;
        let resp = result.into_response();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }
}
```

- [ ] **Step 4: Run to verify failure**

Run: `cd src-tauri && cargo test --lib local_api::routes::chat_completions 2>&1 | tail -20`
Expected: compile error — types / `chat_completions` not found.

- [ ] **Step 5: Implement the wire types + non-stream handler**

Add to `routes.rs` (after the embeddings handler, before its test module). Note `IntoResponse` and `Sse` imports — extend the top `use axum::...` block to include `response::{IntoResponse, Sse, sse::Event}` and `Json`:

```rust
// ===== OpenAI /v1/chat/completions (Slice B — local MiniCPM) =====

use crate::local_llm::chat_template::{render_chatml, ChatMessage};
use crate::local_llm::engine::GenParams;

#[derive(Debug, Deserialize)]
pub struct ChatMessageDto {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum StopField {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Deserialize)]
pub struct ChatCompletionsRequest {
    #[serde(default)]
    pub model: Option<String>,
    pub messages: Vec<ChatMessageDto>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub top_k: Option<usize>,
    #[serde(default)]
    pub max_tokens: Option<usize>,
    #[serde(default)]
    pub stop: Option<StopField>,
}

impl ChatCompletionsRequest {
    pub fn stop_strings(&self) -> Vec<String> {
        match &self.stop {
            None => Vec::new(),
            Some(StopField::One(s)) => vec![s.clone()],
            Some(StopField::Many(v)) => v.clone(),
        }
    }

    /// Map request → GenParams, applying overrides over the defaults.
    pub fn to_gen_params(&self) -> GenParams {
        let d = GenParams::default();
        GenParams {
            temperature: self.temperature.unwrap_or(d.temperature),
            top_p: self.top_p.or(d.top_p),
            top_k: self.top_k.or(d.top_k),
            max_tokens: self.max_tokens.unwrap_or(d.max_tokens),
            stop: self.stop_strings(),
            ..d
        }
    }

    fn prompt(&self) -> String {
        let msgs: Vec<ChatMessage> = self
            .messages
            .iter()
            .map(|m| ChatMessage { role: m.role.clone(), content: m.content.clone() })
            .collect();
        render_chatml(&msgs)
    }
}

#[derive(Serialize)]
struct RespMessage {
    role: &'static str,
    content: String,
}

#[derive(Serialize)]
struct Choice {
    index: usize,
    message: RespMessage,
    finish_reason: String,
}

#[derive(Serialize)]
struct Usage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
}

#[derive(Serialize)]
struct ChatCompletionsResponse {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<Choice>,
    usage: Usage,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn response_model_name(req_model: &Option<String>) -> String {
    req_model.clone().unwrap_or_else(|| format!("local/{}", crate::local_llm::MODEL_ID))
}

/// Build the OpenAI 503 body for a not-ready local model.
fn not_ready_response(msg: String) -> (StatusCode, Json<OpenAIErrorBody>) {
    openai_error(
        StatusCode::SERVICE_UNAVAILABLE,
        msg,
        "server_error",
        Some("model_not_ready"),
    )
}

/// POST /v1/chat/completions — OpenAI-compatible, backed by the in-process
/// MiniCPM engine. Streams SSE when `stream=true`, else returns one JSON body.
/// Returns 503 `model_not_ready` when the model is unavailable so the role
/// router can fall back to the cloud active model.
async fn chat_completions(
    State(state): State<Arc<ApiState>>,
    Json(req): Json<ChatCompletionsRequest>,
) -> axum::response::Response {
    if req.stream {
        return chat_completions_stream(state, req).await;
    }

    let params = req.to_gen_params();
    let prompt = req.prompt();
    let model_name = response_model_name(&req.model);

    let buf = Arc::new(std::sync::Mutex::new(String::new()));
    let buf_w = buf.clone();
    let result = state
        .local_llm
        .generate(&prompt, &params, move |d| {
            buf_w.lock().unwrap().push_str(d);
        })
        .await;

    match result {
        Ok((reason, n_tokens)) => {
            let content = std::mem::take(&mut *buf.lock().unwrap());
            let resp = ChatCompletionsResponse {
                id: format!("chatcmpl-{}", uuid::Uuid::new_v4()),
                object: "chat.completion",
                created: now_unix(),
                model: model_name,
                choices: vec![Choice {
                    index: 0,
                    message: RespMessage { role: "assistant", content },
                    finish_reason: reason.as_str().to_string(),
                }],
                usage: Usage {
                    prompt_tokens: 0,
                    completion_tokens: n_tokens as u32,
                    total_tokens: n_tokens as u32,
                },
            };
            (StatusCode::OK, Json(resp)).into_response()
        }
        Err(crate::local_llm::engine::EngineError::NotReady(m)) => {
            not_ready_response(format!("local model not ready: {m}")).into_response()
        }
        Err(e) => openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("generation failed: {e}"),
            "server_error",
            Some("generation_failed"),
        )
        .into_response(),
    }
}
```

> The `chat_completions_stream` function is implemented in Task 7. To keep this task compiling, add a temporary stub directly below `chat_completions`:
>
> ```rust
> async fn chat_completions_stream(
>     state: Arc<ApiState>,
>     req: ChatCompletionsRequest,
> ) -> axum::response::Response {
>     // Replaced in Task 7 with real SSE streaming.
>     let _ = (&state, &req);
>     openai_error(
>         StatusCode::NOT_IMPLEMENTED,
>         "streaming not yet implemented",
>         "server_error",
>         None,
>     )
>     .into_response()
> }
> ```

- [ ] **Step 6: Build + run tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines.
Run: `cd src-tauri && cargo test --lib local_api::routes::chat_completions 2>&1 | tail -20`
Expected: `test result: ok. 4 passed`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/local_api/routes.rs
git commit -m "feat(local_api): /v1/chat/completions wire types + non-stream handler

Slice B Task 6. OpenAI-compatible request/response; stop string|array; param
mapping; non-stream JSON path. 503 model_not_ready when the engine is unavailable
(the role router's cloud fall-back signal). Streaming stubbed (Task 7)."
```

---

## Task 7: SSE streaming path (`stream: true`)

**Files:**
- Modify: `src-tauri/src/local_api/routes.rs`

- [ ] **Step 1: Replace the streaming stub with the real SSE handler**

Replace the `chat_completions_stream` stub from Task 6 with:

```rust
#[derive(Serialize)]
struct ChunkDelta {
    #[serde(skip_serializing_if = "Option::is_none")]
    role: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    content: Option<String>,
}

#[derive(Serialize)]
struct ChunkChoice {
    index: usize,
    delta: ChunkDelta,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish_reason: Option<String>,
}

#[derive(Serialize)]
struct ChatCompletionChunk {
    id: String,
    object: &'static str,
    created: u64,
    model: String,
    choices: Vec<ChunkChoice>,
}

/// SSE streaming for `stream=true`. Generation runs on a blocking thread; text
/// deltas flow over an unbounded channel and are emitted as OpenAI
/// `chat.completion.chunk` events, terminated by `data: [DONE]`.
async fn chat_completions_stream(
    state: Arc<ApiState>,
    req: ChatCompletionsRequest,
) -> axum::response::Response {
    // Pre-flight readiness: if files are absent, fail with 503 BEFORE opening
    // the SSE stream so the role router sees a clean error, not a half-stream.
    if !state.local_llm.is_present() && !state.local_llm.is_ready().await {
        return not_ready_response(
            "local model not ready: files missing".to_string(),
        )
        .into_response();
    }

    let params = req.to_gen_params();
    let prompt = req.prompt();
    let model_name = response_model_name(&req.model);
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
    let created = now_unix();

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<ChatCompletionChunk>();

    // First chunk: announce the assistant role (OpenAI convention).
    let _ = tx.send(ChatCompletionChunk {
        id: id.clone(),
        object: "chat.completion.chunk",
        created,
        model: model_name.clone(),
        choices: vec![ChunkChoice {
            index: 0,
            delta: ChunkDelta { role: Some("assistant"), content: None },
            finish_reason: None,
        }],
    });

    let engine = state.local_llm.clone();
    let id_gen = id.clone();
    let model_gen = model_name.clone();
    let tx_gen = tx.clone();
    tokio::spawn(async move {
        let tx_delta = tx_gen.clone();
        let id_d = id_gen.clone();
        let model_d = model_gen.clone();
        let result = engine
            .generate(&prompt, &params, move |d| {
                let _ = tx_delta.send(ChatCompletionChunk {
                    id: id_d.clone(),
                    object: "chat.completion.chunk",
                    created,
                    model: model_d.clone(),
                    choices: vec![ChunkChoice {
                        index: 0,
                        delta: ChunkDelta { role: None, content: Some(d.to_string()) },
                        finish_reason: None,
                    }],
                });
            })
            .await;

        let finish = match result {
            Ok((reason, _)) => reason.as_str().to_string(),
            Err(e) => {
                tracing::warn!("[local_api] stream generation error: {e}");
                "error".to_string()
            }
        };
        // Terminal chunk with finish_reason.
        let _ = tx_gen.send(ChatCompletionChunk {
            id: id_gen,
            object: "chat.completion.chunk",
            created,
            model: model_gen,
            choices: vec![ChunkChoice {
                index: 0,
                delta: ChunkDelta { role: None, content: None },
                finish_reason: Some(finish),
            }],
        });
    });

    // Adapt the channel into an SSE stream; close with the `[DONE]` sentinel.
    use tokio_stream::wrappers::UnboundedReceiverStream;
    use tokio_stream::StreamExt;
    let body = UnboundedReceiverStream::new(rx)
        .map(|chunk| {
            let json = serde_json::to_string(&chunk).unwrap_or_else(|_| "{}".to_string());
            Ok::<Event, std::convert::Infallible>(Event::default().data(json))
        })
        .chain(tokio_stream::iter(vec![Ok(Event::default().data("[DONE]"))]));

    Sse::new(body).into_response()
}
```

- [ ] **Step 2: Add a gated streaming smoke test**

Append to the `chat_completions_tests` module in `routes.rs`:

```rust
    /// Gated streaming smoke test: only runs when the model is present locally.
    /// Verifies the stream yields assistant content and a DONE terminator.
    #[tokio::test]
    async fn stream_emits_content_when_model_present() {
        let home = match std::env::var("HOME") {
            Ok(h) => h,
            Err(_) => return,
        };
        let data_dir = std::path::Path::new(&home).join(".uclaw");
        if !crate::local_llm::is_present(&data_dir) {
            eprintln!("[skip] model not present");
            return;
        }
        let state = Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(data_dir)),
        });
        let req = ChatCompletionsRequest {
            model: None,
            messages: vec![ChatMessageDto { role: "user".into(), content: "2+2=".into() }],
            stream: true,
            temperature: Some(0.0),
            top_p: None,
            top_k: None,
            max_tokens: Some(16),
            stop: None,
        };
        let resp = chat_completions_stream(state, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
```

- [ ] **Step 3: Build + test**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines.
Run: `cd src-tauri && cargo test --lib local_api::routes::chat_completions 2>&1 | tail -20`
Expected: 4 prior tests pass; `stream_emits_content_when_model_present` prints `[skip]` and passes (CI).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/local_api/routes.rs
git commit -m "feat(local_api): SSE streaming for /v1/chat/completions

Slice B Task 7. chat.completion.chunk deltas over an unbounded channel →
axum SSE, role-announce first chunk, finish_reason terminal chunk, [DONE]
sentinel. Pre-flight 503 when model absent so the stream never half-opens."
```

---

## Task 8: Wire the engine through `LocalApiService` + `main.rs` Stage 3

**Files:**
- Modify: `src-tauri/src/local_api/server.rs`
- Modify: `src-tauri/src/main.rs:461-477` (`[Stage 3]` LocalApiService construction)

- [ ] **Step 1: Add the engine field to `LocalApiService`**

In `server.rs`, extend the struct (lines 21-33) and `new` (lines 41-52):

```rust
pub struct LocalApiService {
    config: LocalApiConfig,
    embedder: Arc<dyn crate::memory_bucket_seal::Embedder>,
    /// In-process MiniCPM engine for `/v1/chat/completions` (Slice B).
    local_llm: Arc<crate::local_llm::LocalLlmEngine>,
    handle: RwLock<Option<JoinHandle<()>>>,
    is_running: AtomicBool,
    start_time: RwLock<Option<std::time::Instant>>,
}

impl LocalApiService {
    pub fn new(
        config: LocalApiConfig,
        embedder: Arc<dyn crate::memory_bucket_seal::Embedder>,
        local_llm: Arc<crate::local_llm::LocalLlmEngine>,
    ) -> Self {
        Self {
            config,
            embedder,
            local_llm,
            handle: RwLock::new(None),
            is_running: AtomicBool::new(false),
            start_time: RwLock::new(None),
        }
    }
    // listen_addr unchanged
```

- [ ] **Step 2: Pass the engine into `ApiState` at start**

In `server.rs` `start()` (lines 76-80), extend the `ApiState` construction:

```rust
        let state = Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: self.embedder.clone(),
            local_llm: self.local_llm.clone(),
        });
```

- [ ] **Step 3: Construct + inject the engine in `main.rs`**

In `main.rs`, replace the `LocalApiService::new(...)` block (lines 465-477). The engine needs the uClaw data dir — reuse whatever path the embedder factory uses (`build_embedder(cfg, data_dir)`); locate that `data_dir` binding in `main.rs` and reuse it here.

```rust
                    if memubot_config.local_api.enabled {
                        // Slice B: in-process MiniCPM engine, lazy-loaded on
                        // first /v1/chat/completions call (no 688 MB at startup).
                        let local_llm_engine = Arc::new(
                            uclaw_core::local_llm::LocalLlmEngine::new(data_dir.clone()),
                        );
                        let local_api_svc = Arc::new(
                            uclaw_core::local_api::LocalApiService::new(
                                memubot_config.local_api.clone(),
                                {
                                    let state_ref: tauri::State<'_, AppState> = app_handle.state();
                                    state_ref.bucket_seal_embedder.clone()
                                },
                                local_llm_engine,
                            )
                        );
                        service_manager.register(local_api_svc).await;
                        tracing::info!("[Stage 3] LocalApiService registered (+ local_llm engine)");
                    }
```

> **Implementer note:** Confirm the exact identifier for the data dir in scope at `main.rs:465`. If the embedder is built elsewhere with a different binding (e.g. `app_data_dir`, `uclaw_home`), use that exact name. Grep: `grep -n "build_embedder\|data_dir\|bucket_seal_embedder" src-tauri/src/main.rs`.

- [ ] **Step 4: Fix any other `LocalApiService::new` / `ApiState { .. }` construction sites**

Run: `cd src-tauri && grep -rn "LocalApiService::new\|ApiState {" src/`
Expected: only the three sites touched above (server.rs `new`, server.rs `start`, plus the embeddings test module's `make_state` and the new chat test helpers). Update the embeddings-test `make_state` (routes.rs ~line 408) to also set `local_llm`:

```rust
    fn make_state() -> Arc<ApiState> {
        let tmp = tempfile::tempdir().unwrap();
        Arc::new(ApiState {
            start_time: std::time::Instant::now(),
            embedder: Arc::new(InertEmbedder::default()),
            local_llm: Arc::new(crate::local_llm::LocalLlmEngine::new(tmp.keep())),
        })
    }
```

- [ ] **Step 5: Build + run the full local_api + local_llm test set**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no `error` lines.
Run: `cd src-tauri && cargo test --lib local_api 2>&1 | tail -20`
Expected: all embeddings + chat tests pass (gated ones skip).
Run: `cd src-tauri && cargo test --lib local_llm 2>&1 | tail -20`
Expected: all pass (gated smoke skips).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/local_api/server.rs src-tauri/src/main.rs src-tauri/src/local_api/routes.rs
git commit -m "feat(local_api): thread LocalLlmEngine through service + Stage 3 wiring

Slice B Task 8. LocalApiService owns the engine and injects it into ApiState;
main.rs constructs it in [Stage 3] with the uClaw data dir. No new background
service — the engine rides the existing LocalApiService."
```

---

## Task 9: Register the `local/minicpm5-1b` built-in provider

**Files:**
- Modify: `src-tauri/src/providers/registry.rs`

- [ ] **Step 1: Write the failing test**

Append to the `registry.rs` test module (near line 348):

```rust
    #[test]
    fn local_minicpm_provider_registered() {
        let providers = builtin_providers();
        let p = providers
            .iter()
            .find(|p| p.id == "local")
            .expect("local provider must be registered");
        assert_eq!(p.default_base_url, "http://localhost:7337/v1");
        assert!(matches!(p.default_api, ApiType::OpenAiCompletions));
        assert_eq!(p.auth_type, AuthType::None);
        assert!(p.supports_models, "must expose minicpm5-1b in the model dropdown");
    }
```

> **Implementer note:** Confirm `AuthType` has a `None` (or equivalent keyless) variant — grep `grep -n "enum AuthType" -A6 src/providers/types.rs`. If the keyless variant is named differently (e.g. `AuthType::ApiKey` with empty key is the only option), use the closest "no key required" representation and adjust both the test and the registry entry to match. The local endpoint needs no API key.

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib providers::registry::local_minicpm 2>&1 | tail -20`
Expected: FAIL — provider not found (or `AuthType::None` unresolved → fix per note).

- [ ] **Step 3: Add the registry entry**

In `builtin_providers()` (registry.rs), add at the **end** of the vec (the file says "add new ones at the end"), in a new local-models group:

```rust
    // ── Local (in-process, no key) ────────────────────────────────
    KnownProvider {
        id: "local".into(),
        display_name: "本地模型 (MiniCPM)".into(),
        auth_type: AuthType::None,
        default_base_url: "http://localhost:7337/v1".into(),
        default_api: ApiType::OpenAiCompletions,
        service_category: ServiceCategory::Api,
        geo_category: ProviderCategory::Domestic,
        supports_models: true,
    },
```

> If `ServiceCategory` has a more fitting variant for local/on-device, prefer it; `Api` is the safe default that keeps it visible in the model dropdown.

- [ ] **Step 4: Run to verify pass**

Run: `cd src-tauri && cargo test --lib providers::registry 2>&1 | tail -20`
Expected: all registry tests pass, including `local_minicpm_provider_registered`. The existing `test_list_builtin_providers_returns_all` count assertions may need their expected count bumped by 1 — update if it fails.

- [ ] **Step 5: Verify the model id is selectable**

The provider exposes models via the existing model-listing path; `minicpm5-1b` is the model id under provider `local`, so the role assignment value is `local/minicpm5-1b` (matching `response_model_name` in Task 6). No extra code — document this in the commit body.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/providers/registry.rs
git commit -m "feat(providers): register local/minicpm5-1b built-in provider

Slice B Task 9. New keyless 'local' provider at http://localhost:7337/v1
(OpenAiCompletions). The 模型分配 dropdown (wired live by Slice A) can now point
utility/summarizer roles at local/minicpm5-1b."
```

---

## Final verification (before opening the PR)

- [ ] **Full backend build:** `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` → empty.
- [ ] **All Slice B unit tests:** `cd src-tauri && cargo test --lib local_llm 2>&1 | tail -20` and `cargo test --lib local_api 2>&1 | tail -20` and `cargo test --lib providers::registry 2>&1 | tail -20` → all pass (gated model tests skip cleanly).
- [ ] **No clippy regressions on new modules:** `cd src-tauri && cargo clippy --lib 2>&1 | grep -E "local_llm|local_api" | head`.
- [ ] **GitNexus change check:** `gitnexus_detect_changes()` — confirm only expected symbols/flows changed.
- [ ] **Local model smoke (manual, if model available):** drop `MiniCPM5-1B-Q4_K_M.gguf` + `tokenizer.json` into `~/.uclaw/models/minicpm5-1b/`, run `cargo test --lib local_llm::engine::gated_engine_tests -- --nocapture`, confirm output contains `4`. Then `curl -N http://localhost:7337/v1/chat/completions -d '{"model":"local/minicpm5-1b","messages":[{"role":"user","content":"你好"}],"stream":true}'` and confirm Chinese tokens stream without mojibake (validates the UTF-8 TokenStream).

## PR body must call out (cross-cutting per CLAUDE.md)

- **Two-edit rule N/A here:** Slice B adds **no new Tauri commands** (the endpoint is HTTP, not IPC) and **no new background service** (the engine rides the existing `LocalApiService`). Slice C/D add the Tauri commands.
- **Adjacent edits that look like scope creep but aren't:** `ApiState` + `LocalApiService::new` signature changes ripple to `main.rs` and the embeddings test `make_state` — these are required, not scope creep.
- **Degradation follow-up to verify:** B returns 503 `model_not_ready`; confirm whether the role-routing call path (Slice A) performs a **request-time** cloud fall-back on a 503 from the local provider, or only config-time resolution. If only config-time, file a follow-up issue (this is the A↔B degradation seam).
- **Commits (bisectable) table:** one row per Task 1–9.

---

## Self-review notes (done at plan-authoring time)

- **Spec coverage:** mod.rs lazy-load ✓ (Task 5), engine.rs from_gguf/KV/sampling/stop/streaming ✓ (Tasks 3–4), chat_template pure unit-tested ✓ (Task 2), server.rs chat route ✓ (Tasks 6–7), Metal+CPU fallback ✓ (Task 4 `select_device`), serialized-behind-Mutex ✓ (Task 5), warmup 1-token ✓ (Task 5), degradation NotReady→503 ✓ (Tasks 5–6), deps with metal feature ✓ (Task 1), tokenizer.json external + cache-path contract to C ✓ (Task 1 + boundary section), provider registration ✓ (Task 9), gated model tests ✓ (Tasks 4/7).
- **Type consistency:** `GenParams`, `EngineError`, `FinishReason`, `Loaded`, `LocalLlmEngine`, `ChatCompletionsRequest`/`ChatMessageDto`/`StopField`, `ApiState.local_llm` used consistently across tasks. `render_chatml` + `ChatMessage` from chat_template reused in routes.
- **Open implementer confirmations (flagged inline):** exact `data_dir` binding name in main.rs (Task 8); `AuthType::None` variant name (Task 9); candle `load` single-vs-double read simplification (Task 4); builtin-provider count assertion bump (Task 9). Each has a grep command + fallback instruction.
</content>
</invoke>
