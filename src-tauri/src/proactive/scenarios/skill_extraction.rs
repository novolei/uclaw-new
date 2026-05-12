use async_trait::async_trait;
use crate::memubot_config::SkillExtractionConfig;
use super::types::*;

/// 默认系统提示
pub const SKILL_EXTRACTION_SYSTEM_PROMPT: &str = r#"你是一个自我改进代理，在后台分析执行日志以提取可复用的技能和经验。

分析以下执行日志，提取可复用的技能和经验：

## 分析维度
1. **成功模式** — 哪些策略有效，为什么有效
2. **失败教训** — 哪些操作失败了，根本原因是什么，如何避免
3. **新技能指南** — 从经验中提取结构化的操作步骤（包含：上下文、原则、实现步骤、常见陷阱）
4. **优化建议** — 对未来类似任务的策略建议
5. **工具使用模式** — 哪些工具组合在什么场景下最有效

## 输出格式
如果发现了可学习的模式，用以下格式输出：

<skill_report>
<success_patterns>
有效策略及其原因
</success_patterns>
<failure_lessons>
失败教训和改进建议
</failure_lessons>
<new_skills>
<skill>
<name>技能名称</name>
<context>适用场景</context>
<principles>核心原则</principles>
<steps>实现步骤</steps>
<pitfalls>常见陷阱</pitfalls>
<!-- 3-5 条，可选 -->
<signals>
<signal>401 unauthorized</signal>
<signal>token expired</signal>
<signal>API rate limit exceeded</signal>
</signals>
<validation_hint>应用该技能后，如何验证它真的有效（一句话，可选；agent 看到后自行决定要不要验证）</validation_hint>
</skill>
</new_skills>
<optimization_suggestions>
未来优化建议
</optimization_suggestions>
<tool_patterns>
工具使用最佳实践
</tool_patterns>
</skill_report>

如果没有新的可学习模式，返回 [NO_MESSAGE]。

## 重要规则
- 关注**可复用**的模式，不是一次性的解决方案
- 增量学习：如果已有类似技能，更新而不是重复创建
- 对失败日志给予更高权重 — 从错误中学习更有价值
- 不要泛泛而谈，要基于具体的执行日志给出具体建议
"#;

/// Aggregate failure-type signals across all failed execution logs.
///
/// Iterates the failure logs, serialises each `tool_output` to a string,
/// and runs `classify_error` on it.  The union of all returned signals
/// is returned as a sorted, deduplicated `Vec<String>`.
///
/// An empty result means none of the failure messages matched a known
/// pattern — the skill still extracts, just without `signals_seen` entries.
pub fn extract_signals_seen(logs: &[ExecutionLog]) -> Vec<String> {
    use crate::proactive::scenarios::failure_signals::classify_error;
    let mut sigs = std::collections::BTreeSet::new();
    for log in logs {
        if !log.success {
            let msg = serde_json::to_string(&log.tool_output).unwrap_or_default();
            for s in classify_error(&msg) {
                sigs.insert(s.to_string());
            }
        }
    }
    sigs.into_iter().collect()
}

pub struct SkillExtractionScenario {
    config: SkillExtractionConfig,
}

impl SkillExtractionScenario {
    pub fn new(config: SkillExtractionConfig) -> Self {
        Self { config }
    }

    /// 格式化执行日志为 LLM 可读文本
    fn format_execution_logs(logs: &[ExecutionLog]) -> String {
        if logs.is_empty() {
            return "无执行日志".to_string();
        }

        let mut success_logs = Vec::new();
        let mut failure_logs = Vec::new();

        for log in logs {
            let entry = format!(
                "- [{}] Tool: {} | Duration: {}ms | Input: {} | Output: {}",
                if log.success { "SUCCESS" } else { "FAILED" },
                log.tool_name,
                log.duration_ms,
                serde_json::to_string(&log.tool_input).unwrap_or_default(),
                serde_json::to_string(&log.tool_output).unwrap_or_default(),
            );
            if log.success {
                success_logs.push(entry);
            } else {
                failure_logs.push(entry);
            }
        }

        let mut result = String::new();
        if !failure_logs.is_empty() {
            result.push_str(&format!(
                "### 失败日志 ({} 条)\n{}\n\n",
                failure_logs.len(),
                failure_logs.join("\n")
            ));
        }
        if !success_logs.is_empty() {
            result.push_str(&format!(
                "### 成功日志 ({} 条)\n{}\n",
                success_logs.len(),
                success_logs.join("\n")
            ));
        }
        result
    }
}

