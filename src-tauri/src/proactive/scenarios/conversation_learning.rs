use async_trait::async_trait;
use crate::memubot_config::ConversationLearningConfig;
use super::types::*;

/// 默认系统提示
pub const CONVERSATION_LEARNING_SYSTEM_PROMPT: &str = r#"你是一个持续学习助手，在后台默默工作。你的任务是分析用户最近的对话，提取有价值的信息用于改善未来的交互。

分析以下维度：
1. **用户偏好**：沟通风格（正式/随意）、技术偏好（语言、框架、工具）、工作习惯
2. **新知识点**：用户提到的新概念、项目、技术细节
3. **行为模式**：时间偏好、决策风格、反复出现的需求
4. **跟进事项**：未完成的任务、提到要做但还没做的事

## 输出格式
如果发现了新的有价值信息，用以下 XML 格式输出：

<learning_report>
<preferences>
发现的用户偏好描述
</preferences>
<knowledge>
新发现的知识点
</knowledge>
<patterns>
观察到的行为模式
</patterns>
<followups>
需要跟进的事项
</followups>
<summary>
一句话总结本次学习成果
</summary>
</learning_report>

如果没有发现任何新的有价值信息，直接返回 [NO_MESSAGE]。

## 重要规则
- 只提取**新的**信息，不要重复已知的内容
- 关注隐含的偏好（如用户总是用中文提问 = 偏好中文交流）
- 不要臆测，只基于对话中明确的证据
"#;

pub struct ConversationLearningScenario {
    config: ConversationLearningConfig,
}

impl ConversationLearningScenario {
    pub fn new(config: ConversationLearningConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ProactiveScenario for ConversationLearningScenario {
    fn name(&self) -> &str {
        "conversation_learning"
    }

    fn description(&self) -> &str {
        "Always-Learning Assistant - 从每次对话中自动学习用户偏好和行为模式"
    }

    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        if !self.config.enabled {
            return false;
        }

        // 条件 1: 新消息数达到阈值
        if ctx.new_message_count < self.config.trigger_message_count {
            return false;
        }

        // 条件 2: 最小触发间隔
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_millis() < self.config.min_interval_ms as u128 {
                return false;
            }
        }

        // 条件 3: 至少有一些最近消息可供分析
        !ctx.recent_messages.is_empty()
    }

    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        // 构建最近消息的上下文
        let mut context_messages = Vec::new();

        // 添加最近对话内容作为分析材料
        let messages_text = ctx.recent_messages.iter()
            .map(|msg| format!("{}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");

        context_messages.push((
            "user".to_string(),
            format!("以下是需要分析的最近对话记录：\n\n{}", messages_text),
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
        self.config.system_prompt.as_deref()
            .unwrap_or(CONVERSATION_LEARNING_SYSTEM_PROMPT)
    }

    fn memory_types(&self) -> Vec<String> {
        self.config.memory_types.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::infra::ConversationMessage;

    fn default_config() -> ConversationLearningConfig {
        ConversationLearningConfig::default()
    }

    fn disabled_config() -> ConversationLearningConfig {
        ConversationLearningConfig {
            enabled: false,
            ..Default::default()
        }
    }

    fn make_messages(count: usize) -> Vec<ConversationMessage> {
        (0..count)
            .map(|i| ConversationMessage {
                role: if i % 2 == 0 { "user".to_string() } else { "assistant".to_string() },
                content: format!("消息内容 {}", i),
            })
            .collect()
    }

    fn make_context(new_message_count: usize, messages: Vec<ConversationMessage>) -> ScenarioContext {
        ScenarioContext {
            recent_messages: messages,
            execution_logs: vec![],
            pending_multimodal: vec![],
            last_trigger_at: HashMap::new(),
            tick_count: 0,
            new_message_count,
            new_execution_count: 0,
            has_failures: false,
            active_space_id: "default".to_string(),
            active_session_id: None,
            session_context: None,
            existing_skill_fingerprints: vec![],
        }
    }

    #[test]
    fn test_conversation_learning_name_and_description() {
        let scenario = ConversationLearningScenario::new(default_config());
        assert_eq!(scenario.name(), "conversation_learning");
        assert!(!scenario.description().is_empty());
        assert!(scenario.description().contains("Always-Learning"));
    }

    #[tokio::test]
    async fn test_should_trigger_disabled() {
        let scenario = ConversationLearningScenario::new(disabled_config());
        let ctx = make_context(10, make_messages(10));
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_not_enough_messages() {
        let scenario = ConversationLearningScenario::new(default_config());
        // 默认 trigger_message_count = 5，这里只给 2
        let ctx = make_context(2, make_messages(2));
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_passes() {
        let scenario = ConversationLearningScenario::new(default_config());
        // 满足所有条件: enabled=true, new_message_count >= 5, no last_trigger, recent_messages non-empty
        let ctx = make_context(5, make_messages(5));
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_build_context_output() {
        let scenario = ConversationLearningScenario::new(default_config());
        let messages = make_messages(3);
        let ctx = make_context(3, messages);

        let output = scenario.build_context(&ctx).await.unwrap();

        assert_eq!(output.scenario_name, "conversation_learning");
        assert!(!output.system_prompt.is_empty());
        assert_eq!(output.context_messages.len(), 1);
        assert_eq!(output.context_messages[0].0, "user");
        assert!(output.context_messages[0].1.contains("消息内容 0"));
        assert!(output.context_messages[0].1.contains("消息内容 2"));
        assert_eq!(output.memory_types, vec!["profile", "behavior", "event", "knowledge"]);
        assert!(output.additional_instructions.is_none());
    }

    #[test]
    fn test_system_prompt_default() {
        let scenario = ConversationLearningScenario::new(default_config());
        assert_eq!(scenario.system_prompt(), CONVERSATION_LEARNING_SYSTEM_PROMPT);
    }

    #[test]
    fn test_system_prompt_custom() {
        let config = ConversationLearningConfig {
            system_prompt: Some("custom prompt".to_string()),
            ..Default::default()
        };
        let scenario = ConversationLearningScenario::new(config);
        assert_eq!(scenario.system_prompt(), "custom prompt");
    }

    #[test]
    fn test_memory_types() {
        let scenario = ConversationLearningScenario::new(default_config());
        let types = scenario.memory_types();
        assert_eq!(types.len(), 4);
        assert!(types.contains(&"profile".to_string()));
        assert!(types.contains(&"behavior".to_string()));
    }

    #[tokio::test]
    async fn test_should_trigger_empty_messages() {
        let scenario = ConversationLearningScenario::new(default_config());
        // new_message_count 足够，但 recent_messages 为空
        let ctx = make_context(5, vec![]);
        assert!(!scenario.should_trigger(&ctx).await);
    }
}
