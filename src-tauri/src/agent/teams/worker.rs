use std::sync::Arc;
use crate::agent::types::{LoopOutcome, ReasoningContext, ChatMessage, AgenticLoopConfig, LoopDelegate};
use crate::agent::agentic_loop::run_agentic_loop;
use super::channel::{AgentTeamChannel, ChannelRole};

pub struct WorkerSpec {
    pub worker_id: String,
    pub role: String,
    pub task: String,
}

pub struct WorkerResult {
    pub worker_id: String,
    pub success: bool,
    pub result: String,
}

/// Truncate a string to at most `max_bytes` bytes at a valid UTF-8 boundary.
fn truncate_utf8(s: &str, max_bytes: usize) -> &str {
    if s.len() <= max_bytes {
        return s;
    }
    let mut end = max_bytes;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    &s[..end]
}

/// Run a single worker as an isolated agentic loop.
pub async fn run_worker(
    spec: WorkerSpec,
    channel: Arc<AgentTeamChannel>,
    delegate: Box<dyn LoopDelegate + Send>,
    config: AgenticLoopConfig,
) -> WorkerResult {
    channel.send(
        ChannelRole::Worker(spec.worker_id.clone()),
        Some(ChannelRole::Supervisor),
        format!("Starting: {}", spec.task),
    );

    let system_prompt = format!(
        "You are a specialized worker with role: {}.\n\nYour task: {}\n\nComplete this task and provide a detailed result. When done, summarize your findings clearly.",
        spec.role,
        spec.task,
    );

    let mut ctx = ReasoningContext::new(system_prompt.clone());
    ctx.messages.push(ChatMessage::user(&spec.task));

    let outcome = run_agentic_loop(delegate.as_ref(), &mut ctx, &config).await;

    let (success, result) = match outcome {
        LoopOutcome::Response { text, .. } => (true, text),
        LoopOutcome::Stopped => (false, "Worker stopped".to_string()),
        LoopOutcome::Cancelled { .. } => (false, "Worker stopped".to_string()),
        LoopOutcome::MaxIterations => (false, "Worker reached max iterations".to_string()),
        LoopOutcome::NeedApproval { tool_name, .. } => (false, format!("Worker needs approval for tool: {}", tool_name)),
        LoopOutcome::Failure { error } => (false, error),
        _ => (false, "Unexpected outcome".to_string()),
    };

    channel.send(
        ChannelRole::Worker(spec.worker_id.clone()),
        Some(ChannelRole::Supervisor),
        if success {
            format!("Done: {}", truncate_utf8(&result, 200))
        } else {
            format!("Failed: {}", truncate_utf8(&result, 200))
        },
    );

    WorkerResult {
        worker_id: spec.worker_id,
        success,
        result,
    }
}
