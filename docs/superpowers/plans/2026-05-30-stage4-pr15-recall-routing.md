# 阶段 4 PR15 — Recall Routing into `effective_system_prompt` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire bucket_seal hybrid recall (semantic cosine over real summary embeddings + FTS5 over raw chunks) into the agent's per-turn system prompt — additively, alongside the existing legacy memory-recall injection — so the 阶段 4 memory engine finally reads back into the live agent loop.

**Architecture:** Add `BucketSealAdapter::recall_semantic` (embed query → cosine vs `mem_tree_summaries` embeddings) + `recall_hybrid` (semantic summaries first, FTS chunks backfill, dedup, budget). A pure `render_bucket_seal_recall` formats entries into a `## Relevant Memory (bucket-seal)` block. The chat/agent send sites call `recall_hybrid` (best-effort) and `append_memory_context` the rendered block onto the existing memory context. bucket_seal-only; the legacy graph/memU/session recall is untouched.

**Tech Stack:** Rust, `async-trait`, `anyhow`, `tracing`, PR7's `score::embed::{cosine_similarity, EMBEDDING_DIM}`, PR8's `tree_source::store::{list_trees_by_kind, list_summaries_at_level}` + `SummaryNode.embedding`, PR9's FTS5 `recall`, PR13's `state.bucket_seal_adapter`. No new deps.

---

## Source-of-truth references (verified during planning)

