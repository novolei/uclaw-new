use async_trait::async_trait;

use crate::memubot_config::MultimodalContextConfig;
use crate::proactive::multimodal::MultimodalPreprocessor;

use super::types::*;

pub const MULTIMODAL_CONTEXT_SYSTEM_PROMPT: &str = r#"你是一个多模态上下文构建器，负责统一理解和关联不同来源的信息。

你的任务是：
1. **跨模态关联**：找出文本、图片、文档、代码之间的语义联系
2. **统一理解**：构建跨不同输入类型的综合知识图谱
3. **上下文预测**：预测用户可能需要的相关信息
4. **知识整合**：将分散的信息片段整合为连贯的知识

## 输出格式
如果发现了有价值的跨模态关联，用以下格式输出：

<multimodal_report>
<cross_references>
跨模态关联发现（如：文档A中的概念X与代码B中的实现Y相关）
</cross_references>
<unified_understanding>
跨模态统一理解摘要
</unified_understanding>
<predicted_needs>
预测用户可能需要的信息
</predicted_needs>
<knowledge_items>
提取的知识点列表
</knowledge_items>
</multimodal_report>

如果没有发现有价值的跨模态关联，返回 [NO_MESSAGE]。
"#;

pub struct MultimodalContextScenario {
    config: MultimodalContextConfig,
}

impl MultimodalContextScenario {
    pub fn new(config: MultimodalContextConfig) -> Self {
        Self { config }
    }

    /// 检查输入类型是否被支持
    fn is_supported(&self, source_type: &MultimodalSourceType) -> bool {
        let type_str = match source_type {
            MultimodalSourceType::Image => "image",
            MultimodalSourceType::Document => "document",
            MultimodalSourceType::Code => "code",
            MultimodalSourceType::Audio => "audio",
        };
        self.config.supported_types.iter().any(|t| t == type_str)
    }
}

#[async_trait]
impl ProactiveScenario for MultimodalContextScenario {
    fn name(&self) -> &str {
        "multimodal_context"
    }

    fn description(&self) -> &str {
        "Multimodal Context Builder - 统一多模态记忆，实现跨模态上下文构建"
    }

    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        if !self.config.enabled {
            return false;
        }

