//! Build a Markdown manifest block describing the agent's available skills,
//! unioning builtin skills from `SkillsRegistry` and learned skills from
//! `MemoryGraphStore`. Injected into the system prompt by
//! `ChatDelegate::effective_system_prompt`.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §1.

use crate::memory_graph::store::MemoryGraphStore;
use crate::skills::SkillsRegistry;

const APPROX_TOKENS_PER_CHAR: f64 = 0.25;

/// Manifest re-ranking bias: learned skills whose `metadata.category` matches
/// the active strategy receive +3.0 score bonus and float to the top of the
/// learned block (stable sort preserves E3 order within tied score groups).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum StrategyBias {
    /// No category bias — default E3 ordering.
    #[default]
    Balanced,
    /// Bias toward skills tagged `category: "repair"`.
    Repair,
    /// Bias toward skills tagged `category: "optimize"`.
    Optimize,
    /// Bias toward skills tagged `category: "innovate"`.
    Innovate,
}

/// Build the manifest block to append to the system prompt.
///
/// Returns an empty String when no skills exist in either source — caller
/// is responsible for not appending separator markers around an empty body.
///
/// `bias` biases the learned-skill ranking: skills whose `metadata.category`
/// matches the active strategy receive +3.0 score and are sorted to the top
/// of the learned block (stable sort, so E3 order is preserved within ties).
/// A "Current mode: X" annotation is prepended to the header when bias != Balanced.
pub fn build_skills_manifest(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    max_tokens: usize,
    bias: StrategyBias,
) -> String {
    let entries = collect_entries(registry, store, space_id, max_entries, &bias);
    if entries.is_empty() {
        return String::new();
    }
    format_manifest(&entries, max_tokens, &bias)
}

/// Public mirror of `build_skills_manifest` that returns the structured
/// entries (rather than the formatted prompt string) so the Settings UI's
/// "active manifest" debug panel can show users exactly what's being
/// injected. Same inputs, same selection logic, same ranking — only the
/// output shape differs.
pub fn compute_active_manifest_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    bias: StrategyBias,
) -> Vec<ManifestEntry> {
    collect_entries(registry, store, space_id, max_entries, &bias)
}

/// Single row in the manifest. Public for the debug panel IPC.
///
/// `provenance` is a static string: `"builtin"` for static/borrowed skills
/// or `"learned"` for graph-stored ones. The debug-panel IPC layer
/// enriches `"builtin"` to the real tier (`"bundled"`/`"user"`/`"project"`)
/// using the registry's LoadedSkill data so users see the actual
/// provenance, not the lumped category.
#[derive(Debug, Clone)]
pub struct ManifestEntry {
    pub name: String,
    pub summary: String,
    pub provenance: &'static str,
    pub cited_count: u64,
}

