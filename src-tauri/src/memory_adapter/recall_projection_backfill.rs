//! openhuman-A one-time backfill: project existing recallable memory_graph nodes
//! (Reference/Episode/UserProfile — knowledge/event/profile) into the bucket_seal
//! recall surface so pre-slice facts are recallable. Marker-gated + idempotent +
//! best-effort (never blocks boot). EntityPages excluded (2b already projects them);
//! boot kinds (Boot/Identity/Value/Directive/Curated) and Procedure excluded.
//!
//! **Marker mechanism**: mirrors `pages_to_entitypage_migration` exactly — a
//! sentinel EntityPage is written to the MemoryGraphStore under a reserved slug
//! (`BACKFILL_MARKER_SLUG`) once the pass completes without per-node errors.
//! Subsequent boots check `find_entity_page_by_slug` and short-circuit at zero cost.
//!
//! **Space**: always "default" — the only space in practice at boot time (same
//! rationale as WikiView pages→EntityPage migration).

// SPDX-License-Identifier: MIT

use std::sync::Arc;

use crate::memory_bucket_seal::BucketSealAdapter;
use crate::memory_graph::models::MemoryNodeKind;
use crate::memory_graph::store::MemoryGraphStore;

/// Sentinel EntityPage slug written to the graph once backfill completes.
/// Must not collide with any real entity slug (double-underscore convention).
const BACKFILL_MARKER_SLUG: &str = "__recall_projection_backfill_v1__";

/// Space to write the sentinel EntityPage into. Mirrors the pages migration.
const BACKFILL_SPACE: &str = "default";

/// Maximum number of nodes fetched per kind in one listing pass.
/// If the result length equals this cap, some nodes are silently truncated —
/// a warn is emitted so operators can detect the condition.
const LIST_CAP: usize = 100_000;

/// Recallable node kinds projected into the recall surface.
/// Excludes: boot kinds (Boot/Identity/Value/Directive/Curated — injected every
/// prompt), Procedure (skills/tool facades, not per-turn recall), EntityPage
/// (already projected via Step 2b write_page → `pages` namespace).
const RECALLABLE_KINDS: &[MemoryNodeKind] = &[
    MemoryNodeKind::Reference,
    MemoryNodeKind::Episode,
    MemoryNodeKind::UserProfile,
];

