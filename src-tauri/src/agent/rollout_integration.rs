//! M1-T4b — bridge `run_agentic_loop` ↔ `runtime::rollout::RolloutHandle`.
//!
//! Production callsites that already use `run_agentic_loop` directly
//! (`tauri_commands.rs`, `symphony_graph/runtime/node_run.rs`) opt in to the
//! rollout stream via [`run_with_rollout`], gated by the `UCLAW_ROLLOUT_ENABLED`
//! env var (defaults to disabled while M1-T4b/c stabilize).
//!
//! The helper:
//!
//! 1. Optionally emits a `TaskStarted` event before the loop.
//! 2. Resets `reason_ctx.force_text = false` (same R-1 HIGH fix that
//!    `RegularTask::run` applies — see
//!    `docs/superpowers/specs/2026-05-20-agentic-loop-state-audit.md §B.3`).
//! 3. Drives `run_agentic_loop`.
//! 4. Emits derived intermediate events (`ModelTurn` on `Response` with usage,
//!    `Warning` on `Failure`) — same shape as M1-T4a's RegularTask wiring.
//! 5. Emits `TaskFinished{verdict}` mapped via `regular_task::outcome_to_verdict`.
//! 6. Returns the original `LoopOutcome` unchanged so existing format-the-
//!    response-text code paths continue to work.
//!
//! When `rollout` is `None` the helper degrades to a thin wrapper that still
//! applies the R-1 fix but doesn't emit events — useful for callers that
//! want the consistency of the wrapper without the I/O cost.

use crate::agent::agentic_loop::run_agentic_loop;
use crate::agent::regular_task::outcome_to_verdict;
use crate::agent::types::{AgenticLoopConfig, LoopDelegate, LoopOutcome, ReasoningContext};
use crate::runtime::contracts::{TaskEvent, TaskEventSource, TokenUsage};
use crate::runtime::rollout::RolloutHandle;
use tokio_util::sync::CancellationToken;

/// Whether the current process has opted into rollout writing via the
/// `UCLAW_ROLLOUT_ENABLED` env var. Reads once per call — cheap, no caching.
pub fn rollout_enabled_by_env() -> bool {
    matches!(
        std::env::var("UCLAW_ROLLOUT_ENABLED").as_deref(),
        Ok("1") | Ok("true") | Ok("TRUE")
    )
}

