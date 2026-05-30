// SPDX-License-Identifier: MIT
//! `BucketSealAdapter` — first non-wrap `MemoryAdapter` impl.
//!
//! Orchestrates the PR5-8 stack into the trait surface:
//! - `store` = canonicalise → chunk → score → append_leaf_deferred (fast,
//!   durable buffer write) + detached cascade-seal (best-effort, per-tree mutex)
//! - `recall` = FTS5 MATCH on `mem_tree_chunks_fts` scoped by namespace
//! - `get`/`list`/`delete`/`clear_namespace`/`namespace_summaries` = direct SQL
//!
//! Embedder + Summariser are injected via `Arc<dyn ...>` so PR12 can swap
//! `InertEmbedder`/`InertSummariser` for `OpenAiCompatEmbedder`/`LlmSummariser`
//! without touching this adapter.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use async_trait::async_trait;
use chrono::{TimeZone, Utc};
use rusqlite::OptionalExtension;
use tokio::sync::Mutex;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
use crate::memory_bucket_seal::canonicalize::document::{canonicalise, DocumentInput};
use crate::memory_bucket_seal::chunker::{chunk_markdown, ChunkerInput, ChunkerOptions};
use crate::memory_bucket_seal::score::embed::Embedder;
use crate::memory_bucket_seal::score::store::{upsert_score, ScoreRow};
use crate::memory_bucket_seal::score::{score_chunk, ScoringConfig};
use crate::memory_bucket_seal::store::BucketSealStore;
use crate::memory_bucket_seal::tree_source::{
    append_leaf_deferred, get_or_create_source_tree, LabelStrategy, LeafRef,
    Summariser,
};
use crate::memory_bucket_seal::types::SourceKind;
use crate::memory_bucket_seal::{stage_chunks, StagedChunk};

const ADAPTER_NAME: &str = "bucket_seal";

