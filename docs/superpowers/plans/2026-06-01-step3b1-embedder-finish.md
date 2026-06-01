# Step 3b-1 — Embedder Finish (kill memU's embedding role) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Repoint all four `MemUClient::embed_text` callers to the shared in-process `AppState.bucket_seal_embedder` (`Arc<dyn Embedder>` = `OnnxEmbedder`), so nothing in the app calls memU/Python for embeddings.

**Architecture:** The shared embedder handle already exists (`app.rs:230`, built once at boot by `build_embedder`). This slice only *repoints* the four remaining direct callers — it does NOT delete the `MemUClient::embed_text` method or the bridge (that is Step 3b-4, after the store is also off memU). After this slice the `/v1/embeddings` route stays alive but is served in-process; `FASTEMBED_MODEL` and the embedding half of the Python bridge become dead weight.

**Tech Stack:** Rust, `async_trait`, the `memory_bucket_seal::Embedder` trait.

**Key facts from recon (do not re-discover):**
- `Embedder` trait: `memory_bucket_seal/score/embed/mod.rs:58` — `name()`, `dim()`, `async embed(text)->Result<Vec<f32>>` (single text).
- A pure `cosine_similarity(&[f32],&[f32])->f32` already exists at `score/embed/mod.rs:79` (re-exported), semantics identical to memU's `cosine_sim` (0.0 on mismatch/zero) — use it; do NOT move memU's copy.
- The four callers:
  1. `GeneRetriever` (`agent/gep/retrieval.rs`) — holds `Option<Arc<MemUClient>>`, calls `embed_text(&[x])` for single texts. **All six `GeneRetriever::new` sites pass `false, None`** (1 prod helper `build_gene_retriever` at `tauri_commands.rs:74` + 5 tests) → the semantic path is currently DEAD. Swapping the type is behavior-preserving.
  2. `embed_skill_body` (`memu/embedding.rs:18`) — only caller is `proactive/service.rs:3003` (skill-embedding backfill); also uses `serialize_embedding` at `:3004`.
  3. `local_api /v1/embeddings` route (`local_api/routes.rs:323`) — batches via `client.embed_text(&text_refs)`.
  4. `memu_embed_text` Tauri command (`tauri_commands.rs:17666`) — batches.
- `ProactiveService` already holds `skill_adapter: Arc<dyn MemoryAdapter>` (bucket_seal) but NOT a raw embedder; built at `main.rs:363` where `state.bucket_seal_embedder` is reachable (same scope as `memu_client` from `state` at `main.rs:209`).
- `LocalApiService::new(config, memu_client)` (`local_api/server.rs:43`) builds `ApiState { memu_client }` (`:75`); constructed at `main.rs:468` with `memu_client.clone()`. `bucket_seal_embedder` is reachable from `state`/`state_ref` in that Stage-3 block.

---

## File Structure

| File | Responsibility / change |
|---|---|
| `src/memory_bucket_seal/score/embed/mod.rs` | Add `Embedder::embed_batch` default method (loops `embed`) for the batch callers |
| `src/agent/gep/retrieval.rs` | `GeneRetriever`: `Option<Arc<MemUClient>>` → `Option<Arc<dyn Embedder>>`; `embed_text(&[x])` → `embed(x)`; `cosine_sim` → `cosine_similarity`; drop `use crate::memu::*` |
| `src/tauri_commands.rs` | `build_gene_retriever` (`:74`) keeps `None` but with the new type; `memu_embed_text` (`:17666`) → shared embedder `embed_batch` |
| `src/proactive/skill_embedding.rs` (new) | Move `embed_skill_body` (now takes `&Arc<dyn Embedder>`) + `serialize_embedding` + `parse_embedding` here |
| `src/proactive/mod.rs` | `pub mod skill_embedding;` |
| `src/proactive/service.rs` | Add `embedder: Arc<dyn Embedder>` field + builder param; `:3003` calls the moved `embed_skill_body` with the handle |
| `src/memu/embedding.rs` | DELETE (content moved / superseded) |
| `src/memu/mod.rs` | Drop `pub mod embedding;` |
| `src/local_api/server.rs` | `LocalApiService::new` + `ApiState` gain `embedder: Arc<dyn Embedder>` |
| `src/local_api/routes.rs` | `/v1/embeddings` uses `state.embedder.embed_batch(...)` instead of `memu_client.embed_text` |
| `src/main.rs` | Pass `state.bucket_seal_embedder.clone()` into `ProactiveService::new` (`:363`) and `LocalApiService::new` (`:468`) |

