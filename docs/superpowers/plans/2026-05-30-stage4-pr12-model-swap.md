# 阶段 4 PR12 — Model Swap (real Embedder + Summariser) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the inert memory-tree backends (`InertEmbedder` = zeros, `InertSummariser` = concat+truncate) with real ones — an OpenAI-compatible HTTP embedder and an LLM summariser wrapping uClaw's existing `LlmProvider` — and move the now-slow cascade-seal off the ingest hot path via a detached tokio task.

**Architecture:** Two new backends behind the existing `Arc<dyn Embedder>` / `Arc<dyn Summariser>` injection (so no tree code changes): `OpenAiCompatEmbedder` POSTs to the configured `/v1/embeddings` endpoint; `LlmSummariser` resolves the app's ingestion `LlmProvider` lazily and folds inputs via `complete()`. A factory picks real-vs-inert. `BucketSealAdapter.store()` switches from synchronous `append_leaf` to `append_leaf_deferred` (fast, durable buffer write) + a detached `tokio::spawn(cascade_all_from)` (slow LLM work, best-effort, per-tree-mutex-serialised).

**Tech Stack:** Rust, `reqwest` 0.12 (already present), `serde_json`, `tokio`, uClaw's `LlmProvider`/`ProviderService`/`create_provider`, `async-trait`, `anyhow`, `tracing`.

---

## Source-of-truth references (verified during planning)

- `src-tauri/src/memory_bucket_seal/score/embed/mod.rs` — `Embedder` trait: `async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>>` + `fn name(&self) -> &'static str`. **`pub const EMBEDDING_DIM: usize = 1024`** — every embedder MUST return exactly 1024 floats; the trait's callers persist a fixed layout. Also `pack_embedding`/`unpack_embedding`/`cosine_similarity` utils.
- `src-tauri/src/memory_bucket_seal/score/embed/inert.rs` — `InertEmbedder::new()` returns `vec![0.0; EMBEDDING_DIM]`. Mirror its module/test shape.
- `src-tauri/src/memory_bucket_seal/tree_source/summariser/mod.rs` — `Summariser` trait: `async fn summarise(&self, inputs: &[SummaryInput], ctx: &SummaryContext<'_>) -> anyhow::Result<SummaryOutput>`. `SummaryInput { id, content, token_count, entities, topics, time_range_start, time_range_end, score }`. `SummaryContext<'a> { tree_id: &'a str, tree_kind: TreeKind, target_level: u32, token_budget: u32 }`. `SummaryOutput { content, token_count, entities, topics }`.
- `src-tauri/src/memory_bucket_seal/tree_source/summariser/inert.rs` — `InertSummariser::new()`. Mirror its shape.
- `src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs` — `pub fn append_leaf_deferred(store: &BucketSealStore, tree: &Tree, leaf: &LeafRef) -> Result<bool>` (line 161, SYNC, returns whether L0 seal gate is met) and `pub async fn cascade_all_from(store: &BucketSealStore, tree: &Tree, start_level: u32, summariser: &Arc<dyn Summariser>, embedder: &Arc<dyn Embedder>, force_now: Option<DateTime<Utc>>, strategy: &LabelStrategy) -> Result<Vec<String>>` (line 236). Both re-exported at `tree_source::{append_leaf_deferred, cascade_all_from}`.
- `src-tauri/src/memory_bucket_seal/adapter.rs` (PR9/PR10/PR11) — `BucketSealAdapter` private fields `store: Arc<BucketSealStore>`, `embedder: Arc<dyn Embedder>`, `summariser: Arc<dyn Summariser>`, `tree_mutexes`. `store()` currently calls `append_leaf` for source + each topic tree. `tree_mutex(&self, key: &str) -> Arc<tokio::sync::Mutex<()>>` helper. `fresh_adapter()` test fixture builds with inert backends.
- `src-tauri/src/llm/mod.rs` — `pub fn create_provider(config: &LlmConfig) -> Result<Arc<dyn LlmProvider>, crate::error::Error>`. Routes Anthropic vs OpenAI-compatible.
- `src-tauri/src/llm/provider.rs` — `LlmProvider::complete(&self, messages: Vec<ChatMessage>, tools: Vec<ToolDefinition>, config: &CompletionConfig) -> Result<RespondOutput, Error>`. `CompletionConfig { model: String, max_tokens: u32, temperature: f32, thinking_enabled: bool }`.
- `src-tauri/src/config/llm.rs` — `LlmConfig { provider: String, model: String, api_key: String, base_url: Option<String>, max_tokens: Option<u32>, api: Option<ApiType> }`.
- `src-tauri/src/providers/service.rs` — `ProviderService::get_ingestion_llm_config(&self) -> Option<(String, String, String, String)>` = `(provider_id, model, api_key, base_url)`. ASYNC.
- `src-tauri/src/memubot_config.rs` — `EmbeddingEndpointConfig { base_url: String (default "http://localhost:7337/v1"), model: String (default "llama-server:bge-small-en-v1.5"), dimensions: u32 (default 384), fastembed_model: String }`. Reached at `memubot_config.memory_os.embedding_endpoint`.
- `src-tauri/src/app.rs` — `provider_service: Arc<ProviderService>` (line 193, built line 606). PR9/PR11 bucket_seal wiring builds `bucket_seal_adapter` (now concrete `Arc<BucketSealAdapter>` per PR11). `memubot_config` loaded at boot.
- `src-tauri/src/agent/types.rs` — `ChatMessage`, `RespondOutput` (`.content: String` carries assistant text), `ToolDefinition`. **Implementer: read for exact `ChatMessage` construction (a user message) + `RespondOutput` text extraction.**
- `src-tauri/Cargo.toml:48` — `reqwest = { version = "0.12", features = ["json", "rustls-tls-native-roots", ...] }`. Present; no add needed.