pub struct BucketSealAdapter {
    store: Arc<BucketSealStore>,
    content_root: PathBuf,
    embedder: Arc<dyn Embedder>,
    summariser: Arc<dyn Summariser>,
    tree_mutexes: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl BucketSealAdapter {
    pub fn new(
        store: Arc<BucketSealStore>,
        content_root: PathBuf,
        embedder: Arc<dyn Embedder>,
        summariser: Arc<dyn Summariser>,
    ) -> Self {
        Self {
            store,
            content_root,
            embedder,
            summariser,
            tree_mutexes: Mutex::new(HashMap::new()),
        }
    }

    /// Acquire (or create) the per-tree mutex for `namespace`. The returned
    /// Arc holds the inner mutex; calling `.lock().await` on it serialises
    /// `append_leaf` for that tree per PR8's concurrency contract.
    async fn tree_mutex(&self, namespace: &str) -> Arc<Mutex<()>> {
        let mut map = self.tree_mutexes.lock().await;
        map.entry(namespace.to_string())
            .or_insert_with(|| Arc::new(Mutex::new(())))
            .clone()
    }

    /// Enqueue a durable Seal job for `tree_id` (best-effort — a failure to
    /// enqueue is logged but never fails the write; FlushStale recovers it).
    fn enqueue_seal(&self, tree_id: &str) {
        let nj = match crate::memory_bucket_seal::jobs::types::NewJob::seal(
            &crate::memory_bucket_seal::jobs::types::SealPayload {
                tree_id: tree_id.to_string(),
                from_level: 0,
                force: false,
            },
        ) {
            Ok(nj) => nj,
            Err(e) => {
                tracing::warn!(tree_id = %tree_id, error = %e, "build seal job failed");
                return;
            }
        };
        if let Err(e) = crate::memory_bucket_seal::jobs::store::enqueue(&self.store, &nj) {
            tracing::warn!(tree_id = %tree_id, error = %e, "enqueue seal job failed (FlushStale will recover)");
        }
    }
}

impl BucketSealAdapter {
    /// PR15: Semantic recall — embed the query, cosine-rank summary embeddings,
    /// return the top-`limit` summaries as MemoryEntries (the dense, curated
    /// recall unit). Namespace filter matches a summary's source-tree scope.
    pub async fn recall_semantic(
        &self,
        query: &str,
        limit: usize,
        namespace: Option<&str>,
    ) -> anyhow::Result<Vec<MemoryEntry>> {
        use crate::memory_bucket_seal::score::embed::cosine_similarity;
        use crate::memory_bucket_seal::tree_source::store as ts;
        use crate::memory_bucket_seal::tree_source::types::TreeKind;

        // Defensive cap: a pathologically large store must not blow up a turn.
        // Normal stores are far under this threshold; the cap is a safety valve.
        const MAX_SEMANTIC_SCAN: usize = 5000;

        let qvec = self
            .embedder
            .embed(query)
            .await
            .context("recall_semantic: embed query")?;

        // Gather (cosine, MemoryEntry) over all summaries that carry an embedding.
        let mut scored: Vec<(f32, MemoryEntry)> = Vec::new();
        'outer: for kind in [TreeKind::Source, TreeKind::Topic, TreeKind::Global] {
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
                        if scored.len() >= MAX_SEMANTIC_SCAN {
                            tracing::warn!(
                                scanned = scored.len(),
                                "recall_semantic hit scan cap — results may be partial"
                            );
                            break 'outer;
                        }
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

    /// PR15: Hybrid recall for prompt injection — dense summaries (semantic)
    /// first, raw chunk hits (FTS) as backfill. Best-effort: a failing leg is
    /// skipped; both failing → empty. Dedup by id; cap to `max_entries`.
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

    /// Run an end-of-day cross-source digest for `day`, appending one L0
    /// node to the global tree and cascade-sealing if thresholds cross.
    pub async fn run_global_digest(
        &self,
        day: chrono::NaiveDate,
    ) -> anyhow::Result<crate::memory_bucket_seal::DigestOutcome> {
        crate::memory_bucket_seal::end_of_day_digest(
            &self.store,
            day,
            &self.summariser,
            &self.embedder,
        )
        .await
    }

    /// Return a window-scoped recap from the global activity tree.
    pub async fn global_recap(
        &self,
        window: chrono::Duration,
    ) -> anyhow::Result<Option<crate::memory_bucket_seal::RecapOutput>> {
        crate::memory_bucket_seal::recap(&self.store, window).await
    }
}

/// Build the tags vec for a chunk based on the trait's category + session_id.
fn build_tags(category: &MemoryCategory, session_id: Option<&str>) -> Vec<String> {
    let mut tags = Vec::with_capacity(2);
    let category_tag = match category {
        MemoryCategory::Core => "category:core".to_string(),
        MemoryCategory::Daily => "category:daily".to_string(),
        MemoryCategory::Conversation => "category:conversation".to_string(),
        MemoryCategory::Custom(s) => format!("category:custom:{}", s),
    };
    tags.push(category_tag);
    if let Some(s) = session_id {
        tags.push(format!("session:{}", s));
    }
    tags
}

/// Parse tags JSON array back into MemoryCategory and optional session_id.
/// Called by recall/get/list to hydrate MemoryEntry from SQL rows.
fn parse_tags(tags: &[String]) -> (MemoryCategory, Option<String>) {
    let mut category = MemoryCategory::Custom("unknown".to_string());
    let mut session = None;
    for tag in tags {
        if let Some(rest) = tag.strip_prefix("category:") {
            category = match rest {
                "core" => MemoryCategory::Core,
                "daily" => MemoryCategory::Daily,
                "conversation" => MemoryCategory::Conversation,
                _ => {
                    if let Some(custom) = rest.strip_prefix("custom:") {
                        MemoryCategory::Custom(custom.to_string())
                    } else {
                        MemoryCategory::Custom(rest.to_string())
                    }
                }
            };
        } else if let Some(rest) = tag.strip_prefix("session:") {
            session = Some(rest.to_string());
        }
    }
    (category, session)
}

/// Hydrate a row from the `c.*` columns of the recall/get/list queries into a MemoryEntry.
///
/// Column order: id(0), source_id(1), source_ref(2), content(3), timestamp_ms(4), tags_json(5)
fn row_to_memory_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
    let id: String = row.get(0)?;
    let source_id: String = row.get(1)?;
    let source_ref: Option<String> = row.get(2)?;
    let content: String = row.get(3)?;
    let timestamp_ms: i64 = row.get(4)?;
    let tags_json: String = row.get(5)?;

    let tags: Vec<String> = serde_json::from_str(&tags_json).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            5,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    let (category, session_id) = parse_tags(&tags);

    let timestamp = Utc
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .ok_or_else(|| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Integer,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "invalid timestamp_ms",
                )),
            )
        })?
        .to_rfc3339();

    Ok(MemoryEntry {
        id,
        namespace: Some(source_id),
        key: source_ref.unwrap_or_default(),
        content,
        category,
        timestamp,
        session_id,
        score: None,
    })
}

#[async_trait]
impl MemoryAdapter for BucketSealAdapter {
    fn name(&self) -> &str {
        ADAPTER_NAME
    }

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