        // 冷却时间检查
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_millis() < self.config.min_interval_ms as u128 {
                return false;
            }
        }

        // 有待处理的多模态输入且类型被支持
        ctx.pending_multimodal
            .iter()
            .any(|input| self.is_supported(&input.source_type))
    }

    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        let mut context_messages = Vec::new();
        let mut processed_items = Vec::new();

        // 预处理每个多模态输入
        for input in &ctx.pending_multimodal {
            if !self.is_supported(&input.source_type) {
                continue;
            }

            match MultimodalPreprocessor::preprocess(input).await {
                Ok((text, caption)) => {
                    // 检查内容长度限制
                    let truncated_text = if text.len() > self.config.max_content_length {
                        let safe_len = text
                            .char_indices()
                            .map(|(i, _)| i)
                            .take_while(|&i| i < self.config.max_content_length)
                            .last()
                            .unwrap_or(text.len().min(self.config.max_content_length));
                        format!(
                            "{}...[truncated]",
                            &text[..safe_len]
                        )
                    } else {
                        text
                    };
                    processed_items.push(format!("{}\n{}", caption, truncated_text));
                }
                Err(e) => {
                    tracing::warn!(
                        "[MultimodalContext] Failed to preprocess {}: {}",
                        input.filename.as_deref().unwrap_or("unknown"),
                        e
                    );
                }
            }
        }

        if processed_items.is_empty() {
            return Ok(ScenarioOutput {
                scenario_name: self.name().to_string(),
                system_prompt: self.system_prompt().to_string(),
                context_messages: vec![],
                memory_types: self.memory_types(),
                additional_instructions: Some(
                    "No processable multimodal inputs found.".to_string(),
                ),
            });
        }

        // 添加最近对话上下文
        if !ctx.recent_messages.is_empty() {
            let messages_text = ctx
                .recent_messages
                .iter()
                .take(10)
                .map(|msg| format!("[{}]: {}", msg.role, msg.content))
                .collect::<Vec<_>>()
                .join("\n");
            context_messages.push((
                "user".to_string(),
                format!("当前对话上下文：\n{}", messages_text),
            ));
        }

        // 添加多模态输入
        let multimodal_text = processed_items.join("\n\n---\n\n");
        context_messages.push((
            "user".to_string(),
            format!(
                "以下是需要分析的多模态输入（共 {} 项）：\n\n{}",
                processed_items.len(),
                multimodal_text
            ),
        ));

        Ok(ScenarioOutput {
            scenario_name: self.name().to_string(),
            system_prompt: self.system_prompt().to_string(),
            context_messages,
            memory_types: self.memory_types(),
            additional_instructions: None,
        })
    }

    fn system_prompt(&self) -> &str {
        self.config
            .system_prompt
            .as_deref()
            .unwrap_or(MULTIMODAL_CONTEXT_SYSTEM_PROMPT)
    }

    fn memory_types(&self) -> Vec<String> {
        vec!["knowledge".to_string(), "event".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memubot_config::MultimodalContextConfig;
    use std::collections::HashMap;

    fn default_config() -> MultimodalContextConfig {
        MultimodalContextConfig::default()
    }

    fn make_input(
        source_type: MultimodalSourceType,
        content: &str,
        filename: Option<&str>,
    ) -> MultimodalInput {
        MultimodalInput {
            source_type,
            content_text: content.to_string(),
            caption: String::new(),
            mime_type: "application/octet-stream".to_string(),
            filename: filename.map(|s| s.to_string()),
            metadata: serde_json::Value::Null,
            ingested_at: 0,
        }
    }

    fn make_context(inputs: Vec<MultimodalInput>) -> ScenarioContext {
        ScenarioContext {
            recent_messages: vec![],
            execution_logs: vec![],
            pending_multimodal: inputs,
            last_trigger_at: HashMap::new(),
            tick_count: 0,
            new_message_count: 0,
            new_execution_count: 0,
            has_failures: false,
            active_space_id: "default".to_string(),
            active_session_id: None,
            session_context: None,
            existing_skill_fingerprints: vec![],
        }
    }

    #[test]
    fn test_scenario_name_and_description() {
        let scenario = MultimodalContextScenario::new(default_config());
        assert_eq!(scenario.name(), "multimodal_context");
        assert!(!scenario.description().is_empty());
    }

    #[tokio::test]
    async fn test_should_trigger_when_disabled() {
        let mut config = default_config();
        config.enabled = false;
        let scenario = MultimodalContextScenario::new(config);
        let ctx = make_context(vec![make_input(
            MultimodalSourceType::Image,
            "data",
            Some("img.png"),
        )]);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_with_supported_input() {
        let scenario = MultimodalContextScenario::new(default_config());
        let ctx = make_context(vec![make_input(
            MultimodalSourceType::Code,
            "fn main() {}",
            Some("main.rs"),
        )]);
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_not_trigger_unsupported_type() {
        let config = default_config();
        // Audio is not in default supported_types
        let scenario = MultimodalContextScenario::new(config);
        let ctx = make_context(vec![make_input(
            MultimodalSourceType::Audio,
            "",
            Some("speech.wav"),
        )]);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_build_context_empty_inputs() {
        let scenario = MultimodalContextScenario::new(default_config());
        let ctx = make_context(vec![]);
        let output = scenario.build_context(&ctx).await.unwrap();
        assert!(output.context_messages.is_empty());
        assert!(output.additional_instructions.is_some());
    }

    #[tokio::test]
    async fn test_build_context_with_inputs() {
        let scenario = MultimodalContextScenario::new(default_config());
        let ctx = make_context(vec![
            make_input(MultimodalSourceType::Document, "Hello doc", Some("readme.md")),
            make_input(MultimodalSourceType::Code, "fn main() {}", Some("main.rs")),
        ]);
        let output = scenario.build_context(&ctx).await.unwrap();
        assert_eq!(output.scenario_name, "multimodal_context");
        assert!(!output.context_messages.is_empty());
        assert!(output.additional_instructions.is_none());
        // Should contain both processed items
        let last_msg = &output.context_messages.last().unwrap().1;
        assert!(last_msg.contains("2 项"));
    }

    #[tokio::test]
    async fn test_build_context_truncates_long_content() {
        let mut config = default_config();
        config.max_content_length = 20;
        let scenario = MultimodalContextScenario::new(config);
        let ctx = make_context(vec![make_input(
            MultimodalSourceType::Document,
            &"a".repeat(100),
            Some("big.txt"),
        )]);
        let output = scenario.build_context(&ctx).await.unwrap();
        let last_msg = &output.context_messages.last().unwrap().1;
        assert!(last_msg.contains("[truncated]"));
    }

    #[test]
    fn test_system_prompt_default() {
        let scenario = MultimodalContextScenario::new(default_config());
        assert!(scenario.system_prompt().contains("多模态上下文构建器"));
    }

    #[test]
    fn test_system_prompt_custom() {
        let mut config = default_config();
        config.system_prompt = Some("Custom prompt".to_string());
        let scenario = MultimodalContextScenario::new(config);
        assert_eq!(scenario.system_prompt(), "Custom prompt");
    }

    #[test]
    fn test_memory_types() {
        let scenario = MultimodalContextScenario::new(default_config());
        let types = scenario.memory_types();
        assert!(types.contains(&"knowledge".to_string()));
        assert!(types.contains(&"event".to_string()));
    }

    #[tokio::test]
    async fn test_build_context_includes_conversation() {
        use crate::infra::ConversationMessage;

        let scenario = MultimodalContextScenario::new(default_config());
        let mut ctx = make_context(vec![make_input(
            MultimodalSourceType::Document,
            "doc content",
            Some("file.txt"),
        )]);
        ctx.recent_messages = vec![ConversationMessage {
            role: "user".to_string(),
            content: "Tell me about this file".to_string(),
        }];
        let output = scenario.build_context(&ctx).await.unwrap();
        // Should have 2 context messages: conversation + multimodal
        assert_eq!(output.context_messages.len(), 2);
        assert!(output.context_messages[0].1.contains("Tell me about this file"));
    }
}