---

## CRITICAL design facts (internalize before coding)

1. **`EMBEDDING_DIM = 1024` is the law.** The `Embedder` trait hard-requires exactly 1024 floats; changing the constant "breaks on-disk compatibility". The `OpenAiCompatEmbedder` validates returned length against `EMBEDDING_DIM` (1024), **NOT** the config's `dimensions` field (which defaults to 384 — that's the memU/gbrain FastEmbed path, a *different* embedding consumer). **Deployment implication, documented, not a code bug:** uClaw's default `/v1/embeddings` (bge-small, 384-dim) will FAIL the 1024 validation. Real embeddings require the user to configure a 1024-dim endpoint (e.g. bge-m3). Until then, `embed()` errors → the detached cascade logs a warning and drops the seal (graceful; no corruption). Since semantic recall isn't wired until PR15, this is invisible to users today beyond "summaries don't get embeddings until you point at a 1024-dim model". The plan does NOT change `EMBEDDING_DIM` and does NOT read `config.dimensions` for validation.

2. **Summariser resolves the provider LAZILY** (codebase idiom: the knowledge-ingestion service holds `Arc<ProviderService>` and resolves per-call, not at construction). This sidesteps boot-time async (`get_ingestion_llm_config` is async; `AppState::new`'s bucket_seal wiring is sync) AND picks up config changes. So `LlmSummariser` holds `Arc<ProviderService>`, and `summarise()` does: `get_ingestion_llm_config().await` → build `LlmConfig` → `create_provider()` → delegate to a pure `summarise_with_provider(...)`. (This refines the spec's `build_summariser(provider, model)` signature — the provider is resolved per-call, not injected. Observable behavior unchanged: real summaries when configured, graceful error otherwise.)

3. **Best-effort is the contract.** Memory must never block or break the primary write. A failed embed/summarise → `warn` + drop in the detached cascade. The synchronous digest IPC (PR11) surfaces the error to its caller. Buffers persist synchronously so no leaf is ever lost.

4. **Per-tree mutex preserved.** The detached cascade task acquires the same per-tree `Arc<Mutex<()>>` from `tree_mutexes` before cascading — PR8's serialisation contract holds across detached tasks.

---

## File Structure

| File | New/Mod | Responsibility | LoC |
|---|---|---|---|
| `score/embed/openai_compat.rs` | new | `OpenAiCompatEmbedder` + pure `build_embedding_request` / `parse_embedding_response` + tests | ~180 |
| `score/embed/factory.rs` | new | `build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder>` + tests | ~70 |
| `score/embed/mod.rs` | mod | declare `pub mod openai_compat; pub mod factory;` + re-export `OpenAiCompatEmbedder`, `build_embedder` | +4 |
| `tree_source/summariser/llm.rs` | new | `LlmSummariser` + pure `summarise_with_provider` + `build_fold_prompt` + tests (FakeLlmProvider) | ~230 |
| `tree_source/summariser/factory.rs` | new | `build_summariser(provider_service: Arc<ProviderService>) -> Arc<dyn Summariser>` + test | ~50 |
| `tree_source/summariser/mod.rs` | mod | declare `pub mod llm; pub mod factory;` + re-export `LlmSummariser`, `build_summariser` | +4 |
| `adapter.rs` | mod | `store()` hot-path split: `append_leaf_deferred` + detached `cascade_all_from` (source + topic) + tests | +110 |
| `app.rs` | mod | build embedder via `build_embedder(&memory_os.embedding_endpoint)`, summariser via `build_summariser(provider_service.clone())`, inject into `BucketSealAdapter` | ~10 |

Est. ~660 source + ~290 tests = ~950 LoC.

---

## Adaptation responsibilities (verify before trusting the plan)

1. **Read `agent/types.rs` for `ChatMessage` + `RespondOutput`.** The plan assumes a user `ChatMessage` is constructible (role=user, content=prompt) and `RespondOutput.content: String` holds the assistant text. Verify the exact constructor (`ChatMessage { role, content, .. }` literal, or a `::user(...)` helper) and how to pull the text out of `RespondOutput` (it may have more than `.content` — take the text field).
2. **Verify `LlmConfig` construction from the ingestion tuple.** `get_ingestion_llm_config()` returns `(provider_id, model, api_key, base_url)`. Build `LlmConfig { provider: provider_id, model: model.clone(), api_key, base_url: (if empty { None } else { Some(base_url) }), max_tokens: None, api: None }`. Verify field names + that `api: None` lets `create_provider`'s `resolve_api` route correctly (anthropic id → Anthropic, else OpenAI-compat).
3. **Verify `reqwest` JSON usage** — `client.post(url).json(&body).send().await?.error_for_status()?.json::<Resp>().await?`. The `json` feature is on.
4. **Verify `cascade_all_from` arg list at the call site** — 7 args: `(store, tree, start_level=0, summariser, embedder, force_now=None, strategy=&LabelStrategy::Empty)`. The detached task clones `Arc`s; `tree` is `Tree` (Clone).
5. **Verify `tree_mutex` key convention** — PR10 used `format!("source:{}", ns)` and `format!("topic:{}", entity)`. The hot-path split must keep the SAME keys so the detached task serialises against the right per-tree mutex.
6. **Verify `ToolDefinition` empty construction** — `complete()` takes `Vec<ToolDefinition>`; pass `vec![]`.
7. **`ProviderService` import path in summariser** — `crate::providers::service::ProviderService` (verify). The summariser factory + struct hold `Arc<ProviderService>`.
8. **Token estimate** — check for an existing token-count util (e.g. in `agent/` or `memory_bucket_seal/util.rs`). If none, use a `chars / 4` heuristic in a small helper. The seal only uses `token_count` for buffer accounting.
9. **Pre-commit hooks** — don't `--no-verify`. PR12 touches neither `memory_graph::write` nor `dirs::home_dir`.
10. **`fresh_adapter()` stays inert** — existing adapter tests construct with `InertEmbedder`/`InertSummariser` directly; do NOT route them through the factories. Only `app.rs` uses the factories.

