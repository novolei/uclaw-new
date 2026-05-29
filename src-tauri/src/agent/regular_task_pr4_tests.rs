use super::*;
use crate::agent::types::{LoopSignal, RespondOutput, ResponseMetadata, TextAction, ToolCall};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

fn task_spec(id: &str) -> TaskSpec {
    TaskSpec {
        id: id.into(),
        intent_id: format!("intent-{id}"),
        goal: "test".into(),
        plan_ref: None,
        policy: crate::runtime::contracts::PolicySpec {
            effective_autonomy: crate::runtime::contracts::AutonomyLevel::SupervisedTask,
            require_step_approval: false,
            tool_permission_rule_ids: vec![],
        },
        budget: crate::runtime::contracts::BudgetSpec::default(),
        capability_profile: "default".into(),
        output_contract: crate::runtime::contracts::OutputContract::FreeText,
        checkpoint_policy: crate::runtime::contracts::CheckpointPolicy::OnHumanBoundary,
    }
}

struct NeedApprovalDelegate;

#[async_trait]
impl LoopDelegate for NeedApprovalDelegate {
    async fn check_signals(&self) -> LoopSignal {
        LoopSignal::Continue
    }

    async fn before_llm_call(
        &self,
        _ctx: &mut ReasoningContext,
        _iter: usize,
    ) -> Option<LoopOutcome> {
        Some(LoopOutcome::NeedApproval {
            tool_name: "shell".into(),
            tool_call_id: "toolu-1".into(),
            parameters: serde_json::json!({"cmd": "date"}),
        })
    }

    async fn call_llm(
        &self,
        _ctx: &mut ReasoningContext,
        _snapshot: &crate::agent::turn::TurnSnapshot,
        _iter: usize,
    ) -> Result<RespondOutput, crate::error::Error> {
        panic!("NeedApproval from before_llm_call should short-circuit call_llm")
    }

    async fn handle_text_response(
        &self,
        _text: &str,
        _meta: ResponseMetadata,
        _ctx: &mut ReasoningContext,
    ) -> TextAction {
        TextAction::Continue
    }

    async fn execute_tool_calls(
        &self,
        _tcs: Vec<ToolCall>,
        _ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, crate::error::Error> {
        Ok(None)
    }

    async fn on_usage(&self, _usage: &crate::agent::types::TokenUsage, _ctx: &ReasoningContext, _snapshot: &crate::agent::turn::TurnSnapshot) {}

    async fn on_tool_intent_nudge(&self, _text: &str, _ctx: &mut ReasoningContext) {}

    async fn after_iteration(&self, _iter: usize) {}
}

#[tokio::test]
async fn need_approval_emits_boundary_yield_without_terminal_finish() {
    let reason_ctx = Arc::new(Mutex::new(ReasoningContext::new("system".into())));
    let task = Arc::new(RegularTask::new(
        task_spec("approval-boundary"),
        RegularTaskInputs {
            delegate: Arc::new(NeedApprovalDelegate),
            reason_ctx,
            config: AgenticLoopConfig::default(),
        },
    ));

    let events = task.run(CancellationToken::new()).await;

    assert_eq!(
        events.len(),
        2,
        "expected Started + BoundaryYield: {events:?}"
    );
    assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
    match &events[1] {
        TaskEvent::BoundaryYield { source, reason, .. } => {
            assert_eq!(*source, TaskEventSource::AgentLoop);
            assert_eq!(reason, "awaiting approval for tool `shell` (toolu-1)");
        }
        other => panic!("expected BoundaryYield, got {other:?}"),
    }
    assert!(!events
        .iter()
        .any(|event| matches!(event, TaskEvent::TaskFinished { .. })));
}
