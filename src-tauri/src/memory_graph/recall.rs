use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::info;

use super::models::*;
use super::search::MemorySearchResult;
use super::store::MemoryGraphStore;
use crate::memu::client::MemUClient;

// ─── Recall Configuration ─────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct MemoryRecallConfig {
    pub boot_limit: usize,
    pub trigger_limit: usize,
    pub seed_limit: usize,
    pub expansion_limit: usize,
    pub recent_limit: usize,
    pub fusion_strategy: FusionStrategy,
    pub rrf_k: u32,
    pub fts_weight: f32,
    pub vector_weight: f32,
    /// How many top-N learned skills to auto-mount in the boot layer
    /// regardless of query relevance. Set to 0 to disable.
    /// Skills are ranked by usage_count DESC then updated_at DESC.
    pub boot_learned_skills_limit: usize,
}

#[derive(Debug, Clone)]
pub enum FusionStrategy {
    Rrf,
    Weighted,
}

impl Default for MemoryRecallConfig {
    fn default() -> Self {
        Self {
            boot_limit: 8,
            trigger_limit: 6,
            seed_limit: 8,
            expansion_limit: 6,
            recent_limit: 3,
            fusion_strategy: FusionStrategy::Rrf,
            rrf_k: 60,
            fts_weight: 0.5,
            vector_weight: 0.5,
            boot_learned_skills_limit: 3,
        }
    }
}

// ─── Recall Candidate ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallCandidate {
    pub node_id: String,
    pub title: String,
    pub content: String,
    pub kind: MemoryNodeKind,
    pub source: String,
    pub reason: String,
    pub score: Option<f32>,
    pub fts_rank: Option<u32>,
    pub vector_rank: Option<u32>,
    pub matched_keywords: Vec<String>,
    pub metadata: Option<serde_json::Value>,
}

// ─── Timeline Entry ───────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryTimelineEntry {
    pub node_id: String,
    pub title: String,
    pub content_snippet: String,
    pub kind: MemoryNodeKind,
    pub updated_at: String,
}

// ─── Recall Plan ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallPlan {
    pub boot: Vec<MemoryRecallCandidate>,
    pub triggered: Vec<MemoryRecallCandidate>,
    pub relevant: Vec<MemoryRecallCandidate>,
    pub expanded: Vec<MemoryRecallCandidate>,
    pub recent: Vec<MemoryTimelineEntry>,
}

// ─── Recall Explanation (debug) ───────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemoryRecallExplanation {
    pub query: String,
    pub boot: Vec<MemoryRecallCandidate>,
    pub triggered: Vec<MemoryRecallCandidate>,
    pub relevant: Vec<MemoryRecallCandidate>,
    pub expanded: Vec<MemoryRecallCandidate>,
    pub recent: Vec<MemoryTimelineEntry>,
    pub total_candidates: usize,
}

// ─── Recall Engine ────────────────────────────────────────────────────────

pub struct MemoryRecallEngine {
    store: Arc<MemoryGraphStore>,
    memu_client: Option<Arc<MemUClient>>,
    config: MemoryRecallConfig,
}

impl MemoryRecallEngine {
    pub fn new(
        store: Arc<MemoryGraphStore>,
        memu_client: Option<Arc<MemUClient>>,
        config: MemoryRecallConfig,
    ) -> Self {
        Self {
            store,
            memu_client,
            config,
        }
    }

    /// Increment usage_count on every learned-skill candidate that was
    /// rendered into the prompt. Call this AFTER format_recall_for_prompt
    /// has been invoked and the system prompt has been handed to the LLM —
    /// the count tracks "how often this skill influenced a turn", which
    /// is the signal `boot_learned_skills_limit` ranks by.
    ///
    /// Best-effort: errors are logged and swallowed. Failing to bump
    /// usage_count must never fail an agent turn.
    pub fn record_used_skills(&self, plan: &MemoryRecallPlan) {
        let ids = collect_emitted_skill_ids(plan);
        if ids.is_empty() {
            return;
        }
        let id_refs: Vec<&str> = ids.iter().map(String::as_str).collect();
        if let Err(e) = self.store.bump_skill_usage(&id_refs) {
            tracing::warn!(
                count = ids.len(),
                err = %e,
                "memory_graph: failed to bump skill usage_count (non-fatal)"
            );
        }
    }

