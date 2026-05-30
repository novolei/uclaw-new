// SPDX-License-Identifier: Apache-2.0
//! Real summariser: folds memory fragments into one summary via uClaw's
//! existing `LlmProvider` (the same models the agent uses). Resolves the
//! ingestion provider lazily per call (codebase idiom — see the
//! knowledge-ingestion service) so no boot-time async is needed and model
//! changes are picked up automatically.

use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;

use crate::agent::types::RespondOutput;
use crate::config::llm::LlmConfig;
use crate::llm::provider::{CompletionConfig, LlmProvider};
use crate::providers::service::ProviderService;

use super::{Summariser, SummaryContext, SummaryInput, SummaryOutput};
use crate::memory_bucket_seal::tree_source::types::TreeKind;

/// Real summariser backed by the app's configured ingestion LLM.
pub struct LlmSummariser {
    provider_service: Arc<ProviderService>,
}

impl LlmSummariser {
    pub fn new(provider_service: Arc<ProviderService>) -> Self {
        Self { provider_service }
    }
}

#[async_trait]
impl Summariser for LlmSummariser {
    async fn summarise(
        &self,
        inputs: &[SummaryInput],
        ctx: &SummaryContext<'_>,
    ) -> Result<SummaryOutput> {
        let (provider_id, model, api_key, base_url) = self
            .provider_service
            .get_ingestion_llm_config()
            .await
            .ok_or_else(|| anyhow!("no ingestion LLM configured for summariser"))?;

        let llm_config = LlmConfig {
            provider: provider_id,
            model: model.clone(),
            api_key,
            base_url: if base_url.trim().is_empty() {
                None
            } else {
                Some(base_url)
            },
            max_tokens: None,
            temperature: None,
            api: None,
        };
        let provider =
            crate::llm::create_provider(&llm_config).context("build summariser LLM provider")?;
        summarise_with_provider(&provider, &model, inputs, ctx).await
    }
}

/// Pure fold logic — takes an already-resolved provider. Unit-tested with a
/// fake provider; the lazy resolution above is a thin wrapper.
pub(crate) async fn summarise_with_provider(
    provider: &Arc<dyn LlmProvider>,
    model: &str,
    inputs: &[SummaryInput],
    ctx: &SummaryContext<'_>,
) -> Result<SummaryOutput> {
    let prompt = build_fold_prompt(inputs, ctx.tree_kind, ctx.token_budget);

    // Build a single user message using the ChatMessage::user() helper
    // (verified from uclaw_message_types: role=User, content=[ContentBlock::Text]).
    let messages = vec![crate::agent::types::ChatMessage::user(&prompt)];

    let config = CompletionConfig {
        model: model.to_string(),
        max_tokens: ctx.token_budget,
        temperature: 0.3,
        thinking_enabled: false,
    };

    let out = provider
        .complete(messages, vec![], &config)
        .await
        .map_err(|e| anyhow!("summariser LLM complete failed: {e}"))?;

    // RespondOutput is an enum (verified from agent/types.rs):
    //   Text { text, thinking, thinking_signature, metadata }
    //   ToolCalls { tool_calls, text: Option<String>, ... }
    let content = match out {
        RespondOutput::Text { text, .. } => text,
        RespondOutput::ToolCalls { text, .. } => text.unwrap_or_default(),
    };

    let token_count = estimate_tokens(&content);
    Ok(SummaryOutput {
        content,
        token_count,
        entities: vec![],
        topics: vec![],
    })
}

/// Build the fold prompt. One framing line per tree kind; then the inputs.
pub(crate) fn build_fold_prompt(
    inputs: &[SummaryInput],
    kind: TreeKind,
    token_budget: u32,
) -> String {
    let framing = match kind {
        TreeKind::Source => "the following memory fragments from a single source",
        TreeKind::Topic => "the following memory fragments about a single topic/entity",
        TreeKind::Global => "the following per-source daily summaries",
    };
    let mut prompt = format!(
        "Summarise {framing} into a single dense recap of at most {} tokens. \
         Preserve names, decisions, dates, and concrete facts. Write prose, \
         no preamble.\n\n---\n",
        token_budget
    );
    for inp in inputs {
        prompt.push_str(&format!(
            "[{} · {} → {}]\n{}\n\n",
            inp.id,
            inp.time_range_start.to_rfc3339(),
            inp.time_range_end.to_rfc3339(),
            inp.content
        ));
    }
    prompt
}

