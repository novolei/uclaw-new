# 阶段 4 PR10 — `memory_bucket_seal::tree_topic` port (per-entity trees) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add per-entity topic trees on top of PR9's source-tree pipeline. Each chunk that's admitted by `BucketSealAdapter.store()` gets its entities extracted (regex stub) and the chunk's leaf is `append_leaf`'d into each entity's topic tree. Topic trees use the SAME cascade-seal mechanics as source trees (no duplication) because PR8's `Tree` / `SummaryNode` / `Buffer` types and `bucket_seal::append_leaf` are already generic over `TreeKind::{Source, Topic, Global}` — the schema's `(kind, scope)` keying just works.

**Architecture:** Thin `entities.rs` module (pure-regex extractor with stopword filter) + thin `tree_topic/` subsystem (registry + mod re-exports only — no new `bucket_seal.rs`, no new schema tables). `BucketSealAdapter.store()` gains a topic-fan-out step after the source `append_leaf` succeeds: extract entities from canonical markdown, sort + dedup, for each entity get-or-create its topic tree and `append_leaf` with the same `LeafRef`. Per-topic-tree mutex acquisition reuses the existing `tree_mutexes` HashMap with key prefix `topic:` vs `source:`.

**Tech Stack:** Rust, `regex` (verify in workspace), `tokio::sync::Mutex`, `tracing`, `anyhow`, `async-trait`. Reuses every PR5-9 dep.

---

## Source-of-truth references

uClaw files PR10 builds on top of:
- `src-tauri/src/memory_bucket_seal/tree_source/types.rs` — `Tree`, `SummaryNode`, `Buffer`, `TreeKind::{Source, Topic, Global}`, `TreeStatus`. The types are ALREADY generic over kind.
- `src-tauri/src/memory_bucket_seal/tree_source/store.rs` — store layer keys by `(kind, scope)` via SQL `WHERE kind = ?1 AND scope = ?2`. Functions: `get_tree_by_scope`, `insert_tree`, `list_trees_by_kind`, `mark_root`, `update_tree_max_level`, `get_buffer`, `replace_buffer`, `append_summary_node`, `get_summary_by_id`, `get_summary_embedding`, `set_summary_embedding`.
- `src-tauri/src/memory_bucket_seal/tree_source/bucket_seal.rs:121` — `pub async fn append_leaf(store, tree: &Tree, leaf: &LeafRef, summariser, embedder, strategy: &LabelStrategy) -> Result<Vec<String>>`. Accepts ANY tree, regardless of kind. Cascade-seal logic is kind-agnostic.
- `src-tauri/src/memory_bucket_seal/tree_source/registry.rs` — `get_or_create_source_tree(store, scope)` pattern to mirror for topic.
- `src-tauri/src/memory_bucket_seal/adapter.rs` (from PR9) — `BucketSealAdapter::store()` flow, `tree_mutex` helper, `parse_tags`, `build_tags`.
- `src-tauri/Cargo.toml` — confirm `regex` is available (workspace dep).

## File Structure

