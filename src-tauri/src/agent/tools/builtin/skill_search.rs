//! skill_search tool — agent invokes to find learned/builtin skills
//! matching a query. Returns a list of {name, summary, relevance, quality,
//! final_score, match_reasons, warnings, provenance, cited_count, node_id,
//! matched_signals} structs. Does NOT load full content (see load_skill for that).
//!
//! Side effects: emits `agent:skill-recalled` event for UI; bumps
//! usage_count on each learned-skill hit via skills::bump_usage.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §3.
//!
//! NOTE (Step 3 repoint): data source is now `memory_adapter::skills` facade
//! (bucket_seal backend). Three scoring signals that were stored in memory_graph
//! metadata fields with no equivalent in `Skill` are intentionally dropped:
//!   - `signals` (trigger-phrase bonus) — field absent in Skill; kept at 0.
//!   - `lifecycle` (draft/deprecated lifecycle warning) — field absent in Skill.
//!   - `validation_hint` (validation warning) — field absent in Skill.
//!   - Cosine/embedding channel — Skill has no embedding_json; channel removed.
//! Category filter is also dropped (no `category` field in Skill).
//! name/keyword/usage/cited ranking signals are fully preserved.

use std::sync::Arc;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::skills::SkillsRegistry;

pub struct SkillSearchTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub skill_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
    pub space_id: String,
    pub memu_client: Option<Arc<crate::memu::client::MemUClient>>,
}

impl<R: tauri::Runtime> SkillSearchTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        skill_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, skill_adapter, app_handle, conversation_id, space_id, memu_client: None }
    }

    /// Attach a `MemUClient` for future cosine-similarity channel support.
    /// Currently a no-op (Skill has no embedding_json); kept for API stability.
    pub fn with_memu(mut self, client: Option<Arc<crate::memu::client::MemUClient>>) -> Self {
        self.memu_client = client;
        self
    }
}

#[derive(Debug, Serialize)]
struct SearchHit {
    name: String,
    summary: String,
    /// Strength of evidence FROM the query (keyword hits).
    /// Range: 0.0 – higher means stronger match.
    relevance: f64,
    /// How well-validated this skill is, independent of the query.
    /// Derived from cited_count + usage_count.
    quality: f64,
    /// Used for final sorting + truncation. Today: relevance + quality.
    final_score: f64,
    /// Human-readable bullets explaining WHY this hit fired.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    match_reasons: Vec<String>,
    /// Skill-side warnings.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    warnings: Vec<String>,
    provenance: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    cited_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    node_id: Option<String>,
    /// Which signals (if any) matched this query.
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

        // Learned pass — tokenize query, search keywords via skills facade.
        // skill_score holds: slug → (kw_hits, Skill).
        let tokens: Vec<&str> = query
            .split_whitespace()
            .filter(|t| t.len() >= 2)
            .collect();
        let mut skill_score: std::collections::HashMap<
            String,
            (i64, crate::memory_adapter::skills::Skill),
        > = std::collections::HashMap::new();

        for tok in &tokens {
            match crate::memory_adapter::skills::search(
                &self.skill_adapter,
                &self.space_id,
                tok,
                50,
            )
            .await
            {
                Ok(skills) => {
                    for skill in skills {
                        let entry = skill_score
                            .entry(skill.slug.clone())
                            .or_insert((0, skill));
                        entry.0 += 1;
                    }
                }
                Err(e) => {
                    tracing::debug!(error = %e, token = %tok, "skill_search: search failed for token");
                }
            }
        }