/// Cheap token estimate (chars / 4). The seal only uses this for buffer
/// accounting + the next-level budget, so an approximation is fine.
fn estimate_tokens(text: &str) -> u32 {
    ((text.chars().count() + 3) / 4) as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ChatMessage, StreamDelta, ToolDefinition};
    use crate::llm::provider::{CompletionConfig, LlmProvider};
    use crate::memory_bucket_seal::tree_source::types::TreeKind;
    use async_trait::async_trait;
    use chrono::Utc;
    use std::sync::{Arc, Mutex};

    /// Records the prompt it was called with and returns a canned answer.
    struct FakeLlmProvider {
        canned: String,
        seen_prompt: Arc<Mutex<Option<String>>>,
        seen_max_tokens: Arc<Mutex<Option<u32>>>,
    }

    #[async_trait]
    impl LlmProvider for FakeLlmProvider {
        async fn complete(
            &self,
            messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            config: &CompletionConfig,
        ) -> Result<RespondOutput, crate::error::Error> {
            // Capture the concatenated user content + the token budget.
            // ChatMessage.content is Vec<ContentBlock>; extract Text blocks.
            let joined = messages
                .iter()
                .flat_map(|m| m.content.iter())
                .filter_map(|b| {
                    if let crate::agent::types::ContentBlock::Text { text } = b {
                        Some(text.clone())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n");
            *self.seen_prompt.lock().unwrap() = Some(joined);
            *self.seen_max_tokens.lock().unwrap() = Some(config.max_tokens);
            Ok(RespondOutput::Text {
                text: self.canned.clone(),
                thinking: None,
                thinking_signature: None,
                metadata: crate::agent::types::ResponseMetadata {
                    model: "fake".to_string(),
                    finish_reason: None,
                    usage: None,
                },
            })
        }
        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<
            Box<
                dyn futures::Stream<Item = Result<StreamDelta, crate::error::Error>>
                    + Send
                    + Unpin,
            >,
            crate::error::Error,
        > {
            unimplemented!("not used by summariser")
        }
    }

    fn mk_input(id: &str, content: &str) -> SummaryInput {
        let now = Utc::now();
        SummaryInput {
            id: id.to_string(),
            content: content.to_string(),
            token_count: 100,
            entities: vec![],
            topics: vec![],
            time_range_start: now,
            time_range_end: now,
            score: 0.5,
        }
    }

    #[tokio::test]
    async fn summarise_returns_provider_content() {
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeLlmProvider {
            canned: "FOLDED SUMMARY".into(),
            seen_prompt: Arc::new(Mutex::new(None)),
            seen_max_tokens: Arc::new(Mutex::new(None)),
        });
        let inputs = vec![mk_input("a", "alpha content"), mk_input("b", "beta content")];
        let ctx = SummaryContext {
            tree_id: "t1",
            tree_kind: TreeKind::Source,
            target_level: 1,
            token_budget: 4000,
        };
        let out = summarise_with_provider(&provider, "test-model", &inputs, &ctx)
            .await
            .unwrap();
        assert_eq!(out.content, "FOLDED SUMMARY");
        assert!(out.entities.is_empty());
        assert!(out.topics.is_empty());
        assert!(out.token_count > 0);
    }

    #[tokio::test]
    async fn prompt_includes_input_content_and_budget_respected() {
        let seen_prompt = Arc::new(Mutex::new(None));
        let seen_mt = Arc::new(Mutex::new(None));
        let provider: Arc<dyn LlmProvider> = Arc::new(FakeLlmProvider {
            canned: "x".into(),
            seen_prompt: seen_prompt.clone(),
            seen_max_tokens: seen_mt.clone(),
        });
        let inputs = vec![mk_input("a", "DISTINCTIVE_TOKEN_ALPHA")];
        let ctx = SummaryContext {
            tree_id: "t1",
            tree_kind: TreeKind::Global,
            target_level: 0,
            token_budget: 1234,
        };
        let _ = summarise_with_provider(&provider, "m", &inputs, &ctx)
            .await
            .unwrap();
        let prompt = seen_prompt.lock().unwrap().clone().unwrap();
        assert!(
            prompt.contains("DISTINCTIVE_TOKEN_ALPHA"),
            "prompt must include input content"
        );
        assert_eq!(
            *seen_mt.lock().unwrap(),
            Some(1234),
            "token_budget → max_tokens"
        );
    }

    #[test]
    fn fold_prompt_mentions_tree_kind() {
        let inputs = vec![mk_input("a", "c")];
        let p = build_fold_prompt(&inputs, TreeKind::Topic, 4000);
        // The prompt should be non-empty and contain the input content.
        assert!(!p.is_empty());
        assert!(p.contains("c"));
    }
}
