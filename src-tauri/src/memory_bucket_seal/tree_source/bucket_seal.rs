// SPDX-License-Identifier: Apache-2.0
//! Append + cascade-seal for summary trees (openhuman port — Phase 3a).
//!
//! `append_leaf` pushes a persisted chunk into the L0 buffer of a tree.
//! Seal gates differ by level:
//!
//! - **L0 (leaves → L1)**: seal when `token_sum >= INPUT_TOKEN_BUDGET` OR
//!   `item_ids.len() >= SUMMARY_FANOUT`. Bounds the summariser's raw input
//!   and handles sources with individually small chunks.
//! - **L≥1 (summaries → next level)**: seal when
//!   `item_ids.len() >= SUMMARY_FANOUT`. Per-summary token size depends on
//!   summariser quality, so a token-based gate collapses to a 1:1:1 chain
//!   when the summariser is weak. Counting siblings keeps the tree's fan-in
//!   stable regardless.
//!
//! When a buffer seals, its items move into the new summary's `child_ids`,
//! the buffer clears, and the new summary id is queued at the next level.
//! The cascade continues upward until a buffer fails its gate.
//!
//! Concurrency: Phase 3a assumes a single-process SQLite workspace. All DB
//! writes in one seal step run in a single transaction; the async summariser
//! and embedder calls happen outside any open transaction so a slow call
//! doesn't hold DB locks.

use std::collections::BTreeSet;
use std::sync::Arc;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::memory_bucket_seal::score::embed::{pack_checked, Embedder};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::registry::new_summary_id;
use crate::memory_bucket_seal::tree_source::store;
use crate::memory_bucket_seal::tree_source::summariser::{Summariser, SummaryContext, SummaryInput};
use crate::memory_bucket_seal::tree_source::types::{
    Buffer, SummaryNode, Tree, INPUT_TOKEN_BUDGET, OUTPUT_TOKEN_BUDGET, SUMMARY_FANOUT,
};
use crate::memory_bucket_seal::types::approx_token_count;

/// Hard cap on cascade depth — prevents runaway loops if token accounting
/// ever slips. 32 levels at even a 2x fan-in is more than enough for any
/// realistic source.
const MAX_CASCADE_DEPTH: u32 = 32;

/// How a sealed summary node's `entities` and `topics` fields get populated.
///
/// - **`UnionFromChildren`**: dedup-merge each input's `entities` and `topics`
///   into the parent. Used by Global trees where inputs are already-labeled
///   source-tree summaries.
/// - **`Empty`**: leave both fields empty regardless of inputs. Used by Topic
///   trees (scope already pins the dominant theme) and as the default in
///   Phase 3a source trees until the LLM summariser lands.
#[derive(Clone)]
pub enum LabelStrategy {
    /// Dedup-merge each input's `entities` and `topics` into the parent.
    UnionFromChildren,
    /// Leave both fields empty regardless of inputs.
    Empty,
}

impl std::fmt::Debug for LabelStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnionFromChildren => f.write_str("UnionFromChildren"),
            Self::Empty => f.write_str("Empty"),
        }
    }
}

/// Resolve `entities` and `topics` for a freshly-summarised node according
/// to the chosen strategy.
async fn resolve_labels(
    strategy: &LabelStrategy,
    inputs: &[SummaryInput],
    _summary_content: &str,
) -> Result<(Vec<String>, Vec<String>)> {
    match strategy {
        LabelStrategy::UnionFromChildren => {
            let mut entities: BTreeSet<String> = BTreeSet::new();
            let mut topics: BTreeSet<String> = BTreeSet::new();
            for inp in inputs {
                for e in &inp.entities {
                    entities.insert(e.clone());
                }
                for t in &inp.topics {
                    topics.insert(t.clone());
                }
            }
            Ok((entities.into_iter().collect(), topics.into_iter().collect()))
        }
        LabelStrategy::Empty => Ok((Vec::new(), Vec::new())),
    }
}

/// A single leaf being appended to an L0 buffer.
#[derive(Clone, Debug)]
pub struct LeafRef {
    pub chunk_id: String,
    pub token_count: u32,
    pub timestamp: DateTime<Utc>,
    pub content: String,
    pub entities: Vec<String>,
    pub topics: Vec<String>,
    pub score: f32,
}

/// Append a leaf to the source tree for `tree`, sealing buffers as they
/// fill. Returns the ids of any summaries that sealed during this call.
///
/// **Concurrency contract**: callers MUST serialise `append_leaf` calls per
/// tree. The seal flow uses two transactions (buffer-read + summary-write)
/// with an `.await` for summariser+embedder between them; a concurrent
/// `append_leaf` for the same `tree.id` can slip a leaf into the buffer
/// during that window, and the subsequent `clear_buffer_tx` will wipe it.
/// PR9's BucketSealAdapter handles this by per-tree mutex or job-queue
/// serialisation; direct callers MUST match.
///
/// `strategy` controls how each sealed summary's `entities` and `topics`
/// are populated — see [`LabelStrategy`].
pub async fn append_leaf(
    store: &BucketSealStore,
    tree: &Tree,
    leaf: &LeafRef,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    strategy: &LabelStrategy,
) -> Result<Vec<String>> {
    tracing::debug!(
        tree_id = %tree.id,
        leaf_id = %leaf.chunk_id,
        tokens = leaf.token_count,
        strategy = ?strategy,
        "[tree_source::bucket_seal] append_leaf"
    );

    // 1. Push leaf into L0 buffer (transactional).
    append_to_buffer(
        store,
        &tree.id,
        0,
        &leaf.chunk_id,
        leaf.token_count as i64,
        leaf.timestamp,
    )?;

    // 2. Cascade seals upward until a level stays under budget.
    cascade_all_from(store, tree, 0, summariser, embedder, None, strategy).await
}

