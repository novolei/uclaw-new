//! M1-T4e — bridge `AutomationActivity` → `Vec<TaskEvent>` for the rollout
//! writer, closing the ADR M1 exit criterion: chat + browser +
//! **automation** all produce comparable traces in the same rollout
//! JSONL.
//!
//! Mapping rules:
//!
//! - `ActivityStatus::Completed` → `TaskFinished{Completed{summary: report_outcome}}`
//! - `ActivityStatus::Failed`    → `TaskFinished{Failed{error_code: "automation_failed", message: error_text}}`
//! - `ActivityStatus::Cancelled` → `TaskFinished{Cancelled{reason: "cancelled"}}`
//! - `ActivityStatus::Queued`    → no terminator (still pending)
//! - `ActivityStatus::Running`   → no terminator
//! - `ActivityStatus::WaitingUser` → `BoundaryYield` (no `Finished`; paused)
//!
//! Intermediate events when the activity ran the LLM loop:
//! - If `llm_iterations > 0` and `(llm_tokens_in + llm_tokens_out) > 0`,
//!   emit a summary `ModelTurn` with the aggregated token counts.
//!
//! Like `browser::rollout_bridge`, the in-progress states emit
//! `TaskStarted` + any intermediate events but **no** `TaskFinished` so
//! the caller can append a terminator later.

use crate::automation::activity::{ActivityStatus, AutomationActivity};
use crate::runtime::contracts::{
    TaskEvent, TaskEventSource, TaskVerdict, TokenUsage,
};

/// Convert an automation activity to a rollout `Vec<TaskEvent>`.
///
/// `intent_id` should be the upstream session id when the activity is
/// session-bound (typical for chat-triggered automations) or
/// `activity.spec_id` when the activity is system-triggered (schedule /
/// file / webhook).
pub fn activity_to_events(activity: &AutomationActivity, intent_id: &str) -> Vec<TaskEvent> {
    let src = TaskEventSource::Automation;
    let task_id = activity.id.clone();
    let mut out: Vec<TaskEvent> = Vec::with_capacity(3);

    // `started_at` is `Option<i64>` (None for runs that never reached the
    // loop). Fall back to queued_at so the rollout still has a valid ts.
    let start_ms = activity.started_at.unwrap_or(activity.queued_at);
    let start_ts = millis_to_rfc3339(start_ms);
    out.push(TaskEvent::TaskStarted {
        ts: start_ts.clone(),
        source: src,
        task_id: task_id.clone(),
        intent_id: intent_id.to_string(),
    });

    // LLM token summary as a single ModelTurn — automation activities
    // don't expose per-turn token attribution, so this is the closest
    // honest aggregation. The `provider`/`model` strings are placeholders
    // because the activity row doesn't store them; M1-T6b will plumb
    // real attribution through automation runtime.
    if activity.llm_iterations > 0
        && (activity.llm_tokens_in > 0 || activity.llm_tokens_out > 0)
    {
        let total = activity
            .llm_tokens_in
            .saturating_add(activity.llm_tokens_out) as u32;
        out.push(TaskEvent::ModelTurn {
            ts: start_ts.clone(),
            source: src,
            task_id: task_id.clone(),
            provider: "automation".into(),
            model: "aggregated".into(),
            token_usage: TokenUsage {
                input_tokens: activity.llm_tokens_in.max(0) as u32,
                cached_input_tokens: 0,
                output_tokens: activity.llm_tokens_out.max(0) as u32,
                reasoning_output_tokens: 0,
                total_tokens: total,
                cost_usd_micros: None,
            },
        });
    }

    // Terminal event (if status is terminal).
    let term_ms = activity.completed_at.unwrap_or(start_ms);
    let term_ts = millis_to_rfc3339(term_ms);

    let verdict = match &activity.status {
        ActivityStatus::Queued | ActivityStatus::Running => None,
        ActivityStatus::WaitingUser => {
            // Treat WaitingUser as a yield, not a terminator. Caller's
            // future `resume` will emit a fresh activity-to-events with
            // the eventual final status.
            out.push(TaskEvent::BoundaryYield {
                ts: term_ts.clone(),
                source: src,
                task_id: task_id.clone(),
                reason: "waiting for user".into(),
            });
            None
        }
        ActivityStatus::Completed => Some(TaskVerdict::Completed {
            summary: activity.report_outcome.clone(),
        }),
        ActivityStatus::Failed => Some(TaskVerdict::Failed {
            error_code: "automation_failed".into(),
            message: activity
                .error_text
                .clone()
                .unwrap_or_else(|| "automation run failed".into()),
        }),
        ActivityStatus::Cancelled => Some(TaskVerdict::Cancelled {
            reason: Some("cancelled".into()),
        }),
    };

    if let Some(v) = verdict {
        out.push(TaskEvent::TaskFinished {
            ts: term_ts,
            source: src,
            task_id,
            verdict: v,
        });
    }

    out
}