---

## Task 1: `Embedder::embed_batch` default method

**Files:**
- Modify: `src/memory_bucket_seal/score/embed/mod.rs` (trait at `:58`)
- Test: inline `#[cfg(test)]` in the same file

- [ ] **Step 1: Write the failing test** (uses the existing test-only `InertEmbedder`, which returns a fixed-dim zero vector)

```rust
#[tokio::test]
async fn embed_batch_default_loops_embed() {
    use crate::memory_bucket_seal::score::embed::{Embedder, InertEmbedder};
    let e = InertEmbedder::with_dim(8);
    let out = e.embed_batch(&["a", "b", "c"]).await.unwrap();
    assert_eq!(out.len(), 3, "one vector per input");
    for v in &out {
        assert_eq!(v.len(), 8, "each vector has dim()");
    }
}
```

- [ ] **Step 2: Run it, expect failure** — `cargo test --lib embed_batch_default_loops_embed` → FAIL (`no method named embed_batch`).

- [ ] **Step 3: Add the default method to the trait**

In `mod.rs`, inside `pub trait Embedder` (after `embed`):

```rust
    /// Embed many texts. Default loops `embed` sequentially; impls backed by a
    /// batching backend may override. Returns one `Vec<f32>` (length `dim()`)
    /// per input, in order. Errors on the first failing element.
    async fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
        let mut out = Vec::with_capacity(texts.len());
        for t in texts {
            out.push(self.embed(t).await?);
        }
        Ok(out)
    }
```

(The trait is `#[async_trait]`; a defaulted async method is supported.)

- [ ] **Step 4: Run the test, expect PASS** — `cargo test --lib embed_batch_default_loops_embed` → PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/score/embed/mod.rs
git commit -m "feat(embed): Embedder::embed_batch default method (Step 3b-1)"
```

---

## Task 2: Repoint `GeneRetriever` off memU (type-level decouple)

**Files:**
- Modify: `src/agent/gep/retrieval.rs`
- Modify: `src/tauri_commands.rs:74` (the one prod `GeneRetriever::new`)

**Note:** All `GeneRetriever::new` sites pass `false, None`, so the semantic/embed path is dead today. This task is a behavior-preserving type swap that removes the `memu` dependency. Do NOT enable the dormant path.

- [ ] **Step 1: Run the existing gep tests to confirm green baseline**

Run: `cargo test --lib agent::gep::retrieval`
Expected: PASS (5 tests using `GeneRetriever::new(genes, false, None)`).

- [ ] **Step 2: Swap the imports** in `retrieval.rs` (replace lines 14-15)

```rust
use crate::memory_bucket_seal::score::embed::{cosine_similarity, Embedder};
```

(Delete `use crate::memu::client::MemUClient;` and `use crate::memu::embedding::cosine_sim;`.)

- [ ] **Step 3: Swap the field type** (`retrieval.rs:25-26`)

```rust
    /// In-process embedder for Stage-2 semantic search (None disables it).
    embedder: Option<Arc<dyn Embedder>>,
```

- [ ] **Step 4: Update `new` + the two `embed_text` call sites**

In `new` (`:36-48`): rename the param `memu_client: Option<Arc<MemUClient>>` → `embedder: Option<Arc<dyn Embedder>>`, assign `embedder`.

In `match_genes` (`:64`): `self.memu_client.is_some()` → `self.embedder.is_some()`.

In `stage2_semantic_match` (`:130-180`):
```rust
        let client = match &self.embedder {
            Some(c) => c,
            None => return Vec::new(),
        };
        // ...
        let query_embedding = match client.embed(user_message).await {
            Ok(v) if !v.is_empty() => v,
            _ => {
                tracing::warn!("[GeneRetriever] Stage 2: failed to embed user query");
                return Vec::new();
            }
        };
        // ... and for the gene cache fill:
                    match client.embed(&gene_text).await {
                        Ok(v) if !v.is_empty() => {
                            let emb = v;
                            if let Ok(mut cache) = self.gene_embeddings.lock() {
                                cache.insert(gene.gene_id.clone(), emb.clone());
                            }
                            emb
                        }
                        _ => continue,
                    }
        // ... and the similarity call:
            let sim = cosine_similarity(&query_embedding, &gene_embedding);
