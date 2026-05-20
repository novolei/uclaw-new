//! M1-T2c — `RegularTask` implementation of `runtime::task::SessionTask`.
//!
//! This is the **first concrete `SessionTask`**. It wraps the existing
//! 882-line `agentic_loop::run_agentic_loop` so the rest of the runtime
//! (preemptive scheduler, rollout writer, …) can drive it through a typed
//! contract.
//!
//! What this PR fixes:
//!
//! - **R-1 HIGH** (`reason_ctx.force_text` sticky one-way flag, per
//!   `docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md §B.3`):
//!   reset `force_text = false` at the start of every `RegularTask::run`.
//!
//! What this PR does NOT yet fix (deferred to M1-T2d):
//!
//! - **R-6 HIGH** (cancellation cannot interrupt an in-flight LLM call or
//!   tool execution). The `CancellationToken` parameter is currently
//!   checked only before and after the loop, not threaded through
//!   `LoopDelegate`'s 7 async boundaries. M1-T2d extends `LoopDelegate`
//!   methods with `&CancellationToken` and applies `OrCancelExt` at the
//!   two highest-risk callsites (`call_llm`, `execute_tool_calls`).
//!
//! Callsites are NOT modified by this PR — the existing direct
//! `run_agentic_loop(delegate, ctx, config)` call paths continue to work.
//! `RegularTask` is an opt-in alternative for new code (M1-T4 adapters,
//! M1-T5 rollout integration) that needs the typed `SessionTask`
//! interface.

use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::agent::agentic_loop::run_agentic_loop;
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, LoopOutcome, ReasoningContext};
use crate::runtime::contracts::{TaskEvent, TaskEventSource, TaskSpec, TaskVerdict};
use crate::runtime::task::{SessionTask, TaskKind};

/// The borrowed inputs `agentic_loop::run_agentic_loop` needs, hoisted
/// into `Arc`-wrapped shared form so a `RegularTask` can hold them
/// across the async boundary.
pub struct RegularTaskInputs {
    pub delegate: Arc<dyn LoopDelegate>,
    pub reason_ctx: Arc<Mutex<ReasoningContext>>,
    pub config: AgenticLoopConfig,
}

/// Concrete `SessionTask` driving an agent turn.
///
/// One `RegularTask` per user message. Held by `TaskScheduler` between
/// `spawn_task` and `abort_all_tasks` / completion.
pub struct RegularTask {
    spec: TaskSpec,
    inputs: RegularTaskInputs,
}

impl RegularTask {
    pub fn new(spec: TaskSpec, inputs: RegularTaskInputs) -> Self {
        Self { spec, inputs }
    }
}

#[async_trait]
impl SessionTask for RegularTask {
    fn task_id(&self) -> &str {
        &self.spec.id
    }

    fn kind(&self) -> TaskKind {
        TaskKind::Regular
    }

    fn task_spec(&self) -> &TaskSpec {
        &self.spec
    }

    async fn run(self: Arc<Self>, token: CancellationToken) -> Vec<TaskEvent> {
        let mut events = Vec::with_capacity(2);

        events.push(TaskEvent::TaskStarted {
            ts: chrono::Utc::now().to_rfc3339(),
            source: TaskEventSource::AgentLoop,
            task_id: self.spec.id.clone(),
            intent_id: self.spec.intent_id.clone(),
        });

        // Early-cancel check: if the token already fired before we even
        // acquired the context lock, bail out without running the loop.
        // (The `Cancelled` verdict carries no error — preemption is a
        // legitimate exit path, see M1-T2a.)
        if token.is_cancelled() {
            events.push(TaskEvent::TaskFinished {
                ts: chrono::Utc::now().to_rfc3339(),
                source: TaskEventSource::AgentLoop,
                task_id: self.spec.id.clone(),
                verdict: TaskVerdict::Cancelled {
                    reason: Some("cancelled before run".into()),
                },
            });
            return events;
        }

        // Acquire the shared reason_ctx for the duration of this turn.
        // `run_agentic_loop` mutates it; no other task should be reading
        // or writing it concurrently (the scheduler guarantees this by
        // never spawning > 1 Regular task at a time).
        let mut ctx = self.inputs.reason_ctx.lock().await;

        // R-1 HIGH fix — see
        // docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md §B.3.
        //
        // `force_text` is set to `true` on truncation cascade (line 354
        // of agentic_loop.rs) but never reset. Without this line, every
        // turn in a session that ever hit a truncation forces text
        // output, even when the original cause has cleared. Reset-at-
        // start covers 100% of the bug (per design Q3 in the audit
        // spec) at the cost of one extra LLM turn that might have
        // benefited from the constraint.
        ctx.force_text = false;

        let outcome = run_agentic_loop(
            self.inputs.delegate.as_ref(),
            &mut ctx,
            &self.inputs.config,
        )
        .await;

        // Drop the context lock before observing cancellation again so a
        // pending preemption can proceed without contention.
        drop(ctx);

        let verdict = outcome_to_verdict(&outcome, &token);
        events.push(TaskEvent::TaskFinished {
            ts: chrono::Utc::now().to_rfc3339(),
            source: TaskEventSource::AgentLoop,
            task_id: self.spec.id.clone(),
            verdict,
        });
        events
    }
}