/// Convert millis-since-epoch to an RFC 3339 timestamp. Falls back to
/// `now` if the value is out of range (won't happen in practice but
/// keeps the bridge total).
fn millis_to_rfc3339(millis: i64) -> String {
    chrono::DateTime::<chrono::Utc>::from_timestamp_millis(millis)
        .map(|t| t.to_rfc3339())
        .unwrap_or_else(|| chrono::Utc::now().to_rfc3339())
}

// ────────────────────────────────────────────────────────────────────────
// Fire-and-forget rollout emit — same shape as browser::rollout_bridge.
// ────────────────────────────────────────────────────────────────────────

/// Emit a terminal automation activity to the rollout system if enabled
/// by env (`UCLAW_ROLLOUT_ENABLED=1`). Fire-and-forget; caller-visible
/// latency < 5 ms.
pub async fn emit_activity_into_session_dir(
    activity: &AutomationActivity,
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
                activity_id = %activity.id,
                spec_id = %activity.spec_id,
                "automation rollout: failed to spawn writer: {e}"
            );
            return;
        }
    };
    let events = activity_to_events(activity, intent_id);
    for ev in events {
        handle.emit(ev);
    }
    drop(handle);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::automation::activity::TriggerSource;

    fn activity_template(status: ActivityStatus) -> AutomationActivity {
        AutomationActivity {
            id: "automation-run-1".into(),
            spec_id: "spec-A".into(),
            subscription_id: None,
            trigger_source_type: TriggerSource::Schedule,
            trigger_payload_json: "{}".into(),
            status,
            error_text: None,
            queued_at: 1_700_000_000_000,
            started_at: Some(1_700_000_001_000),
            completed_at: Some(1_700_000_005_000),
            duration_ms: 4_000,
            llm_iterations: 0,
            llm_tokens_in: 0,
            llm_tokens_out: 0,
            session_id: None,
            report_artifacts_json: "[]".into(),
            report_text: None,
            report_outcome: None,
            escalation_id: None,
            resumed_from_activity_id: None,
            resumed_from_escalation_id: None,
            working_dir: String::new(),
        }
    }

    #[test]
    fn completed_emits_started_and_finished_completed() {
        let mut a = activity_template(ActivityStatus::Completed);
        a.report_outcome = Some("delivered report".into());
        let events = activity_to_events(&a, "spec-A");
        assert_eq!(events.len(), 2);
        match &events[1] {
            TaskEvent::TaskFinished {
                verdict: TaskVerdict::Completed { summary },
                source,
                ..
            } => {
                assert_eq!(*source, TaskEventSource::Automation);
                assert_eq!(summary.as_deref(), Some("delivered report"));
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    #[test]
    fn failed_carries_error_text_into_verdict() {
        let mut a = activity_template(ActivityStatus::Failed);
        a.error_text = Some("LLM out of budget".into());
        let events = activity_to_events(&a, "spec-A");
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Failed {
                    error_code,
                    message,
                },
                ..
            }) => {
                assert_eq!(error_code, "automation_failed");
                assert_eq!(message, "LLM out of budget");
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn failed_with_no_error_text_falls_back_to_default_message() {
        let a = activity_template(ActivityStatus::Failed);
        let events = activity_to_events(&a, "spec-A");
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Failed { message, .. },
                ..
            }) => {
                assert!(message.contains("automation run failed"));
            }
            other => panic!("expected Failed, got {other:?}"),
        }
    }

    #[test]
    fn cancelled_maps_to_cancelled_verdict() {
        let a = activity_template(ActivityStatus::Cancelled);
        let events = activity_to_events(&a, "spec-A");
        match events.last() {
            Some(TaskEvent::TaskFinished {
                verdict: TaskVerdict::Cancelled { reason },
                ..
            }) => {
                assert_eq!(reason.as_deref(), Some("cancelled"));
            }
            other => panic!("expected Cancelled, got {other:?}"),
        }
    }

    #[test]
    fn waiting_user_emits_boundary_yield_and_no_finished() {
        let a = activity_template(ActivityStatus::WaitingUser);
        let events = activity_to_events(&a, "spec-A");
        // Started + BoundaryYield, NO TaskFinished
        assert_eq!(events.len(), 2);
        assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        match &events[1] {
            TaskEvent::BoundaryYield { reason, .. } => {
                assert!(reason.contains("waiting for user"));
            }
            other => panic!("expected BoundaryYield, got {other:?}"),
        }
        assert!(!events.iter().any(|e| matches!(e, TaskEvent::TaskFinished { .. })));
    }

    #[test]
    fn queued_or_running_have_no_terminator() {
        for status in [ActivityStatus::Queued, ActivityStatus::Running] {
            let a = activity_template(status);
            let events = activity_to_events(&a, "spec-A");
            assert_eq!(events.len(), 1, "{status:?} should emit only TaskStarted");
            assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
        }
    }

    #[test]
    fn llm_token_summary_emits_modelturn_when_iterations_positive() {
        let mut a = activity_template(ActivityStatus::Completed);
        a.llm_iterations = 3;
        a.llm_tokens_in = 1000;
        a.llm_tokens_out = 250;
        let events = activity_to_events(&a, "spec-A");
        // Started + ModelTurn + Finished
        assert_eq!(events.len(), 3);
        match &events[1] {
            TaskEvent::ModelTurn { token_usage, .. } => {
                assert_eq!(token_usage.input_tokens, 1000);
                assert_eq!(token_usage.output_tokens, 250);
                assert_eq!(token_usage.total_tokens, 1250);
            }
            other => panic!("expected ModelTurn, got {other:?}"),
        }
    }

    #[test]
    fn llm_zero_iterations_skips_modelturn() {
        // Even Completed runs with iterations=0 (e.g. filtered before
        // LLM) should not emit ModelTurn.
        let mut a = activity_template(ActivityStatus::Completed);
        a.llm_iterations = 0;
        a.llm_tokens_in = 999; // garbage value — should be ignored
        let events = activity_to_events(&a, "spec-A");
        assert_eq!(events.len(), 2);
        assert!(!events.iter().any(|e| matches!(e, TaskEvent::ModelTurn { .. })));
    }

    #[test]
    fn missing_started_at_falls_back_to_queued_at() {
        let mut a = activity_template(ActivityStatus::Failed);
        a.started_at = None;
        a.error_text = Some("never started".into());
        let events = activity_to_events(&a, "spec-A");
        // TaskStarted ts should equal queued_at, not crash on None.
        match &events[0] {
            TaskEvent::TaskStarted { ts, .. } => {
                assert!(ts.starts_with("2023-"), "got {ts}"); // 1_700_000_000_000 millis → 2023
            }
            other => panic!("expected TaskStarted, got {other:?}"),
        }
    }
}