    /// Build a complete 5-layer recall plan.
    pub async fn build_recall_plan(
        &self,
        space_id: &str,
        user_input: &str,
        is_group_chat: bool,
    ) -> anyhow::Result<MemoryRecallPlan> {
        let mut seen = HashSet::<String>::new();

        // L1 — Boot
        let boot = self.layer_boot(space_id, is_group_chat, &mut seen)?;
        info!(count = boot.len(), "recall: L1 boot candidates");

        // L2 — Triggered
        let triggered = self.layer_triggered(space_id, user_input, &mut seen)?;
        info!(count = triggered.len(), "recall: L2 triggered candidates");

        // L3 — Relevant (seed)
        let relevant = self.layer_relevant(space_id, user_input, &mut seen).await?;
        info!(count = relevant.len(), "recall: L3 relevant candidates");

        // L4 — Expanded (graph walk)
        let expanded = self.layer_expanded(&relevant, &mut seen)?;
        info!(count = expanded.len(), "recall: L4 expanded candidates");

        // L5 — Recent
        let recent = self.layer_recent(space_id)?;
        info!(count = recent.len(), "recall: L5 recent entries");

        Ok(MemoryRecallPlan {
            boot,
            triggered,
            relevant,
            expanded,
            recent,
        })
    }

    /// Build a recall plan with full debug explanation.
    pub async fn explain_recall(
        &self,
        space_id: &str,
        user_input: &str,
    ) -> anyhow::Result<MemoryRecallExplanation> {
        let plan = self.build_recall_plan(space_id, user_input, false).await?;
        let total = plan.boot.len()
            + plan.triggered.len()
            + plan.relevant.len()
            + plan.expanded.len()
            + plan.recent.len();

        Ok(MemoryRecallExplanation {
            query: user_input.to_string(),
            boot: plan.boot,
            triggered: plan.triggered,
            relevant: plan.relevant,
            expanded: plan.expanded,
            recent: plan.recent,
            total_candidates: total,
        })
    }

