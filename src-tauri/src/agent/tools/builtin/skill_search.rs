//! skill_search tool — agent invokes to find learned/builtin skills
//! matching a query. Returns a list of {name, summary, score, provenance,
//! cited_count, node_id} structs. Does NOT load full content (see
//! load_skill for that).
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
    score: f64,
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
        "Search learned skills by keywords. Returns top-N matches with one-line summaries. Use this when facing a problem similar to one you've solved before — load the full skill content via load_skill afterward if a match looks promising."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "query": {
                    "type": "string",
                    "description": "Keywords describing the current task / problem (English works better than Chinese)."
                },
                "top_k": {
                    "type": "integer",
                    "description": "Number of skills to return (default 3, max 10).",
                    "default": 3
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
        let top_k = params["top_k"].as_u64().unwrap_or(3).clamp(1, 10) as usize;

        let mut hits: Vec<SearchHit> = Vec::new();

        // Builtin pass — registry.match_skills returns scored skills already
        let registry = self.registry.read().await;
        for skill in registry.match_skills(query) {
            hits.push(SearchHit {
                name: skill.manifest.name.clone(),
                summary: truncate_summary(&skill.manifest.description, 200),
                score: crate::skills::score_skill(skill, query) as f64,
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
                for (nid, node) in new_nodes {
                    node_score.entry(nid).or_insert((0, node));
                }
                boost_map
            } else {
                std::collections::HashMap::new()
            };

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

            let cosine_bonus = cosine_boost
                .get(&node_id)
                .copied()
                .unwrap_or(0.0) as f64
                * 2.0;
            let score = (kw_hits as f64)
                + signal_match_count * 1.5
                + (cited as f64 * 0.5)
                + (usage as f64 * 0.2)
                + cosine_bonus;
            hits.push(SearchHit {
                name: node.title,
                summary: truncate_summary(&summary, 200),
                score,
                provenance: "learned",
                cited_count: Some(cited),
                node_id: Some(node_id),
                matched_signals,
            });
        }

        // Sort by score desc and trim FIRST so we only bump skills the LLM actually sees.
        hits.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
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

        // Emit agent:skill-recalled event
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

        Ok(ToolOutput::new(
            serde_json::to_value(&hits).unwrap_or(json!([])),
            start.elapsed().as_millis() as u64,
        ))
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
        use crate::memory_graph::models::{MemoryVersion, MemoryVersionStatus};
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
        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
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
}
