//! openhuman-F skill projection backfill — one-time, marker-gated projection of
//! existing Procedure/learned-skill nodes from `memory_graph` into the bucket_seal
//! `skills` namespace. Pre-F skills become searchable via `get_skill` / `top_skills`
//! without requiring a re-write. Best-effort; never blocks boot.
//!
//! Replaces the P3 one-time `migrate_skills` (which did the reverse
//! Procedure→adapter copy).  The difference: Task 1 (`project_skill` /
//! `project_skill_node`) is the *authoritative* write path going forward;
//! this backfill is only the one-time catch-up for nodes that existed before
//! the Slice F landing.
//!
//! **Marker mechanism**: a sentinel `EntityPage` is written to the
//! `MemoryGraphStore` under `BACKFILL_MARKER_SLUG` once the pass completes
//! without per-node errors. Subsequent boots check `find_entity_page_by_slug`
//! and short-circuit at zero cost.
//!
//! **Space**: always `"default"` — the only space in practice at boot time
//! (same rationale as the recall projection backfill + pages migration).

// SPDX-License-Identifier: MIT

use std::sync::Arc;

use crate::memory_adapter::MemoryAdapter;
use crate::memory_graph::store::MemoryGraphStore;

/// Sentinel EntityPage slug written to the graph once the backfill completes.
const BACKFILL_MARKER_SLUG: &str = "__skill_projection_backfill_v1__";

/// Space to write the sentinel EntityPage into.
const BACKFILL_SPACE: &str = "default";

/// Maximum number of learned-skill nodes fetched in a single listing pass.
const LIST_CAP: usize = 100_000;

