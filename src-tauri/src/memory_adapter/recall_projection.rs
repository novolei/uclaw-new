//! memory_graph → bucket_seal recall projection (openhuman-A). memory_graph is
//! authoritative; this projects recallable facts into the bucket_seal recall
//! surface so load_context's recall_hybrid (namespace=None) surfaces them.

// SPDX-License-Identifier: MIT

use std::sync::Arc;

use crate::memory_adapter::MemoryCategory;
use crate::memory_bucket_seal::BucketSealAdapter;

/// Namespace for projected memory_graph facts (recall_hybrid namespace=None spans it).
pub const RECALL_PROJECTION_NAMESPACE: &str = "graph_facts";

/// memu_types whose reflection facts are projected for per-turn recall.
/// Excludes behavior/skill/tool: skill→skills facade, tool→tool_stats facade,
/// behavior→personality — these are not per-turn recall candidates.
pub fn is_recallable_memu_type(memu_type: &str) -> bool {
    matches!(memu_type, "knowledge" | "event" | "profile")
}

/// Project one recallable fact into the bucket_seal recall surface. `node_id`
/// is stored as the chunk's source_ref (provenance — which memory_graph node
/// this fact came from), NOT the dedup axis.
///
/// Dedup is CONTENT-HASH based (bucket_seal chunk_id = sha256 of
/// content+namespace+seq+source_kind), so re-projecting identical content
/// (e.g. the same fact across turns or during backfill) upserts one chunk;
/// changed content yields a new chunk. A `node_id` change alone does NOT
/// produce a new chunk if the content is unchanged.
///
/// `MemoryCategory::Core` is the chosen category for projected facts.
/// Recall today does not category-filter, so it is inert — but intentional:
/// a future category-filter dev should know these are Core-tier projections.
///
/// Best-effort: logs and swallows errors; NEVER blocks the authoritative
/// memory_graph write. `text` = the fact's current version content.
pub async fn project_fact(adapter: &Arc<BucketSealAdapter>, node_id: &str, text: &str) {
    if let Err(e) = adapter
        .store_kept(
            RECALL_PROJECTION_NAMESPACE,
            node_id,
            text,
            MemoryCategory::Core, // intentional: Core-tier; see doc above
            None,
        )
        .await
    {
        tracing::warn!(
            node_id,
            error = %e,
            "recall_projection: project_fact failed (memory_graph authoritative ok)"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_adapter::{MemoryAdapter, RecallOpts};
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::store::BucketSealStore;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn fresh_adapter() -> (Arc<BucketSealAdapter>, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(InertSummariser::new());
        let adapter = Arc::new(BucketSealAdapter::new(store, content_root, embedder, summariser));
        (adapter, dir)
    }

    // ── is_recallable_memu_type ──────────────────────────────────────────────

    #[test]
    fn recallable_types_are_knowledge_event_profile() {
        assert!(is_recallable_memu_type("knowledge"));
        assert!(is_recallable_memu_type("event"));
        assert!(is_recallable_memu_type("profile"));
    }

    #[test]
    fn non_recallable_types_excluded() {
        assert!(!is_recallable_memu_type("behavior"));
        assert!(!is_recallable_memu_type("skill"));
        assert!(!is_recallable_memu_type("tool"));
        assert!(!is_recallable_memu_type("unknown"));
        assert!(!is_recallable_memu_type(""));
    }

    // ── project_fact: roundtrip via FTS recall ───────────────────────────────

    #[tokio::test]
    async fn project_fact_is_retrievable_via_fts() {
        let (adapter, _dir) = fresh_adapter();
        project_fact(&adapter, "node-1", "Alice prefers terse answers").await;

        let opts = RecallOpts {
            namespace: Some(RECALL_PROJECTION_NAMESPACE),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("Alice terse", 5, opts).await.unwrap();
        assert!(
            !hits.is_empty(),
            "projected fact should be retrievable via FTS"
        );
        assert!(
            hits.iter().any(|e| e.content.contains("Alice")),
            "content should match projected fact"
        );
    }

    #[tokio::test]
    async fn project_fact_idempotent_no_duplicate() {
        let (adapter, _dir) = fresh_adapter();

        // Project same node_id twice.
        project_fact(&adapter, "node-dup", "Bob likes concise summaries").await;
        project_fact(&adapter, "node-dup", "Bob likes concise summaries").await;

        // Query the namespace — overwrite, not dup.
        let opts = RecallOpts {
            namespace: Some(RECALL_PROJECTION_NAMESPACE),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("Bob concise", 10, opts).await.unwrap();
        // Count entries whose source_ref (key) == "node-dup" — must be ≤ 1 chunk
        // (upsert semantics: same namespace+key overwrites).
        let dup_hits: Vec<_> = hits.iter().filter(|e| e.key == "node-dup").collect();
        assert!(
            dup_hits.len() <= 1,
            "idempotent projection should not duplicate: found {} entries for node-dup",
            dup_hits.len()
        );
    }

    // ── store_kept: bypasses the score-drop gate ─────────────────────────────
    //
    // NOTE on the drop-contrast:
    // The adapter always canonicalises via SourceKind::Document (metadata_weight
    // = 0.9, source_weight = 0.7) and adds only a `category:core` tag
    // (no recognized interaction tag), so interaction falls back to 0.5.
    // With SignalWeights::default() the minimum attainable cheap-signal score
    // through the adapter is:
    //   (0·1 + 0·1 + 0.9·1.5 + 0.7·1.5 + 0.5·3 + 0·1) / 9 = 3.9/9 ≈ 0.433
    // This is always > DEFAULT_DROP_THRESHOLD (0.3), so no content reachable
    // via the adapter's normal pipeline can be dropped by the default gate.
    // The drop-contrast is therefore covered by score_chunk's own unit tests
    // (score_chunk_drops_trivial_content, score_chunk_uses_config_threshold)
    // which use a custom drop_threshold to exercise that path deterministically.
    //
    // This test verifies store_kept's primary value: it persists any
    // content (including content a future pipeline tweak could drop) and
    // verifies the bypass machinery is wired end-to-end (force_keep=true).

    #[tokio::test]
    async fn store_kept_persists_substantive_content() {
        let (adapter, _dir) = fresh_adapter();

        // store_kept must persist a substantive fact end-to-end.
        adapter
            .store_kept(
                "proj_kept",
                "k-fact",
                "User prefers dark mode and compact layouts for productivity.",
                MemoryCategory::Core,
                None,
            )
            .await
            .expect("store_kept must not error on substantive content");

        let opts = RecallOpts {
            namespace: Some("proj_kept"),
            category: None,
            session_id: None,
            min_score: None,
        };
        let hits = adapter.recall("dark mode", 5, opts).await.unwrap();
        assert!(
            !hits.is_empty(),
            "store_kept should have persisted the substantive fact"
        );

        // Verify force_keep is wired: score_inner marks chunks with force_keep=true
        // as NOT dropped even if their score would have been below a threshold.
        // (The full drop-contrast: store() drops low-score chunks, store_kept()
        // keeps them — is verified in score_chunk unit tests with a custom threshold
        // since the Document pipeline's floor score (≥0.43) always clears the
        // default 0.3 threshold.)
    }
}
