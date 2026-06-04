//! load_skill tool — agent fetches the full body of a skill by name.
//! Returns { name, version, content, parameters, provenance }.
//!
//! Resolution order: SkillsRegistry (builtin) first, then
//! memory_adapter::skills facade for learned skills.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §4.

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::skills::SkillsRegistry;

pub struct LoadSkillTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub skill_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
    /// openhuman-F: Procedure store for authoritative usage-count writes.
    pub memory_graph_store: Arc<crate::memory_graph::store::MemoryGraphStore>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
    pub space_id: String,
}

impl<R: tauri::Runtime> LoadSkillTool<R> {
    pub fn new(
        registry: Arc<RwLock<SkillsRegistry>>,
        skill_adapter: Arc<dyn crate::memory_adapter::MemoryAdapter>,
        memory_graph_store: Arc<crate::memory_graph::store::MemoryGraphStore>,
        app_handle: tauri::AppHandle<R>,
        conversation_id: String,
        space_id: String,
    ) -> Self {
        Self { registry, skill_adapter, memory_graph_store, app_handle, conversation_id, space_id }
    }
}

#[async_trait]
impl<R: tauri::Runtime> Tool for LoadSkillTool<R> {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load full content of a named skill (use after skill_search picks a match). Apply the returned prompt body to the current task."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Exact skill name." },
                "reason": { "type": "string", "description": "One sentence: why loading this. Shown as a chip in the UI." }
            },
            "required": ["name", "reason"]
        })
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();

        let name = params["name"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("name is required".into()))?
            .trim()
            .to_string();
        let reason = params["reason"]
            .as_str()
            .ok_or_else(|| ToolError::InvalidParams("reason is required".into()))?
            .trim()
            .to_string();

        // Builtin first
        {
            let registry = self.registry.read().await;
            if let Some(loaded) = registry.get_loaded(&name) {
                let result = json!({
                    "name": loaded.manifest.name,
                    "version": loaded.manifest.version,
                    "content": loaded.prompt_content,
                    "parameters": loaded.manifest.parameters.iter().map(|p| json!({
                        "name": p.name,
                        "type": p.r#type,
                        "required": p.required,
                        "description": p.description,
                    })).collect::<Vec<_>>(),
                    "provenance": "builtin",
                    "validation_hint": serde_json::Value::Null,
                });
                self.emit_recalled(&params, "builtin", &name, &reason);
                return Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64));
            }
        }

        // Learned — normalize title same way record_skill_cited / skill_parser does
        let normalized = crate::proactive::skill_parser::normalize_title_for_dedup(&name);

        let skill = match crate::memory_adapter::skills::get_skill(&self.skill_adapter, &self.space_id, &normalized).await {
            Ok(Some(s)) => s,
            Ok(None) => return Err(ToolError::Execution(format!("Skill '{}' not found", name))),
            Err(e) => return Err(ToolError::Execution(format!("get_skill failed: {e:#}"))),
        };

        // openhuman-F: bump usage_count on the authoritative Procedure node + re-project.
        // Falls back silently if the skill isn't in the Procedure store yet (projection-only entries).
        crate::proactive::skill_parser::bump_skill_and_reproject(
            &self.memory_graph_store,
            &self.skill_adapter,
            &self.space_id,
            &normalized,
            false,
            true,
        ).await;

        self.emit_recalled(&params, "learned", &skill.name, &reason);

        let result = json!({
            "name": skill.name,
            "version": skill.slug,
            "content": skill.body,
            "parameters": [],
            "provenance": "learned",
            "validation_hint": serde_json::Value::Null,
        });
        Ok(ToolOutput::new(result, start.elapsed().as_millis() as u64))
    }
}

