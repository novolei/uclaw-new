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

/// Increment a skill's `cited_count` by 1 (read-modify-write).
/// Returns `Some(new_count)` on success, `None` if the skill is absent.
pub async fn bump_cited(
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    slug: &str,
) -> anyhow::Result<Option<u64>> {
    match get_skill(adapter, space_id, slug).await? {
        Some(mut s) => {
            s.cited_count = s.cited_count.saturating_add(1);
            let n = s.cited_count;
            put_skill(adapter, &s).await?;
            Ok(Some(n))
        }
        None => Ok(None),
    }
}

/// Increment a skill's `usage_count` by 1 (read-modify-write).
/// Returns `true` on success, `false` if the skill is absent (no-op).
pub async fn bump_usage(
    adapter: &Arc<dyn MemoryAdapter>,
    space_id: &str,
    slug: &str,
) -> anyhow::Result<bool> {
    match get_skill(adapter, space_id, slug).await? {
        Some(mut s) => {
            s.usage_count = s.usage_count.saturating_add(1);
            put_skill(adapter, &s).await?;
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

    #[tokio::test]
    async fn bump_cited_increments_and_reports_absent() {
        let a = InMemoryAdapter::new();
        put_skill(&a, &skill("s", "S", 2)).await.unwrap();
        assert_eq!(bump_cited(&a, "default", "s").await.unwrap(), Some(3));
        assert_eq!(
            get_skill(&a, "default", "s").await.unwrap().unwrap().cited_count,
            3
        );
        assert_eq!(bump_cited(&a, "default", "absent").await.unwrap(), None);
        assert!(get_skill(&a, "default", "absent").await.unwrap().is_none());
    }

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

    #[tokio::test]
    async fn bump_usage_and_cited_scoped() {
        let a = InMemoryAdapter::new();
        put_skill(
            &a,
            &Skill {
                slug: "s".into(),
                space: "sp".into(),
                name: "S".into(),
                body: "".into(),
                usage_count: 0,
                cited_count: 0,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();
        assert_eq!(bump_cited(&a, "sp", "s").await.unwrap(), Some(1));
        assert!(bump_usage(&a, "sp", "s").await.unwrap());
        assert!(!bump_usage(&a, "sp", "absent").await.unwrap());
        let s = get_skill(&a, "sp", "s").await.unwrap().unwrap();
        assert_eq!((s.cited_count, s.usage_count), (1, 1));
    }

    /// P3-skills site C — promotion flips at threshold = 3.
    /// Simulates the exact gate logic in `record_skill_cited` / `resolve_slash_skill`:
    ///   bump_cited → if Some(n) && n >= 3 → get_skill → s.status != "promoted" → put_skill.
    #[tokio::test]
    async fn promotion_flips_status_at_threshold_3() {
        const PROMOTION_THRESHOLD: u64 = 3;
        let a = InMemoryAdapter::new();
        // Start with a draft skill at cited_count = 2 (one below threshold).
        put_skill(
            &a,
            &Skill {
                slug: "promo-test".into(),
                space: "default".into(),
                name: "PromoTest".into(),
                body: "body".into(),
                usage_count: 0,
                cited_count: 2,
                keywords: vec![],
                status: "draft".into(),
            },
        )
        .await
        .unwrap();

        // First citation takes it to 3 — should trigger promotion.
        let new_cited = bump_cited(&a, "default", "promo-test")
            .await
            .unwrap()
            .expect("skill must exist");
        assert_eq!(new_cited, 3);

        // Apply the site-C promotion logic.
        if new_cited >= PROMOTION_THRESHOLD {
            let mut s = get_skill(&a, "default", "promo-test")
                .await
                .unwrap()
                .unwrap();
            assert_eq!(s.status, "draft", "status must still be draft before flip");
            if s.status != "promoted" {
                s.status = "promoted".into();
                put_skill(&a, &s).await.unwrap();
            }
        }

        let after = get_skill(&a, "default", "promo-test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(after.status, "promoted", "status must flip to promoted at threshold");
        assert_eq!(after.cited_count, 3);

        // A second citation beyond threshold must NOT flip back from promoted.
        let new_cited2 = bump_cited(&a, "default", "promo-test")
            .await
            .unwrap()
            .expect("skill must exist");
        assert_eq!(new_cited2, 4);
        if new_cited2 >= PROMOTION_THRESHOLD {
            let mut s = get_skill(&a, "default", "promo-test")
                .await
                .unwrap()
                .unwrap();
            // Already promoted — the guard `s.status != "promoted"` short-circuits.
            if s.status != "promoted" {
                s.status = "promoted".into();
                put_skill(&a, &s).await.unwrap();
            }
        }
        let still_promoted = get_skill(&a, "default", "promo-test")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(still_promoted.status, "promoted");
        assert_eq!(still_promoted.cited_count, 4);
    }
}