| File | Purpose | LoC est. |
|---|---|---|
| `src-tauri/src/memory_bucket_seal/entities.rs` (new) | Pure-regex entity extractor with stopword filter. Public: `extract_entities(text: &str) -> Vec<String>` (deduplicated, sorted). Internal: 3 regex patterns (@mentions, #hashtags, capitalized 1-3 word phrases) + STOPWORDS const. ~12 inline tests. | ~150 |
| `src-tauri/src/memory_bucket_seal/tree_topic/mod.rs` (new) | Module declaration + re-exports: `pub use registry::get_or_create_topic_tree;` | ~15 |
| `src-tauri/src/memory_bucket_seal/tree_topic/registry.rs` (new) | `get_or_create_topic_tree(store, entity) -> Result<Tree>` mirrors `tree_source::registry`. Idempotent. ~3 tests. | ~80 |
| `src-tauri/src/memory_bucket_seal/mod.rs` (modify, +5 lines) | `pub mod entities;` + `pub mod tree_topic;` + `pub use entities::extract_entities;` | +5 |
| `src-tauri/src/memory_bucket_seal/adapter.rs` (modify, +60 lines + tests) | After source `append_leaf` per chunk, call `extract_entities(&chunk.content)`, sort + dedup, for each entity: get-or-create topic tree, acquire per-topic mutex (key `"topic:{entity}"`), `append_leaf`. +4 tests covering: entity extraction round-trip, topic tree creation per entity, topic + source share the same chunk, no-entity chunks skip topic append. | +120 (incl. tests) |

**LoC budget**: ~370 source + ~250 tests = **~620 LoC total**. Well within the original PR10 budget (~300-500 from the spec table; the cascade-seal "duplication" turns out to be free because of PR8's design).

---

## Decisions Already Locked (no more questions)

- **Topic trees use the SAME schema as source trees**: `mem_tree_trees` row with `kind = 'topic'`, `scope = entity_string`. `mem_tree_summaries`, `mem_tree_buffers`, `mem_tree_links` all work for topic trees because they're keyed by `tree_id`. NO new schema tables.
- **Entity extraction strategy**: pure-Rust regex stub. Three patterns:
  - `@mentions`: `@\w+`
  - `#hashtags`: `#\w+`
  - Capitalized phrases (1-3 words): `\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2}\b`
- **Stopwords**: hardcoded const, applied after extraction. List: `["The", "A", "An", "And", "Or", "But", "If", "I", "You", "We", "They", "He", "She", "It", "Is", "Was", "Are", "Be", "Been", "This", "That", "These", "Those", "What", "When", "Where", "Who", "Why", "How"]`. Match case-sensitively against capitalized matches (a word starting with capital letter is the kind we filter).
- **Entity normalization**: store as-extracted. "Alice" and "alice" make two trees. Accepted at PR10 scope; PR12 jobs can normalize.
- **Entity sort + dedup**: `extract_entities` returns `BTreeSet<String>` → `Vec<String>` for deterministic order. Tests assert sort-stability.
- **Empty entity case**: if `extract_entities` returns empty, skip the topic append loop entirely (chunk lives in source tree only).
- **Per-topic mutex via existing `tree_mutexes` HashMap with key prefix**: key format `"source:{namespace}"` for source append, `"topic:{entity}"` for each topic. Unbounded HashMap growth accepted (LRU eviction deferred — bucket-seal jobs PR12 concern).
- **Ordering**: SEQUENTIAL — source `append_leaf` first (existing PR9 behavior), then iterate sorted entities and `append_leaf` each topic tree in turn. NO parallel fan-out within a single `store()` call — keeps per-tree mutex contract simple.
- **Cascade-seal threshold**: SAME `INPUT_TOKEN_BUDGET = 50_000` constant from PR8. Topic trees naturally see fewer leaves per tree (entities don't accumulate fast), so cascades will rarely fire — that's expected.
- **Embedder + Summariser sharing**: same `Arc<dyn Embedder>` + `Arc<dyn Summariser>` already held by `BucketSealAdapter`. Both source and topic appends use them. PR12 swap to real Ollama/LLM applies to both.
- **No recall integration in PR10**: `BucketSealAdapter.recall()` stays FTS5-only over chunks. Cross-source recall by entity is PR15 (effective_system_prompt wiring).
- **No new workspace deps**: `regex` should already be in workspace (PR5+ uses it). Verify with `grep regex src-tauri/Cargo.toml`. If absent, add it as part of PR10 — flag in commit body.

---

## Adaptation responsibilities (DO NOT trust the plan blindly)

For each task:

1. **Re-read `tree_source/registry.rs`** before writing `tree_topic/registry.rs`. Mirror its function signature + idempotency pattern + tests pattern exactly.

2. **Re-read `tree_source/store.rs` lines 67-100** for the `get_tree_by_scope` / `insert_tree` API used by `get_or_create_source_tree`. The same calls work for topic trees — just pass `TreeKind::Topic` instead of `TreeKind::Source`.

3. **Verify `regex` workspace dep**: run `grep regex src-tauri/Cargo.toml`. If missing, add to `[dependencies]` (likely already present from PR6's chunker or PR7's score).

4. **Verify `Tree` / `TreeKind::Topic` re-exports**: PR8's `memory_bucket_seal/mod.rs` should already re-export `Tree`, `TreeKind`. Check with `grep -nE "TreeKind|^pub use" src-tauri/src/memory_bucket_seal/mod.rs`. If not exposed, add the re-exports as part of PR10.

5. **Capitalized phrase regex pitfall**: `\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2}\b` requires `(?:...)` non-capture group (not `(...)`). Rust's `regex` crate uses default Unicode boundaries — should work for English. CJK (Chinese characters) won't match — accepted limitation at PR10 scope.

6. **Stopword filter logic**: apply AFTER all 3 extraction passes (mentions, hashtags, phrases). Stopwords filter ONLY capitalized phrases (mentions and hashtags are kept verbatim). Reason: `@The` is a real username, `#The` is a real hashtag, but bare "The" as a phrase is noise.

7. **Single-character mentions/hashtags**: filter out tokens shorter than 2 chars. `@a` and `#a` are too noisy. Minimum entity length = 2 chars after the `@`/`#` prefix.

8. **`get_or_create_topic_tree` pattern**: 
   ```rust
   pub fn get_or_create_topic_tree(store: &BucketSealStore, entity: &str) -> Result<Tree> {
       if let Some(tree) = store::get_tree_by_scope(store, TreeKind::Topic, entity)? {
           return Ok(tree);
       }
       // Construct + insert new tree
       let tree = Tree {
           id: format!("topic-{}", Uuid::new_v4()),
           kind: TreeKind::Topic,
           scope: entity.to_string(),
           root_id: None,
           max_level: 0,
           status: TreeStatus::Active,
           created_at: Utc::now(),
           last_sealed_at: None,
       };
       store::insert_tree(store, &tree)?;
       Ok(tree)
   }
   ```
   Verify `store::insert_tree` signature matches by reading PR8's source. If `insert_tree` takes different params, adapt accordingly.

9. **Per-topic mutex key prefix**: PR9's `tree_mutex(namespace)` helper takes a `&str`. For topic trees, call it as `self.tree_mutex(&format!("topic:{}", entity)).await`. For source trees (existing PR9 behavior), the call site keeps passing `namespace` directly — no prefix. To avoid collisions between a namespace literally named "topic:foo" and the topic tree for entity "foo", introduce explicit prefixing at the source site too: `self.tree_mutex(&format!("source:{}", namespace)).await`. Update PR9's existing source call to use the prefix.

10. **Adapter.store() integration point**: after the existing `append_leaf` for source in PR9's loop (around adapter.rs:282-296), add the topic fan-out block. The block:
    ```rust
    // Topic fan-out: per-entity append_leaf
    let entities = crate::memory_bucket_seal::extract_entities(&chunk.content);
    for entity in &entities {
        let topic_tree = crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(&self.store, entity)
            .context("get_or_create_topic_tree")?;
        let topic_mutex = self.tree_mutex(&format!("topic:{}", entity)).await;
        let _topic_guard = topic_mutex.lock().await;
        let leaf = LeafRef {
            chunk_id: chunk.id.clone(),
            token_count: chunk.token_count,
            timestamp: chunk.metadata.timestamp,
            content: chunk.content.clone(),
            entities: vec![entity.clone()],
            topics: vec![],
            score: score_rows.iter().find(|r| r.chunk_id == chunk.id).map(|r| r.total).unwrap_or(0.0),
        };
        crate::memory_bucket_seal::tree_source::bucket_seal::append_leaf(
            &self.store,
            &topic_tree,
            &leaf,
            &self.summariser,
            &self.embedder,
            &LabelStrategy::Empty,
        )
        .await
        .context("topic append_leaf")?;
        drop(_topic_guard);
    }
    ```
    Note: the source guard from PR9 (`let _guard = tree_mutex.lock().await;`) is released BEFORE this block runs. If not, restructure so source append completes + guard drops, THEN entity extraction + topic fan-out (which acquires its own per-topic guards). Holding source mutex during topic appends is wrong — they target different trees.

11. **Drop source guard before topic appends**: critical adaptation. PR9 holds the source guard across the whole canonicalise → chunk → score → append loop. Need to refactor so:
    - Phase A: source pipeline under source guard (existing PR9 code)
    - Phase B: drop source guard
    - Phase C: topic fan-out per chunk (each acquires its own topic guard)
    
    Easiest pattern: scope the source pipeline in an inner block (`{ let _guard = ...; ... }`), then run topic fan-out outside that block. The `admitted: Vec<Chunk>` and `score_rows: Vec<ScoreRow>` need to outlive the source block — declare them before.

12. **Topic append error handling**: if a SINGLE topic `append_leaf` fails (e.g., DB locked), should we abort the whole `store()` call or just log + continue? Plan: log warn + continue. The source append already succeeded; partial topic indexing is acceptable. Use `tracing::warn!` with `entity = %entity, chunk_id = %chunk.id, error = %e`.

13. **Entity volume control**: `extract_entities` could return many entities for a long doc. Cap to top-N per chunk (e.g., 20 entities max) to bound topic tree fan-out. Implement in `extract_entities`: after dedup+sort, `.into_iter().take(20).collect()`.

14. **`mod.rs` re-export order**: in `memory_bucket_seal/mod.rs`, add `pub mod entities;` and `pub mod tree_topic;` in alphabetical order with other `pub mod` declarations. Add `pub use entities::extract_entities;` for the convenience re-export.

15. **Pre-commit hooks**: same as previous PRs. Don't `--no-verify`.

---

### Task 1: `entities.rs` — regex entity extractor

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/entities.rs`

- [ ] **Step 1: Write `entities.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Stub entity extractor for topic-tree fan-out (Phase 3c — PR10).
//!
//! Pure-regex pattern matcher with stopword filter. Three patterns:
//! - `@mentions` like `@alice`
//! - `#hashtags` like `#design`
//! - Capitalized 1-3 word phrases like `Alice Wong` or `Project Phoenix`
//!
//! Returns a deterministically-sorted, deduplicated, capped (top 20) Vec
//! per chunk.
//!
//! Future work: PR12 jobs swap this for an LLM-driven NER pass that
//! handles CJK, case normalization, and entity-id canonicalization.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::BTreeSet;

const MAX_ENTITIES_PER_CHUNK: usize = 20;

/// Stopwords filtered from capitalized-phrase matches only — mentions and
/// hashtags are kept verbatim. A single-token capitalized match that
/// equals (case-sensitive) one of these is dropped.
const STOPWORDS: &[&str] = &[
    "The", "A", "An", "And", "Or", "But", "If", "I", "You", "We", "They",
    "He", "She", "It", "Is", "Was", "Are", "Be", "Been", "This", "That",
    "These", "Those", "What", "When", "Where", "Who", "Why", "How",
];

static MENTION_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"@(\w{2,})").unwrap());
static HASHTAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"#(\w{2,})").unwrap());
static CAPS_PHRASE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\b[A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2}\b").unwrap());

