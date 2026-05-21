//! skill_search tool — agent invokes to find learned/builtin skills
//! matching a query. Returns a list of {name, summary, relevance, quality,
//! final_score, match_reasons, warnings, provenance, cited_count, node_id,
//! matched_signals} structs. Does NOT load full content (see load_skill for that).
//!
//! Side effects: emits `agent:skill-recalled` event for UI; bumps
//! usage_count on each learned-skill hit via existing bump_skill_usage.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §3.

use std::sync::Arc;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::memory_graph::models::MemoryNode;
use crate::memory_graph::store::MemoryGraphStore;
use crate::memu::client::MemUClient;
use crate::skills::SkillsRegistry;

pub struct SkillSearchTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub store: Arc<MemoryGraphStore>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
    pub space_id: String,
    pub memu_client: Option<Arc<MemUClient>>,
}

impl<R: tauri::Runtime> SkillSearchTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        store: Arc<MemoryGraphStore>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, store, app_handle, conversation_id, space_id, memu_client: None }
    }

    /// Attach a `MemUClient` to enable the cosine-similarity channel during search.
    /// If not set (or if fastembed is unavailable at runtime), the cosine channel is
    /// silently skipped and keyword-only results are returned.
    pub fn with_memu(mut self, client: Option<Arc<MemUClient>>) -> Self {
        self.memu_client = client;
        self
    }

    /// Scan all active learned-skill versions and compute cosine similarity against
    /// `query_embedding`. Returns a map of node_id → cosine score (only entries with
    /// sim > 0.0) and a vec of (node_id, MemoryNode) for skills that were NOT already
    /// in `existing_node_ids` — so callers can inject them into the keyword channel's
    /// node_score map.
    ///
    /// Extracted so tests can drive it with pre-computed fake embeddings without needing
    /// a live fastembed bridge.
    pub fn apply_cosine_scoring(
        &self,
        query_embedding: &[f32],
        existing_node_ids: &std::collections::HashSet<String>,
    ) -> (
        std::collections::HashMap<String, f32>,  // node_id → cosine boost
        Vec<(String, MemoryNode)>,               // new nodes not in existing_node_ids
    ) {
        let mut cosine_boost: std::collections::HashMap<String, f32> =
            std::collections::HashMap::new();
        let mut new_nodes: Vec<(String, MemoryNode)> = Vec::new();

        if let Ok(all_skills) = self.store.list_top_learned_skills(&self.space_id, 500) {
            for detail in &all_skills {
                if let Some(ref active_ver) = detail.active_version {
                    if let Some(stored_emb) = crate::memu::embedding::parse_embedding(
                        active_ver.embedding_json.as_deref()
                    ) {
                        let sim = crate::memu::embedding::cosine_sim(query_embedding, &stored_emb);
                        if sim > 0.0 {
                            cosine_boost.insert(detail.node.id.clone(), sim);
                            if !existing_node_ids.contains(&detail.node.id) {
                                new_nodes.push((detail.node.id.clone(), detail.node.clone()));
                            }
                        }
                    }
                }
            }
        }

        (cosine_boost, new_nodes)
    }
}

#[derive(Debug, Serialize)]
struct SearchHit {
    name: String,
    summary: String,
    /// Strength of evidence FROM the query (keyword hits, signal matches, cosine similarity).
    /// Range: 0.0 – higher means stronger match.
    relevance: f64,
    /// How well-validated this skill is, independent of the query.
    /// Derived from cited_count + usage_count.
    quality: f64,
    /// Used for final sorting + truncation. Today: relevance + quality.
    final_score: f64,
    /// Human-readable bullets explaining WHY this hit fired.
    /// LLM uses these to audit recall; UI may display them as chip subtitles.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    match_reasons: Vec<String>,
    /// Skill-side warnings — pulled from metadata (e.g. validation_hint).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    provenance: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cited_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_id: Option<String>,
    /// Which signals (if any) matched this query — surfaced to the LLM
    /// so it can explain why this skill was recalled.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    matched_signals: Vec<String>,
}

#[async_trait]
impl<R: tauri::Runtime> Tool for SkillSearchTool<R> {
    fn name(&self) -> &str {
        "skill_search"
    }