        // Build learned hits.
        for (slug, (kw_hits, skill)) in skill_score {
            let cited = skill.cited_count;
            let usage = skill.usage_count;
            // Use first 200 chars of body as summary (Skill has no separate summary field).
            let summary = truncate_summary(&skill.body, 200);

            // Tri-tier scoring (signal channel dropped — no signals field in Skill).
            let relevance = kw_hits as f64;
            let quality = (cited as f64 * 0.5) + (usage as f64 * 0.2);
            let final_score = relevance + quality;

            // Build human-readable match_reasons.
            let mut match_reasons: Vec<String> = Vec::new();
            if kw_hits > 0 {
                match_reasons.push(format!("关键词命中 {} 个", kw_hits));
            }
            if cited > 0 {
                match_reasons.push(format!("曾被引用 {} 次", cited));
            }

            hits.push(SearchHit {
                name: skill.name.clone(),
                summary,
                relevance,
                quality,
                final_score,
                match_reasons,
                warnings: vec![],
                provenance: "learned",
                cited_count: Some(cited),
                node_id: Some(slug),
                matched_signals: vec![],
            });
        }

        // Sort by final_score desc and trim FIRST so we only bump skills the LLM actually sees.
        hits.sort_by(|a, b| b.final_score.partial_cmp(&a.final_score).unwrap_or(std::cmp::Ordering::Equal));
        hits.truncate(top_k);

        // Bump usage_count for learned hits that survived truncation (fire-and-forget; soft signal).
        for hit in hits.iter().filter(|h| h.provenance == "learned") {
            if let Some(ref slug) = hit.node_id {
                let _ = crate::memory_adapter::skills::bump_usage(
                    &self.skill_adapter,
                    &self.space_id,
                    slug,
                )
                .await;
            }
        }

