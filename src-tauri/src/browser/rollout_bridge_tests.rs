use super::*;
use crate::browser::session_state::BrowserTaskStep;

fn step(
    idx: u32,
    phase: BrowserTaskStepPhase,
    action: &str,
    ok: bool,
    msg: Option<&str>,
) -> BrowserTaskStep {
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
            step(
                0,
                BrowserTaskStepPhase::Act,
                "click",
                true,
                Some("clicked button"),
            ),
            step(
                1,
                BrowserTaskStepPhase::Act,
                "type",
                true,
                Some("typed query"),
            ),
        ],
    );
    let events = browser_run_to_events(&run, "intent-A");

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
            verdict:
                TaskVerdict::Failed {
                    error_code,
                    message,
                },
            ..
        }) => {
            assert_eq!(error_code, "browser_failed");
            assert_eq!(message, "step error");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn user_intervention_emits_permission_boundary_and_no_finished() {
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

    assert_eq!(events.len(), 6);
    assert!(matches!(events[1], TaskEvent::PermissionRequested { .. }));
    match &events[2] {
        TaskEvent::PermissionDecided { decision, .. } => {
            assert_eq!(*decision, PermissionDecision::Granted);
        }
        other => panic!("expected PermissionDecided, got {other:?}"),
    }
    match events.last() {
        Some(TaskEvent::BoundaryYield { reason, .. }) => {
            assert_eq!(reason, "browser needs user intervention");
        }
        other => panic!("expected BoundaryYield, got {other:?}"),
    }
    assert!(!events
        .iter()
        .any(|event| matches!(event, TaskEvent::TaskFinished { .. })));
}

#[test]
fn paused_checkpointed_emits_checkpoint_boundary_and_no_finished() {
    let run = run_with(BrowserTaskStatus::PausedCheckpointed, vec![]);
    let events = browser_run_to_events(&run, "intent-A");

    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
    match &events[1] {
        TaskEvent::Checkpoint { checkpoint_ref, .. } => {
            assert_eq!(checkpoint_ref, "browser:browser-run-1:paused_checkpointed");
        }
        other => panic!("expected Checkpoint, got {other:?}"),
    }
    match &events[2] {
        TaskEvent::BoundaryYield { reason, .. } => {
            assert_eq!(reason, "browser checkpoint paused");
        }
        other => panic!("expected BoundaryYield, got {other:?}"),
    }
    assert!(!events
        .iter()
        .any(|event| matches!(event, TaskEvent::TaskFinished { .. })));
}

#[test]
fn running_run_emits_no_terminator() {
    let run = run_with(
        BrowserTaskStatus::Running,
        vec![step(0, BrowserTaskStepPhase::Act, "click", true, None)],
    );
    let events = browser_run_to_events(&run, "intent-A");

    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
    assert!(!events
        .iter()
        .any(|event| matches!(event, TaskEvent::TaskFinished { .. })));
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