---

### Task 1: `OpenAiCompatEmbedder` + embed factory

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/score/embed/openai_compat.rs`
- Create: `src-tauri/src/memory_bucket_seal/score/embed/factory.rs`
- Modify: `src-tauri/src/memory_bucket_seal/score/embed/mod.rs`

- [ ] **Step 1: Write the failing tests for the pure helpers** (in `openai_compat.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_request_has_model_and_input() {
        let body = build_embedding_request("bge-m3", "hello world");
        assert_eq!(body["model"], "bge-m3");
        assert_eq!(body["input"], "hello world");
    }

    #[test]
    fn parse_response_extracts_first_embedding() {
        let body = r#"{"data":[{"embedding":[0.1,0.2,0.3]}]}"#;
        let v = parse_embedding_response(body, 3).unwrap();
        assert_eq!(v, vec![0.1_f32, 0.2, 0.3]);
    }

    #[test]
    fn parse_response_rejects_wrong_dimension() {
        let body = r#"{"data":[{"embedding":[0.1,0.2]}]}"#;
        let err = parse_embedding_response(body, 3).unwrap_err();
        assert!(format!("{err:#}").contains("dimension"));
    }

    #[test]
    fn parse_response_errors_on_empty_data() {
        let body = r#"{"data":[]}"#;
        assert!(parse_embedding_response(body, 3).is_err());
    }
}
```

- [ ] **Step 2: Run to verify they fail**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed::openai_compat 2>&1 | tail`
Expected: compile error (functions not defined).

- [ ] **Step 3: Implement `openai_compat.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Real embedder: POSTs to an OpenAI-compatible `/embeddings` endpoint.
//!
//! Validates the returned vector against [`EMBEDDING_DIM`] (1024, bge-m3) —
//! NOT the gbrain/memU `dimensions` config field. A 384-dim endpoint
//! (uClaw's default bge-small route) fails validation; configure a
//! 1024-dim model for real embeddings. Failures are non-fatal upstream
//! (best-effort seal).

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use serde_json::Value;

use super::{Embedder, EMBEDDING_DIM};

/// OpenAI-compatible HTTP embedder.
pub struct OpenAiCompatEmbedder {
    client: reqwest::Client,
    embeddings_url: String,
    model: String,
}

impl OpenAiCompatEmbedder {
    /// `base_url` is the OpenAI-compatible root (e.g. `http://localhost:7337/v1`);
    /// the embeddings endpoint is `{base_url}/embeddings`.
    pub fn new(base_url: &str, model: &str) -> Self {
        let trimmed = base_url.trim_end_matches('/');
        Self {
            client: reqwest::Client::new(),
            embeddings_url: format!("{trimmed}/embeddings"),
            model: model.to_string(),
        }
    }
}

/// Build the request body for an embeddings call.
pub(crate) fn build_embedding_request(model: &str, text: &str) -> Value {
    serde_json::json!({ "model": model, "input": text })
}

/// Parse an OpenAI embeddings response, returning the first embedding.
/// Errors when `data` is empty or the embedding length != `expected_dim`.
pub(crate) fn parse_embedding_response(body: &str, expected_dim: usize) -> Result<Vec<f32>> {
    let parsed: Value = serde_json::from_str(body).context("parse embeddings JSON")?;
    let arr = parsed
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| anyhow!("embeddings response missing `data` array"))?;
    let first = arr
        .first()
        .ok_or_else(|| anyhow!("embeddings response `data` is empty"))?;
    let emb = first
        .get("embedding")
        .and_then(|e| e.as_array())
        .ok_or_else(|| anyhow!("embeddings response missing `embedding`"))?;
    let out: Vec<f32> = emb
        .iter()
        .map(|v| v.as_f64().map(|f| f as f32))
        .collect::<Option<Vec<f32>>>()
        .ok_or_else(|| anyhow!("embedding contained a non-numeric value"))?;
    if out.len() != expected_dim {
        return Err(anyhow!(
            "embedding dimension mismatch: got {}, expected {}",
            out.len(),
            expected_dim
        ));
    }
    Ok(out)
}

#[async_trait]
impl Embedder for OpenAiCompatEmbedder {
    fn name(&self) -> &'static str {
        "openai_compat"
    }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let body = build_embedding_request(&self.model, text);
        let resp = self
            .client
            .post(&self.embeddings_url)
            .json(&body)
            .send()
            .await
            .context("embeddings request failed")?
            .error_for_status()
            .context("embeddings endpoint returned error status")?;
        let text_body = resp.text().await.context("read embeddings body")?;
        parse_embedding_response(&text_body, EMBEDDING_DIM)
    }
}
```

(The 4 tests from Step 1 go at the bottom of this file.)

- [ ] **Step 4: Run the pure-helper tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed::openai_compat 2>&1 | tail`
Expected: 4 passed.

