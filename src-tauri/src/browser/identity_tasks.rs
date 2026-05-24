//! Live browser identity task tracking and revocation drain state.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::browser::session_state::{BrowserTaskRun, BrowserTaskStatus};

pub const DEFAULT_IDENTITY_REVOCATION_DRAIN_MS: i64 = 5_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentityActiveTaskSummary {
    pub profile_id: String,
    pub run_id: String,
    pub session_id: String,
    pub task: String,
    pub status: BrowserTaskStatus,
    pub started_at_ms: i64,
    pub updated_at_ms: i64,
    pub drain_deadline_ms: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrowserIdentityRevocationDecision {
    NotRevoked,
    Draining { drain_deadline_ms: i64 },
    CheckpointRequired { drain_deadline_ms: i64 },
}

#[derive(Debug, Clone)]
struct BrowserIdentityActiveTaskRecord {
    profile_id: String,
    run_id: String,
    session_id: String,
    task: String,
    status: BrowserTaskStatus,
    started_at_ms: i64,
    updated_at_ms: i64,
    drain_deadline_ms: Option<i64>,
}

#[derive(Debug, Clone)]
struct BrowserIdentityRevocationState {
    requested_at_ms: i64,
    drain_deadline_ms: i64,
}

#[derive(Debug, Default)]
struct BrowserIdentityTaskRegistryState {
    active_tasks: HashMap<String, BrowserIdentityActiveTaskRecord>,
    revocations: HashMap<String, BrowserIdentityRevocationState>,
}

#[derive(Debug, Clone, Default)]
pub struct BrowserIdentityTaskRegistry {
    state: Arc<Mutex<BrowserIdentityTaskRegistryState>>,
}

impl BrowserIdentityTaskRegistry {
    pub fn register(
        &self,
        profile_id: impl Into<String>,
        run: &BrowserTaskRun,
    ) -> BrowserIdentityTaskRegistration {
        let profile_id = profile_id.into();
        let now = now_ms();
        let mut state = self.state.lock().expect("identity task registry poisoned");
        let drain_deadline_ms = state
            .revocations
            .get(&profile_id)
            .map(|revocation| revocation.drain_deadline_ms);
        state.active_tasks.insert(
            run.run_id.clone(),
            BrowserIdentityActiveTaskRecord {
                profile_id,
                run_id: run.run_id.clone(),
                session_id: run.session_id.clone(),
                task: run.task.clone(),
                status: run.status.clone(),
                started_at_ms: now,
                updated_at_ms: now,
                drain_deadline_ms,
            },
        );

        BrowserIdentityTaskRegistration {
            registry: self.clone(),
            run_id: run.run_id.clone(),
        }
    }

    pub fn update_status(&self, run_id: &str, status: BrowserTaskStatus) {
        let mut state = self.state.lock().expect("identity task registry poisoned");
        if let Some(record) = state.active_tasks.get_mut(run_id) {
            record.status = status;
            record.updated_at_ms = now_ms();
        }
    }

    pub fn active_tasks(&self) -> Vec<BrowserIdentityActiveTaskSummary> {
        let state = self.state.lock().expect("identity task registry poisoned");
        summaries_from_records(state.active_tasks.values())
    }

    pub fn active_tasks_for_profile(
        &self,
        profile_id: &str,
    ) -> Vec<BrowserIdentityActiveTaskSummary> {
        let state = self.state.lock().expect("identity task registry poisoned");
        summaries_from_records(
            state
                .active_tasks
                .values()
                .filter(|record| record.profile_id == profile_id),
        )
    }

    pub fn active_task_count(&self) -> usize {
        let state = self.state.lock().expect("identity task registry poisoned");
        state.active_tasks.len()
    }

    pub fn begin_revocation(
        &self,
        profile_id: &str,
        drain_window_ms: i64,
    ) -> Vec<BrowserIdentityActiveTaskSummary> {
        let now = now_ms();
        let drain_deadline_ms = now + drain_window_ms.max(0);
        let mut state = self.state.lock().expect("identity task registry poisoned");
        state.revocations.insert(
            profile_id.to_string(),
            BrowserIdentityRevocationState {
                requested_at_ms: now,
                drain_deadline_ms,
            },
        );
        for record in state.active_tasks.values_mut() {
            if record.profile_id == profile_id {
                record.drain_deadline_ms = Some(drain_deadline_ms);
                record.updated_at_ms = now;
            }
        }
        summaries_from_records(
            state
                .active_tasks
                .values()
                .filter(|record| record.profile_id == profile_id),
        )
    }

    pub fn revocation_decision(&self, profile_id: &str) -> BrowserIdentityRevocationDecision {
        let state = self.state.lock().expect("identity task registry poisoned");
        let Some(revocation) = state.revocations.get(profile_id) else {
            return BrowserIdentityRevocationDecision::NotRevoked;
        };
        let _requested_at_ms = revocation.requested_at_ms;
        if now_ms() >= revocation.drain_deadline_ms {
            BrowserIdentityRevocationDecision::CheckpointRequired {
                drain_deadline_ms: revocation.drain_deadline_ms,
            }
        } else {
            BrowserIdentityRevocationDecision::Draining {
                drain_deadline_ms: revocation.drain_deadline_ms,
            }
        }
    }

    fn unregister(&self, run_id: &str) {
        let mut state = self.state.lock().expect("identity task registry poisoned");
        state.active_tasks.remove(run_id);
    }
}

pub struct BrowserIdentityTaskRegistration {
    registry: BrowserIdentityTaskRegistry,
    run_id: String,
}

impl Drop for BrowserIdentityTaskRegistration {
    fn drop(&mut self) {
        self.registry.unregister(&self.run_id);
    }
}

fn summaries_from_records<'a>(
    records: impl Iterator<Item = &'a BrowserIdentityActiveTaskRecord>,
) -> Vec<BrowserIdentityActiveTaskSummary> {
    let mut summaries = records
        .map(|record| BrowserIdentityActiveTaskSummary {
            profile_id: record.profile_id.clone(),
            run_id: record.run_id.clone(),
            session_id: record.session_id.clone(),
            task: record.task.clone(),
            status: record.status.clone(),
            started_at_ms: record.started_at_ms,
            updated_at_ms: record.updated_at_ms,
            drain_deadline_ms: record.drain_deadline_ms,
        })
        .collect::<Vec<_>>();
    summaries.sort_by(|a, b| a.started_at_ms.cmp(&b.started_at_ms));
    summaries
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(run_id: &str, status: BrowserTaskStatus) -> BrowserTaskRun {
        BrowserTaskRun {
            run_id: run_id.to_string(),
            session_id: "session-1".to_string(),
            task: "Use the dashboard".to_string(),
            status,
            steps: Vec::new(),
        }
    }

    #[test]
    fn tracks_active_tasks_until_registration_drops() {
        let registry = BrowserIdentityTaskRegistry::default();

        let registration = registry.register("auth-1", &run("run-1", BrowserTaskStatus::Running));

        assert_eq!(registry.active_task_count(), 1);
        let tasks = registry.active_tasks_for_profile("auth-1");
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].run_id, "run-1");
        assert_eq!(tasks[0].status, BrowserTaskStatus::Running);

        drop(registration);

        assert_eq!(registry.active_task_count(), 0);
    }

    #[test]
    fn revocation_marks_active_tasks_with_drain_deadline() {
        let registry = BrowserIdentityTaskRegistry::default();
        let _registration = registry.register("auth-1", &run("run-1", BrowserTaskStatus::Running));

        let tasks = registry.begin_revocation("auth-1", 5_000);

        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].run_id, "run-1");
        assert!(tasks[0].drain_deadline_ms.is_some());
        assert!(matches!(
            registry.revocation_decision("auth-1"),
            BrowserIdentityRevocationDecision::Draining { .. }
        ));
    }

    #[test]
    fn zero_drain_requires_checkpoint_at_next_boundary() {
        let registry = BrowserIdentityTaskRegistry::default();
        let _registration = registry.register("auth-1", &run("run-1", BrowserTaskStatus::Running));

        registry.begin_revocation("auth-1", 0);

        assert!(matches!(
            registry.revocation_decision("auth-1"),
            BrowserIdentityRevocationDecision::CheckpointRequired { .. }
        ));
    }

    #[test]
    fn status_updates_are_reflected_in_summaries() {
        let registry = BrowserIdentityTaskRegistry::default();
        let _registration = registry.register("auth-1", &run("run-1", BrowserTaskStatus::Running));

        registry.update_status("run-1", BrowserTaskStatus::PausedCheckpointed);

        let tasks = registry.active_tasks_for_profile("auth-1");
        assert_eq!(tasks[0].status, BrowserTaskStatus::PausedCheckpointed);
    }
}
