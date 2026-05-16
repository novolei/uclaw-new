pub mod types;
// 预留：后续 Task 创建
pub mod conversation_learning;
pub mod failure_signals;
pub mod skill_extraction;
pub mod multimodal_context;

pub use types::*;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

/// 场景管理器 - 管理所有已注册的 proactive 场景
pub struct ScenarioManager {
    scenarios: Vec<Arc<dyn ProactiveScenario>>,
    last_trigger_at: Arc<RwLock<HashMap<String, Instant>>>,
}

impl ScenarioManager {
    pub fn new() -> Self {
        Self {
            scenarios: Vec::new(),
            last_trigger_at: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 注册一个场景
    pub fn register(&mut self, scenario: Arc<dyn ProactiveScenario>) {
        tracing::info!(
            "[ScenarioManager] Registered scenario: {} - {}",
            scenario.name(),
            scenario.description()
        );
        self.scenarios.push(scenario);
    }

    /// 评估所有场景，返回应该触发的场景列表
    pub async fn evaluate_all(&self, ctx: &ScenarioContext) -> Vec<Arc<dyn ProactiveScenario>> {
        let mut triggered = Vec::new();
        for scenario in &self.scenarios {
            if scenario.should_trigger(ctx).await {
                triggered.push(Arc::clone(scenario));
            }
        }
        triggered
    }

    /// 标记场景已触发
    pub async fn mark_triggered(&self, scenario_name: &str) {
        let mut map = self.last_trigger_at.write().await;
        map.insert(scenario_name.to_string(), Instant::now());
    }

    /// 获取上次触发时间映射
    pub async fn get_last_trigger_map(&self) -> HashMap<String, Instant> {
        self.last_trigger_at.read().await.clone()
    }

    /// 获取已注册场景数量
    pub fn scenario_count(&self) -> usize {
        self.scenarios.len()
    }

    /// 获取所有场景名称
    pub fn scenario_names(&self) -> Vec<String> {
        self.scenarios.iter().map(|s| s.name().to_string()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;

    /// 测试用的 mock 场景
    struct MockScenario {
        name: String,
        description: String,
        should_trigger: bool,
    }

    impl MockScenario {
        fn new(name: &str, should_trigger: bool) -> Self {
            Self {
                name: name.to_string(),
                description: format!("Mock scenario: {}", name),
                should_trigger,
            }
        }
    }

    #[async_trait]
    impl ProactiveScenario for MockScenario {
        fn name(&self) -> &str {
            &self.name
        }

        fn description(&self) -> &str {
            &self.description
        }

        async fn should_trigger(&self, _ctx: &ScenarioContext) -> bool {
            self.should_trigger
        }

        async fn build_context(&self, _ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
            Ok(ScenarioOutput {
                scenario_name: self.name.clone(),
                system_prompt: "test prompt".to_string(),
                context_messages: vec![],
                memory_types: vec![],
                additional_instructions: None,
            })
        }

        fn system_prompt(&self) -> &str {
            "test prompt"
        }

        fn memory_types(&self) -> Vec<String> {
            vec!["test_memory".to_string()]
        }
    }

    fn make_empty_context() -> ScenarioContext {
        ScenarioContext {
            recent_messages: vec![],
            execution_logs: vec![],
            pending_multimodal: vec![],
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
    fn test_scenario_manager_new() {
        let manager = ScenarioManager::new();
        assert_eq!(manager.scenario_count(), 0);
        assert!(manager.scenario_names().is_empty());
    }

    #[test]
    fn test_register_scenario() {
        let mut manager = ScenarioManager::new();
        let scenario = Arc::new(MockScenario::new("test", true));
        manager.register(scenario);
        assert_eq!(manager.scenario_count(), 1);
        assert_eq!(manager.scenario_names(), vec!["test".to_string()]);
    }

    #[tokio::test]
    async fn test_evaluate_all_no_scenarios() {
        let manager = ScenarioManager::new();
        let ctx = make_empty_context();
        let triggered = manager.evaluate_all(&ctx).await;
        assert!(triggered.is_empty());
    }

    #[tokio::test]
    async fn test_evaluate_all_with_trigger() {
        let mut manager = ScenarioManager::new();
        manager.register(Arc::new(MockScenario::new("always", true)));
        manager.register(Arc::new(MockScenario::new("never", false)));

        let ctx = make_empty_context();
        let triggered = manager.evaluate_all(&ctx).await;
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].name(), "always");
    }

    #[tokio::test]
    async fn test_mark_triggered() {
        let manager = ScenarioManager::new();
        manager.mark_triggered("test_scenario").await;

        let map = manager.get_last_trigger_map().await;
        assert!(map.contains_key("test_scenario"));
    }
}