/// Queue-oriented variant of [`append_leaf`].
///
/// Only appends the leaf to the L0 buffer and returns whether the caller
/// should enqueue a follow-up seal job for level 0.
///
/// **Concurrency contract**: shares the same inter-transaction gap as
/// [`append_leaf`] — see its doc for the full hazard. The job queue that
/// calls [`cascade_all_from`] on the returned `true` signal MUST be
/// serialised per tree to prevent a concurrent append from losing a leaf
/// when the seal clears the buffer.
pub fn append_leaf_deferred(store: &BucketSealStore, tree: &Tree, leaf: &LeafRef) -> Result<bool> {
    append_to_buffer(
        store,
        &tree.id,
        0,
        &leaf.chunk_id,
        leaf.token_count as i64,
        leaf.timestamp,
    )?;
    let buf = store::get_buffer(store, &tree.id, 0)?;
    Ok(should_seal(&buf))
}

/// Transactionally append a single item to `(tree_id, level)`'s buffer.
/// Idempotent on `(tree_id, level, item_id)`: a retry after a failed cascade
/// is a no-op, so duplicated children and double-counted tokens can't slip
/// into the buffer. `oldest_at` stays on first-seen.
fn append_to_buffer(
    store: &BucketSealStore,
    tree_id: &str,
    level: u32,
    item_id: &str,
    token_delta: i64,
    item_ts: DateTime<Utc>,
) -> Result<()> {
    let mut conn = store.lock_conn()?;
    let tx = conn.transaction()?;
    let mut buf = store::get_buffer_conn(&tx, tree_id, level)?;
    if buf.item_ids.iter().any(|existing| existing == item_id) {
        tracing::debug!(
            item_id = %item_id,
            tree_id = %tree_id,
            level = level,
            "[tree_source::bucket_seal] append_to_buffer: item already in buffer — no-op"
        );
        return Ok(());
    }
    buf.item_ids.push(item_id.to_string());
    buf.token_sum = buf.token_sum.saturating_add(token_delta);
    buf.oldest_at = match buf.oldest_at {
        Some(existing) => Some(existing.min(item_ts)),
        None => Some(item_ts),
    };
    store::upsert_buffer_tx(&tx, &buf)?;
    tx.commit()?;
    Ok(())
}

/// Level-aware seal gate.
///
/// L0 buffers gate on `token_sum >= INPUT_TOKEN_BUDGET` OR sibling count
/// `>= SUMMARY_FANOUT`. L≥1 buffers gate on sibling count alone.
pub(crate) fn should_seal(buf: &Buffer) -> bool {
    if buf.is_empty() {
        return false;
    }
    if buf.level == 0 {
        buf.token_sum >= INPUT_TOKEN_BUDGET as i64 || (buf.item_ids.len() as u32) >= SUMMARY_FANOUT
    } else {
        (buf.item_ids.len() as u32) >= SUMMARY_FANOUT
    }
}

/// Seal buffers starting at `start_level` and cascade upward. When
/// `force_now` is `Some`, the buffer at `start_level` is sealed regardless
/// of token budget (used by time-based flush). Upper levels are sealed only
/// when they cross the budget.
///
/// Returns the ids of all summaries sealed during this call.
///
/// **Concurrency contract**: shares the same inter-transaction gap as
/// [`append_leaf`] — between the buffer snapshot read and the write
/// transaction that clears it, a concurrent `append_leaf` for the same
/// tree can insert a leaf that is then wiped by `clear_buffer_tx`. Callers
/// MUST serialise both `append_leaf` and `cascade_all_from` per tree.
pub async fn cascade_all_from(
    store: &BucketSealStore,
    tree: &Tree,
    start_level: u32,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    force_now: Option<DateTime<Utc>>,
    strategy: &LabelStrategy,
) -> Result<Vec<String>> {
    let mut sealed_ids: Vec<String> = Vec::new();
    let mut level: u32 = start_level;
    let mut first_iteration = true;

    for _ in 0..MAX_CASCADE_DEPTH {
        let buf = store::get_buffer(store, &tree.id, level)?;
        let forced = first_iteration && force_now.is_some();
        first_iteration = false;

        if !forced && !should_seal(&buf) {
            tracing::debug!(
                tree_id = %tree.id,
                stop_level = level,
                token_sum = buf.token_sum,
                "[tree_source::bucket_seal] cascade done"
            );
            break;
        }
        if buf.is_empty() {
            tracing::debug!(
                tree_id = %tree.id,
                level = level,
                "[tree_source::bucket_seal] cascade hit empty buffer — stopping"
            );
            break;
        }

        let summary_id =
            seal_one_level(store, tree, &buf, summariser, embedder, strategy).await?;
        sealed_ids.push(summary_id);
        level += 1;
    }

    Ok(sealed_ids)
}