        // Bundle 26-A — Skill-use telemetry: bump returned_count on each
        // disk-resident skill that came back as a hit.
        {
            let registry = self.registry.read().await;
            let now_ms = chrono::Utc::now().timestamp_millis();
            for hit in &hits {
                let Some(loaded) = registry.get_loaded(&hit.name) else {
                    continue;
                };
                let skill_dir = &loaded.manifest.path;
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

        // Emit agent:skill-recalled event.
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

        // Lite mode.
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
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;

    use crate::memory_adapter::{MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
    use crate::memory_adapter::skills::{put_skill, get_skill, Skill};

    // ── Minimal in-process adapter for tests ────────────────────────────

    /// Thread-safe in-memory `MemoryAdapter` used for unit tests.
    struct InMemoryAdapter {
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
            query: &str,
            limit: usize,
            opts: RecallOpts<'_>,
        ) -> anyhow::Result<Vec<MemoryEntry>> {
            let store = self.store.lock().unwrap();
            let terms: Vec<String> = query
                .split_whitespace()
                .map(|t| t.to_lowercase())
                .filter(|t| !t.is_empty())
                .collect();
            let mut out: Vec<MemoryEntry> = store
                .values()
                .filter(|e| {
                    if let Some(ns) = opts.namespace {
                        if e.namespace.as_deref() != Some(ns) {
                            return false;
                        }
                    }
                    let content_lower = e.content.to_lowercase();
                    terms.iter().any(|t| content_lower.contains(t.as_str()))
                })
                .cloned()
                .collect();
            out.sort_by(|a, b| a.id.cmp(&b.id));
            out.truncate(limit);
            Ok(out)
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

    // ── Test helpers ──────────────────────────────────────────────────────

    async fn seed_skill(
        adapter: &Arc<dyn MemoryAdapter>,
        slug: &str,
        name: &str,
        keywords: &[&str],
        cited: u64,
    ) {
        let skill = Skill {
            slug: slug.into(),
            space: "default".into(),
            name: name.into(),
            body: format!("body for {} {}", name, keywords.join(" ")),
            usage_count: 0,
            cited_count: cited,
            keywords: keywords.iter().map(|k| k.to_string()).collect(),
            status: "draft".into(),
        };
        put_skill(adapter, &skill).await.unwrap();
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn empty_query_returns_empty_array() {
        let adapter = InMemoryAdapter::new();
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            adapter,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool.execute(json!({ "query": "" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }

    #[tokio::test]
    async fn learned_keyword_hit_returns_skill() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "stock-research", "stock-research", &["stock", "financials"], 5).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
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
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "stock-research", "stock-research", &["stock"], 0).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let _ = tool.execute(json!({ "query": "stock" })).await.unwrap();

        // usage_count should be incremented
        let updated = get_skill(&adapter, "default", "stock-research").await.unwrap().unwrap();
        assert_eq!(updated.usage_count, 1);
    }

    #[tokio::test]
    async fn no_matches_returns_empty_array() {
        let adapter = InMemoryAdapter::new();
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            adapter,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool.execute(json!({ "query": "nonexistent_xyz_query" })).await.unwrap();
        assert_eq!(out.result, json!([]));
    }

    #[tokio::test]
    async fn final_score_split_into_relevance_and_quality() {
        let adapter = InMemoryAdapter::new();
        // cited=10 → quality = 10 * 0.5 = 5.0
        seed_skill(&adapter, "cited-skill", "cited-skill", &["budget", "finance"], 10).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
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

    #[tokio::test]
    async fn match_reasons_populated_for_keyword_hit() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "kw-skill", "kw-skill", &["refactor"], 0).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
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

    /// Lite mode: when `lite: true`, the tool output drops to {name, provenance, summary}
    /// per hit — no relevance / quality / final_score / match_reasons / warnings /
    /// cited_count / node_id / matched_signals.
    #[tokio::test]
    async fn lite_mode_returns_slim_hits() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "lite-skill", "lite-skill", &["target"], 7).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool.execute(json!({ "query": "target", "lite": true })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let hit = &hits[0];
        let obj = hit.as_object().unwrap();
        assert!(obj.contains_key("name"));
        assert!(obj.contains_key("provenance"));
        assert!(obj.contains_key("summary"));
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

    /// Default (lite=false / absent) preserves the full SearchHit shape.
    #[tokio::test]
    async fn default_mode_returns_full_hits() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "fat-skill", "fat-skill", &["target"], 5).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool.execute(json!({ "query": "target" })).await.unwrap();
        let hits = out.result.as_array().unwrap();
        assert_eq!(hits.len(), 1);
        let obj = hits[0].as_object().unwrap();
        assert!(obj.contains_key("relevance"));
        assert!(obj.contains_key("quality"));
        assert!(obj.contains_key("final_score"));
        assert!(obj.contains_key("node_id"));
    }

    /// parameters_schema should reflect default=5, max=20, and the category enum.
    #[test]
    fn top_k_default_is_5_max_is_20() {
        let adapter = InMemoryAdapter::new();
        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            adapter,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let schema = tool.parameters_schema();
        let top_k_prop = &schema["properties"]["top_k"];
        assert_eq!(top_k_prop["default"], 5, "top_k default should be 5");
        assert_eq!(top_k_prop["maximum"], 20, "top_k maximum should be 20");
        assert_eq!(top_k_prop["minimum"], 1, "top_k minimum should be 1");

        let category_prop = &schema["properties"]["category"];
        let enum_vals = category_prop["enum"].as_array().unwrap();
        assert!(enum_vals.iter().any(|v| v == "repair"), "enum should include repair");
        assert!(enum_vals.iter().any(|v| v == "optimize"), "enum should include optimize");
        assert!(enum_vals.iter().any(|v| v == "innovate"), "enum should include innovate");
    }

    /// Higher cited_count skill should rank above lower cited_count when keyword hits are equal.
    #[tokio::test]
    async fn quality_ranking_higher_cited_ranks_first() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "low-cited", "low-cited", &["deploy"], 1).await;
        seed_skill(&adapter, "high-cited", "high-cited", &["deploy"], 10).await;

        let app = tauri::test::mock_app();
        let tool = SkillSearchTool::new(
            Arc::new(RwLock::new(SkillsRegistry::new())),
            Arc::clone(&adapter),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );
        let out = tool.execute(json!({ "query": "deploy", "top_k": 10 })).await.unwrap();
        let hits = out.result.as_array().unwrap();

        let pos_low = hits.iter().position(|h| h["name"] == "low-cited");
        let pos_high = hits.iter().position(|h| h["name"] == "high-cited");

        assert!(pos_low.is_some() && pos_high.is_some(), "both skills should appear");
        assert!(
            pos_high.unwrap() < pos_low.unwrap(),
            "high-cited skill should rank above low-cited; hits: {:#?}", hits
        );
    }
}
