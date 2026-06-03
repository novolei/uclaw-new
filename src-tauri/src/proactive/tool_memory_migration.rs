//! P3-edges — one-time migration of memory_graph tool-stat nodes + co-usage edges
//! into the adapter tool_stats facade + edges.rs. Idempotent, marker-gated, boot-safe.
//!
//! **Disjoint from P3-skills**: tool nodes are Procedure-kind, SAME as skills, but
//! distinguishable because:
//!   1. They have **no active MemoryVersion** (tool nodes never get `create_version`).
//!   2. Their `metadata_json` parses as `ToolNodeStats` (contains `total_uses` /
//!      `success_count`). Skills have `cited_count` / `usage_count` / `lifecycle`.
//! We check both: skip if `get_active_version` returns `Some` (that's a skill),
//! and skip if the metadata doesn't parse as ToolNodeStats.
//!
//! **Edge reads**: co-usage edges are `relation_kind = 'relates_to'` on `memory_edges`,
//! queried directly via `store.conn` (raw SQL) — the same approach skill_migration uses
//! for `list_procedure_spaces`. There is no public store method that returns typed
//! `RelatesTo` edges for a space, so we use:
//!   `SELECT parent_node_id, child_node_id FROM memory_edges
//!    WHERE space_id=?1 AND relation_kind='relates_to'
//!    AND parent_node_id IS NOT NULL`
//!
//! **Marker strategy**: after a fully-clean pass, writes a sentinel `ToolStatsRecord`
//! with `space = "__migration__"`, `tool_name = "__tool_memory_migrated_v1__"`.
//! On subsequent boots the marker check short-circuits before any store reads.
//!
//! Production `migrate_tool_memory` is **read-only on memory_graph** (no `create_*`).

use std::collections::HashMap;
use std::sync::Arc;

use crate::memory_adapter::{
    edges,
    tool_stats::{self, ToolStatsRecord},
    MemoryAdapter,
};
use crate::memory_graph::models::MemoryNodeKind;
use crate::memory_graph::store::MemoryGraphStore;

const MARKER_TOOL: &str = "__tool_memory_migrated_v1__";
const MARKER_SPACE: &str = "__migration__";

// ─── Internal ToolNodeStats shape (mirrors tool_memory.rs) ───────────────────

/// Local copy of `tool_memory::ToolNodeStats` — we can't pub-use it from there
/// because it is `struct ToolNodeStats` (private). We redeclare it here for the
/// migration read path only; serde will match by field names.
#[derive(Debug, Clone, serde::Deserialize)]
struct ToolNodeStats {
    total_uses: u64,
    success_count: u64,
    failure_count: u64,
    total_latency_ms: u64,
    output_sizes: Vec<u64>,
    parameter_fingerprints: HashMap<String, u64>,
    last_used_at: String,
}

// ─── Pure mapping helper ──────────────────────────────────────────────────────

/// Pure, unit-testable map: tool node fields → `ToolStatsRecord`.
pub(crate) fn node_to_record(
    space: &str,
    tool_name: &str,
    total_uses: u64,
    success_count: u64,
    failure_count: u64,
    total_latency_ms: u64,
    output_sizes: Vec<u64>,
    parameter_fingerprints: HashMap<String, u64>,
    last_used_at: String,
) -> ToolStatsRecord {
    ToolStatsRecord {
        space: space.into(),
        tool_name: tool_name.into(),
        total_uses,
        success_count,
        failure_count,
        total_latency_ms,
        output_sizes,
        parameter_fingerprints,
        last_used_at,
    }
}

// ─── Space enumeration (direct SQL, mirrors skill_migration) ─────────────────

