use super::channel::{AgentTeamChannel, ChannelRole};
use super::reviewer::{run_reviewer, ReviewRequest, ReviewVerdict};
use super::runtime_policy::{ReviewGateDecision, ReviewGateState, TeamRuntimePolicy};
use super::supervisor::{supervisor_system_prompt, supervisor_tools};
use super::worker::{run_worker, WorkerResult, WorkerSpec};
use crate::agent::types::{
    AgenticLoopConfig, ChatMessage, ContentBlock, LoopDelegate, MessageRole, RespondOutput,
};
use crate::llm::{CompletionConfig, LlmProvider};
use std::collections::HashMap;
use std::sync::Arc;
use tauri::Emitter;
use tokio::time::{timeout, Duration};

pub struct TeamRunConfig {
    pub team_id: String,
    pub session_id: String,
    pub task: String,
    pub max_review_cycles: u32,
}

pub struct AgentTeamOrchestrator {
    llm: Arc<dyn LlmProvider>,
    model: String,
    app_handle: tauri::AppHandle,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    delegate_factory: Arc<dyn Fn(String) -> Box<dyn LoopDelegate + Send> + Send + Sync>,
}

impl AgentTeamOrchestrator {
    pub fn new(
        llm: Arc<dyn LlmProvider>,
        model: String,
        app_handle: tauri::AppHandle,
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        delegate_factory: impl Fn(String) -> Box<dyn LoopDelegate + Send> + Send + Sync + 'static,
    ) -> Self {
        Self {
            llm,
            model,
            app_handle,
            db,
            delegate_factory: Arc::new(delegate_factory),
        }
    }

    pub async fn run(&self, config: TeamRunConfig) -> String {
        let policy = TeamRuntimePolicy::default();
        let mut review_gate = ReviewGateState::default();
        let channel = Arc::new(AgentTeamChannel::new(
            config.team_id.clone(),
            self.db.clone(),
            self.app_handle.clone(),
        ));

        let _ = self.app_handle.emit(
            "agent:team-started",
            serde_json::json!({
                "teamId": config.team_id,
                "sessionId": config.session_id,
                "task": config.task,
            }),
        );

        let system_prompt = supervisor_system_prompt(&config.task);
        let tools = supervisor_tools();
        let llm_config = CompletionConfig {
            model: self.model.clone(),
            max_tokens: 1024,
            temperature: 0.0,
            thinking_enabled: false,
        };

        let mut messages: Vec<ChatMessage> = vec![
            ChatMessage::system(&system_prompt),
            ChatMessage::user(&config.task),
        ];

        let mut worker_handles: HashMap<String, tokio::task::JoinHandle<WorkerResult>> =
            HashMap::new();
        let mut worker_results: Vec<(String, String)> = vec![];
        let mut review_cycles = 0u32;
        let mut final_result: Option<String> = None;

        // Supervisor loop is policy-bounded so team runs cannot fan out forever.
        for _iter in 0..policy.max_supervisor_turns {
            let response = match self
                .llm
                .complete(messages.clone(), tools.clone(), &llm_config)
                .await
            {
                Ok(r) => r,
                Err(e) => {
                    tracing::error!("Supervisor LLM error: {}", e);
                    break;
                }
            };

            match response {
                RespondOutput::Text { text, .. } => {
                    if let Some(error) = review_gate.completion_error_for_result(Some(&text)) {
                        messages.push(ChatMessage::user(&error));
                        continue;
                    }
                    final_result = Some(text);
                    break;
                }
                RespondOutput::ToolCalls {
                    tool_calls, text, ..
                } => {
                    if tool_calls.is_empty() {
                        if let Some(candidate) = text {
                            if let Some(error) =
                                review_gate.completion_error_for_result(Some(&candidate))
                            {
                                messages.push(ChatMessage::user(&error));
                                continue;
                            }
                            final_result = Some(candidate);
                            break;
                        }
                        messages.push(ChatMessage::user(
                            "No supervisor tool call or final result was provided.",
                        ));
                        continue;
                    }

                    // Build assistant message with all tool call blocks
                    let mut assistant_blocks: Vec<ContentBlock> = Vec::new();
                    if let Some(ref t) = text {
                        if !t.is_empty() {
                            assistant_blocks.push(ContentBlock::Text { text: t.clone() });
                        }
                    }
                    for tc in &tool_calls {
                        assistant_blocks.push(ContentBlock::ToolUse {
                            id: tc.id.clone(),
                            name: tc.name.clone(),
                            input: tc.arguments.clone(),
                        });
                    }
                    messages.push(ChatMessage {
                        role: MessageRole::Assistant,
                        content: assistant_blocks,
                        compacted: false,
                    });

                    // Execute tools and collect results in a single user message
                    let mut tool_result_blocks: Vec<ContentBlock> = Vec::new();
                    for tc in &tool_calls {
                        let (result, is_error) = self
                            .execute_supervisor_tool(
                                &tc.name,
                                &tc.arguments,
                                &channel,
                                &mut worker_handles,
                                &mut worker_results,
                                &config,
                                &mut review_cycles,
                                &mut final_result,
                                &policy,
                                &mut review_gate,
                            )
                            .await;
                        tool_result_blocks.push(ContentBlock::ToolResult {
                            tool_use_id: tc.id.clone(),
                            content: result,
                            is_error: Some(is_error),
                        });
                    }
                    messages.push(ChatMessage {
                        role: MessageRole::User,
                        content: tool_result_blocks,
                        compacted: false,
                    });

                    // Check if complete_task was called
                    if final_result.is_some() {
                        break;
                    }
                }
            }
        }

        // Wait for any still-running workers (with timeout)
        for (_, handle) in worker_handles {
            let _ = timeout(Duration::from_secs(policy.drain_timeout_secs), handle).await;
        }

        let result = match final_result {
            Some(r) => r,
            None => worker_results
                .iter()
                .map(|(r, v)| format!("**{}**\n{}", r, v))
                .collect::<Vec<_>>()
                .join("\n\n"),
        };

        let _ = self.app_handle.emit(
            "agent:team-completed",
            serde_json::json!({
                "teamId": config.team_id,
                "result": result.clone(),
            }),
        );

        result
    }