    /// Format a recall plan as injectable system-prompt text.
    pub fn format_recall_for_prompt(plan: &MemoryRecallPlan) -> String {
        // Note: the emit-detection logic below is duplicated by
        // collect_emitted_skill_ids() at the bottom of this file. Keep
        // the two in sync — both must agree on what counts as "rendered
        // into the Learned Skills section" for usage_count to be honest.
        let mut out = String::from("<memory_context>\n");

        // Collect all learned skills from every layer for a dedicated section
        let all_layers: Vec<&MemoryRecallCandidate> = plan
            .boot
            .iter()
            .chain(plan.triggered.iter())
            .chain(plan.relevant.iter())
            .chain(plan.expanded.iter())
            .collect();

        let mut learned_skills: Vec<&MemoryRecallCandidate> = Vec::new();
        let mut skill_ids: HashSet<String> = HashSet::new();

        for c in &all_layers {
            if c.kind == MemoryNodeKind::Procedure {
                if let Some(ref meta) = c.metadata {
                    let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    let skill_type = meta.get("skill_type").and_then(|v| v.as_str()).unwrap_or("");
                    if enabled && skill_type == "learned" {
                        learned_skills.push(c);
                        skill_ids.insert(c.node_id.clone());
                    } else if !enabled {
                        // disabled skills are filtered out entirely
                        skill_ids.insert(c.node_id.clone());
                    }
                }
            }
        }

        // Helper closure: should this candidate be rendered in its normal section?
        let should_render_normal = |c: &MemoryRecallCandidate| -> bool {
            if skill_ids.contains(&c.node_id) {
                return false; // handled in learned skills section or disabled
            }
            // Filter out disabled Procedure nodes that weren't caught above
            if c.kind == MemoryNodeKind::Procedure {
                if let Some(ref meta) = c.metadata {
                    let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
                    if !enabled {
                        return false;
                    }
                }
            }
            true
        };

        // ── Learned Skills section (top priority) ──
        // E1: stronger directive — softer wording lets the LLM treat
        // the section as background reading. Now we require an explicit
        // citation ("> 应用技能：X — 因为 Y") at the start of any
        // response that uses one. This makes citation observable
        // (downstream code can detect "> 应用技能：" prefix) and makes
        // the agent more likely to actually follow the SOP rather than
        // just having it visible in context.
        if !learned_skills.is_empty() {
            out.push_str("## Learned Skills (已学技能)\n");
            out.push_str(
                "以下是你已学会的技能 SOP。**强制规则**：\n\
                 1. 任务开始前先判断本次请求是否匹配下列任一技能的「适用场景」。\n\
                 2. 如匹配，**必须**在你的响应开头以引用块形式声明：\n   \
                 `> 应用技能：<技能名> — <一句话说明为何匹配>`\n   \
                 然后严格按该技能的「SOP 步骤」执行，不要绕过 / 简化 / 自创流程。\n\
                 3. 如不匹配任何技能，无需声明，正常回复即可。\n\
                 这一规则用于积累\"技能是否真正被使用\"的反馈数据，请配合执行。\n\n",
            );
            for c in &learned_skills {
                let meta = c.metadata.as_ref().unwrap(); // safe: checked above
                let context = meta.get("context").and_then(|v| v.as_str()).unwrap_or("");
                let principles = meta.get("principles").and_then(|v| v.as_str()).unwrap_or("");
                let steps = meta.get("steps").and_then(|v| v.as_str()).unwrap_or("");
                let pitfalls = meta.get("pitfalls").and_then(|v| v.as_str()).unwrap_or("");

                out.push_str(&format!("### {}\n", c.title));
                if !context.is_empty() {
                    out.push_str(&format!("**适用场景**: {}\n", context));
                }
                if !principles.is_empty() {
                    out.push_str(&format!("**核心原则**: {}\n", principles));
                }
                if !steps.is_empty() {
                    out.push_str("**SOP 步骤**:\n");
                    for step in steps.lines() {
                        if step.trim().is_empty() {
                            continue;
                        }
                        out.push_str(&format!("{}\n", step));
                    }
                }
                if !pitfalls.is_empty() {
                    out.push_str(&format!("**注意事项**: {}\n", pitfalls));
                }
                out.push('\n');
            }
        }

        // ── Normal sections (filtering out learned/disabled skills) ──
        let boot_normal: Vec<_> = plan.boot.iter().filter(|c| should_render_normal(c)).collect();
        if !boot_normal.is_empty() {
            out.push_str("## Boot Memories (启动记忆)\n");
            for c in &boot_normal {
                let snippet = truncate_content(&c.content, 200);
                out.push_str(&format!(
                    "- [{}] {}: {}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    snippet,
                ));
            }
            out.push('\n');
        }

        let triggered_normal: Vec<_> = plan.triggered.iter().filter(|c| should_render_normal(c)).collect();
        if !triggered_normal.is_empty() {
            out.push_str("## Triggered Memories (触发记忆)\n");
            for c in &triggered_normal {
                let snippet = truncate_content(&c.content, 200);
                let kw_display = if c.matched_keywords.is_empty() {
                    String::new()
                } else {
                    format!("（触发词：{}）", c.matched_keywords.join(", "))
                };
                out.push_str(&format!(
                    "- [{}] {}: {}{}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    snippet,
                    kw_display,
                ));
            }
            out.push('\n');
        }

        let relevant_normal: Vec<_> = plan.relevant.iter().filter(|c| should_render_normal(c)).collect();
        if !relevant_normal.is_empty() {
            out.push_str("## Relevant Memories (相关记忆)\n");
            for c in &relevant_normal {
                let snippet = truncate_content(&c.content, 200);
                let score_display = c
                    .score
                    .map(|s| format!("（相关度：{:.2}）", s))
                    .unwrap_or_default();
                out.push_str(&format!(
                    "- [{}] {}: {}{}\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    snippet,
                    score_display,
                ));
            }
            out.push('\n');
        }

        let expanded_normal: Vec<_> = plan.expanded.iter().filter(|c| should_render_normal(c)).collect();
        if !expanded_normal.is_empty() {
            out.push_str("## Expanded Context (扩展上下文)\n");
            for c in &expanded_normal {
                let snippet = truncate_content(&c.content, 200);
                out.push_str(&format!(
                    "- [{}] {}: {}（via {}）\n",
                    capitalize_kind(&c.kind),
                    c.title,
                    snippet,
                    c.reason,
                ));
            }
            out.push('\n');
        }

        if !plan.recent.is_empty() {
            out.push_str("## Recent Activity (近期活动)\n");
            for e in &plan.recent {
                let date = e.updated_at.get(..10).unwrap_or(&e.updated_at);
                out.push_str(&format!(
                    "- [{}] {}: {}\n",
                    capitalize_kind(&e.kind),
                    date,
                    e.title,
                ));
            }
            out.push('\n');
        }

        out.push_str("</memory_context>");
        out
    }

