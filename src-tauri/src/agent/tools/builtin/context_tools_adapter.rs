//! C2-Dirac-B2 — `ContextSearchTool` + `ContextReadTool`: builtin tool
//! wrappers over the M2-F [`ContextToolSet`](crate::runtime::context_tools::ContextToolSet).
//!
//! Only the two working `ContextToolSet` operations are exposed as tools
//! (spec §8.5): `context.search` and `context.read`. The other five ops
//! (`fold` / `cite` / `compare` / `pin` / `release`) are either
//! `Err(unimplemented)` stubs or lifecycle ops out of B2 scope, and
//! registering them would let the LLM call tools that just fail — so they
//! are deliberately NOT wrapped here.
//!
//! ## search → read round-trip (the contract the LLM follows)
//!
//! `context.search { topics: [..] }` returns a JSON array of `ContextRef`
//! objects (serde camelCase: `{ "id", "source", "label"? }`). To
//! materialize one, the LLM copies a whole `ContextRef` object from that
//! array and passes it back as `context.read`'s `ref` parameter. We
//! deserialize it straight back into a [`ContextRef`] and hand it to
//! `ContextToolSet::read`. The tool descriptions spell this out so the
//! model round-trips the structured ref rather than guessing an id
//! string. (`ContextRef` needs both `id` AND `source` to resolve — an
//! id-only string would be ambiguous, hence we pass the whole object.)

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::RwLock;

use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::runtime::context::ContextRef;
use crate::runtime::context_tools::ContextToolSet;

/// `context.search` — discover fragments by topic tag.
pub struct ContextSearchTool {
    toolset: Arc<RwLock<ContextToolSet>>,
}

impl ContextSearchTool {
    pub fn new(toolset: Arc<RwLock<ContextToolSet>>) -> Self {
        Self { toolset }
    }
}

#[async_trait]
impl Tool for ContextSearchTool {
    fn name(&self) -> &str {
        "context.search"
    }

    fn description(&self) -> &str {
        "Search the available context fragments by topic tag(s). Returns a JSON array of \
         ContextRef objects, each shaped { \"id\", \"source\", \"label\"? }. Pass one or more \
         lowercase topic tags (e.g. \"conversation\", \"codebase\", \"memory\"); fragments \
         matching ANY topic are returned (OR), de-duplicated. To read a fragment's content, \
         copy one whole ContextRef object from the results and pass it as the `ref` argument \
         of context.read. Use this to pull supporting context on demand instead of preloading \
         everything."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "topics": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Lowercase topic tags to search. Multiple topics are OR-combined."
                }
            },
            "required": ["topics"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Read-only context query — never needs approval.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let topics: Vec<String> = serde_json::from_value(params["topics"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("topics: {e}")))?;

        let ts = self.toolset.read().await;
        // ContextToolSet::search takes a SINGLE topic. Loop + dedup by
        // ContextRef (which is PartialEq) preserving first-seen order.
        let mut refs: Vec<ContextRef> = Vec::new();
        for topic in &topics {
            for r in ts.search(topic) {
                if !refs.contains(&r) {
                    refs.push(r);
                }
            }
        }

        let out = serde_json::to_string_pretty(&refs)
            .map_err(|e| ToolError::Execution(format!("serialize refs: {e}")))?;
        Ok(ToolOutput::success(&out, start.elapsed().as_millis() as u64))
    }
}

/// `context.read` — materialize one fragment by its `ContextRef`.
pub struct ContextReadTool {
    toolset: Arc<RwLock<ContextToolSet>>,
}

impl ContextReadTool {
    pub fn new(toolset: Arc<RwLock<ContextToolSet>>) -> Self {
        Self { toolset }
    }
}

#[async_trait]
impl Tool for ContextReadTool {
    fn name(&self) -> &str {
        "context.read"
    }