impl<R: tauri::Runtime> LoadSkillTool<R> {
    fn emit_recalled(&self, params: &serde_json::Value, provenance: &str, name: &str, reason: &str) {
        let tool_call_id = params["_tool_call_id"].as_str().unwrap_or("").to_string();
        let _ = self.app_handle.emit("agent:skill-recalled", json!({
            "conversationId": self.conversation_id,
            "toolCallId": tool_call_id,
            "kind": "load",
            "name": name,
            "reason": reason,
            "provenance": provenance,
            "timestamp": chrono::Utc::now().to_rfc3339(),
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use crate::memory_adapter::{MemoryAdapter, MemoryCategory, MemoryEntry, NamespaceSummary, RecallOpts};
    use crate::memory_adapter::skills::{put_skill, Skill};

    // ── Minimal in-memory adapter (mirrors the one in memory_adapter::skills tests) ──

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

    // ── Helpers ───────────────────────────────────────────────────────────

    async fn seed_skill(adapter: &Arc<dyn MemoryAdapter>, slug: &str, name: &str, body: &str) {
        put_skill(adapter, &Skill {
            slug: slug.into(),
            space: "default".into(),
            name: name.into(),
            body: body.into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec![],
            status: "draft".into(),
        }).await.unwrap();
    }

    fn fresh_memory_graph_store() -> Arc<crate::memory_graph::store::MemoryGraphStore> {
        use crate::memory_graph::store::MemoryGraphStore;
        use std::sync::Mutex;
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        let conn = Arc::new(Mutex::new(conn));
        let store = MemoryGraphStore::new(conn);
        store.ensure_tables();
        Arc::new(store)
    }

    fn make_tool(adapter: Arc<dyn MemoryAdapter>) -> LoadSkillTool<tauri::test::MockRuntime> {
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        LoadSkillTool::new(
            registry,
            adapter,
            fresh_memory_graph_store(),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        )
    }

    // ── Tests ─────────────────────────────────────────────────────────────

    #[tokio::test]
    async fn learned_skill_loads_active_version_content() {
        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "stock-research", "stock-research", "# Stock Research SOP\n\nStep 1: ...").await;
        let tool = make_tool(adapter);

        let out = tool.execute(json!({
            "name": "stock-research",
            "reason": "User asked about Apple financials"
        })).await.unwrap();

        assert_eq!(out.result["provenance"], "learned");
        assert_eq!(out.result["name"], "stock-research");
        assert!(out.result["content"].as_str().unwrap().contains("Step 1"));
    }

    #[tokio::test]
    async fn unknown_skill_returns_tool_error() {
        let adapter = InMemoryAdapter::new();
        let tool = make_tool(adapter);

        let err = tool.execute(json!({
            "name": "does-not-exist",
            "reason": "trying"
        })).await.unwrap_err();

        match err {
            ToolError::Execution(msg) => assert!(msg.contains("not found")),
            other => panic!("expected Execution error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn load_bumps_usage_count() {
        use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};

        let adapter = InMemoryAdapter::new();
        seed_skill(&adapter, "stock-research", "stock-research", "body").await;

        // openhuman-F: usage bump goes through the Procedure node. Create one so the
        // authoritative path can find it and bump usage_count.
        let store = fresh_memory_graph_store();
        let now = chrono::Utc::now().to_rfc3339();
        let node_id = "test-load-bump-01".to_string();
        let node = MemoryNode {
            id: node_id.clone(),
            space_id: "default".to_string(),
            kind: MemoryNodeKind::Procedure,
            title: "stock-research".to_string(),
            metadata: Some(serde_json::json!({
                "skill_type": "learned",
                "usage_count": 0u64,
                "cited_count": 0u64,
                "lifecycle": "draft",
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).unwrap();
        store.create_version(&MemoryVersion {
            id: "test-load-bump-v01".to_string(),
            node_id: node_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: "body".to_string(),
            metadata: None,
            embedding_json: None,
            created_at: now.clone(),
        }).unwrap();

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            Arc::clone(&adapter),
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let _ = tool.execute(json!({
            "name": "stock-research",
            "reason": "trying"
        })).await.unwrap();

        // usage_count incremented on the Procedure node (authoritative).
        let updated = store.find_learned_skill_by_normalized_title("default", "stock-research")
            .unwrap().unwrap();
        let usage = updated.metadata.as_ref()
            .and_then(|m| m.get("usage_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        assert_eq!(usage, 1, "usage_count must be incremented on the Procedure node");
    }

    #[tokio::test]
    async fn builtin_skill_loads_from_registry() {
        use crate::skills::{LoadedSkill, SkillManifest, ActivationCriteria};
        use std::path::PathBuf;

        let adapter = InMemoryAdapter::new();
        let mut registry = SkillsRegistry::new();
        let skill = LoadedSkill {
            manifest: SkillManifest {
                name: "writing-assistant".to_string(),
                version: "2.1.0".to_string(),
                description: "Help refine prose".to_string(),
                author: "uclaw".to_string(),
                category: "general".to_string(),
                enabled: true,
                activation: ActivationCriteria::default(),
                parameters: vec![],
                requires: vec![],
                tools: vec![],
                path: PathBuf::from("/test/writing-assistant/SKILL.md"),
            },
            prompt_content: "# Writing Assistant\n\nUse for prose work.".to_string(),
            compiled_patterns: vec![],
            lowercased_keywords: vec![],
            lowercased_exclude_keywords: vec![],
            lowercased_tags: vec![],
            provenance: crate::skills::SkillProvenance::Bundled,
            plugin_id: None,
        };
        registry.register(skill);
        let registry = Arc::new(RwLock::new(registry));

        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            adapter,
            fresh_memory_graph_store(),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let out = tool.execute(json!({
            "name": "writing-assistant",
            "reason": "User wants prose help"
        })).await.unwrap();

        assert_eq!(out.result["provenance"], "builtin");
        assert_eq!(out.result["name"], "writing-assistant");
        assert_eq!(out.result["version"], "2.1.0");
        assert!(out.result["content"].as_str().unwrap().contains("Writing Assistant"));
        assert!(out.result["validation_hint"].is_null(), "builtin should return null for validation_hint");
    }

    #[tokio::test]
    async fn returns_body_as_content_from_skill_facade() {
        let adapter = InMemoryAdapter::new();
        put_skill(&adapter, &Skill {
            slug: "verify-skill".into(),
            space: "default".into(),
            name: "verify-skill".into(),
            body: "body content here".into(),
            usage_count: 0,
            cited_count: 0,
            keywords: vec![],
            status: "draft".into(),
        }).await.unwrap();

        let tool = make_tool(adapter);

        let out = tool.execute(json!({ "name": "verify-skill", "reason": "test" })).await.unwrap();
        assert_eq!(out.result["content"], "body content here");
        assert_eq!(out.result["provenance"], "learned");
    }
}