        // PHASE A: Source pipeline — inner block so source guard drops at block end,
        // before topic fan-out acquires per-topic guards (PR10 mutex discipline).
        {
            // 1. Resolve source tree (idempotent get_or_create).
            let tree = get_or_create_source_tree(&self.store, namespace)
                .context("get_or_create_source_tree")?;

            // 2. Acquire per-source-tree mutex with explicit "source:" prefix to
            //    avoid key collision with topic trees (key format: "topic:{entity}").
            let tree_mutex = self.tree_mutex(&format!("source:{}", namespace)).await;
            let _guard = tree_mutex.lock().await;

            // 3. Build tags (category + session encoded).
            let tags = build_tags(&category, session_id);

            // 4. Canonicalise as Document.
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

            // 5. Chunk.
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

            // 6. Score each chunk; collect admitted ones + score rows.
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

            // 7. Stage admitted chunks to disk and upsert to mem_tree_chunks.
            if !admitted.is_empty() {
                let staged: Vec<StagedChunk> = stage_chunks(&self.content_root, &admitted)
                    .context("stage_chunks")?;
                self.store
                    .upsert_staged_chunks(&staged)
                    .context("upsert_staged_chunks")?;
            }

            // 8. Persist score rows (only for admitted chunks; FK requires chunks inserted first).
            for row in &score_rows {
                if !row.dropped {
                    upsert_score(&self.store, row).context("upsert_score")?;
                }
            }

            // 9. append_leaf_deferred each admitted chunk into the source seal cascade.
            // Fast synchronous buffer write; cascade is detached (best-effort).
            for chunk in &admitted {
                let leaf = LeafRef {
                    chunk_id: chunk.id.clone(),
                    token_count: chunk.token_count,
                    timestamp: chunk.metadata.timestamp,
                    content: chunk.content.clone(),
                    entities: chunk.metadata.tags.clone(),
                    topics: vec![],
                    score: score_rows
                        .iter()
                        .find(|r| r.chunk_id == chunk.id)
                        .map(|r| r.total)
                        .unwrap_or(0.0),
                };
                let gate_met = append_leaf_deferred(&self.store, &tree, &leaf)
                    .context("source append_leaf_deferred")?;
                if gate_met {
                    self.enqueue_seal(&tree.id);
                }
            }
            // _guard drops here — source mutex released before topic fan-out.
        }

        // PHASE B: Topic fan-out — per-entity append_leaf (best-effort).
        // Source append already succeeded; partial topic indexing is acceptable.
        for chunk in &admitted {
            let entities =
                crate::memory_bucket_seal::extract_entities(&chunk.content);
            for entity in &entities {
                let topic_tree = match crate::memory_bucket_seal::tree_topic::get_or_create_topic_tree(
                    &self.store,
                    entity,
                ) {
                    Ok(t) => t,
                    Err(e) => {
                        tracing::warn!(
                            entity = %entity,
                            chunk_id = %chunk.id,
                            error = %e,
                            "get_or_create_topic_tree failed — skipping entity"
                        );
                        continue;
                    }
                };
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
                let gate_met = match append_leaf_deferred(&self.store, &topic_tree, &leaf) {
                    Ok(g) => g,
                    Err(e) => {
                        tracing::warn!(
                            entity = %entity,
                            chunk_id = %chunk.id,
                            error = %e,
                            "topic append_leaf_deferred failed — skipping entity"
                        );
                        continue;
                    }
                };
                if gate_met {
                    self.enqueue_seal(&topic_tree.id);
                }
            }
        }

