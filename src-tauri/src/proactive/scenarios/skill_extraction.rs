use async_trait::async_trait;
use crate::memubot_config::SkillExtractionConfig;
use super::types::*;

/// 默认系统提示
///
/// Rewritten 2026-05-13 (PR-mattpocock-A) — borrowing the authoring stance
/// from mattpocock/skills: opinionated single-purpose procedures with
/// anti-patterns leading, not neutral five-dimension bullet lists.
///
/// Core changes vs the pre-PR version:
/// 1. Extracting a *new skill* is the only first-class output. Success
///    patterns / failure lessons / optimization suggestions / tool patterns
///    are auxiliary — drop them when there's nothing notable.
/// 2. The `description` field gains hard constraints: one sentence, third
///    person, "Use when..." trigger clause, ≤120 chars. Quality of recall
///    is upper-bounded by quality of this string.
/// 3. New `## 反模式` (anti-pattern) section listing concrete noise to
///    discard. LLMs default to producing generic advice; named anti-patterns
///    keep them honest.
/// 4. Self-audit checklist before output — "is this skill specific to a
///    failure mode? is description ≤120 chars? am I describing one verb?"
pub const SKILL_EXTRACTION_SYSTEM_PROMPT: &str = r#"你是一个自我改进代理，在后台分析执行日志，**抽取一个新的可复用 skill**。

## 核心任务

**只有一件事重要：抽取新 skill**。其他四个维度（成功模式 / 失败教训 / 优化建议 / 工具使用模式）是辅料；
没有明确观察到就**跳过**，不要为了凑齐输出而泛泛而谈。

一个合格的 skill 满足三个条件，缺一不可：

1. **治一个具体的 agent 失败模式**（不是宽泛建议）。如果只能描述成"应该多用 X / 注意 Y"，那不是 skill，是常识。
2. **`description` 字段一句话能讲清**：≤120 字符；第三人称；以"用于 X，当 Y 时触发"（"Use when..."）的结构写。
   - ✅ "跨源校验股票财报，当 Yahoo 返回 403 / API key 失效时切换源"
   - ❌ "Helps with stock research"（含义模糊）
   - ❌ "提供完整的股票财报研究方法学指南"（自吹自夸 / 内容自描述）
3. **不是已有 skill 的重复**。如果跟既有 skill 高度相似，宁可省略也不要创建近似副本。

## 反模式（看到这些请放弃当前 skill 抽取）

- **复述大模型常识**："多用 try-catch" / "写好测试" / "仔细阅读文档" / "保持代码简洁"
- **单次成功的偶然事件**：一次性的 patch / 配置改动 / debug 输出，下次不会再用
- **过度泛化**："勤总结" / "及时复盘" / "工具组合很重要" —— 没操作步骤就不是 skill
- **重复修复同一个 bug 仍当作新技能**：3 次失败地处理同一个 API key 问题不是 3 个 skill，是同一个
- **description 写成 `Helps with X` / `提供 X 的指南`**：违反 `description` 必须含触发条件的硬约束

## 输出优先级

1. **`<new_skills>` 是主菜**：找到 0~3 条合格的就够了。**没有也很好**，返回 [NO_MESSAGE]。
2. 辅料（按价值排序）：
   - `<failure_lessons>`：从这次执行的真实失败中提炼。**未观察到失败就留空**。
   - `<optimization_suggestions>`：明确可衡量的改进点（"用 cache_read 可省 N token"）。**无明确改进就留空**。
   - `<success_patterns>` 和 `<tool_patterns>`：这两个最容易变成水货，**默认不输出**，只有出现强信号才填。

## 输出格式

<skill_report>
<new_skills>
<skill>
<name>kebab-case-skill-name</name>
<description>一句话，≤120 字符，第三人称，含 "用于 X，当 Y 时" 结构</description>
<context>简短描述适用场景（agent 调用前应该处在什么状况）</context>
<principles>核心原则（这条 skill 的世界观，2-4 句）</principles>
<steps>实现步骤（编号或 markdown 列表，可执行的具体动作）</steps>
<anti_patterns>
<!-- 这条 skill 的反模式：执行时绝对不要做的事。可选但强烈推荐填。
     例："不要在 401 时立即重试同一 endpoint" / "不要假设单一数据源是权威" -->
