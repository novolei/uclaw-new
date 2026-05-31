use crate::agent::hook_bus::HookBus;
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, ReasoningContext};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use crate::agent::run_assembly::{
    AgentRunConfig as AgentHarnessRunConfig, AgentRunOutcome as AgentHarnessRunOutcome,
};

pub async fn run_agent_harness(
    delegate: &dyn LoopDelegate,
    ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    token: CancellationToken,
    hook_bus: Arc<HookBus>,
    run_config: AgentHarnessRunConfig,
) -> AgentHarnessRunOutcome {
    ctx.force_text = false;
    ctx.cancellation_token = Some(token.clone());

    crate::agent::run_assembly::run_agent(crate::agent::run_assembly::AgentRunAssembly {
        delegate,
        ctx,
        config,
        token,
        hook_bus,
        run_config,
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::hook_bus::{HookEvent, HookEventKind, HookSubscriber, SubscriberId};
    use crate::agent::types::{
        AgenticLoopConfig, LoopDelegate, LoopOutcome, LoopSignal, ReasoningContext, RespondOutput,
        ResponseMetadata, TextAction,
    };
    use crate::error::Error;
    use async_trait::async_trait;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct RecordingDelegate {
        observed_force_text: Mutex<Vec<bool>>,
        observed_token_present: Mutex<Vec<bool>>,
    }

    #[async_trait]
    impl LoopDelegate for RecordingDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }

        async fn before_llm_call(
            &self,
            reason_ctx: &mut ReasoningContext,
            _iteration: usize,
        ) -> Option<LoopOutcome> {
            self.observed_force_text
                .lock()
                .unwrap()
                .push(reason_ctx.force_text);
            self.observed_token_present
                .lock()
                .unwrap()
                .push(reason_ctx.cancellation_token.is_some());
            Some(LoopOutcome::Response {
                text: "done".to_string(),
                usage: None,
                truncated: false,
                model: None,
            })
        }

        async fn call_llm(
            &self,
            _reason_ctx: &mut ReasoningContext,
            _snapshot: &crate::agent::turn::TurnSnapshot,
            _iteration: usize,
        ) -> Result<RespondOutput, Error> {
            unreachable!("before_llm_call returns a terminal outcome")
        }

        async fn handle_text_response(
            &self,
            _text: &str,
            _metadata: ResponseMetadata,
            _reason_ctx: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Stopped)
        }

        async fn execute_tool_calls(
            &self,
            _tool_calls: Vec<crate::agent::types::ToolCall>,
            _reason_ctx: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, Error> {
            Ok(None)
        }
    }

    struct RecordingHookSubscriber {
        events: Arc<Mutex<Vec<HookEvent>>>,
    }

    #[async_trait]
    impl HookSubscriber for RecordingHookSubscriber {
        fn id(&self) -> SubscriberId {
            SubscriberId::new("harness-test")
        }

        fn interest_in(&self) -> &'static [HookEventKind] {
            &[HookEventKind::TaskStart, HookEventKind::TaskEnd]
        }

        async fn on_event(
            &self,
            event: &HookEvent,
        ) -> Option<crate::runtime::contracts::HookDecision> {
            self.events.lock().unwrap().push(event.clone());
            None
        }
    }

    fn config() -> AgenticLoopConfig {
        AgenticLoopConfig {
            max_iterations: 1,
            ..Default::default()
        }
    }

    fn run_config(task_id: &str) -> AgentHarnessRunConfig {
        AgentHarnessRunConfig {
            task_id: task_id.to_string(),
            timeout_secs: 5,
        }
    }

    fn hook_bus_with_recorder() -> (Arc<HookBus>, Arc<Mutex<Vec<HookEvent>>>) {
        let mut bus = HookBus::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        bus.register(Arc::new(RecordingHookSubscriber {
            events: events.clone(),
        }))
        .unwrap();
        (Arc::new(bus), events)
    }

    #[tokio::test]
    async fn harness_resets_force_text_and_installs_cancellation_token() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("system".to_string());
        ctx.force_text = true;
        let (hook_bus, _events) = hook_bus_with_recorder();
        let token = CancellationToken::new();

        let outcome = run_agent_harness(
            &delegate,
            &mut ctx,
            &config(),
            token,
            hook_bus,
            run_config("task-1"),
        )
        .await;

        assert!(matches!(outcome, AgentHarnessRunOutcome::Completed(_)));
        assert_eq!(*delegate.observed_force_text.lock().unwrap(), vec![false]);
        assert_eq!(*delegate.observed_token_present.lock().unwrap(), vec![true]);
    }

    #[tokio::test]
    async fn harness_dispatches_task_start_and_end_once() {
        let delegate = RecordingDelegate::default();
        let mut ctx = ReasoningContext::new("system".to_string());
        let (hook_bus, events) = hook_bus_with_recorder();

        let outcome = run_agent_harness(
            &delegate,
            &mut ctx,
            &config(),
            CancellationToken::new(),
            hook_bus,
            run_config("task-2"),
        )
        .await;

        assert!(matches!(outcome, AgentHarnessRunOutcome::Completed(_)));
        let events = events.lock().unwrap().clone();
        assert_eq!(events.len(), 2);
        assert!(matches!(&events[0], HookEvent::TaskStart { task_id, .. } if task_id == "task-2"));
        assert!(
            matches!(&events[1], HookEvent::TaskEnd { task_id, outcome } if task_id == "task-2" && outcome == "completed")
        );
    }
}
