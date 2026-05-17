//! HeadlessDelegate — a headless [`LoopDelegate`] for automation runs and IM
//! close-loop replies.
//!
//! Extracted from `automation/runtime/execute.rs` so that both the automation
//! runtime and the IM gateway can share one delegate implementation.
//!
//! # IM close-loop extension fields
//!
//! Three optional fields extend the base automation behaviour:
//!
//! - `reply_handle` — when set, `notify_user` sends the notification text
//!   directly to the originating IM chat before (and instead of) the legacy
//!   `ChannelManager` dispatch.
//! - `streaming_handle` — when set, `handle_text_response` forwards each
//!   partial LLM text chunk to the IM channel as a streaming update and
//!   returns `TextAction::Return` to short-circuit the loop.
//! - `system_prompt_override` — optional system prompt injected at
//!   construction time; the automation runtime ignores it (the spec's own
//!   prompt is used), but the IM gateway can pass a custom one.

use crate::agent::types::{
    ChatMessage, LoopOutcome, LoopSignal, RespondOutput, ReasoningContext, ResponseMetadata,
    TextAction, ToolCall,
};
use crate::automation::memory::MemoryStore;
use crate::automation::permissions;
use crate::automation::runtime::{AutoContinueConfig, CompletionGate, PermissionSet};
use crate::automation::tools::{
    memory::MemoryInput,
    notify_user::NotifyInput,
    report_to_user::ReportInput,
    request_escalation::RequestEscalationInput,
};
use crate::agent::tools::tool::ToolRegistry;
use crate::automation::runtime::cost::CostCapState;
use crate::channels::{ChannelManager, ChannelNotification};
use crate::error::Error;
use crate::llm::LlmProvider;
use async_trait::async_trait;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Emitter;
use tokio::sync::{Mutex, RwLock};

/// LoopDelegate for headless runs (automation and IM close-loop).
///
/// Drives `run_agentic_loop` with no Tauri AppHandle for interactive UI:
/// streaming uses a `NoopSink`, the four Humane tools + the full base tool
/// set are dispatched here, and cost is bounded by a per-run cap. Terminal
/// state lands in `gate`; the transcript is persisted to `agent_messages`
/// under `session_id`.
///
/// See the module doc for the three IM extension fields.
pub struct HeadlessDelegate {
    pub spec_id: String,
    pub activity_id: String,
    /// The run's agent_session id — transcript rows are persisted under this.
    pub session_id: String,
    pub permissions: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Holds the terminal state once the run completes (report or escalation).
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
    pub auto_continue: AutoContinueConfig,
    /// LLM provider resolved from the app's ProviderService.
    pub llm: Arc<dyn LlmProvider>,
    /// Model id for this run.
    pub model: String,
    /// Full base tool set + the four Humane tool schemas.
    pub tools: Arc<ToolRegistry>,
    /// Per-run cost accumulator + cap.
    pub cost: Arc<CostCapState>,
    /// Working directory the run operates in (file/edit/search base + shell cwd).
    pub workspace_root: PathBuf,
    /// IPC handle for `"system"` channel notifications. None in unit tests.
    pub app_handle: Option<tauri::AppHandle>,
    /// External channel manager for `"wecom"`, `"email"`, etc. None in unit tests.
    pub channel_manager: Option<Arc<RwLock<ChannelManager>>>,

    // ── IM close-loop extension fields ────────────────────────────────────────

    /// When set, `notify_user` sends the notification body directly to the
    /// originating IM chat via this handle before the legacy channel dispatch.
    pub reply_handle: Option<Arc<crate::channels::types::ReplyHandle>>,

    /// When set, `handle_text_response` forwards partial LLM text as a
    /// streaming update and returns `TextAction::Return` to finish the loop.
    pub streaming_handle: Option<Arc<dyn crate::channels::types::StreamingHandle>>,

    /// Optional system prompt override (used by the IM gateway; ignored when
    /// the automation runtime supplies its own prompt via `ReasoningContext`).
    pub system_prompt_override: Option<String>,
}

/// Maps a `notify_user` channel name to the corresponding `ChannelType`.
/// Unknown names return `None` and are silently skipped.
fn channel_type_for_name(name: &str) -> Option<crate::channels::ChannelType> {
    use crate::channels::ChannelType;
    match name {
        "wecom"   => Some(ChannelType::WeChat),
        "email"   => Some(ChannelType::Email),
        "webhook" => Some(ChannelType::Webhook),
        _         => None,
    }
}

#[async_trait]
impl crate::agent::types::LoopDelegate for HeadlessDelegate {
    async fn check_signals(&self) -> LoopSignal {
        LoopSignal::Continue
    }

