use super::*;
use crate::browser::playwright_cli_capabilities;
use crate::browser::provider::{
    decide_browser_provider_route, local_chromium_capabilities, BrowserCapabilityProbe,
    BrowserProviderCapabilities, BrowserProviderReadinessProbe, BrowserProviderRouteDecision,
    BrowserProviderRouteDecisionStatus, BrowserProviderRouteEventIntent,
    BrowserProviderRouteRequest, BrowserProviderRouteSkippedProvider, BrowserProviderStatus,
    BrowserSetupCheck, LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{BrowserProviderSelectionRequest, BrowserTaskEventName};
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

fn ready_provider_status(capabilities: BrowserProviderCapabilities) -> BrowserProviderStatus {
    BrowserProviderStatus::from_probe(
        capabilities.clone(),
        BrowserProviderReadinessProbe {
            provider_id: capabilities.provider_id,
            setup_checks: vec![BrowserSetupCheck::passed("setup", "Provider setup")],
            capability_probes: vec![BrowserCapabilityProbe::passed("click", true)],
            active_contexts: 0,
            notes: Vec::new(),
        },
    )
}

fn degraded_provider_status(capabilities: BrowserProviderCapabilities) -> BrowserProviderStatus {
    BrowserProviderStatus::from_probe(
        capabilities.clone(),
        BrowserProviderReadinessProbe {
            provider_id: capabilities.provider_id,
            setup_checks: vec![BrowserSetupCheck::passed("setup", "Provider setup")],
            capability_probes: vec![BrowserCapabilityProbe::failed(
                "click",
                true,
                "Provider click probe failed.",
            )],
            active_contexts: 0,
            notes: Vec::new(),
        },
    )
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
fn paused_waiting_runtime_emits_checkpoint_boundary_and_no_finished() {
    let run = run_with(BrowserTaskStatus::PausedWaitingForBrowserRuntime, vec![]);
    let events = browser_run_to_events(&run, "intent-A");

    assert_eq!(events.len(), 3);
    assert!(matches!(events[0], TaskEvent::TaskStarted { .. }));
    match &events[1] {
        TaskEvent::Checkpoint { checkpoint_ref, .. } => {
            assert_eq!(
                checkpoint_ref,
                "browser:browser-run-1:paused_waiting_for_browser_runtime"
            );
        }
        other => panic!("expected Checkpoint, got {other:?}"),
    }
    match &events[2] {
        TaskEvent::BoundaryYield { reason, .. } => {
            assert_eq!(reason, "browser runtime unavailable; task paused");
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

#[test]
fn provider_route_selected_intent_emits_browser_signal() {
    let request = BrowserProviderRouteRequest {
        selection: BrowserProviderSelectionRequest {
            action: Some("click".into()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        disabled_provider_ids: Vec::new(),
        previous_provider_id: None,
    };
    let decision = decide_browser_provider_route(
        &request,
        &[ready_provider_status(local_chromium_capabilities())],
    );

    let events = provider_route_decision_to_events(&decision, "browser-run-1");

    assert_eq!(events.len(), 1);
    match &events[0] {
        TaskEvent::Signal {
            source,
            task_id,
            code,
            message,
            ..
        } => {
            assert_eq!(*source, TaskEventSource::Browser);
            assert_eq!(task_id, "browser-run-1");
            assert_eq!(code, "browser.provider.selected");
            let payload: serde_json::Value = serde_json::from_str(message).unwrap();
            assert_eq!(payload["providerId"], LOCAL_CHROMIUM_PROVIDER_ID);
            assert_eq!(payload["selectedProviderId"], LOCAL_CHROMIUM_PROVIDER_ID);
            assert_eq!(payload["routeStatus"], "selected");
            assert_eq!(payload["reason"], "provider_selected");
        }
        other => panic!("expected provider route Signal, got {other:?}"),
    }
}

#[test]
fn provider_route_signal_includes_skipped_provider_reasons() {
    let decision = BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::Selected,
        selected_provider_id: Some(LOCAL_CHROMIUM_PROVIDER_ID.to_string()),
        candidates: Vec::new(),
        event_intents: vec![BrowserProviderRouteEventIntent {
            event_name: BrowserTaskEventName::ProviderSelected,
            provider_id: Some(LOCAL_CHROMIUM_PROVIDER_ID.to_string()),
            reason: "provider_selected".to_string(),
        }],
        skipped_providers: vec![BrowserProviderRouteSkippedProvider {
            provider_id: crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
            reason: "probe_failed".to_string(),
        }],
    };

    let events = provider_route_decision_to_events(&decision, "browser-run-1");

    match &events[0] {
        TaskEvent::Signal { message, .. } => {
            let payload: serde_json::Value = serde_json::from_str(message).unwrap();
            assert_eq!(payload["selectedProviderId"], LOCAL_CHROMIUM_PROVIDER_ID);
            assert_eq!(
                payload["skippedProviders"][0]["providerId"],
                crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            );
            assert_eq!(payload["skippedProviders"][0]["reason"], "probe_failed");
        }
        other => panic!("expected provider route Signal, got {other:?}"),
    }
}

#[test]
fn provider_route_degraded_candidate_emits_signal_without_warning_count() {
    let request = BrowserProviderRouteRequest {
        selection: BrowserProviderSelectionRequest {
            action: Some("click".into()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        disabled_provider_ids: Vec::new(),
        previous_provider_id: None,
    };
    let decision = decide_browser_provider_route(
        &request,
        &[
            degraded_provider_status(local_chromium_capabilities()),
            ready_provider_status(playwright_cli_capabilities()),
        ],
    );

    let events = provider_route_decision_to_events(&decision, "browser-run-1");

    assert!(events.iter().any(|event| match event {
        TaskEvent::Signal { code, message, .. } if code == "browser.provider.degraded" => {
            let payload: serde_json::Value = serde_json::from_str(message).unwrap();
            payload["providerId"] == LOCAL_CHROMIUM_PROVIDER_ID
                && payload["reason"] == "provider_readiness_degraded"
        }
        _ => false,
    }));
    assert!(!events
        .iter()
        .any(|event| matches!(event, TaskEvent::Warning { .. })));
}

#[test]
fn provider_route_rollback_emits_previous_and_fallback_signals() {
    let request = BrowserProviderRouteRequest {
        selection: BrowserProviderSelectionRequest {
            action: Some("click".into()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        disabled_provider_ids: vec![LOCAL_CHROMIUM_PROVIDER_ID.to_string()],
        previous_provider_id: Some(LOCAL_CHROMIUM_PROVIDER_ID.to_string()),
    };
    let decision = decide_browser_provider_route(
        &request,
        &[
            ready_provider_status(local_chromium_capabilities()),
            ready_provider_status(playwright_cli_capabilities()),
        ],
    );

    let events = provider_route_decision_to_events(&decision, "browser-run-1");
    let codes = events
        .iter()
        .map(TaskEvent::kind)
        .collect::<Vec<&'static str>>();

    assert!(codes.iter().all(|kind| *kind == "signal"));
    assert!(events.iter().any(|event| match event {
        TaskEvent::Signal { code, message, .. } if code == "browser.provider.rolled_back" => {
            let payload: serde_json::Value = serde_json::from_str(message).unwrap();
            payload["providerId"] == LOCAL_CHROMIUM_PROVIDER_ID
                && payload["routeStatus"] == "rolled_back"
                && payload["reason"] == "previous_provider_unavailable_or_disabled"
        }
        _ => false,
    }));
    assert!(events.iter().any(|event| match event {
        TaskEvent::Signal { code, message, .. } if code == "browser.provider.selected" => {
            let payload: serde_json::Value = serde_json::from_str(message).unwrap();
            payload["providerId"] == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
                && payload["reason"] == "fallback_provider_selected"
        }
        _ => false,
    }));
}

#[test]
fn provider_route_signal_batch_uses_one_timestamp() {
    let request = BrowserProviderRouteRequest {
        selection: BrowserProviderSelectionRequest {
            action: Some("click".into()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        disabled_provider_ids: vec![LOCAL_CHROMIUM_PROVIDER_ID.to_string()],
        previous_provider_id: Some(LOCAL_CHROMIUM_PROVIDER_ID.to_string()),
    };
    let decision = decide_browser_provider_route(
        &request,
        &[
            ready_provider_status(local_chromium_capabilities()),
            ready_provider_status(playwright_cli_capabilities()),
        ],
    );

    let events = provider_route_decision_to_events(&decision, "browser-run-1");
    let signal_timestamps = events
        .iter()
        .filter_map(|event| match event {
            TaskEvent::Signal { ts, .. } => Some(ts.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(signal_timestamps.len() >= 2);
    assert!(signal_timestamps
        .iter()
        .all(|ts| *ts == signal_timestamps[0]));
}
