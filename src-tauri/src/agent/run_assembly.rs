use crate::agent::agentic_loop::run_agentic_loop;
use crate::agent::hook_bus::{HookBus, HookEvent};
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, LoopOutcome, ReasoningContext};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum AgentRunOutcome {
    Completed(LoopOutcome),
    TimedOut,
    Cancelled,
}

pub struct AgentRunConfig {
    pub task_id: String,
    pub timeout_secs: u64,
}

pub struct AgentRunAssembly<'a> {
    pub delegate: &'a dyn LoopDelegate,
    pub ctx: &'a mut ReasoningContext,
    pub config: &'a AgenticLoopConfig,
    pub token: CancellationToken,
    pub hook_bus: Arc<HookBus>,
    pub run_config: AgentRunConfig,
}

pub async fn run_agent(assembly: AgentRunAssembly<'_>) -> AgentRunOutcome {
    let AgentRunAssembly {
        delegate,
        ctx,
        config,
        token,
        hook_bus,
        run_config,
    } = assembly;

    hook_bus
        .dispatch_observe(&HookEvent::TaskStart {
            task_id: run_config.task_id.clone(),
            intent_id: String::new(),
        })
        .await;

    let outcome = tokio::select! {
        result = tokio::time::timeout(
            std::time::Duration::from_secs(run_config.timeout_secs),
            run_agentic_loop(delegate, ctx, config),
        ) => match result {
            Ok(outcome) => AgentRunOutcome::Completed(outcome),
            Err(_) => AgentRunOutcome::TimedOut,
        },
        _ = token.cancelled() => AgentRunOutcome::Cancelled,
    };

    hook_bus
        .dispatch_observe(&HookEvent::TaskEnd {
            task_id: run_config.task_id,
            outcome: task_outcome_label(&outcome).to_string(),
        })
        .await;

    outcome
}

fn task_outcome_label(outcome: &AgentRunOutcome) -> &'static str {
    match outcome {
        AgentRunOutcome::Completed(loop_outcome) => match loop_outcome {
            LoopOutcome::Response { .. } | LoopOutcome::ToolResult { .. } => "completed",
            LoopOutcome::Stopped | LoopOutcome::Cancelled { .. } => "cancelled",
            LoopOutcome::MaxIterations | LoopOutcome::Failure { .. } => "failed",
            LoopOutcome::NeedApproval { .. } => "completed",
        },
        AgentRunOutcome::TimedOut => "failed",
        AgentRunOutcome::Cancelled => "cancelled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn completed_need_approval_labels_completed() {
        let outcome = AgentRunOutcome::Completed(LoopOutcome::NeedApproval {
            tool_name: "edit".to_string(),
            tool_call_id: "tool-call".to_string(),
            parameters: serde_json::json!({}),
        });
        assert_eq!(task_outcome_label(&outcome), "completed");
    }

    #[test]
    fn cancelled_labels_cancelled() {
        assert_eq!(task_outcome_label(&AgentRunOutcome::Cancelled), "cancelled");
    }
}