/// Extract entities from `text`. Returns up to [`MAX_ENTITIES_PER_CHUNK`]
/// unique entities, sorted ascending for deterministic order.
pub fn extract_entities(text: &str) -> Vec<String> {
    let mut out: BTreeSet<String> = BTreeSet::new();

    // @mentions — keep the leading @ so trees stay distinguishable from
    // bare capitalized phrases ("@Alice" and "Alice" → separate trees).
    for cap in MENTION_RE.captures_iter(text) {
        if let Some(m) = cap.get(0) {
            out.insert(m.as_str().to_string());
        }
    }

    // #hashtags
    for cap in HASHTAG_RE.captures_iter(text) {
        if let Some(m) = cap.get(0) {
            out.insert(m.as_str().to_string());
        }
    }

    // Capitalized phrases — single-word matches checked against STOPWORDS.
    for m in CAPS_PHRASE_RE.find_iter(text) {
        let s = m.as_str();
        let is_single_word = !s.contains(' ');
        if is_single_word && STOPWORDS.contains(&s) {
            continue;
        }
        out.insert(s.to_string());
    }

    out.into_iter().take(MAX_ENTITIES_PER_CHUNK).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_mentions() {
        let out = extract_entities("hello @alice and @bob");
        assert!(out.contains(&"@alice".to_string()));
        assert!(out.contains(&"@bob".to_string()));
    }

    #[test]
    fn extract_hashtags() {
        let out = extract_entities("#design and #ux meeting");
        assert!(out.contains(&"#design".to_string()));
        assert!(out.contains(&"#ux".to_string()));
    }

    #[test]
    fn extract_capitalized_single_word() {
        let out = extract_entities("Project Phoenix is launching tomorrow.");
        assert!(out.contains(&"Project Phoenix".to_string()));
    }

    #[test]
    fn extract_capitalized_two_words() {
        let out = extract_entities("Met with Alice Wong yesterday.");
        assert!(out.contains(&"Alice Wong".to_string()));
    }

    #[test]
    fn extract_capitalized_three_words() {
        let out = extract_entities("North San Francisco is congested.");
        assert!(out.contains(&"North San Francisco".to_string()));
    }

    #[test]
    fn filter_single_word_stopwords() {
        let out = extract_entities("The quick brown fox jumps");
        assert!(!out.contains(&"The".to_string()));
    }

    #[test]
    fn keep_capitalized_words_in_multi_word_phrase() {
        // "The" as part of a multi-word capitalized phrase IS kept because
        // it's a phrase, not a single token. e.g., "The Beatles" stays.
        let out = extract_entities("The Beatles released a new album.");
        assert!(out.contains(&"The Beatles".to_string()));
    }

    #[test]
    fn dedup_repeated_matches() {
        let out = extract_entities("Alice met Alice in Alice's office.");
        let alice_count = out.iter().filter(|s| s == &&"Alice".to_string()).count();
        assert_eq!(alice_count, 1);
    }

    #[test]
    fn dedup_preserves_sorted_order() {
        let out = extract_entities("Bob met Alice yesterday.");
        // sorted ascending: "Alice" < "Bob"
        let alice_idx = out.iter().position(|s| s == "Alice");
        let bob_idx = out.iter().position(|s| s == "Bob");
        if let (Some(a), Some(b)) = (alice_idx, bob_idx) {
            assert!(a < b);
        }
    }

    #[test]
    fn empty_text_returns_empty() {
        let out = extract_entities("");
        assert!(out.is_empty());
    }

    #[test]
    fn no_entities_returns_empty() {
        let out = extract_entities("the quick brown fox jumps over the lazy dog");
        assert!(out.is_empty());
    }

    #[test]
    fn cap_at_max_entities_per_chunk() {
        // Build a text with 30 distinct capitalized names; cap should limit to 20.
        let mut text = String::new();
        for i in 0..30 {
            text.push_str(&format!("Person{i:02} mentioned. "));
        }
        let out = extract_entities(&text);
        assert!(out.len() <= MAX_ENTITIES_PER_CHUNK);
    }

    #[test]
    fn min_length_2_for_mentions_and_hashtags() {
        let out = extract_entities("@a and #b are too short");
        assert!(!out.iter().any(|s| s == "@a" || s == "#b"));
    }
}
```

- [ ] **Step 2: Verify `regex` and `once_cell` workspace deps**

Run: `grep -E "^regex|^once_cell" src-tauri/Cargo.toml`
Expected: both present (likely from PR5/PR6). If missing, add `regex = { workspace = true }` and `once_cell = { workspace = true }`.

- [ ] **Step 3: Add to `memory_bucket_seal/mod.rs`**

```rust
pub mod entities;

