//! Per-turn relevant skills (uclaw-pi backport plan T3; unified in P3 Task 9).
//!
//! Two sources feed one `<relevant_skills>` pointer block in the per-turn
//! dynamic context:
//! - **learned** skills: they live in bucket_seal's `skills` namespace, so
//!   the turn's general recall already surfaces the ones that match the user
//!   message. This module splits those entries out of the recall results
//!   (their raw JSON is noise in the memory block);
//! - **registry** skills (bundled / user / project / marketplace / plugin
//!   `SKILL.md`): deterministic lexical matching of this turn's message via
//!   `SkillsRegistry::match_skills` (transcript mode only).
//!
//! No extra recall (the bounded recall is single-flight, and a second scan
//! would contend for the same connection), and nothing goes into the system
//! prompt, so its cache prefix stays stable across turns.

use serde::Serialize;

use crate::memory_adapter::skills::{parse_skill_entry, Skill, SKILLS_NAMESPACE};
use crate::memory_adapter::{MemoryAdapter, MemoryEntry};
use crate::skills::SkillsRegistry;

/// At most this many learned skills are pointed at per turn.
pub const MAX_RELEVANT_SKILLS: usize = 3;

/// At most this many registry (builtin / plugin) skills are pointed at per turn.
pub const MAX_RELEVANT_REGISTRY_SKILLS: usize = 3;

/// Length cap for each skill's "when to use" line.
const WHEN_TO_USE_MAX_CHARS: usize = 120;

/// Status written by the skills backfill marker row — never a real skill.
const MIGRATION_MARKER_STATUS: &str = "_migration_marker";

/// Space whose skills apply everywhere (most learned skills live here, and
/// `skill_search` searches it).
const GLOBAL_SPACE: &str = "default";

/// Where a pointed-at skill comes from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RelevantSkillSource {
    /// Learned skill recalled from bucket_seal's `skills` namespace.
    Learned,
    /// `SKILL.md` skill from the skills registry (bundled/user/plugin/…).
    Registry,
}

/// One skill the per-turn block points the model at.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelevantSkill {
    pub name: String,
    pub when_to_use: String,
    pub source: RelevantSkillSource,
}

impl RelevantSkill {
    fn learned(skill: &Skill) -> Self {
        Self {
            name: skill.name.clone(),
            when_to_use: when_to_use(&skill.body),
            source: RelevantSkillSource::Learned,
        }
    }
}

/// Lexically match this turn's message against the skills registry.
///
/// Order is `match_skills` order (score desc, then name — deterministic);
/// disabled skills and skills of disabled plugins are already filtered out by
/// the registry. "When to use" is the manifest description, falling back to
/// the prompt body like learned skills.
pub fn registry_matches(
    registry: &SkillsRegistry,
    current_message: &str,
    max: usize,
) -> Vec<RelevantSkill> {
    let message = current_message.trim();
    if message.is_empty() || max == 0 {
        return Vec::new();
    }
    registry
        .match_skills(message)
        .into_iter()
        .take(max)
        .map(|s| {
            let desc = s.manifest.description.split_whitespace().collect::<Vec<_>>().join(" ");
            let when = if desc.is_empty() {
                when_to_use(&s.prompt_content)
            } else {
                truncate_chars(&desc, WHEN_TO_USE_MAX_CHARS)
            };
            RelevantSkill {
                name: s.manifest.name.clone(),
                when_to_use: when,
                source: RelevantSkillSource::Registry,
            }
        })
        .collect()
}

/// Learned first, then registry; de-duplicated by name (first wins).
pub fn merge(learned: Vec<RelevantSkill>, registry: Vec<RelevantSkill>) -> Vec<RelevantSkill> {
    let mut out: Vec<RelevantSkill> = Vec::with_capacity(learned.len() + registry.len());
    for skill in learned.into_iter().chain(registry) {
        if !out.iter().any(|s| s.name == skill.name) {
            out.push(skill);
        }
    }
    out
}

/// Remove every `skills`-namespace entry from `entries` and parse up to `max`
/// of them (in recall order) into learned skills for `space_id` or the global
/// `default` space (deduped by slug).
///
/// Returns `(other_entries, skills)`. Skill entries are always removed from
/// the first list — even ones that don't parse or belong to another space —
/// because their raw JSON is noise in the memory block.
pub async fn split_skill_entries(
    adapter: &dyn MemoryAdapter,
    entries: Vec<MemoryEntry>,
    space_id: &str,
    max: usize,
) -> (Vec<MemoryEntry>, Vec<RelevantSkill>) {
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
    (rest, skills.iter().map(RelevantSkill::learned).collect())
}