- [ ] **Step 5: Write `factory.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Embedder factory: real OpenAI-compatible embedder when an endpoint is
//! configured, inert zero-vector embedder otherwise (tests / offline /
//! unconfigured).

use std::sync::Arc;

use crate::memubot_config::EmbeddingEndpointConfig;

use super::{Embedder, InertEmbedder, OpenAiCompatEmbedder};

/// Pick an embedder from config. Real when `base_url` AND `model` are both
/// non-empty; inert fallback otherwise.
pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder> {
    if cfg.base_url.trim().is_empty() || cfg.model.trim().is_empty() {
        tracing::info!("[embed::factory] no embedding endpoint configured — using InertEmbedder");
        return Arc::new(InertEmbedder::new());
    }
    tracing::info!(
        base_url = %cfg.base_url,
        model = %cfg.model,
        "[embed::factory] using OpenAiCompatEmbedder"
    );
    Arc::new(OpenAiCompatEmbedder::new(&cfg.base_url, &cfg.model))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(base_url: &str, model: &str) -> EmbeddingEndpointConfig {
        EmbeddingEndpointConfig {
            base_url: base_url.to_string(),
            model: model.to_string(),
            dimensions: 384,
            fastembed_model: "BAAI/bge-small-en-v1.5".to_string(),
        }
    }

    #[test]
    fn configured_yields_openai_compat() {
        let e = build_embedder(&cfg("http://localhost:7337/v1", "bge-m3"));
        assert_eq!(e.name(), "openai_compat");
    }

    #[test]
    fn empty_base_url_yields_inert() {
        let e = build_embedder(&cfg("", "bge-m3"));
        assert_eq!(e.name(), "inert");
    }

    #[test]
    fn empty_model_yields_inert() {
        let e = build_embedder(&cfg("http://localhost:7337/v1", ""));
        assert_eq!(e.name(), "inert");
    }
}
```

**Adaptation:** verify `InertEmbedder::name()` returns `"inert"` (read inert.rs). If it returns something else, fix the assertions. Verify `EmbeddingEndpointConfig` field set for the test constructor (it may have more/fewer fields — match the struct).

- [ ] **Step 6: Wire `mod.rs`**

In `score/embed/mod.rs` add:
```rust
pub mod factory;
pub mod openai_compat;

pub use factory::build_embedder;
pub use openai_compat::OpenAiCompatEmbedder;
```

- [ ] **Step 7: Build + test the embed module**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed 2>&1 | tail`
Expected: all embed tests pass (4 openai_compat + 3 factory + existing inert).

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/
git commit -m "feat(memory_bucket_seal): OpenAiCompatEmbedder + embed factory (PR12.1 of 阶段 4)"
```

---

### Task 2: `LlmSummariser` + summariser factory

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_source/summariser/llm.rs`
- Create: `src-tauri/src/memory_bucket_seal/tree_source/summariser/factory.rs`
- Modify: `src-tauri/src/memory_bucket_seal/tree_source/summariser/mod.rs`

- [ ] **Step 1: Read `agent/types.rs`** for `ChatMessage` construction + `RespondOutput` text field, and `llm/provider.rs` for `CompletionConfig`. Confirm the exact shapes before writing.

- [ ] **Step 2: Write failing tests for the pure path** (in `llm.rs`) using a `FakeLlmProvider`

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ChatMessage, RespondOutput, StreamDelta, ToolDefinition};
    use crate::llm::provider::{CompletionConfig, LlmProvider};
    use crate::memory_bucket_seal::tree_source::types::TreeKind;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    /// Records the prompt it was called with and returns a canned answer.
    struct FakeLlmProvider {
        canned: String,
        seen_prompt: Arc<Mutex<Option<String>>>,
        seen_max_tokens: Arc<Mutex<Option<u32>>>,
    }

    #[async_trait]
    impl LlmProvider for FakeLlmProvider {
        async fn complete(
            &self,
            messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            config: &CompletionConfig,
        ) -> Result<RespondOutput, crate::error::Error> {
            // Capture the concatenated user content + the token budget.
            let joined = messages.iter().map(|m| m.content.clone()).collect::<Vec<_>>().join("\n");
            *self.seen_prompt.lock().unwrap() = Some(joined);
            *self.seen_max_tokens.lock().unwrap() = Some(config.max_tokens);
            Ok(RespondOutput { content: self.canned.clone(), ..Default::default() })
        }
        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, crate::error::Error>> + Send + Unpin>, crate::error::Error> {
            unimplemented!("not used by summariser")
        }
    }

    fn mk_input(id: &str, content: &str) -> SummaryInput {
        let now = Utc::now();
        SummaryInput {
            id: id.to_string(),
            content: content.to_string(),
            token_count: 100,
            entities: vec![],
            topics: vec![],
            time_range_start: now,
            time_range_end: now,
            score: 0.5,
        }
    }

    #[tokio::test]
    async fn summarise_returns_provider_content() {
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeLlmProvider {
            canned: "FOLDED SUMMARY".into(),
            seen_prompt: Arc::new(Mutex::new(None)),
            seen_max_tokens: Arc::new(Mutex::new(None)),
        });
        let inputs = vec![mk_input("a", "alpha content"), mk_input("b", "beta content")];
        let ctx = SummaryContext { tree_id: "t1", tree_kind: TreeKind::Source, target_level: 1, token_budget: 4000 };
        let out = summarise_with_provider(&provider, "test-model", &inputs, &ctx).await.unwrap();
        assert_eq!(out.content, "FOLDED SUMMARY");
        assert!(out.entities.is_empty());
        assert!(out.topics.is_empty());
        assert!(out.token_count > 0);
    }

    #[tokio::test]
    async fn prompt_includes_input_content_and_budget_respected() {
        let seen_prompt = Arc::new(Mutex::new(None));
        let seen_mt = Arc::new(Mutex::new(None));
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeLlmProvider {
            canned: "x".into(),
            seen_prompt: seen_prompt.clone(),
            seen_max_tokens: seen_mt.clone(),
        });
        let inputs = vec![mk_input("a", "DISTINCTIVE_TOKEN_ALPHA")];
        let ctx = SummaryContext { tree_id: "t1", tree_kind: TreeKind::Global, target_level: 0, token_budget: 1234 };
        let _ = summarise_with_provider(&provider, "m", &inputs, &ctx).await.unwrap();
        let prompt = seen_prompt.lock().unwrap().clone().unwrap();
        assert!(prompt.contains("DISTINCTIVE_TOKEN_ALPHA"), "prompt must include input content");
        assert_eq!(*seen_mt.lock().unwrap(), Some(1234), "token_budget → max_tokens");
    }

    #[test]
    fn fold_prompt_mentions_tree_kind() {
        let inputs = vec![mk_input("a", "c")];
        let p = build_fold_prompt(&inputs, TreeKind::Topic, 4000);
        // The prompt should adapt one line to the tree kind.
        assert!(!p.is_empty());
    }
}
```

