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
use crate::skills::SkillsRegistry;

pub struct SkillSearchTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub store: Arc<MemoryGraphStore>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
    pub space_id: String,
}

impl<R: tauri::Runtime> SkillSearchTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        store: Arc<MemoryGraphStore>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, store, app_handle, conversation_id, space_id }
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

            let score = (kw_hits as f64)
                + signal_match_count * 1.5
                + (cited as f64 * 0.5)
                + (usage as f64 * 0.2);
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
}