fn collect_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    bias: &StrategyBias,
) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // Builtin: alphabetical (list_enabled is already alpha-sorted),
    // honors runtime disable via SkillsRegistry::disable.
    let builtin: Vec<_> = registry.list_enabled().into_iter().collect();
    for info in builtin {
        entries.push(ManifestEntry {
            name: info.name.clone(),
            summary: truncate_summary(&info.description, 100),
            provenance: "builtin",
            cited_count: 0,
        });
        if entries.len() >= max_entries {
            return entries;
        }
    }

    // Learned: E3 ranking from list_promoted_learned_skills (cited DESC, usage DESC, updated DESC),
    // then re-ranked by category bonus when bias != Balanced.
    // Fetch a larger window (max_entries * 2, capped at 200) so bias re-ranking has
    // candidates to promote even when the top E3 results aren't category-matched.
    //
    // PR-mattpocock-3: drafts are deliberately excluded here — they're still
    // reachable via skill_search and slash commands, but only `promoted`
    // skills get auto-injected into the system prompt.
    let learned_limit = max_entries.saturating_sub(entries.len());
    if learned_limit == 0 {
        return entries;
    }
    let fetch_limit = if *bias == StrategyBias::Balanced {
        learned_limit
    } else {
        (learned_limit * 2).min(200)
    };
    let learned = store
        .list_promoted_learned_skills(space_id, fetch_limit)
        .unwrap_or_else(|e| {
            tracing::warn!("skills_manifest: failed to load learned skills: {}", e);
            Vec::new()
        });

    // Collect candidates with their category, then optionally re-rank.
    let bias_category = match bias {
        StrategyBias::Balanced  => None,
        StrategyBias::Repair    => Some("repair"),
        StrategyBias::Optimize  => Some("optimize"),
        StrategyBias::Innovate  => Some("innovate"),
    };

    struct Candidate {
        entry: ManifestEntry,
        bonus: f32,
    }
    let mut candidates: Vec<Candidate> = learned.into_iter().map(|detail| {
        let summary = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| detail.node.title.clone());
        let cited_count = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("cited_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let category = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("category"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let bonus = if let (Some(bias_cat), Some(ref cat)) = (bias_category, &category) {
            if cat.as_str() == bias_cat { 3.0_f32 } else { 0.0 }
        } else {
            0.0
        };
        Candidate {
            entry: ManifestEntry {
                name: detail.node.title.clone(),
                summary: truncate_summary(&summary, 100),
                provenance: "learned",
                cited_count,
            },
            bonus,
        }
    }).collect();

    // Stable sort by bonus descending (E3 order preserved within ties).
    if *bias != StrategyBias::Balanced {
        candidates.sort_by(|a, b| b.bonus.partial_cmp(&a.bonus).unwrap_or(std::cmp::Ordering::Equal));
    }

    for cand in candidates.into_iter().take(learned_limit) {
        entries.push(cand.entry);
        if entries.len() >= max_entries {
            break;
        }
    }

    entries
}

