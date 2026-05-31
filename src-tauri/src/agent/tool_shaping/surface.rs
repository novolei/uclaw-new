use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

use crate::agent::tool_shaping::exposure::ToolExposurePolicy;
use crate::agent::tool_shaping::normalize::{
    normalize_tool_schema, NormalizeStats, DEFAULT_MAX_NESTING_DEPTH,
};
use crate::agent::tools::tool::ToolRegistry;
use crate::agent::types::ToolDefinition;

#[derive(Debug, Clone)]
pub struct PerTurnToolSurface {
    pub definitions: Vec<ToolDefinition>,
    pub normalize_stats: NormalizeStats,
    pub definition_hash: u64,
}

impl PerTurnToolSurface {
    pub fn from_registry(registry: &ToolRegistry) -> Self {
        Self::from_registry_with_policy(registry, &ToolExposurePolicy::default_policy())
    }

    pub fn from_registry_with_policy(registry: &ToolRegistry, policy: &ToolExposurePolicy) -> Self {
        let mut definitions = registry
            .list_definitions()
            .into_iter()
            .filter(|definition| policy.is_announced_by_default(&definition.name))
            .collect::<Vec<_>>();

        let mut normalize_stats = NormalizeStats::default();
        for definition in definitions.iter_mut() {
            let raw = std::mem::replace(&mut definition.parameters, serde_json::Value::Null);
            let (rewritten, stats) = normalize_tool_schema(raw, DEFAULT_MAX_NESTING_DEPTH);
            definition.parameters = rewritten;
            normalize_stats.examples_dropped += stats.examples_dropped;
            normalize_stats.enums_deduped += stats.enums_deduped;
            normalize_stats.deep_nests_pruned += stats.deep_nests_pruned;
        }

        let definition_hash = hash_definition_names(&definitions);
        Self {
            definitions,
            normalize_stats,
            definition_hash,
        }
    }
}

fn hash_definition_names(definitions: &[ToolDefinition]) -> u64 {
    let mut hasher = DefaultHasher::new();
    for definition in definitions {
        definition.name.hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::tool_shaping::ToolExposure;
    use crate::agent::tools::tool::{Tool, ToolError, ToolOutput};
    use async_trait::async_trait;

    struct DummyTool(&'static str);

    #[async_trait]
    impl Tool for DummyTool {
        fn name(&self) -> &str {
            self.0
        }

        fn description(&self) -> &str {
            "dummy"
        }

        fn parameters_schema(&self) -> serde_json::Value {
            serde_json::json!({
                "type": "object",
                "description": {
                    "text": "dummy",
                    "examples": ["drop me"]
                }
            })
        }

        async fn execute(&self, _params: serde_json::Value) -> Result<ToolOutput, ToolError> {
            Ok(ToolOutput::new(serde_json::json!({"ok": true}), 0))
        }
    }

    #[test]
    fn hidden_tools_do_not_enter_surface() {
        let mut registry = ToolRegistry::new();
        registry.register(DummyTool("visible"));
        registry.register(DummyTool("hidden"));

        let policy = ToolExposurePolicy::default_policy().with_tool("hidden", ToolExposure::Hidden);
        let surface = PerTurnToolSurface::from_registry_with_policy(&registry, &policy);

        let names = surface
            .definitions
            .iter()
            .map(|definition| definition.name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["visible"]);
        assert_eq!(surface.normalize_stats.examples_dropped, 1);
    }
}
