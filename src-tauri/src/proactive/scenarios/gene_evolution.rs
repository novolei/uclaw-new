use async_trait::async_trait;

use crate::agent::gep::distillation;
use crate::agent::gep::types::*;
use crate::memubot_config::GeneEvolutionConfig;

use super::types::*;

/// GEP Self-Evolution scenario — 从 LearningCard 候选池蒸馏新 Gene。
///
/// 当 gene_candidate_pool 积累到阈值（默认 5 条）且超过冷却时间后触发。
/// 调用 distillation prompt 让 LLM 从候选 LearningCard 中提取 Gene 六元组。
pub struct GeneEvolutionScenario {
    config: GeneEvolutionConfig,
}

impl GeneEvolutionScenario {
    pub fn new(config: GeneEvolutionConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl ProactiveScenario for GeneEvolutionScenario {
    fn name(&self) -> &str {
        "gene_evolution"
    }

    fn description(&self) -> &str {
        "GEP Self-Evolution — 从 LearningCard 候选池蒸馏新 Gene"
    }

    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        if !self.config.enabled {
            tracing::debug!("[gene_evolution] scenario disabled, skip trigger");
            return false;
        }

        // Check candidate pool threshold
        if ctx.gene_candidate_count < self.config.gene_distillation_threshold {
            tracing::debug!(
                "[gene_evolution] candidate count {} below threshold {}, skip",
                ctx.gene_candidate_count,
                self.config.gene_distillation_threshold
            );
            return false;
        }

        // Check cooldown
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            let elapsed = last.elapsed().as_secs();
            if elapsed < self.config.gene_distillation_cooldown_secs {
                tracing::debug!(
                    "[gene_evolution] cooldown not elapsed ({}s < {}s), skip",
                    elapsed,
                    self.config.gene_distillation_cooldown_secs
                );
                return false;
            }
        }

        tracing::info!(
            "[gene_evolution] trigger conditions met: candidates={}, threshold={}",
            ctx.gene_candidate_count,
            self.config.gene_distillation_threshold
        );
        true
    }

    async fn build_context(&self, ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        let mut context_messages: Vec<(String, String)> = Vec::new();

        // Inject existing Gene fingerprints for dedup (if any)
        if !ctx.existing_gene_fingerprints.is_empty() {
            let count = ctx.existing_gene_fingerprints.len();
            let fp_text = ctx
                .existing_gene_fingerprints
                .iter()
                .enumerate()
                .map(|(i, fp)| format!("{}. {}", i + 1, fp))
                .collect::<Vec<_>>()
                .join("\n");
            context_messages.push((
                "user".to_string(),
                format!(
                    "## 已有 Gene 库（{} 条，避免重复）\n\n以下是当前已学得的 Gene 汇总。如果候选 LearningCard 的模式已被已有 Gene 覆盖，请勿抽取新 Gene。\n\n{}",
                    count, fp_text
                ),
            ));
        }

        // Format candidate LearningCards
        let cards_text = distillation::format_learning_cards(&ctx.gene_candidates);
        context_messages.push((
            "user".to_string(),
            format!(
                "以下是 {} 条候选学习记录，请从中蒸馏出 Gene：\n\n{}",
                ctx.gene_candidates.len(),
                cards_text
            ),
        ));

        Ok(ScenarioOutput {
            scenario_name: self.name().to_string(),
            system_prompt: self.system_prompt().to_string(),
            context_messages,
            memory_types: self.memory_types(),
            additional_instructions: Some(
                "请严格按照 Gene XML 格式输出。如果没有可蒸馏的 Gene，返回 [NO_GENE]。".to_string(),
            ),
        })
    }

    fn system_prompt(&self) -> &str {
        distillation::GENE_DISTILLATION_SYSTEM_PROMPT
    }

    fn memory_types(&self) -> Vec<String> {
        vec!["gene".to_string(), "learning".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn default_gene_config() -> GeneEvolutionConfig {
        GeneEvolutionConfig {
            enabled: true,
            ..GeneEvolutionConfig::default()
        }
    }

    fn make_context(
        candidate_count: usize,
        candidates: Vec<LearningCard>,
        last_trigger_secs_ago: Option<u64>,
    ) -> ScenarioContext {
        let mut last_trigger_at = HashMap::new();
        if let Some(secs) = last_trigger_secs_ago {
            last_trigger_at.insert(
                "gene_evolution".to_string(),
                std::time::Instant::now() - std::time::Duration::from_secs(secs),
            );
        }
        ScenarioContext {
            recent_messages: vec![],
            execution_logs: vec![],
            pending_multimodal: vec![],
            last_trigger_at,
            tick_count: 0,
            new_message_count: 0,
            new_execution_count: 0,
            has_failures: false,
            active_space_id: "default".to_string(),
            active_session_id: None,
            session_context: None,
            existing_skill_fingerprints: vec![],
            gene_candidate_count: candidate_count,
            gene_candidates: candidates,
            existing_gene_fingerprints: vec![],
        }
    }

    #[test]
    fn test_gene_evolution_name_and_description() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        assert_eq!(scenario.name(), "gene_evolution");
        assert!(!scenario.description().is_empty());
    }

    #[tokio::test]
    async fn test_should_trigger_disabled() {
        let mut config = default_gene_config();
        config.enabled = false;
        let scenario = GeneEvolutionScenario::new(config);
        let ctx = make_context(10, vec![], None);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_below_threshold() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        // Default threshold is 5, give only 3
        let ctx = make_context(3, vec![], None);
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_meets_threshold() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        let ctx = make_context(5, vec![], None);
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_cooldown_not_elapsed() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        // Last triggered 100s ago, cooldown is 600s default
        let ctx = make_context(5, vec![], Some(100));
        assert!(!scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_should_trigger_cooldown_elapsed() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        // Last triggered 700s ago, cooldown is 600s default
        let ctx = make_context(5, vec![], Some(700));
        assert!(scenario.should_trigger(&ctx).await);
    }

    #[tokio::test]
    async fn test_build_context_basic() {
        let scenario = GeneEvolutionScenario::new(default_gene_config());
        let learning_card = LearningCard {
            raw: "当 Yahoo 403 时，切换到 Alpha Vantage 作为备用源".to_string(),
            card_type: LearningCardType::FailureLesson,
            failure_signal: Some("403".to_string()),
            tool_name: Some("web_fetch".to_string()),
            strategy_hint: StrategyHint {
                condition: Some("Yahoo returns 403".to_string()),
                action: Some("switch to Alpha Vantage".to_string()),
                reason: Some("Alpha Vantage has free tier".to_string()),
            },
            files_touched: vec![],
            session_id: "test-session".to_string(),
            score: 0.85,
            timestamp: 1715779200000,
        };

        let ctx = make_context(1, vec![learning_card], None);
        let output = scenario.build_context(&ctx).await.unwrap();

        assert_eq!(output.scenario_name, "gene_evolution");
        assert!(!output.system_prompt.is_empty());
        assert!(!output.context_messages.is_empty());
        assert!(output.context_messages[0].1.contains("候选学习记录"));
        assert_eq!(output.memory_types, vec!["gene", "learning"]);
    }
}