#[async_trait]
impl ProactiveScenario for SkillExtractionScenario {
    fn name(&self) -> &str {
        "skill_extraction"
    }

    fn description(&self) -> &str {
        "Self-Improving Agent - 从执行日志中学习，自动生成技能指南和优化建议"
    }

    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        if !self.config.enabled {
            return false;
        }

        // 条件 1: 有执行失败且配置了立即触发
        if self.config.trigger_on_failure && ctx.has_failures {
            // 仍需检查最小间隔（失败触发间隔减半但仍需间隔）
            if let Some(last) = ctx.last_trigger_at.get(self.name()) {
                if last.elapsed().as_millis() < (self.config.min_interval_ms / 2) as u128 {
                    return false;
                }
            }
            return !ctx.execution_logs.is_empty();
        }

        // 条件 2: 执行次数达到阈值
        if ctx.new_execution_count < self.config.trigger_execution_count {
            return false;
        }

        // 条件 3: 最小触发间隔
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_millis() < self.config.min_interval_ms as u128 {
                return false;
            }
        }

        !ctx.execution_logs.is_empty()
    }

    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        let mut context_messages = Vec::new();

        // 格式化执行日志
        let logs_text = Self::format_execution_logs(&ctx.execution_logs);
        context_messages.push((
            "user".to_string(),
            format!("以下是需要分析的执行日志：\n\n{}", logs_text),
        ));

        // 如果有最近对话上下文，添加作为参考
        if !ctx.recent_messages.is_empty() {
            let messages_text = ctx
                .recent_messages
                .iter()
                .take(5)
                .map(|msg| format!("[{}]: {}", msg.role, msg.content))
                .collect::<Vec<_>>()
                .join("\n");
            context_messages.push((
                "user".to_string(),
                format!("当前对话上下文（供参考）：\n{}", messages_text),
            ));
        }

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
            .unwrap_or(SKILL_EXTRACTION_SYSTEM_PROMPT)
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

    fn default_config() -> SkillExtractionConfig {
        SkillExtractionConfig::default()
    }

    fn disabled_config() -> SkillExtractionConfig {
        SkillExtractionConfig {
            enabled: false,
            ..Default::default()
        }
    }

    fn make_execution_log(tool_name: &str, success: bool, duration_ms: u64) -> ExecutionLog {
        ExecutionLog {
            session_id: "test-session".to_string(),
            iteration: 1,
            tool_name: tool_name.to_string(),
            tool_input: serde_json::json!({"arg": "value"}),
            tool_output: serde_json::json!({"result": "ok"}),
            success,
            duration_ms,
            timestamp: 1000000,
            context_summary: "test context".to_string(),
        }
    }

    fn make_context_with_logs(
        new_execution_count: usize,
        logs: Vec<ExecutionLog>,
        has_failures: bool,
    ) -> ScenarioContext {
        ScenarioContext {
            recent_messages: vec![],
            execution_logs: logs,
            pending_multimodal: vec![],
            last_trigger_at: HashMap::new(),
            tick_count: 0,
            new_message_count: 0,
            new_execution_count,
            has_failures,
            active_space_id: "default".to_string(),
        }
    }

    #[test]
    fn test_skill_extraction_name_and_description() {
        let scenario = SkillExtractionScenario::new(default_config());
        assert_eq!(scenario.name(), "skill_extraction");
        assert!(!scenario.description().is_empty());
        assert!(scenario.description().contains("Self-Improving"));
    }

    #[tokio::test]
    async fn test_should_trigger_disabled() {
        let scenario = SkillExtractionScenario::new(disabled_config());
        let logs = vec![make_execution_log("read_file", true, 100)];
        let ctx = make_context_with_logs(10, logs, false);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_not_enough_executions() {
        let scenario = SkillExtractionScenario::new(default_config());
        // 默认 trigger_execution_count = 10，这里只给 3，且无失败
        let logs = vec![make_execution_log("read_file", true, 100)];
        let ctx = make_context_with_logs(3, logs, false);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_on_failure() {
        let scenario = SkillExtractionScenario::new(default_config());
        // 有失败，且 trigger_on_failure = true，即使执行次数不够也应触发
        let logs = vec![make_execution_log("write_file", false, 200)];
        let ctx = make_context_with_logs(2, logs, true);
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_normal_threshold() {
        let scenario = SkillExtractionScenario::new(default_config());
        // 执行次数达到阈值，无失败
        let logs = vec![
            make_execution_log("read_file", true, 50),
            make_execution_log("search", true, 120),
        ];
        let ctx = make_context_with_logs(10, logs, false);
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_build_context_with_mixed_logs() {
        let scenario = SkillExtractionScenario::new(default_config());
        let logs = vec![
            make_execution_log("read_file", true, 50),
            make_execution_log("write_file", false, 200),
            make_execution_log("search", true, 120),
        ];
        let messages = vec![
            ConversationMessage {
                role: "user".to_string(),
                content: "帮我修复这个bug".to_string(),
            },
            ConversationMessage {
                role: "assistant".to_string(),
                content: "好的，我来看看".to_string(),
            },
        ];
        let ctx = ScenarioContext {
            recent_messages: messages,
            execution_logs: logs,
            pending_multimodal: vec![],
            last_trigger_at: HashMap::new(),
            tick_count: 0,
            new_message_count: 2,
            new_execution_count: 3,
            has_failures: true,
            active_space_id: "default".to_string(),
        };

        let output = scenario.build_context(&ctx).await.unwrap();

        assert_eq!(output.scenario_name, "skill_extraction");
        assert!(!output.system_prompt.is_empty());
        // 应有 2 条 context_messages: 执行日志 + 对话上下文
        assert_eq!(output.context_messages.len(), 2);
        // 第一条包含执行日志
        assert!(output.context_messages[0].1.contains("read_file"));
        assert!(output.context_messages[0].1.contains("write_file"));
        assert!(output.context_messages[0].1.contains("FAILED"));
        assert!(output.context_messages[0].1.contains("SUCCESS"));
        // 第二条包含对话上下文
        assert!(output.context_messages[1].1.contains("帮我修复这个bug"));
        assert_eq!(output.memory_types, vec!["skill", "tool"]);
        assert!(output.additional_instructions.is_none());
    }

    #[test]
    fn test_format_execution_logs() {
        // 空日志
        let empty_result = SkillExtractionScenario::format_execution_logs(&[]);
        assert_eq!(empty_result, "无执行日志");

        // 混合日志
        let logs = vec![
            make_execution_log("read_file", true, 50),
            make_execution_log("write_file", false, 200),
        ];
        let result = SkillExtractionScenario::format_execution_logs(&logs);
        assert!(result.contains("失败日志 (1 条)"));
        assert!(result.contains("成功日志 (1 条)"));
        assert!(result.contains("write_file"));
        assert!(result.contains("read_file"));
        assert!(result.contains("FAILED"));
        assert!(result.contains("SUCCESS"));
    }

    #[test]
    fn signals_seen_extracted_from_failure_logs() {
        // Build a failure log whose tool_output contains a 403 message.
        let mut log = make_execution_log("web_fetch", false, 500);
        log.tool_output = serde_json::json!({"error": "HTTP 403 Forbidden"});

        let logs = vec![log];
        let sigs = super::extract_signals_seen(&logs);

        assert!(sigs.contains(&"http_4xx".to_string()),
            "expected http_4xx; got {:?}", sigs);
        assert!(sigs.contains(&"permission_denied".to_string()),
            "expected permission_denied; got {:?}", sigs);
    }

    #[test]
    fn signals_seen_empty_for_success_logs_only() {
        let logs = vec![
            make_execution_log("read_file", true, 50),
            make_execution_log("search", true, 100),
        ];
        let sigs = super::extract_signals_seen(&logs);
        assert!(sigs.is_empty(), "no failures → signals_seen should be empty; got {:?}", sigs);
    }

    #[test]
    fn signals_seen_unions_across_multiple_failure_logs() {
        // Two distinct failure logs contributing different signals.
        // Result should contain BOTH signal sets (union, not just first).
        let mut log_a = make_execution_log("web_fetch", false, 100);
        log_a.tool_output = serde_json::json!({"error": "HTTP 403 Forbidden"});
        let mut log_b = make_execution_log("api_call", false, 500);
        log_b.tool_output = serde_json::json!({"error": "request timed out after 30s"});

        let sigs = extract_signals_seen(&[log_a, log_b]);
        // From log_a: http_4xx + permission_denied
        // From log_b: timeout
        // Union should have all three.
        assert!(sigs.contains(&"http_4xx".to_string()),  "missing http_4xx; got {:?}", sigs);
        assert!(sigs.contains(&"permission_denied".to_string()), "missing permission_denied; got {:?}", sigs);
        assert!(sigs.contains(&"timeout".to_string()),  "missing timeout; got {:?}", sigs);
    }
}