/// Render the per-turn pointer block. `None` when there are no skills.
pub fn render_block(skills: &[RelevantSkill]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }
    let mut out = String::from(
        "<relevant_skills>\n\
         以下技能可能与当前任务相关。确实适用时，先用 load_skill 按名称加载全文再照做；不相关就忽略。\n",
    );
    for skill in skills {
        if skill.when_to_use.is_empty() {
            out.push_str(&format!("- **{}**\n", skill.name));
        } else {
            out.push_str(&format!("- **{}**：{}\n", skill.name, skill.when_to_use));
        }
    }
    out.push_str("</relevant_skills>");
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
/// agent UI shows which skills this turn pointed the model at. `provenance`
/// uses the UI's existing vocabulary: `"learned"` | `"builtin"` (registry).
pub fn recalled_event_payload(conversation_id: &str, skills: &[RelevantSkill]) -> serde_json::Value {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let results: Vec<serde_json::Value> = skills
        .iter()
        .map(|s| {
            serde_json::json!({
                "name": s.name,
                "summary": s.when_to_use,
                "score": 0.0,
                "provenance": match s.source {
                    RelevantSkillSource::Learned => "learned",
                    RelevantSkillSource::Registry => "builtin",
                },
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

    fn rs(name: &str, when: &str, source: RelevantSkillSource) -> RelevantSkill {
        RelevantSkill { name: name.into(), when_to_use: when.into(), source }
    }

    fn registry_with(skills: &[(&str, &str, &str)]) -> (SkillsRegistry, tempfile::TempDir) {
        // (name, description, keywords-yaml-list); small token cost so the
        // registry 4000-token budget never drops a match.
        let dir = tempfile::TempDir::new().unwrap();
        for (name, desc, kws) in skills {
            let d = dir.path().join(name);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("SKILL.md"),
                format!(
                    "---\nname: {name}\nversion: \"1.0.0\"\ndescription: {desc}\nactivation:\n  keywords: {kws}\n  max_context_tokens: 100\n---\n# {name}\nbody"
                ),
            )
            .unwrap();
        }
        let mut reg = SkillsRegistry::new();
        reg.add_scan_dir(dir.path().to_path_buf(), crate::skills::SkillProvenance::Bundled);
        reg.discover();
        (reg, dir)
    }

    #[test]
    fn render_block_is_none_when_empty_and_lists_names() {
        assert!(render_block(&[]).is_none());
        let block = render_block(&[
            rs("deploy", "滚动发布", RelevantSkillSource::Learned),
            rs("bare", "", RelevantSkillSource::Registry),
        ])
        .unwrap();
        assert!(block.contains("load_skill"));
        assert!(block.contains("- **deploy**：滚动发布\n"));
        assert!(block.contains("- **bare**\n"));
    }

    #[test]
    fn render_block_uses_relevant_skills_tag() {
        let block = render_block(&[rs("deploy", "滚动发布", RelevantSkillSource::Learned)]).unwrap();
        assert!(block.starts_with("<relevant_skills>\n"), "{block}");
        assert!(block.ends_with("</relevant_skills>"), "{block}");
        assert!(!block.contains("relevant_learned_skills"), "{block}");
        assert!(block.contains("以下技能可能与当前任务相关"), "{block}");
    }

    #[test]
    fn relevant_skills_merges_registry_and_learned_dedup_by_name() {
        let learned = vec![
            rs("deploy", "learned deploy", RelevantSkillSource::Learned),
            rs("triage", "learned triage", RelevantSkillSource::Learned),
        ];
        let registry = vec![
            rs("pdf", "builtin pdf", RelevantSkillSource::Registry),
            rs("deploy", "builtin deploy", RelevantSkillSource::Registry),
        ];
        let merged = merge(learned, registry);
        let names: Vec<(&str, &RelevantSkillSource)> =
            merged.iter().map(|s| (s.name.as_str(), &s.source)).collect();
        assert_eq!(
            names,
            vec![
                ("deploy", &RelevantSkillSource::Learned),
                ("triage", &RelevantSkillSource::Learned),
                ("pdf", &RelevantSkillSource::Registry),
            ]
        );
        assert_eq!(merged[0].when_to_use, "learned deploy", "learned wins on name clash");
    }

    /// bucket_seal recall timed out (`None`) → the block is the lexical
    /// registry matches alone.
    #[test]
    fn relevant_skills_registry_only_when_recall_times_out() {
        let (reg, _dir) = registry_with(&[
            ("pdf-tools", "处理 PDF 文件", "[\"pdf\"]"),
            ("xlsx-tools", "处理表格", "[\"xlsx\"]"),
        ]);
        let recalled: Option<Vec<MemoryEntry>> = None; // recall deadline fired
        let learned: Vec<RelevantSkill> = match recalled {
            Some(_) => unreachable!(),
            None => Vec::new(),
        };
        let registry = registry_matches(&reg, "please merge this pdf", MAX_RELEVANT_REGISTRY_SKILLS);
        let merged = merge(learned, registry);
        assert_eq!(merged, vec![rs("pdf-tools", "处理 PDF 文件", RelevantSkillSource::Registry)]);
        let block = render_block(&merged).unwrap();
        assert!(block.contains("- **pdf-tools**：处理 PDF 文件\n"), "{block}");
    }

    #[test]
    fn registry_matches_is_capped_and_deterministic() {
        // Five equally-scored skills: ties break by name (not HashMap order),
        // capped at max.
        let (reg, _dir) = registry_with(&[
            ("e-skill", "e", "[\"deploy\"]"),
            ("c-skill", "c", "[\"deploy\"]"),
            ("a-skill", "a", "[\"deploy\"]"),
            ("d-skill", "d", "[\"deploy\"]"),
            ("b-skill", "b", "[\"deploy\"]"),
        ]);
        for _ in 0..5 {
            let names: Vec<String> = registry_matches(&reg, "deploy now", 3)
                .into_iter()
                .map(|s| s.name)
                .collect();
            assert_eq!(names, vec!["a-skill", "b-skill", "c-skill"]);
        }
        assert!(registry_matches(&reg, "   ", 3).is_empty());
        assert!(registry_matches(&reg, "nothing relevant", 3).is_empty());
    }

    #[test]
    fn registry_matches_skips_disabled_plugin_skill() {
        let tmp = tempfile::TempDir::new().unwrap();
        let plug = tmp.path().join("plug");
        std::fs::create_dir_all(plug.join("skills").join("ps")).unwrap();
        std::fs::write(
            plug.join("skills/ps/SKILL.md"),
            "---\nname: ps\nversion: \"1.0.0\"\ndescription: plugin pdf\nactivation:\n  keywords: [\"pdf\"]\n---\n# b",
        )
        .unwrap();
        let mut reg = SkillsRegistry::new();
        reg.discover_plugin_skills(&plug, "P");
        let map = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::from([(
            "P".to_string(),
            true,
        )])));
        reg.set_plugin_enabled_handle(map.clone());
        assert_eq!(registry_matches(&reg, "a pdf please", 3).len(), 1);

        map.write().unwrap().insert("P".to_string(), false);
        assert!(registry_matches(&reg, "a pdf please", 3).is_empty(), "disabled plugin skill");

        map.write().unwrap().insert("P".to_string(), true);
        reg.disable("ps");
        assert!(registry_matches(&reg, "a pdf please", 3).is_empty(), "disabled skill");
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
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["one", "two"], "space-scoped, deduped, marker skipped, capped");
        assert!(skills.iter().all(|s| s.source == RelevantSkillSource::Learned));
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
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["ws-skill", "global-skill"]);
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
        // Parsed from the hydrated body, not the truncated preview.
        assert_eq!(skills[0].when_to_use, when_to_use(&long.body));
    }

    #[test]
    fn event_payload_uses_inject_kind() {
        let v = recalled_event_payload(
            "sess-1",
            &[
                rs("deploy", "滚动发布", RelevantSkillSource::Learned),
                rs("pdf", "处理 PDF", RelevantSkillSource::Registry),
            ],
        );
        assert_eq!(v["kind"], "inject");
        assert_eq!(v["conversationId"], "sess-1");
        assert_eq!(v["results"][0]["name"], "deploy");
        assert_eq!(v["results"][0]["summary"], "滚动发布");
        assert_eq!(v["results"][0]["provenance"], "learned");
        assert_eq!(v["results"][1]["provenance"], "builtin");
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
        assert_eq!(skills.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), vec!["quuxify-skill"]);
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
