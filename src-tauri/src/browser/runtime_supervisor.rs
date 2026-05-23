//! Browser Runtime Supervisor Phase 1 shell.
//!
//! This module adds supervised state, deadline, doctor, artifact, and
//! projection metadata around the current local Chromium lane. It intentionally
//! does not replace browser action execution yet.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::browser::context_manager::BrowserContextManager;
use crate::browser::provider::{BrowserProviderReadiness, LOCAL_CHROMIUM_PROVIDER_ID};
use crate::browser::runtime_contracts::{
    browser_provider_capability_card, is_allowed_browser_runtime_transition, BrowserIdentityMode,
    BrowserIdentityProjection, BrowserProviderCapabilityCard, BrowserRuntimeProjection,
    BrowserRuntimeState, BrowserRuntimeTransition, BrowserStartupDoctorProjection,
    BrowserTaskBoundaryProjection, BrowserTaskBoundaryStatus, BrowserTaskEventName,
    BrowserWorldProjectionSummary, StartupDoctorStatus,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeDeadlineProfile {
    pub startup_ms: u64,
    pub connect_ms: u64,
    pub action_ms: u64,
    pub wait_ms: u64,
    pub network_idle_ms: u64,
    pub first_frame_ms: u64,
    pub no_output_heartbeat_ms: u64,
}

impl BrowserRuntimeDeadlineProfile {
    pub const fn local_chromium_defaults() -> Self {
        Self {
            startup_ms: 60_000,
            connect_ms: 15_000,
            action_ms: 30_000,
            wait_ms: 10_000,
            network_idle_ms: 15_000,
            first_frame_ms: 8_000,
            no_output_heartbeat_ms: 5_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeSessionSummary {
    pub session_id: String,
    pub provider_id: String,
    pub state: BrowserRuntimeState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub degraded_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_artifact_pack_ref: Option<String>,
    pub last_state_change_at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeDoctorOutcome {
    pub provider_id: String,
    pub session_id: String,
    pub readiness: BrowserProviderReadiness,
    pub status: StartupDoctorStatus,
    pub runtime_state: BrowserRuntimeState,
    pub active_contexts: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remediation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_name: Option<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeDegradation {
    pub provider_id: String,
    pub session_id: String,
    pub code: String,
    pub message: String,
    pub event_name: &'static str,
    pub artifact_recommended: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeArtifactPack {
    pub artifact_ref: String,
    pub provider_id: String,
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    pub reason: String,
    pub event_name: &'static str,
    pub created_at_ms: i64,
}

pub struct BrowserRuntimeSupervisor {
    provider_id: String,
    deadlines: BrowserRuntimeDeadlineProfile,
    sessions: HashMap<String, BrowserRuntimeSessionSummary>,
}

impl BrowserRuntimeSupervisor {
    pub fn new_local_chromium() -> Self {
        Self {
            provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
            deadlines: BrowserRuntimeDeadlineProfile::local_chromium_defaults(),
            sessions: HashMap::new(),
        }
    }

    pub fn with_deadlines(mut self, deadlines: BrowserRuntimeDeadlineProfile) -> Self {
        self.deadlines = deadlines;
        self
    }

    pub fn deadlines(&self) -> BrowserRuntimeDeadlineProfile {
        self.deadlines
    }

    pub fn provider_card(&self) -> Option<&'static BrowserProviderCapabilityCard> {
        browser_provider_capability_card(&self.provider_id)
    }

    pub fn ensure_session(
        &mut self,
        session_id: impl Into<String>,
        now_ms: i64,
    ) -> &BrowserRuntimeSessionSummary {
        let session_id = session_id.into();
        self.sessions
            .entry(session_id.clone())
            .or_insert_with(|| BrowserRuntimeSessionSummary {
                session_id,
                provider_id: self.provider_id.clone(),
                state: BrowserRuntimeState::Starting,
                active_task_id: None,
                degraded_reason: None,
                last_artifact_pack_ref: None,
                last_state_change_at_ms: now_ms,
            })
    }

    pub fn session(&self, session_id: &str) -> Option<&BrowserRuntimeSessionSummary> {
        self.sessions.get(session_id)
    }

    pub fn transition_session(
        &mut self,
        session_id: &str,
        to: BrowserRuntimeState,
        now_ms: i64,
    ) -> Result<BrowserRuntimeTransition, BrowserRuntimeDegradation> {
        let provider_id = self.provider_id.clone();
        let session = self
            .sessions
            .entry(session_id.to_string())
            .or_insert_with(|| BrowserRuntimeSessionSummary {
                session_id: session_id.to_string(),
                provider_id: provider_id.clone(),
                state: BrowserRuntimeState::Starting,
                active_task_id: None,
                degraded_reason: None,
                last_artifact_pack_ref: None,
                last_state_change_at_ms: now_ms,
            });
        let from = session.state;
        if is_allowed_browser_runtime_transition(from, to) {
            session.state = to;
            session.degraded_reason = None;
            session.last_state_change_at_ms = now_ms;
            return Ok(BrowserRuntimeTransition { from, to });
        }

        let degradation = BrowserRuntimeDegradation {
            provider_id,
            session_id: session_id.to_string(),
            code: "invalid_state_transition".to_string(),
            message: format!("Invalid browser runtime transition from {from:?} to {to:?}."),
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: false,
        };
        session.state = BrowserRuntimeState::Degraded;
        session.degraded_reason = Some(degradation.message.clone());
        session.last_state_change_at_ms = now_ms;
        Err(degradation)
    }

    pub fn mark_action_started(
        &mut self,
        session_id: &str,
        task_id: impl Into<String>,
        now_ms: i64,
    ) -> Result<BrowserRuntimeTransition, BrowserRuntimeDegradation> {
        self.ensure_session(session_id.to_string(), now_ms);
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.active_task_id = Some(task_id.into());
        }
        self.transition_session(session_id, BrowserRuntimeState::Acting, now_ms)
    }

    pub fn mark_action_finished(
        &mut self,
        session_id: &str,
        now_ms: i64,
    ) -> Result<BrowserRuntimeTransition, BrowserRuntimeDegradation> {
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.active_task_id = None;
        }
        self.transition_session(session_id, BrowserRuntimeState::Idle, now_ms)
    }

    pub fn classify_action_elapsed(
        &mut self,
        session_id: &str,
        elapsed_ms: u64,
        now_ms: i64,
    ) -> Option<BrowserRuntimeDegradation> {
        if elapsed_ms <= self.deadlines.action_ms {
            return None;
        }
        let degradation = self.degradation(
            session_id,
            "action_deadline_exceeded",
            format!(
                "Browser action exceeded deadline: {elapsed_ms}ms > {}ms.",
                self.deadlines.action_ms
            ),
            BrowserTaskEventName::RuntimeArtifactPackCreated,
            true,
        );
        self.apply_degradation(session_id, &degradation, now_ms);
        Some(degradation)
    }

    pub fn classify_no_output_heartbeat(
        &mut self,
        session_id: &str,
        elapsed_ms: u64,
        now_ms: i64,
    ) -> Option<BrowserRuntimeDegradation> {
        if elapsed_ms <= self.deadlines.no_output_heartbeat_ms {
            return None;
        }
        let degradation = self.degradation(
            session_id,
            "no_output_heartbeat_missed",
            format!(
                "Browser runtime missed no-output heartbeat: {elapsed_ms}ms > {}ms.",
                self.deadlines.no_output_heartbeat_ms
            ),
            BrowserTaskEventName::RuntimeHeartbeatMissed,
            true,
        );
        self.apply_degradation(session_id, &degradation, now_ms);
        Some(degradation)
    }

    pub fn doctor_from_active_contexts(
        &self,
        session_id: &str,
        active_sessions: &[String],
    ) -> BrowserRuntimeDoctorOutcome {
        let active = active_sessions.iter().any(|active| active == session_id);
        if active {
            BrowserRuntimeDoctorOutcome {
                provider_id: self.provider_id.clone(),
                session_id: session_id.to_string(),
                readiness: BrowserProviderReadiness::Ready,
                status: StartupDoctorStatus::Ready,
                runtime_state: BrowserRuntimeState::Ready,
                active_contexts: active_sessions.len(),
                detail: Some("Local Chromium context is active.".to_string()),
                remediation: None,
                event_name: Some(BrowserTaskEventName::StartupDoctorCheck.as_str()),
            }
        } else {
            BrowserRuntimeDoctorOutcome {
                provider_id: self.provider_id.clone(),
                session_id: session_id.to_string(),
                readiness: BrowserProviderReadiness::NeedsSetup,
                status: StartupDoctorStatus::Deferred,
                runtime_state: BrowserRuntimeState::Stopped,
                active_contexts: active_sessions.len(),
                detail: Some(
                    "No active local Chromium context exists for this session.".to_string(),
                ),
                remediation: Some(
                    "Launch or resume a browser task to create a supervised context.".to_string(),
                ),
                event_name: Some(BrowserTaskEventName::StartupDoctorCheck.as_str()),
            }
        }
    }

    pub async fn doctor_from_context_manager(
        &self,
        session_id: &str,
        context_manager: &BrowserContextManager,
    ) -> BrowserRuntimeDoctorOutcome {
        let active_sessions = context_manager.list_active_sessions().await;
        self.doctor_from_active_contexts(session_id, &active_sessions)
    }

    pub fn artifact_pack(
        &mut self,
        session_id: &str,
        task_id: Option<String>,
        reason: impl Into<String>,
        event_name: BrowserTaskEventName,
        created_at_ms: i64,
    ) -> BrowserRuntimeArtifactPack {
        let reason = reason.into();
        let artifact_ref = format!(
            "browser-runtime://{}/{}/{}",
            session_id,
            reason.replace(|ch: char| !ch.is_ascii_alphanumeric(), "-"),
            created_at_ms
        );
        let pack = BrowserRuntimeArtifactPack {
            artifact_ref,
            provider_id: self.provider_id.clone(),
            session_id: session_id.to_string(),
            task_id,
            reason,
            event_name: event_name.as_str(),
            created_at_ms,
        };
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.last_artifact_pack_ref = Some(pack.artifact_ref.clone());
        }
        pack
    }

    pub fn projection_for_session(
        &self,
        session_id: &str,
        doctor: &BrowserRuntimeDoctorOutcome,
    ) -> BrowserWorldProjectionSummary {
        let session = self.sessions.get(session_id);
        BrowserWorldProjectionSummary {
            startup_doctor: BrowserStartupDoctorProjection {
                status: doctor.status,
                last_check_at: None,
                current_check: Some("local_chromium_context".to_string()),
                failure_code: doctor
                    .remediation
                    .as_ref()
                    .map(|_| "local_chromium_context_missing".to_string()),
                detail_visible: doctor.status != StartupDoctorStatus::Ready,
            },
            runtime: BrowserRuntimeProjection {
                state: session
                    .map(|session| session.state)
                    .unwrap_or(doctor.runtime_state),
                provider_id: Some(self.provider_id.clone()),
                active_session_id: Some(session_id.to_string()),
                active_task_id: session.and_then(|session| session.active_task_id.clone()),
                degraded_reason: session.and_then(|session| session.degraded_reason.clone()),
                last_artifact_pack_ref: session
                    .and_then(|session| session.last_artifact_pack_ref.clone()),
            },
            identity: BrowserIdentityProjection {
                mode: BrowserIdentityMode::Isolated,
                authorized: false,
                last_used_at: None,
                active_task_ids: session
                    .and_then(|session| session.active_task_id.clone())
                    .into_iter()
                    .collect(),
                revoked: false,
            },
            task_boundary: BrowserTaskBoundaryProjection {
                task_id: session.and_then(|session| session.active_task_id.clone()),
                status: task_boundary_status(session),
                reason: session.and_then(|session| session.degraded_reason.clone()),
                checkpoint_ref: None,
            },
        }
    }

    fn apply_degradation(
        &mut self,
        session_id: &str,
        degradation: &BrowserRuntimeDegradation,
        now_ms: i64,
    ) {
        self.ensure_session(session_id.to_string(), now_ms);
        if let Some(session) = self.sessions.get_mut(session_id) {
            session.state = BrowserRuntimeState::Degraded;
            session.degraded_reason = Some(degradation.message.clone());
            session.last_state_change_at_ms = now_ms;
        }
    }

    fn degradation(
        &self,
        session_id: &str,
        code: &str,
        message: String,
        event_name: BrowserTaskEventName,
        artifact_recommended: bool,
    ) -> BrowserRuntimeDegradation {
        BrowserRuntimeDegradation {
            provider_id: self.provider_id.clone(),
            session_id: session_id.to_string(),
            code: code.to_string(),
            message,
            event_name: event_name.as_str(),
            artifact_recommended,
        }
    }
}

fn task_boundary_status(
    session: Option<&BrowserRuntimeSessionSummary>,
) -> BrowserTaskBoundaryStatus {
    match session {
        Some(session) if session.state == BrowserRuntimeState::Degraded => {
            BrowserTaskBoundaryStatus::PausedCheckpointed
        }
        Some(session) if session.active_task_id.is_some() => BrowserTaskBoundaryStatus::Running,
        Some(_) => BrowserTaskBoundaryStatus::None,
        None => BrowserTaskBoundaryStatus::None,
    }
}

#[cfg(test)]
#[path = "runtime_supervisor_tests.rs"]
mod tests;
