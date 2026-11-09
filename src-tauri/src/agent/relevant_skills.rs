//! Per-turn relevant learned skills (uclaw-pi backport plan, T3).
//!
//! Learned skills live in bucket_seal's `skills` namespace, so the turn's
//! general recall already surfaces the ones that match the user message — but
//! until now as raw, preview-truncated JSON inside the memory block. This
//! module splits those entries out of the recall results and renders them as a
//! short pointer block in the per-turn dynamic context.
//!
//! No extra recall (the bounded recall is single-flight, and a second scan
//! would contend for the same connection), and nothing goes into the system
//! prompt, so its cache prefix stays stable across turns.

use crate::memory_adapter::skills::{parse_skill_entry, Skill, SKILLS_NAMESPACE};
use crate::memory_adapter::{MemoryAdapter, MemoryEntry};

/// At most this many skills are pointed at per turn.
pub const MAX_RELEVANT_SKILLS: usize = 3;

/// Length cap for each skill's "when to use" line.
const WHEN_TO_USE_MAX_CHARS: usize = 120;

/// Status written by the skills backfill marker row — never a real skill.
const MIGRATION_MARKER_STATUS: &str = "_migration_marker";

/// Space whose skills apply everywhere (most learned skills live here, and
/// `skill_search` searches it).
const GLOBAL_SPACE: &str = "default";

/// Remove every `skills`-namespace entry from `entries` and parse up to `max`
/// of them (in recall order) into skills for `space_id` or the global
/// `default` space.
///
/// Returns `(other_entries, skills)`. Skill entries are always removed from
/// the first list — even ones that don't parse or belong to another space —
/// because their raw JSON is noise in the memory block.
pub async fn split_skill_entries(
    adapter: &dyn MemoryAdapter,
    entries: Vec<MemoryEntry>,
    space_id: &str,
    max: usize,
) -> (Vec<MemoryEntry>, Vec<Skill>) {
    let mut rest = Vec::with_capacity(entries.len());
    let mut skills: Vec<Skill> = Vec::new();
    for entry in entries {
        if entry.namespace.as_deref() != Some(SKILLS_NAMESPACE) {
            rest.push(entry);
            continue;
        }
        if skills.len() >= max {
            continue;
        }
        let Some(skill) = parse_skill_entry(adapter, &entry).await else {
            continue;
        };
        if (skill.space == space_id || skill.space == GLOBAL_SPACE)
            && skill.status != MIGRATION_MARKER_STATUS
            && !skills.iter().any(|s| s.slug == skill.slug)
        {
            skills.push(skill);
        }
    }
    (rest, skills)
}

/// Render the per-turn pointer block. `None` when there are no skills.
pub fn render_block(skills: &[Skill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut out = String::from(
        "<relevant_learned_skills>\n\
         以下已学习的技能可能与当前任务相关。确实适用时，先用 load_skill 按名称加载全文再照做；不相关就忽略。\n",
    );
    for skill in skills {
        let when = when_to_use(&skill.body);
        if when.is_empty() {
            out.push_str(&format!("- **{}**\n", skill.name));
        } else {
            out.push_str(&format!("- **{}**：{}\n", skill.name, when));
        }
    }
    out.push_str("</relevant_learned_skills>");
    Some(out)
}

/// One line saying when a skill applies: the `## 适用场景` section when the
/// body has one, else the first prose line. Whitespace is collapsed and the
/// result capped at [`WHEN_TO_USE_MAX_CHARS`].
pub fn when_to_use(body: &str) -> String {
    let mut lines = body.lines();
    let mut picked: Vec<&str> = Vec::new();
    while let Some(line) = lines.next() {
        let t = line.trim();
        if t.starts_with("##") && t.contains("适用场景") {
            for next in lines.by_ref() {
                let n = next.trim();
                if n.starts_with('#') {
                    break;
                }
                if !n.is_empty() {
                    picked.push(n);
                }
            }
            break;
        }
    }
    if picked.is_empty() {
        if let Some(first) = body
            .lines()
            .map(str::trim)
            .find(|l| !l.is_empty() && !l.starts_with('#'))
        {
            picked.push(first);
        }
    }
    let joined = picked.join(" ");
    let collapsed = joined.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_chars(&collapsed, WHEN_TO_USE_MAX_CHARS)
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max).collect();
    out.push('…');
    out
}