/// Return every distinct `space_id` that owns at least one Procedure node.
/// Uses `store.conn` directly to avoid an extra round-trip; mirrors
/// `skill_migration::list_procedure_spaces`.
fn list_procedure_spaces(store: &MemoryGraphStore) -> Vec<String> {
    match store.conn.lock() {
        Err(e) => {
            tracing::warn!(error = %e, "tool_memory migration: cannot lock DB to list spaces");
            vec![]
        }
        Ok(conn) => {
            let mut stmt = match conn.prepare(
                "SELECT DISTINCT space_id FROM memory_nodes WHERE kind = ?1",
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "tool_memory migration: prepare list_spaces failed");
                    return vec![];
                }
            };
            stmt.query_map(
                rusqlite::params![MemoryNodeKind::Procedure.as_str()],
                |row| row.get::<_, String>(0),
            )
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
        }
    }
}

/// Read all (parent_node_id, child_node_id) pairs for `relation_kind = 'relates_to'`
/// edges in the given space. Used to migrate co-usage edges.
///
/// Raw SQL via `store.conn` — there is no existing public method that returns
/// typed RelatesTo edges for a space without joining on nodes.
fn list_relates_to_edges(store: &MemoryGraphStore, space_id: &str) -> Vec<(String, String)> {
    match store.conn.lock() {
        Err(e) => {
            tracing::warn!(
                space = %space_id,
                error = %e,
                "tool_memory migration: cannot lock DB to list edges"
            );
            vec![]
        }
        Ok(conn) => {
            let mut stmt = match conn.prepare(
                "SELECT parent_node_id, child_node_id \
                 FROM memory_edges \
                 WHERE space_id = ?1 \
                   AND relation_kind = 'relates_to' \
                   AND parent_node_id IS NOT NULL",
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(
                        space = %space_id,
                        error = %e,
                        "tool_memory migration: prepare list_edges failed"
                    );
                    return vec![];
                }
            };
            stmt.query_map(rusqlite::params![space_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map(|rows| rows.flatten().collect())
            .unwrap_or_default()
        }
    }
}

// ─── Migration entry point ────────────────────────────────────────────────────

/// One-time idempotent migration of tool-stat nodes + co-usage edges.
///
/// Returns the count of `ToolStatsRecord`s successfully written to the adapter.
/// Best-effort / boot-safe: every error is logged and skipped; never panics.
/// Production code is **read-only on memory_graph** — no `create_*` calls.
pub async fn migrate_tool_memory(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
) -> usize {
    // ── Marker check — short-circuit if already done ─────────────────────
    match tool_stats::get_stats(adapter, MARKER_SPACE, MARKER_TOOL).await {
        Ok(Some(_)) => {
            tracing::debug!("tool_memory migration: marker present; skip");
            return 0;
        }
        Err(e) => {
            // Marker read failed; log and proceed (re-migrate rather than
            // silently skip on a transient DB error).
            tracing::warn!(
                error = %e,
                "tool_memory migration: marker check failed; will attempt migration"
            );
        }
        Ok(None) => {} // not yet migrated — fall through
    }

    // ── Enumerate spaces with Procedure nodes ──────────────────────────────
    let spaces = list_procedure_spaces(store);
    if spaces.is_empty() {
        tracing::debug!("tool_memory migration: no Procedure spaces found; writing marker");
        let marker = marker_record();
        let _ = tool_stats::put_stats(adapter, &marker).await;
        return 0;
    }

    let mut migrated = 0usize;
    let mut all_ok = true;

    for space_id in &spaces {
        // ── List Procedure nodes for this space ──────────────────────────
        let nodes = match store.list_nodes_by_kind(space_id, MemoryNodeKind::Procedure, 100_000) {
            Ok(n) => n,
            Err(e) => {
                tracing::warn!(
                    space = %space_id,
                    error = %format!("{e:#}"),
                    "tool_memory migration: list_nodes_by_kind failed; skipping space"
                );
                all_ok = false;
                continue;
            }
        };

        // node_id → tool_name map for co-usage edge resolution.
        let mut tool_node_map: HashMap<String, String> = HashMap::new();

        for node in nodes {
            // ── Skip nodes that have an active version — those are skills ──
            match store.get_active_version(&node.id) {
                Ok(Some(_)) => {
                    // Has a MemoryVersion → skill, not a tool node; skip.
                    continue;
                }
                Ok(None) => {} // no version → candidate tool node
                Err(e) => {
                    tracing::warn!(
                        node = %node.id,
                        error = %format!("{e:#}"),
                        "tool_memory migration: get_active_version failed; skipping node"
                    );
                    all_ok = false;
                    continue;
                }
            }

            // ── Parse metadata as ToolNodeStats ─────────────────────────
            let stats: ToolNodeStats = match node.metadata.as_ref().and_then(|m| {
                serde_json::from_value::<ToolNodeStats>(m.clone()).ok()
            }) {
                Some(s) => s,
                None => {
                    // Metadata absent or doesn't match ToolNodeStats shape
                    // (e.g. skills that also lost their version — defensive skip).
                    tracing::debug!(
                        node = %node.id,
                        title = %node.title,
                        "tool_memory migration: metadata not ToolNodeStats; skipping node"
                    );
                    continue;
                }
            };

            // ── Validate — must have meaningful stats (at least one use) ──
            // Nodes with total_uses == 0 and no last_used_at are likely
            // orphan scaffold nodes; skip them rather than writing noise.
            if stats.total_uses == 0 && stats.last_used_at.is_empty() {
                tracing::debug!(
                    node = %node.id,
                    title = %node.title,
                    "tool_memory migration: zero-use orphan node; skipping"
                );
                continue;
            }

            // ── Write to adapter ─────────────────────────────────────────
            let record = node_to_record(
                &node.space_id,
                &node.title,
                stats.total_uses,
                stats.success_count,
                stats.failure_count,
                stats.total_latency_ms,
                stats.output_sizes,
                stats.parameter_fingerprints,
                stats.last_used_at,
            );

            if let Err(e) = tool_stats::put_stats(adapter, &record).await {
                tracing::warn!(
                    node = %node.id,
                    error = %format!("{e:#}"),
                    "tool_memory migration: put_stats failed; skipping node"
                );
                all_ok = false;
                continue;
            }

            tool_node_map.insert(node.id.clone(), node.title.clone());
            migrated += 1;
        }

        // ── Migrate co-usage edges for this space ────────────────────────
        // Only emit edges where both endpoints resolved as tool nodes.
        let raw_edges = list_relates_to_edges(store, space_id);
        for (parent_id, child_id) in raw_edges {
            let from_name = match tool_node_map.get(&parent_id) {
                Some(n) => n.clone(),
                None => continue,
            };
            let to_name = match tool_node_map.get(&child_id) {
                Some(n) => n.clone(),
                None => continue,
            };
            if let Err(e) = edges::relate(adapter, &from_name, &to_name, "co_used").await {
                tracing::warn!(
                    from = %from_name,
                    to = %to_name,
                    error = %format!("{e:#}"),
                    "tool_memory migration: relate failed; skipping edge"
                );
                all_ok = false;
            }
        }
    }

    // ── Write marker only on a fully-clean pass ───────────────────────────
    if all_ok {
        let marker = marker_record();
        if let Err(e) = tool_stats::put_stats(adapter, &marker).await {
            tracing::warn!(error = %format!("{e:#}"), "tool_memory migration: failed to write marker");
        }
    }

    tracing::info!(migrated, all_ok, "tool_memory migration pass complete");
    migrated
}

fn marker_record() -> ToolStatsRecord {
    ToolStatsRecord {
        space: MARKER_SPACE.into(),
        tool_name: MARKER_TOOL.into(),
        ..Default::default()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
    use crate::memory_graph::models::{
        MemoryEdge, MemoryNode, MemoryNodeKind, MemoryRelationKind, MemoryVisibility,
    };

    // ── Minimal in-memory MemoryAdapter ───────────────────────────────────

    struct InMemoryAdapter {
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Arc<dyn MemoryAdapter> {
            Arc::new(Self {
                store: Mutex::new(HashMap::new()),
            })
        }
    }

    #[async_trait]
    impl MemoryAdapter for InMemoryAdapter {
        fn name(&self) -> &str {
            "tool_migration_test"
        }

        async fn store(
            &self,
            namespace: &str,
            key: &str,
            content: &str,
            category: MemoryCategory,
            session_id: Option<&str>,
        ) -> anyhow::Result<()> {
            let entry = MemoryEntry {
                id: key.to_string(),
                key: key.to_string(),
                content: content.to_string(),
                namespace: Some(namespace.to_string()),
                category,
                timestamp: chrono::Utc::now().to_rfc3339(),
                session_id: session_id.map(String::from),
                score: None,
            };
            self.store
                .lock()
                .unwrap()
                .insert((namespace.to_string(), key.to_string()), entry);
            Ok(())
        }

        async fn recall(
            &self,
            _query: &str,
            _limit: usize,
            _opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            Ok(Vec::new())
        }

        async fn get(
            &self,
            namespace: &str,
            key: &str,
        ) -> anyhow::Result<Option<MemoryEntry>> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .get(&(namespace.to_string(), key.to_string()))
                .cloned())
        }

        async fn list(
            &self,
            namespace: Option<&str>,
            _category: Option<&MemoryCategory>,
            _session_id: Option<&str>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| match namespace {
                    Some(ns) => e.namespace.as_deref() == Some(ns),
                    None => true,
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            Ok(out)
        }

        async fn delete(&self, namespace: &str, key: &str) -> anyhow::Result<bool> {
            Ok(self
                .store
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()))
                .is_some())
        }

        async fn clear_namespace(&self, namespace: &str) -> anyhow::Result<u64> {
            let mut store = self.store.lock().unwrap();
            let before = store.len();
            store.retain(|(ns, _), _| ns != namespace);
            Ok((before - store.len()) as u64)
        }

        async fn namespace_summaries(&self) -> anyhow::Result<Vec<NamespaceSummary>> {
            Ok(Vec::new())
        }
    }

    // ── In-memory MemoryGraphStore constructor ────────────────────────────
    // Suppress freeze-guard via UCLAW_MEMORY_GRAPH_ALLOW_WRITES=1 (test fixtures only;
    // production migrate_tool_memory is read-only on memory_graph).

    fn fresh_graph_store() -> Arc<MemoryGraphStore> {
        std::env::set_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES", "1");
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        crate::db::migrations::run(&conn).expect("full migrations");
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    /// Insert a Procedure node with ToolNodeStats metadata (no MemoryVersion).
    fn insert_tool_node(
        store: &MemoryGraphStore,
        id: &str,
        space_id: &str,
        tool_name: &str,
        total_uses: u64,
        success_count: u64,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let metadata = serde_json::json!({
            "total_uses": total_uses,
            "success_count": success_count,
            "failure_count": total_uses - success_count,
            "total_latency_ms": total_uses * 100u64,
            "output_sizes": [],
            "parameter_fingerprints": {},
            "last_used_at": now
        });
        let node = MemoryNode {
            id: id.to_string(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: tool_name.to_string(),
            metadata: Some(metadata),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).expect("create_node");
        // No MemoryVersion created — that's what makes it a tool node.
    }

    /// Insert a RelatesTo edge between two nodes (test fixture for co-usage).
    fn insert_relates_to_edge(store: &MemoryGraphStore, space_id: &str, from_id: &str, to_id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let edge = MemoryEdge {
            id: uuid::Uuid::new_v4().to_string(),
            space_id: space_id.to_string(),
            parent_node_id: Some(from_id.to_string()),
            child_node_id: to_id.to_string(),
            relation_kind: MemoryRelationKind::RelatesTo,
            visibility: MemoryVisibility::Shared,
            priority: 1,
            trigger_text: None,
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_edge(&edge).expect("create_edge");
    }

    // ── node_to_record pure test ──────────────────────────────────────────

    #[test]
    fn node_to_record_maps_fields() {
        let mut fp = HashMap::new();
        fp.insert("p".into(), 2u64);
        let r = node_to_record(
            "sp",
            "write_file",
            5,
            4,
            1,
            100,
            vec![10, 20],
            fp,
            "t".into(),
        );
        assert_eq!(
            (r.tool_name.as_str(), r.total_uses, r.success_count, r.failure_count),
            ("write_file", 5, 4, 1)
        );
        assert_eq!(r.output_sizes, vec![10, 20]);
        assert_eq!(r.total_latency_ms, 100);
        assert_eq!(r.space.as_str(), "sp");
    }

    // ── Idempotency: marker short-circuit ────────────────────────────────

    #[tokio::test]
    async fn migrate_idempotent_when_marker_present() {
        let store = fresh_graph_store();
        // Insert a valid tool node that would otherwise be migrated.
        insert_tool_node(&store, "t1", "default", "write_file", 3, 3);

        let adapter = InMemoryAdapter::new();

        // First run — should migrate 1 tool stat.
        let first = migrate_tool_memory(&store, &adapter).await;
        assert_eq!(first, 1, "first run should migrate 1 tool stat");

        // Verify the stats were written.
        let got = tool_stats::get_stats(&adapter, "default", "write_file")
            .await
            .unwrap()
            .expect("stats should be present");
        assert_eq!(got.total_uses, 3);

        // Second run — marker present; returns 0 without re-reading the store.
        let second = migrate_tool_memory(&store, &adapter).await;
        assert_eq!(second, 0, "second run must return 0 (marker gate)");
    }

    // ── Skill nodes (have active version) are skipped ────────────────────

    #[tokio::test]
    async fn skill_nodes_with_active_version_are_skipped() {
        use crate::memory_graph::models::{MemoryVersion, MemoryVersionStatus};

        let store = fresh_graph_store();
        let now = chrono::Utc::now().to_rfc3339();

        // Insert a Procedure node with skill-style metadata (has cited_count, not total_uses).
        let node = MemoryNode {
            id: "skill-1".into(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: "My Skill".into(),
            metadata: Some(serde_json::json!({"cited_count": 2, "usage_count": 5, "lifecycle": "promoted"})),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).expect("create_node");

        // Give it an active version — this is what marks it as a skill.
        let version = MemoryVersion {
            id: "skill-1-v1".into(),
            node_id: "skill-1".into(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: "skill body".into(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).expect("create_version");

        let adapter = InMemoryAdapter::new();
        let count = migrate_tool_memory(&store, &adapter).await;

        // Node skipped (has active version → skill); 0 tool stats migrated.
        assert_eq!(count, 0);
        // Marker IS written (all_ok stays true; skip != error).
        let marker = tool_stats::get_stats(&adapter, MARKER_SPACE, MARKER_TOOL)
            .await
            .unwrap();
        assert!(marker.is_some(), "marker should be written even when 0 tool nodes migrated");
    }

    // ── Co-usage edges are migrated ───────────────────────────────────────

    #[tokio::test]
    async fn co_usage_edges_are_migrated() {
        let store = fresh_graph_store();

        insert_tool_node(&store, "t-a", "default", "write_file", 5, 5);
        insert_tool_node(&store, "t-b", "default", "run_tests", 3, 2);
        insert_relates_to_edge(&store, "default", "t-a", "t-b");

        let adapter = InMemoryAdapter::new();
        let count = migrate_tool_memory(&store, &adapter).await;

        assert_eq!(count, 2, "both tool nodes should be migrated");

        // The co-usage edge should appear in the edges namespace.
        let neighbors = edges::neighbors(&adapter, "write_file", Some("co_used"))
            .await
            .unwrap();
        assert!(
            neighbors.contains(&"run_tests".to_string()),
            "run_tests should be a co_used neighbor of write_file; got: {neighbors:?}"
        );
    }
}