    fn description(&self) -> &str {
        "Search learned skills by keywords. Returns top-N matches; load full content via load_skill if a match looks promising. Set lite=true when enumerating many skills (>5) to save tokens."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords describing the task. English works better than Chinese."
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of skills to return (default 5, max 20).",
                    "minimum": 1,
                    "maximum": 20,
                    "default": 5
                },
                "category": {
                    "type": "string",
                    "enum": ["repair", "optimize", "innovate"],
                    "description": "Restrict to skills tagged with this category."
                },
                "lite": {
                    "type": "boolean",
                    "description": "When true returns only {name, provenance, summary} per hit (saves ~70% tokens). Set this when user asks 'list/enumerate all my skills'; leave false when investigating a specific problem.",
                    "default": false
                }
            },
            "required": ["query"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let query = params["query"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("query is required".into()))?
            .trim();
        if query.is_empty() {
            return Ok(ToolOutput::new(json!([]), start.elapsed().as_millis() as u64));
        }
        let top_k = params["top_k"].as_u64().unwrap_or(5).clamp(1, 20) as usize;

        // Optional category filter — only applied to learned skills (builtins have no category metadata).
        let category_filter: Option<&str> = params["category"].as_str();

        let mut hits: Vec<SearchHit> = Vec::new();

        // Builtin pass — registry.match_skills returns scored skills already.
        // Builtins carry no quality signal (no citation/usage history), so quality=0.0.
        let registry = self.registry.read().await;
        for skill in registry.match_skills(query) {
            let relevance = crate::skills::score_skill(skill, query) as f64;
            hits.push(SearchHit {
                name: skill.manifest.name.clone(),
                summary: truncate_summary(&skill.manifest.description, 200),
                relevance,
                quality: 0.0,
                final_score: relevance,
                match_reasons: vec![],
                warnings: vec![],
                provenance: "builtin",
                cited_count: None,
                node_id: None,
                matched_signals: vec![],
            });
        }
        drop(registry);

        // Learned pass — tokenize query, search keywords, score by hit count + priors.
        // node_score holds the full node so we avoid a redundant get_node round-trip.
        let tokens: Vec<&str> = query
            .split_whitespace()
            .filter(|t| t.len() >= 2)
            .collect();
        let mut node_score: std::collections::HashMap<String, (i64, MemoryNode)> =
            std::collections::HashMap::new();
        for tok in &tokens {
            if let Ok(nodes) = self.store.search_by_keyword(&self.space_id, tok) {
                for node in nodes {
                    // Filter to learned procedures only
                    let is_learned = node
                        .metadata
                        .as_ref()
                        .and_then(|m| m.get("skill_type"))
                        .and_then(|v| v.as_str())
                        == Some("learned");
                    if !is_learned {
                        continue;
                    }
                    // Apply category filter if requested
                    if let Some(cat) = category_filter {
                        let node_cat = node
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("category"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if node_cat != cat {
                            continue;
                        }
                    }
                    let entry = node_score.entry(node.id.clone()).or_insert((0, node));
                    entry.0 += 1;
                }
            }
        }

        // Cosine channel — embed the query and compare against stored embeddings.
        // Adds up to +2.0 to score (cosine_sim * 2.0). Gracefully skipped if:
        //   - memu_client is None (fastembed unavailable)
        //   - embed call fails
        //   - stored versions have NULL embedding_json (use 0.0 boost)
        //
        // We also add any cosine-hit nodes that the keyword channel missed.
        let query_embedding: Option<Vec<f32>> = if let Some(ref memu) = self.memu_client {
            match memu.embed_text(&[query]).await {
                Ok(mut vecs) if !vecs.is_empty() => Some(vecs.remove(0)),
                Ok(_) => None,
                Err(e) => {
                    tracing::debug!(error = %e, "skill_search: embed_text failed, skipping cosine channel");
                    None
                }
            }
        } else {
            None
        };

        // Map node_id → cosine boost so we can apply it uniformly.
        let cosine_boost: std::collections::HashMap<String, f32> =
            if let Some(ref q_emb) = query_embedding {
                let existing_ids: std::collections::HashSet<String> =
                    node_score.keys().cloned().collect();
                let (boost_map, new_nodes) = self.apply_cosine_scoring(q_emb, &existing_ids);
                // Inject cosine-only hits so they participate in score building below.
                // Apply category filter to cosine-only hits as well.
                for (nid, node) in new_nodes {
                    if let Some(cat) = category_filter {
                        let node_cat = node
                            .metadata
                            .as_ref()
                            .and_then(|m| m.get("category"))
                            .and_then(|v| v.as_str())
                            .unwrap_or("");
                        if node_cat != cat {
                            continue;
                        }
                    }
                    node_score.entry(nid).or_insert((0, node));
                }
                boost_map
            } else {
                std::collections::HashMap::new()
            };

        let semantic_unavailable = self.memu_client.is_none();

        // Build learned hits — node already in hand, no extra DB call needed.
        let query_lower = query.to_lowercase();
        for (node_id, (kw_hits, node)) in node_score {
            let cited = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("cited_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let usage = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("usage_count"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let summary = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("summary"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .unwrap_or_else(|| node.title.clone());

            // Signal scoring: +1.5 per signal phrase that appears as a
            // substring in the lowercased query.
            let signals: Vec<String> = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("signals"))
                .and_then(|v| v.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(|s| s.to_lowercase()))
                        .collect()
                })
                .unwrap_or_default();
            let matched_signals: Vec<String> = signals
                .iter()
                .filter(|sig| query_lower.contains(sig.as_str()))
                .cloned()
                .collect();
            let signal_match_count = matched_signals.len() as f64;

            let cosine_sim_val = cosine_boost.get(&node_id).copied().unwrap_or(0.0);
            let cosine_bonus = cosine_sim_val as f64 * 2.0;

            // Tri-tier scoring
            let relevance = (kw_hits as f64) + signal_match_count * 1.5 + cosine_bonus;
            let quality = (cited as f64 * 0.5) + (usage as f64 * 0.2);
            let final_score = relevance + quality;

            // Build human-readable match_reasons
            let mut match_reasons: Vec<String> = Vec::new();
            if kw_hits > 0 {
                match_reasons.push(format!("关键词命中 {} 个", kw_hits));
            }
            if signal_match_count > 0.0 {
                match_reasons.push(format!(
                    "信号匹配 {} 个: {}",
                    signal_match_count as usize,
                    matched_signals.join("、")
                ));
            }
            if cosine_sim_val > 0.01 {
                match_reasons.push(format!("语义相似度 {:.2}", cosine_sim_val));
            }
            if cited > 0 {
                match_reasons.push(format!("曾被引用 {} 次", cited));
            }

            // Build warnings from metadata
            let mut warnings: Vec<String> = Vec::new();

            // Lifecycle flag — PR #117 introduced draft / promoted / deprecated.
            // skill_search returns hits at every stage (drafts are still
            // useful for the agent to discover) but tags non-promoted ones
            // so the LLM knows to weigh them differently. Promoted skills
            // (and pre-PR rows missing the field) get no flag.
            if let Some(lc) = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("lifecycle"))
                .and_then(|v| v.as_str())
            {
                match lc {
                    "draft" => warnings.push(
                        "draft 阶段（未经使用验证；引用 3 次后自动升级为 promoted）".to_string(),
                    ),
                    "deprecated" => warnings.push(
                        "deprecated 阶段（已手动退役，仅作历史参考）".to_string(),
                    ),
                    _ => {}
                }
            }

            if let Some(hint) = node
                .metadata
                .as_ref()
                .and_then(|m| m.get("validation_hint"))
                .and_then(|v| v.as_str())
            {
                warnings.push(format!("应用后建议验证: {}", hint));
            }
            // Surface semantic-channel unavailability only when this hit had keyword/signal
            // matches but might have ranked higher with cosine boosting.
            if semantic_unavailable && (kw_hits > 0 || signal_match_count > 0.0) {
                warnings.push("需启用 fastembed 才能语义检索".to_string());
            }

            hits.push(SearchHit {
                name: node.title,
                summary: truncate_summary(&summary, 200),
                relevance,
                quality,
                final_score,
                match_reasons,
                warnings,
                provenance: "learned",
                cited_count: Some(cited),
                node_id: Some(node_id),
                matched_signals,
            });
        }

        // Sort by final_score desc and trim FIRST so we only bump skills the LLM actually sees.
        hits.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);

        // Bump usage_count for learned hits that survived truncation (fire-and-forget; soft signal).
        let bump_ids: Vec<&str> = hits
            .iter()
            .filter_map(|h| if h.provenance == "learned" { h.node_id.as_deref() } else { None })
            .collect();
        if !bump_ids.is_empty() {
            if let Err(e) = self.store.bump_skill_usage(&bump_ids) {
                tracing::warn!("bump_skill_usage failed: {}", e);
            }
        }

        // Bundle 26-A — Skill-use telemetry: bump returned_count on each
        // disk-resident skill that came back as a hit. Separate from
        // bump_skill_usage (which updates the memory_graph cited_count
        // ONLY for learned-tier skills) — meta.json sits next to
        // SKILL.md for ALL tiers (auto_extracted / user / project /
        // marketplace / bundled) so the future SkillDistillationScenario
        // (Bundle 26-B) has uniform data to reason about.
        {
            let registry = self.registry.read().await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            for hit in &hits {
                let Some(loaded) = registry.get_loaded(&hit.name) else {
                    continue;
                };
                let skill_dir = &loaded.manifest.path;
                // Slug: filesystem dirname — for _auto_extracted/ this
                // matches skill_parser.slugify_for_filesystem; for other
                // tiers it's whatever the disk layout has. Falls back to
                // skill name if the path has no terminal component (very
                // unusual but defensive).
                let slug = skill_dir
                    .file_name()
                    .and_then(|s| s.to_str())
                    .unwrap_or(hit.name.as_str())
                    .to_string();
                if let Err(e) = crate::proactive::skill_telemetry::record_returned(
                    skill_dir, &slug, now_ms,
                ) {
                    tracing::warn!(
                        skill = %hit.name,
                        skill_dir = %skill_dir.display(),
                        error = %e,
                        "[skill_telemetry] record_returned failed"
                    );
                }
            }
        }

        // Emit agent:skill-recalled event — always with the FULL hit shape
        // so the UI can still render rich chips even when the LLM saw lite.
        let tool_call_id = params["_tool_call_id"]
            .as_str()
            .unwrap_or("")
            .to_string();
        let _ = self.app_handle.emit("agent:skill-recalled", json!({
            "conversationId": self.conversation_id,
            "toolCallId": tool_call_id,
            "kind": "search",
            "query": query,
            "results": &hits,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));

        // Lite mode: slim each hit to {name, provenance, summary} before
        // serializing back to the LLM. The full SearchHit shape (with
        // relevance/quality/match_reasons/warnings/cited_count/node_id/
        // matched_signals) is ~100-150 tokens per row; the lite shape is
        // ~25-40 tokens — for a top_k=20 enumeration query, that's
        // ~1500-2000 tokens saved.
        //
        // Summaries also get truncated harder (200 → 100 chars) under
        // lite, because the use case is "give me the catalog" not "help
        // me decide which one to load".
        let lite = params["lite"].as_bool().unwrap_or(false);
        let body = if lite {
            let slim: Vec<serde_json::Value> = hits
                .iter()
                .map(|h| {
                    let summary = truncate_summary(&h.summary, 100);
                    json!({
                        "name": h.name,
                        "provenance": h.provenance,
                        "summary": summary,
                    })
                })
                .collect();
            serde_json::to_value(&slim).unwrap_or(json!([]))
        } else {
            serde_json::to_value(&hits).unwrap_or(json!([]))
        };
        Ok(ToolOutput::new(body, start.elapsed().as_millis() as u64))
    }
}

