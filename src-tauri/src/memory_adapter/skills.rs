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
    pub name: String,
    pub body: String,
    #[serde(default)]
    pub cited_count: u64,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub status: String,
}

/// Upsert a skill (dedup by `slug`, latest-wins overwrite). key = slug, content = JSON(Skill).
pub async fn put_skill(adapter: &Arc<dyn MemoryAdapter>, skill: &Skill) -> anyhow::Result<()> {
    let content = serde_json::to_string(skill)?;
    adapter.store(SKILLS_NAMESPACE, &skill.slug, &content, MemoryCategory::Core, None).await
}

/// Fetch a skill by slug. None if absent or content isn't a valid `Skill`.
pub async fn get_skill(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<Option<Skill>> {
    match adapter.get(SKILLS_NAMESPACE, slug).await? {
        Some(entry) => Ok(serde_json::from_str::<Skill>(&entry.content).ok()),
        None => Ok(None),
    }
}

/// Top-N skills by `cited_count` descending. List-scans the namespace (fine for the
/// learned-skill volume; an index is later). Unparseable entries skipped.
pub async fn top_skills(adapter: &Arc<dyn MemoryAdapter>, limit: usize) -> anyhow::Result<Vec<Skill>> {
    let entries = adapter.list(Some(SKILLS_NAMESPACE), None, None).await?;
    let mut skills: Vec<Skill> = entries
        .into_iter()
        .filter_map(|e| serde_json::from_str::<Skill>(&e.content).ok())
        .collect();
    skills.sort_by(|a, b| b.cited_count.cmp(&a.cited_count));
    skills.truncate(limit);
    Ok(skills)
}

/// Increment a skill's `cited_count` by 1 (read-modify-write). `false` if absent (no-op).
pub async fn bump_cited(adapter: &Arc<dyn MemoryAdapter>, slug: &str) -> anyhow::Result<bool> {
    match get_skill(adapter, slug).await? {
        Some(mut skill) => {
            skill.cited_count = skill.cited_count.saturating_add(1);
            put_skill(adapter, &skill).await?;
            Ok(true)
        }
        None => Ok(false),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};

    // ── Minimal in-process adapter for tests ────────────────────────────

    /// Thread-safe in-memory `MemoryAdapter` used for unit tests.
    /// Stores entries in a HashMap; `list` honors the namespace filter.
    struct InMemoryAdapter {
        /// (namespace, key) → MemoryEntry
        store: Mutex<HashMap<(String, String), MemoryEntry>>,
    }

    impl InMemoryAdapter {
        fn new() -> Self {
            Self {
                store: Mutex::new(HashMap::new()),
            }
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
            name: name.into(),
            body: "b".into(),
            cited_count: cited,
            keywords: vec!["k".into()],
            status: "draft".into(),
        }
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn put_then_get_round_trips_all_fields() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        let s = skill("intro", "Intro", 3);
        put_skill(&a, &s).await.unwrap();
        assert_eq!(get_skill(&a, "intro").await.unwrap(), Some(s));
    }

    #[tokio::test]
    async fn put_same_slug_dedups_latest_wins() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        put_skill(&a, &skill("s", "S", 1)).await.unwrap();
        put_skill(&a, &skill("s", "S", 9)).await.unwrap();
        assert_eq!(a.list(Some("skills"), None, None).await.unwrap().len(), 1);
        assert_eq!(get_skill(&a, "s").await.unwrap().unwrap().cited_count, 9);
    }

    #[tokio::test]
    async fn top_skills_sorts_desc_and_truncates() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        put_skill(&a, &skill("a", "A", 1)).await.unwrap();
        put_skill(&a, &skill("b", "B", 9)).await.unwrap();
        put_skill(&a, &skill("c", "C", 5)).await.unwrap();
        let top = top_skills(&a, 2).await.unwrap();
        assert_eq!(
            top.iter().map(|s| s.slug.clone()).collect::<Vec<_>>(),
            vec!["b".to_string(), "c".to_string()]
        );
    }

    #[tokio::test]
    async fn bump_cited_increments_and_reports_absent() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        put_skill(&a, &skill("s", "S", 2)).await.unwrap();
        assert!(bump_cited(&a, "s").await.unwrap());
        assert_eq!(get_skill(&a, "s").await.unwrap().unwrap().cited_count, 3);
        assert!(!bump_cited(&a, "absent").await.unwrap());
        assert!(get_skill(&a, "absent").await.unwrap().is_none());
    }

    #[tokio::test]
    async fn get_and_top_skip_malformed() {
        let a: Arc<dyn MemoryAdapter> = Arc::new(InMemoryAdapter::new());
        a.store("skills", "bad", "not json", MemoryCategory::Core, None)
            .await
            .unwrap();
        put_skill(&a, &skill("ok", "OK", 1)).await.unwrap();
        assert_eq!(get_skill(&a, "bad").await.unwrap(), None);
        assert_eq!(top_skills(&a, 10).await.unwrap().len(), 1);
    }

    #[test]
    fn skill_serde_defaults() {
        let s: Skill =
            serde_json::from_str(r#"{"slug":"s","name":"N","body":"b"}"#).unwrap();
        assert_eq!(s.cited_count, 0);
        assert!(s.keywords.is_empty());
        assert_eq!(s.status, "");
    }
}