```

- [ ] **Step 5: Update the six `GeneRetriever::new` sites** — they keep `None`, type now infers `Option<Arc<dyn Embedder>>`. The 5 test sites at `retrieval.rs:383,393,407,416,459` and the prod helper `tauri_commands.rs:74` compile unchanged (literal `None`). No edit needed unless a turbofish is required; if the compiler complains about ambiguous `None`, annotate at `tauri_commands.rs:74`:

```rust
    let mut retriever = crate::agent::gep::retrieval::GeneRetriever::new(active_genes, false, None);
```
(Leave as-is first; only annotate if the build errors.)

- [ ] **Step 6: Build + test**

Run: `cargo build 2>&1 | grep -E "^error"` (expect none) and `cargo test --lib agent::gep::retrieval` (expect PASS).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/agent/gep/retrieval.rs src-tauri/src/tauri_commands.rs
git commit -m "refactor(gep): GeneRetriever uses Arc<dyn Embedder>, drops memU coupling (Step 3b-1)

Behavior-preserving: all GeneRetriever::new sites pass None (semantic path was
already dead). Swaps the embed call to the in-process Embedder trait + reuses
score::embed::cosine_similarity, removing the memu::client/embedding imports."
```

---

## Task 3: Move skill-embedding helpers out of `src/memu/`; repoint to the shared embedder

**Files:**
- Create: `src/proactive/skill_embedding.rs`
- Modify: `src/proactive/mod.rs`, `src/proactive/service.rs`
- Modify: `src/main.rs:363` (pass the embedder into `ProactiveService::new`)
- Delete: `src/memu/embedding.rs`; Modify: `src/memu/mod.rs`

- [ ] **Step 1: Confirm `parse_embedding` consumer set** (so nothing breaks on the move)

Run: `grep -rn "parse_embedding" src-tauri/src/`
Expected: only `memu/embedding.rs` defs/tests (no external caller). If an external caller exists, it must be repointed to `crate::proactive::skill_embedding::parse_embedding` in this task.

- [ ] **Step 2: Create `src/proactive/skill_embedding.rs`** with the moved helpers (embed via the trait)

```rust
// SPDX-License-Identifier: <match the repo header used in src/proactive/*.rs>
//! Skill-body embedding helpers (moved out of the deprecated memU module).
//!
//! - `embed_skill_body` — embed a skill body via the in-process embedder
//! - `serialize_embedding` / `parse_embedding` — `embedding_json` round-trip

use std::sync::Arc;

use crate::memory_bucket_seal::score::embed::Embedder;

/// Embed the full text body of a skill and return the raw vector.
///
/// Returns `None` (and logs a warning) if the embedder call fails or the
/// response vector is empty.
pub async fn embed_skill_body(embedder: &Arc<dyn Embedder>, body: &str) -> Option<Vec<f32>> {
    match embedder.embed(body).await {
        Ok(v) if !v.is_empty() => Some(v),
        Ok(_) => {
            tracing::warn!("embed_skill_body: embedder returned empty vector");
            None
        }
        Err(e) => {
            tracing::warn!(error = %e, "embed_skill_body: embed failed");
            None
        }
    }
}

/// Serialize a `Vec<f32>` to a compact JSON string for `embedding_json` storage.
pub fn serialize_embedding(embedding: &[f32]) -> String {
    serde_json::to_string(embedding).unwrap_or_else(|_| "[]".to_string())
}

/// Deserialize an `embedding_json` string back to `Vec<f32>`.
pub fn parse_embedding(json: Option<&str>) -> Option<Vec<f32>> {
    let s = json?.trim();
    if s.is_empty() || s == "null" {
        return None;
    }
    serde_json::from_str::<Vec<f32>>(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_embedding_round_trip() {
        let original: Vec<f32> = (0..8).map(|i| i as f32 * 0.1).collect();
        let json = serialize_embedding(&original);
        let parsed = parse_embedding(Some(&json)).expect("should round-trip");
        assert_eq!(parsed.len(), original.len());
        for (a, b) in original.iter().zip(parsed.iter()) {
            assert!((a - b).abs() < 1e-6, "value mismatch: {} vs {}", a, b);
        }
    }

    #[test]
    fn parse_embedding_rejects_empty_and_null() {
        assert!(parse_embedding(None).is_none());
        assert!(parse_embedding(Some("")).is_none());
        assert!(parse_embedding(Some("null")).is_none());
    }
}
```