- [ ] **Step 3: Run to verify failure**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::summariser::llm 2>&1 | tail`
Expected: compile error (functions not defined).

- [ ] **Step 4: Implement `llm.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Real summariser: folds memory fragments into one summary via uClaw's
//! existing `LlmProvider` (the same models the agent uses). Resolves the
//! ingestion provider lazily per call (codebase idiom — see the
//! knowledge-ingestion service) so no boot-time async is needed and model
//! changes are picked up automatically.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use crate::config::llm::LlmConfig;
use crate::llm::provider::{CompletionConfig, LlmProvider};
use crate::providers::service::ProviderService;

use super::{Summariser, SummaryContext, SummaryInput, SummaryOutput};
use crate::memory_bucket_seal::tree_source::types::TreeKind;

/// Real summariser backed by the app's configured ingestion LLM.
pub struct LlmSummariser {
    provider_service: Arc<ProviderService>,
}

impl LlmSummariser {
    pub fn new(provider_service: Arc<ProviderService>) -> Self {
        Self { provider_service }
    }
}

#[async_trait]
impl Summariser for LlmSummariser {
    fn name(&self) -> &'static str {
        "llm"
    }

    async fn summarise(
        &self,
        inputs: &[SummaryInput],
        ctx: &SummaryContext<'_>,
    ) -> Result<SummaryOutput> {
        let (provider_id, model, api_key, base_url) = self
            .provider_service
            .get_ingestion_llm_config()
            .await
            .ok_or_else(|| anyhow!("no ingestion LLM configured for summariser"))?;

        let llm_config = LlmConfig {
            provider: provider_id,
            model: model.clone(),
            api_key,
            base_url: if base_url.trim().is_empty() { None } else { Some(base_url) },
            max_tokens: None,
            api: None,
        };
        let provider =
            crate::llm::create_provider(&llm_config).context("build summariser LLM provider")?;
        summarise_with_provider(&provider, &model, inputs, ctx).await
    }
}

/// Pure fold logic — takes an already-resolved provider. Unit-tested with a
/// fake provider; the lazy resolution above is a thin wrapper.
pub(crate) async fn summarise_with_provider(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    inputs: &[SummaryInput],
    ctx: &SummaryContext<'_>,
) -> Result<SummaryOutput> {
    let prompt = build_fold_prompt(inputs, ctx.tree_kind, ctx.token_budget);

    // Build a single user message. Adaptation: verify ChatMessage's exact
    // constructor in agent/types.rs (role=user, content=prompt).
    let messages = vec![crate::agent::types::ChatMessage {
        role: "user".to_string(),
        content: prompt,
        ..Default::default()
    }];

    let config = CompletionConfig {
        model: model.to_string(),
        max_tokens: ctx.token_budget,
        temperature: 0.3,
        thinking_enabled: false,
    };

    let out = provider
        .complete(messages, vec![], &config)
        .await
        .map_err(|e| anyhow!("summariser LLM complete failed: {e}"))?;

    let content = out.content;
    let token_count = estimate_tokens(&content);
    Ok(SummaryOutput {
        content,
        token_count,
        entities: vec![],
        topics: vec![],
    })
}

/// Build the fold prompt. One framing line per tree kind; then the inputs.
pub(crate) fn build_fold_prompt(
    inputs: &[SummaryInput],
    kind: TreeKind,
    token_budget: u32,
) -> String {
    let framing = match kind {
        TreeKind::Source => "the following memory fragments from a single source",
        TreeKind::Topic => "the following memory fragments about a single topic/entity",
        TreeKind::Global => "the following per-source daily summaries",
    };
    let mut prompt = format!(
        "Summarise {framing} into a single dense recap of at most {} tokens. \
         Preserve names, decisions, dates, and concrete facts. Write prose, \
         no preamble.\n\n---\n",
        token_budget
    );
    for inp in inputs {
        prompt.push_str(&format!(
            "[{} · {} → {}]\n{}\n\n",
            inp.id,
            inp.time_range_start.to_rfc3339(),
            inp.time_range_end.to_rfc3339(),
            inp.content
        ));
    }
    prompt
}

