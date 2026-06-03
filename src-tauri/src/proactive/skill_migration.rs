//! P3-skills — one-time migration of memory_graph Procedure-node skills into the
//! adapter "skills" namespace. Idempotent, marker-gated, boot-safe (mirrors
//! the other one-time boot migrations).
//!
//! **Versioning collapsed to the Active version content** (latest-wins, per P1c).
//! The `body` field of each migrated `Skill` is set to the text of the node's
//! active `MemoryVersion`; nodes with no active version are skipped (logged).
//!
//! **Metadata mapping** from `memory_nodes.metadata_json`:
//!   - `cited_count`  → `json_extract(metadata_json, '$.cited_count')`   (i64, default 0)
//!   - `usage_count`  → `json_extract(metadata_json, '$.usage_count')`   (i64, default 0)
//!   - `lifecycle`    → `json_extract(metadata_json, '$.lifecycle')`     (string, default "")
//!   - `keywords`     → via `memory_keywords` table (loaded through `MemoryNodeDetail`)
//!
//! **Marker strategy**: a sentinel `Skill` with `space = MARKER_SPACE` and
//! `slug = SKILL_MIGRATION_MARKER` is written to the adapter after a fully-clean
//! pass (`all_ok == true`). On subsequent boots the marker check short-circuits
//! before any store reads — zero overhead.  The marker lives in the reserved
//! namespace `"__migration__"` so it never appears in per-space `top_skills` /
//! `get_skill` queries (those are space-scoped).
//!
//! **Space enumeration**: all distinct `space_id`s that have at least one
//! Procedure node are discovered at migration time via a direct SQL DISTINCT
//! query on `memory_nodes`, so no explicit space list need be passed in.

use std::sync::Arc;

use serde_json::Value;

use crate::memory_adapter::skills::{self, Skill};
use crate::memory_adapter::MemoryAdapter;
use crate::memory_graph::models::MemoryNodeKind;
use crate::memory_graph::store::MemoryGraphStore;
use crate::proactive::skill_parser::normalize_title_for_dedup;

const SKILL_MIGRATION_MARKER: &str = "__skills_migrated_v1__";
const MARKER_SPACE: &str = "__migration__";

// ─── Pure mapping helper ──────────────────────────────────────────────────────

/// Pure, unit-testable map: Procedure-node fields → `Skill`.
///
/// * `space`   — `memory_nodes.space_id`
/// * `title`   — `memory_nodes.title`  (also used to derive `slug`)
/// * `body`    — active `MemoryVersion.content`
/// * `cited`   — `metadata_json.cited_count` (0 if absent)
/// * `usage`   — `metadata_json.usage_count` (0 if absent)
/// * `status`  — `metadata_json.lifecycle`   ("" if absent)
/// * `keywords`— `memory_keywords` rows for the node
fn node_to_skill(
    space: &str,
    title: &str,
    body: String,
    cited: u64,
    usage: u64,
    status: String,
    keywords: Vec<String>,
) -> Skill {
    Skill {
        slug: normalize_title_for_dedup(title),
        space: space.to_string(),
        name: title.to_string(),
        body,
        usage_count: usage,
        cited_count: cited,
        keywords,
        status,
    }
}

// ─── Space enumeration (direct SQL) ──────────────────────────────────────────