</anti_patterns>
<pitfalls>常见陷阱（已经看到/可预见的失败方式）</pitfalls>
<signals>
<!-- 3-5 条触发关键词或错误消息，agent 看到这些信号会想到本 skill -->
<signal>触发短语 / 错误关键词</signal>
</signals>
<validation_hint>应用该 skill 后，如何在不依赖人工的情况下验证它真的有效（一句话，可选）</validation_hint>
<!-- 三选一：repair（修 bug / 错误恢复）| optimize（已知问题的更高效解法）| innovate（探索新方法）。不确定就留空。 -->
<category>repair</category>
<tags>
<!-- 0-3 个领域标签，用于 per-workspace 作用域过滤（V19）。
     格式：小写 + 单词/kebab-case；只在很有把握时填。空集合表示"全局可用"（默认）。
     常用词表（推荐复用，避免每条 skill 自创近义词）：
       engineering · testing · debugging · navigation · refactor
       frontend · backend · data · infrastructure
       research · writing · communication · design · planning
     如果 skill 跨域通用（如 "压缩输出"、"礼貌拒答"），**留空** —— V19 用
     "未打标 = 全局" 规则保持它在所有 workspace 可见。-->
<tag>领域标签1</tag>
<tag>领域标签2</tag>
</tags>
</skill>
</new_skills>
<failure_lessons>
<!-- 可选。仅在执行日志含明确失败时填。 -->
</failure_lessons>
<optimization_suggestions>
<!-- 可选。仅在有可衡量改进点时填。 -->
</optimization_suggestions>
</skill_report>

如果没有新的可学习模式，返回 [NO_MESSAGE]。

## 输出前自审清单

逐条检查；任意一条不满足，就**不要**输出该 skill：

- [ ] 这条 skill 治一个**具体**的 agent 失败模式（不是 "应该多注意 X"）
- [ ] `description` ≤120 字符、第三人称、含"用于 X，当 Y 时"触发结构
- [ ] 跟当前 skill 库里现有的 skill **不重复**（明显主题/做法相似就不算新 skill）
- [ ] 至少能填出 2-3 条具体的 `steps`（"建立工作流" 不算具体）
- [ ] 反模式 (`anti_patterns`) 来自这次执行的真实观察，不是脑补
- [ ] `tags` 只填**很有把握**的领域标签；跨域通用 skill 必须留空（V19 "未打标 = 全局"）
- [ ] 如果你抽出来的内容大模型已经默认会做（"用 git 提交前 review diff"），跳过

## 重要规则