    async fn execute_supervisor_tool(
        &self,
        tool_name: &str,
        args: &serde_json::Value,
        channel: &Arc<AgentTeamChannel>,
        worker_handles: &mut HashMap<String, tokio::task::JoinHandle<WorkerResult>>,
        worker_results: &mut Vec<(String, String)>,
        config: &TeamRunConfig,
        review_cycles: &mut u32,
        final_result: &mut Option<String>,
        policy: &TeamRuntimePolicy,
        review_gate: &mut ReviewGateState,
    ) -> (String, bool) {
        match tool_name {
            "assign_worker" => {
                let worker_id = args["worker_id"].as_str().unwrap_or("worker").to_string();
                let role = args["role"].as_str().unwrap_or("").to_string();
                let task = args["task"].as_str().unwrap_or("").to_string();

                if worker_handles.contains_key(&worker_id) {
                    return (
                        format!(
                            "Worker ID '{}' is already in use. Choose a different ID.",
                            worker_id
                        ),
                        true,
                    );
                }
                if let Err(violation) = policy.check_worker_fanout(worker_handles.len()) {
                    return (
                        format!(
                            "Worker assignment blocked by team runtime policy: {:?}",
                            violation
                        ),
                        true,
                    );
                }
                review_gate.reset_for_new_work();

                channel.send(
                    ChannelRole::Supervisor,
                    Some(ChannelRole::Worker(worker_id.clone())),
                    format!("Assigned task: {}", task),
                );

                let _ = self.app_handle.emit(
                    "agent:team-worker-started",
                    serde_json::json!({
                        "teamId": config.team_id,
                        "workerId": worker_id,
                        "role": role,
                    }),
                );

                let spec = WorkerSpec {
                    worker_id: worker_id.clone(),
                    role: role.clone(),
                    task,
                };

                let ch = Arc::clone(channel);
                let df = Arc::clone(&self.delegate_factory);
                let mut loop_config = AgenticLoopConfig::from_model(&self.model);
                loop_config.max_iterations = policy.worker_max_iterations;
                // Create the delegate before spawning (the factory captures what it needs)
                let delegate = (df)(format!("Worker role: {}", role));

                let handle =
                    tokio::spawn(async move { run_worker(spec, ch, delegate, loop_config).await });

                worker_handles.insert(worker_id.clone(), handle);
                (
                    format!("Worker {} started with role: {}", worker_id, role),
                    false,
                )
            }

            "read_channel" => {
                let msgs = channel.get_messages();
                let summary = msgs
                    .iter()
                    .map(|m| format!("[{:?}]: {}", m.from_role, m.message))
                    .collect::<Vec<_>>()
                    .join("\n");
                let content = if summary.is_empty() {
                    "No messages yet".to_string()
                } else {
                    summary
                };
                (content, false)
            }

            "request_review" => {
                if *review_cycles >= config.max_review_cycles {
                    review_gate.record_review(ReviewGateDecision::MaxCyclesReached);
                    return (
                        "Maximum review cycles reached. Reviewer approval is still required before complete_task."
                            .to_string(),
                        false,
                    );
                }

                // Await all pending workers (with timeout)
                let pending_ids: Vec<String> = worker_handles.keys().cloned().collect();
                for wid in pending_ids {
                    if let Some(handle) = worker_handles.remove(&wid) {
                        match timeout(
                            Duration::from_secs(policy.worker_await_timeout_secs),
                            handle,
                        )
                        .await
                        {
                            Ok(Ok(result)) => {
                                worker_results.push((result.worker_id.clone(), result.result))
                            }
                            Ok(Err(_)) => worker_results.push((wid, "Worker panicked".to_string())),
                            Err(_) => worker_results.push((wid, "Worker timed out".to_string())),
                        }
                    }
                }

                let req = ReviewRequest {
                    original_task: config.task.clone(),
                    supervisor_plan: args["combined_result"].as_str().unwrap_or("").to_string(),
                    worker_results: worker_results.clone(),
                };
                let reviewed_result = req.supervisor_plan.clone();

                let verdict =
                    run_reviewer(req, Arc::clone(&self.llm), &self.model, Arc::clone(channel))
                        .await;
                *review_cycles += 1;
                match &verdict {
                    ReviewVerdict::Pass => review_gate
                        .record_reviewed_result(ReviewGateDecision::Pass, &reviewed_result),
                    ReviewVerdict::Revise(feedback) => {
                        review_gate.record_review(ReviewGateDecision::Revise {
                            feedback: feedback.clone(),
                        });
                    }
                    ReviewVerdict::Fail(reason) => {
                        review_gate.record_review(ReviewGateDecision::Fail {
                            reason: reason.clone(),
                        });
                    }
                }

                let content = match verdict {
                    ReviewVerdict::Pass => {
                        "Reviewer approved. Proceed to complete_task.".to_string()
                    }
                    ReviewVerdict::Revise(feedback) => {
                        format!("Reviewer says revise: {}", feedback)
                    }
                    ReviewVerdict::Fail(reason) => {
                        format!(
                            "Reviewer says fail: {}. Revise the work before calling complete_task.",
                            reason
                        )
                    }
                };
                (content, false)
            }

            "complete_task" => {
                let result = args["result"].as_str().unwrap_or("").to_string();
                if let Some(error) = review_gate.completion_error_for_result(Some(&result)) {
                    return (error, true);
                }
                *final_result = Some(result.clone());
                (result, false)
            }

            _ => (format!("Unknown supervisor tool: {}", tool_name), true),
        }
    }
}