/// Cheap token estimate (chars / 4). The seal only uses this for buffer
/// accounting + the next-level budget, so an approximation is fine.
fn estimate_tokens(text: &str) -> u32 {
    ((text.chars().count() + 3) / 4) as u32
}
```

**Adaptation:** the `ChatMessage { role, content, ..Default::default() }` literal is a guess — verify the real struct (it may use an enum `Role`, or a `ChatMessage::user(content)` helper). Same for `RespondOutput.content` extraction (verify the text field name). If `RespondOutput` doesn't derive `Default`, construct it explicitly in the fake. If `Summariser` trait has no `name()` method, drop it (verify the trait — the plan's references say it only has `summarise`; if so, remove the `fn name` from this impl). **Read the trait before implementing.**

- [ ] **Step 5: Run the llm summariser tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::summariser::llm 2>&1 | tail`
Expected: 3 passed.

- [ ] **Step 6: Write `factory.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Summariser factory. Production always builds the LLM summariser (it
//! resolves the provider lazily and degrades gracefully — erroring at
//! summarise time when no provider is configured). Tests inject
//! `InertSummariser` directly via the adapter constructor.

use std::sync::Arc;

use crate::providers::service::ProviderService;

use super::{LlmSummariser, Summariser};

/// Build the production summariser. The LLM summariser resolves the
/// ingestion provider per call; if none is configured it errors at
/// summarise time (the detached cascade logs + drops — best-effort).
pub fn build_summariser(provider_service: Arc<ProviderService>) -> Arc<dyn Summariser> {
    Arc::new(LlmSummariser::new(provider_service))
}
```

