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
//! - Run status → terminal `TaskFinished` verdict:
//!     Completed              → `TaskVerdict::Completed`
//!     Failed                 → `TaskVerdict::Failed { error_code: "browser_failed", message }`
//!     Stopped                → `TaskVerdict::Cancelled { reason: Some("stopped") }`
//!     NeedsUserIntervention  → `TaskVerdict::Completed { summary: Some("paused for intervention") }`
//!     PausedCheckpointed     → `TaskVerdict::Completed { summary: Some("checkpoint") }`
//!     Running                → no terminator (caller appends later)

use crate::browser::session_state::{
    BrowserTaskRun, BrowserTaskStatus, BrowserTaskStepPhase,
};
use crate::runtime::contracts::{
    PermissionDecision, TaskEvent, TaskEventSource, TaskVerdict,
};

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

    // Terminal verdict — None when the run is still in progress.
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
        BrowserTaskStatus::NeedsUserIntervention => Some(TaskVerdict::Completed {
            summary: Some("paused for intervention".into()),
        }),
        BrowserTaskStatus::PausedCheckpointed => Some(TaskVerdict::Completed {
            summary: Some("checkpoint".into()),
        }),
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
mod tests {
    use super::*;
    use crate::browser::session_state::BrowserTaskStep;

    fn step(idx: u32, phase: BrowserTaskStepPhase, action: &str, ok: bool, msg: Option<&str>) -> BrowserTaskStep {
        BrowserTaskStep {
            step_index: idx,
            phase,
            observation_summary: String::new(),
            reasoning: format!("step-{idx}"),
            action_name: action.into(),
            action_args: serde_json::json!({"a": idx}),
            ok,
            message: msg.map(String::from),
            error: if ok { None } else { Some("step error".into()) },
            timestamp_ms: 1_700_000_000_000 + idx as i64 * 1000,
        }
    }

    fn run_with(status: BrowserTaskStatus, steps: Vec<BrowserTaskStep>) -> BrowserTaskRun {
        BrowserTaskRun {
            run_id: "browser-run-1".into(),
            session_id: "session-A".into(),
            task: "test task".into(),
            status,
            steps,
        }
    }

    #[test]
    fn completed_run_emits_started_two_pairs_finished() {
        let run = run_with(
            BrowserTaskStatus::Completed,
            vec![
                step(0, BrowserTaskStepPhase::Act, "click", true, Some("clicked button")),
                step(1, BrowserTaskStepPhase::Act, "type", true, Some("typed query")),
            ],
        );
        let events = browser_run_to_events(&run, "intent-A");
        // Started + 2 * (ToolCall, ToolResult) + Finished
        assert_eq!(events.len(), 6);
        assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        assert!(matches!(events[1], TaskEvent::ToolCall { .. }));
        assert!(matches!(events[2], TaskEvent::ToolResult { ok: true, .. }));
        assert!(matches!(events[3], TaskEvent::ToolCall { .. }));
        assert!(matches!(events[4], TaskEvent::ToolResult { ok: true, .. }));
        match &events[5] {
            TaskEvent::TaskFinished {
                verdict: TaskVerdict::Completed { summary },
                source,
                ..
            } => {
                assert_eq!(*source, TaskEventSource::Browser);
                assert!(summary.is_none());
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn failed_run_carries_last_error_into_verdict() {
        let run = run_with(
            BrowserTaskStatus::Failed,
            vec![
                step(0, BrowserTaskStepPhase::Act, "click", true, Some("ok")),
                step(1, BrowserTaskStepPhase::Act, "navigate", false, None),
            ],
        );
        let events = browser_run_to_events(&run, "intent-A");
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Failed { error_code, message },
                ..
            }) => {
                assert_eq!(error_code, "browser_failed");
                assert_eq!(message, "step error");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn user_intervention_emits_permission_pair() {
        let run = run_with(
            BrowserTaskStatus::NeedsUserIntervention,
            vec![step(
                0,
                BrowserTaskStepPhase::UserIntervention,
                "ask_human",
                true,
                Some("granted"),
            )],
        );
        let events = browser_run_to_events(&run, "intent-A");
        // Started + PermissionRequested + PermissionDecided + ToolCall + ToolResult + Finished
        assert_eq!(events.len(), 6);
        assert!(matches!(events[1], TaskEvent::PermissionRequested { .. }));
        match &events[2] {
            TaskEvent::PermissionDecided { decision, .. } => {
                assert_eq!(*decision, PermissionDecision::Granted);
            }
            other => panic!("expected PermissionDecided, got {other:?}"),
        }
        // Finished carries the "paused for intervention" summary.
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Completed { summary },
                ..
            }) => {
                assert_eq!(summary.as_deref(), Some("paused for intervention"));
            }
            other => panic!("expected Completed(intervention), got {other:?}"),
        }
    }

    #[test]
    fn running_run_emits_no_terminator() {
        let run = run_with(
            BrowserTaskStatus::Running,
            vec![step(0, BrowserTaskStepPhase::Act, "click", true, None)],
        );
        let events = browser_run_to_events(&run, "intent-A");
        // Started + (Call, Result) + NO Finished
        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        assert!(!events.iter().any(|e| matches!(e, TaskEvent::TaskFinished { .. })));
    }

    #[test]
    fn stopped_run_maps_to_cancelled() {
        let run = run_with(BrowserTaskStatus::Stopped, vec![]);
        let events = browser_run_to_events(&run, "intent-A");
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Cancelled { reason },
                ..
            }) => {
                assert_eq!(reason.as_deref(), Some("stopped"));
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn empty_steps_completed_emits_started_then_finished() {
        let run = run_with(BrowserTaskStatus::Completed, vec![]);
        let events = browser_run_to_events(&run, "intent-A");
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        assert!(matches!(events[1], TaskEvent::TaskFinished { .. }));
    }
}