/// Seal `buf` at `level` into one summary at `level + 1`. Returns the new
/// summary id.
///
/// Algorithm:
/// 1. Hydrate `SummaryInput`s from buffer item_ids.
/// 2. Compute time range + max score across children.
/// 3. Call `summariser.summarise()` — async, no DB lock held.
/// 4. Call `resolve_labels()` for entities/topics.
/// 5. Call `embedder.embed()` — async, no DB lock held. Abort on failure.
/// 6. In a single transaction: insert summary, clear buffer, append summary
///    to parent buffer, update tree `max_level`/`root_id`/`last_sealed_at`.
pub(crate) async fn seal_one_level(
    store: &BucketSealStore,
    tree: &Tree,
    buf: &Buffer,
    summariser: &Arc<dyn Summariser>,
    embedder: &Arc<dyn Embedder>,
    strategy: &LabelStrategy,
) -> Result<String> {
    let level = buf.level;
    let target_level = level + 1;

    // Hydrate inputs (synchronous DB reads — no lock held across await).
    let inputs = hydrate_inputs(store, level, &buf.item_ids)?;
    if inputs.is_empty() {
        anyhow::bail!(
            "[tree_source::bucket_seal] refused to seal empty buffer tree_id={} level={}",
            tree.id,
            level
        );
    }

    // Compute envelope across children (time range, max score).
    let time_range_start = inputs
        .iter()
        .map(|i| i.time_range_start)
        .min()
        .unwrap_or_else(Utc::now);
    let time_range_end = inputs
        .iter()
        .map(|i| i.time_range_end)
        .max()
        .unwrap_or_else(Utc::now);
    let score = inputs
        .iter()
        .map(|i| i.score)
        .fold(f32::NEG_INFINITY, f32::max)
        .max(0.0);

    // Run summariser — async, no DB lock held.
    let ctx = SummaryContext {
        tree_id: &tree.id,
        tree_kind: tree.kind,
        target_level,
        token_budget: OUTPUT_TOKEN_BUDGET,
    };
    let output = summariser
        .summarise(&inputs, &ctx)
        .await
        .context("summariser failed during seal")?;

    // Resolve labels for the new summary node.
    let (node_entities, node_topics) =
        resolve_labels(strategy, &inputs, &output.content).await?;

    // Embed the summary BEFORE opening the write tx so an embedder failure
    // aborts the seal cleanly — buffer stays intact, retry re-embeds.
    let embed_input = truncate_for_embed(&output.content, 1_000);
    let embedding = embedder
        .embed(&embed_input)
        .await
        .with_context(|| {
            format!(
                "embed summary during seal tree_id={} level={}",
                tree.id, level
            )
        })?;
    // Validate dimension before persisting — catches misbehaving embedders
    // before we open the write transaction.
    pack_checked(&embedding).with_context(|| {
        format!(
            "pack embedding for summary tree_id={} level={}",
            tree.id, level
        )
    })?;

    tracing::debug!(
        tree_id = %tree.id,
        level = level,
        target_level = target_level,
        content_len = output.content.len(),
        embed_dim = embedding.len(),
        "[tree_source::bucket_seal] embedded summary"
    );

    // Build the new summary node.
    let now = Utc::now();
    let summary_id = new_summary_id(target_level);
    let node = SummaryNode {
        id: summary_id.clone(),
        tree_id: tree.id.clone(),
        tree_kind: tree.kind,
        level: target_level,
        parent_id: None,
        child_ids: buf.item_ids.clone(),
        content: output.content,
        token_count: output.token_count,
        entities: node_entities,
        topics: node_topics,
        time_range_start,
        time_range_end,
        score,
        sealed_at: now,
        deleted: false,
        embedding: Some(embedding),
    };

    // Single write transaction: insert summary, clear this level's buffer,
    // append summary id to parent buffer, update tree metadata.
    {
        let mut conn = store.lock_conn()?;
        let tx = conn.transaction()?;

        // Re-read max_level inside the tx so cascading seals see updated value.
        let current_max: u32 = tx
            .query_row(
                "SELECT max_level FROM mem_tree_trees WHERE id = ?1",
                rusqlite::params![&tree.id],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n.max(0) as u32)
            .context("Failed to read current max_level for tree")?;

        store::insert_summary_tx(&tx, &node)?;

        // Backlink L≥1 summary children → new parent.
        // NOTE: L0 children are `mem_tree_chunks` rows. The `parent_summary_id`
        // column for chunks is deferred to PR9 (BucketSealAdapter wiring), so
        // we only backlink L≥1 summary→summary here.
        if level > 0 {
            for child_id in &node.child_ids {
                tx.execute(
                    "UPDATE mem_tree_summaries
                        SET parent_id = ?1
                      WHERE id = ?2 AND parent_id IS NULL",
                    rusqlite::params![&summary_id, child_id],
                )
                .context("Failed to backlink summary to parent")?;
            }
        }

        store::clear_buffer_tx(&tx, &tree.id, level)?;

        // Append to parent buffer.
        let mut parent = store::get_buffer_conn(&tx, &tree.id, target_level)?;
        parent.item_ids.push(summary_id.clone());
        parent.token_sum = parent
            .token_sum
            .saturating_add(node.token_count as i64);
        parent.oldest_at = match parent.oldest_at {
            Some(existing) => Some(existing.min(time_range_start)),
            None => Some(time_range_start),
        };
        store::upsert_buffer_tx(&tx, &parent)?;

        // Update tree root/max_level if we just climbed.
        if target_level > current_max {
            store::update_tree_after_seal_tx(&tx, &tree.id, &summary_id, target_level, now)?;
        } else {
            store::refresh_last_sealed_tx(&tx, &tree.id, now)?;
        }

        tx.commit()?;
    }

    tracing::info!(
        tree_id = %tree.id,
        level = level,
        target_level = target_level,
        summary_id = %summary_id,
        children = buf.item_ids.len(),
        "[tree_source::bucket_seal] sealed"
    );

    Ok(summary_id)
}