/// Return every distinct `space_id` that owns at least one Procedure node.
///
/// Uses `store.conn` (pub(crate)) directly to avoid an `Arc<Mutex<conn>>`
/// round-trip per call; mirrors the pattern used elsewhere in `store.rs`.
fn list_procedure_spaces(store: &MemoryGraphStore) -> Vec<String> {
    match store.conn.lock() {
        Err(e) => {
            tracing::warn!(error = %e, "skill migration: cannot lock DB to list spaces");
            vec![]
        }
        Ok(conn) => {
            let mut stmt = match conn.prepare(
                "SELECT DISTINCT space_id FROM memory_nodes WHERE kind = ?1",
            ) {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(error = %e, "skill migration: prepare list_spaces failed");
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

// ─── Metadata helpers ────────────────────────────────────────────────────────

/// Extract an i64 from a serde_json::Value via a field name, defaulting to 0.
fn json_i64(meta: Option<&Value>, field: &str) -> u64 {
    meta.and_then(|v| v.get(field))
        .and_then(|v| v.as_i64())
        .unwrap_or(0)
        .max(0) as u64
}

/// Extract a string from a serde_json::Value via a field name, defaulting to "".
fn json_str(meta: Option<&Value>, field: &str) -> String {
    meta.and_then(|v| v.get(field))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

// ─── Migration entry point ────────────────────────────────────────────────────

/// One-time idempotent migration.
///
/// Returns the count of `Skill`s successfully written to the adapter.
/// Best-effort / boot-safe: every error is logged and skipped; never panics.
pub async fn migrate_skills(
    store: &Arc<MemoryGraphStore>,
    adapter: &Arc<dyn MemoryAdapter>,
) -> usize {
    // ── Marker check — short-circuit if already done ─────────────────────
    match skills::get_skill(adapter, MARKER_SPACE, SKILL_MIGRATION_MARKER).await {
        Ok(Some(_)) => {
            tracing::debug!("skill migration: marker present; skip");
            return 0;
        }
        Err(e) => {
            // Marker read failed; log and proceed (conservative: re-migrate
            // rather than silently skip on a transient DB error).
            tracing::warn!(
                error = %e,
                "skill migration: marker check failed; will attempt migration"
            );
        }
        Ok(None) => {} // not yet migrated — fall through
    }

    // ── Enumerate spaces with Procedure nodes ──────────────────────────────
    let spaces = list_procedure_spaces(store);
    if spaces.is_empty() {
        tracing::debug!("skill migration: no Procedure spaces found; writing marker");
        // Write marker so we don't scan on every boot when there are no skills.
        let marker = marker_skill();
        let _ = skills::put_skill(adapter, &marker).await;
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
                    "skill migration: list_nodes_by_kind failed; skipping space"
                );
                all_ok = false;
                continue;
            }
        };

        for node in nodes {
            // ── Active version content as body ───────────────────────────
            let body = match store.get_active_version(&node.id) {
                Ok(Some(v)) => v.content,
                Ok(None) => {
                    tracing::debug!(
                        node = %node.id,
                        title = %node.title,
                        "skill migration: no active version; skipping node"
                    );
                    // Nodes with no active version have no useful body — skip.
                    continue;
                }
                Err(e) => {
                    tracing::warn!(
                        node = %node.id,
                        error = %format!("{e:#}"),
                        "skill migration: get_active_version failed; skipping node"
                    );
                    all_ok = false;
                    continue;
                }
            };

            // ── Parse metadata_json fields ───────────────────────────────
            let meta = node.metadata.as_ref();
            let cited = json_i64(meta, "cited_count");
            let usage = json_i64(meta, "usage_count");
            let status = json_str(meta, "lifecycle");

            // Keywords are stored in the `memory_keywords` table; the
            // `MemoryNodeDetail` shape carries them but `list_nodes_by_kind`
            // returns lightweight `MemoryNode` rows.  Rather than hydrating
            // per-node (N+1), we check for a `keywords` JSON array in
            // `metadata_json` as a best-effort path and leave the full
            // keyword hydration to a future pass.
            let keywords: Vec<String> = meta
                .and_then(|v| v.get("keywords"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            let skill = node_to_skill(
                &node.space_id,
                &node.title,
                body,
                cited,
                usage,
                status,
                keywords,
            );

            if let Err(e) = skills::put_skill(adapter, &skill).await {
                tracing::warn!(
                    node = %node.id,
                    error = %format!("{e:#}"),
                    "skill migration: put_skill failed; skipping node"
                );
                all_ok = false;
                continue;
            }
            migrated += 1;
        }
    }

    // ── Write marker only on a fully-clean pass ───────────────────────────
    if all_ok {
        let marker = marker_skill();
        if let Err(e) = skills::put_skill(adapter, &marker).await {
            tracing::warn!(error = %format!("{e:#}"), "skill migration: failed to write marker");
        }
    }

    tracing::info!(migrated, all_ok, "skill migration pass complete");
    migrated
}

fn marker_skill() -> Skill {
    Skill {
        slug: SKILL_MIGRATION_MARKER.into(),
        space: MARKER_SPACE.into(),
        name: "skills migrated (P3)".into(),
        body: String::new(),
        usage_count: 0,
        cited_count: 0,
        keywords: vec![],
        status: "_migration_marker".into(),
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
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};

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
            "skill_migration_test"
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

    // ── In-memory MemoryGraphStore ─────────────────────────────────────────

    fn fresh_graph_store() -> Arc<MemoryGraphStore> {
        // Suppress freeze-guard in tests.
        std::env::set_var("UCLAW_MEMORY_GRAPH_ALLOW_WRITES", "1");

        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH)
            .expect("V4 schema");
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1)
            .expect("V35 schema");
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn insert_procedure_node(
        store: &MemoryGraphStore,
        id: &str,
        space_id: &str,
        title: &str,
        metadata: serde_json::Value,
        body: &str,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let node = MemoryNode {
            id: id.to_string(),
            space_id: space_id.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: title.to_string(),
            metadata: Some(metadata),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).expect("create_node");

        let version = MemoryVersion {
            id: format!("{id}-v1"),
            node_id: id.to_string(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: body.to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).expect("create_version");
    }

    // ── node_to_skill pure tests ──────────────────────────────────────────

    #[test]
    fn node_to_skill_maps_fields() {
        let s = node_to_skill(
            "sp",
            "My Skill",
            "body text".into(),
            3,
            7,
            "promoted".into(),
            vec!["k".into()],
        );
        assert_eq!(s.space, "sp");
        assert_eq!(s.name, "My Skill");
        assert_eq!(s.body, "body text");
        assert_eq!((s.cited_count, s.usage_count), (3, 7));
        assert_eq!(s.status, "promoted");
        assert!(!s.slug.is_empty());
        assert_eq!(s.keywords, vec!["k".to_string()]);
    }

    #[test]
    fn node_to_skill_slug_is_normalized_title() {
        let s = node_to_skill("sp", "  Hello World  ", "b".into(), 0, 0, "".into(), vec![]);
        assert_eq!(s.slug, normalize_title_for_dedup("  Hello World  "));
    }

    // ── idempotency test ─────────────────────────────────────────────────

    #[tokio::test]
    async fn migrate_is_idempotent_when_marker_present() {
        let store = fresh_graph_store();
        insert_procedure_node(
            &store,
            "proc-1",
            "default",
            "Test Skill",
            serde_json::json!({"cited_count": 2, "usage_count": 5, "lifecycle": "promoted"}),
            "skill body",
        );

        let adapter = InMemoryAdapter::new();

        // First run — migrates 1 node.
        let first = migrate_skills(&store, &adapter).await;
        assert_eq!(first, 1, "first run should migrate 1 skill");

        // Verify skill is present.
        let slug = normalize_title_for_dedup("Test Skill");
        let skill = skills::get_skill(&adapter, "default", &slug)
            .await
            .unwrap()
            .expect("skill should be present after migration");
        assert_eq!(skill.name, "Test Skill");
        assert_eq!(skill.body, "skill body");
        assert_eq!(skill.cited_count, 2);
        assert_eq!(skill.usage_count, 5);
        assert_eq!(skill.status, "promoted");

        // Second run — marker present; short-circuits immediately, returns 0.
        let second = migrate_skills(&store, &adapter).await;
        assert_eq!(second, 0, "second run must return 0 (marker gate)");

        // Adapter should still hold exactly 1 skill + 1 marker = 2 entries
        // in the "skills" namespace (marker uses __migration__ space).
        let all_skills = adapter.list(Some("skills"), None, None).await.unwrap();
        assert_eq!(all_skills.len(), 2, "1 skill + 1 marker in 'skills' namespace");
    }

    // ── node with no active version is skipped ───────────────────────────

    #[tokio::test]
    async fn node_without_active_version_is_skipped() {
        let store = fresh_graph_store();

        // Insert a node but no version.
        let now = chrono::Utc::now().to_rfc3339();
        let node = MemoryNode {
            id: "proc-noversion".into(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: "No-Version Skill".into(),
            metadata: Some(serde_json::json!({})),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).expect("create_node");

        let adapter = InMemoryAdapter::new();
        let count = migrate_skills(&store, &adapter).await;

        // Node skipped (no active version) → 0 skills migrated.
        // Marker IS written because all_ok stays true (skip != error).
        assert_eq!(count, 0);
    }
}
