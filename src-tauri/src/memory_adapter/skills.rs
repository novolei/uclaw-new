//! Thin skill-store facade over `MemoryAdapter` (convergence ADR P1c).
//! Free functions — NOT trait methods — over store/get/list. A latest-wins ranked
//! keyed store (no version history). No live wiring yet; P3 repoints skill_parser here.
use std::sync::Arc;

use crate::memory_adapter::{MemoryAdapter, MemoryCategory};

const SKILLS_NAMESPACE: &str = "skills";

/// A learned skill — the ranked-keyed-store subset of skill_parser's record.
/// `slug` is the normalized-title key (write-time dedup = same slug overwrites).
/// Version history is NOT modeled (latest-wins); see the convergence ADR P1c.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Skill {
    pub slug: String,
    /// P3-skills — space_id scope (was implicit single-namespace).
    #[serde(default)]
    pub space: String,
    pub name: String,
    pub body: String,
    /// P3-skills — primary ranking signal (skill usage frequency).
    #[serde(default)]
    pub usage_count: u64,
    #[serde(default)]
    pub cited_count: u64,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub status: String,
}

/// Space-qualified storage key so identical slugs in different spaces don't collide.
fn skill_key(space: &str, slug: &str) -> String {
    format!("{space}\u{1}{slug}")
}

/// Upsert a skill (dedup by `space`+`slug`, latest-wins overwrite). key = space\x01slug, content = JSON(Skill).
pub async fn put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill) -> anyhow::Result<()> {
    let content = serde_json::to_string(skill)?;
    adapter
        .store(
            SKILLS_NAMESPACE,
            &skill_key(&skill.space, &skill.slug),
            &content,
            MemoryCategory::Core,
            None,
        )
        .await
}

/// Fetch a skill by space and slug. None if absent or content isn't a valid `Skill`.
pub async fn get_skill(
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    slug: &str,
) -> anyhow::Result<Option<Skill>> {
    match adapter.get(SKILLS_NAMESPACE, &skill_key(space_id, slug)).await? {
        Some(e) => Ok(serde_json::from_str::<Skill>(&e.content).ok()),
        None => Ok(None),
    }
}

/// Top-N skills in `space_id` by usage_count DESC, then cited_count DESC, then recency.
/// List-scans the namespace (fine for the learned-skill volume; an index is later).
/// Unparseable entries skipped.
pub async fn top_skills(
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<Skill>> {
    let entries = adapter.list(Some(SKILLS_NAMESPACE), None, None).await?;
    let mut skills: Vec<(Skill, String)> = entries
        .into_iter()
        .filter_map(|e| {
            serde_json::from_str::<Skill>(&e.content)
                .ok()
                .map(|s| (s, e.timestamp))
        })
        .filter(|(s, _)| s.space == space_id)
        .collect();
    skills.sort_by(|(a, ta), (b, tb)| {
        b.usage_count
            .cmp(&a.usage_count)
            .then(b.cited_count.cmp(&a.cited_count))
            .then(tb.cmp(ta))
    });
    Ok(skills.into_iter().take(limit).map(|(s, _)| s).collect())
}

/// Hybrid search within `space_id` over the skills namespace (semantic over summaries
/// + FTS backfill) via `BucketSealAdapter::recall_hybrid`. Infallible — returns empty
/// on any internal failure. With `InertEmbedder` the semantic leg yields zero-vectors
/// and hybrid degrades gracefully to FTS only (keyword assertion still holds in tests).
pub async fn search(
    adapter: &Arc<crate::memory_bucket_seal::BucketSealAdapter>,
    space_id: &str,
    query: &str,
    limit: usize,
) -> Vec<Skill> {
    let entries = adapter
        .recall_hybrid(query, Some(SKILLS_NAMESPACE), limit.saturating_mul(2))
        .await;
    let mut out: Vec<Skill> = entries
        .into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok())
        .filter(|s| s.space == space_id)
        .collect();
    out.truncate(limit);
    out
}