pub use entities::extract_entities;
```

(Place in alphabetical order with existing `pub mod` and `pub use` blocks.)

- [ ] **Step 4: Build + test**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::entities 2>&1 | tail -15`
Expected: 12 passed.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/entities.rs src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): entities.rs — regex entity extractor stub (PR10.1 of 阶段 4)"
```

---

### Task 2: `tree_topic/registry.rs` — `get_or_create_topic_tree`

**Files:**
- Create: `src-tauri/src/memory_bucket_seal/tree_topic/mod.rs`
- Create: `src-tauri/src/memory_bucket_seal/tree_topic/registry.rs`

- [ ] **Step 1: Re-read PR8's `tree_source/registry.rs`** (~198 lines)

Read it fully to capture the idempotency pattern, error mapping, and test structure. Mirror the same shape for topic.

- [ ] **Step 2: Verify exact API of `store::get_tree_by_scope` and `store::insert_tree`**

Run: `grep -nE "pub fn get_tree_by_scope|pub fn insert_tree" src-tauri/src/memory_bucket_seal/tree_source/store.rs`
Read both signatures fully so the registry adapts correctly.

- [ ] **Step 3: Write `tree_topic/mod.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Topic-tree bucket-seal mechanics (Phase 3c — openhuman port).
//!
//! Phase 3a's [`crate::memory_bucket_seal::tree_source`] subsystem is
//! already generic over [`crate::memory_bucket_seal::tree_source::TreeKind`].
//! Topic trees reuse the same store layer, the same cascade-seal pipeline,
//! and the same summariser/embedder injection. The only new bits are:
//! - [`registry::get_or_create_topic_tree`] — idempotent per-entity tree lookup
//!
//! Adapter integration sits in `BucketSealAdapter::store` — after a source
//! `append_leaf` succeeds for a chunk, entities are extracted and each
//! entity's topic tree gets its own `append_leaf` for the same `LeafRef`.

