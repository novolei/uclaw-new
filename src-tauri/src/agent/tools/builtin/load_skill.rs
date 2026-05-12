//! load_skill tool — agent fetches the full body of a skill by name.
//! Returns { name, version, content, parameters, provenance }.
//!
//! Resolution order: SkillsRegistry (builtin) first, then
//! MemoryGraphStore.find_learned_skill_by_normalized_title for learned.
//!
//! See docs/superpowers/specs/2026-05-12-skill-recall-design.md §4.

use std::sync::Arc;
use async_trait::async_trait;
use serde_json::json;
use tauri::Emitter;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
use crate::memory_graph::store::MemoryGraphStore;
use crate::skills::SkillsRegistry;

pub struct LoadSkillTool<R: tauri::Runtime = tauri::Wry> {
    pub registry: Arc<RwLock<SkillsRegistry>>,
    pub store: Arc<MemoryGraphStore>,
    pub app_handle: tauri::AppHandle<R>,
    pub conversation_id: String,
    pub space_id: String,
}

impl<R: tauri::Runtime> LoadSkillTool<R> {
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

#[async_trait]
impl<R: tauri::Runtime> Tool for LoadSkillTool<R> {
    fn name(&self) -> &str {
        "load_skill"
    }

    fn description(&self) -> &str {
        "Load the full content of a skill. Use after skill_search identifies a promising match. The returned content is the skill's full prompt body — read it, then apply it to the current task."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        json!({
            "type": "object",
            "properties": {
                "name": { "type": "string", "description": "Exact skill name." },
                "reason": {
                    "type": "string",
                    "description": "One sentence: why you're loading this skill in the current context. Surfaces as a chip in the UI; helps the user audit your reasoning."
                }
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

        // Learned — normalize title same way record_skill_cited does
        let normalized = crate::skills::normalize_skill_title(&name);

        let node = self.store
            .find_learned_skill_by_normalized_title(&self.space_id, &normalized)
            .map_err(|e| ToolError::Execution(format!("lookup failed: {}", e)))?;

        let node = match node {
            Some(n) => n,
            None => {
                return Err(ToolError::Execution(format!("Skill '{}' not found", name)));
            }
        };

        let version = self.store
            .get_active_version(&node.id)
            .map_err(|e| ToolError::Execution(format!("get_active_version failed: {}", e)))?
            .ok_or_else(|| ToolError::Execution(format!("Skill '{}' has no active version", name)))?;

        // Bump usage_count for the load action (same counter as search; soft signal)
        if let Err(e) = self.store.bump_skill_usage(&[node.id.as_str()]) {
            tracing::warn!("bump_skill_usage failed: {}", e);
        }

        self.emit_recalled(&params, "learned", &node.title, &reason);

        let validation_hint = node.metadata.as_ref()
            .and_then(|m| m.get("validation_hint"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let result = json!({
            "name": node.title,
            "version": version.id,
            "content": version.content,
            "parameters": [],
            "provenance": "learned",
            "validation_hint": validation_hint,
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
    use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};
    use chrono::Utc;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = std::sync::Arc::new(std::sync::Mutex::new(
            rusqlite::Connection::open_in_memory().unwrap(),
        ));
        let _ = conn.lock().unwrap().execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
        Arc::new(MemoryGraphStore::new(conn))
    }

    fn make_learned(store: &MemoryGraphStore, title: &str, body: &str) -> String {
        let now = Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        store.create_node(&MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: title.into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": format!("Summary for {}", title),
                "cited_count": 0,
                "usage_count": 0,
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        }).unwrap();
        store.create_version(&MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: body.into(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        }).unwrap();
        id
    }

    #[tokio::test]
    async fn learned_skill_loads_active_version_content() {
        let store = fresh_store();
        make_learned(&store, "stock-research", "# Stock Research SOP\n\nStep 1: ...");
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

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
        let store = fresh_store();
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

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
        let store = fresh_store();
        let id = make_learned(&store, "stock-research", "body");
        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            Arc::clone(&store),
            app.handle().clone(),
            "test-session".into(),
            "default".into(),
        );

        let _ = tool.execute(json!({
            "name": "stock-research",
            "reason": "trying"
        })).await.unwrap();

        let node = store.get_node(&id).unwrap().unwrap();
        let usage = node.metadata.unwrap().get("usage_count").unwrap().as_u64().unwrap();
        assert_eq!(usage, 1);
    }

    #[tokio::test]
    async fn builtin_skill_loads_from_registry() {
        use crate::skills::{LoadedSkill, SkillManifest, ActivationCriteria};
        use std::path::PathBuf;

        let store = fresh_store();
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
        };
        registry.register(skill);
        let registry = Arc::new(RwLock::new(registry));

        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
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
    async fn returns_validation_hint_from_metadata() {
        use crate::memory_graph::models::{MemoryNode, MemoryNodeKind, MemoryVersion, MemoryVersionStatus};

        let store = fresh_store();
        let now = chrono::Utc::now().to_rfc3339();
        let id = uuid::Uuid::new_v4().to_string();
        store.create_node(&MemoryNode {
            id: id.clone(),
            space_id: "default".into(),
            kind: MemoryNodeKind::Procedure,
            title: "verify-skill".into(),
            metadata: Some(json!({
                "skill_type": "learned",
                "enabled": true,
                "summary": "x",
                "validation_hint": "Re-run with --quiet and check exit code"
            })),
            created_at: now.clone(),
            updated_at: now.clone(),
        }).unwrap();
        store.create_version(&MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: id,
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: "body".into(),
            metadata: None,
            embedding_json: None,
            created_at: now,
        }).unwrap();

        let registry = Arc::new(RwLock::new(SkillsRegistry::new()));
        let app = tauri::test::mock_app();
        let tool = LoadSkillTool::new(
            registry,
            store,
            app.handle().clone(),
            "sess".into(),
            "default".into(),
        );

        let out = tool.execute(json!({ "name": "verify-skill", "reason": "test" })).await.unwrap();
        assert_eq!(out.result["validation_hint"], "Re-run with --quiet and check exit code");
        assert_eq!(out.result["provenance"], "learned");
    }
}