(Confirm the exact SPDX header line from an existing `src/proactive/*.rs` and copy it verbatim — the pre-commit hook rejects a missing SPDX.)

- [ ] **Step 3: Register the module** — in `src/proactive/mod.rs` add `pub mod skill_embedding;` (alphabetical with siblings).

- [ ] **Step 4: Add the embedder to `ProactiveService`**

In `service.rs`, add a field beside `skill_adapter` (the struct around `:416`):
```rust
    /// In-process embedder for skill-body embedding backfill.
    embedder: std::sync::Arc<dyn crate::memory_bucket_seal::score::embed::Embedder>,
```
Add the same to the matching `ProactiveStateRefs`/builder struct if one is used (around `:564`/`:632`), and to the `new(...)` signature + assignment. Thread it through `from_*`/`for_tests` constructors (tests can pass `Arc::new(InertEmbedder::new())`).

- [ ] **Step 5: Repoint the call site** (`service.rs:3003-3004`)

```rust
                            if let Some(vec) = crate::proactive::skill_embedding::embed_skill_body(&self.embedder, content).await {
                                let json = crate::proactive::skill_embedding::serialize_embedding(&vec);
```
(`memu` local var is no longer used for this; confirm it isn't dropped from other uses in the same fn — if it becomes unused, remove the binding.)

- [ ] **Step 6: Pass the embedder at construction** (`main.rs:363`)

Add `state.bucket_seal_embedder.clone()` as the new `ProactiveService::new` argument in the correct positional slot (match the field order added in Step 4). `state` is in scope (same place `memu_client` was cloned).

- [ ] **Step 7: Delete the memU embedding module**

```bash
git rm src-tauri/src/memu/embedding.rs
```
Remove `pub mod embedding;` from `src/memu/mod.rs`.

- [ ] **Step 8: Build + test**

Run: `cargo build 2>&1 | grep -E "^error"` (expect none); `cargo test --lib proactive::skill_embedding` (expect PASS).

- [ ] **Step 9: Commit**

```bash
git add -A src-tauri/src/proactive/ src-tauri/src/memu/ src-tauri/src/main.rs
git commit -m "refactor(proactive): skill embedding via in-process embedder; delete memu::embedding (Step 3b-1)"
```

---

## Task 4: Repoint `local_api /v1/embeddings` + `memu_embed_text` to the shared embedder

**Files:**
- Modify: `src/local_api/server.rs` (`LocalApiService`, `ApiState`)
- Modify: `src/local_api/routes.rs` (`openai_embeddings`)
- Modify: `src/main.rs:468` (`LocalApiService::new` call)
- Modify: `src/tauri_commands.rs:17666` (`memu_embed_text`)

- [ ] **Step 1: Add `embedder` to `LocalApiService` + `ApiState`** (`server.rs`)

- Field on `LocalApiService` (beside `memu_client` at `:28`): `embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder>,`
- `new(config, memu_client)` → `new(config, memu_client, embedder)`; assign it (`:43-46`).
- `ApiState` (the struct built at `:75`) gains the same `embedder` field; set `embedder: self.embedder.clone()`.

- [ ] **Step 2: Use the embedder in the route** (`routes.rs:357-377`)

Replace the `state.memu_client.as_ref().ok_or_else(...)` block and the `client.embed_text(&text_refs)` call with:
```rust
    let text_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let total_chars: usize = texts.iter().map(|s| s.len()).sum();

    let vectors = state.embedder.embed_batch(&text_refs).await.map_err(|e| {
        openai_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            format!("embed failed: {}", e),
            "server_error",
            Some("embed_failed"),
        )
    })?;
```
(The 503 "memu unavailable" branch is removed — the in-process embedder is always present. The vector-count check + response mapping below it stay unchanged. Update the doc-comment at `:313-322` to say "in-process embedder" instead of "memU FastEmbed".)

- [ ] **Step 3: Pass the embedder at construction** (`main.rs:468`)

```rust
                            uclaw_core::local_api::LocalApiService::new(
                                local_api_config,            // existing arg
                                memu_client.clone(),
                                state.bucket_seal_embedder.clone(),
                            )
```
(Use the exact existing first-arg expression; only add the third arg. Confirm `state`/`state_ref` field name for the embedder is `bucket_seal_embedder`.)

- [ ] **Step 4: Repoint `memu_embed_text`** (`tauri_commands.rs:17666`)

```rust
#[tauri::command]
pub async fn memu_embed_text(
    state: State<'_, AppState>,
    texts: Vec<String>,
) -> Result<Vec<Vec<f32>>, String> {
    let texts_refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    state
        .bucket_seal_embedder
        .embed_batch(&texts_refs)
        .await
        .map_err(|e| format!("Failed to generate embeddings: {:?}", e))
}
```
(Command name kept for IPC compatibility; doc-comment updated to "in-process embedder".)

- [ ] **Step 5: Build + clippy + test**

Run: `cargo build 2>&1 | grep -E "^error"` (none); `cargo clippy --lib 2>&1 | grep -E "^error"` (none); `cargo test --lib local_api` (PASS if any).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/local_api/ src-tauri/src/main.rs src-tauri/src/tauri_commands.rs
git commit -m "refactor(local_api): /v1/embeddings + memu_embed_text use in-process embedder (Step 3b-1)"
```

---

## Task 5: Whole-slice verification (no remaining app callers of `MemUClient::embed_text`)

**Files:** none (verification only)

- [ ] **Step 1: Full build + clippy**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` (none) and `cargo clippy --lib 2>&1 | grep -E "^error" | head` (none).

- [ ] **Step 2: Confirm no app code calls memU for embeddings**

Run: `grep -rn "embed_text" src-tauri/src/ | grep -v "score/embed"`
Expected: ONLY the `MemUClient::embed_text` method definition (`memu/client.rs:298`) and its bridge plumbing remain — NO callers in `agent/`, `proactive/`, `local_api/`, `tauri_commands.rs`. (The method itself is deleted with the bridge in 3b-4.)

- [ ] **Step 3: Confirm `memu::embedding` is gone**

Run: `grep -rn "memu::embedding\|mod embedding" src-tauri/src/`
Expected: no matches.

- [ ] **Step 4: Targeted test run**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::score::embed && cargo test --lib agent::gep::retrieval && cargo test --lib proactive::skill_embedding`
Expected: all PASS.

- [ ] **Step 5: Final commit (if any verification tweak), else proceed to PR.**

---

## Self-Review

- **Spec coverage:** 3b-1 spec items → embed_batch (T1), GeneRetriever (T2), embed_skill_body+cosine_sim move (T3), local_api+command (T4), "nothing calls memU for embeddings" (T5). ✓
- **No placeholders:** every code step has concrete code. ✓
- **Type consistency:** `Option<Arc<dyn Embedder>>` (GeneRetriever) vs `Arc<dyn Embedder>` (ProactiveService field, ApiState field — always present) — intentional: GeneRetriever's is optional (dormant path), the others are always-on. `embed_batch` signature matches its three batch callers. ✓
- **Finish-line discipline:** each task deletes its memU embed usage; `memu/embedding.rs` deleted in T3. The `MemUClient::embed_text` *method* + bridge remain for the store, deleted in 3b-4 — this is per-spec, not a half-cut path (memU stays fully wired for the store until 3b-3/3b-4). ✓