    // ── L1 Boot ──────────────────────────────────────────────────────────

    fn layer_boot(
        &self,
        space_id: &str,
        is_group_chat: bool,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let details = self.store.list_boot_nodes(space_id, self.config.boot_limit)?;
        let mut candidates = Vec::new();

        for detail in details {
            let node = &detail.node;

            // In group chat, filter out Curated/UserProfile/Episode
            if is_group_chat {
                match node.kind {
                    MemoryNodeKind::Curated
                    | MemoryNodeKind::UserProfile
                    | MemoryNodeKind::Episode => continue,
                    _ => {}
                }
            }

            let content = match &detail.active_version {
                Some(v) => v.content.clone(),
                None => continue, // skip if no active version
            };

            seen.insert(node.id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node.id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "boot".to_string(),
                reason: "Explicit boot membership".to_string(),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: detail.keywords.clone(),
                metadata: node.metadata.clone(),
            });
        }

        // ── Auto-mount top-N learned skills regardless of query ─────
        // Without this, learned skills only surface when L2 (keyword) or
        // L3 (FTS) lights them up — and FTS5 unicode61 needs literal
        // word overlap. A power user's high-usage skill should always be
        // in context. boot_learned_skills_limit=0 disables this layer.
        if self.config.boot_learned_skills_limit > 0 {
            let learned = self
                .store
                .list_top_learned_skills(space_id, self.config.boot_learned_skills_limit)
                .unwrap_or_default();
            for detail in learned {
                let node = &detail.node;
                if seen.contains(&node.id) {
                    continue;
                }
                let content = match &detail.active_version {
                    Some(v) => v.content.clone(),
                    None => continue,
                };
                seen.insert(node.id.clone());
                candidates.push(MemoryRecallCandidate {
                    node_id: node.id.clone(),
                    title: node.title.clone(),
                    content,
                    kind: node.kind,
                    source: "boot".to_string(),
                    reason: "Top learned skill (auto-mount)".to_string(),
                    score: None,
                    fts_rank: None,
                    vector_rank: None,
                    matched_keywords: detail.keywords.clone(),
                    metadata: node.metadata.clone(),
                });
            }
        }

