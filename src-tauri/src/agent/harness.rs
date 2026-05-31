use crate::agent::agentic_loop::run_agentic_loop;
use crate::agent::hook_bus::{HookBus, HookEvent};
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, LoopOutcome, ReasoningContext};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub enum AgentHarnessRunOutcome {
    Completed(LoopOutcome),
    TimedOut,
    Cancelled,
}

pub struct AgentHarnessRunConfig {
    pub task_id: String,
    pub timeout_secs: u64,
}

pub async fn run_agent_harness(
    delegate: &dyn LoopDelegate,
    ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    token: CancellationToken,
    hook_bus: Arc<HookBus>,
    run_config: AgentHarnessRunConfig,
) -> AgentHarnessRunOutcome {
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
            Ok(outcome) => AgentHarnessRunOutcome::Completed(outcome),
            Err(_) => AgentHarnessRunOutcome::TimedOut,
        },
        _ = token.cancelled() => AgentHarnessRunOutcome::Cancelled,
    };

    let task_outcome = match &outcome {
        AgentHarnessRunOutcome::Completed(loop_outcome) => task_outcome_label(loop_outcome),
        AgentHarnessRunOutcome::TimedOut => "failed",
        AgentHarnessRunOutcome::Cancelled => "cancelled",
    };
    hook_bus
        .dispatch_observe(&HookEvent::TaskEnd {
            task_id: run_config.task_id,
            outcome: task_outcome.to_string(),
        })
        .await;

    outcome
}

fn task_outcome_label(outcome: &LoopOutcome) -> &'static str {
    match outcome {
        LoopOutcome::Response { .. } | LoopOutcome::ToolResult { .. } => "completed",
        LoopOutcome::Stopped | LoopOutcome::Cancelled { .. } => "cancelled",
        LoopOutcome::MaxIterations | LoopOutcome::Failure { .. } => "failed",
        LoopOutcome::NeedApproval { .. } => "completed",
    }
}