fn format_manifest(entries: &[ManifestEntry], max_tokens: usize, bias: &StrategyBias) -> String {
    let mode_hint = match bias {
        StrategyBias::Balanced  => String::new(),
        StrategyBias::Repair    => "Current mode: 修 bug — repair-tagged skills ranked first.\n".to_string(),
        StrategyBias::Optimize  => "Current mode: 优化 — optimize-tagged skills ranked first.\n".to_string(),
        StrategyBias::Innovate  => "Current mode: 探索 — innovate-tagged skills ranked first.\n".to_string(),
    };
    let header = format!(
        "\n\n---\n\n## 你已学习到的技能 (Learned Skills)\n\n{}下面是你过往会话中沉淀的技能清单。当遇到相关问题时，先用 `skill_search` 查询，再用 `load_skill` 加载完整内容。**不强制使用**——只有当技能确实匹配当前任务时再调用。\n\n",
        mode_hint,
    );
    let footer = "\n\n**使用流程**：\n1. `skill_search(query: \"...\", top_k: 3)` → 看摘要决定\n2. `load_skill(name: \"...\", reason: \"...\")` → 拿完整指引\n3. 应用后，在回复末尾用 `> 应用技能：name — 简短原因` 标注（供未来检索）\n\n---\n";

    let mut body = String::new();
    let budget_chars = (max_tokens as f64 / APPROX_TOKENS_PER_CHAR) as usize;
    let header_footer_chars = header.len() + footer.len();
    let mut remaining = budget_chars.saturating_sub(header_footer_chars);

    for entry in entries {
        let cite = if entry.cited_count > 0 {
            format!(" · cited {}", entry.cited_count)
        } else {
            String::new()
        };
        let line = format!(
            "- **{}** [{}{}] — {}\n",
            entry.name, entry.provenance, cite, entry.summary
        );
        if line.len() > remaining {
            break;
        }
        remaining -= line.len();
        body.push_str(&line);
    }

    if body.is_empty() {
        return String::new();
    }

    format!("{}{}{}", header, body, footer)
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    let mut out: String = s.chars().take(max_chars).collect();
    if s.chars().count() > max_chars {
        // Trim at last word boundary to avoid mid-word cut
        if let Some(last_space) = out.rfind(char::is_whitespace) {
            out.truncate(last_space);
        }
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_registry_and_store_returns_empty_string() {
        // Will need a way to construct an empty MemoryGraphStore — use the
        // in-memory SQLite helper from memory_graph tests.
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        // Run minimal V4 migration so memory_nodes table exists.
        let _ = conn.lock().unwrap().execute_batch(
            crate::db::migrations::V4_MEMORY_GRAPH,
        );
        let store = MemoryGraphStore::new(conn);
        let registry = SkillsRegistry::new();

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced);
        assert!(manifest.is_empty(), "expected empty manifest, got: {}", manifest);
    }

    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
    use chrono::Utc;
    use serde_json::json;

    fn fresh_store() -> std::sync::Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        std::sync::Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned_node(
        store: &MemoryGraphStore,
        title: &str,
        summary: &str,
        cited_count: u64,
        usage_count: u64,
    ) {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": summary,
                "cited_count": cited_count,
                "usage_count": usage_count,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("# {}\n\n{}", title, summary),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).unwrap();
    }

    #[test]
    fn learned_only_renders_block_with_cited_count() {
        let store = fresh_store();
        make_learned_node(&store, "stock-research", "Cross-validate stock financials", 7, 12);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced);

        assert!(manifest.contains("## 你已学习到的技能"), "missing header");
        assert!(manifest.contains("**stock-research**"), "missing skill name");
        assert!(manifest.contains("[learned · cited 7]"), "expected cited 7 segment; got:\n{}", manifest);
        assert!(manifest.contains("Cross-validate stock financials"), "missing summary");
    }

    #[test]
    fn zero_cited_omits_cite_segment() {
        let store = fresh_store();
        make_learned_node(&store, "newly-extracted", "Just extracted, never cited yet", 0, 1);

        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced);

        assert!(manifest.contains("[learned]"), "expected bare [learned] when cited=0; got:\n{}", manifest);
        assert!(!manifest.contains("cited 0"), "must not say 'cited 0'");
    }

    #[test]
    fn max_entries_caps_count() {
        let store = fresh_store();
        for i in 0..5 {
            make_learned_node(&store, &format!("skill-{}", i), "blah", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 2, 1500, StrategyBias::Balanced);

        let lines = manifest.matches("\n- **").count();
        assert_eq!(lines, 2, "expected exactly 2 entries; got:\n{}", manifest);
    }

    #[test]
    fn long_summary_truncates_at_word_boundary() {
        let summary = "Cross-validate stock financials across Yahoo Finance Macrotrends and StockAnalysis with HTTP 403 fallback handling and many more details that should be cut off";
        let store = fresh_store();
        make_learned_node(&store, "long-skill", summary, 0, 0);
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced);

        // Should be truncated with "…"
        assert!(manifest.contains("…"), "expected truncation marker; got:\n{}", manifest);
        // Should not contain the tail of the original summary
        assert!(!manifest.contains("cut off"));
    }

    #[test]
    fn token_budget_can_truncate_entries() {
        let store = fresh_store();
        for i in 0..30 {
            make_learned_node(&store, &format!("skill-{}", i), "some summary text", 0, 0);
        }
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 100, StrategyBias::Balanced);

        let lines = manifest.matches("\n- **").count();
        assert!(lines < 30, "expected token budget to drop entries; got {} lines", lines);
    }

    #[test]
    fn builtin_emitted_before_learned_with_alpha_sort() {
        use crate::skills::{ActivationCriteria, LoadedSkill, SkillManifest};
        use std::path::PathBuf;

        let store = fresh_store();
        // Insert two learned skills with high cited_count — would rank first if there were no builtins
        make_learned_node(&store, "z-learned-skill", "Z summary", 99, 99);
        make_learned_node(&store, "a-learned-skill", "A summary", 99, 99);

        // Build a registry with two builtin skills (note: out-of-alpha order)
        let mut registry = SkillsRegistry::new();
        let mk_skill = |name: &str, description: &str| LoadedSkill {
            manifest: SkillManifest {
                name: name.to_string(),
                version: "1.0.0".to_string(),
                description: description.to_string(),
                author: String::new(),
                enabled: true,
                category: String::new(),
                activation: ActivationCriteria::default(),
                parameters: vec![],
                requires: vec![],
                tools: vec![],
                path: PathBuf::from(format!("/test/{}", name)),
            },
            prompt_content: format!("# {}\n\nbody", name),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance: crate::skills::SkillProvenance::Bundled,
        };
        registry.register(mk_skill("z-builtin", "Z builtin"));
        registry.register(mk_skill("a-builtin", "A builtin"));

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced);

        // Find positions of each skill name in the manifest body
        let a_builtin_pos = manifest.find("**a-builtin**").expect("a-builtin missing");
        let z_builtin_pos = manifest.find("**z-builtin**").expect("z-builtin missing");
        let a_learned_pos = manifest.find("**a-learned-skill**").expect("a-learned-skill missing");
        let z_learned_pos = manifest.find("**z-learned-skill**").expect("z-learned-skill missing");

        // Builtins come first (both before any learned)
        assert!(a_builtin_pos < a_learned_pos);
        assert!(z_builtin_pos < a_learned_pos);
        assert!(a_builtin_pos < z_learned_pos);
        assert!(z_builtin_pos < z_learned_pos);

        // Builtins are alphabetical: a-builtin before z-builtin
        assert!(a_builtin_pos < z_builtin_pos, "expected alphabetical builtin order");

        // Runtime disable removes a builtin from the manifest
        registry.disable("a-builtin");
        let manifest2 = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced);
        assert!(
            !manifest2.contains("**a-builtin**"),
            "runtime-disabled skill should not appear; got:\n{}",
            manifest2
        );
        assert!(
            manifest2.contains("**z-builtin**"),
            "non-disabled builtin should still appear"
        );
    }

    fn make_learned_with_category(
        store: &MemoryGraphStore,
        title: &str,
        summary: &str,
        cited_count: u64,
        usage_count: u64,
        category: Option<&str>,
    ) {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let mut meta = serde_json::json!({
            "skill_type": "learned",
            "enabled": true,
            "summary": summary,
            "cited_count": cited_count,
            "usage_count": usage_count,
        });
        if let Some(cat) = category {
            meta["category"] = serde_json::Value::String(cat.to_string());
        }
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(meta),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("# {}\n\n{}", title, summary),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).unwrap();
    }

    #[test]
    fn repair_bias_reorders_learned() {
        let store = fresh_store();
        // Insert a non-repair skill with higher E3 rank (higher cited_count)
        make_learned_with_category(&store, "optimize-skill", "Optimize DB queries", 10, 20, Some("optimize"));
        // Insert a repair skill with lower E3 rank
        make_learned_with_category(&store, "repair-skill", "Fix auth failures", 1, 1, Some("repair"));

        let registry = SkillsRegistry::new();

        // Balanced: optimize-skill (cited=10) comes before repair-skill (cited=1)
        let balanced = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced);
        let opt_pos = balanced.find("**optimize-skill**").expect("optimize-skill missing");
        let rep_pos = balanced.find("**repair-skill**").expect("repair-skill missing");
        assert!(opt_pos < rep_pos, "balanced: higher-cited skill should appear first");

        // Repair bias: repair-skill should float to the top of the learned block
        let biased = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Repair);
        let opt_biased = biased.find("**optimize-skill**").expect("optimize-skill missing in biased");
        let rep_biased = biased.find("**repair-skill**").expect("repair-skill missing in biased");
        assert!(rep_biased < opt_biased, "repair bias: repair-tagged skill must precede unmatched skill");

        // Mode hint should appear in biased manifest
        assert!(biased.contains("Current mode: 修 bug"), "mode hint missing from biased manifest");

        // Balanced manifest should NOT have mode hint
        assert!(!balanced.contains("Current mode:"), "balanced manifest must not have mode hint");
    }
}