/// Payload for the `agent:skill-recalled` event (`kind: "inject"`), so the
/// agent UI shows which skills this turn pointed the model at.
pub fn recalled_event_payload(conversation_id: &str, skills: &[Skill]) -> serde_json::Value {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let results: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "summary": when_to_use(&s.body),
                "score": 0.0,
                "provenance": "learned",
                "cited_count": s.cited_count,
            })
        })
        .collect();
    serde_json::json!({
        "conversationId": conversation_id,
        "toolCallId": format!("inject:{timestamp}"),
        "kind": "inject",
        "results": results,
        "timestamp": timestamp,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_adapter::MemoryCategory;
    use std::sync::Arc;

    fn skill(slug: &str, space: &str, body: &str) -> Skill {
        Skill {
            slug: slug.into(),
            space: space.into(),
            name: slug.into(),
            body: body.into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec![],
            status: "draft".into(),
        }
    }

    fn entry(namespace: &str, id: &str, content: String) -> MemoryEntry {
        MemoryEntry {
            id: id.into(),
            namespace: Some(namespace.into()),
            key: id.into(),
            content,
            category: MemoryCategory::Core,
            timestamp: "2026-11-09T00:00:00Z".into(),
            session_id: None,
            score: None,
        }
    }

    fn fresh_adapter() -> (Arc<crate::memory_bucket_seal::BucketSealAdapter>, tempfile::TempDir) {
        use crate::memory_bucket_seal::{
            score::embed::InertEmbedder, store::BucketSealStore, tree_source::InertSummariser,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let adapter = Arc::new(crate::memory_bucket_seal::BucketSealAdapter::new(
            store,
            dir.path().join("content"),
            Arc::new(InertEmbedder::new()),
            Arc::new(InertSummariser::new()),
        ));
        (adapter, dir)
    }

    #[test]
    fn when_to_use_prefers_the_scenario_section() {
        let body = "# deploy\n\n## 适用场景\n需要滚动发布服务\n且不能停机\n\n## 核心原则\n先探针";
        assert_eq!(when_to_use(body), "需要滚动发布服务 且不能停机");
    }

    #[test]
    fn when_to_use_falls_back_to_first_prose_line_and_truncates() {
        assert_eq!(when_to_use("# title\n\nUse it for X.\nMore."), "Use it for X.");
        let long = format!("## 适用场景\n{}", "长".repeat(200));
        let out = when_to_use(&long);
        assert_eq!(out.chars().count(), WHEN_TO_USE_MAX_CHARS + 1);
        assert!(out.ends_with('…'));
        assert_eq!(when_to_use(""), "");
    }

    #[test]
    fn render_block_is_none_when_empty_and_lists_names() {
        assert!(render_block(&[]).is_none());
        let block = render_block(&[
            skill("deploy", "default", "## 适用场景\n滚动发布"),
            skill("bare", "default", "# bare"),
        ])
        .unwrap();
        assert!(block.starts_with("<relevant_learned_skills>"));
        assert!(block.ends_with("</relevant_learned_skills>"));
        assert!(block.contains("load_skill"));
        assert!(block.contains("- **deploy**：滚动发布\n"));
        assert!(block.contains("- **bare**\n"));
    }

    #[tokio::test]
    async fn split_moves_skills_out_scopes_space_and_caps() {
        let (adapter, _dir) = fresh_adapter();
        let json = |s: &Skill| serde_json::to_string(s).unwrap();
        let mut marker = skill("marker", "default", "");
        marker.status = MIGRATION_MARKER_STATUS.into();
        let entries = vec![
            entry("pages", "p1", "a page".into()),
            entry(SKILLS_NAMESPACE, "s1", json(&skill("one", "default", "b"))),
            entry(SKILLS_NAMESPACE, "s1dup", json(&skill("one", "default", "b"))),
            entry(SKILLS_NAMESPACE, "s2", json(&skill("other-space", "ws-2", "b"))),
            entry(SKILLS_NAMESPACE, "s3", json(&marker)),
            entry(SKILLS_NAMESPACE, "s4", "{not json".into()),
            entry(SKILLS_NAMESPACE, "s5", json(&skill("two", "default", "b"))),
            entry(SKILLS_NAMESPACE, "s6", json(&skill("three", "default", "b"))),
            entry("graph_facts", "f1", "a fact".into()),
        ];
        let (rest, skills) = split_skill_entries(adapter.as_ref(), entries, "default", 2).await;
        let rest_ids: Vec<&str> = rest.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(rest_ids, vec!["p1", "f1"], "every skills entry leaves the memory list");
        let slugs: Vec<&str> = skills.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(slugs, vec!["one", "two"], "space-scoped, deduped, marker skipped, capped");
    }

    #[tokio::test]
    async fn split_from_a_workspace_session_sees_its_space_and_default() {
        let (adapter, _dir) = fresh_adapter();
        let json = |s: &Skill| serde_json::to_string(s).unwrap();
        let entries = vec![
            entry(SKILLS_NAMESPACE, "a", json(&skill("ws-skill", "ws-1", "b"))),
            entry(SKILLS_NAMESPACE, "b", json(&skill("global-skill", "default", "b"))),
            entry(SKILLS_NAMESPACE, "c", json(&skill("foreign-skill", "ws-2", "b"))),
        ];
        let (_rest, skills) = split_skill_entries(adapter.as_ref(), entries, "ws-1", 3).await;
        let slugs: Vec<&str> = skills.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(slugs, vec!["ws-skill", "global-skill"]);
    }

    #[tokio::test]
    async fn split_hydrates_skills_longer_than_the_preview() {
        let (adapter, _dir) = fresh_adapter();
        let dyn_adapter: Arc<dyn MemoryAdapter> = adapter.clone();
        let long = skill("longskill", "default", &format!("## 适用场景\n{}", "步骤说明。".repeat(150)));
        crate::memory_adapter::skills::put_skill(&dyn_adapter, &long).await.unwrap();
        let recalled = adapter
            .recall_hybrid("longskill", Some(SKILLS_NAMESPACE), 5)
            .await;
        assert!(!recalled.is_empty(), "FTS should find the stored skill");
        let (_rest, skills) =
            split_skill_entries(adapter.as_ref(), recalled, "default", MAX_RELEVANT_SKILLS).await;
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].body, long.body);
    }

    #[test]
    fn event_payload_uses_inject_kind() {
        let v = recalled_event_payload("sess-1", &[skill("deploy", "default", "## 适用场景\n滚动发布")]);
        assert_eq!(v["kind"], "inject");
        assert_eq!(v["conversationId"], "sess-1");
        assert_eq!(v["results"][0]["name"], "deploy");
        assert_eq!(v["results"][0]["summary"], "滚动发布");
        assert!(v["toolCallId"].as_str().unwrap().starts_with("inject:"));
    }
}

