//! M1-T4c — bridge `BrowserTaskRun` → `Vec<TaskEvent>` for the rollout
//! writer. Conversion-only; the actual wiring (calling this helper from
//! `agent_loop::AgentLoop::run`) is deferred to M1-T4d so the diff stays
//! reviewable.
//!
//! Mapping rules (one-pass over the run's steps):
//!
//! - `BrowserTaskStatus::Running` (in-progress run) — caller usually
//!   skips conversion until the run terminates, but if asked, we emit
//!   `TaskStarted` + per-step `ToolCall`/`ToolResult` pairs and NO
//!   terminator. The owner is expected to add a terminator later.
//! - Each step → one `ToolCall` (with `tool_name = step.action_name`,
//!   `input_ref = JSON of step.action_args`) followed by one
//!   `ToolResult` (`output_ref = step.message or step.error or ""`,
//!   `ok = step.ok`).
//! - `UserIntervention` phase → an extra `PermissionRequested` /
//!   `PermissionDecided` pair so the rollout shows the human boundary
//!   explicitly.
//! - Run status → terminal `TaskFinished` verdict or resumable yield:
//!     Completed              → `TaskVerdict::Completed`
//!     Failed                 → `TaskVerdict::Failed { error_code: "browser_failed", message }`
//!     Stopped                → `TaskVerdict::Cancelled { reason: Some("stopped") }`
//!     NeedsUserIntervention  → `BoundaryYield`, no terminator
//!     PausedWaitingForBrowserRuntime → `Checkpoint` + `BoundaryYield`, no terminator
//!     PausedCheckpointed     → `Checkpoint` + `BoundaryYield`, no terminator
//!     Running                → no terminator (caller appends later)

use crate::browser::provider::BrowserProviderRouteDecision;
use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus, BrowserTaskStepPhase};
use crate::runtime::contracts::{PermissionDecision, TaskEvent, TaskEventSource, TaskVerdict};

/// Convert a finished or in-progress browser run into a sequence of
/// `TaskEvent`s ready for the rollout writer.
///
/// `intent_id` should typically be the upstream chat session id (the
/// browser task was started in response to a user chat); if no parent
/// exists, the caller can pass `run.session_id`.
pub fn browser_run_to_events(run: &BrowserTaskRun, intent_id: &str) -> Vec<TaskEvent> {
    let now = || chrono::Utc::now().to_rfc3339();
    let task_id = run.run_id.clone();
    let src = TaskEventSource::Browser;
    let mut out: Vec<TaskEvent> = Vec::with_capacity(2 + run.steps.len() * 2);

    out.push(TaskEvent::TaskStarted {
        ts: now(),
        source: src,
        task_id: task_id.clone(),
        intent_id: intent_id.to_string(),
    });

    for step in &run.steps {
        let ts = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(step.timestamp_ms)
            .map(|t| t.to_rfc3339())
            .unwrap_or_else(now);

        // UserIntervention phase gets an explicit Permission pair so the
        // rollout makes the human boundary observable as a distinct event.
        if matches!(step.phase, BrowserTaskStepPhase::UserIntervention) {
            out.push(TaskEvent::PermissionRequested {
                ts: ts.clone(),
                source: src,
                task_id: task_id.clone(),
                request_id: format!("{}-{}", task_id, step.step_index),
                reason: step.reasoning.clone(),
            });
            // The browser_run shape doesn't currently carry the decision
            // separately — successful intervention means "granted",
            // failed means "denied".
            out.push(TaskEvent::PermissionDecided {
                ts: ts.clone(),
                source: src,
                task_id: task_id.clone(),
                request_id: format!("{}-{}", task_id, step.step_index),
                decision: if step.ok {
                    PermissionDecision::Granted
                } else {
                    PermissionDecision::Denied
                },
            });
        }

        // Every step is a (ToolCall, ToolResult) pair so the rollout
        // index can answer "which tools did this browser task call?".
        let input_ref = serde_json::to_string(&step.action_args).unwrap_or_default();
        out.push(TaskEvent::ToolCall {
            ts: ts.clone(),
            source: src,
            task_id: task_id.clone(),
            tool_name: step.action_name.clone(),
            input_ref,
        });
        let output_ref = step
            .message
            .clone()
            .or_else(|| step.error.clone())
            .unwrap_or_default();
        out.push(TaskEvent::ToolResult {
            ts,
            source: src,
            task_id: task_id.clone(),
            tool_name: step.action_name.clone(),
            output_ref,
            ok: step.ok,
        });
    }

    // Terminal verdict — None when the run is still in progress or has
    // yielded at a resumable boundary.
    let verdict = match &run.status {
        BrowserTaskStatus::Running => None,
        BrowserTaskStatus::Completed => Some(TaskVerdict::Completed { summary: None }),
        BrowserTaskStatus::Failed => {
            // Pull the latest step error if there is one for the message.
            let last_err = run
                .steps
                .iter()
                .rev()
                .find_map(|s| s.error.as_ref().or(s.message.as_ref()))
                .cloned()
                .unwrap_or_else(|| "browser task failed".into());
            Some(TaskVerdict::Failed {
                error_code: "browser_failed".into(),
                message: last_err,
            })
        }
        BrowserTaskStatus::Stopped => Some(TaskVerdict::Cancelled {
            reason: Some("stopped".into()),
        }),
        BrowserTaskStatus::NeedsUserIntervention => {
            out.push(TaskEvent::BoundaryYield {
                ts: now(),
                source: src,
                task_id: task_id.clone(),
                reason: "browser needs user intervention".into(),
            });
            None
        }
        BrowserTaskStatus::PausedWaitingForBrowserRuntime => {
            out.push(TaskEvent::Checkpoint {
                ts: now(),
                source: src,
                task_id: task_id.clone(),
                checkpoint_ref: format!("browser:{task_id}:paused_waiting_for_browser_runtime"),
            });
            out.push(TaskEvent::BoundaryYield {
                ts: now(),
                source: src,
                task_id: task_id.clone(),
                reason: "browser runtime unavailable; task paused".into(),
            });
            None
        }
        BrowserTaskStatus::PausedCheckpointed => {
            out.push(TaskEvent::Checkpoint {
                ts: now(),
                source: src,
                task_id: task_id.clone(),
                checkpoint_ref: format!("browser:{task_id}:paused_checkpointed"),
            });
            out.push(TaskEvent::BoundaryYield {
                ts: now(),
                source: src,
                task_id: task_id.clone(),
                reason: "browser checkpoint paused".into(),
            });
            None
        }
    };

    if let Some(v) = verdict {
        out.push(TaskEvent::TaskFinished {
            ts: now(),
            source: src,
            task_id,
            verdict: v,
        });
    }

    out
}

