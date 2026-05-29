//! LLM streaming + cancellation-aware sleep for ChatDelegate.
//!
//! `impl StreamSink` receives delta callbacks from the LLM stream and
//! forwards them to the observability emit_* methods (text/thinking/done).
//! The inherent `sleep_or_abort` is the cancellation-aware sleep that ALL
//! loop pauses go through (interruption-friendly). In P3-5b this file
//! will also hold `call_llm` once we untangle it from LoopDelegate.

use async_trait::async_trait;

use super::ChatDelegate;
use crate::agent::llm_stream::StreamSink;
use crate::agent::retry::AgentRetryEvent;

impl ChatDelegate {
    /// Sleep for `duration`, but wake up early if the session's stop flag
    /// flips. Returns `true` if the wake was triggered by the stop flag
    /// (caller should bail), `false` if the full duration elapsed.
    pub(super) async fn sleep_or_abort(&self, duration: std::time::Duration) -> bool {
        let stop = self.stop_flag.clone();
        tokio::select! {
            _ = tokio::time::sleep(duration) => false,
            _ = async {
                loop {
                    if stop.load(std::sync::atomic::Ordering::Relaxed) {
                        break;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                }
            } => true,
        }
    }
}

#[async_trait]
impl StreamSink for ChatDelegate {
    fn on_text_delta(&self, text: &str) {
        self.emit_text_delta(text);
    }
    fn on_thinking(&self, thinking: &str) {
        self.emit_thinking(thinking);
    }
    fn on_thinking_done(&self, duration_ms: u64) {
        self.emit_thinking_done(duration_ms);
    }
    fn on_stream_reset(&self) {
        self.emit_stream_reset();
    }
    fn on_retry_event(&self, event: AgentRetryEvent) {
        self.emit_retry_event(event);
    }
    async fn sleep_or_abort(&self, delay: std::time::Duration) -> bool {
        ChatDelegate::sleep_or_abort(self, delay).await
    }
}

#[cfg(test)]
mod panic_recovery_tests {
    use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
    use async_trait::async_trait;

    struct PanickyTool;

    #[async_trait]
    impl Tool for PanickyTool {
        fn name(&self) -> &str { "panicky" }
        fn description(&self) -> &str { "test-only" }
        fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
        fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
            ApprovalRequirement::Never
        }
        async fn execute(&self, _: serde_json::Value) -> Result<ToolOutput, ToolError> {
            panic!("deliberate test panic");
        }
    }

    /// Verify the panic-recovery shape: tokio::task::spawn catches panic,
    /// JoinHandle yields is_panic=true, we map that to a ToolError.
    /// This mirrors what dispatcher::execute_tool does.
    #[tokio::test]
    async fn tool_panic_converts_to_tool_error() {
        let tool = PanickyTool;
        let tool_name = tool.name().to_string();
        let join = tokio::task::spawn(async move {
            tool.execute(serde_json::json!({})).await
        });
        let result = match join.await {
            Ok(r) => r,
            Err(e) if e.is_panic() => Err(ToolError::Execution(format!(
                "Tool '{}' crashed unexpectedly.", tool_name
            ))),
            Err(e) => Err(ToolError::Execution(format!("Join error: {}", e))),
        };
        assert!(result.is_err());
        let msg = format!("{}", result.unwrap_err());
        assert!(
            msg.contains("panicky") && msg.contains("crashed"),
            "expected panic-recovery error, got: {}", msg
        );
    }
}

#[cfg(test)]
mod truncated_continuation_tests {
    use super::super::signals_truncated_plan_continuation;

    // Large-output + tiny-text is the shape of "thinking-heavy LLM
    // produced a transition stub but forgot the tool_use block". Triggers
    // the plan guard as a final fallback when the keyword gate misses.

    #[test]
    fn gomoku_signal_passes() {
        // The actual production case: 14 chars of text, 1722 output tokens.
        assert!(signals_truncated_plan_continuation(14, 1722));
    }

    #[test]
    fn long_text_does_not_pass() {
        // A real long answer can have many output tokens — we must not
        // hijack it. Threshold is text_len < 100.
        assert!(!signals_truncated_plan_continuation(300, 1722));
        assert!(!signals_truncated_plan_continuation(100, 1722));
    }

    #[test]
    fn small_output_does_not_pass() {
        // Tiny output (a normal short reply, no thinking) → not suspicious.
        assert!(!signals_truncated_plan_continuation(14, 50));
        assert!(!signals_truncated_plan_continuation(14, 800));
    }

    #[test]
    fn boundary_around_thresholds() {
        // > 800 tokens AND < 100 chars.
        assert!(signals_truncated_plan_continuation(99, 801));
        assert!(!signals_truncated_plan_continuation(99, 800));
        assert!(!signals_truncated_plan_continuation(100, 801));
    }
}