fn truncate_summary(s: &str, max_chars: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max_chars).collect();
    if let Some(last_space) = out.rfind(char::is_whitespace) {
        out.truncate(last_space);
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryKeyword};
    use chrono::Utc;
    use serde_json::json;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned_node_with_keywords(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        cited: u64,
    ) -> String {
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
                "summary": format!("Summary for {}", title),
                "cited_count": cited,
                "usage_count": 0,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    #[tokio::test]
    async fn empty_query_returns_empty_array() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();

        let tool = SkillSearchTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }

    #[tokio::test]
    async fn learned_keyword_hit_returns_skill() {
        let store = fresh_store();
        let _id = make_learned_node_with_keywords(&store, "stock-research", &["stock", "financials"], 5);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "stock financials" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0]["name"], "stock-research");
        assert_eq!(hits[0]["provenance"], "learned");
        assert_eq!(hits[0]["cited_count"], 5);
    }

    #[tokio::test]
    async fn bump_skill_usage_called_on_hit() {
        let store = fresh_store();
        let id = make_learned_node_with_keywords(&store, "stock-research", &["stock"], 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let _ = tool.execute(json!({ "query": "stock" })).await.unwrap();

        let node = store.get_node(&id).unwrap().unwrap();
        let usage = node.metadata.unwrap()
            .get("usage_count").unwrap().as_u64().unwrap();
        assert_eq!(usage, 1);
    }

    #[tokio::test]
    async fn no_matches_returns_empty_array() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "nonexistent_xyz_query" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }

    /// Helper: create a learned node with explicit metadata (including signals).
    fn make_learned_node_with_signals(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        signals: &[&str],
    ) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let signals_value: serde_json::Value = signals
            .iter()
            .map(|s| serde_json::Value::String(s.to_string()))
            .collect::<Vec<_>>()
            .into();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": 0u64,
                "usage_count": 0u64,
                "signals": signals_value,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    #[tokio::test]
    async fn signal_match_boosts_score_over_keyword_only() {
        let store = fresh_store();
        // Skill A: keyword "api" only, no signals
        let _id_a = make_learned_node_with_keywords(&store, "skill-a", &["api"], 0);
        // Skill B: keyword "api" + signal "401 unauthorized"
        let _id_b = make_learned_node_with_signals(&store, "skill-b", &["api"], &["401 unauthorized"]);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        // Query includes both keyword token AND the signal phrase
        let out = tool.execute(json!({ "query": "api 401 unauthorized" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert!(!hits.is_empty(), "expected at least one hit");

        let pos_a = hits.iter().position(|h| h["name"] == "skill-a");
        let pos_b = hits.iter().position(|h| h["name"] == "skill-b");

        assert!(pos_a.is_some(), "skill-a should appear in results");
        assert!(pos_b.is_some(), "skill-b should appear in results");
        assert!(
            pos_b.unwrap() < pos_a.unwrap(),
            "expected skill-b (signal+keyword match) before skill-a (keyword only); hits: {:#?}",
            hits
        );

        // Also verify matched_signals is populated for skill-b
        let b_hit = &hits[pos_b.unwrap()];
        let matched = b_hit["matched_signals"].as_array().unwrap();
        assert_eq!(matched.len(), 1);
        assert_eq!(matched[0], "401 unauthorized");
    }

    /// Helper: create a learned node with an explicit embedding_json stored in the
    /// active version. Used to test the cosine channel without a live fastembed process.
    fn make_learned_node_with_embedding(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        embedding: &[f32],
    ) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": 0u64,
                "usage_count": 0u64,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        let embedding_json = serde_json::to_string(embedding).unwrap();
        let version = crate::memory_graph::models::MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id.clone(),
            supersedes_version_id: None,
            status: crate::memory_graph::models::MemoryVersionStatus::Active,
            content: format!("Content for {}", title),
            metadata: None,
            embedding_json: Some(embedding_json),
            created_at: now.clone(),
        };
        store.create_version(&version).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    /// Verify graceful degradation: with NULL embeddings and no memu client,
    /// keyword-only results are returned normally (cosine channel is silently skipped).
    #[tokio::test]
    async fn cosine_degrades_gracefully_without_memu() {
        let store = fresh_store();
        // Skill with no embedding stored
        let _id = make_learned_node_with_keywords(&store, "null-embedding-skill", &["database"], 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        // No memu client — cosine channel should be skipped
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        ); // no .with_memu() — memu_client stays None

        let out = tool.execute(json!({ "query": "database" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1, "keyword channel should still find the skill");
        assert_eq!(hits[0]["name"], "null-embedding-skill");
    }

    /// Verify the cosine channel does NOT surface a semantic-only skill via keyword search.
    /// (Keyword-only path: the tool without memu must not find the semantic skill.)
    #[tokio::test]
    async fn cosine_skill_not_surfaced_by_keyword_search() {
        let store = fresh_store();

        // Unit vector along dim 0 — represents the "query concept"
        let q_vec: Vec<f32> = {
            let mut v = vec![0.0f32; 8];
            v[0] = 1.0;
            v
        };
        // Matching embedding (cosine sim = 1.0 with q_vec)
        let matching_emb = q_vec.clone();
        // Orthogonal embedding (cosine sim = 0.0 with q_vec)
        let unrelated_emb: Vec<f32> = {
            let mut v = vec![0.0f32; 8];
            v[1] = 1.0;
            v
        };

        // Skill A: no overlapping keyword, but stored embedding matches q_vec perfectly
        let _id_a = make_learned_node_with_embedding(&store, "semantic-skill", &["zzzunique"], &matching_emb);
        // Skill B: keyword match for "database", orthogonal embedding
        let _id_b = make_learned_node_with_embedding(&store, "keyword-skill", &["database"], &unrelated_emb);

        // Verify cosine_sim math directly (no bridge needed for stored embeddings)
        let stored_a_sim = crate::memu::embedding::cosine_sim(&q_vec, &matching_emb);
        let stored_b_sim = crate::memu::embedding::cosine_sim(&q_vec, &unrelated_emb);
        assert!((stored_a_sim - 1.0).abs() < 1e-5, "matching embedding should have sim ~1.0");
        assert!(stored_b_sim.abs() < 1e-5, "orthogonal embedding should have sim ~0.0");

        // Keyword channel alone: "database" hits keyword-skill but NOT semantic-skill
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool_no_memu = SkillSearchTool::new(
            Arc::clone(&registry),
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool_no_memu.execute(json!({ "query": "database", "top_k": 10 })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert!(
            hits.iter().all(|h| h["name"] != "semantic-skill"),
            "keyword-only search must NOT surface semantic-skill (no keyword overlap)"
        );
    }

    /// Positive cosine path: `apply_cosine_scoring` surfaces the semantically matching
    /// skill and ranks it above the orthogonal one.
    ///
    /// Uses two orthogonal unit vectors so we can predict cosine similarity exactly,
    /// no live fastembed bridge required.
    #[test]
    fn apply_cosine_scoring_surfaces_matching_skill() {
        let store = fresh_store();

        // dim-0 unit vector → matches q_vec perfectly (cosine = 1.0)
        let matching_emb: Vec<f32> = {
            let mut v = vec![0.0f32; 4];
            v[0] = 1.0;
            v
        };
        // dim-1 unit vector → orthogonal to q_vec (cosine = 0.0)
        let orthogonal_emb: Vec<f32> = {
            let mut v = vec![0.0f32; 4];
            v[1] = 1.0;
            v
        };

        let id_match = make_learned_node_with_embedding(
            &store, "semantic-match", &["zzz_unique_kw"], &matching_emb,
        );
        let id_ortho = make_learned_node_with_embedding(
            &store, "orthogonal-skill", &["other_kw"], &orthogonal_emb,
        );

        // Build the tool (no memu needed — we call apply_cosine_scoring directly)
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        // Query embedding aligned with dim-0 (matches semantic-match, not orthogonal-skill)
        let q_emb: Vec<f32> = {
            let mut v = vec![0.0f32; 4];
            v[0] = 1.0;
            v
        };

        let existing = std::collections::HashSet::new();
        let (boost_map, new_nodes) = tool.apply_cosine_scoring(&q_emb, &existing);

        // semantic-match should have boost ~1.0; orthogonal-skill should NOT appear
        let match_boost = boost_map.get(&id_match).copied().unwrap_or(0.0);
        let ortho_boost = boost_map.get(&id_ortho).copied().unwrap_or(0.0);

        assert!(
            (match_boost - 1.0).abs() < 1e-5,
            "semantic-match should have cosine boost ~1.0, got {}",
            match_boost
        );
        assert!(
            ortho_boost.abs() < 1e-5,
            "orthogonal-skill should have cosine boost ~0.0, got {}",
            ortho_boost
        );

        // Both nodes were not in existing_node_ids, so both should appear in new_nodes
        let new_ids: Vec<&str> = new_nodes.iter().map(|(id, _)| id.as_str()).collect();
        assert!(
            new_ids.contains(&id_match.as_str()),
            "semantic-match should be in new_nodes"
        );
        // orthogonal-skill should NOT be in new_nodes (boost was 0.0, filtered out)
        assert!(
            !new_ids.contains(&id_ortho.as_str()),
            "orthogonal-skill must not be in new_nodes (zero cosine)"
        );
    }

    // ─── New tests for tri-tier scoring, warnings, category filter, top_k ───

    /// Helper: create a learned node with category metadata.
    fn make_learned_node_with_category(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        category: &str,
        cited: u64,
    ) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": cited,
                "usage_count": 0u64,
                "category": category,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    /// Helper: create a learned node with a validation_hint in metadata.
    fn make_learned_node_with_validation_hint(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        validation_hint: &str,
    ) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": 0u64,
                "usage_count": 0u64,
                "validation_hint": validation_hint,
            })),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    /// final_score = relevance + quality, both > 0 when there's a keyword hit and citations.
    #[tokio::test]
    async fn final_score_split_into_relevance_and_quality() {
        let store = fresh_store();
        // cited=10 → quality = 10 * 0.5 = 5.0
        let _id = make_learned_node_with_keywords(&store, "cited-skill", &["budget", "finance"], 10);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "budget finance" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];

        let relevance = hit["relevance"].as_f64().unwrap();
        let quality = hit["quality"].as_f64().unwrap();
        let final_score = hit["final_score"].as_f64().unwrap();

        assert!(relevance > 0.0, "relevance should be positive for keyword hits; got {}", relevance);
        assert!(quality > 0.0, "quality should be positive for cited skill; got {}", quality);
        assert!(
            (final_score - (relevance + quality)).abs() < 1e-10,
            "final_score must equal relevance + quality; got final={} relevance={} quality={}",
            final_score, relevance, quality
        );
    }

    /// match_reasons should contain a "关键词命中" entry for keyword hits.
    #[tokio::test]
    async fn match_reasons_populated_for_keyword_hit() {
        let store = fresh_store();
        let _id = make_learned_node_with_keywords(&store, "kw-skill", &["refactor"], 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "refactor" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);

        let reasons = hits[0]["match_reasons"].as_array().unwrap();
        assert!(
            reasons.iter().any(|r| r.as_str().unwrap_or("").contains("关键词命中")),
            "match_reasons should contain '关键词命中'; got: {:?}", reasons
        );
    }

    /// warnings should include the validation_hint text when present in metadata.
    #[tokio::test]
    async fn warnings_includes_validation_hint() {
        let store = fresh_store();
        let _id = make_learned_node_with_validation_hint(
            &store,
            "hint-skill",
            &["deploy"],
            "Run tests after",
        );

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "deploy" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);

        let warnings = hits[0]["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w.as_str().unwrap_or("").contains("Run tests after")),
            "warnings should contain the validation_hint text; got: {:?}", warnings
        );
    }

    /// category=repair filter should exclude skills tagged innovate.
    #[tokio::test]
    async fn category_filter_excludes_other_categories() {
        let store = fresh_store();
        let _repair_id = make_learned_node_with_category(&store, "repair-skill", &["fix", "bug"], "repair", 0);
        let _innovate_id = make_learned_node_with_category(&store, "innovate-skill", &["fix", "feature"], "innovate", 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "query": "fix bug", "category": "repair" })).await.unwrap();
        let hits = out.result.as_array().unwrap();

        assert!(
            hits.iter().all(|h| h["name"] != "innovate-skill"),
            "innovate-skill must be excluded when category=repair; hits: {:?}", hits
        );
        assert!(
            hits.iter().any(|h| h["name"] == "repair-skill"),
            "repair-skill should be included; hits: {:?}", hits
        );
    }

    /// No category filter → both skills returned.
    #[tokio::test]
    async fn category_filter_omitted_returns_all() {
        let store = fresh_store();
        let _repair_id = make_learned_node_with_category(&store, "repair-skill", &["fix", "bug"], "repair", 0);
        let _innovate_id = make_learned_node_with_category(&store, "innovate-skill", &["fix", "feature"], "innovate", 0);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        // No category param — both skills share keyword "fix"
        let out = tool.execute(json!({ "query": "fix", "top_k": 20 })).await.unwrap();
        let hits = out.result.as_array().unwrap();

        assert!(
            hits.iter().any(|h| h["name"] == "repair-skill"),
            "repair-skill should appear without category filter; hits: {:?}", hits
        );
        assert!(
            hits.iter().any(|h| h["name"] == "innovate-skill"),
            "innovate-skill should appear without category filter; hits: {:?}", hits
        );
    }

    /// parameters_schema should reflect default=5, max=20, and the category enum.
    #[test]
    fn top_k_default_is_5_max_is_20() {
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let schema = tool.parameters_schema();
        let top_k_prop = &schema["properties"]["top_k"];
        assert_eq!(top_k_prop["default"], 5, "top_k default should be 5");
        assert_eq!(top_k_prop["maximum"], 20, "top_k maximum should be 20");
        assert_eq!(top_k_prop["minimum"], 1, "top_k minimum should be 1");

        // Verify category enum is present
        let category_prop = &schema["properties"]["category"];
        let enum_vals = category_prop["enum"].as_array().unwrap();
        assert!(enum_vals.iter().any(|v| v == "repair"), "enum should include repair");
        assert!(enum_vals.iter().any(|v| v == "optimize"), "enum should include optimize");
        assert!(enum_vals.iter().any(|v| v == "innovate"), "enum should include innovate");
    }

    /// Insert a learned node with an explicit `lifecycle` metadata field.
    /// Used by the lifecycle-warning tests below — keeps them independent of
    /// `make_learned_node_with_validation_hint`, which also bumps warnings.
    fn make_learned_node_with_lifecycle(
        store: &MemoryGraphStore,
        title: &str,
        keywords: &[&str],
        lifecycle: Option<&str>,
    ) -> String {
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        let mut meta = json!({
            "skill_type": "learned",
            "enabled": true,
            "summary": format!("Summary for {}", title),
            "cited_count": 0u64,
            "usage_count": 0u64,
        });
        if let Some(lc) = lifecycle {
            meta.as_object_mut().unwrap().insert(
                "lifecycle".into(),
                serde_json::Value::String(lc.to_string()),
            );
        }
        let node = MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: crate::memory_graph::models::MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(meta),
            created_at: now.clone(),
            updated_at: now,
        };
        store.create_node(&node).unwrap();
        for kw in keywords {
            store.create_keyword(&MemoryKeyword {
                id: uuid::Uuid::new_v4().to_string(),
                space_id: "default".into(),
                node_id: id.clone(),
                keyword: kw.to_string(),
                created_at: chrono::Utc::now().to_rfc3339(),
            }).unwrap();
        }
        id
    }

    /// draft skills must surface a lifecycle warning so the LLM knows the
    /// skill is unvalidated. PR-mattpocock-3 spec §Layer B-2 deferred this
    /// to a follow-up — this is that follow-up.
    #[tokio::test]
    async fn warnings_flag_draft_skill() {
        let store = fresh_store();
        let _id = make_learned_node_with_lifecycle(&store, "draft-skill", &["alpha"], Some("draft"));

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry, Arc::clone(&store), app.handle().clone(),
            "test-session".into(), "default".into(),
        );

        let out = tool.execute(json!({ "query": "alpha" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let warnings = hits[0]["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w.as_str().unwrap_or("").contains("draft")),
            "draft hit should be flagged in warnings; got: {:?}", warnings
        );
    }

    /// deprecated skills must surface a lifecycle warning so the LLM
    /// down-weights them. Hits are still returned (the agent might be
    /// looking for historical context) but with an explicit flag.
    #[tokio::test]
    async fn warnings_flag_deprecated_skill() {
        let store = fresh_store();
        let _id = make_learned_node_with_lifecycle(&store, "old-skill", &["beta"], Some("deprecated"));

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry, Arc::clone(&store), app.handle().clone(),
            "test-session".into(), "default".into(),
        );

        let out = tool.execute(json!({ "query": "beta" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let warnings = hits[0]["warnings"].as_array().unwrap();
        assert!(
            warnings.iter().any(|w| w.as_str().unwrap_or("").contains("deprecated")),
            "deprecated hit should be flagged in warnings; got: {:?}", warnings
        );
    }

    /// promoted skills and pre-PR rows missing the `lifecycle` field must
    /// NOT have a lifecycle warning — only validation_hint / fastembed
    /// warnings are eligible. This is the grandfathering invariant.
    #[tokio::test]
    async fn warnings_omit_lifecycle_flag_for_promoted_and_missing() {
        let store = fresh_store();
        let _p = make_learned_node_with_lifecycle(&store, "promoted-skill", &["gamma"], Some("promoted"));
        let _l = make_learned_node_with_lifecycle(&store, "legacy-skill",  &["gamma"], None);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry, Arc::clone(&store), app.handle().clone(),
            "test-session".into(), "default".into(),
        );

        let out = tool.execute(json!({ "query": "gamma" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 2);
        for hit in hits {
            let warnings = hit["warnings"].as_array().unwrap();
            assert!(
                !warnings.iter().any(|w| {
                    let s = w.as_str().unwrap_or("");
                    s.contains("draft") || s.contains("deprecated")
                }),
                "promoted/legacy hit must not carry lifecycle warning; got: {:?} for {:?}",
                warnings, hit["name"],
            );
        }
    }

    /// Lite mode (PR 2026-05-13 token-cost optim): when `lite: true`, the
    /// tool output drops to {name, provenance, summary} per hit — no
    /// relevance / quality / final_score / match_reasons / warnings /
    /// cited_count / node_id / matched_signals. For top_k=20 enumeration
    /// queries this saves ~1500-2000 tokens. The summary cap also tightens
    /// from 200 → 100 chars.
    #[tokio::test]
    async fn lite_mode_returns_slim_hits() {
        let store = fresh_store();
        let _id = make_learned_node_with_keywords(&store, "lite-skill", &["target"], 7);

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry, Arc::clone(&store), app.handle().clone(),
            "test-session".into(), "default".into(),
        );

        let out = tool.execute(json!({ "query": "target", "lite": true })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        let obj = hit.as_object().unwrap();
        // Must keep: name, provenance, summary.
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("provenance"));
        assert!(obj.contains_key("summary"));
        // Must drop: everything else.
        for absent in [
            "relevance", "quality", "final_score",
            "match_reasons", "warnings",
            "cited_count", "node_id", "matched_signals",
        ] {
            assert!(
                !obj.contains_key(absent),
                "lite hit must not include `{}`; got keys: {:?}",
                absent, obj.keys().collect::<Vec<_>>(),
            );
        }
    }

    /// Default (lite=false / absent) preserves the full SearchHit shape
    /// so existing integrations keep working.
    #[tokio::test]
    async fn default_mode_returns_full_hits() {
        let store = fresh_store();
        let _id = make_learned_node_with_keywords(&store, "fat-skill", &["target"], 5);
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            registry, Arc::clone(&store), app.handle().clone(),
            "test-session".into(), "default".into(),
        );

        let out = tool.execute(json!({ "query": "target" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let obj = hits[0].as_object().unwrap();
        // Full shape: must keep the richer fields.
        assert!(obj.contains_key("relevance"));
        assert!(obj.contains_key("quality"));
        assert!(obj.contains_key("final_score"));
        // node_id is in the rich shape (it's the field skill_search uses
        // to track which learned skill to bump usage on).
        assert!(obj.contains_key("node_id"));
    }
}