#[cfg(test)]
mod load_context_tests {
    use super::*;
    use crate::memory_adapter::pages::{put_page, Page};
    use std::sync::Arc;

    /// Chat path: skills leave the formatted memory block and come back
    /// parsed; with `None` the output matches plain `load_context`.
    #[tokio::test]
    async fn load_context_with_skills_splits_skills_out_of_memory() {
        use crate::memory_bucket_seal::{
            score::embed::InertEmbedder, store::BucketSealStore, tree_source::InertSummariser,
        };
        let dir = tempfile::TempDir::new().unwrap();
        let store = Arc::new(BucketSealStore::open(&dir.path().join("chunks.db")).unwrap());
        store.ensure_schema().unwrap();
        let bs = Arc::new(crate::memory_bucket_seal::BucketSealAdapter::new(
            store,
            dir.path().join("content"),
            Arc::new(InertEmbedder::new()),
            Arc::new(InertSummariser::new()),
        ));
        let adapter: Arc<dyn MemoryAdapter> = bs.clone();
        put_page(&adapter, &Page {
            slug: "quuxify-page".into(),
            title: "Quuxify notes".into(),
            page_type: "note".into(),
            body: "How we quuxify the build.".into(),
            tags: vec![],
        })
        .await
        .unwrap();
        crate::memory_adapter::skills::put_skill(&adapter, &Skill {
            slug: "quuxify-skill".into(),
            space: "default".into(),
            name: "quuxify-skill".into(),
            body: "## 适用场景\nquuxify the build safely".into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec![],
            status: "promoted".into(),
        })
        .await
        .unwrap();
        let adapters = std::collections::HashMap::new();

        let (ctx, skills) = crate::memory_adapter::load_context_with_skills(
            &adapters, "bucket_seal", Some(&bs), false, "quuxify", 8000, vec![], Some("default"),
        )
        .await;
        assert_eq!(skills.iter().map(|s| s.slug.as_str()).collect::<Vec<_>>(), vec!["quuxify-skill"]);
        assert!(ctx.contains("quuxify the build"), "page still in memory: {ctx}");
        assert!(!ctx.contains("quuxify-skill"), "skill JSON must not reach the memory block: {ctx}");

        let plain = crate::memory_adapter::load_context(
            &adapters, "bucket_seal", Some(&bs), false, "quuxify", 8000, vec![],
        )
        .await;
        let (unsplit, none) = crate::memory_adapter::load_context_with_skills(
            &adapters, "bucket_seal", Some(&bs), false, "quuxify", 8000, vec![], None,
        )
        .await;
        assert_eq!(plain, unsplit);
        assert!(none.is_empty());
    }
}