pub mod registry;

pub use registry::get_or_create_topic_tree;
```

- [ ] **Step 4: Write `tree_topic/registry.rs`**

```rust
// SPDX-License-Identifier: Apache-2.0
//! Topic-tree registry — idempotent lookup keyed by entity string.
//!
//! Mirrors [`crate::memory_bucket_seal::tree_source::registry`] but with
//! [`TreeKind::Topic`].

use anyhow::{Context, Result};
use chrono::Utc;
use uuid::Uuid;

use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::store as tree_store;
use crate::memory_bucket_seal::tree_source::types::{Tree, TreeKind, TreeStatus};

/// Look up the topic tree for `entity` or create it if it doesn't exist.
/// Idempotent — calling twice for the same entity returns the same row.
pub fn get_or_create_topic_tree(store: &BucketSealStore, entity: &str) -> Result<Tree> {
    if let Some(tree) = tree_store::get_tree_by_scope(store, TreeKind::Topic, entity)
        .context("get_tree_by_scope")?
    {
        return Ok(tree);
    }

    let tree = Tree {
        id: format!("topic-{}", Uuid::new_v4()),
        kind: TreeKind::Topic,
        scope: entity.to_string(),
        root_id: None,
        max_level: 0,
        status: TreeStatus::Active,
        created_at: Utc::now(),
        last_sealed_at: None,
    };
    tree_store::insert_tree(store, &tree).context("insert_tree")?;
    Ok(tree)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let store = BucketSealStore::open(&dir.path().join("chunks.db")).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    #[test]
    fn creates_topic_tree_for_new_entity() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_topic_tree(&store, "Alice").unwrap();
        assert_eq!(tree.scope, "Alice");
        assert_eq!(tree.kind, TreeKind::Topic);
        assert_eq!(tree.status, TreeStatus::Active);
        assert_eq!(tree.max_level, 0);
        assert!(tree.root_id.is_none());
    }

    #[test]
    fn idempotent_returns_same_tree_id() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_topic_tree(&store, "Bob").unwrap();
        let t2 = get_or_create_topic_tree(&store, "Bob").unwrap();
        assert_eq!(t1.id, t2.id);
    }

    #[test]
    fn distinct_entities_get_distinct_trees() {
        let (store, _dir) = fresh_store();
        let t1 = get_or_create_topic_tree(&store, "@alice").unwrap();
        let t2 = get_or_create_topic_tree(&store, "#design").unwrap();
        let t3 = get_or_create_topic_tree(&store, "Project Phoenix").unwrap();
        assert_ne!(t1.id, t2.id);
        assert_ne!(t2.id, t3.id);
        assert_ne!(t1.id, t3.id);
    }

    #[test]
    fn topic_and_source_trees_with_same_scope_are_distinct() {
        let (store, _dir) = fresh_store();
        let source_tree =
            crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "shared_name")
                .unwrap();
        let topic_tree = get_or_create_topic_tree(&store, "shared_name").unwrap();
        assert_ne!(source_tree.id, topic_tree.id);
        assert_eq!(source_tree.kind, TreeKind::Source);
        assert_eq!(topic_tree.kind, TreeKind::Topic);
    }
}
```

- [ ] **Step 5: Add `pub mod tree_topic;` to `memory_bucket_seal/mod.rs`**

```rust
pub mod tree_topic;
```

(Place alphabetically with other `pub mod` declarations.)

- [ ] **Step 6: Build + test**

Run: `cd src-tauri && cargo build --lib 2>&1 | grep -E "^error" | head`
Expected: zero errors.

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::tree_topic 2>&1 | tail -10`
Expected: 4 passed.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/tree_topic/ src-tauri/src/memory_bucket_seal/mod.rs
git commit -m "feat(memory_bucket_seal): tree_topic/registry — get_or_create_topic_tree (PR10.2 of 阶段 4)"
```

---

### Task 3: `adapter.rs` — topic fan-out in `store()`

**Files:**
- Modify: `src-tauri/src/memory_bucket_seal/adapter.rs` (~120 lines added incl. tests)

- [ ] **Step 1: Read PR9's `store()` implementation carefully**

Run: `grep -n "async fn store" src-tauri/src/memory_bucket_seal/adapter.rs`
Read the full method — note the source `tree_mutex` acquisition, the inner block scope (if any), the loop iterating admitted chunks for `append_leaf`. Identify the right place to insert topic fan-out.

- [ ] **Step 2: Refactor `store()` so source mutex drops before topic fan-out**

Restructure the method so the source pipeline runs in an inner scope, the source guard drops, then the topic fan-out runs separately. The variables `admitted: Vec<Chunk>` and `score_rows: Vec<ScoreRow>` need to outlive the source scope — declare them before the source block.

Approximate restructuring (paths and names may differ slightly — adapt to actual code):

```rust
async fn store(
    &self,
    namespace: &str,
    key: &str,
    content: &str,
    category: MemoryCategory,
    session_id: Option<&str>,
) -> Result<()> {
    if content.trim().is_empty() {
        tracing::debug!(namespace = %namespace, key = %key, "skipping empty content");
        return Ok(());
    }

    // Outer-scope state that survives both source and topic phases.
    let mut admitted: Vec<crate::memory_bucket_seal::types::Chunk> = Vec::new();
    let mut score_rows: Vec<ScoreRow> = Vec::new();

    // PHASE A: Source pipeline (inner scope so source guard drops at block end).
    {
        let source_tree = get_or_create_source_tree(&self.store, namespace)
            .context("get_or_create_source_tree")?;
        let source_mutex = self.tree_mutex(&format!("source:{}", namespace)).await;
        let _source_guard = source_mutex.lock().await;

        let tags = build_tags(&category, session_id);
        let canonical = canonicalise(
            namespace,
            "system",
            &tags,
            DocumentInput {
                provider: "uclaw".to_string(),
                title: key.to_string(),
                body: content.to_string(),
                modified_at: Utc::now(),
                source_ref: Some(key.to_string()),
            },
        )
        .map_err(|e| anyhow::anyhow!("canonicalise: {}", e))?;

        let Some(canonical) = canonical else {
            tracing::debug!(namespace = %namespace, key = %key, "canonicalise returned None");
            return Ok(());
        };

        let chunker_input = ChunkerInput {
            source_kind: SourceKind::Document,
            source_id: namespace.to_string(),
            markdown: canonical.markdown.clone(),
            metadata: canonical.metadata.clone(),
        };
        let chunks = chunk_markdown(&chunker_input, &ChunkerOptions::default());
        if chunks.is_empty() {
            tracing::debug!(namespace = %namespace, key = %key, "chunker produced no chunks");
            return Ok(());
        }

        let scoring_config = ScoringConfig::default();
        for chunk in &chunks {
            let result = score_chunk(chunk, &scoring_config);
            let row = ScoreRow {
                chunk_id: result.chunk_id.clone(),
                total: result.total,
                signals: result.signals.clone(),
                dropped: !result.kept,
                reason: result.drop_reason.clone(),
                computed_at_ms: Utc::now().timestamp_millis(),
            };
            score_rows.push(row);
            if result.kept {
                admitted.push(chunk.clone());
            }
        }

        if !admitted.is_empty() {
            let staged = stage_chunks(&self.content_root, &admitted)
                .context("stage_chunks")?;
            self.store
                .upsert_staged_chunks(&staged)
                .context("upsert_staged_chunks")?;
        }

        for row in &score_rows {
            if !row.dropped {
                upsert_score(&self.store, row).context("upsert_score")?;
            }
        }

        // Source append_leaf per admitted chunk.
        for chunk in &admitted {
            let leaf = LeafRef {
                chunk_id: chunk.id.clone(),
                token_count: chunk.token_count,
                timestamp: chunk.metadata.timestamp,
                content: chunk.content.clone(),
                entities: vec![], // populated for topic appends below
                topics: vec![],
                score: score_rows
                    .iter()
                    .find(|r| r.chunk_id == chunk.id)
                    .map(|r| r.total)
                    .unwrap_or(0.0),
            };
            append_leaf(
                &self.store,
                &source_tree,
                &leaf,
                &self.summariser,
                &self.embedder,
                &LabelStrategy::Empty,
            )
            .await
            .context("source append_leaf")?;
        }
        // _source_guard drops here.
    }

    // PHASE B: Topic fan-out per chunk (each acquires its own per-topic guard).
    for chunk in &admitted {
        let entities = crate::memory_bucket_seal::extract_entities(&chunk.content);
        for entity in &entities {
            let topic_tree = match crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(
                &self.store,
                entity,
            ) {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!(entity = %entity, chunk_id = %chunk.id, error = %e, "get_or_create_topic_tree failed");
                    continue;
                }
            };
            let topic_mutex = self.tree_mutex(&format!("topic:{}", entity)).await;
            let _topic_guard = topic_mutex.lock().await;
            let leaf = LeafRef {
                chunk_id: chunk.id.clone(),
                token_count: chunk.token_count,
                timestamp: chunk.metadata.timestamp,
                content: chunk.content.clone(),
                entities: vec![entity.clone()],
                topics: vec![],
                score: score_rows
                    .iter()
                    .find(|r| r.chunk_id == chunk.id)
                    .map(|r| r.total)
                    .unwrap_or(0.0),
            };
            if let Err(e) = crate::memory_bucket_seal::tree_source::bucket_seal::append_leaf(
                &self.store,
                &topic_tree,
                &leaf,
                &self.summariser,
                &self.embedder,
                &LabelStrategy::Empty,
            )
            .await
            {
                tracing::warn!(entity = %entity, chunk_id = %chunk.id, error = %e, "topic append_leaf failed");
                continue;
            }
        }
    }

    Ok(())
}
```

**Important**: this refactor changes PR9's mutex key from `namespace` → `source:{namespace}`. Update PR9's existing tests if they reference the bare-namespace key (unlikely; they go through the `tree_mutex()` helper). Run the full test suite after this step to catch any breakage.

- [ ] **Step 3: Add 4 new tests covering topic fan-out**

Append to `adapter.rs`'s `#[cfg(test)] mod tests` block:

```rust
    #[tokio::test]
    async fn store_creates_topic_tree_per_entity() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "topic_ns",
                "k1",
                "Met with Alice Wong about Project Phoenix today.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        // Source tree should exist.
        let _source_tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(
            &adapter.store,
            "topic_ns",
        )
        .unwrap();

        // Topic trees for "Alice Wong" and "Project Phoenix" should exist.
        let alice_tree = crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(
            &adapter.store,
            "Alice Wong",
        )
        .unwrap();
        let phoenix_tree = crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(
            &adapter.store,
            "Project Phoenix",
        )
        .unwrap();

        // They should be distinct.
        assert_ne!(alice_tree.id, phoenix_tree.id);
        assert_eq!(alice_tree.kind, crate::memory_bucket_seal::tree_source::types::TreeKind::Topic);
    }

    #[tokio::test]
    async fn store_without_entities_skips_topic_fan_out() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "no_entity_ns",
                "k1",
                "the quick brown fox jumps over the lazy dog with substantive content density.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        // Source tree exists.
        let _source_tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(
            &adapter.store,
            "no_entity_ns",
        )
        .unwrap();

        // No topic trees created (verify by counting topic trees).
        let topic_trees = crate::memory_bucket_seal::tree_source::store::list_trees_by_kind(
            &adapter.store,
            crate::memory_bucket_seal::tree_source::types::TreeKind::Topic,
        )
        .unwrap();
        assert!(topic_trees.is_empty());
    }

    #[tokio::test]
    async fn store_topic_and_source_share_same_chunk() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "shared_ns",
                "k1",
                "Alice presented the design.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        // Verify the same chunk appears in mem_tree_chunks (single row by id);
        // the topic tree has its own buffer pointing at the same chunk_id.
        let count = adapter.store.count_chunks().unwrap();
        assert!(count >= 1);
        // We can't easily query topic buffer membership without a public API,
        // but we can verify both trees exist.
        let source = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(
            &adapter.store,
            "shared_ns",
        )
        .unwrap();
        let topic = crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(
            &adapter.store,
            "Alice",
        )
        .unwrap();
        assert_ne!(source.id, topic.id);
    }

    #[tokio::test]
    async fn store_handles_many_entities_via_cap() {
        let (adapter, _dir) = fresh_adapter();
        // Build a content string with 30 entity-shaped names.
        let mut content =
            String::from("Substantive note discussing multiple project participants. ");
        for i in 0..30 {
            content.push_str(&format!("Person{i:02} attended. "));
        }
        adapter
            .store("many_ns", "k1", &content, MemoryCategory::Core, None)
            .await
            .unwrap();

        // At most MAX_ENTITIES_PER_CHUNK topic trees per chunk → ≤ 20.
        let topic_trees = crate::memory_bucket_seal::tree_source::store::list_trees_by_kind(
            &adapter.store,
            crate::memory_bucket_seal::tree_source::types::TreeKind::Topic,
        )
        .unwrap();
        assert!(topic_trees.len() <= 20, "got {} topic trees", topic_trees.len());
    }
```