        Ok(candidates)
    }

    // ── L2 Triggered ─────────────────────────────────────────────────────

    fn layer_triggered(
        &self,
        space_id: &str,
        user_input: &str,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let tokens = tokenize_input(user_input);
        let token_refs: Vec<&str> = tokens.iter().map(|s| s.as_str()).collect();

        // Keyword match
        let keyword_nodes = self.store.keyword_search(space_id, &token_refs)?;

        // Trigger text match
        let trigger_edges = self.store.trigger_text_search(space_id, user_input)?;

        let mut candidates = Vec::new();
        let mut triggered_ids = HashSet::new();

        // Process keyword matches
        for node in keyword_nodes {
            if seen.contains(&node.id) || triggered_ids.contains(&node.id) {
                continue;
            }
            let content = match self.store.get_active_version(&node.id)? {
                Some(v) => v.content,
                None => continue,
            };
            let matched: Vec<String> = tokens
                .iter()
                .filter(|t| {
                    node.title.to_lowercase().contains(&t.to_lowercase())
                        || content.to_lowercase().contains(&t.to_lowercase())
                })
                .cloned()
                .collect();

            triggered_ids.insert(node.id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node.id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "trigger".to_string(),
                reason: format!("Keyword matched: [{}]", matched.join(", ")),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: matched,
                metadata: node.metadata.clone(),
            });
        }

        // Process trigger text matches
        for edge in trigger_edges {
            let node_id = &edge.child_node_id;
            if seen.contains(node_id) || triggered_ids.contains(node_id) {
                continue;
            }
            let node = match self.store.get_node(node_id)? {
                Some(n) => n,
                None => continue,
            };
            let content = match self.store.get_active_version(node_id)? {
                Some(v) => v.content,
                None => continue,
            };
            let trigger_text = edge.trigger_text.clone().unwrap_or_default();

            triggered_ids.insert(node_id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id: node_id.clone(),
                title: node.title.clone(),
                content,
                kind: node.kind,
                source: "trigger".to_string(),
                reason: format!("Disclosure text matched: [{}]", trigger_text),
                score: None,
                fts_rank: None,
                vector_rank: None,
                matched_keywords: vec![trigger_text],
                metadata: node.metadata.clone(),
            });
        }

        // Sort by priority (we don't have direct priority, sort by update time desc)
        candidates.truncate(self.config.trigger_limit);

        for c in &candidates {
            seen.insert(c.node_id.clone());
        }

        Ok(candidates)
    }

    // ── L3 Relevant (seed) ───────────────────────────────────────────────

    async fn layer_relevant(
        &self,
        space_id: &str,
        user_input: &str,
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        // FTS5 search
        let fts_results = self
            .store
            .fts_search(space_id, user_input, self.config.seed_limit * 2)
            .unwrap_or_default();

        // Vector search via memU (if available)
        let vector_results = if let Some(ref memu) = self.memu_client {
            let queries = vec![serde_json::json!({
                "role": "user",
                "content": user_input,
            })];
            match memu.retrieve(queries, None, None).await {
                Ok(result) => result.items,
                Err(e) => {
                    info!(error = %e, "recall: memU retrieve failed, falling back to FTS only");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        // Build rank maps
        let mut fts_rank_map: HashMap<String, (u32, &MemorySearchResult)> = HashMap::new();
        for (rank, r) in fts_results.iter().enumerate() {
            fts_rank_map.insert(r.node_id.clone(), (rank as u32 + 1, r));
        }

        let mut vector_rank_map: HashMap<String, u32> = HashMap::new();
        for (rank, item) in vector_results.iter().enumerate() {
            if let Some(id) = item.get("node_id").and_then(|v| v.as_str()) {
                vector_rank_map.insert(id.to_string(), rank as u32 + 1);
            }
        }

        // Collect all candidate node_ids
        let mut all_ids: Vec<String> = Vec::new();
        for id in fts_rank_map.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }
        for id in vector_rank_map.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }

        // Fuse scores
        let k = self.config.rrf_k;
        let mut scored: Vec<(String, f32, Option<u32>, Option<u32>)> = Vec::new();

        for id in &all_ids {
            if seen.contains(id) {
                continue;
            }
            let fts_r = fts_rank_map.get(id).map(|(r, _)| *r);
            let vec_r = vector_rank_map.get(id).copied();

            let score = match &self.config.fusion_strategy {
                FusionStrategy::Rrf => {
                    let mut s = 0.0f32;
                    if let Some(r) = fts_r {
                        s += 1.0 / (k as f32 + r as f32);
                    }
                    if let Some(r) = vec_r {
                        s += 1.0 / (k as f32 + r as f32);
                    }
                    s
                }
                FusionStrategy::Weighted => {
                    let fts_score = fts_r
                        .and_then(|_r| fts_rank_map.get(id).map(|(_, res)| res.score))
                        .unwrap_or(0.0);
                    let vec_score = vec_r.map(|r| 1.0 / r as f32).unwrap_or(0.0);
                    self.config.fts_weight * fts_score + self.config.vector_weight * vec_score
                }
            };

            scored.push((id.clone(), score, fts_r, vec_r));
        }

        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(self.config.seed_limit);

        let mut candidates = Vec::new();
        for (node_id, score, fts_r, vec_r) in scored {
            let (title, content, kind, node_metadata) = if let Some((_, fts_res)) = fts_rank_map.get(&node_id) {
                // Use FTS result data, but get full content from active version
                let full_content = self
                    .store
                    .get_active_version(&node_id)?
                    .map(|v| v.content)
                    .unwrap_or_else(|| fts_res.content_snippet.clone());
                let meta = self.store.get_node(&node_id)?.and_then(|n| n.metadata);
                (fts_res.title.clone(), full_content, fts_res.kind, meta)
            } else {
                // Vector-only result — fetch from store
                match self.store.get_node(&node_id)? {
                    Some(node) => {
                        let content = self
                            .store
                            .get_active_version(&node_id)?
                            .map(|v| v.content)
                            .unwrap_or_default();
                        let meta = node.metadata.clone();
                        (node.title, content, node.kind, meta)
                    }
                    None => continue,
                }
            };

            let fts_label = fts_r
                .map(|r| format!("fts #{}", r))
                .unwrap_or_else(|| "fts -".to_string());
            let vec_label = vec_r
                .map(|r| format!("vector #{}", r))
                .unwrap_or_else(|| "vector -".to_string());

            seen.insert(node_id.clone());
            candidates.push(MemoryRecallCandidate {
                node_id,
                title,
                content,
                kind,
                source: "search".to_string(),
                reason: format!("Hybrid recall hit ({}, {})", fts_label, vec_label),
                score: Some(score),
                fts_rank: fts_r,
                vector_rank: vec_r,
                matched_keywords: Vec::new(),
                metadata: node_metadata,
            });
        }

        Ok(candidates)
    }

    // ── L4 Expanded (graph walk) ─────────────────────────────────────────

    fn layer_expanded(
        &self,
        seeds: &[MemoryRecallCandidate],
        seen: &mut HashSet<String>,
    ) -> anyhow::Result<Vec<MemoryRecallCandidate>> {
        let mut candidates = Vec::new();
        let seed_count = seeds.len().min(3);

        for seed in seeds.iter().take(seed_count) {
            // 1. Primary route node
            if let Ok(Some(route)) = self.store.get_primary_route(&seed.node_id) {
                if route.node_id != seed.node_id && !seen.contains(&route.node_id) {
                    if let Some(c) =
                        self.make_expansion_candidate(&route.node_id, "graph_primary", &format!(
                            "primary route of '{}'",
                            seed.title
                        ))?
                    {
                        seen.insert(c.node_id.clone());
                        candidates.push(c);
                    }
                }
            }

            // 2. Parent nodes
            if let Ok(parents) = self.store.get_parent_nodes(&seed.node_id) {
                for parent in parents {
                    if seen.contains(&parent.id) {
                        continue;
                    }
                    if let Some(c) =
                        self.make_expansion_candidate(&parent.id, "graph_parent", &format!(
                            "parent of '{}'",
                            seed.title
                        ))?
                    {
                        seen.insert(c.node_id.clone());
                        candidates.push(c);
                    }
                }
            }

            // 3. High-priority child nodes (top 2)
            if let Ok(children) = self.store.get_child_nodes(&seed.node_id, 2) {
                for child in children {
                    if seen.contains(&child.id) {
                        continue;
                    }
                    if let Some(c) =
                        self.make_expansion_candidate(&child.id, "graph_child", &format!(
                            "child of '{}'",
                            seed.title
                        ))?
                    {
                        seen.insert(c.node_id.clone());
                        candidates.push(c);
                    }
                }
            }
        }

        // Sort by priority approximation (use updated_at as proxy) and truncate
        candidates.truncate(self.config.expansion_limit);
        Ok(candidates)
    }

    fn make_expansion_candidate(
        &self,
        node_id: &str,
        source: &str,
        reason: &str,
    ) -> anyhow::Result<Option<MemoryRecallCandidate>> {
        let node = match self.store.get_node(node_id)? {
            Some(n) => n,
            None => return Ok(None),
        };
        let content = match self.store.get_active_version(node_id)? {
            Some(v) => v.content,
            None => return Ok(None),
        };
        Ok(Some(MemoryRecallCandidate {
            node_id: node_id.to_string(),
            title: node.title.clone(),
            content,
            kind: node.kind,
            source: source.to_string(),
            reason: reason.to_string(),
            score: None,
            fts_rank: None,
            vector_rank: None,
            matched_keywords: Vec::new(),
            metadata: node.metadata.clone(),
        }))
    }

    // ── L5 Recent ────────────────────────────────────────────────────────

    fn layer_recent(
        &self,
        space_id: &str,
    ) -> anyhow::Result<Vec<MemoryTimelineEntry>> {
        let nodes = self
            .store
            .list_recent_nodes(space_id, self.config.recent_limit)?;

        let mut entries = Vec::new();
        for node in nodes {
            let snippet = self
                .store
                .get_active_version(&node.id)?
                .map(|v| truncate_content(&v.content, 120))
                .unwrap_or_default();

            entries.push(MemoryTimelineEntry {
                node_id: node.id,
                title: node.title,
                content_snippet: snippet,
                kind: node.kind,
                updated_at: node.updated_at,
            });
        }

        Ok(entries)
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────────

/// Simple tokenizer that handles both CJK and ASCII text.
/// - CJK characters are emitted individually.
/// - ASCII/Latin runs are split on whitespace and common punctuation.
fn tokenize_input(input: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut ascii_buf = String::new();

    for ch in input.chars() {
        if is_cjk(ch) {
            // Flush any pending ASCII buffer
            if !ascii_buf.is_empty() {
                flush_ascii(&mut ascii_buf, &mut tokens);
            }
            tokens.push(ch.to_string());
        } else if ch.is_alphanumeric() || ch == '_' || ch == '-' {
            ascii_buf.push(ch);
        } else {
            // Delimiter (space, punctuation, etc.)
            if !ascii_buf.is_empty() {
                flush_ascii(&mut ascii_buf, &mut tokens);
            }
        }
    }
    if !ascii_buf.is_empty() {
        flush_ascii(&mut ascii_buf, &mut tokens);
    }

    // Deduplicate while preserving order
    let mut seen = HashSet::new();
    tokens.retain(|t| {
        let lower = t.to_lowercase();
        if lower.len() < 2 && !is_cjk(lower.chars().next().unwrap_or(' ')) {
            return false; // skip single ASCII chars
        }
        seen.insert(lower)
    });

    tokens
}

fn flush_ascii(buf: &mut String, tokens: &mut Vec<String>) {
    let word = std::mem::take(buf);
    if !word.is_empty() {
        tokens.push(word);
    }
}

fn is_cjk(ch: char) -> bool {
    matches!(ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{F900}'..='\u{FAFF}' // CJK Compatibility Ideographs
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
    )
}

fn truncate_content(content: &str, max_len: usize) -> String {
    if content.chars().count() <= max_len {
        content.to_string()
    } else {
        let truncated: String = content.chars().take(max_len).collect();
        format!("{}...", truncated)
    }
}

fn capitalize_kind(kind: &MemoryNodeKind) -> &'static str {
    match kind {
        MemoryNodeKind::Boot => "Boot",
        MemoryNodeKind::Identity => "Identity",
        MemoryNodeKind::Value => "Value",
        MemoryNodeKind::UserProfile => "UserProfile",
        MemoryNodeKind::Directive => "Directive",
        MemoryNodeKind::Curated => "Curated",
        MemoryNodeKind::Episode => "Episode",
        MemoryNodeKind::Procedure => "Procedure",
        MemoryNodeKind::Reference => "Reference",
    }
}

/// Collect node IDs of every learned skill that `format_recall_for_prompt`
/// would render into the "Learned Skills" section. Must mirror the same
/// filter (Procedure + skill_type=='learned' + enabled).
///
/// Used by `MemoryRecallEngine::record_used_skills` to bump usage_count.
fn collect_emitted_skill_ids(plan: &MemoryRecallPlan) -> Vec<String> {
    let mut ids: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    let layers: Vec<&MemoryRecallCandidate> = plan
        .boot
        .iter()
        .chain(plan.triggered.iter())
        .chain(plan.relevant.iter())
        .chain(plan.expanded.iter())
        .collect();
    for c in layers {
        if c.kind != MemoryNodeKind::Procedure {
            continue;
        }
        let Some(ref meta) = c.metadata else { continue };
        let enabled = meta.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
        let skill_type = meta.get("skill_type").and_then(|v| v.as_str()).unwrap_or("");
        if !(enabled && skill_type == "learned") {
            continue;
        }
        if seen.insert(c.node_id.clone()) {
            ids.push(c.node_id.clone());
        }
    }
    ids
}

#[cfg(test)]
mod recall_helpers_tests {
    use super::*;

    fn skill_candidate(node_id: &str, enabled: bool, learned: bool) -> MemoryRecallCandidate {
        MemoryRecallCandidate {
            node_id: node_id.into(),
            title: "t".into(),
            content: "c".into(),
            kind: MemoryNodeKind::Procedure,
            source: "boot".into(),
            reason: "r".into(),
            score: None,
            fts_rank: None,
            vector_rank: None,
            matched_keywords: vec![],
            metadata: Some(serde_json::json!({
                "enabled": enabled,
                "skill_type": if learned { "learned" } else { "static" },
            })),
        }
    }

    fn empty_plan() -> MemoryRecallPlan {
        MemoryRecallPlan {
            boot: vec![],
            triggered: vec![],
            relevant: vec![],
            expanded: vec![],
            recent: vec![],
        }
    }

    #[test]
    fn collect_emitted_skips_disabled_and_non_learned() {
        let mut plan = empty_plan();
        plan.boot.push(skill_candidate("a", true, true));     // included
        plan.boot.push(skill_candidate("b", false, true));    // disabled, skip
        plan.boot.push(skill_candidate("c", true, false));    // static, skip
        plan.triggered.push(skill_candidate("a", true, true)); // dup of a, skip
        plan.relevant.push(skill_candidate("d", true, true)); // included
        let ids = collect_emitted_skill_ids(&plan);
        assert_eq!(ids, vec!["a".to_string(), "d".to_string()]);
    }

    #[test]
    fn collect_emitted_returns_empty_when_no_learned() {
        let mut plan = empty_plan();
        plan.boot.push(skill_candidate("a", true, false));
        let ids = collect_emitted_skill_ids(&plan);
        assert!(ids.is_empty());
    }
}