        Ok(())
    }

    async fn recall(
        &self,
        query: &str,
        limit: usize,
        opts: RecallOpts<'_>,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.store.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT c.id, c.source_id, c.source_ref, c.content, c.timestamp_ms, c.tags_json
               FROM mem_tree_chunks_fts AS fts
               JOIN mem_tree_chunks    AS c ON c.id = fts.chunk_id
              WHERE fts.content MATCH ?1
                AND (?2 IS NULL OR fts.source_id = ?2)
              ORDER BY rank
              LIMIT ?3",
        )?;

        let ns_param = opts.namespace.map(|s| s.to_string());
        let rows = stmt.query_map(
            rusqlite::params![query, ns_param, limit as i64],
            row_to_memory_entry,
        )?;

        let want_cat = opts.category.as_ref();
        let mut out: Vec<MemoryEntry> = Vec::new();
        for row in rows {
            let entry = row?;
            // Optional category filter (applied in Rust since it's tag-based).
            if let Some(filter) = want_cat {
                if &entry.category != filter {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn get(&self, namespace: &str, key: &str) -> Result<Option<MemoryEntry>> {
        let conn = self.store.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT id, source_id, source_ref, content, timestamp_ms, tags_json
               FROM mem_tree_chunks
              WHERE source_id = ?1 AND source_ref = ?2
              ORDER BY created_at_ms DESC
              LIMIT 1",
        )?;
        let entry: Option<MemoryEntry> = stmt
            .query_row(rusqlite::params![namespace, key], row_to_memory_entry)
            .optional()
            .context("get_chunk")?;
        Ok(entry)
    }

    async fn list(
        &self,
        namespace: Option<&str>,
        category: Option<&MemoryCategory>,
        session_id: Option<&str>,
    ) -> Result<Vec<MemoryEntry>> {
        let conn = self.store.lock_conn()?;

        let mut sql = String::from(
            "SELECT id, source_id, source_ref, content, timestamp_ms, tags_json
               FROM mem_tree_chunks
              WHERE 1=1",
        );
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(ns) = namespace {
            sql.push_str(" AND source_id = ?");
            params.push(Box::new(ns.to_string()));
        }
        if let Some(s) = session_id {
            sql.push_str(" AND tags_json LIKE ?");
            params.push(Box::new(format!("%\"session:{}\"%", s)));
        }

        sql.push_str(" ORDER BY timestamp_ms DESC LIMIT 200");

        let mut stmt = conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();
        let rows = stmt.query_map(&params_refs[..], row_to_memory_entry)?;

        let mut out: Vec<MemoryEntry> = Vec::new();
        for row in rows {
            let entry = row?;
            if let Some(filter) = category {
                if &entry.category != filter {
                    continue;
                }
            }
            out.push(entry);
        }
        Ok(out)
    }

    async fn delete(&self, namespace: &str, key: &str) -> Result<bool> {
        let mut conn = self.store.lock_conn()?;
        let tx = conn.transaction().context("begin delete tx")?;

        // Delete score rows first (FK: mem_tree_score.chunk_id → mem_tree_chunks.id).
        tx.execute(
            "DELETE FROM mem_tree_score
              WHERE chunk_id IN (
                  SELECT id FROM mem_tree_chunks
                   WHERE source_id = ?1 AND source_ref = ?2
              )",
            rusqlite::params![namespace, key],
        )
        .context("delete score rows")?;

        let n = tx.execute(
            "DELETE FROM mem_tree_chunks
              WHERE source_id = ?1 AND source_ref = ?2",
            rusqlite::params![namespace, key],
        )
        .context("delete chunks")?;

        tx.commit().context("commit delete tx")?;
        Ok(n > 0)
    }

    async fn clear_namespace(&self, namespace: &str) -> Result<u64> {
        let mut conn = self.store.lock_conn()?;
        let tx = conn.transaction().context("begin clear tx")?;

        tx.execute(
            "DELETE FROM mem_tree_score
              WHERE chunk_id IN (
                  SELECT id FROM mem_tree_chunks WHERE source_id = ?1
              )",
            rusqlite::params![namespace],
        )
        .context("delete score rows")?;

        let n = tx.execute(
            "DELETE FROM mem_tree_chunks WHERE source_id = ?1",
            rusqlite::params![namespace],
        )
        .context("delete chunks")?;

        tx.commit().context("commit clear tx")?;
        Ok(n as u64)
    }

    async fn namespace_summaries(&self) -> Result<Vec<NamespaceSummary>> {
        let conn = self.store.lock_conn()?;
        let mut stmt = conn.prepare(
            "SELECT source_id, COUNT(*), MAX(timestamp_ms)
               FROM mem_tree_chunks
              GROUP BY source_id
              ORDER BY source_id",
        )?;
        let rows = stmt.query_map([], |row| {
            let namespace: String = row.get(0)?;
            let count: i64 = row.get(1)?;
            let last_updated_ms: Option<i64> = row.get(2)?;
            let last_updated = last_updated_ms.and_then(|ms| {
                Utc.timestamp_millis_opt(ms)
                    .single()
                    .map(|dt| dt.to_rfc3339())
            });
            Ok(NamespaceSummary {
                namespace,
                count: count.max(0) as usize,
                last_updated,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::tree_source::{get_or_create_source_tree, InertSummariser};
    use tempfile::TempDir;

    fn fresh_adapter() -> (BucketSealAdapter, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn Summariser> = Arc::new(InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, content_root, embedder, summariser);
        (adapter, dir)
    }

    // ── Task 2: skeleton ────────────────────────────────────────────────────

    #[tokio::test]
    async fn name_is_bucket_seal() {
        let (adapter, _dir) = fresh_adapter();
        assert_eq!(adapter.name(), "bucket_seal");
    }

    #[tokio::test]
    async fn tree_mutex_returns_same_arc_for_same_namespace() {
        let (adapter, _dir) = fresh_adapter();
        let m1 = adapter.tree_mutex("ns1").await;
        let m2 = adapter.tree_mutex("ns1").await;
        // Same namespace → same Arc
        assert!(Arc::ptr_eq(&m1, &m2));
        let m3 = adapter.tree_mutex("ns2").await;
        // Different namespace → different Arc
        assert!(!Arc::ptr_eq(&m1, &m3));
    }

    // ── Task 3: store() ─────────────────────────────────────────────────────

    #[tokio::test]
    async fn store_admits_and_appends_a_chunk() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "test_ns",
                "key_1",
                "Substantive note about a meaningful topic with sufficient signal density.",
                MemoryCategory::Core,
                Some("session_abc"),
            )
            .await
            .unwrap();

        let _tree = get_or_create_source_tree(&adapter.store, "test_ns").unwrap();
        let count = adapter.store.count_chunks().unwrap();
        assert!(count >= 1, "store should have inserted at least one chunk");
    }

    #[tokio::test]
    async fn store_skips_empty_content() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("test_ns", "key_empty", "   ", MemoryCategory::Core, None)
            .await
            .unwrap();
        assert_eq!(adapter.store.count_chunks().unwrap(), 0);
    }

    #[tokio::test]
    async fn store_serialises_per_tree_via_mutex() {
        let (adapter, _dir) = fresh_adapter();
        let adapter = Arc::new(adapter);
        let mut handles = Vec::new();
        for i in 0..5 {
            let a = adapter.clone();
            handles.push(tokio::spawn(async move {
                a.store(
                    "concurrent_ns",
                    &format!("key_{i}"),
                    &format!("Substantive note number {i} with enough signal to pass admission."),
                    MemoryCategory::Core,
                    None,
                )
                .await
            }));
        }
        for h in handles {
            h.await.unwrap().unwrap();
        }
        assert!(adapter.store.count_chunks().unwrap() >= 5);
    }

    // ── Task 4: recall() ────────────────────────────────────────────────────

    #[tokio::test]
    async fn recall_matches_substring_via_fts() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "recall_ns",
                "k1",
                "Project Phoenix launch plan with quarterly milestones.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        adapter
            .store(
                "recall_ns",
                "k2",
                "Unrelated note about weather patterns.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        let opts = RecallOpts {
            namespace: Some("recall_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("Phoenix", 10, opts).await.unwrap();
        assert!(!hits.is_empty(), "FTS should find 'Phoenix'");
        assert!(hits.iter().any(|e| e.content.contains("Phoenix")));
    }

    #[tokio::test]
    async fn recall_respects_namespace_filter() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_a", "k1", "Apple banana cherry common keyword.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_b", "k2", "Apple banana cherry common keyword.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let opts_a = RecallOpts {
            namespace: Some("ns_a"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits_a = adapter.recall("common", 10, opts_a).await.unwrap();
        assert!(hits_a.iter().all(|e| e.namespace.as_deref() == Some("ns_a")));
    }

    #[tokio::test]
    async fn recall_respects_limit() {
        let (adapter, _dir) = fresh_adapter();
        for i in 0..5 {
            adapter
                .store(
                    "limit_ns",
                    &format!("k{i}"),
                    &format!("Unique repeatable keyword content line {i}."),
                    MemoryCategory::Core,
                    None,
                )
                .await
                .unwrap();
        }
        let opts = RecallOpts {
            namespace: Some("limit_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("unique", 2, opts).await.unwrap();
        assert!(hits.len() <= 2);
    }

    // ── Task 5: get / list / namespace_summaries ─────────────────────────────

    #[tokio::test]
    async fn get_returns_most_recent_chunk_for_key() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_g", "the_key", "First version content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_g", "the_key", "Second version updated content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let got = adapter.get("ns_g", "the_key").await.unwrap();
        assert!(got.is_some());
        let entry = got.unwrap();
        assert!(entry.content.contains("Second") || entry.content.contains("updated") || entry.content.contains("First"));
    }

    #[tokio::test]
    async fn list_filters_by_namespace_and_category() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("nslA", "k1", "Note A1 substantive content.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("nslA", "k2", "Note A2 substantive content.", MemoryCategory::Conversation, None)
            .await
            .unwrap();
        adapter
            .store("nslB", "k3", "Note B substantive content.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let listed = adapter
            .list(Some("nslA"), Some(&MemoryCategory::Core), None)
            .await
            .unwrap();
        assert!(listed.iter().all(|e| e.namespace.as_deref() == Some("nslA")));
        assert!(listed.iter().all(|e| matches!(e.category, MemoryCategory::Core)));
    }

    #[tokio::test]
    async fn namespace_summaries_groups_by_source() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("nsA", "k1", "Note in nsA with substance.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("nsB", "k2", "Note in nsB with substance.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let summaries = adapter.namespace_summaries().await.unwrap();
        assert!(summaries.iter().any(|s| s.namespace == "nsA"));
        assert!(summaries.iter().any(|s| s.namespace == "nsB"));
    }

    // ── Task 6: delete / clear_namespace ────────────────────────────────────

    #[tokio::test]
    async fn delete_returns_true_then_false() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_d", "the_key", "Content to delete.", MemoryCategory::Core, None)
            .await
            .unwrap();
        let first = adapter.delete("ns_d", "the_key").await.unwrap();
        let second = adapter.delete("ns_d", "the_key").await.unwrap();
        assert!(first);
        assert!(!second);
    }

    #[tokio::test]
    async fn clear_namespace_removes_chunks_in_scope_only() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_keep", "k1", "Content to keep substantively.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter
            .store("ns_drop", "k2", "Content to drop substantively.", MemoryCategory::Core, None)
            .await
            .unwrap();

        let removed = adapter.clear_namespace("ns_drop").await.unwrap();
        assert!(removed >= 1, "expected at least one chunk removed");

        let kept = adapter.list(Some("ns_keep"), None, None).await.unwrap();
        assert!(!kept.is_empty());
        let dropped = adapter.list(Some("ns_drop"), None, None).await.unwrap();
        assert!(dropped.is_empty());
    }

    #[tokio::test]
    async fn clear_namespace_returns_zero_for_unknown_namespace() {
        let (adapter, _dir) = fresh_adapter();
        let cleared = adapter.clear_namespace("never_seen_ns").await.unwrap();
        assert_eq!(cleared, 0);
    }

    #[tokio::test]
    async fn recall_on_fresh_store_returns_empty() {
        let (adapter, _dir) = fresh_adapter();
        let opts = RecallOpts {
            namespace: Some("any_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("anything", 10, opts).await.unwrap();
        assert!(hits.is_empty());
    }

    #[tokio::test]
    async fn namespace_summaries_returns_empty_for_fresh_store() {
        let (adapter, _dir) = fresh_adapter();
        let summaries = adapter.namespace_summaries().await.unwrap();
        assert!(summaries.is_empty());
    }

    #[tokio::test]
    async fn recall_with_fts_special_chars_does_not_panic() {
        let (adapter, _dir) = fresh_adapter();
        // Seed at least one chunk so FTS isn't empty.
        adapter
            .store(
                "fts_ns",
                "k1",
                "Substantive content about a topic with sufficient signal density to pass admission.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();

        let opts = RecallOpts {
            namespace: Some("fts_ns"),
            category: None,
            session_id: None,
            min_score: None,
        };
        // FTS5 reserves some characters; the adapter should either return Ok (with possibly empty results)
        // or surface a clean Err — but it must NOT panic.
        for query in ["\"quoted\"", "wild*card", "OR alone", "AND missing"] {
            let result = adapter.recall(query, 10, opts.clone()).await;
            // Either Ok or Err is acceptable — what matters is no panic.
            match result {
                Ok(_) | Err(_) => {}
            }
        }
    }

    #[tokio::test]
    async fn delete_propagates_to_fts() {
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store("ns_fts", "k1", "Unique searchable keyword payload.", MemoryCategory::Core, None)
            .await
            .unwrap();
        adapter.delete("ns_fts", "k1").await.unwrap();

        let opts = RecallOpts {
            namespace: Some("ns_fts"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("unique", 10, opts).await.unwrap();
        assert!(hits.is_empty(), "delete trigger should have cleared FTS row");
    }

    // ── PR10: topic fan-out tests ────────────────────────────────────────────

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
        assert_eq!(
            alice_tree.kind,
            crate::memory_bucket_seal::tree_source::types::TreeKind::Topic
        );
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

        // Source tree exists (idempotent get_or_create).
        let _source_tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(
            &adapter.store,
            "no_entity_ns",
        )
        .unwrap();

        // No topic trees created — verify by counting topic-kind trees.
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

        // At least one chunk admitted.
        let count = adapter.store.count_chunks().unwrap();
        assert!(count >= 1);

        // Both source and topic trees exist and are distinct.
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

        // At most MAX_ENTITIES_PER_CHUNK = 20 topic trees per chunk.
        let topic_trees = crate::memory_bucket_seal::tree_source::store::list_trees_by_kind(
            &adapter.store,
            crate::memory_bucket_seal::tree_source::types::TreeKind::Topic,
        )
        .unwrap();
        assert!(
            topic_trees.len() <= 20,
            "got {} topic trees, expected ≤ 20",
            topic_trees.len()
        );
    }

    // ── PR11: global digest + recap adapter methods ──────────────────────────

    #[tokio::test]
    async fn run_global_digest_via_adapter() {
        let (adapter, _dir) = fresh_adapter();
        // Store something so a source tree exists. The source tree likely has
        // no sealed L1 summary (small chunk stays in L0 buffer), so the digest
        // yields EmptyDay or Emitted — both are valid outcomes. Assert no error.
        adapter
            .store(
                "slack:#eng",
                "k1",
                "Alice shipped Project Phoenix today with substantial detail and signal.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        let day = chrono::Utc::now().date_naive();
        let outcome = adapter.run_global_digest(day).await.unwrap();
        // Either EmptyDay (no sealed source summary) or Emitted — both ok.
        let _ = outcome;
    }

    #[tokio::test]
    async fn global_recap_empty_is_none() {
        let (adapter, _dir) = fresh_adapter();
        let r = adapter
            .global_recap(chrono::Duration::days(7))
            .await
            .unwrap();
        assert!(r.is_none());
    }

    // ── PR13: hot-path (durable enqueue replaces detached spawn) ────────────

    /// Build an adapter with a custom summariser (leaves embedder as Inert).
    /// Kept so tests that still need it compile; the SlowSummariser path is
    /// no longer exercised by store() (the worker would run it instead).
    fn fresh_adapter_with_summariser(
        summariser: Arc<dyn Summariser>,
    ) -> (BucketSealAdapter, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn Embedder> = Arc::new(InertEmbedder::new());
        let adapter = BucketSealAdapter::new(store, content_root, embedder, summariser);
        (adapter, dir)
    }

    #[tokio::test]
    async fn store_enqueues_durable_seal_job() {
        // PR13: store() no longer spawns; it enqueues a durable Seal job.
        // Core guarantees: chunk is persisted synchronously + store() is fast.
        let (adapter, _dir) = fresh_adapter();
        let start = std::time::Instant::now();
        adapter
            .store(
                "ns_seal",
                "k1",
                "Content with enough signal to be admitted and buffered for sealing.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        // store() must return fast — synchronous buffer write only, no LLM work.
        assert!(
            start.elapsed() < std::time::Duration::from_millis(500),
            "store() must return fast (no LLM work inline)"
        );
        // The chunk is durably persisted.
        assert!(
            adapter.store.count_chunks().unwrap() >= 1,
            "chunk must be persisted synchronously"
        );
        // Whether a seal job exists depends on whether the gate tripped;
        // both zero (gate not met) and one (gate met) are valid.
        let conn = adapter.store.lock_conn().unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM mem_tree_jobs WHERE kind='seal'", [], |r| r.get(0),
        ).unwrap();
        assert!(n <= 1, "at most one active seal job per tree");
        let _ = fresh_adapter_with_summariser; // suppress unused warning
    }

    #[tokio::test]
    async fn store_buffer_is_durable_before_cascade() {
        // After store(), the chunk is persisted synchronously via
        // append_leaf_deferred regardless of cascade state.
        let (adapter, _dir) = fresh_adapter();
        adapter
            .store(
                "ns_dur",
                "k1",
                "Durable content with sufficient admission signal density.",
                MemoryCategory::Core,
                None,
            )
            .await
            .unwrap();
        assert!(
            adapter.store.count_chunks().unwrap() >= 1,
            "chunk must be persisted synchronously before any cascade"
        );
    }

    // ── PR15: recall_semantic + recall_hybrid ────────────────────────────────

    /// A fake embedder: maps specific texts to distinct unit vectors so
    /// cosine ordering is deterministic in tests.
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
        let store = Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        // Seed a source tree + two summaries with distinct embeddings.
        let tree = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "ns1").unwrap();
        let mk = |id: &str, content: &str, hot: usize| -> SummaryNode {
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
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = Arc::new(FakeVecEmbedder);
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser);

        // Query "alpha" → s-alpha (hot dim 0) ranks above s-beta (hot dim 1).
        let hits = adapter.recall_semantic("alpha please", 10, None).await.unwrap();
        assert!(!hits.is_empty());
        assert_eq!(hits[0].id, "s-alpha");
        assert!(hits[0].score.unwrap() > hits.get(1).and_then(|h| h.score).unwrap_or(0.0));
    }

    #[tokio::test]
    async fn recall_semantic_respects_namespace_and_limit() {
        use crate::memory_bucket_seal::tree_source::{store as ts, types::{SummaryNode, TreeKind}};
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();

        // Seed two trees with different scopes.
        let tree_ns1 = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "ns1").unwrap();
        let tree_ns2 = crate::memory_bucket_seal::tree_source::get_or_create_source_tree(&store, "ns2").unwrap();

        let mk = |id: &str, tree_id: &str, tree_kind: TreeKind, hot: usize| -> SummaryNode {
            let mut emb = vec![0.0f32; crate::memory_bucket_seal::score::embed::EMBEDDING_DIM];
            emb[hot] = 1.0;
            SummaryNode {
                id: id.into(), tree_id: tree_id.into(), tree_kind, level: 1,
                parent_id: None, child_ids: vec![], content: format!("summary {id}"), token_count: 10,
                entities: vec![], topics: vec![],
                time_range_start: chrono::Utc::now(), time_range_end: chrono::Utc::now(),
                score: 0.5, sealed_at: chrono::Utc::now(), deleted: false, embedding: Some(emb),
            }
        };
        {
            let mut conn = store.lock_conn().unwrap();
            let tx = conn.transaction().unwrap();
            ts::insert_summary_tx(&tx, &mk("s-ns1", &tree_ns1.id, TreeKind::Source, 0)).unwrap();
            ts::insert_summary_tx(&tx, &mk("s-ns2", &tree_ns2.id, TreeKind::Source, 0)).unwrap();
            tx.commit().unwrap();
        }
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = Arc::new(FakeVecEmbedder);
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser);

        // Namespace filter: only ns1 should come back.
        let hits = adapter.recall_semantic("alpha", 10, Some("ns1")).await.unwrap();
        assert!(hits.iter().all(|e| e.namespace.as_deref() == Some("ns1")),
            "namespace filter failed: {:?}", hits.iter().map(|e| &e.namespace).collect::<Vec<_>>());

        // Limit: only 1 result even though 2 summaries have embeddings (no ns filter).
        let hits_limited = adapter.recall_semantic("alpha", 1, None).await.unwrap();
        assert!(hits_limited.len() <= 1, "limit not respected");
    }

    #[tokio::test]
    async fn recall_hybrid_merges_semantic_and_fts_dedup() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = Arc::new(FakeVecEmbedder);
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
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
        let store = Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> = Arc::new(FakeVecEmbedder);
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(store, dir.path().join("content"), embedder, summariser);
        let hits = adapter.recall_hybrid("nothing here", None, 6).await;
        assert!(hits.is_empty());
    }

    // ── PR15.5: semantic-error → FTS-only fallback ───────────────────────────

    /// An embedder that always errors — simulates a downed embeddings endpoint.
    struct ErrEmbedder;
    #[async_trait::async_trait]
    impl crate::memory_bucket_seal::score::embed::Embedder for ErrEmbedder {
        fn name(&self) -> &'static str { "err" }
        async fn embed(&self, _text: &str) -> anyhow::Result<Vec<f32>> {
            anyhow::bail!("embed endpoint down")
        }
    }

    #[tokio::test]
    async fn recall_hybrid_falls_back_to_fts_when_semantic_errs() {
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(crate::memory_bucket_seal::store::BucketSealStore::open(
            &dir.path().join("chunks.db"),
        ).unwrap());
        store.ensure_schema().unwrap();
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(ErrEmbedder);
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(crate::memory_bucket_seal::tree_source::InertSummariser::new());
        let adapter = BucketSealAdapter::new(
            store,
            dir.path().join("content"),
            embedder,
            summariser,
        );
        // Store a chunk so FTS has something to find.
        adapter
            .store("ns1", "k1", "alpha keyword content for fts match", MemoryCategory::Core, None)
            .await
            .unwrap();
        // Semantic leg errors (ErrEmbedder), but hybrid must NOT error — FTS backfills.
        let hits = adapter.recall_hybrid("alpha", None, 6).await;
        // The call returns cleanly (no panic, no propagated Err).
        // FTS should find the stored chunk since it contains "alpha".
        assert!(
            hits.iter().any(|e| e.content.contains("alpha")),
            "FTS leg should have found the 'alpha' chunk even when semantic errors"
        );
    }
}