/// Convert provider route decision intents into rollout-visible task signals.
///
/// Provider selection is normal browser runtime state, not a warning. The
/// generic `Signal` event keeps provider selection/degradation/rollback visible
/// without incrementing task warning counts or pretending the route decision is
/// a browser tool call.
pub fn provider_route_decision_to_events(
    decision: &BrowserProviderRouteDecision,
    task_id: &str,
) -> Vec<TaskEvent> {
    decision
        .event_intents
        .iter()
        .map(|intent| TaskEvent::Signal {
            ts: chrono::Utc::now().to_rfc3339(),
            source: TaskEventSource::Browser,
            task_id: task_id.to_string(),
            code: intent.event_name.as_str().to_string(),
            message: provider_route_signal_message(decision, intent),
        })
        .collect()
}

fn provider_route_signal_message(
    decision: &BrowserProviderRouteDecision,
    intent: &crate::browser::provider::BrowserProviderRouteEventIntent,
) -> String {
    serde_json::json!({
        "providerId": intent.provider_id.as_deref(),
        "reason": intent.reason,
        "routeStatus": decision.status,
        "selectedProviderId": decision.selected_provider_id.as_deref(),
    })
    .to_string()
}

// ────────────────────────────────────────────────────────────────────────
// M1-T4d — fire-and-forget helper that emits a finished browser run to
// the canonical sessions rollout dir. Env-gated via UCLAW_ROLLOUT_ENABLED
// for parity with the chat dispatcher (M1-T4b).
// ────────────────────────────────────────────────────────────────────────

/// Emit a finished browser run to the rollout system if enabled by env.
///
/// Spawns a short-lived `RolloutWriter` into `~/.uclaw/sessions/`, emits
/// every event from `browser_run_to_events`, then drops the handle. The
/// writer task drains the channel and closes the JSONL file.
///
/// Non-blocking from the caller's perspective: the helper awaits the
/// writer spawn (which is a single tokio task spawn) but does NOT await
/// the actual JSONL write. Total caller-visible latency is < 5 ms.
///
/// `intent_id` should be the upstream chat session id when called from
/// a chat-driven browser tool; falls back to `run.session_id` if no
/// better identifier is available.
pub async fn emit_browser_run_into_session_dir(
    run: &crate::browser::session_state::BrowserTaskRun,
    intent_id: &str,
    db_path: Option<std::path::PathBuf>,
) {
    if !crate::agent::rollout_integration::rollout_enabled_by_env() {
        return;
    }
    let sessions_dir = uclaw_utils_home::uclaw_home_pathbuf()
        .map(|p| p.join("sessions"))
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/.uclaw/sessions"));
    let handle = match crate::runtime::rollout::RolloutWriter::spawn(sessions_dir, db_path).await {
        Ok(h) => h,
        Err(e) => {
            tracing::warn!(
                run_id = %run.run_id,
                "browser rollout: failed to spawn writer: {e}"
            );
            return;
        }
    };
    let events = browser_run_to_events(run, intent_id);
    for ev in events {
        handle.emit(ev);
    }
    // Drop the handle to close the mpsc; the writer task drains
    // remaining events from the channel before exiting.
    drop(handle);
}

#[cfg(test)]
#[path = "rollout_bridge_tests.rs"]
mod tests;