// openhuman-F: bump_cited and bump_usage removed — counter writes are now
// routed through the Procedure-authoritative path in
// proactive::skill_parser::{bump_skill_and_reproject, promote_skill_and_reproject}.

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── BucketSealAdapter helper (used by search tests) ──────────────────────

    fn fresh_bucket_seal_adapter() -> (Arc<crate::memory_bucket_seal::BucketSealAdapter>, tempfile::TempDir) {
        use crate::memory_bucket_seal::{
            score::embed::InertEmbedder,
            store::BucketSealStore,
            tree_source::InertSummariser,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = Arc::new(BucketSealStore::open(&db_path).unwrap());
        store.ensure_schema().unwrap();
        let content_root = dir.path().join("content");
        let embedder: Arc<dyn crate::memory_bucket_seal::score::embed::Embedder> =
            Arc::new(InertEmbedder::new());
        let summariser: Arc<dyn crate::memory_bucket_seal::tree_source::Summariser> =
            Arc::new(InertSummariser::new());
        let adapter = Arc::new(crate::memory_bucket_seal::BucketSealAdapter::new(
            store,
            content_root,
            embedder,
            summariser,
        ));
        (adapter, dir)
    }

    // ── Minimal in-process adapter for tests ────────────────────────────

    /// Thread-safe in-memory `MemoryAdapter` used for unit tests.
    /// Stores entries in a HashMap; `list` honors the namespace filter.
    struct InMemoryAdapter {
        /// (namespace, key) → MemoryEntry
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
            "in_memory_test"
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
            query: &str,
            limit: usize,
            opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            // Split query on whitespace so any individual term can match
            // (mirrors FTS5 OR semantics for the in-memory test adapter).
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| {
                    // Namespace filter
                    if let Some(ns) = opts.namespace {
                        if e.namespace.as_deref() != Some(ns) {
                            return false;
                        }
                    }
                    // Any term matches anywhere in content
                    let content_lower = e.content.to_lowercase();
                    terms.iter().any(|t| content_lower.contains(t.as_str()))
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out.truncate(limit);
            Ok(out)
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
            let removed = self
                .store
                .lock()
                .unwrap()
                .remove(&(namespace.to_string(), key.to_string()))
                .is_some();
            Ok(removed)
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

    // ── Test helpers ─────────────────────────────────────────────────────

    fn skill(slug: &str, name: &str, cited: u64) -> Skill {
        Skill {
            slug: slug.into(),
            space: "default".into(),
            name: name.into(),
            body: "b".into(),
            usage_count: 0,
            cited_count: cited,
            keywords: vec!["k".into()],
            status: "draft".into(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn put_then_get_round_trips_all_fields() {
        let a = InMemoryAdapter::new();
        let s = skill("intro", "Intro", 3);
        put_skill(&a, &s).await.unwrap();
        assert_eq!(get_skill(&a, "default", "intro").await.unwrap(), Some(s));
    }

    #[tokio::test]
    async fn put_same_slug_dedups_latest_wins() {
        let a = InMemoryAdapter::new();
        put_skill(&a, &skill("s", "S", 1)).await.unwrap();
        put_skill(&a, &skill("s", "S", 9)).await.unwrap();
        assert_eq!(a.list(Some("skills"), None, None).await.unwrap().len(), 1);
        assert_eq!(
            get_skill(&a, "default", "s").await.unwrap().unwrap().cited_count,
            9
        );
    }

    #[tokio::test]
    async fn top_skills_sorts_desc_and_truncates() {
        let a = InMemoryAdapter::new();
        put_skill(&a, &skill("a", "A", 1)).await.unwrap();
        put_skill(&a, &skill("b", "B", 9)).await.unwrap();
        put_skill(&a, &skill("c", "C", 5)).await.unwrap();
        let top = top_skills(&a, "default", 2).await.unwrap();
        assert_eq!(
            top.iter().map(|s| s.slug.clone()).collect::<Vec<_>>(),
            vec!["b".to_string(), "c".to_string()]
        );
    }

    // openhuman-F: bump_cited_increments_and_reports_absent removed —
    // counter writes now go through Procedure-authoritative path.

    #[tokio::test]
    async fn get_and_top_skip_malformed() {
        let a = InMemoryAdapter::new();
        a.store("skills", "bad", "not json", MemoryCategory::Core, None)
            .await
            .unwrap();
        put_skill(&a, &skill("ok", "OK", 1)).await.unwrap();
        // "bad" key is not a space-qualified key, so get_skill won't find it via normal path;
        // but top_skills will attempt to parse it and skip it.
        assert_eq!(top_skills(&a, "default", 10).await.unwrap().len(), 1);
    }

    #[test]
    fn skill_serde_defaults() {
        let s: Skill =
            serde_json::from_str(r#"{"slug":"s","name":"N","body":"b"}"#).unwrap();
        assert_eq!(s.cited_count, 0);
        assert_eq!(s.usage_count, 0);
        assert_eq!(s.space, "");
        assert!(s.keywords.is_empty());
        assert_eq!(s.status, "");
    }

    // ── New P3-skills tests ───────────────────────────────────────────────

    #[tokio::test]
    async fn top_skills_orders_by_usage_then_cited_and_scopes_by_space() {
        let a = InMemoryAdapter::new();
        put_skill(
            &a,
            &Skill {
                slug: "x".into(),
                space: "s1".into(),
                name: "X".into(),
                body: "".into(),
                usage_count: 1,
                cited_count: 9,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        put_skill(
            &a,
            &Skill {
                slug: "y".into(),
                space: "s1".into(),
                name: "Y".into(),
                body: "".into(),
                usage_count: 5,
                cited_count: 0,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        put_skill(
            &a,
            &Skill {
                slug: "z".into(),
                space: "s2".into(),
                name: "Z".into(),
                body: "".into(),
                usage_count: 99,
                cited_count: 0,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        let top = top_skills(&a, "s1", 10).await.unwrap();
        assert_eq!(
            top.iter().map(|s| s.slug.clone()).collect::<Vec<_>>(),
            vec!["y".to_string(), "x".to_string()]
        );
    }

    #[tokio::test]
    async fn same_slug_different_space_no_collision() {
        let a = InMemoryAdapter::new();
        put_skill(
            &a,
            &Skill {
                slug: "dup".into(),
                space: "s1".into(),
                name: "A".into(),
                body: "a".into(),
                usage_count: 0,
                cited_count: 0,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        put_skill(
            &a,
            &Skill {
                slug: "dup".into(),
                space: "s2".into(),
                name: "B".into(),
                body: "b".into(),
                usage_count: 0,
                cited_count: 0,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(
            get_skill(&a, "s1", "dup").await.unwrap().unwrap().name,
            "A"
        );
        assert_eq!(
            get_skill(&a, "s2", "dup").await.unwrap().unwrap().name,
            "B"
        );
    }

    // openhuman-F: bump_usage_and_cited_scoped + promotion_flips_status_at_threshold_3
    // removed — counter writes now go through Procedure-authoritative path
    // (bump_skill_and_reproject / promote_skill_and_reproject in skill_parser.rs).

    // ── openhuman-F: project_skill_node test ────────────────────────────

    /// Build a fresh in-memory MemoryGraphStore with the V4 schema.
    fn fresh_memory_graph_store() -> crate::memory_graph::store::MemoryGraphStore {
        use crate::memory_graph::store::MemoryGraphStore;
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = std::sync::Arc::new(std::sync::Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);
        store.ensure_tables();
        store
    }

    #[tokio::test]
    async fn project_skill_node_puts_matching_row_in_skills_namespace() {
        use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
        use crate::proactive::skill_parser::{normalize_title_for_dedup, project_skill_node};

        let store = fresh_memory_graph_store();
        let (bucket_adapter, _dir) = fresh_bucket_seal_adapter();
        let dyn_adapter: Arc<dyn MemoryAdapter> = bucket_adapter.clone();

        let now = chrono::Utc::now().to_rfc3339();
        let node_id = "test-node-project-skill-01".to_string();
        let title = "Test Skill For Projection";
        let space = "default";
        let body_text = "body text of the projected skill";

        // Create a Procedure node with metadata {skill_type, usage_count, cited_count, lifecycle}
        let node = MemoryNode {
            id: node_id.clone(),
            space_id: space.to_string(),
            kind: MemoryNodeKind::Procedure,
            title: title.to_string(),
            metadata: Some(serde_json::json!({
                "skill_type": "learned",
                "usage_count": 2u64,
                "cited_count": 1u64,
                "lifecycle": "promoted",
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();

        // Create an active version with the body text
        let version = MemoryVersion {
            id: "test-version-proj-01".to_string(),
            node_id: node_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: body_text.to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now.clone(),
        };
        store.create_version(&version).unwrap();

        // Project the node into the skills namespace via the new helper
        project_skill_node(&store, &dyn_adapter, &node).await;

        // Verify the projection was written: get_skill should return a matching Skill
        let slug = normalize_title_for_dedup(title);
        let projected = get_skill(&dyn_adapter, space, &slug).await.unwrap();
        assert!(projected.is_some(), "project_skill_node should write a row into the skills namespace");
        let s = projected.unwrap();
        assert_eq!(s.name, title, "name must match node title");
        assert_eq!(s.body, body_text, "body must match active version content");
        assert_eq!(s.usage_count, 2, "usage_count must come from node metadata");
        assert_eq!(s.cited_count, 1, "cited_count must come from node metadata");
        assert_eq!(s.status, "promoted", "status must come from node metadata lifecycle");
        assert_eq!(s.space, space, "space must match node space_id");
    }

    #[tokio::test]
    async fn search_finds_by_keyword_and_scopes_space() {
        let (bucket_adapter, _dir) = fresh_bucket_seal_adapter();
        // seed via put_skill which takes &Arc<dyn MemoryAdapter>
        let dyn_adapter: Arc<dyn MemoryAdapter> = bucket_adapter.clone();
        put_skill(&dyn_adapter, &Skill {
            slug: "rust-async".into(),
            space: "default".into(),
            name: "Rust async".into(),
            body: "tokio and futures".into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec!["tokio".into()],
            status: "draft".into(),
        }).await.unwrap();
        put_skill(&dyn_adapter, &Skill {
            slug: "py".into(),
            space: "default".into(),
            name: "Python".into(),
            body: "asyncio".into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec![],
            status: "draft".into(),
        }).await.unwrap();
        // recall_hybrid with InertEmbedder falls back to FTS — keyword hit still expected.
        let hits = search(&bucket_adapter, "default", "tokio", 10).await;
        assert!(hits.iter().any(|s| s.slug == "rust-async"), "expected rust-async hit for 'tokio'");
        assert!(!hits.iter().any(|s| s.slug == "py"), "py should not match 'tokio'");
    }
}