/// Clamp `text` to roughly `max_tokens` tokens before passing to the
/// embedder. Uses the same ~4 chars/token heuristic as `approx_token_count`.
fn truncate_for_embed(text: &str, max_tokens: u32) -> String {
    let approx = approx_token_count(text);
    if approx <= max_tokens {
        return text.to_string();
    }
    let char_ceiling = (max_tokens as usize).saturating_mul(4);
    text.chars().take(char_ceiling).collect()
}

/// Fetch contributions for `item_ids`. At level 0 we pull from
/// `mem_tree_chunks`; at ≥1 we pull from `mem_tree_summaries`.
fn hydrate_inputs(
    store: &BucketSealStore,
    level: u32,
    item_ids: &[String],
) -> Result<Vec<SummaryInput>> {
    if level == 0 {
        hydrate_leaf_inputs(store, item_ids)
    } else {
        hydrate_summary_inputs(store, item_ids)
    }
}

fn hydrate_leaf_inputs(store: &BucketSealStore, chunk_ids: &[String]) -> Result<Vec<SummaryInput>> {
    let mut out: Vec<SummaryInput> = Vec::with_capacity(chunk_ids.len());
    for id in chunk_ids {
        let chunk = match store.get_chunk(id)? {
            Some(c) => c,
            None => {
                tracing::warn!(
                    chunk_id = %id,
                    "[tree_source::bucket_seal] hydrate_leaf_inputs: missing chunk — skipping"
                );
                continue;
            }
        };
        // TODO(PR9): hydrate full body via content_store::read.
        //
        // `chunk.content` here is the ≤500-char plain-text preview stored in the
        // `mem_tree_chunks.content` column. The full body lives at `content_path`
        // on disk (PR5 atomic-write target). InertSummariser tolerates this since
        // it just concat+truncates; when PR12 wires the real LLM summariser the
        // full-body read must be in place to avoid silent quality degradation.
        //
        // Reading the body here requires porting openhuman's
        // `content_store/read.rs` (~480 LoC) — out of PR8 scope. Track on the PR9
        // BucketSealAdapter checklist as a hard blocker before swapping in
        // LlmSummariser.
        out.push(SummaryInput {
            id: chunk.id.clone(),
            content: chunk.content.clone(),
            token_count: chunk.token_count,
            entities: Vec::new(), // PR8: no entity index yet (deferred to extract port)
            topics: chunk.metadata.tags.clone(),
            time_range_start: chunk.metadata.time_range.0,
            time_range_end: chunk.metadata.time_range.1,
            score: 0.0, // PR8: score lookup deferred — BucketSealAdapter wires this in PR9
        });
    }
    Ok(out)
}

