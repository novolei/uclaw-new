//! PR10 pure contract for mapping jcode-style ambient work into uClaw
//! automation/scheduled-worker primitives.

use crate::automation::activity::TriggerSource;
use crate::automation::protocol::humane_v1::{ScheduleSubscription, Subscription};
use crate::runtime::contracts::{AutonomyLevel, IntentOrigin, TaskEventSource};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientWorkKind {
    ExternalDirective,
    UserFollowUp,
    InternalMaintenance,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AmbientScheduleInput {
    Cron { cron: String },
    Every { every: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientDeliveryMode {
    ExistingSessionSoftInterrupt,
    QueuedScheduledWorker,
    NewAutomationRun,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ForbiddenAmbientSurface {
    SecondScheduler,
    AmbientTriggerSource,
    AmbientJsonCanonicalState,
    AmbientSessionRegistry,
    PermissionBypass,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AmbientSchedulePolicy {
    pub pause_on_active_user_session: bool,
    pub reserve_user_token_headroom: bool,
    pub rate_limit_backoff: bool,
    pub min_interval_minutes: u16,
    pub max_interval_minutes: u16,
}

impl Default for AmbientSchedulePolicy {
    fn default() -> Self {
        Self {
            pause_on_active_user_session: true,
            reserve_user_token_headroom: true,
            rate_limit_backoff: true,
            min_interval_minutes: 15,
            max_interval_minutes: 240,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AmbientPermissionContext {
    pub rationale: String,
    pub planned_steps: Vec<String>,
    pub risks: Vec<String>,
    pub rollback: String,
    pub expected_outcome: String,
    pub requires_boundary_yield: bool,
}

impl AmbientPermissionContext {
    pub fn new(
        rationale: impl Into<String>,
        planned_steps: Vec<impl Into<String>>,
        risks: Vec<impl Into<String>>,
        rollback: impl Into<String>,
        expected_outcome: impl Into<String>,
    ) -> Self {
        Self {
            rationale: rationale.into(),
            planned_steps: planned_steps.into_iter().map(Into::into).collect(),
            risks: risks.into_iter().map(Into::into).collect(),
            rollback: rollback.into(),
            expected_outcome: expected_outcome.into(),
            requires_boundary_yield: true,
        }
    }

    pub fn is_complete(&self) -> bool {
        !self.rationale.trim().is_empty()
            && !self.planned_steps.is_empty()
            && self
                .planned_steps
                .iter()
                .all(|step| !step.trim().is_empty())
            && !self.risks.is_empty()
            && self.risks.iter().all(|risk| !risk.trim().is_empty())
            && !self.rollback.trim().is_empty()
            && !self.expected_outcome.trim().is_empty()
            && self.requires_boundary_yield
    }
}

#[derive(Debug, Clone)]
pub struct AmbientAutomationMapping {
    pub intent_origin: IntentOrigin,
    pub autonomy_level: AutonomyLevel,
    pub task_event_source: TaskEventSource,
    pub trigger_source: TriggerSource,
    pub subscription: Subscription,
    pub schedule_policy: AmbientSchedulePolicy,
    pub delivery_mode: AmbientDeliveryMode,
    pub requires_automation_activity_ledger: bool,
    pub requires_gbrain_memory_receipts: bool,
    pub requires_worker_heartbeat_receipt: bool,
    pub required_task_event_kinds: Vec<&'static str>,
    pub forbidden_surfaces: Vec<ForbiddenAmbientSurface>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AmbientMappingError {
    WrongOrigin,
    WrongAutonomyLevel,
    WrongEventSource,
    WrongTriggerSource,
    MissingAutomationLedger,
    MissingGbrainReceiptPolicy,
    MissingHeartbeatReceiptPolicy,
    MissingBoundaryYield,
    UsesForbiddenAmbientSurface,
}

pub fn ambient_to_automation_mapping(
    kind: AmbientWorkKind,
    schedule: AmbientScheduleInput,
) -> AmbientAutomationMapping {
    let intent_origin = match kind {
        AmbientWorkKind::InternalMaintenance => IntentOrigin::System,
        AmbientWorkKind::ExternalDirective | AmbientWorkKind::UserFollowUp => {
            IntentOrigin::Automation
        }
    };

    AmbientAutomationMapping {
        intent_origin,
        autonomy_level: AutonomyLevel::ScheduledWorker,
        task_event_source: TaskEventSource::Automation,
        trigger_source: TriggerSource::Schedule,
        subscription: Subscription::Schedule(schedule.into_subscription()),
        schedule_policy: AmbientSchedulePolicy::default(),
        delivery_mode: AmbientDeliveryMode::QueuedScheduledWorker,
        requires_automation_activity_ledger: true,
        requires_gbrain_memory_receipts: true,
        requires_worker_heartbeat_receipt: true,
        required_task_event_kinds: vec![
            "task_started",
            "checkpoint",
            "boundary_yield",
            "memory_write",
            "warning",
            "task_finished",
        ],
        forbidden_surfaces: vec![
            ForbiddenAmbientSurface::SecondScheduler,
            ForbiddenAmbientSurface::AmbientTriggerSource,
            ForbiddenAmbientSurface::AmbientJsonCanonicalState,
            ForbiddenAmbientSurface::AmbientSessionRegistry,
            ForbiddenAmbientSurface::PermissionBypass,
        ],
    }
}

pub fn classify_delivery(
    active_session: bool,
    directive_due: bool,
    spawn_requested: bool,
) -> AmbientDeliveryMode {
    if active_session && directive_due {
        AmbientDeliveryMode::ExistingSessionSoftInterrupt
    } else if spawn_requested && directive_due {
        AmbientDeliveryMode::NewAutomationRun
    } else {
        AmbientDeliveryMode::QueuedScheduledWorker
    }
}

pub fn validate_ambient_mapping(
    mapping: &AmbientAutomationMapping,
) -> Result<(), AmbientMappingError> {
    if !matches!(
        mapping.intent_origin,
        IntentOrigin::Automation | IntentOrigin::System
    ) {
        return Err(AmbientMappingError::WrongOrigin);
    }
    if mapping.autonomy_level != AutonomyLevel::ScheduledWorker {
        return Err(AmbientMappingError::WrongAutonomyLevel);
    }
    if mapping.task_event_source != TaskEventSource::Automation {
        return Err(AmbientMappingError::WrongEventSource);
    }
    if mapping.trigger_source != TriggerSource::Schedule {
        return Err(AmbientMappingError::WrongTriggerSource);
    }
    if !mapping.requires_automation_activity_ledger {
        return Err(AmbientMappingError::MissingAutomationLedger);
    }
    if !mapping.requires_gbrain_memory_receipts {
        return Err(AmbientMappingError::MissingGbrainReceiptPolicy);
    }
    if !mapping.requires_worker_heartbeat_receipt {
        return Err(AmbientMappingError::MissingHeartbeatReceiptPolicy);
    }
    if !mapping
        .required_task_event_kinds
        .contains(&"boundary_yield")
    {
        return Err(AmbientMappingError::MissingBoundaryYield);
    }
    if !mapping
        .forbidden_surfaces
        .contains(&ForbiddenAmbientSurface::SecondScheduler)
        || !mapping
            .forbidden_surfaces
            .contains(&ForbiddenAmbientSurface::AmbientTriggerSource)
        || !mapping
            .forbidden_surfaces
            .contains(&ForbiddenAmbientSurface::AmbientJsonCanonicalState)
        || !mapping
            .forbidden_surfaces
            .contains(&ForbiddenAmbientSurface::AmbientSessionRegistry)
        || !mapping
            .forbidden_surfaces
            .contains(&ForbiddenAmbientSurface::PermissionBypass)
    {
        return Err(AmbientMappingError::UsesForbiddenAmbientSurface);
    }

    Ok(())
}

impl AmbientScheduleInput {
    fn into_subscription(self) -> ScheduleSubscription {
        match self {
            Self::Cron { cron } => ScheduleSubscription {
                cron: Some(cron),
                every: None,
            },
            Self::Every { every } => ScheduleSubscription {
                cron: None,
                every: Some(every),
            },
        }
    }
}

#[cfg(test)]
#[path = "ambient_mapping_tests.rs"]
mod ambient_mapping_tests;