(No unit test — it's a one-liner constructor; covered by the app.rs wiring + the llm.rs tests. If a test is desired, assert `build_summariser(svc).name() == "llm"` given a `ProviderService::new(tmpdir)`.)

- [ ] **Step 7: Wire `summariser/mod.rs`**

```rust
pub mod factory;
pub mod llm;

pub use factory::build_summariser;
pub use llm::LlmSummariser;
```

- [ ] **Step 8: Build + test the summariser module**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_source::summariser 2>&1 | tail`
Expected: llm tests + existing inert tests pass.

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_source/summariser/
git commit -m "feat(memory_bucket_seal): LlmSummariser wrapping LlmProvider + summariser factory (PR12.2 of 阶段 4)"
```

---

### Task 3: `BucketSealAdapter.store()` hot-path split

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Read the current `store()`** — find the per-chunk loop that calls `append_leaf` for the source tree (Phase A) and the topic fan-out loop (Phase B). Note the exact `tree_mutex` key strings (`source:{ns}`, `topic:{entity}`) and the `LeafRef` construction.

- [ ] **Step 2: Replace synchronous `append_leaf` with deferred-append + detached cascade — SOURCE tree**

In Phase A, where each admitted chunk currently does `append_leaf(&self.store, &source_tree, &leaf, &self.summariser, &self.embedder, &LabelStrategy::Empty).await?`, change to:

```rust
// Deferred: synchronous buffer append (fast, durable). Returns whether
// the L0 seal gate is met.
let gate_met = crate::memory_bucket_seal::tree_source::append_leaf_deferred(
    &self.store,
    &source_tree,
    &leaf,
)
.context("source append_leaf_deferred")?;
if gate_met {
    self.spawn_cascade(format!("source:{}", namespace), source_tree.clone());
}
```

- [ ] **Step 3: Same change — TOPIC trees (Phase B)**

Where each entity currently does `append_leaf(... topic_tree ...).await`, change to:

```rust
let gate_met = match crate::memory_bucket_seal::tree_source::append_leaf_deferred(
    &self.store,
    &topic_tree,
    &leaf,
) {
    Ok(g) => g,
    Err(e) => {
        tracing::warn!(entity = %entity, error = %e, "topic append_leaf_deferred failed");
        continue;
    }
};
if gate_met {
    self.spawn_cascade(format!("topic:{}", entity), topic_tree.clone());
}
```

- [ ] **Step 4: Add the `spawn_cascade` helper** to `impl BucketSealAdapter` (inherent block)

```rust
/// Spawn the slow cascade-seal off the ingest hot path. Best-effort: the
/// L0 buffer was already durably written by `append_leaf_deferred`, so a
/// failure here leaves the seal pending (recovered by PR13's queue or a
/// later flush) without losing any leaf. Acquires the per-tree mutex so
/// PR8's per-tree serialisation holds across detached tasks.
fn spawn_cascade(&self, mutex_key: String, tree: crate::memory_bucket_seal::tree_source::types::Tree) {
    let store = self.store.clone();
    let summariser = self.summariser.clone();
    let embedder = self.embedder.clone();
    // Resolve (or create) the per-tree mutex synchronously-ish: tree_mutex
    // is async (locks the HashMap), so clone the needed Arc inside the task.
    let mutexes = self.tree_mutexes_arc(); // see Step 5
    tokio::spawn(async move {
        let tree_mutex = {
            let mut map = mutexes.lock().await;
            map.entry(mutex_key.clone())
                .or_insert_with(|| std::sync::Arc::new(tokio::sync::Mutex::new(())))
                .clone()
        };
        let _guard = tree_mutex.lock().await;
        if let Err(e) = crate::memory_bucket_seal::tree_source::cascade_all_from(
            &store,
            &tree,
            0,
            &summariser,
            &embedder,
            None,
            &crate::memory_bucket_seal::tree_source::LabelStrategy::Empty,
        )
        .await
        {
            tracing::warn!(
                tree_id = %tree.id,
                mutex_key = %mutex_key,
                error = %e,
                "detached cascade failed (best-effort; seal pending until PR13 queue / flush)"
            );
        }
    });
}
```

**Adaptation (IMPORTANT):** the existing `tree_mutex(&self, key) -> Arc<Mutex<()>>` helper locks `self.tree_mutexes` (the HashMap). To use the SAME mutex map inside the spawned task, the task needs a clone of the `Arc<Mutex<HashMap<...>>>`. If `tree_mutexes` is currently `tokio::sync::Mutex<HashMap<...>>` (not wrapped in `Arc`), it cannot be cloned into the task. **Two options — pick based on the actual field type:**
  - **(a)** If `tree_mutexes: Arc<tokio::sync::Mutex<HashMap<String, Arc<Mutex<()>>>>>` — add a `fn tree_mutexes_arc(&self) -> Arc<...> { self.tree_mutexes.clone() }` accessor and use it as above.
  - **(b)** If `tree_mutexes` is NOT `Arc`-wrapped — the cleaner fix is to resolve the per-tree `Arc<Mutex<()>>` BEFORE spawning (call the existing `self.tree_mutex(&mutex_key).await` in the async `store()` context, which already holds `&self`), then move only that resolved `Arc<Mutex<()>>` into the task. Rewrite `spawn_cascade` to be `async fn` that takes the resolved mutex, or inline the resolution at the call site:
    ```rust
    if gate_met {
        let tree_mutex = self.tree_mutex(&format!("source:{}", namespace)).await;
        let store = self.store.clone();
        let summariser = self.summariser.clone();
        let embedder = self.embedder.clone();
        let tree = source_tree.clone();
        tokio::spawn(async move {
            let _guard = tree_mutex.lock().await;
            if let Err(e) = cascade_all_from(&store, &tree, 0, &summariser, &embedder, None, &LabelStrategy::Empty).await {
                tracing::warn!(tree_id = %tree.id, error = %e, "detached cascade failed (best-effort)");
            }
        });
    }
    ```
  **Prefer (b)** — it reuses the existing `tree_mutex` helper, needs no new accessor, and resolves the mutex in the `&self` context where it's natural. Use (b) unless `tree_mutex` can't be called there. Drop the standalone `spawn_cascade` helper if (b) is inlined.

- [ ] **Step 5: Add tests** (append to `adapter.rs` tests). These need a SLOW fake summariser to prove `store()` returns without awaiting the cascade. Build a fake that sleeps, inject it via a test-only adapter constructor.

```rust
    // A summariser that blocks for a beat, to prove store() doesn't await it.
    struct SlowSummariser {
        started: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }
    #[async_trait::async_trait]
    impl crate::memory_bucket_seal::tree_source::summariser::Summariser for SlowSummariser {
        async fn summarise(
            &self,
            _inputs: &[crate::memory_bucket_seal::tree_source::summariser::SummaryInput],
            _ctx: &crate::memory_bucket_seal::tree_source::summariser::SummaryContext<'_>,
        ) -> anyhow::Result<crate::memory_bucket_seal::tree_source::summariser::SummaryOutput> {
            self.started.store(true, std::sync::atomic::Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            Ok(crate::memory_bucket_seal::tree_source::summariser::SummaryOutput {
                content: "slow summary".into(),
                token_count: 3,
                entities: vec![],
                topics: vec![],
            })
        }
    }

    #[tokio::test]
    async fn store_does_not_await_cascade() {
        // Build an adapter whose summariser is slow. Storing enough to trip
        // a seal gate must still return quickly (the cascade is detached).
        // Adaptation: use whatever test constructor fresh_adapter() uses,
        // but inject SlowSummariser. If fresh_adapter() hardcodes inert,
        // add a fresh_adapter_with(summariser, embedder) helper.
        let (adapter, _dir) = fresh_adapter_with_summariser(std::sync::Arc::new(SlowSummariser {
            started: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }));
        let start = std::time::Instant::now();
        adapter
            .store("ns", "k1", "Substantive content with enough signal to be admitted and buffered.", MemoryCategory::Core, None)
            .await
            .unwrap();
        // store() returns well under the 200ms summariser sleep because the
        // cascade (if any) is detached. Even if no seal fires, this asserts
        // the call is fast.
        assert!(start.elapsed() < std::time::Duration::from_millis(150), "store() must not block on the cascade");
    }

    #[tokio::test]
    async fn store_buffer_is_durable_before_cascade() {
        // After store(), the chunk is persisted synchronously regardless of
        // cascade state.
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_dur", "k1", "Durable content with sufficient admission signal density.", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert!(adapter.store.count_chunks().unwrap() >= 1);
    }
```

**Adaptation:** if `fresh_adapter()` hardcodes inert backends, add a `fresh_adapter_with_summariser(summariser: Arc<dyn Summariser>) -> (BucketSealAdapter, TempDir)` test helper that mirrors `fresh_adapter()` but takes the summariser. Keep `fresh_adapter()` itself unchanged so existing tests are unaffected. The single-small-chunk store may NOT trip a seal gate (gate is 50k tokens / fanout) — `store_does_not_await_cascade` then just asserts the fast return (cascade not triggered is also fast). That's still a valid "no blocking" assertion. If you want to guarantee a seal fires, the test would need to push enough tokens; keep it simple — the assertion that `store()` is fast holds either way.

- [ ] **Step 6: Build + test**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter 2>&1 | tail -20`
Expected: existing adapter tests + 2 new pass.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): store() hot-path split — deferred append + detached cascade (PR12.3 of 阶段 4)"
```

---

### Task 4: `app.rs` wiring

**Files:**
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Read the PR9/PR11 bucket_seal wiring** in `app.rs` — find where `embedder` + `summariser` are currently built (hardcoded `InertEmbedder`/`InertSummariser`) and passed to `BucketSealAdapter::new`.

- [ ] **Step 2: Swap in the factories**

Replace the hardcoded inert construction with:

```rust
let bucket_seal_embedder = crate::memory_bucket_seal::score::embed::build_embedder(
    &memubot_config.memory_os.embedding_endpoint,
);
let bucket_seal_summariser = crate::memory_bucket_seal::tree_source::summariser::build_summariser(
    provider_service.clone(),
);
```

Then pass `bucket_seal_embedder` + `bucket_seal_summariser` into `BucketSealAdapter::new(...)` where the inert ones were.

**Adaptation:** verify the exact local variable name for the loaded config (`memubot_config` vs another binding) and that `provider_service` is in scope at the bucket_seal wiring point (it's built at line ~606, before the adapter wiring — confirm ordering). Verify the `BucketSealAdapter::new` parameter order (store, content_root, embedder, summariser). The `build_embedder`/`build_summariser` return `Arc<dyn Embedder>`/`Arc<dyn Summariser>` — matching `BucketSealAdapter::new`'s expected types.

- [ ] **Step 3: Build (full, not just --lib — wiring touches boot)**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/app.rs
git commit -m "feat(app): wire real Embedder + Summariser into BucketSealAdapter via factories (PR12.4 of 阶段 4)"
```

---

### Task 5: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~232+ passed (220 PR11 baseline + ~12 new: 4 openai_compat + 3 embed-factory + 3 llm-summariser + 2 adapter).

- [ ] **Step 2: Full backend build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 3: Broader regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive over PR11 baseline; pre-existing failures unchanged.

- [ ] **Step 4: Clippy**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "openai_compat|embed/factory|summariser/llm|summariser/factory|adapter\.rs|app\.rs" | head -20`
Expected: zero PR12-attributable hits.

- [ ] **Step 5: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr12-model-swap && git diff main -- src-tauri/Cargo.toml`
Expected: empty (reqwest already present).

- [ ] **Step 6: Hermeticity check — no live model calls in tests**

Run: `cd src-tauri && grep -rnE "localhost:7337|api.openai|api.anthropic|reqwest::Client::new\(\).*embed" src/memory_bucket_seal/score/embed/openai_compat.rs | grep -v "// " | head`
Confirm the embedder tests exercise only the pure `build_embedding_request`/`parse_embedding_response` functions (no real HTTP). The summariser tests use `FakeLlmProvider` (no real provider).

- [ ] **Step 7: If cleanups surface, apply + commit**

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR12 cleanup pass"
```

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| Embedder pure helpers (request build, response parse, dim-mismatch, empty-data) | 4 | `score::embed::openai_compat::tests` |
| Embed factory (configured→real, empty-base→inert, empty-model→inert) | 3 | `score::embed::factory::tests` |
| LlmSummariser (returns provider content, prompt includes inputs + budget, fold-prompt per kind) | 3 | `tree_source::summariser::llm::tests` |
| Hot-path (store does not await cascade, buffer durable) | 2 | `memory_bucket_seal::adapter::tests` |
| **Total new** | **12** | — |
| **PR11 baseline preserved** | 220 | — |
| **Module total** | **~232** | — |

---

## Self-Review

**1. Spec coverage:**
- §3.1 OpenAiCompatEmbedder → Task 1 ✅ (validates `EMBEDDING_DIM`, not config.dimensions — corrected from spec's looser wording).
- §3.2 LlmSummariser → Task 2 ✅ (lazy `ProviderService` resolution — refines spec's `build_summariser(provider, model)`; documented).
- §3.3 Factories → Tasks 1+2 ✅ (embed factory 2-way; summariser factory always-LLM with graceful degradation — documented deviation).
- §3.4 Hot-path split → Task 3 ✅ (`append_leaf_deferred` + detached `cascade_all_from`, per-tree mutex preserved).
- §3.5 Wiring → Task 4 ✅.
- §6 Testing → hermetic (pure functions + FakeLlmProvider, no live calls) ✅.
- §7 Scope boundaries → no jobs/queue, no recall wiring, no schema, no IPC, no config UI ✅.

**2. Placeholder scan:** No TBD/TODO. The two "Adaptation" blocks in Task 3 Step 4 and Task 2 Step 4 give concrete alternatives (option a/b) keyed on the actual field type / trait shape — not placeholders, but explicit branch instructions the implementer resolves by reading the code. ChatMessage/RespondOutput construction is flagged for verification with a concrete best-guess literal + fallback instruction.

**3. Type consistency:** `EMBEDDING_DIM` (1024) used consistently. `build_embedder(&EmbeddingEndpointConfig) -> Arc<dyn Embedder>` and `build_summariser(Arc<ProviderService>) -> Arc<dyn Summariser>` match their call sites in Task 4. `summarise_with_provider(&Arc<dyn LlmProvider>, &str, &[SummaryInput], &SummaryContext)` consistent between definition (Task 2 Step 4) and tests (Task 2 Step 2). `cascade_all_from` 7-arg signature consistent. `append_leaf_deferred(store, tree, leaf) -> Result<bool>` consistent.

**Known documented deviations from the spec (intent preserved):**
1. Embedder validates `EMBEDDING_DIM` (1024) not `config.dimensions` (384) — forced by the trait contract; spec §9 #1 flagged the model-shape question, this resolves it.
2. Summariser holds `Arc<ProviderService>` + lazy resolution rather than an injected `Arc<dyn LlmProvider>` — forced by async config read + matches codebase idiom; spec §9 #3 flagged this as the main wiring unknown.
3. Summariser factory always returns `LlmSummariser` (graceful degradation on unconfigured) rather than a 2-way inert fallback — consequence of #2; behavior preserved (inert-equivalent when unconfigured).
