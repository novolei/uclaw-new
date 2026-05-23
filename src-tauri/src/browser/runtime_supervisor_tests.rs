use super::*;

#[test]
fn default_deadlines_cover_phase1_health_windows() {
    let deadlines = BrowserRuntimeDeadlineProfile::local_chromium_defaults();

    assert_eq!(deadlines.startup_ms, 60_000);
    assert_eq!(deadlines.connect_ms, 15_000);
    assert_eq!(deadlines.action_ms, 30_000);
    assert_eq!(deadlines.wait_ms, 10_000);
    assert_eq!(deadlines.network_idle_ms, 15_000);
    assert_eq!(deadlines.first_frame_ms, 8_000);
    assert_eq!(deadlines.no_output_heartbeat_ms, 5_000);
}

#[test]
fn supervisor_transitions_sessions_and_degrades_invalid_jumps() {
    let mut supervisor = BrowserRuntimeSupervisor::new_local_chromium();
    let session = supervisor.ensure_session("session-a", 100);
    assert_eq!(session.state, BrowserRuntimeState::Starting);

    let transition = supervisor
        .transition_session("session-a", BrowserRuntimeState::Ready, 200)
        .unwrap();
    assert_eq!(
        transition,
        BrowserRuntimeTransition {
            from: BrowserRuntimeState::Starting,
            to: BrowserRuntimeState::Ready,
        }
    );

    let transition = supervisor
        .mark_action_started("session-a", "task-1", 300)
        .unwrap();
    assert_eq!(transition.to, BrowserRuntimeState::Acting);
    assert_eq!(
        supervisor.session("session-a").unwrap().active_task_id,
        Some("task-1".to_string())
    );

    let degradation = supervisor
        .transition_session("session-a", BrowserRuntimeState::Starting, 400)
        .unwrap_err();
    assert_eq!(degradation.code, "invalid_state_transition");
    let session = supervisor.session("session-a").unwrap();
    assert_eq!(session.state, BrowserRuntimeState::Degraded);
    assert!(session.degraded_reason.is_some());
}

#[test]
fn local_chromium_doctor_classifies_active_and_missing_contexts() {
    let supervisor = BrowserRuntimeSupervisor::new_local_chromium();
    let active_sessions = vec!["session-a".to_string(), "session-b".to_string()];

    let ready = supervisor.doctor_from_active_contexts("session-a", &active_sessions);
    assert_eq!(ready.readiness, BrowserProviderReadiness::Ready);
    assert_eq!(ready.status, StartupDoctorStatus::Ready);
    assert_eq!(ready.runtime_state, BrowserRuntimeState::Ready);
    assert_eq!(ready.active_contexts, 2);

    let missing = supervisor.doctor_from_active_contexts("session-c", &active_sessions);
    assert_eq!(missing.readiness, BrowserProviderReadiness::NeedsSetup);
    assert_eq!(missing.status, StartupDoctorStatus::Deferred);
    assert_eq!(missing.runtime_state, BrowserRuntimeState::Stopped);
    assert!(missing.remediation.is_some());
}

#[test]
fn heartbeat_and_action_deadlines_degrade_projection_with_attention() {
    let mut supervisor = BrowserRuntimeSupervisor::new_local_chromium().with_deadlines(
        BrowserRuntimeDeadlineProfile {
            action_ms: 100,
            no_output_heartbeat_ms: 50,
            ..BrowserRuntimeDeadlineProfile::local_chromium_defaults()
        },
    );
    supervisor.ensure_session("session-a", 1);
    supervisor
        .transition_session("session-a", BrowserRuntimeState::Ready, 2)
        .unwrap();
    supervisor
        .mark_action_started("session-a", "task-1", 3)
        .unwrap();

    assert!(supervisor
        .classify_no_output_heartbeat("session-a", 30, 4)
        .is_none());
    let heartbeat = supervisor
        .classify_no_output_heartbeat("session-a", 51, 5)
        .unwrap();
    assert_eq!(heartbeat.code, "no_output_heartbeat_missed");
    assert_eq!(
        heartbeat.event_name,
        BrowserTaskEventName::RuntimeHeartbeatMissed.as_str()
    );

    let action = supervisor
        .classify_action_elapsed("session-a", 101, 6)
        .unwrap();
    assert_eq!(action.code, "action_deadline_exceeded");
    assert!(action.artifact_recommended);

    let doctor = supervisor.doctor_from_active_contexts("session-a", &["session-a".to_string()]);
    let projection = supervisor.projection_for_session("session-a", &doctor);
    assert_eq!(projection.runtime.state, BrowserRuntimeState::Degraded);
    assert_eq!(
        projection.task_boundary.status,
        BrowserTaskBoundaryStatus::PausedCheckpointed
    );
    assert!(projection.attention_reasons().contains(&"runtime_degraded"));
    assert!(projection
        .attention_reasons()
        .contains(&"task_paused_checkpointed"));
}

#[test]
fn artifact_pack_records_supervisor_metadata_and_session_reference() {
    let mut supervisor = BrowserRuntimeSupervisor::new_local_chromium();
    supervisor.ensure_session("session-a", 1);

    let pack = supervisor.artifact_pack(
        "session-a",
        Some("task-1".to_string()),
        "action timeout",
        BrowserTaskEventName::RuntimeArtifactPackCreated,
        1234,
    );

    assert_eq!(pack.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
    assert_eq!(pack.session_id, "session-a");
    assert_eq!(pack.task_id, Some("task-1".to_string()));
    assert_eq!(pack.reason, "action timeout");
    assert_eq!(
        pack.event_name,
        BrowserTaskEventName::RuntimeArtifactPackCreated.as_str()
    );
    assert!(pack.artifact_ref.contains("session-a"));
    assert_eq!(
        supervisor
            .session("session-a")
            .unwrap()
            .last_artifact_pack_ref,
        Some(pack.artifact_ref)
    );
}

#[tokio::test]
async fn context_manager_snapshot_does_not_launch_browser_in_tests() {
    let supervisor = BrowserRuntimeSupervisor::new_local_chromium();
    let context_manager = BrowserContextManager::new_for_test("/tmp/uclaw-browser-test".into());

    let outcome = supervisor
        .doctor_from_context_manager("session-a", &context_manager)
        .await;

    assert_eq!(outcome.active_contexts, 0);
    assert_eq!(outcome.readiness, BrowserProviderReadiness::NeedsSetup);
    assert_eq!(outcome.runtime_state, BrowserRuntimeState::Stopped);
}

#[test]
fn supervisor_uses_phase0_local_chromium_provider_card() {
    let supervisor = BrowserRuntimeSupervisor::new_local_chromium();
    let card = supervisor.provider_card().unwrap();

    assert_eq!(card.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
    assert!(card.enabled_by_default);
    assert!(!card.requires_runtime_pack);
    assert!(card.uses_isolated_profile_by_default);
}
