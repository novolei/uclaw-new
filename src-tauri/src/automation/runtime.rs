use std::sync::Arc;
use crate::agent::agentic_loop::run_agentic_loop;
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, LoopOutcome, ReasoningContext, ChatMessage};
use super::activity::{ActivityStatus, ActivityStore, AutomationActivity};
use super::spec::AutomationSpec;

pub struct AutomationRuntime {
    pub activity_store: Arc<ActivityStore>,
    pub delegate_factory: Arc<dyn Fn(String) -> Box<dyn LoopDelegate + Send> + Send + Sync>,
}

impl AutomationRuntime {
    pub async fn run(&self, spec_id: &str, spec: &AutomationSpec, trigger: &str) {
        let activity_id = uuid::Uuid::new_v4().to_string();
        let run_id = uuid::Uuid::new_v4().to_string();
        let now = chrono::Utc::now().timestamp_millis();

        let activity = AutomationActivity {
            id: activity_id.clone(),
            spec_id: spec_id.to_string(),
            run_id,
            trigger: trigger.to_string(),
            status: ActivityStatus::Running,
            result: None,
            error: None,
            duration_ms: 0,
            created_at: now,
            completed_at: None,
        };

        if let Err(e) = self.activity_store.insert(&activity) {
            tracing::error!("Failed to record automation activity: {}", e);
            return;
        }

        let system_prompt = format!(
            "You are an automation agent. Execute the following task autonomously and report the result.\n\nTask: {}",
            spec.task,
        );
        let mut ctx = ReasoningContext::new(system_prompt);
        ctx.messages.push(ChatMessage::user(&spec.task));

        let config = AgenticLoopConfig {
            max_iterations: spec.max_iterations.unwrap_or(10) as usize,
            ..AgenticLoopConfig::default()
        };

        let delegate = (self.delegate_factory)(format!("automation:{}", spec.name));
        let start = std::time::Instant::now();
        let outcome = run_agentic_loop(delegate.as_ref(), &mut ctx, &config).await;
        let duration_ms = start.elapsed().as_millis() as i64;

        match outcome {
            LoopOutcome::Response { text, .. } => {
                if let Err(e) = self.activity_store.complete(&activity_id, &text, duration_ms) {
                    tracing::error!("Failed to record automation completion: {}", e);
                }
            }
            LoopOutcome::Failure { error } => {
                if let Err(e) = self.activity_store.fail(&activity_id, &error, duration_ms) {
                    tracing::error!("Failed to record automation failure: {}", e);
                }
            }
            other => {
                let msg = format!("{:?}", other);
                if let Err(e) = self.activity_store.fail(&activity_id, &msg, duration_ms) {
                    tracing::error!("Failed to record automation outcome: {}", e);
                }
            }
        }
    }
}