fn hydrate_summary_inputs(
    store: &BucketSealStore,
    summary_ids: &[String],
) -> Result<Vec<SummaryInput>> {
    let mut out: Vec<SummaryInput> = Vec::with_capacity(summary_ids.len());
    for id in summary_ids {
        let node = match store::get_summary(store, id)? {
            Some(n) => n,
            None => {
                tracing::warn!(
                    summary_id = %id,
                    "[tree_source::bucket_seal] hydrate_summary_inputs: missing summary — skipping"
                );
                continue;
            }
        };
        out.push(SummaryInput {
            id: node.id.clone(),
            content: node.content.clone(),
            token_count: node.token_count,
            entities: node.entities.clone(),
            topics: node.topics.clone(),
            time_range_start: node.time_range_start,
            time_range_end: node.time_range_end,
            score: node.score,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::{InertEmbedder, EMBEDDING_DIM};
    use crate::memory_bucket_seal::store::BucketSealStore;
    use crate::memory_bucket_seal::tree_source::registry::get_or_create_source_tree;
    use crate::memory_bucket_seal::tree_source::summariser::inert::InertSummariser;
    use crate::memory_bucket_seal::tree_source::types::{
        SummaryNode, TreeKind, TreeStatus, INPUT_TOKEN_BUDGET, SUMMARY_FANOUT,
    };
    use crate::memory_bucket_seal::{stage_chunks, Chunk, Metadata, SourceKind};
    use chrono::{TimeZone, Utc};
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let s = BucketSealStore::open(&db_path).unwrap();
        s.ensure_schema().unwrap();
        (s, dir)
    }

    fn mk_summariser() -> Arc<dyn Summariser> {
        Arc::new(InertSummariser::new())
    }

    fn mk_embedder() -> Arc<dyn Embedder> {
        Arc::new(InertEmbedder::new())
    }

    fn mk_leaf(chunk_id: &str, tokens: u32, ts_ms: i64) -> LeafRef {
        LeafRef {
            chunk_id: chunk_id.to_string(),
            token_count: tokens,
            timestamp: Utc.timestamp_millis_opt(ts_ms).single().unwrap(),
            content: format!("content for {chunk_id}"),
            entities: vec![],
            topics: vec![],
            score: 0.5,
        }
    }

    /// Upsert a chunk into the store and return a LeafRef for it.
    fn seed_chunk(
        store: &BucketSealStore,
        dir: &TempDir,
        seq: u32,
        tokens: u32,
    ) -> LeafRef {
        let ts = Utc
            .timestamp_millis_opt(1_700_000_000_000 + seq as i64 * 1000)
            .unwrap();
        let chunk = Chunk {
            id: format!("chunk_{seq:04}"),
            content: format!("chunk content for seq {seq}"),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec![],
                source_ref: None,
            },
            token_count: tokens,
            seq_in_source: seq,
            created_at: ts,
            partial_message: false,
        };
        let staged = stage_chunks(dir.path(), &[chunk.clone()]).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
        LeafRef {
            chunk_id: chunk.id,
            token_count: tokens,
            timestamp: ts,
            content: chunk.content,
            entities: vec![],
            topics: vec![],
            score: 0.5,
        }
    }

    // ── Basic buffer accounting ──────────────────────────────────────────

    #[tokio::test]
    async fn append_below_budget_does_not_seal() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = mk_leaf("leaf-1", 100, 1_700_000_000_000);
        let sealed = append_leaf(
            &store,
            &tree,
            &leaf,
            &mk_summariser(),
            &mk_embedder(),
            &LabelStrategy::Empty,
        )
        .await
        .unwrap();
        assert!(sealed.is_empty(), "under budget — no seal expected");

        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids, vec!["leaf-1".to_string()]);
        assert_eq!(buf.token_sum, 100);
        assert_eq!(store::count_summaries(&store, &tree.id).unwrap(), 0);
    }

    #[tokio::test]
    async fn append_to_buffer_is_idempotent() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = mk_leaf("leaf-1", 100, 1_700_000_000_000);
        // Append same leaf twice — second call must be a no-op.
        append_to_buffer(&store, &tree.id, 0, "leaf-1", 100, leaf.timestamp).unwrap();
        append_to_buffer(&store, &tree.id, 0, "leaf-1", 100, leaf.timestamp).unwrap();
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert_eq!(buf.item_ids.len(), 1, "idempotent: only one entry");
        assert_eq!(buf.token_sum, 100, "idempotent: tokens counted once");
    }

    // ── Budget-triggered L0→L1 seal ──────────────────────────────────────

    #[tokio::test]
    async fn crossing_budget_triggers_l1_seal() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser = mk_summariser();
        let embedder = mk_embedder();

        // Two leaves that together exceed INPUT_TOKEN_BUDGET.
        let per_leaf = INPUT_TOKEN_BUDGET * 6 / 10;
        let leaf1 = seed_chunk(&store, &dir, 0, per_leaf);
        let leaf2 = seed_chunk(&store, &dir, 1, per_leaf);

        let first = append_leaf(&store, &tree, &leaf1, &summariser, &embedder, &LabelStrategy::Empty)
            .await
            .unwrap();
        assert!(first.is_empty(), "first append below budget — no seal");

        let second = append_leaf(&store, &tree, &leaf2, &summariser, &embedder, &LabelStrategy::Empty)
            .await
            .unwrap();
        assert_eq!(second.len(), 1, "second append crosses budget — one seal");

        let summary_id = &second[0];
        let summary = store::get_summary(&store, summary_id).unwrap().unwrap();
        assert_eq!(summary.level, 1);
        assert_eq!(summary.child_ids.len(), 2);
        assert!(summary.token_count > 0);
        assert!(
            summary.embedding.is_some(),
            "PR8 summaries must have embedding populated"
        );
        assert_eq!(summary.embedding.as_ref().unwrap().len(), EMBEDDING_DIM);

        // L0 buffer cleared, L1 buffer carries the new summary id.
        let l0 = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert!(l0.is_empty(), "L0 buffer must clear after seal");
        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert_eq!(l1.item_ids, vec![summary_id.clone()]);

        // Tree metadata updated.
        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 1);
        assert_eq!(t.root_id.as_deref(), Some(summary_id.as_str()));
        assert!(t.last_sealed_at.is_some());
    }

    // ── Fanout-triggered L1→L2 cascade ───────────────────────────────────

    #[tokio::test]
    async fn fanout_at_l1_triggers_l2_seal() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser = mk_summariser();
        let embedder = mk_embedder();

        // Each leaf busts INPUT_TOKEN_BUDGET alone → every append fires an L1 seal.
        // After SUMMARY_FANOUT seals the L1 buffer hits the fanout gate → L2 seal.
        let per_leaf = INPUT_TOKEN_BUDGET + 1;
        let mut all_sealed: Vec<String> = Vec::new();
        for seq in 0..SUMMARY_FANOUT {
            let leaf = seed_chunk(&store, &dir, seq, per_leaf);
            let sealed =
                append_leaf(&store, &tree, &leaf, &summariser, &embedder, &LabelStrategy::Empty)
                    .await
                    .unwrap();
            all_sealed.extend(sealed);
        }

        // SUMMARY_FANOUT L1 seals + 1 L2 seal.
        assert_eq!(
            all_sealed.len() as u32,
            SUMMARY_FANOUT + 1,
            "expected {} L1 seals + 1 L2 seal, got {}",
            SUMMARY_FANOUT,
            all_sealed.len()
        );

        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 2, "tree should have climbed to L2");

        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert!(l1.is_empty(), "L1 buffer must clear when fanout seal fires");

        let l2 = store::get_buffer(&store, &tree.id, 2).unwrap();
        assert_eq!(l2.item_ids.len(), 1, "exactly one L2 summary queued");

        let l2_summary = store::get_summary(&store, &l2.item_ids[0]).unwrap().unwrap();
        assert_eq!(l2_summary.level, 2);
        assert_eq!(
            l2_summary.child_ids.len() as u32,
            SUMMARY_FANOUT,
            "L2 summary should fold all {SUMMARY_FANOUT} L1 children"
        );
    }

    // ── Upper level does not seal below fanout ────────────────────────────

    #[tokio::test]
    async fn upper_level_does_not_seal_below_fanout() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser = mk_summariser();
        let embedder = mk_embedder();

        let stop_before = SUMMARY_FANOUT.saturating_sub(1);
        let per_leaf = INPUT_TOKEN_BUDGET + 1;
        for seq in 0..stop_before {
            let leaf = seed_chunk(&store, &dir, seq, per_leaf);
            let _ = append_leaf(&store, &tree, &leaf, &summariser, &embedder, &LabelStrategy::Empty)
                .await
                .unwrap();
        }

        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert_eq!(t.max_level, 1, "should plateau at L1 below fanout");

        let l1 = store::get_buffer(&store, &tree.id, 1).unwrap();
        assert_eq!(
            l1.item_ids.len() as u32,
            stop_before,
            "L1 buffer should hold the unsealed siblings"
        );
        assert_eq!(
            store::count_summaries(&store, &tree.id).unwrap(),
            stop_before as u64
        );
    }

    // ── LabelStrategy tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn seal_with_empty_strategy_leaves_labels_empty() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = seed_chunk(&store, &dir, 0, INPUT_TOKEN_BUDGET + 1);
        let sealed = append_leaf(
            &store,
            &tree,
            &leaf,
            &mk_summariser(),
            &mk_embedder(),
            &LabelStrategy::Empty,
        )
        .await
        .unwrap();
        assert_eq!(sealed.len(), 1);
        let summary = store::get_summary(&store, &sealed[0]).unwrap().unwrap();
        assert!(
            summary.entities.is_empty(),
            "Empty strategy must leave entities empty"
        );
        assert!(
            summary.topics.is_empty(),
            "Empty strategy must leave topics empty"
        );
    }

    #[tokio::test]
    async fn seal_with_union_strategy_inherits_labels_from_children() {
        // UnionFromChildren operates on SummaryInput.entities/topics. L0 chunk
        // hydration cannot carry entities (entity index deferred to PR9), so we
        // test the strategy at L1→L2 by pre-inserting two L1 summaries that
        // carry labels, then triggering a fanout-based seal at L1.
        let (store, _dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let summariser = mk_summariser();
        let embedder = mk_embedder();

        let now = Utc::now();

        // Build SUMMARY_FANOUT L1 nodes — the last two have distinct entities/topics.
        let mut l1_ids: Vec<String> = Vec::new();
        for i in 0..SUMMARY_FANOUT {
            let sid = format!("s1-test-{i:04}");
            let entities = if i == SUMMARY_FANOUT - 2 {
                vec!["email:alice@example.com".into(), "topic:phoenix".into()]
            } else if i == SUMMARY_FANOUT - 1 {
                vec!["email:alice@example.com".into(), "person:bob".into()]
            } else {
                vec![]
            };
            let topics = if i == SUMMARY_FANOUT - 2 {
                vec!["phoenix".into(), "launch".into()]
            } else if i == SUMMARY_FANOUT - 1 {
                vec!["launch".into(), "qa".into()]
            } else {
                vec![]
            };
            let node = SummaryNode {
                id: sid.clone(),
                tree_id: tree.id.clone(),
                tree_kind: tree.kind,
                level: 1,
                parent_id: None,
                child_ids: vec![format!("chunk-placeholder-{i}")],
                content: format!("summary content {i}"),
                token_count: 100,
                entities,
                topics,
                time_range_start: now,
                time_range_end: now,
                score: 0.5,
                sealed_at: now,
                deleted: false,
                embedding: None,
            };
            {
                let mut conn = store.lock_conn().unwrap();
                let tx = conn.transaction().unwrap();
                store::insert_summary_tx(&tx, &node).unwrap();
                tx.commit().unwrap();
            }
            l1_ids.push(sid);
        }

        // Manually fill the L1 buffer to fanout — this triggers an L1→L2 seal.
        for sid in &l1_ids {
            append_to_buffer(&store, &tree.id, 1, sid, 100, now).unwrap();
        }

        // cascade_all_from at level 1 — should fire exactly one L2 seal.
        let sealed = cascade_all_from(
            &store,
            &tree,
            1,
            &summariser,
            &embedder,
            None,
            &LabelStrategy::UnionFromChildren,
        )
        .await
        .unwrap();
        assert_eq!(sealed.len(), 1, "expected one L2 seal; got {}", sealed.len());

        let summary = store::get_summary(&store, &sealed[0]).unwrap().unwrap();
        let entities: BTreeSet<&str> = summary.entities.iter().map(String::as_str).collect();
        let topics: BTreeSet<&str> = summary.topics.iter().map(String::as_str).collect();
        assert!(entities.contains("email:alice@example.com"), "missing alice entity");
        assert!(entities.contains("topic:phoenix"), "missing phoenix entity");
        assert!(entities.contains("person:bob"), "missing bob entity");
        assert_eq!(entities.len(), 3, "expected 3 unique entities; got {entities:?}");
        assert!(topics.contains("phoenix"));
        assert!(topics.contains("launch"));
        assert!(topics.contains("qa"));
        assert_eq!(topics.len(), 3, "expected 3 unique topics; got {topics:?}");
    }

    // ── Tree metadata after seal ──────────────────────────────────────────

    #[tokio::test]
    async fn tree_metadata_updated_after_seal() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = seed_chunk(&store, &dir, 0, INPUT_TOKEN_BUDGET + 1);
        let sealed = append_leaf(
            &store,
            &tree,
            &leaf,
            &mk_summariser(),
            &mk_embedder(),
            &LabelStrategy::Empty,
        )
        .await
        .unwrap();
        assert_eq!(sealed.len(), 1);
        let t = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert!(t.last_sealed_at.is_some(), "last_sealed_at should be set");
        assert_eq!(t.max_level, 1);
        assert!(t.root_id.is_some());
    }

    // ── Topic tree kind preserved ─────────────────────────────────────────

    #[tokio::test]
    async fn topic_tree_seal_persists_topic_kind_not_source() {
        let (store, dir) = fresh_store();
        let tree = Tree {
            id: "topic-tree-test-id".to_string(),
            kind: TreeKind::Topic,
            scope: "topic:launch".to_string(),
            root_id: None,
            max_level: 0,
            status: TreeStatus::Active,
            created_at: Utc::now(),
            last_sealed_at: None,
        };
        store::insert_tree(&store, &tree).unwrap();

        let leaf = seed_chunk(&store, &dir, 0, INPUT_TOKEN_BUDGET + 1);
        let sealed = append_leaf(
            &store,
            &tree,
            &leaf,
            &mk_summariser(),
            &mk_embedder(),
            &LabelStrategy::Empty,
        )
        .await
        .unwrap();
        assert_eq!(sealed.len(), 1);

        let summary = store::get_summary(&store, &sealed[0]).unwrap().unwrap();
        assert_eq!(
            summary.tree_kind,
            TreeKind::Topic,
            "topic-tree summary must persist tree_kind=Topic, not Source"
        );
    }

    // ── append_leaf_deferred ──────────────────────────────────────────────

    #[test]
    fn append_leaf_deferred_returns_false_below_budget() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = mk_leaf("leaf-1", 100, 1_700_000_000_000);
        let should = append_leaf_deferred(&store, &tree, &leaf).unwrap();
        assert!(!should, "under budget — should not trigger deferred seal");
    }

    #[test]
    fn append_leaf_deferred_returns_true_over_budget() {
        let (store, _dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        // Leaf token_count alone exceeds budget.
        let leaf = mk_leaf("leaf-1", INPUT_TOKEN_BUDGET + 1, 1_700_000_000_000);
        let should = append_leaf_deferred(&store, &tree, &leaf).unwrap();
        assert!(should, "over budget — should trigger deferred seal");
    }

    // ── Embedding validation ──────────────────────────────────────────────

    #[tokio::test]
    async fn inert_embedder_produces_1024_zero_vector() {
        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "slack:#eng").unwrap();
        let leaf = seed_chunk(&store, &dir, 0, INPUT_TOKEN_BUDGET + 1);
        let sealed = append_leaf(
            &store,
            &tree,
            &leaf,
            &mk_summariser(),
            &mk_embedder(),
            &LabelStrategy::Empty,
        )
        .await
        .unwrap();
        assert_eq!(sealed.len(), 1);
        let summary = store::get_summary(&store, &sealed[0]).unwrap().unwrap();
        let emb = summary.embedding.unwrap();
        assert_eq!(emb.len(), EMBEDDING_DIM);
        assert!(emb.iter().all(|&v| v == 0.0), "InertEmbedder must return all zeros");
    }

    // ── should_seal logic ─────────────────────────────────────────────────

    #[test]
    fn should_seal_empty_buffer_is_false() {
        let buf = Buffer::empty("t1", 0);
        assert!(!should_seal(&buf));
    }

    #[test]
    fn should_seal_l0_on_token_budget() {
        let buf = Buffer {
            tree_id: "t1".into(),
            level: 0,
            item_ids: vec!["a".into()],
            token_sum: INPUT_TOKEN_BUDGET as i64,
            oldest_at: None,
        };
        assert!(should_seal(&buf));
    }

    #[test]
    fn should_seal_l0_on_fanout() {
        let item_ids: Vec<String> = (0..SUMMARY_FANOUT).map(|i| format!("x{i}")).collect();
        let buf = Buffer {
            tree_id: "t1".into(),
            level: 0,
            item_ids,
            token_sum: 1, // well below token budget
            oldest_at: None,
        };
        assert!(should_seal(&buf));
    }

    #[test]
    fn should_seal_l1_only_on_fanout_not_tokens() {
        // L1 buffer with many tokens but only 1 item — should NOT seal.
        let buf = Buffer {
            tree_id: "t1".into(),
            level: 1,
            item_ids: vec!["s1".into()],
            token_sum: INPUT_TOKEN_BUDGET as i64 * 100,
            oldest_at: None,
        };
        assert!(!should_seal(&buf));

        // L1 buffer at fanout — should seal.
        let item_ids: Vec<String> = (0..SUMMARY_FANOUT).map(|i| format!("s{i}")).collect();
        let buf2 = Buffer {
            tree_id: "t1".into(),
            level: 1,
            item_ids,
            token_sum: 0,
            oldest_at: None,
        };
        assert!(should_seal(&buf2));
    }

    // ── Embedder failure aborts seal cleanly ──────────────────────────────

    /// Verifies that an embedder failure during seal aborts the seal cleanly:
    /// no summary is written, the buffer is NOT cleared, and the error
    /// surfaces to the caller. Mirrors openhuman's same-named test.
    #[tokio::test]
    async fn embedder_failure_aborts_seal_cleanly() {
        use async_trait::async_trait;

        // A deliberately-failing embedder.
        struct FailingEmbedder;
        #[async_trait]
        impl crate::memory_bucket_seal::score::embed::Embedder for FailingEmbedder {
            fn name(&self) -> &'static str {
                "failing"
            }
            async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
                anyhow::bail!("simulated embedder transport error")
            }
        }

        let (store, dir) = fresh_store();
        let tree = get_or_create_source_tree(&store, "test:#failing").unwrap();
        let summariser = mk_summariser();
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(FailingEmbedder);

        // Push enough leaves to cross INPUT_TOKEN_BUDGET so a seal fires.
        // Each leaf is slightly over half the budget so two together cross it.
        let per_leaf = INPUT_TOKEN_BUDGET * 6 / 10;
        let mut last_err: Option<anyhow::Error> = None;
        for seq in 0..10_u32 {
            let leaf = seed_chunk(&store, &dir, seq, per_leaf);
            match append_leaf(
                &store,
                &tree,
                &leaf,
                &summariser,
                &embedder,
                &LabelStrategy::Empty,
            )
            .await
            {
                Ok(_) => continue,
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }

        // 1. The seal triggered by the threshold-crossing leaf must have failed.
        let err = last_err.expect("embedder failure must surface as Err from append_leaf");
        let msg = format!("{:#}", err);
        assert!(
            msg.contains("embedder") || msg.contains("embed") || msg.contains("simulated"),
            "error message should reference the embedder failure; got: {msg}"
        );

        // 2. No summary should have been written.
        assert_eq!(
            store::count_summaries(&store, &tree.id).unwrap(),
            0,
            "embedder failure must NOT write a summary row"
        );

        // 3. Buffer at L0 must still hold the leaves (not cleared by the failed seal).
        let buf = store::get_buffer(&store, &tree.id, 0).unwrap();
        assert!(
            !buf.item_ids.is_empty(),
            "L0 buffer must still hold leaves after failed seal"
        );
        assert!(
            buf.token_sum >= INPUT_TOKEN_BUDGET as i64,
            "buffer token_sum must still be at or above INPUT_TOKEN_BUDGET after failed seal; got {}",
            buf.token_sum
        );

        // 4. Tree's last_sealed_at and root_id must remain None (no successful seal).
        let refreshed = store::get_tree(&store, &tree.id).unwrap().unwrap();
        assert!(
            refreshed.last_sealed_at.is_none(),
            "tree.last_sealed_at must remain None after failed seal"
        );
        assert!(
            refreshed.root_id.is_none(),
            "tree.root_id must remain None after failed seal"
        );
    }
}
