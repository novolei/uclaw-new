use super::*;
use crate::automation::activity::TriggerSource;
use crate::automation::protocol::humane_v1::Subscription;
use crate::runtime::contracts::{AutonomyLevel, IntentOrigin, TaskEventSource};

#[test]
fn external_ambient_work_maps_to_scheduled_automation() {
    let mapping = ambient_to_automation_mapping(
        AmbientWorkKind::ExternalDirective,
        AmbientScheduleInput::Every {
            every: "30m".into(),
        },
    );

    assert_eq!(mapping.intent_origin, IntentOrigin::Automation);
    assert_eq!(mapping.autonomy_level, AutonomyLevel::ScheduledWorker);
    assert_eq!(mapping.task_event_source, TaskEventSource::Automation);
    assert_eq!(mapping.trigger_source, TriggerSource::Schedule);
    assert!(mapping.requires_automation_activity_ledger);
    assert!(mapping.requires_gbrain_memory_receipts);
    assert!(mapping.requires_worker_heartbeat_receipt);

    match mapping.subscription {
        Subscription::Schedule(schedule) => {
            assert_eq!(schedule.cron, None);
            assert_eq!(schedule.every.as_deref(), Some("30m"));
        }
        _ => panic!("ambient work must map to existing schedule subscriptions"),
    }
}

#[test]
fn internal_maintenance_uses_system_origin_without_changing_scheduler_owner() {
    let mapping = ambient_to_automation_mapping(
        AmbientWorkKind::InternalMaintenance,
        AmbientScheduleInput::Cron {
            cron: "0 8 * * *".into(),
        },
    );

    assert_eq!(mapping.intent_origin, IntentOrigin::System);
    assert_eq!(mapping.autonomy_level, AutonomyLevel::ScheduledWorker);
    assert_eq!(mapping.trigger_source, TriggerSource::Schedule);

    match mapping.subscription {
        Subscription::Schedule(schedule) => {
            assert_eq!(schedule.cron.as_deref(), Some("0 8 * * *"));
            assert_eq!(schedule.every, None);
        }
        _ => panic!("internal maintenance must still use the schedule source"),
    }
}

#[test]
fn delivery_classification_preserves_jcode_semantics_without_new_runner() {
    assert_eq!(
        classify_delivery(true, true, false),
        AmbientDeliveryMode::ExistingSessionSoftInterrupt
    );
    assert_eq!(
        classify_delivery(false, true, false),
        AmbientDeliveryMode::QueuedScheduledWorker
    );
    assert_eq!(
        classify_delivery(false, true, true),
        AmbientDeliveryMode::NewAutomationRun
    );
    assert_eq!(
        classify_delivery(false, false, true),
        AmbientDeliveryMode::QueuedScheduledWorker
    );
}

#[test]
fn permission_context_requires_boundary_review_fields() {
    let context = AmbientPermissionContext::new(
        "needs filesystem access to inspect generated logs",
        vec!["read logs", "summarize findings"],
        vec!["may expose local file names"],
        "stop before writing files",
        "diagnostic summary with no mutation",
    );

    assert!(context.requires_boundary_yield);
    assert!(context.is_complete());

    let incomplete = AmbientPermissionContext::new(
        "needs filesystem access",
        vec!["read logs"],
        Vec::<String>::new(),
        "stop before writing files",
        "diagnostic summary",
    );

    assert!(!incomplete.is_complete());
}

#[test]
fn default_policy_keeps_user_and_budget_headroom() {
    let policy = AmbientSchedulePolicy::default();

    assert!(policy.pause_on_active_user_session);
    assert!(policy.reserve_user_token_headroom);
    assert!(policy.rate_limit_backoff);
    assert_eq!(policy.min_interval_minutes, 15);
    assert_eq!(policy.max_interval_minutes, 240);
}

#[test]
fn mapping_validation_rejects_forbidden_jcode_surfaces() {
    let mut mapping = ambient_to_automation_mapping(
        AmbientWorkKind::ExternalDirective,
        AmbientScheduleInput::Every { every: "1h".into() },
    );

    assert!(validate_ambient_mapping(&mapping).is_ok());
    assert!(mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::SecondScheduler));
    assert!(mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::AmbientTriggerSource));
    assert!(mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::AmbientJsonCanonicalState));
    assert!(mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::AmbientSessionRegistry));
    assert!(mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::PermissionBypass));

    mapping.requires_gbrain_memory_receipts = false;
    assert_eq!(
        validate_ambient_mapping(&mapping),
        Err(AmbientMappingError::MissingGbrainReceiptPolicy)
    );
}