/// Translate a `LoopOutcome` (and the token state) into a `TaskVerdict`.
///
/// Kept as a free function so the rest of the codebase can use the same
/// mapping when bridging older direct-callers of `run_agentic_loop` into
/// the new event vocabulary (M1-T4 adapters).
pub fn outcome_to_verdict(outcome: &LoopOutcome, token: &CancellationToken) -> TaskVerdict {
    // If we hit `Cancelled` AND the token was actually fired, attribute
    // it to preemption (so the rollout shows "user cancelled"). If the
    // token wasn't fired, the loop saw `LoopSignal::Cancel` from another
    // source (e.g. the dispatcher's explicit cancel from the safety
    // manager) — surface that distinction.
    match outcome {
        LoopOutcome::Stopped => TaskVerdict::Cancelled {
            reason: Some("stop signal".into()),
        },
        LoopOutcome::Cancelled { .. } => TaskVerdict::Cancelled {
            reason: if token.is_cancelled() {
                Some("preempted by scheduler".into())
            } else {
                Some("loop signal Cancel".into())
            },
        },
        LoopOutcome::MaxIterations => TaskVerdict::BudgetExhausted {
            dimension: "max_iterations".into(),
        },
        LoopOutcome::Failure { error } => TaskVerdict::Failed {
            error_code: "agent_loop_failure".into(),
            message: error.clone(),
        },
        LoopOutcome::NeedApproval { tool_name, .. } => TaskVerdict::Completed {
            summary: Some(format!("awaiting approval for {tool_name}")),
        },
        LoopOutcome::Response { .. } | LoopOutcome::ToolResult { .. } => {
            TaskVerdict::Completed { summary: None }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{
        ChatMessage, ContentBlock, LoopSignal, MessageRole, RespondOutput, ResponseMetadata,
        TextAction, ThreadState, ToolCall,
    };
    use crate::runtime::contracts::{
        AutonomyLevel, BudgetSpec, CheckpointPolicy, OutputContract, PolicySpec, TaskEventSource,
    };
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    fn task_spec(id: &str) -> TaskSpec {
        TaskSpec {
            id: id.into(),
            intent_id: format!("intent-{id}"),
            goal: "test".into(),
            plan_ref: None,
            policy: PolicySpec {
                effective_autonomy: AutonomyLevel::SupervisedTask,
                require_step_approval: false,
                tool_permission_rule_ids: vec![],
            },
            budget: BudgetSpec::default(),
            capability_profile: "default".into(),
            output_contract: OutputContract::FreeText,
            checkpoint_policy: CheckpointPolicy::PerTurn,
        }
    }

    fn make_ctx(force_text_initial: bool) -> Arc<Mutex<ReasoningContext>> {
        let mut ctx = ReasoningContext::new("system prompt".into());
        ctx.force_text = force_text_initial;
        Arc::new(Mutex::new(ctx))
    }

    /// A minimal LoopDelegate that returns a single text response on the
    /// first iteration. Records whether `force_text` was set in the
    /// context at the moment the delegate observed it (i.e. via
    /// `call_llm`). Used to verify the R-1 fix.
    struct ImmediateTextDelegate {
        observed_force_text_on_call: Arc<AtomicBool>,
        call_llm_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LoopDelegate for ImmediateTextDelegate {
        async fn check_signals(&self) -> LoopSignal {
            LoopSignal::Continue
        }
        async fn before_llm_call(
            &self,
            _ctx: &mut ReasoningContext,
            _iter: usize,
        ) -> Option<LoopOutcome> {
            None
        }
        async fn call_llm(
            &self,
            ctx: &mut ReasoningContext,
            _iter: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            self.observed_force_text_on_call
                .store(ctx.force_text, Ordering::SeqCst);
            self.call_llm_count.fetch_add(1, Ordering::SeqCst);
            Ok(RespondOutput::Text {
                text: "hello".into(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata { model: "test-model".into(), finish_reason: Some("stop".into()), usage: None },
            })
        }
        async fn handle_text_response(
            &self,
            _text: &str,
            _meta: ResponseMetadata,
            _ctx: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Response {
                text: "hello".into(),
                usage: None,
                truncated: false,
            })
        }
        async fn execute_tool_calls(
            &self,
            _tcs: Vec<ToolCall>,
            _ctx: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn on_usage(
            &self,
            _usage: &crate::agent::types::TokenUsage,
            _ctx: &ReasoningContext,
        ) {
        }
        async fn on_tool_intent_nudge(
            &self,
            _text: &str,
            _ctx: &mut ReasoningContext,
        ) {
        }
        async fn after_iteration(&self, _iter: usize) {}
    }

    #[tokio::test]
    async fn force_text_is_reset_before_call_llm_observes_it() {
        // Setup: simulate a previous turn that left force_text = true.
        let ctx = make_ctx(/*force_text_initial=*/ true);
        let observed = Arc::new(AtomicBool::new(true));
        let calls = Arc::new(AtomicUsize::new(0));

        let delegate = Arc::new(ImmediateTextDelegate {
            observed_force_text_on_call: observed.clone(),
            call_llm_count: calls.clone(),
        });

        let task = Arc::new(RegularTask::new(
            task_spec("reset-1"),
            RegularTaskInputs {
                delegate,
                reason_ctx: ctx.clone(),
                config: AgenticLoopConfig::default(),
            },
        ));

        let token = CancellationToken::new();
        let events = task.run(token).await;

        // The delegate's call_llm ran once.
        assert_eq!(calls.load(Ordering::SeqCst), 1);

        // R-1 HIGH fix: when call_llm observed the context,
        // force_text was already reset to false.
        assert!(
            !observed.load(Ordering::SeqCst),
            "force_text must be reset before call_llm observes the context"
        );

        // Two events emitted: TaskStarted + TaskFinished{Completed}.
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        match &events[1] {
            TaskEvent::TaskFinished {
                verdict: TaskVerdict::Completed { .. },
                source,
                ..
            } => {
                assert_eq!(*source, TaskEventSource::AgentLoop);
            }
            other => panic!("expected TaskFinished/Completed, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pre_cancelled_token_short_circuits_run() {
        let ctx = make_ctx(false);
        let observed = Arc::new(AtomicBool::new(false));
        let calls = Arc::new(AtomicUsize::new(0));
        let delegate = Arc::new(ImmediateTextDelegate {
            observed_force_text_on_call: observed.clone(),
            call_llm_count: calls.clone(),
        });

        let task = Arc::new(RegularTask::new(
            task_spec("precancel-1"),
            RegularTaskInputs {
                delegate,
                reason_ctx: ctx,
                config: AgenticLoopConfig::default(),
            },
        ));

        let token = CancellationToken::new();
        token.cancel();
        let events = task.run(token).await;

        // call_llm must NOT have been invoked.
        assert_eq!(calls.load(Ordering::SeqCst), 0);

        // We still emit Started + Finished{Cancelled} so the rollout
        // has a complete trace.
        assert_eq!(events.len(), 2);
        match &events[1] {
            TaskEvent::TaskFinished {
                verdict: TaskVerdict::Cancelled { reason },
                ..
            } => {
                assert_eq!(reason.as_deref(), Some("cancelled before run"));
            }
            other => panic!("expected TaskFinished/Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn outcome_to_verdict_max_iterations_is_budget_exhausted() {
        let token = CancellationToken::new();
        let v = outcome_to_verdict(&LoopOutcome::MaxIterations, &token);
        match v {
            TaskVerdict::BudgetExhausted { dimension } => {
                assert_eq!(dimension, "max_iterations");
            }
            other => panic!("expected BudgetExhausted, got {other:?}"),
        }
    }

    #[test]
    fn outcome_to_verdict_failure_carries_error_message() {
        let token = CancellationToken::new();
        let v = outcome_to_verdict(
            &LoopOutcome::Failure {
                error: "boom".into(),
            },
            &token,
        );
        match v {
            TaskVerdict::Failed {
                error_code,
                message,
            } => {
                assert_eq!(error_code, "agent_loop_failure");
                assert_eq!(message, "boom");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn outcome_to_verdict_cancelled_attributes_preemption_when_token_fired() {
        let token = CancellationToken::new();
        token.cancel();
        let v = outcome_to_verdict(&LoopOutcome::Cancelled { partial_code: None }, &token);
        match v {
            TaskVerdict::Cancelled { reason } => {
                assert!(reason.unwrap().contains("preempted"));
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn outcome_to_verdict_cancelled_attributes_signal_when_token_not_fired() {
        let token = CancellationToken::new();
        let v = outcome_to_verdict(&LoopOutcome::Cancelled { partial_code: None }, &token);
        match v {
            TaskVerdict::Cancelled { reason } => {
                assert!(reason.unwrap().contains("loop signal"));
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }
}
