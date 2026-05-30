# 阶段 4 PR12 — Model Swap (real Embedder + Summariser) Design Spec

**Status:** Approved design — pending user review gate before plan.
**Date:** 2026-05-30
**Position in 阶段 4 sequence:** PR12 of 15. Follows PR11 (tree_global). Precedes PR13 (async job queue).

---

## 1. Goal

Replace the inert stubs (`InertSummariser` = concat+truncate, `InertEmbedder` = zeros) that have backed the three-tier memory tree since PR7/PR8 with **real, model-backed implementations**, so the source/topic/global trees produce genuine summary content and semantic embeddings. Because the `Arc<dyn Summariser>` / `Arc<dyn Embedder>` injection (PR9) already abstracts the backend, **no tree code changes** — the swap lands at the factory + wiring layer.

The one structural change is on the **ingest hot path**: LLM summarisation is slow (seconds/call), and the cascade-seal currently runs synchronously inside `BucketSealAdapter.store()`. PR12 moves the cascade off the hot path via a **detached tokio task** (using PR8's already-exported `append_leaf_deferred` + `cascade_all_from` primitives), keeping chat/agent writes fast. This is a documented interim until PR13 ships the durable SQLite job queue.

**Out of scope (deferred):**
- The durable async job queue + worker pool + scheduler → **PR13** (openhuman's `jobs/`, ~2000 LoC).
- Semantic recall (cosine rerank over embeddings) in `effective_system_prompt` → **PR15**. PR12 makes embeddings *real*, not yet *queried*.
- Embedding-dimension migration of historical inert-zero rows → not needed until PR15 wires semantic recall; flagged there.

---

## 2. Why this slice (vs. the full jobs subsystem)

The bundled "jobs" scope conflated three separable efforts: real Embedder, real Summariser (the *feature* — real content), and the async job queue (the *optimization* — durability/retry/dedupe off the hot path). The synchronous path already works (PR8-11). PR12 delivers the feature with a lightweight hot-path fix; PR13 delivers the durable queue. Each is independently valuable and reviewable. (Brainstorming decision: "Split: PR12 = model swap, PR13 = job queue".)

---

## 3. Components

### 3.1 `OpenAiCompatEmbedder` — `score/embed/openai_compat.rs`

Real embedder that POSTs to an OpenAI-compatible `/embeddings` endpoint. uClaw already ships the config (`memubot_config::EmbeddingEndpointConfig`) and a local `/v1/embeddings` route (port 7337, backed by memU's FastEmbed bridge, ~100ms warm-path). Users can repoint at OpenAI / Voyage / llama-server / ollama / any compatible endpoint.

```rust
pub struct OpenAiCompatEmbedder {
    client: reqwest::Client,
    base_url: String,   // e.g. "http://localhost:7337/v1"
    model: String,      // the model id the endpoint expects
    dimensions: usize,  // expected output length (384 default)
}

#[async_trait]
impl Embedder for OpenAiCompatEmbedder {
    fn name(&self) -> &'static str { "openai_compat" }
    async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
        // POST {base_url}/embeddings { model, input: text }
        // parse { data: [ { embedding: [f32; dimensions] } ] }
        // validate length == self.dimensions
    }
}
```

- **Request**: `POST {base_url}/embeddings` body `{"model": <model>, "input": <text>}`. Standard OpenAI embeddings shape.
- **Response**: `{"data": [{"embedding": [...]}]}` — take `data[0].embedding`.
- **Validation**: returned vector length must equal `dimensions`; error otherwise (a dimension mismatch silently corrupts cosine rerank in PR15).
- **`model` field shape**: the config default is gbrain-shaped (`llama-server:bge-small-en-v1.5`). The adaptation step verifies what uClaw's `/v1/embeddings` route expects for `model` (full `<recipe>:<model>` vs bare model name) and sends the correct form.
- **Errors**: HTTP failure / non-200 / malformed body / wrong dimension → `anyhow::Error`. The caller (cascade task) is best-effort and logs+drops on error (the seal is retried later via PR13 / flush).

### 3.2 `LlmSummariser` — `tree_source/summariser/llm.rs`

Real summariser that folds a `&[SummaryInput]` into one summary by delegating to uClaw's existing `LlmProvider` (anthropic/openai — the agent's configured model). No new HTTP plumbing; reuses BYOK/key management.

```rust
pub struct LlmSummariser {
    provider: Arc<dyn LlmProvider>,
    model: String,        // CompletionConfig.model
}

#[async_trait]
impl Summariser for LlmSummariser {
    fn name(&self) -> &'static str { "llm" }
    async fn summarise(&self, inputs: &[SummaryInput], ctx: &SummaryContext<'_>)
        -> anyhow::Result<SummaryOutput> {
        // 1. Build a fold prompt: a system-style instruction + the
        //    concatenated inputs (each prefixed with its id/time-range).
        // 2. provider.complete(vec![user_msg], vec![], &CompletionConfig {
        //        model, max_tokens: ctx.token_budget, temperature: 0.3,
        //        thinking_enabled: false })
        // 3. Extract assistant text from RespondOutput → SummaryOutput {
        //        content, token_count (estimate), entities: vec![], topics: vec![] }
    }
}
```

- **Prompt**: a fixed fold instruction (e.g. "Summarise the following memory fragments into a single dense recap of ≤N tokens; preserve names, decisions, and dates") + the inputs joined with provenance markers. Tree kind (Source/Topic/Global) tunes one line of the instruction (per-source vs per-entity vs cross-source-daily framing).
- **`CompletionConfig`**: `max_tokens = ctx.token_budget`, `temperature ≈ 0.3` (summaries want determinism), `thinking_enabled = false`, `model` from config. Empty `tools`.
- **entities/topics**: stay `vec![]` in PR12 (matching the inert contract — the union-from-children path in seal/digest already propagates labels). An LLM-driven entity/topic extractor is a later refinement, not PR12.
- **token_count**: estimate from the output text (chars/4 heuristic or a shared util if one exists) — the seal only uses it for buffer accounting + the next-level budget.
- **Errors**: provider error → `anyhow::Error`; the detached cascade task logs+drops.

### 3.3 Factories — `score/embed/factory.rs` + `summariser/factory.rs`

Pick real-vs-inert from config, mirroring openhuman's factory pattern. Fallback to inert keeps tests + offline + unconfigured installs working.

```rust
// embed/factory.rs
pub fn build_embedder(cfg: &EmbeddingEndpointConfig) -> Arc<dyn Embedder> {
    if cfg.base_url.is_empty() || cfg.model.is_empty() {
        Arc::new(InertEmbedder::new())
    } else {
        Arc::new(OpenAiCompatEmbedder::new(cfg))
    }
}

// summariser/factory.rs
pub fn build_summariser(provider: Option<Arc<dyn LlmProvider>>, model: String) -> Arc<dyn Summariser> {
    match provider {
        Some(p) => Arc::new(LlmSummariser::new(p, model)),
        None => Arc::new(InertSummariser::new()),
    }
}
```

- Decision rule kept deliberately simple. The full openhuman factory has a 3-way precedence (explicit override → runtime-enabled → inert); PR12 ships the 2-way (configured → real, else inert) and notes the richer precedence as a later refinement if needed.

### 3.4 Hot-path change — `BucketSealAdapter.store()`

The only change to existing tree behavior. Today (PR9/PR10) `store()` calls the synchronous `append_leaf` (buffer append **+ cascade**) per chunk, for the source tree and each topic tree. With a real LLM summariser, a seal-triggering write would block the caller for seconds.

PR12 splits it using PR8's existing primitives:

```rust
// Per admitted chunk, per tree (source + each topic):
let gate_met = append_leaf_deferred(&self.store, &tree, &leaf)?;  // SYNC, fast, durable
if gate_met {
    // Spawn the slow LLM cascade off the hot path.
    let store = self.store.clone();
    let summariser = self.summariser.clone();
    let embedder = self.embedder.clone();
    let tree_mutex = self.tree_mutex(&mutex_key).await;  // same per-tree Arc<Mutex>
    let tree = tree.clone();
    tokio::spawn(async move {
        let _guard = tree_mutex.lock().await;  // preserves PR8 per-tree serialisation
        if let Err(e) = cascade_all_from(&store, &tree, 0, &summariser, &embedder, None, &LabelStrategy::Empty).await {
            tracing::warn!(tree_id = %tree.id, error = %e, "detached cascade failed (best-effort; recovered by PR13 queue / flush)");
        }
    });
}
```

- **Durability**: `append_leaf_deferred` commits the L0 buffer row synchronously. A crash mid-cascade loses only the *un-run seal* — the buffer persists, so a later `cascade_all_from` (PR13 queue, or a `flush_stale` pass) re-runs it. No leaf is lost.
- **Read-after-write**: chunks are inserted synchronously (before the spawn), so recall over chunks is unaffected. Only summaries lag by the cascade latency — acceptable (summaries aren't read-after-write critical).
- **Concurrency contract preserved**: the detached task acquires the same per-tree `Arc<Mutex<()>>` from `tree_mutexes` before cascading, so PR8's per-tree serialisation still holds. Ordering across detached tasks for the same tree is non-deterministic but safe (each acquires the mutex; the buffer-then-cascade sequence is idempotent on already-sealed levels).
- **`store()` return**: returns as soon as all chunks are buffered + their cascades are *spawned* — it does NOT await the cascades. The chat/agent write is fast again.
- **Signature note**: `cascade_all_from`'s exact parameter list is verified at implementation time (PR8 signature: `cascade_all_from(store, tree, from_level, summariser, embedder, ?, strategy)`); the spawn adapts to it.

### 3.5 Wiring — `app.rs`

At `BucketSealAdapter` construction, build the real embedder + summariser from config instead of hardcoding inert:

```rust
let embedder = crate::memory_bucket_seal::score::embed::build_embedder(
    &memubot_config.memory_os.embedding_endpoint,
);
let summariser = crate::memory_bucket_seal::tree_source::summariser::build_summariser(
    bucket_seal_llm_provider,  // the app's configured LlmProvider, or None → inert
    summariser_model,
);
// ... BucketSealAdapter::new(store, content_root, embedder, summariser)
```

- The `LlmProvider` handle: reuse whatever the agent already constructs at boot. If a provider isn't configured (no API key, offline), pass `None` → `InertSummariser` fallback so the app still boots + the trees still accept leaves (with inert summaries).
- Embedding endpoint comes from `memubot_config.memory_os.embedding_endpoint` (already loaded at boot — `app.rs` reads `memubot_config` already).

---

## 4. Data flow (after PR12)

```text
chat/agent write
  └─ BucketSealAdapter.store()                [SYNC, fast]
       canonicalise → chunk → score admission
       stage_chunks + upsert_score            [durable]
       per admitted chunk:
         append_leaf_deferred(source tree)    [SYNC buffer append, durable]
           gate met? → spawn cascade_all_from(source) ──┐
         extract_entities                                │  [DETACHED, slow]
         per entity:                                     │    summariser.summarise (LLM)
           append_leaf_deferred(topic tree)   [SYNC]     │    embedder.embed (HTTP)
             gate met? → spawn cascade_all_from(topic) ──┤    seal node + climb
       return Ok(())                          [FAST]  ◄──┘  (best-effort, mutex-serialised)

manual IPC: memory_global_digest_run
  └─ end_of_day_digest → LlmSummariser folds cross-source → embed → daily node + count-cascade
```

---

## 5. Error handling

| Failure | Behavior |
|---|---|
| Embedder HTTP error / bad dimension | `anyhow::Error` → detached cascade logs `warn` + drops; seal retried later (buffer persists). For the synchronous digest IPC, the error propagates to the IPC result. |
| Summariser provider error | Same — `warn` + drop in cascade; propagate in digest IPC. |
| No embedding endpoint configured | Factory returns `InertEmbedder` — trees work with zero-vectors (no semantic recall, which isn't wired until PR15 anyway). |
| No LLM provider configured | Factory returns `InertSummariser` — trees work with concat-summaries. |
| Detached task panic | Isolated to the task (tokio catches); the hot-path write already returned `Ok`. Logged. |

The guiding principle: **memory is best-effort and never blocks or breaks the primary write.** A degraded backend (inert fallback) is always preferable to a failed ingest.

---

## 6. Testing

| Area | Tests |
|---|---|
| `OpenAiCompatEmbedder` | request shape (mock HTTP server e.g. a tiny `wiremock`/`httptest`, or a trait-level fake): correct body, parses `data[0].embedding`, validates dimension, errors on non-200 / bad body. ~4 tests. |
| `LlmSummariser` | with a fake `LlmProvider` (returns canned `RespondOutput`): builds a prompt containing the inputs, returns the provider's text as `SummaryOutput.content`, respects `ctx.token_budget` in `CompletionConfig`, empty entities/topics. ~4 tests. |
| Factories | configured → real type; empty/None → inert type. ~4 tests. |
| Hot-path detached seal | `store()` with a fake slow summariser returns quickly (does not await the cascade); the cascade eventually seals (await a short settle, or expose a test hook). Buffer is durable even if the cascade hasn't run. ~3 tests. |
| Regression | full `cargo test --lib memory_bucket_seal` stays green (existing inert-based tests unaffected — they construct adapters with inert directly). |

Fakes over live calls: no test should require a running Ollama or a real API key. A `FakeLlmProvider` + an HTTP mock (or a trait-level injected embedder fake) keep CI hermetic.

---

## 7. Scope boundaries (what PR12 does NOT touch)

- **No `jobs/` module, no `mem_tree_jobs` table, no worker pool, no scheduler** — that's PR13. The detached `tokio::spawn` is the interim.
- **No `effective_system_prompt` / recall wiring** — PR15. Embeddings become real but aren't queried.
- **No schema migration** — embeddings already have a column (PR7); inert wrote zeros, real writes real vectors, same blob format.
- **No new IPC commands** — `memory_global_digest_run` (PR11) already exists; it now produces real content automatically.
- **No config UI** — `EmbeddingEndpointConfig` + its settings page already exist (Sprint 2.2). PR12 just consumes the config.

---

## 8. File plan (preview — detailed in the implementation plan)

| File | New/Mod | Purpose |
|---|---|---|
| `score/embed/openai_compat.rs` | new | `OpenAiCompatEmbedder` + tests |
| `score/embed/factory.rs` | new | `build_embedder(cfg)` + tests |
| `score/embed/mod.rs` | mod | declare + re-export |
| `tree_source/summariser/llm.rs` | new | `LlmSummariser` + tests |
| `tree_source/summariser/factory.rs` | new | `build_summariser(provider, model)` + tests |
| `tree_source/summariser/mod.rs` | mod | declare + re-export |
| `adapter.rs` | mod | `store()` hot-path split (deferred append + detached cascade) + tests |
| `app.rs` | mod | build real embedder/summariser from config, inject into adapter |
| `Cargo.toml` | maybe | `reqwest` (likely already present — verify) |

Est. ~900 source + ~400 tests.

---

## 9. Open adaptation questions (resolved at implementation time, not blocking)

1. **Embedding `model` field shape** — does uClaw's `/v1/embeddings` route want the full `<recipe>:<model>` (`llama-server:bge-small-en-v1.5`) or a bare model name? Verify against `local_api/routes.rs`.
2. **`reqwest` presence + features** — confirm in `Cargo.toml`; the agent providers (anthropic/openai) likely already pull it with the needed TLS feature.
3. **`LlmProvider` handle at boot** — locate where the agent constructs its provider in `app.rs` so the summariser can share it (vs. constructing a second one). If the agent's provider is per-session, decide whether the summariser gets a dedicated boot-time provider built from the same config.
4. **`RespondOutput` text extraction** — read `agent::types::RespondOutput` to pull the assistant text cleanly.
5. **token_count estimator** — check for an existing util before adding a chars/4 heuristic.

---

## 10. Success criteria

- Real summaries + embeddings flow through source/topic/global trees when an endpoint + provider are configured; inert fallback otherwise.
- Chat/agent writes stay fast — no synchronous LLM call on the ingest hot path (verified by a test that `store()` returns without awaiting the cascade).
- Durable: a crash between buffer-append and cascade loses no leaf (buffer persists).
- All existing `memory_bucket_seal` tests stay green; ~15 new tests pass.
- CI hermetic — no live model calls in tests.