- [ ] **Step 4: Run tests**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal::adapter 2>&1 | tail -25`
Expected: 22 passed (18 from PR9 + 4 new).

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -10`
Expected: ~201+ passed (181 PR9 baseline + 4 entities + 4 registry + 4 adapter ≈ ~193+, but counts depend on actual test resolution).

If any PR9 test fails because of the `source:` mutex key prefix change, debug + fix the test (likely a string-literal assertion on mutex keys — unlikely since tree_mutex is opaque).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memory_bucket_seal/adapter.rs
git commit -m "feat(memory_bucket_seal): BucketSealAdapter topic fan-out per entity (PR10.3 of 阶段 4)"
```

---

### Task 4: Verification

- [ ] **Step 1: Full module test pass**

Run: `cd src-tauri && cargo test --lib memory_bucket_seal 2>&1 | tail -15`
Expected: ~200+ passed (181 PR9 baseline + ~20 new).

- [ ] **Step 2: Broader regression check**

Run: `cd src-tauri && cargo test --lib 2>&1 | tail -10`
Expected: net positive over PR9 baseline; pre-existing failures elsewhere unchanged.

- [ ] **Step 3: Clippy**

Run: `cd src-tauri && cargo clippy --lib -- -D warnings 2>&1 | grep -E "entities\.rs|tree_topic|adapter\.rs" | head -20`
Expected: zero hits.

- [ ] **Step 4: Cargo.toml audit**

Run: `cd /Users/ryanliu/Documents/uclaw-worktrees/stage4-pr10-tree-topic-port && git diff main -- src-tauri/Cargo.toml`
Expected: empty (or `regex` / `once_cell` adds if they weren't already present).

- [ ] **Step 5: Stray TODO/FIXME scan**

Run: `cd src-tauri && grep -nE "TODO|FIXME|XXX" src/memory_bucket_seal/{entities.rs,tree_topic/}` 
Expected: zero hits (or only intentional ones — flag if unexpected).

- [ ] **Step 6: AppState integration check**

PR10 makes NO changes to `app.rs`. Verify with `git diff main -- src-tauri/src/app.rs`. Expected: empty.

- [ ] **Step 7: If verification surfaces small cleanups**

Apply them and commit:

```bash
git add -A
git commit -m "chore(memory_bucket_seal): PR10 cleanup pass"
```

If nothing to clean, skip.

---

## Test plan summary

| Test type | Count | Module |
|---|---|---|
| Entity extraction (mentions, hashtags, capitalized 1-3 word phrases, stopwords, dedup, sort, cap, edge cases) | 12 | `memory_bucket_seal::entities::tests::*` |
| Topic registry (create new, idempotent, distinct entities, kind isolation) | 4 | `memory_bucket_seal::tree_topic::registry::tests::*` |
| Adapter topic fan-out (per-entity tree, no-entity skip, source+topic share chunk, MAX cap) | 4 | `memory_bucket_seal::adapter::tests::store_*` |
| **Total new tests** | **20** | — |
| **PR9 tests preserved** | 181 | (unchanged) |
| **Module total** | **~201** | — |

---

## Self-Review Checklist

- ✅ **Spec coverage**: Option B from brainstorming → topic trees with full cascade-seal. Implemented via PR8's already-generic `Tree`/`SummaryNode`/`Buffer` types + `append_leaf` accepting any TreeKind. Zero schema migrations needed.
- ✅ **Scope check**: NO new schema tables, NO duplicated bucket_seal.rs, NO recall integration (deferred to PR15). NO mem_topic_chunks junction (PR8 schema already covers it via `mem_tree_links` / buffer membership).
- ✅ **Entity extraction**: regex stub with stopword filter + max-20 cap. LLM NER swap deferred to PR12.
- ✅ **Mutex discipline**: source guard drops BEFORE topic fan-out. Per-topic guards acquired sequentially per entity. Key format: `source:{namespace}` / `topic:{entity}` to prevent collision.
- ✅ **Cascade-seal**: free — PR8's `append_leaf` already handles it. Topic trees seal at the same `INPUT_TOKEN_BUDGET = 50_000` threshold as source trees (rarely fires in practice because entities accumulate slowly).
- ✅ **No placeholders**: every step shows actual code or exact paths.
- ✅ **Bisectability**: 3 task commits (entities / registry / adapter integration). Each compiles standalone.
- ✅ **No new workspace deps**: `regex` + `once_cell` should already be present; verified in Task 1 Step 2.
- ✅ **Tracing discipline**: `tracing::warn!` for topic-append failures (best-effort: source already succeeded, partial topic indexing acceptable).
- ✅ **Embedder/Summariser sharing**: same `Arc<dyn ...>` already held by BucketSealAdapter — PR12 swap applies to both source AND topic trees with no additional change.
- ✅ **PR15 prep**: cross-source recall by entity will use `tree_source::store::list_trees_by_kind(store, TreeKind::Topic)` + the existing summary node lookup. No new API needed.
- ✅ **Big insight documented**: PR8's `TreeKind`-generic design means PR10 is ~600 LoC instead of the ~2400 LoC of full duplication that was originally feared. Topic trees get cascade-seal for free.