/// One-time idempotent backfill. Returns the number of nodes projected (0 on skip).
///
/// Never panics; all per-node errors are logged and the node is skipped.
/// The marker is written only when the enumeration / projection passes are
/// fully clean (`all_ok`).
pub async fn backfill_skill_projections(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
) -> anyhow::Result<usize> {
    // ── Idempotency check: marker EntityPage already present → skip ───────────
    match store.find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG) {
        Ok(Some(_)) => {
            tracing::debug!("skill_projection_backfill: marker present; skipping");
            return Ok(0);
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(
                error = %e,
                "skill_projection_backfill: marker check failed; proceeding anyway"
            );
        }
    }

    // ── Enumerate learned Procedure skills for space "default" ────────────────
    let details = match store.list_top_learned_skills(BACKFILL_SPACE, LIST_CAP) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!(
                error = %e,
                "skill_projection_backfill: list_top_learned_skills failed; aborting pass"
            );
            return Err(anyhow::anyhow!("list_top_learned_skills failed: {e:#}"));
        }
    };

    if details.len() == LIST_CAP {
        tracing::warn!(
            cap = LIST_CAP,
            "skill_projection_backfill: hit list cap; some skill nodes may not be projected"
        );
    }

    let mut projected = 0usize;
    let mut all_ok = true;

    for detail in &details {
        // Fetch the active version ourselves so a DB error is a real signal
        // (mirrors recall_projection_backfill). project_skill_node swallows all
        // errors internally, so we use the body-taking project_skill instead.
        let body = match store.get_active_version(&detail.node.id) {
            Ok(Some(v)) => v.content,
            Ok(None) => {
                tracing::debug!(
                    node_id = %detail.node.id,
                    "skill_projection_backfill: no active version; skipping"
                );
                continue;
            }
            Err(e) => {
                tracing::warn!(
                    node_id = %detail.node.id,
                    error = %e,
                    "skill_projection_backfill: get_active_version failed; skipping"
                );
                all_ok = false;
                continue;
            }
        };
        crate::proactive::skill_parser::project_skill(adapter, &detail.node, &body).await;
        projected += 1;
    }

    // ── Write sentinel marker only on a fully-clean pass ─────────────────────
    if all_ok {
        let marker_body = "skill projection backfill marker — do not edit";
        match store.entity_page_put(BACKFILL_SPACE, BACKFILL_MARKER_SLUG, marker_body) {
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "skill_projection_backfill: failed to write marker; backfill will re-run next boot"
                );
            }
        }
    }

    tracing::info!(
        projected,
        all_ok,
        "skill_projection_backfill: backfill complete"
    );
    Ok(projected)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::embed::InertEmbedder;
    use crate::memory_bucket_seal::store::BucketSealStore;
    use crate::memory_bucket_seal::tree_source::InertSummariser;
    use crate::memory_bucket_seal::BucketSealAdapter;
    use crate::memory_graph::models::{
        MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus,
    };
    use crate::memory_adapter::skills;
    use crate::proactive::skill_parser::normalize_title_for_dedup;
    use std::sync::Arc;
    use tempfile::TempDir;

    // ── In-memory MemoryGraphStore ────────────────────────────────────────────

    fn make_graph_store() -> Arc<MemoryGraphStore> {
        // Allow memory_graph writes in tests.
        std::env::set_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES", "1");
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        crate::db::migrations::run(&conn).expect("full migrations");
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(std::sync::Mutex::new(conn))))
    }

    // ── In-memory BucketSealAdapter ───────────────────────────────────────────

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

    // ── Seed helper: insert a learned Procedure node + active version ─────────

    fn seed_skill_node(
        store: &Arc<MemoryGraphStore>,
        space_id: &str,
        title: &str,
        body: &str,
    ) -> String {
        let node_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().to_rfc3339();

        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: title.to_string(),
            // skill_type=learned is required for list_top_learned_skills to return the node.
            metadata: Some(serde_json::json!({"skill_type": "learned", "enabled": 1})),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).expect("create_node");

        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: body.to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).expect("create_version");

        node_id
    }

    // ── Tests ─────────────────────────────────────────────────────────────────

    /// Happy path: 2 learned Procedure skill nodes projected; both findable via get_skill.
    #[tokio::test]
    async fn backfill_projects_two_skills() {
        let store = make_graph_store();
        let (bucket_adapter, _dir) = make_bucket_seal();
        let adapter: Arc<dyn MemoryAdapter> = bucket_adapter.clone() as Arc<dyn MemoryAdapter>;

        seed_skill_node(&store, BACKFILL_SPACE, "Write tests with TDD", "Always write the test first using red-green-refactor cycle");
        seed_skill_node(&store, BACKFILL_SPACE, "Cargo build check", "Run cargo build --release before committing to verify no compile errors");

        let n = backfill_skill_projections(&store, &adapter).await.unwrap();
        assert_eq!(n, 2, "both skill nodes should be projected");

        // Both should be findable via get_skill.
        let slug1 = normalize_title_for_dedup("Write tests with TDD");
        let slug2 = normalize_title_for_dedup("Cargo build check");
        let s1 = skills::get_skill(&adapter, BACKFILL_SPACE, &slug1).await.unwrap();
        let s2 = skills::get_skill(&adapter, BACKFILL_SPACE, &slug2).await.unwrap();
        assert!(s1.is_some(), "first skill should be findable via get_skill");
        assert!(s2.is_some(), "second skill should be findable via get_skill");

        // Marker should be set.
        assert!(
            store
                .find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG)
                .unwrap()
                .is_some(),
            "sentinel marker should be present after successful backfill"
        );
    }

    /// Idempotency: second run with marker present returns 0.
    #[tokio::test]
    async fn second_run_short_circuits_via_marker() {
        let store = make_graph_store();
        let (bucket_adapter, _dir) = make_bucket_seal();
        let adapter: Arc<dyn MemoryAdapter> = bucket_adapter as Arc<dyn MemoryAdapter>;

        seed_skill_node(&store, BACKFILL_SPACE, "Some skill", "The body of some skill");

        let first = backfill_skill_projections(&store, &adapter).await.unwrap();
        assert_eq!(first, 1);

        // Marker present.
        assert!(
            store
                .find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG)
                .unwrap()
                .is_some(),
            "marker should be set after first run"
        );

        // Second run must short-circuit.
        let second = backfill_skill_projections(&store, &adapter).await.unwrap();
        assert_eq!(second, 0, "second run must return 0 (marker gate)");
    }

    /// Empty store — no learned Procedure nodes.
    /// Should return 0 and write the marker (all_ok=true, zero iterations).
    #[tokio::test]
    async fn empty_store_writes_marker_and_returns_zero() {
        let store = make_graph_store();
        let (bucket_adapter, _dir) = make_bucket_seal();
        let adapter: Arc<dyn MemoryAdapter> = bucket_adapter as Arc<dyn MemoryAdapter>;

        let n = backfill_skill_projections(&store, &adapter).await.unwrap();
        assert_eq!(n, 0);

        assert!(
            store
                .find_entity_page_by_slug(BACKFILL_SPACE, BACKFILL_MARKER_SLUG)
                .unwrap()
                .is_some(),
            "marker must be written even with zero skill nodes"
        );
    }
}