    fn description(&self) -> &str {
        "Materialize the content of one context fragment. The `ref` argument is a ContextRef \
         object exactly as returned by context.search (shaped { \"id\", \"source\", \"label\"? }) \
         — copy a whole element from the search results. Returns the fragment as a JSON \
         ContextArtifact with its `content`, `ref`, and any `citations`."
    }

    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "ref": {
                    "type": "object",
                    "description": "A ContextRef object from a prior context.search result.",
                    "properties": {
                        "id": { "type": "string" },
                        "source": {
                            "type": "string",
                            "description": "One of: conversation, task_trace, codebase, browser, memory, artifacts, team, automation, cluster."
                        },
                        "label": { "type": "string" }
                    },
                    "required": ["id", "source"]
                }
            },
            "required": ["ref"]
        })
    }

    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        // Round-trip: the ref object came from context.search's serialized
        // output, so deserialize it straight back into a ContextRef.
        let context_ref: ContextRef = serde_json::from_value(params["ref"].clone())
            .map_err(|e| ToolError::InvalidParams(format!("ref must be a ContextRef object: {e}")))?;

        let ts = self.toolset.read().await;
        let artifact = ts
            .read(&context_ref)
            .await
            .map_err(|e| ToolError::kinded(crate::agent::tools::tool::ToolErrorKind::ResourceNotFound, e.to_string()))?;

        let out = serde_json::to_string_pretty(&artifact)
            .map_err(|e| ToolError::Execution(format!("serialize artifact: {e}")))?;
        Ok(ToolOutput::success(&out, start.elapsed().as_millis() as u64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::context::{
        ContextFragment, ContextSource, ConversationHistoryFragment, MemoryRecallFragment,
    };

    fn convo(id: &str) -> Arc<dyn ContextFragment> {
        Arc::new(ConversationHistoryFragment {
            thread_id: id.into(),
            turns: vec!["hi".into(), "there".into()],
        })
    }

    fn mem(query: &str) -> Arc<dyn ContextFragment> {
        Arc::new(MemoryRecallFragment {
            query: query.into(),
            mock_hits: vec![("page-1".into(), "recall body".into())],
        })
    }

    fn toolset_with(frags: Vec<Arc<dyn ContextFragment>>) -> Arc<RwLock<ContextToolSet>> {
        let mut ts = ContextToolSet::new();
        ts.add_all(frags);
        Arc::new(RwLock::new(ts))
    }

    // ── tool metadata ───────────────────────────────────────────────

    #[test]
    fn tools_are_named_and_never_require_approval() {
        let ts = toolset_with(vec![]);
        let search = ContextSearchTool::new(ts.clone());
        let read = ContextReadTool::new(ts);
        assert_eq!(search.name(), "context.search");
        assert_eq!(read.name(), "context.read");
        assert_eq!(
            search.requires_approval(&serde_json::json!({})),
            ApprovalRequirement::Never
        );
        assert_eq!(
            read.requires_approval(&serde_json::json!({})),
            ApprovalRequirement::Never
        );
    }

    // ── search ──────────────────────────────────────────────────────

    #[tokio::test]
    async fn search_returns_matching_refs() {
        // 2 conversation fragments + 1 memory; search "conversation" → 2.
        let ts = toolset_with(vec![convo("t1"), convo("t2"), mem("rust")]);
        let tool = ContextSearchTool::new(ts);
        let out = tool
            .execute(serde_json::json!({"topics": ["conversation"]}))
            .await
            .unwrap();
        let text = out.result["content"].as_str().unwrap();
        let refs: Vec<ContextRef> = serde_json::from_str(text).unwrap();
        assert_eq!(refs.len(), 2);
        assert!(refs.iter().all(|r| r.source == ContextSource::Conversation));
    }

    #[tokio::test]
    async fn search_dedups_across_multiple_topics() {
        // A conversation fragment is tagged both "conversation" and
        // "history"; searching both topics must NOT return it twice.
        let ts = toolset_with(vec![convo("t1")]);
        let tool = ContextSearchTool::new(ts);
        let out = tool
            .execute(serde_json::json!({"topics": ["conversation", "history"]}))
            .await
            .unwrap();
        let text = out.result["content"].as_str().unwrap();
        let refs: Vec<ContextRef> = serde_json::from_str(text).unwrap();
        assert_eq!(refs.len(), 1, "same fragment must not be duplicated");
    }

    #[tokio::test]
    async fn search_rejects_non_array_topics() {
        let ts = toolset_with(vec![]);
        let tool = ContextSearchTool::new(ts);
        let err = tool
            .execute(serde_json::json!({"topics": "not-an-array"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }

    // ── read (the round-trip) ───────────────────────────────────────

    #[tokio::test]
    async fn read_round_trips_a_search_result_ref() {
        // The full search → read contract: search output is fed verbatim
        // into read's `ref` param.
        let ts = toolset_with(vec![convo("t1")]);
        let search = ContextSearchTool::new(ts.clone());
        let read = ContextReadTool::new(ts);

        let search_out = search
            .execute(serde_json::json!({"topics": ["conversation"]}))
            .await
            .unwrap();
        let refs: serde_json::Value =
            serde_json::from_str(search_out.result["content"].as_str().unwrap()).unwrap();
        let first_ref = refs[0].clone();

        let read_out = read
            .execute(serde_json::json!({"ref": first_ref}))
            .await
            .unwrap();
        // The artifact is JSON-serialized into the tool output's `content`.
        // Parse it back and check the artifact's own `content` field
        // (asserting on the raw string would trip over JSON newline escaping).
        let artifact_text = read_out.result["content"].as_str().unwrap();
        let artifact: serde_json::Value = serde_json::from_str(artifact_text).unwrap();
        assert_eq!(
            artifact["content"].as_str().unwrap(),
            "hi\nthere",
            "fragment content missing"
        );
    }

    #[tokio::test]
    async fn read_returns_not_found_for_unknown_ref() {
        let ts = toolset_with(vec![convo("t1")]);
        let tool = ContextReadTool::new(ts);
        let err = tool
            .execute(serde_json::json!({
                "ref": { "id": "file/missing.rs", "source": "codebase" }
            }))
            .await
            .unwrap_err();
        // Mapped to a kinded ResourceNotFound error.
        match err {
            ToolError::Kinded { kind, .. } => {
                assert_eq!(kind, crate::agent::tools::tool::ToolErrorKind::ResourceNotFound)
            }
            other => panic!("expected kinded NotFound, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn read_rejects_malformed_ref() {
        let ts = toolset_with(vec![]);
        let tool = ContextReadTool::new(ts);
        let err = tool
            .execute(serde_json::json!({"ref": "just-a-string"}))
            .await
            .unwrap_err();
        assert!(matches!(err, ToolError::InvalidParams(_)));
    }
}