/// One-time idempotent backfill. Returns number of nodes projected (0 on skip).
///
/// Never panics; all per-node errors are logged and the node is skipped.
/// The marker is written only when the DB listing/active-version passes are
/// fully clean (`all_ok`). NOTE: `project_fact` is best-effort (returns (),
/// swallows bucket_seal write errors), so a silent projection failure does NOT
/// block the marker — but re-projection is content-hash idempotent, so a manual
/// marker reset safely re-projects.
pub async fn backfill_recall_projections(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<BucketSealAdapter>,
) -> anyhow::Result<usize> {
    // ── Idempotency check: marker EntityPage already present → skip ───────────
    match store.find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG) {
        Ok(Some(_)) => {
            tracing::debug!("recall_projection_backfill: marker present; skipping");
            return Ok(0);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(
                error = %e,
                "recall_projection_backfill: marker check failed; proceeding anyway"
            );
        }
    }

    let mut projected = 0usize;
    let mut all_ok = true;

    // ── Iterate recallable kinds ──────────────────────────────────────────────
    for &kind in RECALLABLE_KINDS {
        let nodes = match store.list_nodes_by_kind(BACKFILL_SPACE, kind, LIST_CAP) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    kind = kind.as_str(),
                    error = %e,
                    "recall_projection_backfill: list_nodes_by_kind failed; skipping kind"
                );
                all_ok = false;
                continue;
            }
        };
        if nodes.len() == LIST_CAP {
            tracing::warn!(
                ?kind,
                cap = LIST_CAP,
                "recall_projection_backfill: hit list cap; some nodes may not be projected"
            );
        }

        for node in nodes {
            // ── Active version → content ──────────────────────────────────
            let content = match store.get_active_version(&node.id) {
                Ok(Some(v)) => v.content,
                Ok(None) => {
                    tracing::debug!(
                        node_id = %node.id,
                        kind = kind.as_str(),
                        "recall_projection_backfill: no active version; skipping node"
                    );
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        node_id = %node.id,
                        kind = kind.as_str(),
                        error = %e,
                        "recall_projection_backfill: get_active_version failed; skipping node"
                    );
                    all_ok = false;
                    continue;
                }
            };

            // ── Project into bucket_seal recall surface ────────────────────
            // project_fact is already best-effort (logs+swallows internally).
            crate::memory_adapter::recall_projection::project_fact(adapter, &node.id, &content)
                .await;
            projected += 1;
        }
    }

    // ── Write sentinel marker only on a fully-clean pass ─────────────────────
    if all_ok {
        let marker_body = "recall projection backfill marker — do not edit";
        match store.entity_page_put(BACKFILL_SPACE, BACKFILL_MARKER_SLUG, marker_body) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "recall_projection_backfill: failed to write marker; backfill will re-run next boot"
                );
            }
        }
    }

    tracing::info!(
        projected,
        all_ok,
        "recall_projection_backfill: backfill complete"
    );
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_adapter::RecallOpts;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::store::BucketSealStore;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use crate::memory_graph::models::{
        MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus,
    };
    use std::sync::Arc;
    use tempfile::TempDir;

    // ── In-memory MemoryGraphStore (mirrors pages migration test helper) ──────

    fn make_graph_store() -> Arc<MemoryGraphStore> {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1)
            .expect("V35 schema");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(std::sync::Mutex::new(conn))))
    }

    // ── In-memory BucketSealAdapter (mirrors recall_projection.rs test helper) ─

    fn make_bucket_seal() -> (Arc<BucketSealAdapter>, TempDir) {
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

    // ── Seed helper: insert a node + active version into the graph store ──────

    fn seed_node(
        store: &Arc<MemoryGraphStore>,
        space_id: &str,
        kind: MemoryNodeKind,
        title: &str,
        content: &str,
    ) -> String {
        let node_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space_id.to_string(),
            kind,
            title: title.to_string(),
            metadata: None,
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).expect("create_node");

        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: content.to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).expect("create_version");

        node_id
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// Happy path: 2 recallable nodes (Reference + Episode) + 1 excluded (Identity).
    /// Backfill returns 2; recallable nodes are retrievable via FTS; excluded not.
    #[tokio::test]
    async fn backfill_projects_recallable_excludes_boot_kinds() {
        let store = make_graph_store();
        let (adapter, _dir) = make_bucket_seal();

        // Recallable nodes
        let ref_id = seed_node(
            &store,
            BACKFILL_SPACE,
            MemoryNodeKind::Reference,
            "Alice knows Rust",
            "Alice is a Rust expert with 10 years experience in systems programming",
        );
        let _ep_id = seed_node(
            &store,
            BACKFILL_SPACE,
            MemoryNodeKind::Episode,
            "Meeting on 2026-06-01",
            "Meeting with Bob about Q3 roadmap planning and memory OS milestones",
        );

        // Excluded (boot kind)
        let _id_id = seed_node(
            &store,
            BACKFILL_SPACE,
            MemoryNodeKind::Identity,
            "User Identity",
            "Identity node — must be excluded from recall backfill",
        );

        let n = backfill_recall_projections(&store, &adapter)
            .await
            .unwrap();
        assert_eq!(n, 2, "only the 2 recallable nodes should be projected");

        // Verify Reference node is recallable via FTS
        let opts = RecallOpts {
            namespace: Some(crate::memory_adapter::recall_projection::RECALL_PROJECTION_NAMESPACE),
            category: None,
            session_id: None,
            min_score: None,
        };
        use crate::memory_adapter::MemoryAdapter;
        let hits = adapter.recall("Rust expert", 5, opts).await.unwrap();
        assert!(
            hits.iter().any(|e| e.key == ref_id),
            "Reference node should be recallable via FTS (key=node_id)"
        );

        // Verify boot kind NOT in recall surface
        let boot_hit = hits.iter().find(|e| e.content.contains("excluded from recall"));
        assert!(boot_hit.is_none(), "Identity (boot kind) must not be in recall surface");
    }

    /// Idempotency: second run with marker present returns 0.
    #[tokio::test]
    async fn second_run_short_circuits_via_marker() {
        let store = make_graph_store();
        let (adapter, _dir) = make_bucket_seal();

        seed_node(
            &store,
            BACKFILL_SPACE,
            MemoryNodeKind::Reference,
            "Some fact",
            "A recallable fact about the user preferences for dark mode",
        );

        let first = backfill_recall_projections(&store, &adapter)
            .await
            .unwrap();
        assert_eq!(first, 1);

        // Marker should be present in the graph store.
        assert!(
            store
                .find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG)
                .unwrap()
                .is_some(),
            "sentinel EntityPage marker should exist after first run"
        );

        // Second run must short-circuit.
        let second = backfill_recall_projections(&store, &adapter)
            .await
            .unwrap();
        assert_eq!(second, 0, "second run must return 0 (marker gate)");
    }

    /// UserProfile kind is also recallable.
    #[tokio::test]
    async fn user_profile_kind_is_projected() {
        let store = make_graph_store();
        let (adapter, _dir) = make_bucket_seal();

        seed_node(
            &store,
            BACKFILL_SPACE,
            MemoryNodeKind::UserProfile,
            "User preferences",
            "User prefers concise responses and dark mode interfaces always",
        );

        let n = backfill_recall_projections(&store, &adapter)
            .await
            .unwrap();
        assert_eq!(n, 1, "UserProfile should be projected");

        let opts = RecallOpts {
            namespace: Some(crate::memory_adapter::recall_projection::RECALL_PROJECTION_NAMESPACE),
            category: None,
            session_id: None,
            min_score: None,
        };
        use crate::memory_adapter::MemoryAdapter;
        let hits = adapter.recall("concise dark mode", 5, opts).await.unwrap();
        assert!(!hits.is_empty(), "UserProfile fact should be recallable");
    }

    /// Empty store — no nodes of any recallable kind.
    /// Should return 0 and write the marker (all_ok=true, zero iterations).
    #[tokio::test]
    async fn empty_store_writes_marker_and_returns_zero() {
        let store = make_graph_store();
        let (adapter, _dir) = make_bucket_seal();

        let n = backfill_recall_projections(&store, &adapter)
            .await
            .unwrap();
        assert_eq!(n, 0);

        assert!(
            store
                .find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG)
                .unwrap()
                .is_some(),
            "marker must be written even with zero recallable nodes"
        );
    }
}