/// Run `run_agentic_loop` with rollout emission.
///
/// `task_id` and `intent_id` identify the task in the rollout. Use any stable
/// string — agent_session id + a UUID is a typical choice.
///
/// Returns the underlying `LoopOutcome` unchanged.
pub async fn run_with_rollout(
    delegate: &dyn LoopDelegate,
    reason_ctx: &mut ReasoningContext,
    config: &AgenticLoopConfig,
    rollout: Option<&RolloutHandle>,
    task_id: &str,
    intent_id: &str,
) -> LoopOutcome {
    let now = || chrono::Utc::now().to_rfc3339();

    if let Some(handle) = rollout {
        handle.emit(TaskEvent::TaskStarted {
            ts: now(),
            source: TaskEventSource::AgentLoop,
            task_id: task_id.to_string(),
            intent_id: intent_id.to_string(),
        });
    }

    // R-1 HIGH fix — same reset RegularTask::run performs.
    reason_ctx.force_text = false;

    let outcome = run_agentic_loop(delegate, reason_ctx, config).await;

    if let Some(handle) = rollout {
        match &outcome {
            LoopOutcome::Response {
                usage: Some(usage), ..
            } => {
                handle.emit(TaskEvent::ModelTurn {
                    ts: now(),
                    source: TaskEventSource::AgentLoop,
                    task_id: task_id.to_string(),
                    provider: "agent_loop".into(),
                    model: "aggregated".into(),
                    token_usage: TokenUsage {
                        input_tokens: usage.input_tokens,
                        cached_input_tokens: usage.cache_read_tokens,
                        output_tokens: usage.output_tokens,
                        // M1-T6 — real reasoning tokens from the provider if reported.
                        reasoning_output_tokens: usage.reasoning_output_tokens,
                        total_tokens: usage
                            .input_tokens
                            .saturating_add(usage.output_tokens)
                            .saturating_add(usage.reasoning_output_tokens),
                        cost_usd_micros: None,
                    },
                });
            }
            LoopOutcome::Failure { error } => {
                handle.emit(TaskEvent::Warning {
                    ts: now(),
                    source: TaskEventSource::AgentLoop,
                    task_id: task_id.to_string(),
                    code: "agent_loop_failure".into(),
                    message: error.clone(),
                });
            }
            _ => {}
        }
        // Rollout-bridge callers don't have a token; the loop reports
        // Cancelled via LoopSignal::Cancel or LoopOutcome::Cancelled itself.
        let token = CancellationToken::new();
        let verdict = outcome_to_verdict(&outcome, &token);
        handle.emit(TaskEvent::TaskFinished {
            ts: now(),
            source: TaskEventSource::AgentLoop,
            task_id: task_id.to_string(),
            verdict,
        });
    }

    outcome
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{
        ChatMessage, ContentBlock, LoopSignal, MessageRole, RespondOutput, ResponseMetadata,
        TextAction, ThreadState, ToolCall,
    };
    use crate::runtime::rollout::RolloutWriter;
    use async_trait::async_trait;
    use std::sync::Arc;
    use tempfile::tempdir;

    /// LoopDelegate that returns a Response with usage on the first turn.
    struct OneShotResponseDelegate;

    #[async_trait]
    impl LoopDelegate for OneShotResponseDelegate {
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
            _ctx: &mut ReasoningContext,
            _iter: usize,
        ) -> Result<RespondOutput, crate::error::Error> {
            Ok(RespondOutput::Text {
                text: "ok".into(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata {
                    model: "test-model".into(),
                    finish_reason: Some("stop".into()),
                    usage: Some(crate::agent::types::TokenUsage {
                        input_tokens: 50,
                        output_tokens: 10,
                        cache_read_tokens: 20,
                        cache_creation_tokens: 0,
                        reasoning_output_tokens: 0,
                    }),
                },
            })
        }
        async fn handle_text_response(
            &self,
            _t: &str,
            meta: ResponseMetadata,
            _c: &mut ReasoningContext,
        ) -> TextAction {
            TextAction::Return(LoopOutcome::Response {
                text: "ok".into(),
                usage: meta.usage,
                truncated: false,
            })
        }
        async fn execute_tool_calls(
            &self,
            _: Vec<ToolCall>,
            _: &mut ReasoningContext,
        ) -> Result<Option<LoopOutcome>, crate::error::Error> {
            Ok(None)
        }
        async fn on_usage(
            &self,
            _: &crate::agent::types::TokenUsage,
            _: &ReasoningContext,
        ) {
        }
        async fn on_tool_intent_nudge(&self, _: &str, _: &mut ReasoningContext) {}
        async fn after_iteration(&self, _: usize) {}
    }

    #[tokio::test]
    async fn run_with_rollout_emits_started_modelturn_finished() {
        let dir = tempdir().unwrap();
        let handle = RolloutWriter::spawn(dir.path().to_path_buf(), None)
            .await
            .unwrap();

        let mut ctx = ReasoningContext::new("system".into());
        ctx.force_text = true; // verify R-1 fix runs
        let config = AgenticLoopConfig::default();
        let delegate = OneShotResponseDelegate;

        let outcome = run_with_rollout(
            &delegate,
            &mut ctx,
            &config,
            Some(&handle),
            "task-roundtrip",
            "intent-roundtrip",
        )
        .await;

        // R-1: ctx.force_text was reset before the loop ran.
        assert!(!ctx.force_text);

        // Outcome unchanged (the helper returns the loop's outcome verbatim).
        assert!(matches!(outcome, LoopOutcome::Response { .. }));

        // Drop handle to flush + close the channel.
        let file = handle.rollout_file().to_path_buf();
        drop(handle);
        tokio::time::sleep(std::time::Duration::from_millis(150)).await;

        let body = tokio::fs::read_to_string(&file).await.unwrap();
        let lines: Vec<&str> = body.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(
            lines.len(),
            3,
            "expected Started + ModelTurn + Finished, got {lines:?}"
        );
        let kinds: Vec<String> = lines
            .iter()
            .map(|l| {
                let v: serde_json::Value = serde_json::from_str(l).unwrap();
                v["event"]["kind"].as_str().unwrap().to_string()
            })
            .collect();
        assert_eq!(
            kinds,
            vec!["task_started", "model_turn", "task_finished"]
        );
    }

    #[tokio::test]
    async fn run_with_rollout_none_still_applies_r1_fix() {
        // Even without a rollout handle, the helper resets force_text.
        let mut ctx = ReasoningContext::new("system".into());
        ctx.force_text = true;
        let config = AgenticLoopConfig::default();
        let delegate = OneShotResponseDelegate;

        let _outcome = run_with_rollout(
            &delegate,
            &mut ctx,
            &config,
            None,
            "task-no-rollout",
            "intent-no-rollout",
        )
        .await;

        assert!(!ctx.force_text);
    }

    #[test]
    fn rollout_enabled_by_env_recognizes_truthy_values() {
        // SAFETY: env mutation is process-global; this test owns the var.
        unsafe {
            std::env::remove_var("UCLAW_ROLLOUT_ENABLED");
        }
        assert!(!rollout_enabled_by_env());

        unsafe {
            std::env::set_var("UCLAW_ROLLOUT_ENABLED", "1");
        }
        assert!(rollout_enabled_by_env());

        unsafe {
            std::env::set_var("UCLAW_ROLLOUT_ENABLED", "true");
        }
        assert!(rollout_enabled_by_env());

        unsafe {
            std::env::set_var("UCLAW_ROLLOUT_ENABLED", "TRUE");
        }
        assert!(rollout_enabled_by_env());

        unsafe {
            std::env::set_var("UCLAW_ROLLOUT_ENABLED", "yes");
        }
        // Strict — "yes" not recognized.
        assert!(!rollout_enabled_by_env());

        unsafe {
            std::env::remove_var("UCLAW_ROLLOUT_ENABLED");
        }
    }
}
