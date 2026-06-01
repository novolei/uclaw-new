//! 实体 → gbrain 页:get_page 命中则 LLM 合并,未命中则新建,统一 put_page。

use crate::gbrain::browse::{self, PageDetail};
use crate::ingestion::extract::ExtractedEntity;
use crate::ingestion::job::IngestError;
use crate::llm::provider::LlmProvider;
use crate::mcp::SharedMcpManager;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeAction {
    Create,
    Merge,
}

/// 据现有页是否存在决定动作。纯函数,易测。
pub fn decide(existing: Option<&PageDetail>) -> MergeAction {
    match existing {
        Some(_) => MergeAction::Merge,
        None => MergeAction::Create,
    }
}

const MERGE_SYSTEM: &str = "You merge NEW information into an EXISTING knowledge page. \
Preserve the existing structure and all existing facts; add or update with the new info, do not delete. \
Return ONLY the full merged markdown body (no code fences, no commentary).";

/// 写一个实体。返回写入的 slug。
pub async fn write_entity(
    mcp: &SharedMcpManager,
    adapter: Option<&Arc<dyn crate::memory_adapter::MemoryAdapter>>,
    dual_write_enabled: bool,
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    entity: &ExtractedEntity,
) -> Result<String, IngestError> {
    let existing = match browse::get_page(mcp, &entity.slug).await {
        Ok(p) => Some(p),
        Err(e) => {
            let msg = e_to_string(&e);
            if msg.contains("page_not_found") {
                None
            } else {
                return Err(IngestError::Gbrain(msg));
            }
        }
    };

    let content = match decide(existing.as_ref()) {
        MergeAction::Create => entity.compiled_truth.clone(),
        MergeAction::Merge => {
            let existing = existing.unwrap();
            let user = format!(
                "EXISTING PAGE (slug {}):\n\n{}\n\n---\nNEW INFORMATION:\n\n{}",
                entity.slug, existing.compiled_truth, entity.compiled_truth
            );
            complete_text(provider, model, MERGE_SYSTEM, &user).await?
        }
    };

    crate::memory_adapter::page_dual_write::dual_write_page(
        mcp,
        adapter,
        &entity.slug,
        &content,
        dual_write_enabled,
    )
    .await
    .map_err(|e| IngestError::Gbrain(e_to_string(&e)))?;
    Ok(entity.slug.clone())
}

/// LLM 一次性文本补全(摄入复用)。
pub async fn complete_text(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    system: &str,
    user: &str,
) -> Result<String, IngestError> {
    use crate::agent::types::{ChatMessage, RespondOutput};
    use crate::llm::provider::CompletionConfig;
    let messages = vec![ChatMessage::system(system), ChatMessage::user(user)];
    let config = CompletionConfig {
        model: model.to_string(),
        max_tokens: 4096,
        temperature: 0.3,
        thinking_enabled: false,
    };
    let out = provider
        .complete(messages, vec![], &config)
        .await
        .map_err(|e| IngestError::Llm(e.to_string()))?;
    Ok(match out {
        RespondOutput::Text { text, .. } => text,
        RespondOutput::ToolCalls { text, .. } => text.unwrap_or_default(),
    })
}

fn e_to_string(e: &crate::gbrain::browse::GbrainError) -> String {
    format!("{:?}", e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decide_create_when_absent() {
        assert_eq!(decide(None), MergeAction::Create);
    }

    #[test]
    fn decide_merge_when_present() {
        let p = PageDetail {
            slug: "concept/x".into(), title: "X".into(), page_type: "concept".into(),
            compiled_truth: "body".into(), frontmatter: serde_json::json!({}),
            created_at: None, updated_at: None, tags: vec![], raw_markdown: String::new(),
        };
        assert_eq!(decide(Some(&p)), MergeAction::Merge);
    }
}