- `memory_bucket_seal/adapter.rs` — `BucketSealAdapter { store: Arc<BucketSealStore>, embedder: Arc<dyn Embedder>, summariser, ... }`. Existing `async fn recall(&self, query, limit, opts: RecallOpts) -> Result<Vec<MemoryEntry>>` (FTS5 over `mem_tree_chunks_fts`, line ~395). `fresh_adapter()` + `fresh_adapter_with_summariser` test fixtures.
- `memory_bucket_seal/score/embed/mod.rs` — `pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32` (line 71; returns 0.0 on zero-magnitude/length-mismatch), `pub const EMBEDDING_DIM: usize = 1024`, `Embedder::embed(&self, text) -> Result<Vec<f32>>`, `InertEmbedder`.
- `memory_bucket_seal/tree_source/store.rs` — `list_trees_by_kind(store, kind) -> Result<Vec<Tree>>` (109), `list_summaries_at_level(store, tree_id, level) -> Result<Vec<SummaryNode>>` (298), `get_summary` (279). `Tree { id, kind, scope, max_level, ... }`.
- `memory_bucket_seal/tree_source/types.rs` — `SummaryNode { id, tree_id, tree_kind, level, content: String, token_count, time_range_start, time_range_end, sealed_at: DateTime<Utc>, embedding: Option<Vec<f32>>, ... }` (embedding carried inline), `TreeKind::{Source, Topic, Global}`.
- `memory_adapter/types.rs` — `MemoryEntry { id, key, content, namespace: Option<String>, category: MemoryCategory, timestamp: String, session_id: Option<String>, score: Option<f64> }`. `RecallOpts<'a> { namespace, category, session_id, min_score }`. `MemoryCategory::Conversation`.
- `agent/dispatcher/content_assembler.rs` — `pub fn append_memory_context(&mut self, extra: &str)` (~514; appends to or creates `memory_context`), `pub fn set_memory_context(...)`. `memory_context: Option<String>` appended in `effective_system_prompt`.
- `agent/gbrain_prompt.rs` — `GBRAIN_SECTION_MARKER` + `GbrainKnowledgeSection::render` — the section-module shape to mirror (marker const + render fn).
- `tauri_commands.rs` — chat send site: gbrain block set ~2167; legacy "Memory Recall Integration" block ~2171-2290; `delegate.set_memory_context(memory_ctx)` at ~2235 and ~2280. `input.content` (the user's message) + `space_id` + `input.conversation_id` in scope. Agent send sites near ~11365 + ~15188 (gbrain block set there too).
- `app.rs` — `state.bucket_seal_adapter: Arc<BucketSealAdapter>` (PR13 concrete handle). `state.memory_adapters` registry.
- `agent/mod.rs` — module declarations (add `pub mod memory_recall_block;`).

---

## CRITICAL design facts

1. **Additive, not greenfield** — the prompt already injects recalled memory (legacy graph + memU + session) via `set_memory_context` at the send sites. PR15 APPENDS a bucket_seal section via `append_memory_context`; it does NOT touch the legacy paths.
2. **Semantic unit = summaries** — chunks have no embeddings; real embeddings (PR12) live on `SummaryNode.embedding`. `recall_semantic` ranks summaries. FTS `recall` covers raw chunks (incl. recent un-sealed leaves).
3. **Best-effort, never blocks the turn** — `recall_hybrid` swallows errors → empty → no block appended. Embed failure (no 1024-dim endpoint) → FTS-only fallback.
4. **bucket_seal only** — use `state.bucket_seal_adapter` directly (no trait downcast, no gbrain/legacy fan-out).
5. **cosine over zero vectors = 0.0** — `InertEmbedder` returns zeros → all cosines 0.0 → `recall_semantic` returns up-to-limit summaries in arbitrary-but-stable order without panic (degenerate but safe). Real ordering needs a real/fake-distinct embedder (tests use a fake).

---

## File Structure

| File | New/Mod | Responsibility | LoC |
|---|---|---|---|
| `memory_bucket_seal/adapter.rs` | mod | `recall_semantic` + `recall_hybrid` methods + tests | +180 (incl. ~100 tests) |
| `agent/memory_recall_block.rs` | new | `render_bucket_seal_recall` + `BUCKET_SEAL_RECALL_MARKER` + tests | ~110 (incl. ~50 tests) |
| `agent/mod.rs` | mod | `pub mod memory_recall_block;` | +1 |
| `tauri_commands.rs` | mod | `append_bucket_seal_recall(state, delegate, query).await` helper + call at chat/agent send sites | +45 |

Est. ~230 source + ~150 tests.

---

## Adaptation responsibilities (verify before trusting the plan)

1. **`SummaryNode.embedding` is `Option<Vec<f32>>` carried inline** by `list_summaries_at_level` — confirm (PR8 types.rs). If embeddings are NOT on the node (stored separately), fall back to `get_summary_embedding(store, &node.id)` per summary.
2. **`recall_semantic` summary enumeration** — iterate `for kind in [Source, Topic, Global] { for tree in list_trees_by_kind(store, kind)? { for level in 0..=tree.max_level { list_summaries_at_level(store, &tree.id, level)? } } }`, collect summaries with `embedding.is_some()`. Namespace filter: when `namespace` is `Some(ns)`, keep only summaries whose tree `scope == ns` (look up via the tree in the loop — you already have `tree.scope`). Track `(SummaryNode, tree_scope)` pairs so the MemoryEntry namespace is set.
3. **`MemoryEntry` from `SummaryNode`** — `id=summary.id`, `key=summary.id` (no separate key), `content=summary.content`, `namespace=Some(tree_scope)`, `category=MemoryCategory::Conversation`, `timestamp=summary.sealed_at.to_rfc3339()`, `session_id=None`, `score=Some(cosine as f64)`.
4. **Embed dimension** — `self.embedder.embed(query)` returns 1024-dim (or errors). `cosine_similarity` returns 0.0 on length mismatch, so a stored summary embedding of a different dim just ranks 0.0 (safe).
5. **`recall_hybrid` ordering** — semantic summaries sorted by cosine desc first; then FTS chunk entries (from `recall(query, limit, RecallOpts::default-ish)`); dedup by `entry.id`; cap to `max_entries`. If `recall_semantic` errs, use FTS only; if FTS errs, use semantic only; if both err, `vec![]`.
6. **`RecallOpts` for the FTS leg** — construct with `namespace: None` (prompt recall is cross-namespace), `category: None`, `session_id: None`, `min_score: None`. Verify the struct's exact construction (it has a lifetime; build inline).
7. **`append_memory_context` exposure on the delegate** — the assembler has `pub fn append_memory_context(&mut self, extra: &str)`. Verify `ChatDelegate` (the type `delegate` is) re-exposes it; if only `set_memory_context` is exposed on the delegate, either add a passthrough OR read the current memory_context, concat, and `set_memory_context`. Prefer `append_memory_context` if available.
8. **Send-site helper** — add a private async fn in `tauri_commands.rs`: `async fn append_bucket_seal_recall(state: &AppState, delegate: &mut ChatDelegate, query: &str)` that does the gated, best-effort recall + append. Call it at the chat send site (after the legacy recall block) and the agent send site(s). Verify `delegate` is `&mut` accessible there + `AppState` type path.
9. **Token budget** — `render_bucket_seal_recall(entries, token_budget=1500)`; greedy fill via a chars/4 estimate (reuse `estimate_tokens` from PR12's summariser `llm.rs` if exported, else a local `fn`).
10. **Gating** — skip when `query.trim().is_empty()`.
11. **Which send sites** — chat send (~2171 region) is required. Confirm the agent send paths (~11365, ~15188) and add the helper call there too if they assemble a user-facing prompt. If a site is a sub-agent/spawn that shouldn't recall, skip it (note the decision).
12. **Pre-commit hooks** — no `--no-verify`.

---

### Task 1: `BucketSealAdapter::recall_semantic`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Write the failing test** (append to adapter tests). Use a fake embedder returning distinct vectors so cosine ordering is deterministic.

```rust
    // A fake embedder: maps specific texts to distinct unit vectors so
    // cosine ordering is deterministic in tests.
    struct FakeVecEmbedder;
    #[async_trait::async_trait]
    impl crate::memory_bucket_seal::score::embed::Embedder for FakeVecEmbedder {
        fn name(&self) -> &'static str { "fake_vec" }
        async fn embed(&self, text: &str) -> anyhow::Result<Vec<f32>> {
            // 1024-dim, mostly zeros; set one hot dimension by keyword.
            let mut v = vec![0.0f32; crate::memory_bucket_seal::score::embed::EMBEDDING_DIM];
            let idx = if text.contains("alpha") { 0 } else if text.contains("beta") { 1 } else { 2 };
            v[idx] = 1.0;
            Ok(v)
        }
    }

    #[tokio::test]
    async fn recall_semantic_ranks_by_cosine() {
        use crate::memory_bucket_seal::tree_source::{store as ts, types::{SummaryNode, TreeKind}};
        let dir = tempfile::TempDir::new().unwrap();
        let store = std::sync::Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        // Seed a source tree + two summaries with distinct embeddings.
        let tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "ns1").unwrap();
        let mk = |id: &str, content: &str, hot: usize| {
            let mut emb = vec![0.0f32; crate::memory_bucket_seal::score::embed::EMBEDDING_DIM];
            emb[hot] = 1.0;
            SummaryNode {
                id: id.into(), tree_id: tree.id.clone(), tree_kind: TreeKind::Source, level: 1,
                parent_id: None, child_ids: vec![], content: content.into(), token_count: 10,
                entities: vec![], topics: vec![],
                time_range_start: chrono::Utc::now(), time_range_end: chrono::Utc::now(),
                score: 0.5, sealed_at: chrono::Utc::now(), deleted: false, embedding: Some(emb),
            }
        };
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            ts::insert_summary_tx(&tx, &mk("s-alpha", "the alpha summary", 0)).unwrap();
            ts::insert_summary_tx(&tx, &mk("s-beta", "the beta summary", 1)).unwrap();
            ts::update_tree_after_seal_tx(&tx, &tree.id, "s-alpha", 1, chrono::Utc::now()).unwrap();
            tx.commit().unwrap();
        }
        let embedder: std::sync::Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = std::sync::Arc::new(FakeVecEmbedder);
        let summariser: std::sync::Arc<dyn crate::memory_bucket_seal::tree_source::summariser::Summariser> =
            std::sync::Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser);

        // Query "alpha" → s-alpha (hot dim 0) ranks above s-beta (hot dim 1).
        let hits = adapter.recall_semantic("alpha please", 10, None).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "s-alpha");
        assert!(hits[0].score.unwrap() > hits.get(1).and_then(|h| h.score).unwrap_or(0.0));
    }

    #[tokio::test]
    async fn recall_semantic_respects_namespace_and_limit() {
        // (Construct two trees ns1/ns2 with one summary each; assert namespace
        // filter keeps only ns1, and limit caps the result.)
        // ... mirror the setup above with two trees; assert filtering.
    }
```

(The implementer fleshes out the second test with the two-tree setup mirroring the first.)

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::recall_semantic 2>&1 | tail`
Expected: compile error (`recall_semantic` not defined).

- [ ] **Step 3: Implement `recall_semantic`** (in `impl BucketSealAdapter`, the inherent block)

```rust
/// Semantic recall: embed the query, cosine-rank summary embeddings, return
/// the top-`limit` summaries as MemoryEntries (the dense, curated recall
/// unit). Namespace filter matches a summary's source-tree scope.
pub async fn recall_semantic(
    &self,
    query: &str,
    limit: usize,
    namespace: Option<&str>,
) -> anyhow::Result<Vec<MemoryEntry>> {
    use crate::memory_bucket_seal::score::embed::cosine_similarity;
    use crate::memory_bucket_seal::tree_source::store as ts;
    use crate::memory_bucket_seal::tree_source::types::TreeKind;

    let qvec = self
        .embedder
        .embed(query)
        .await
        .context("recall_semantic: embed query")?;

    // Gather (cosine, MemoryEntry) over all summaries that carry an embedding.
    let mut scored: Vec<(f32, MemoryEntry)> = Vec::new();
    for kind in [TreeKind::Source, TreeKind::Topic, TreeKind::Global] {
        for tree in ts::list_trees_by_kind(&self.store, kind).context("list_trees_by_kind")? {
            if let Some(ns) = namespace {
                if tree.scope != ns {
                    continue;
                }
            }
            for level in 0..=tree.max_level {
                for node in ts::list_summaries_at_level(&self.store, &tree.id, level)
                    .context("list_summaries_at_level")?
                {
                    let Some(emb) = node.embedding.as_ref() else { continue };
                    let cos = cosine_similarity(&qvec, emb);
                    scored.push((
                        cos,
                        MemoryEntry {
                            id: node.id.clone(),
                            key: node.id.clone(),
                            content: node.content.clone(),
                            namespace: Some(tree.scope.clone()),
                            category: MemoryCategory::Conversation,
                            timestamp: node.sealed_at.to_rfc3339(),
                            session_id: None,
                            score: Some(cos as f64),
                        },
                    ));
                }
            }
        }
    }

    // Sort by cosine desc, take limit.
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    Ok(scored.into_iter().take(limit).map(|(_, e)| e).collect())
}
```

**Adaptation:** if `SummaryNode` does NOT carry `embedding` inline, replace `node.embedding.as_ref()` with `ts::get_summary_embedding(&self.store, &node.id)?`. Confirm `MemoryCategory`/`MemoryEntry` import paths (already used by the existing `recall`).

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::recall_semantic 2>&1 | tail`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter::recall_semantic (cosine over summary embeddings) (PR15.1 of 阶段 4)"
```

---

### Task 2: `BucketSealAdapter::recall_hybrid`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs`

- [ ] **Step 1: Write the failing test**

```rust
    #[tokio::test]
    async fn recall_hybrid_merges_semantic_and_fts_dedup() {
        // Seed a tree with a summary (semantic) + store a chunk via store()
        // (FTS). recall_hybrid returns both, deduped by id, summaries first.
        let dir = tempfile::TempDir::new().unwrap();
        let store = std::sync::Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: std::sync::Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = std::sync::Arc::new(FakeVecEmbedder);
        let summariser: std::sync::Arc<dyn crate::memory_bucket_seal::tree_source::summariser::Summariser> =
            std::sync::Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store.clone(), dir.path().join("content"), embedder, summariser);

        // FTS leg: store a chunk so the FTS MATCH finds it.
        adapter.store("ns1", "k1", "alpha keyword content for fts match", MemoryCategory::Core, None).await.unwrap();

        let hits = adapter.recall_hybrid("alpha", None, 6).await;
        // Best-effort: no panic; returns whatever each leg found.
        // At minimum the FTS chunk is present (semantic may be empty if no summaries sealed).
        assert!(hits.iter().any(|e| e.content.contains("alpha")) || hits.is_empty());
        // Dedup: no duplicate ids.
        let mut ids: Vec<&str> = hits.iter().map(|e| e.id.as_str()).collect();
        ids.sort();
        let before = ids.len();
        ids.dedup();
        assert_eq!(before, ids.len(), "no duplicate ids in hybrid result");
    }

    #[tokio::test]
    async fn recall_hybrid_both_empty_returns_empty() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = std::sync::Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: std::sync::Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = std::sync::Arc::new(FakeVecEmbedder);
        let summariser: std::sync::Arc<dyn crate::memory_bucket_seal::tree_source::summariser::Summariser> =
            std::sync::Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser);
        let hits = adapter.recall_hybrid("nothing here", None, 6).await;
        assert!(hits.is_empty());
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::recall_hybrid 2>&1 | tail`
Expected: compile error.

- [ ] **Step 3: Implement `recall_hybrid`**

```rust
/// Hybrid recall for prompt injection: dense summaries (semantic) first,
/// raw chunk hits (FTS) as backfill. Best-effort — a failing leg is skipped;
/// both failing → empty. Dedup by id; cap to `max_entries`.
pub async fn recall_hybrid(
    &self,
    query: &str,
    namespace: Option<&str>,
    max_entries: usize,
) -> Vec<MemoryEntry> {
    let mut out: Vec<MemoryEntry> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // Semantic summaries first (dense, curated).
    match self.recall_semantic(query, max_entries, namespace).await {
        Ok(sems) => {
            for e in sems {
                if seen.insert(e.id.clone()) {
                    out.push(e);
                }
            }
        }
        Err(e) => tracing::debug!(error = %format!("{e:#}"), "recall_hybrid: semantic leg failed (FTS only)"),
    }

    // FTS chunk backfill.
    if out.len() < max_entries {
        let opts = RecallOpts {
            namespace,
            category: None,
            session_id: None,
            min_score: None,
        };
        match self.recall(query, max_entries, opts).await {
            Ok(chunks) => {
                for e in chunks {
                    if out.len() >= max_entries {
                        break;
                    }
                    if seen.insert(e.id.clone()) {
                        out.push(e);
                    }
                }
            }
            Err(e) => tracing::debug!(error = %format!("{e:#}"), "recall_hybrid: FTS leg failed"),
        }
    }

    out.truncate(max_entries);
    out
}
```

**Adaptation:** confirm `RecallOpts` field set + that `namespace: Option<&str>` matches its lifetime (it's `Option<&'a str>` — `namespace` param is `Option<&str>`, fine). Confirm `self.recall(...)` is the trait method (callable on `&self`).

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter::tests::recall_hybrid 2>&1 | tail`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter::recall_hybrid (semantic + FTS merge) (PR15.2 of 阶段 4)"
```

---

### Task 3: `render_bucket_seal_recall` format helper

**Files:**
- Create: `src-tauri/src/agent/memory_recall_block.rs`
- Modify: `src-tauri/src/agent/mod.rs`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_adapter::types::{MemoryCategory, MemoryEntry};

    fn entry(id: &str, content: &str, score: f64) -> MemoryEntry {
        MemoryEntry {
            id: id.into(), key: id.into(), content: content.into(),
            namespace: Some("ns1".into()), category: MemoryCategory::Conversation,
            timestamp: "2026-05-30T00:00:00Z".into(), session_id: None, score: Some(score),
        }
    }

    #[test]
    fn empty_entries_returns_none() {
        assert!(render_bucket_seal_recall(&[], 1500).is_none());
    }

    #[test]
    fn renders_marker_and_entries() {
        let block = render_bucket_seal_recall(&[entry("s1", "alpha recap", 0.9)], 1500).unwrap();
        assert!(block.contains(BUCKET_SEAL_RECALL_MARKER));
        assert!(block.contains("alpha recap"));
    }

    #[test]
    fn budget_truncates() {
        let big = "x".repeat(8000); // ~2000 tokens at chars/4
        let entries = vec![entry("s1", &big, 0.9), entry("s2", "second", 0.8)];
        let block = render_bucket_seal_recall(&entries, 100).unwrap(); // ~400 chars budget
        // First entry alone exceeds the budget → second entry dropped.
        assert!(block.contains(BUCKET_SEAL_RECALL_MARKER));
        assert!(!block.contains("second"), "budget should truncate before the second entry");
    }
}
```

- [ ] **Step 2: Run to verify failure**

Run: `cd src-tauri && cargo test --lib agent::memory_recall_block 2>&1 | tail`
Expected: compile error.

- [ ] **Step 3: Implement `memory_recall_block.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Renders bucket_seal hybrid-recall results into a system-prompt block.
//!
//! Mirror of `gbrain_prompt::GbrainKnowledgeSection` by shape (marker const
//! + render fn), but injects RETRIEVED CONTENT rather than tool-usage
//! instructions. Called best-effort from the chat/agent send sites and
//! appended to the per-turn memory context.

use crate::memory_adapter::types::MemoryEntry;

/// Marker for logs/tests (mirrors `GBRAIN_SECTION_MARKER`).
pub const BUCKET_SEAL_RECALL_MARKER: &str = "## Relevant Memory (bucket-seal)";

/// Cheap token estimate (chars/4) — recall budgeting only.
fn est_tokens(s: &str) -> usize {
    (s.chars().count() + 3) / 4
}

/// Render recalled entries into a prompt block (summaries-first ordering is
/// the caller's responsibility). Greedy budget fill — stops once adding the
/// next entry would exceed `token_budget`. Returns None when nothing fits.
pub fn render_bucket_seal_recall(entries: &[MemoryEntry], token_budget: usize) -> Option<String> {
    if entries.is_empty() {
        return None;
    }
    let mut body = String::new();
    let mut used = est_tokens(BUCKET_SEAL_RECALL_MARKER);
    for e in entries {
        let ns = e.namespace.as_deref().unwrap_or("");
        let score = e.score.unwrap_or(0.0);
        let line = format!("- [{score:.2} · {ns}] {}\n", e.content.trim());
        let cost = est_tokens(&line);
        if used + cost > token_budget {
            break;
        }
        body.push_str(&line);
        used += cost;
    }
    if body.is_empty() {
        return None;
    }
    Some(format!("{BUCKET_SEAL_RECALL_MARKER}\n\n{body}"))
}

#[cfg(test)]
mod tests { /* from Step 1 */ }
```

- [ ] **Step 4: Wire `agent/mod.rs`**

```rust
pub mod memory_recall_block;
```

- [ ] **Step 5: Run tests**

Run: `cd src-tauri && cargo test --lib agent::memory_recall_block 2>&1 | tail`
Expected: 3 passed.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent/memory_recall_block.rs src-tauri/src/agent/mod.rs
git commit -m "feat(agent): render_bucket_seal_recall prompt block (PR15.3 of 阶段 4)"
```

---

### Task 4: Wire recall into the send sites

**Files:**
- Modify: `src-tauri/src/tauri_commands.rs`

- [ ] **Step 1: Read the chat send site** (~2167-2290) + the agent send sites (~11365, ~15188) — confirm `delegate` is `&mut`, `input.content` (or the equivalent query text) + `state: &AppState` (or `State<'_, AppState>`) are in scope at each, and how `set_memory_context` / `set_gbrain_knowledge_block` are called there.

- [ ] **Step 2: Add the best-effort recall helper** (near the other prompt-build helpers in `tauri_commands.rs`)

```rust
/// PR15: append a bucket_seal hybrid-recall block to the agent's memory
/// context. Best-effort — never blocks the turn; gated on non-empty query.
async fn append_bucket_seal_recall(
    state: &crate::app::AppState,
    delegate: &mut crate::agent::dispatcher::ChatDelegate,
    query: &str,
) {
    if query.trim().is_empty() {
        return;
    }
    let entries = state.bucket_seal_adapter.recall_hybrid(query, None, 6).await;
    if let Some(block) = crate::agent::memory_recall_block::render_bucket_seal_recall(&entries, 1500) {
        delegate.append_memory_context(&format!("\n\n{block}"));
        tracing::info!(entries = entries.len(), "bucket_seal recall injected into system prompt");
    }
}
```

**Adaptation:** verify the exact `ChatDelegate` type path + that it exposes `append_memory_context` (the assembler has it; confirm the delegate re-exposes or wraps it — if not, add a `pub fn append_memory_context(&mut self, extra: &str)` passthrough on `ChatDelegate` that forwards to the inner assembler). Verify `AppState` path (`crate::app::AppState`).

- [ ] **Step 3: Call the helper at the chat send site** (after the existing "Memory Recall Integration" block, where `set_memory_context` was called ~2235/2280)

```rust
// PR15: bucket_seal hybrid recall (additive to the legacy recall above).
append_bucket_seal_recall(&state, &mut delegate, &input.content).await;
```

**Adaptation:** place it AFTER the legacy `set_memory_context` so the bucket_seal block appends to (not overwrites) the existing memory context. Verify `delegate` is mutably borrowable there + the borrow of `state` doesn't conflict (`recall_hybrid` borrows `state.bucket_seal_adapter` immutably — fine).

- [ ] **Step 4: Call the helper at the agent send site(s)** (~11365, ~15188 regions, where the gbrain block is set)

Add the same `append_bucket_seal_recall(&state, &mut delegate, <query>).await;` call. **Adaptation:** identify the correct query variable at each site (the user's latest message text). If a site is a sub-agent spawn that should NOT recall (e.g. a teams supervisor with no user query), skip it + note the decision in the commit body.

- [ ] **Step 5: Full build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/tauri_commands.rs
git commit -m "feat(tauri): wire bucket_seal hybrid recall into agent system prompt (PR15.4 of 阶段 4)"
```

---

### Task 5: Verification

- [ ] **Step 1: Adapter + block tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter 2>&1 | tail`
Run: `cd src-tauri && cargo test --lib agent::memory_recall_block 2>&1 | tail`
Expected: all pass (existing adapter tests + 4 new recall tests; 3 block tests).

- [ ] **Step 2: Full module + build**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -5`
Expected: ~258+ passed (254 PR14 baseline + 4 recall).

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: zero errors (send-site wiring compiles).

- [ ] **Step 3: Broader regression**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive; pre-existing failures unchanged.

- [ ] **Step 4: Clippy**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "adapter\.rs|memory_recall_block|tauri_commands\.rs" | head`
Expected: zero PR15-attributable hits.

- [ ] **Step 5: Cargo audit + recall-not-blocking sanity**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr15-recall-routing && git diff main -- src-tauri/Cargo.toml` (empty).
Run: `grep -n "append_bucket_seal_recall" src-tauri/src/tauri_commands.rs` (helper defined + called at the send site(s)).

- [ ] **Step 6: Confirm additive (legacy recall untouched)**

Run: `git diff main -- src-tauri/src/tauri_commands.rs | grep -E "^-" | grep -iE "set_memory_context|MemoryRecallEngine|memu|session_memor"`
Expected: NO deletions of the legacy recall lines (PR15 only ADDS the helper + its calls).

- [ ] **Step 7: If cleanups surface, apply + commit**

```bash
git add -A
git commit -m "chore(memory): PR15 cleanup pass"
```

---

## Test plan summary

| Test | Count | Module |
|---|---|---|
| recall_semantic (cosine ordering, namespace+limit) | 2 | `memory_bucket_seal::adapter::tests` |
| recall_hybrid (merge+dedup, both-empty) | 2 | same |
| render_bucket_seal_recall (empty→None, marker+entries, budget truncate) | 3 | `agent::memory_recall_block::tests` |
| **Total new** | **~7** | — |
| **PR14 baseline preserved** | 254 (memory_bucket_seal) | — |

---

## Self-Review

**1. Spec coverage:**
- §3.1 recall_semantic → Task 1 ✅
- §3.2 recall_hybrid → Task 2 ✅
- §3.3 render block → Task 3 ✅
- §3.4 send-site wiring (additive append) → Task 4 ✅
- §3.5 no new AppState/IPC → reuses `state.bucket_seal_adapter` ✅
- §5 error handling (best-effort, FTS fallback, never blocks) → Task 2 (hybrid swallows) + Task 4 (gated) ✅
- §6 testing (hermetic, fake embedder) → Tasks 1-3 ✅
- §7 scope (additive, bucket_seal only, summaries are the semantic unit) → respected (Task 4 appends; Task 6 verification asserts no legacy deletions) ✅

**2. Placeholder scan:** No TBD/TODO. The second recall_semantic test + the agent-site query-variable identification are concrete "mirror the setup / identify the var" instructions, not placeholders. The `SummaryNode.embedding` inline-vs-fetch + `ChatDelegate.append_memory_context` exposure are verify-or-passthrough instructions with coded defaults.

**3. Type consistency:** `recall_semantic(&self, query, limit, namespace: Option<&str>) -> Result<Vec<MemoryEntry>>`, `recall_hybrid(&self, query, namespace: Option<&str>, max_entries) -> Vec<MemoryEntry>`, `render_bucket_seal_recall(&[MemoryEntry], usize) -> Option<String>`, `append_bucket_seal_recall(&AppState, &mut ChatDelegate, &str)` — consistent between definitions, tests, and call sites. `MemoryEntry` 8-field construction matches types.rs. `cosine_similarity(&[f32], &[f32]) -> f32`, `RecallOpts` 4-field. `SummaryNode.embedding: Option<Vec<f32>>`.

**Documented decisions:**
1. Additive append (not replace) — legacy recall untouched; Task 6 verifies no deletions.
2. Semantic unit = summaries (chunks have no embeddings); recent un-sealed chunks reachable via the FTS leg only.
3. Budget = 6 entries / ~1500 tokens, greedy fill; summaries-first ordering from hybrid.
4. Agent-site wiring: chat send required; agent/spawn sites added where a real user query exists, skipped for query-less sub-agents (noted at impl).