- **优先 failure-driven**：从 failure 学到的 skill 比 success 复述更有价值
- **写作风格用祈使句 + 反例**：参考 `caveman / diagnose / tdd` 这类高质量公共 skill 的体例
- **`description` 决定一切**：agent 的 router 只看 description 决定要不要 load 这条 skill。description 含义模糊 → 这条 skill 永远不会被召回 → 写了等于没写。
- **宁缺毋滥**：返回 `[NO_MESSAGE]` 是合格输出。出 1 条好 skill 比出 5 条水货价值大得多。
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

    /// 格式化执行日志为 LLM 可读文本（压缩版）
    ///
    /// 优化策略：
    /// - 单独列出失败日志（高信号），成功日志取 top 条目
    /// - 每条的 input/output 截断至 ≤200 字符
    /// - 总日志数限制 ≤20 条（10 失败 + 10 成功），超出则标注截断
    /// - 相比原始全量输出，可减少 40-60% token
    fn format_execution_logs(logs: &[ExecutionLog]) -> String {
        if logs.is_empty() {
            return "无执行日志".to_string();
        }

        let mut failure_logs: Vec<String> = Vec::new();
        let mut success_logs: Vec<String> = Vec::new();

        let truncate = |s: &str, max_len: usize| -> String {
            if s.chars().count() > max_len {
                format!("{}…", s.chars().take(max_len).collect::<String>())
            } else {
                s.to_string()
            }
        };

        for log in logs {
            let input_str = serde_json::to_string(&log.tool_input).unwrap_or_default();
            let output_str = serde_json::to_string(&log.tool_output).unwrap_or_default();
            let entry = format!(
                "- [{}] {} | {}ms | in:{} | out:{}",
                if log.success { "OK" } else { "FAIL" },
                log.tool_name,
                log.duration_ms,
                truncate(&input_str, 200),
                truncate(&output_str, 200),
            );
            if log.success {
                success_logs.push(entry);
            } else {
                failure_logs.push(entry);
            }
        }

        let total = logs.len();
        let mut result = String::with_capacity(4096);
        result.push_str(&format!("### 执行日志（共 {} 条，{} 成功 / {} 失败）\n", total, success_logs.len(), failure_logs.len()));

        // 失败日志优先展示（高信号），限制 ≤10 条
        if !failure_logs.is_empty() {
            let limit = 10usize.min(failure_logs.len());
            if failure_logs.len() > limit {
                result.push_str(&format!("失败日志（最近 {} / {} 条）:\n", limit, failure_logs.len()));
            }
            for entry in failure_logs.iter().rev().take(limit).rev() {
                result.push_str(entry);
                result.push('\n');
            }
        }

        // 成功日志限制 ≤10 条
        if !success_logs.is_empty() {
            let limit = 10usize.min(success_logs.len());
            if success_logs.len() > limit {
                result.push_str(&format!("\n成功日志（最近 {} / {} 条）:\n", limit, success_logs.len()));
            } else {
                result.push_str("\n成功日志:\n");
            }
            for entry in success_logs.iter().rev().take(limit).rev() {
                result.push_str(entry);
                result.push('\n');
            }
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
            tracing::debug!("[skill_extraction] scenario disabled, skip trigger");
            return false;
        }

        // 条件 1: 有执行失败且配置了立即触发
        if self.config.trigger_on_failure && ctx.has_failures {
            // 仍需检查最小间隔（失败触发间隔减半但仍需间隔）
            if let Some(last) = ctx.last_trigger_at.get(self.name()) {
                if last.elapsed().as_millis() < (self.config.min_interval_ms / 2) as u128 {
                    tracing::debug!("[skill_extraction] failure-trigger cooldown not elapsed, skip");
                    return false;
                }
            }
            let has_logs = !ctx.execution_logs.is_empty();
            tracing::info!(
                has_failures = true,
                execution_logs = ctx.execution_logs.len(),
                "[skill_extraction] failure-triggered, will_fire={}",
                has_logs
            );
            return has_logs;
        }

        // 条件 2: 执行次数达到阈值
        if ctx.new_execution_count < self.config.trigger_execution_count {
            tracing::debug!(
                new_execution_count = ctx.new_execution_count,
                threshold = self.config.trigger_execution_count,
                "[skill_extraction] execution count below threshold, skip"
            );
            return false;
        }

        // 条件 3: 最小触发间隔
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_millis() < self.config.min_interval_ms as u128 {
                tracing::debug!("[skill_extraction] min_interval cooldown not elapsed, skip");
                return false;
            }
        }

        let has_logs = !ctx.execution_logs.is_empty();
        tracing::info!(
            new_execution_count = ctx.new_execution_count,
            execution_logs = ctx.execution_logs.len(),
            "[skill_extraction] threshold-triggered, will_fire={}",
            has_logs
        );
        has_logs
    }

    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        let mut context_messages = Vec::new();

        // 注入已有技能指纹作为前置去重参考（首个 context message，优先级最高）。
        // 每条指纹格式: "title | description(≤60chars) | category | cited:N"
        // LLM 在生成新 <skill> 前逐条对比，避免创建近似副本。
        if !ctx.existing_skill_fingerprints.is_empty() {
            let count = ctx.existing_skill_fingerprints.len();
            let fp_text = ctx
                .existing_skill_fingerprints
                .iter()
                .enumerate()
                .map(|(i, fp)| format!("{}. {}", i + 1, fp))
                .collect::<Vec<_>>()
                .join("\n");
            context_messages.push((
                "user".to_string(),
                format!(
                    "## 已有技能库（{} 条，避免重复）\n\n以下是当前已学得的技能汇总。如果执行日志中的模式已被已有技能覆盖，请勿抽取新 skill。\n\n{}",
                    count, fp_text
                ),
            ));
        }

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
            active_session_id: None,
            session_context: None,
            existing_skill_fingerprints: vec![],
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
            active_session_id: None,
            session_context: None,
            existing_skill_fingerprints: vec![],
        };

        let output = scenario.build_context(&ctx).await.unwrap();

        assert_eq!(output.scenario_name, "skill_extraction");
        assert!(!output.system_prompt.is_empty());
        // 应有 2 条 context_messages: 执行日志 + 对话上下文
        assert_eq!(output.context_messages.len(), 2);
        // 第一条包含执行日志
        assert!(output.context_messages[0].1.contains("read_file"));
        assert!(output.context_messages[0].1.contains("write_file"));
        assert!(output.context_messages[0].1.contains("FAIL"));
        assert!(output.context_messages[0].1.contains("OK"));
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
        assert!(result.contains("1 成功 / 1 失败"));
        assert!(result.contains("write_file"));
        assert!(result.contains("read_file"));
        assert!(result.contains("FAIL"));
        assert!(result.contains("OK"));
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
