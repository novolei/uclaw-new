//! Build a Markdown manifest block describing the agent's available skills,
//! unioning builtin skills from `SkillsRegistry` and learned skills from
//! `MemoryGraphStore`. Injected into the system prompt by
//! `ChatDelegate::effective_system_prompt`.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §1.

use crate::browser::playwright_skills::{
    classify_playwright_skill, is_managed_playwright_skill_path,
    PlaywrightSkillCompatibilityStatus, PlaywrightSkillManifest,
};
use crate::memory_graph::store::MemoryGraphStore;
use crate::skills::{LoadedSkill, SkillsRegistry};
use std::hash::{Hash, Hasher};

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
    workspace_tags: Option<&[String]>,
) -> String {
    let entries = collect_entries(registry, store, space_id, max_entries, &bias, workspace_tags);
    if entries.is_empty() {
        return String::new();
    }

    // Best-effort: bump manifest_appearance_count for learned skills that
    // survived ranking + budget and will appear in the system prompt.
    let learned_ids: Vec<&str> = entries
        .iter()
        .filter_map(|e| e.node_id.as_deref())
        .collect();
    if !learned_ids.is_empty() {
        store.bump_manifest_appearance(&learned_ids);
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
    workspace_tags: Option<&[String]>,
) -> Vec<ManifestEntry> {
    collect_entries(registry, store, space_id, max_entries, &bias, workspace_tags)
}

/// Manifest filter rule for per-workspace skill tag scoping (V19+).
///
/// Returns true iff the skill should be included given the workspace's
/// tag set. The semantic is **"opt-in scoping with global fallback"**:
///
///   - workspace has no tags (None or empty) → always include (default)
///   - workspace has tags AND skill has no tags → include (untagged =
///     global, like a fresh-extracted learned skill before tagging)
///   - workspace has tags AND skill has tags → include iff intersection
///
/// Tag comparison is case-sensitive — normalization happened at write
/// time via `normalize_skill_tags`.
pub fn skill_matches_workspace(
    skill_tags: &[String],
    workspace_tags: Option<&[String]>,
) -> bool {
    let Some(ws) = workspace_tags else { return true };
    if ws.is_empty() { return true; }
    if skill_tags.is_empty() { return true; }
    skill_tags.iter().any(|t| ws.iter().any(|w| w == t))
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
    /// Node ID for learned skills (None for builtins). Used by
    /// `bump_manifest_appearance` to record the inclusion signal.
    pub node_id: Option<String>,
}

fn collect_entries(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
    max_entries: usize,
    bias: &StrategyBias,
    workspace_tags: Option<&[String]>,
) -> Vec<ManifestEntry> {
    let mut entries = Vec::new();

    // Builtin: alphabetical (list_enabled is already alpha-sorted),
    // honors runtime disable via SkillsRegistry::disable.
    // V19+: workspace tag filter applied via skill_matches_workspace().
    // Skills with no activation.tags are global (included regardless).
    let mut builtin: Vec<_> = registry
        .all_loaded_skills()
        .into_iter()
        .filter(|skill| registry.is_enabled(&skill.manifest.name))
        .collect();
    builtin.sort_by(|a, b| a.manifest.name.cmp(&b.manifest.name));
    for loaded in builtin {
        if let Some(playwright_skill) = managed_playwright_skill_manifest(loaded) {
            let report = classify_playwright_skill(&playwright_skill);
            if report.status != PlaywrightSkillCompatibilityStatus::Enabled {
                tracing::warn!(
                    skill = %report.name,
                    reason = ?report.reason,
                    "Skipping unavailable managed Playwright skill in agent manifest"
                );
                continue;
            }
        }
        let info = &loaded.manifest;
        if !skill_matches_workspace(&info.activation.tags, workspace_tags) {
            continue;
        }
        entries.push(ManifestEntry {
            name: info.name.clone(),
            summary: truncate_summary(&info.description, 100),
            provenance: "builtin",
            cited_count: 0,
            node_id: None,
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
    let mut candidates: Vec<Candidate> = learned.into_iter().filter_map(|detail| {
        // V19+ workspace tag filter for learned skills. Tags live in
        // `metadata.tags` (JSON array of strings) when populated, or
        // are missing entirely on legacy/untagged skills. Missing or
        // empty → global (always included if the workspace has no
        // filter or the skill is untagged).
        let skill_tags: Vec<String> = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("tags"))
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if !skill_matches_workspace(&skill_tags, workspace_tags) {
            return None;
        }

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
        Some(Candidate {
            entry: ManifestEntry {
                name: detail.node.title.clone(),
                summary: truncate_summary(&summary, 100),
                provenance: "learned",
                cited_count,
                node_id: Some(detail.node.id.clone()),
            },
            bonus,
        })
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

// ─── T3: bounded, deterministic per-session manifest ────────────────────

/// Token cap for the session skills manifest frozen into the system prompt.
pub const SYSTEM_PROMPT_MANIFEST_MAX_TOKENS: usize = 1500;
/// Entry cap for the session skills manifest frozen into the system prompt.
pub const SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES: usize = 200;

/// The skills manifest built once per session prompt frame.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct SessionManifest {
    /// Formatted Markdown block (empty when no skills are available).
    pub block: String,
    /// Skill names actually listed in `block`, in order.
    pub listed: Vec<String>,
    /// Total eligible skills (registry + promoted learned, deduped by name).
    pub total_available: usize,
    /// Node ids of the learned skills that were listed — the caller passes
    /// these to `bump_manifest_appearance` once, at frame creation.
    pub learned_node_ids: Vec<String>,
}

struct SessionEntry {
    name: String,
    summary: String,
    node_id: Option<String>,
}

/// Build the bounded, deterministic session manifest (Decision 12).
///
/// Order: Bundled (by name) → user-installed User/Project/Marketplace/Plugin
/// (by name) → promoted learned skills (E3 order). Honors runtime disable,
/// disabled-plugin filtering and managed-Playwright compatibility. Pure
/// read: does NOT bump `manifest_appearance_count` (the caller does, once).
/// Same registry + store state → byte-identical `block`.
pub fn build_session_manifest(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_id: &str,
) -> SessionManifest {
    build_session_manifest_spaces(registry, store, &[space_id])
}

/// [`build_session_manifest`] over several memory spaces: learned skills of
/// each space in `space_ids` order (e.g. the session space, then
/// `"default"` where proactive writes them), deduped by node id and name;
/// repeated space ids are queried once.
pub fn build_session_manifest_spaces(
    registry: &SkillsRegistry,
    store: &MemoryGraphStore,
    space_ids: &[&str],
) -> SessionManifest {
    use crate::skills::SkillProvenance;

    // `list_enabled` filters runtime-disabled AND disabled-plugin skills and
    // is name-sorted.
    let mut bundled = Vec::new();
    let mut installed = Vec::new();
    for info in registry.list_enabled() {
        let Some(loaded) = registry.get_loaded(&info.name) else { continue };
        if let Some(pw) = managed_playwright_skill_manifest(loaded) {
            if classify_playwright_skill(&pw).status != PlaywrightSkillCompatibilityStatus::Enabled {
                continue;
            }
        }
        let entry = SessionEntry {
            name: info.name.clone(),
            summary: truncate_summary(&info.description, 100),
            node_id: None,
        };
        match loaded.provenance {
            SkillProvenance::Bundled => bundled.push(entry),
            _ => installed.push(entry),
        }
    }
    let mut entries: Vec<SessionEntry> = bundled.into_iter().chain(installed).collect();

    let mut queried: Vec<&str> = Vec::new();
    let mut learned = Vec::new();
    for space_id in space_ids {
        if queried.contains(space_id) {
            continue;
        }
        queried.push(space_id);
        learned.extend(store.list_promoted_learned_skills(space_id, usize::MAX).unwrap_or_else(|e| {
            tracing::warn!("skills_manifest: failed to load learned skills: {}", e);
            Vec::new()
        }));
    }
    let mut seen: std::collections::HashSet<String> =
        entries.iter().map(|e| e.name.clone()).collect();
    let mut seen_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
    for detail in learned {
        if !seen_ids.insert(detail.node.id.clone()) || !seen.insert(detail.node.title.clone()) {
            continue;
        }
        let summary = detail
            .node
            .metadata
            .as_ref()
            .and_then(|m| m.get("summary"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| detail.node.title.clone());
        entries.push(SessionEntry {
            name: detail.node.title.clone(),
            summary: truncate_summary(&summary, 100),
            node_id: Some(detail.node.id.clone()),
        });
    }

    format_session_manifest(entries)
}

fn session_manifest_footer(total: usize, listed: usize) -> String {
    if listed == total {
        format!("共 {} 个技能，此处列出 {} 个。", total, listed)
    } else {
        format!(
            "共 {} 个技能，此处列出 {} 个；其余用 skill_search 查找，用 load_skill 按名称加载全文。",
            total, listed
        )
    }
}

fn format_session_manifest(entries: Vec<SessionEntry>) -> SessionManifest {
    let total = entries.len();
    if total == 0 {
        return SessionManifest::default();
    }
    const HEADER: &str = "## 可用技能\n\n";
    // Budget in bytes (UTF-8 length; conservative for CJK), same estimator
    // as `format_manifest`. Reserve the worst-case footer (listed rendered
    // with total's digit count + the "其余" clause).
    let budget = (SYSTEM_PROMPT_MANIFEST_MAX_TOKENS as f64 / APPROX_TOKENS_PER_CHAR) as usize;
    let worst_footer = format!(
        "共 {} 个技能，此处列出 {} 个；其余用 skill_search 查找，用 load_skill 按名称加载全文。",
        total, total
    );
    let reserved = HEADER.len() + 1 + worst_footer.len();
    let mut remaining = budget.saturating_sub(reserved);

    let mut body = String::new();
    let mut listed = Vec::new();
    let mut learned_node_ids = Vec::new();
    for entry in entries {
        if listed.len() >= SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES {
            break;
        }
        let line = format!("- **{}** — {}\n", entry.name, entry.summary);
        if line.len() > remaining {
            break;
        }
        remaining -= line.len();
        body.push_str(&line);
        if let Some(id) = entry.node_id {
            learned_node_ids.push(id);
        }
        listed.push(entry.name);
    }

    let block = format!(
        "{}{}\n{}",
        HEADER,
        body,
        session_manifest_footer(total, listed.len())
    );
    SessionManifest { block, listed, total_available: total, learned_node_ids }
}

fn managed_playwright_skill_manifest(skill: &LoadedSkill) -> Option<PlaywrightSkillManifest> {
    if !is_managed_playwright_skill_path(&skill.manifest.path) {
        return None;
    }

    let mut capabilities = Vec::new();
    if skill.manifest.tools.iter().any(|tool| tool.command.is_some()) {
        capabilities.push("raw_shell".to_string());
    }
    for tag in &skill.manifest.activation.tags {
        if matches!(
            tag.as_str(),
            "navigate" | "click" | "type" | "snapshot" | "screenshot" | "trace" | "raw_shell"
        ) && !capabilities.iter().any(|existing| existing == tag)
        {
            capabilities.push(tag.clone());
        }
    }
    if capabilities.is_empty() {
        capabilities.push(infer_playwright_capability(&skill.manifest.name).to_string());
    }

    Some(PlaywrightSkillManifest {
        name: skill.manifest.name.clone(),
        source_version: skill.manifest.version.clone(),
        required_capabilities: capabilities,
        hash: prompt_hash(&skill.prompt_content),
    })
}

fn prompt_hash(content: &str) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    content.hash(&mut hasher);
    format!("{:x}", hasher.finish())
}

fn infer_playwright_capability(name: &str) -> &'static str {
    let lower = name.to_ascii_lowercase();
    if lower.contains("click") {
        "click"
    } else if lower.contains("type") || lower.contains("fill") {
        "type"
    } else if lower.contains("snapshot") || lower.contains("accessibility") {
        "snapshot"
    } else if lower.contains("screenshot") {
        "screenshot"
    } else if lower.contains("trace") {
        "trace"
    } else {
        "navigate"
    }
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

    // ─── Pure filter helper: covers all four branches of the rule ────────

    #[test]
    fn skill_matches_workspace_no_filter_when_workspace_tags_missing() {
        // None == "this workspace was loaded with no filter info" — must
        // never exclude anything (graceful degradation).
        assert!(skill_matches_workspace(&[], None));
        assert!(skill_matches_workspace(&["engineering".into()], None));
    }

    #[test]
    fn skill_matches_workspace_no_filter_when_workspace_tags_empty() {
        // Empty == "user explicitly opted out of scoping" — same effect
        // as None: include everything.
        let empty: &[String] = &[];
        assert!(skill_matches_workspace(&[], Some(empty)));
        assert!(skill_matches_workspace(&["engineering".into()], Some(empty)));
    }

    #[test]
    fn skill_matches_workspace_untagged_skill_is_global() {
        // The "untagged = global" rule: a skill with no tags appears in
        // every workspace, even tag-scoped ones. Protects fresh-extracted
        // learned skills from being silently hidden.
        let ws = ["engineering".to_string()];
        assert!(skill_matches_workspace(&[], Some(&ws)));
    }

    #[test]
    fn skill_matches_workspace_intersection_required() {
        let ws = ["engineering".to_string(), "process".to_string()];

        // Skill `["engineering", "tdd"]` matches via `engineering`.
        let skill1 = ["engineering".to_string(), "tdd".to_string()];
        assert!(skill_matches_workspace(&skill1, Some(&ws)));

        // Skill `["research"]` has no overlap — excluded.
        let skill2 = ["research".to_string()];
        assert!(!skill_matches_workspace(&skill2, Some(&ws)));
    }

    #[test]
    fn skill_matches_workspace_case_sensitive() {
        // Normalization happens at write time. The filter does bare
        // comparison so we don't pay per-call lowercase costs.
        let ws = ["engineering".to_string()];
        let skill_upper = ["Engineering".to_string()];
        assert!(!skill_matches_workspace(&skill_upper, Some(&ws)),
            "filter is case-sensitive — caller must normalize before write");
    }

    // ─── Existing tests below ────────────────────────────────────────────

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

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None);
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
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None);

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
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None);

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
        let manifest = build_skills_manifest(&registry, &store, "default", 2, 1500, StrategyBias::Balanced, None);

        let lines = manifest.matches("\n- **").count();
        assert_eq!(lines, 2, "expected exactly 2 entries; got:\n{}", manifest);
    }

    #[test]
    fn long_summary_truncates_at_word_boundary() {
        let summary = "Cross-validate stock financials across Yahoo Finance Macrotrends and StockAnalysis with HTTP 403 fallback handling and many more details that should be cut off";
        let store = fresh_store();
        make_learned_node(&store, "long-skill", summary, 0, 0);
        let registry = SkillsRegistry::new();
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 1500, StrategyBias::Balanced, None);

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
        let manifest = build_skills_manifest(&registry, &store, "default", 30, 100, StrategyBias::Balanced, None);

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
            plugin_id: None,
        };
        registry.register(mk_skill("z-builtin", "Z builtin"));
        registry.register(mk_skill("a-builtin", "A builtin"));

        let manifest = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None);

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
        let manifest2 = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None);
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

    #[test]
    fn unavailable_managed_playwright_skill_is_not_injected() {
        use crate::skills::{ActivationCriteria, LoadedSkill, SkillManifest, SkillToolDef};
        use std::path::PathBuf;

        let store = fresh_store();
        let mut registry = SkillsRegistry::new();
        registry.register(LoadedSkill {
            manifest: SkillManifest {
                name: "playwright-raw-shell".to_string(),
                version: "1.0.0".to_string(),
                description: "Raw Playwright shell".to_string(),
                author: String::new(),
                enabled: true,
                category: String::new(),
                activation: ActivationCriteria::default(),
                parameters: vec![],
                requires: vec![],
                tools: vec![SkillToolDef {
                    name: "raw".to_string(),
                    description: "raw shell".to_string(),
                    parameters: serde_json::Value::Null,
                    command: Some("playwright-cli".to_string()),
                }],
                path: PathBuf::from("/tmp/uclaw/builtin-skills/playwright-cli/raw-shell"),
            },
            prompt_content: "raw".to_string(),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance: crate::skills::SkillProvenance::Bundled,
            plugin_id: None,
        });

        let manifest = build_skills_manifest(
            &registry,
            &store,
            "default",
            30,
            4000,
            StrategyBias::Balanced,
            None,
        );

        assert!(!manifest.contains("playwright-raw-shell"));
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
        let balanced = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Balanced, None);
        let opt_pos = balanced.find("**optimize-skill**").expect("optimize-skill missing");
        let rep_pos = balanced.find("**repair-skill**").expect("repair-skill missing");
        assert!(opt_pos < rep_pos, "balanced: higher-cited skill should appear first");

        // Repair bias: repair-skill should float to the top of the learned block
        let biased = build_skills_manifest(&registry, &store, "default", 30, 4000, StrategyBias::Repair, None);
        let opt_biased = biased.find("**optimize-skill**").expect("optimize-skill missing in biased");
        let rep_biased = biased.find("**repair-skill**").expect("repair-skill missing in biased");
        assert!(rep_biased < opt_biased, "repair bias: repair-tagged skill must precede unmatched skill");

        // Mode hint should appear in biased manifest
        assert!(biased.contains("Current mode: 修 bug"), "mode hint missing from biased manifest");

        // Balanced manifest should NOT have mode hint
        assert!(!balanced.contains("Current mode:"), "balanced manifest must not have mode hint");
    }

    // ─── T3: bounded deterministic session manifest ──────────────────────

    fn mk_prov_skill(
        name: &str,
        description: &str,
        provenance: crate::skills::SkillProvenance,
        plugin_id: Option<&str>,
    ) -> LoadedSkill {
        use crate::skills::{ActivationCriteria, SkillManifest};
        LoadedSkill {
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
                path: std::path::PathBuf::from(format!("/test/{}", name)),
            },
            prompt_content: format!("# {}\n\nbody", name),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance,
            plugin_id: plugin_id.map(String::from),
        }
    }

    fn approx_tokens(s: &str) -> usize {
        (s.len() as f64 * APPROX_TOKENS_PER_CHAR).ceil() as usize
    }

    #[test]
    fn session_manifest_respects_token_and_entry_caps() {
        use crate::skills::SkillProvenance;
        let store = fresh_store();

        // Short summaries → entry cap binds.
        let mut registry = SkillsRegistry::new();
        for i in 0..300 {
            registry.register(mk_prov_skill(&format!("s{:03}", i), "d", SkillProvenance::Bundled, None));
        }
        let m = build_session_manifest(&registry, &store, "default");
        assert_eq!(m.total_available, 300);
        assert!(m.listed.len() <= SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES, "listed {}", m.listed.len());
        assert!(approx_tokens(&m.block) <= SYSTEM_PROMPT_MANIFEST_MAX_TOKENS);

        // Long summaries → token cap binds.
        let long = "长".repeat(100);
        let mut registry = SkillsRegistry::new();
        for i in 0..300 {
            registry.register(mk_prov_skill(&format!("skill-{:03}", i), &long, SkillProvenance::Bundled, None));
        }
        let m = build_session_manifest(&registry, &store, "default");
        assert!(m.listed.len() < SYSTEM_PROMPT_MANIFEST_MAX_ENTRIES);
        assert!(!m.listed.is_empty());
        assert!(
            approx_tokens(&m.block) <= SYSTEM_PROMPT_MANIFEST_MAX_TOKENS,
            "block ~{} tokens",
            approx_tokens(&m.block)
        );
        assert_eq!(m.block.matches("\n- **").count(), m.listed.len());
    }

    #[test]
    fn session_manifest_is_byte_identical_across_calls() {
        use crate::skills::SkillProvenance;
        let store = fresh_store();
        make_learned_node(&store, "learned-a", "Learned A", 3, 4);
        make_learned_node(&store, "learned-b", "Learned B", 1, 1);
        let mut registry = SkillsRegistry::new();
        for i in 0..50 {
            registry.register(mk_prov_skill(&format!("b{:02}", i), "bundled desc", SkillProvenance::Bundled, None));
            registry.register(mk_prov_skill(&format!("u{:02}", i), "user desc", SkillProvenance::User, None));
        }
        let a = build_session_manifest(&registry, &store, "default");
        let b = build_session_manifest(&registry, &store, "default");
        assert_eq!(a.block, b.block);
        assert_eq!(a.listed, b.listed);
        assert_eq!(a.learned_node_ids, b.learned_node_ids);
    }

    #[test]
    fn session_manifest_orders_bundled_then_installed_then_learned() {
        use crate::skills::SkillProvenance;
        let store = fresh_store();
        make_learned_node(&store, "0-learned", "Learned", 9, 9);
        let mut registry = SkillsRegistry::new();
        registry.register(mk_prov_skill("z-bundled", "Z", SkillProvenance::Bundled, None));
        registry.register(mk_prov_skill("m-bundled", "M", SkillProvenance::Bundled, None));
        registry.register(mk_prov_skill("c-plugin", "C", SkillProvenance::Plugin, Some("p1")));
        registry.register(mk_prov_skill("a-user", "A", SkillProvenance::User, None));
        registry.register(mk_prov_skill("b-market", "B", SkillProvenance::Marketplace, None));

        let m = build_session_manifest(&registry, &store, "default");
        assert_eq!(
            m.listed,
            vec!["m-bundled", "z-bundled", "a-user", "b-market", "c-plugin", "0-learned"]
        );
        assert_eq!(m.learned_node_ids.len(), 1);
        assert!(m.block.starts_with("## 可用技能\n"), "block:\n{}", m.block);
        assert!(m.block.contains("- **a-user** — A\n"), "block:\n{}", m.block);
    }

    #[test]
    fn session_manifest_spans_session_and_default_spaces() {
        let store = fresh_store();
        make_learned_node(&store, "d-skill", "D", 5, 5);
        make_learned_node(&store, "w-skill", "W", 5, 5);
        make_learned_node(&store, "dup", "in default", 5, 5);
        make_learned_node(&store, "dup", "in ws1", 5, 5);
        {
            let conn = store.conn.lock().unwrap();
            conn.execute("UPDATE memory_nodes SET space_id = 'ws1' WHERE title = 'w-skill'", []).unwrap();
            conn.execute(
                "UPDATE memory_nodes SET space_id = 'ws1' WHERE id = (SELECT id FROM memory_nodes WHERE title = 'dup' LIMIT 1)",
                [],
            )
            .unwrap();
        }
        let reg = SkillsRegistry::new();
        let session_only = build_session_manifest(&reg, &store, "ws1");
        assert!(session_only.listed.contains(&"w-skill".to_string()));
        assert!(!session_only.listed.contains(&"d-skill".to_string()));

        let both = build_session_manifest_spaces(&reg, &store, &["ws1", "default"]);
        let pos = |n: &str| both.listed.iter().position(|x| x == n).unwrap_or_else(|| panic!("{n} missing"));
        assert!(pos("w-skill") < pos("d-skill"), "session space first: {:?}", both.listed);
        assert_eq!(both.listed.iter().filter(|n| *n == "dup").count(), 1, "deduped by name");
        assert_eq!(both.total_available, 3);
        assert_eq!(both.learned_node_ids.len(), 3);
        // Same space twice → queried once, identical to the single-space build.
        assert_eq!(
            build_session_manifest_spaces(&reg, &store, &["default", "default"]),
            build_session_manifest(&reg, &store, "default")
        );
    }

    #[test]
    fn session_manifest_footer_reports_total_and_listed() {
        use crate::skills::SkillProvenance;
        let store = fresh_store();

        // All fit → no "其余" clause.
        let mut registry = SkillsRegistry::new();
        registry.register(mk_prov_skill("a", "A", SkillProvenance::Bundled, None));
        registry.register(mk_prov_skill("b", "B", SkillProvenance::Bundled, None));
        let m = build_session_manifest(&registry, &store, "default");
        assert!(m.block.ends_with("共 2 个技能，此处列出 2 个。"), "block:\n{}", m.block);
        assert!(!m.block.contains("其余"));

        // Over the entry cap → full footer.
        let mut registry = SkillsRegistry::new();
        for i in 0..250 {
            registry.register(mk_prov_skill(&format!("s{:03}", i), "d", SkillProvenance::Bundled, None));
        }
        let m = build_session_manifest(&registry, &store, "default");
        let expected = format!(
            "共 250 个技能，此处列出 {} 个；其余用 skill_search 查找，用 load_skill 按名称加载全文。",
            m.listed.len()
        );
        assert!(m.block.ends_with(&expected), "block tail:\n{}", &m.block[m.block.len().saturating_sub(300)..]);

        // Nothing available → empty block.
        let m = build_session_manifest(&SkillsRegistry::new(), &store, "default");
        assert!(m.block.is_empty());
        assert_eq!(m.total_available, 0);
    }

    #[test]
    fn session_manifest_excludes_disabled_and_disabled_plugin_skills() {
        use crate::skills::SkillProvenance;
        let store = fresh_store();
        let mut registry = SkillsRegistry::new();
        registry.register(mk_prov_skill("keep", "K", SkillProvenance::Bundled, None));
        registry.register(mk_prov_skill("off", "O", SkillProvenance::Bundled, None));
        registry.register(mk_prov_skill("plug-on", "P", SkillProvenance::Plugin, Some("on")));
        registry.register(mk_prov_skill("plug-off", "Q", SkillProvenance::Plugin, Some("off-plugin")));
        registry.disable("off");
        let mut map = std::collections::HashMap::new();
        map.insert("on".to_string(), true);
        map.insert("off-plugin".to_string(), false);
        registry.set_plugin_enabled_handle(std::sync::Arc::new(std::sync::RwLock::new(map)));

        let m = build_session_manifest(&registry, &store, "default");
        assert_eq!(m.listed, vec!["keep", "plug-on"]);
        assert_eq!(m.total_available, 2);
        assert!(!m.block.contains("**off**"));
        assert!(!m.block.contains("plug-off"));
    }
}