    async fn before_llm_call(
        &self,
        _reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Option<LoopOutcome> {
        // Per-run cost cap: if the accumulated cost already crossed the cap
        // (from a prior iteration's on_usage), abort before spending more.
        if self.cost.per_run_exceeded() {
            let msg = format!(
                "per-run cost cap exceeded (${:.4})",
                self.cost.total_usd()
            );
            tracing::warn!(spec_id = %self.spec_id, activity_id = %self.activity_id, "{}", msg);
            *self.gate.lock().await = Some(CompletionGate::ErrorTerminal(msg.clone()));
            return Some(LoopOutcome::Failure { error: msg });
        }
        None
    }

    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        let mut messages = vec![ChatMessage::system(&reason_ctx.system_prompt)];
        messages.extend(reason_ctx.messages.clone());

        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            self.tools.list_definitions()
        };
        let config = crate::llm::CompletionConfig {
            model: self.model.clone(),
            max_tokens: 8192,
            temperature: 0.7,
            thinking_enabled: false,
        };

        tracing::info!(
            spec_id = %self.spec_id,
            model = %self.model,
            message_count = messages.len(),
            tool_count = tools.len(),
            force_text = reason_ctx.force_text,
            "headless run: calling LLM"
        );

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            &crate::agent::llm_stream::NoopSink,
        )
        .await
    }

    async fn on_usage(
        &self,
        usage: &crate::agent::types::TokenUsage,
        _reason_ctx: &ReasoningContext,
    ) {
        let cost = crate::agent::types::calculate_cost(
            &self.model,
            usage.input_tokens,
            usage.output_tokens,
        );
        let total = self.cost.add(cost);
        tracing::debug!(
            spec_id = %self.spec_id,
            turn_cost_usd = cost,
            total_cost_usd = total,
            "headless run: cost accumulated"
        );
    }

    async fn handle_text_response(
        &self,
        text: &str,
        _metadata: ResponseMetadata,
        _reason_ctx: &mut ReasoningContext,
    ) -> TextAction {
        // IM close-loop: streaming path — forward partial text and exit loop.
        if let Some(sh) = &self.streaming_handle {
            if let Err(e) = sh.update(text).await {
                tracing::warn!(
                    spec_id = %self.spec_id,
                    "headless run: streaming_handle update error: {}", e
                );
            }
            if let Err(e) = sh.finish(text).await {
                tracing::warn!(
                    spec_id = %self.spec_id,
                    "headless run: streaming_handle finish error: {}", e
                );
            }
            return TextAction::Return(crate::agent::types::LoopOutcome::Response {
                text: text.to_string(),
                usage: None,
                truncated: false,
            });
        }
        // IM close-loop: non-streaming reply path — text response is also terminal.
        // Without this early exit the loop would continue until max_iterations since
        // TextAction::Continue re-invokes the LLM on every turn.
        if self.reply_handle.is_some() {
            return TextAction::Return(crate::agent::types::LoopOutcome::Response {
                text: text.to_string(),
                usage: None,
                truncated: false,
            });
        }
        TextAction::Continue
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<ToolCall>,
        reason_ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, Error> {
        for call in tool_calls {
            // Permission gate: deny-list beats grant-list beats spec default.
            if let Err(e) = permissions::check(
                &self.permissions.spec,
                &self.permissions.granted,
                &self.permissions.denied,
                &call.name,
            ) {
                reason_ctx.messages.push(ChatMessage::user_tool_result(
                    &call.id,
                    &format!("permission error: {}", e),
                    true,
                ));
                continue;
            }

            match call.name.as_str() {
                "report_to_user" => {
                    let input: ReportInput = serde_json::from_value(call.arguments.clone())?;
                    let artifacts_json = serde_json::to_string(&input.artifacts)
                        .unwrap_or_else(|_| "[]".into());
                    {
                        let conn = self.db.lock().unwrap();
                        conn.execute(
                            "UPDATE automation_activities \
                             SET status='completed', report_text=?1, report_outcome=?2, \
                                 report_artifacts_json=?3, completed_at=?4 \
                             WHERE id=?5",
                            rusqlite::params![
                                input.text,
                                input.outcome,
                                artifacts_json,
                                chrono::Utc::now().timestamp_millis(),
                                self.activity_id,
                            ],
                        )?;
                    }
                    *self.gate.lock().await = Some(CompletionGate::Reported {
                        text: input.text.clone(),
                        outcome: input.outcome.clone(),
                    });
                    tracing::info!(
                        spec_id = %self.spec_id,
                        activity_id = %self.activity_id,
                        outcome = %input.outcome,
                        artifact_count = input.artifacts.len(),
                        "headless run reported"
                    );
                    // Push the tool_result so the terminal tool_use is a
                    // balanced exchange — without it the run-session view
                    // renders report_to_user as a perpetually-pending call.
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id, &input.text, false,
                    ));
                    return Ok(Some(LoopOutcome::Response {
                        text: input.text,
                        usage: None,
                        truncated: false,
                    }));
                }

                "request_escalation" => {
                    let input: RequestEscalationInput =
                        serde_json::from_value(call.arguments.clone())?;
                    // Serialize choices by pulling the raw JSON array from the
                    // original arguments — avoids requiring Serialize on EscalationChoice.
                    let choices_json = call
                        .arguments
                        .get("choices")
                        .and_then(|v| serde_json::to_string(v).ok())
                        .unwrap_or_else(|| "[]".into());
                    let escalation_id = uuid::Uuid::new_v4().to_string();
                    {
                        let conn = self.db.lock().unwrap();
                        conn.execute(
                            "INSERT INTO automation_escalations \
                             (id, spec_id, activity_id, question, choices_json, status, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, 'waiting', ?6)",
                            rusqlite::params![
                                escalation_id,
                                self.spec_id,
                                self.activity_id,
                                input.question,
                                choices_json,
                                chrono::Utc::now().timestamp_millis(),
                            ],
                        )?;
                    }
                    *self.gate.lock().await = Some(CompletionGate::Escalated {
                        escalation_id: escalation_id.clone(),
                    });
                    // TODO(humane-phase-2): emit InfraEvent::AutomationRunEscalated
                    tracing::info!(
                        spec_id = %self.spec_id,
                        activity_id = %self.activity_id,
                        escalation_id = %escalation_id,
                        "headless run escalation requested"
                    );
                    // Balance the terminal tool_use with a tool_result (see
                    // the report_to_user arm).
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id, "escalated", false,
                    ));
                    return Ok(Some(LoopOutcome::Response {
                        text: "escalated".into(),
                        usage: None,
                        truncated: false,
                    }));
                }

                "memory" => {
                    let input: MemoryInput = serde_json::from_value(call.arguments.clone())?;
                    let result = match input.op.as_str() {
                        "read" => self.memory.read(&self.spec_id).await?,
                        "write" => {
                            let c = input.content.as_deref().unwrap_or("");
                            self.memory.write(&self.spec_id, c).await?;
                            "ok".into()
                        }
                        "append" => {
                            let c = input.content.as_deref().unwrap_or("");
                            self.memory.append(&self.spec_id, c).await?;
                            "ok".into()
                        }
                        "compact" => {
                            let p = self.memory.compact(&self.spec_id).await?;
                            let path_str = p.to_string_lossy().into_owned();
                            {
                                let conn = self.db.lock().unwrap();
                                let _ = crate::automation::memory::record_compaction(
                                    &conn, &self.spec_id, &path_str,
                                );
                            }
                            path_str
                        }
                        _ => "unknown memory op".into(),
                    };
                    reason_ctx
                        .messages
                        .push(ChatMessage::user_tool_result(&call.id, &result, false));
                }

                "notify_user" => {
                    let input: NotifyInput = serde_json::from_value(call.arguments.clone())?;

                    // IM close-loop: if a reply handle is attached, send the
                    // notification body directly to the originating IM chat
                    // first, then fall through to the legacy channel dispatch
                    // so other channels also receive it.
                    if let Some(reply) = &self.reply_handle {
                        let report_text = format!(
                            "[{}] {}: {}",
                            input.level, input.title, input.body
                        );
                        if let Err(e) = reply.send(&report_text).await {
                            tracing::warn!(
                                spec_id = %self.spec_id,
                                "notify_user: reply_handle send error: {}", e
                            );
                        }
                        reason_ctx.messages.push(ChatMessage::user_tool_result(
                            &call.id,
                            "notification dispatched",
                            false,
                        ));
                        continue;
                    }

                    let notification = ChannelNotification {
                        title: input.title.clone(),
                        body:  input.body.clone(),
                        level: input.level.clone(),
                        metadata: None,
                    };

                    for ch in &input.channels {
                        match ch.as_str() {
                            "system" => {
                                if let Some(handle) = &self.app_handle {
                                    let _ = handle.emit("automation_notify", &notification);
                                }
                            }
                            other => {
                                if let (Some(ct), Some(cm_lock)) =
                                    (channel_type_for_name(other), &self.channel_manager)
                                {
                                    let cm = cm_lock.read().await;
                                    let results = cm.send_to_type(&ct, &notification).await;
                                    for (ch_id, res) in results {
                                        if let Err(e) = res {
                                            tracing::warn!(
                                                spec_id = %self.spec_id,
                                                ch_id = %ch_id,
                                                "notify_user channel error: {}", e
                                            );
                                        }
                                    }
                                }
                            }
                        }
                    }

                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id,
                        "notification dispatched",
                        false,
                    ));
                }

                other => {
                    // Dispatch to the base built-in tool set via ToolRegistry.
                    // Permission was already checked at the top of the loop.
                    match self.tools.get(other) {
                        Some(tool) => {
                            match tool.execute(call.arguments.clone()).await {
                                Ok(output) => {
                                    let result = serde_json::to_string(&output.result)
                                        .unwrap_or_else(|_| "{}".into());
                                    let is_err = crate::agent::dispatcher::detect_soft_tool_error(&output.result);
                                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                                        &call.id, &result, is_err,
                                    ));
                                }
                                Err(e) => {
                                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                                        &call.id,
                                        &format!("tool '{}' error: {}", other, e),
                                        true,
                                    ));
                                }
                            }
                        }
                        None => {
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &call.id,
                                &format!("tool '{}' not found in registry", other),
                                true,
                            ));
                        }
                    }
                }
            }
        }
        Ok(None)
    }
}
