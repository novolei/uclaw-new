//! Worker spec types.

use serde::{Deserialize, Serialize};

use crate::runtime::contracts::{AutonomyLevel, RiskClass, WorkerId};

/// Coarse role tag for a sub-agent. The orchestrator picks a role
/// based on the parent task's plan; the role drives the worker's
/// system prompt template + tool exposure defaults.
///
/// Open-ended via `Custom(String)` so plugins can declare new roles.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "role", content = "subrole", rename_all = "snake_case")]
pub enum WorkerRole {
    /// Gathers information / answers research questions.
    Researcher,
    /// Reviews work product (code, docs, plans).
    Reviewer,
    /// Implements concrete changes.
    Implementor,
    /// Combines findings from multiple workers into a single output.
    Synthesizer,
    /// Watches long-running processes / external signals.
    Monitor,
    Custom(String),
}

impl WorkerRole {
    pub fn id(&self) -> String {
        match self {
            Self::Researcher => "researcher".into(),
            Self::Reviewer => "reviewer".into(),
            Self::Implementor => "implementor".into(),
            Self::Synthesizer => "synthesizer".into(),
            Self::Monitor => "monitor".into(),
            Self::Custom(s) => format!("custom:{s}"),
        }
    }
}

/// Lifecycle status of a worker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkerStatus {
    /// Spec created; not yet scheduled.
    Pending,
    /// Spawned and currently running.
    Running,
    /// Paused awaiting external event (e.g. user approval at a hook).
    Paused,
    /// Finished successfully.
    Completed,
    /// Finished with failure or cancellation.
    Failed,
}

impl WorkerStatus {
    /// `true` if the status is terminal — orchestrator can drop the
    /// worker handle.
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed)
    }
}

/// Why a worker ended. Captured on transition Pending/Running → Completed/Failed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkerTerminationReason {
    /// Worker reported a result successfully.
    Success { summary: Option<String> },
    /// Worker exhausted its budget.
    BudgetExhausted { dimension: String },
    /// Worker hit its max_turns ceiling.
    MaxTurnsReached,
    /// Worker was cancelled by parent / user.
    Cancelled { reason: Option<String> },
    /// Worker errored out.
    Errored { error_code: String, message: String },
}

/// Scope limits applied to a worker. The orchestrator enforces these
/// at spawn time (max_turns / max_cost_micros / allowed_tool_kinds)
/// and at runtime (per-turn).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerScope {
    /// Hard cap on number of LLM turns the worker may run. None = no
    /// cap (only budget gating applies).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_turns: Option<u32>,
    /// Total budget in micro-USD. None = inherit parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_cost_micros: Option<u64>,
    /// Tool kinds the worker is allowed to call (string ids matching
    /// `ToolEntry.kind` in the M3-T1 registry). Empty = all kinds
    /// allowed.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_tool_kinds: Vec<String>,
    /// Worker's effective risk class (used to cap its autonomy).
    pub risk_class: RiskClass,
    /// Maximum autonomy the worker may operate at, BEFORE risk capping.
    pub autonomy_target: AutonomyLevel,
}

impl WorkerScope {
    pub fn restricted_assist() -> Self {
        Self {
            max_turns: Some(5),
            max_cost_micros: None,
            allowed_tool_kinds: Vec::new(),
            risk_class: RiskClass::Restricted,
            autonomy_target: AutonomyLevel::AssistedAction,
        }
    }

    pub fn supervised(max_turns: u32) -> Self {
        Self {
            max_turns: Some(max_turns),
            max_cost_micros: None,
            allowed_tool_kinds: Vec::new(),
            risk_class: RiskClass::Medium,
            autonomy_target: AutonomyLevel::SupervisedTask,
        }
    }

    /// Effective autonomy after risk capping (per `AutonomyLevel::cap_for_risk`).
    pub fn effective_autonomy(&self) -> AutonomyLevel {
        self.autonomy_target.cap_for_risk(self.risk_class)
    }

    /// `true` if the given tool kind is allowed under this scope.
    /// Empty `allowed_tool_kinds` means all kinds allowed.
    pub fn allows_tool_kind(&self, kind: &str) -> bool {
        self.allowed_tool_kinds.is_empty()
            || self.allowed_tool_kinds.iter().any(|k| k == kind)
    }
}

/// Worker spec — the immutable description the orchestrator hands to
/// the runtime to spawn a worker.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerSpec {
    pub id: WorkerId,
    pub role: WorkerRole,
    pub goal: String,
    pub scope: WorkerScope,
    /// Parent task id that spawned this worker.
    pub parent_task_id: String,
    /// RFC 3339 timestamp at which the spec was created.
    pub created_at: String,
}

impl WorkerSpec {
    pub fn new(
        id: WorkerId,
        role: WorkerRole,
        goal: impl Into<String>,
        scope: WorkerScope,
        parent_task_id: impl Into<String>,
        created_at: impl Into<String>,
    ) -> Self {
        Self {
            id,
            role,
            goal: goal.into(),
            scope,
            parent_task_id: parent_task_id.into(),
            created_at: created_at.into(),
        }
    }
}

/// Lifecycle events the orchestrator emits. M5 HookBus subscribers
/// (e.g. AuditLogger) consume these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case")]
pub enum WorkerLifecycleEvent {
    WorkerSpawned {
        worker_id: WorkerId,
        role: WorkerRole,
    },
    WorkerStatusChanged {
        worker_id: WorkerId,
        from: WorkerStatus,
        to: WorkerStatus,
    },
    WorkerCompleted {
        worker_id: WorkerId,
        reason: WorkerTerminationReason,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wid(s: &str) -> WorkerId {
        WorkerId::new(s)
    }

    // ── WorkerRole ──────────────────────────────────────────────────

    #[test]
    fn role_ids_distinct_and_custom_prefixed() {
        let roles = [
            WorkerRole::Researcher,
            WorkerRole::Reviewer,
            WorkerRole::Implementor,
            WorkerRole::Synthesizer,
            WorkerRole::Monitor,
            WorkerRole::Custom("planner".into()),
        ];
        let mut ids: Vec<_> = roles.iter().map(|r| r.id()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 6);
        assert!(WorkerRole::Custom("planner".into()).id().starts_with("custom:"));
    }

    #[test]
    fn role_serde_tag_snake_case_with_subrole() {
        let v = serde_json::to_value(WorkerRole::Researcher).unwrap();
        assert_eq!(v["role"], "researcher");
        let v = serde_json::to_value(WorkerRole::Custom("x".into())).unwrap();
        assert_eq!(v["role"], "custom");
        assert_eq!(v["subrole"], "x");
    }

    // ── WorkerStatus ────────────────────────────────────────────────

    #[test]
    fn is_terminal_correctly_classified() {
        assert!(!WorkerStatus::Pending.is_terminal());
        assert!(!WorkerStatus::Running.is_terminal());
        assert!(!WorkerStatus::Paused.is_terminal());
        assert!(WorkerStatus::Completed.is_terminal());
        assert!(WorkerStatus::Failed.is_terminal());
    }

    // ── WorkerTerminationReason ────────────────────────────────────

    #[test]
    fn termination_reason_serde_each_variant() {
        let cases = [
            WorkerTerminationReason::Success {
                summary: Some("done".into()),
            },
            WorkerTerminationReason::BudgetExhausted {
                dimension: "input_tokens".into(),
            },
            WorkerTerminationReason::MaxTurnsReached,
            WorkerTerminationReason::Cancelled {
                reason: Some("user".into()),
            },
            WorkerTerminationReason::Errored {
                error_code: "E_TOOL_FAILED".into(),
                message: "shell exited 1".into(),
            },
        ];
        for r in cases {
            let json = serde_json::to_string(&r).unwrap();
            let back: WorkerTerminationReason = serde_json::from_str(&json).unwrap();
            assert_eq!(r, back);
        }
    }

    // ── WorkerScope ────────────────────────────────────────────────

    #[test]
    fn restricted_assist_presets() {
        let s = WorkerScope::restricted_assist();
        assert_eq!(s.max_turns, Some(5));
        assert_eq!(s.risk_class, RiskClass::Restricted);
        assert_eq!(s.autonomy_target, AutonomyLevel::AssistedAction);
        // Risk-capped autonomy = AssistedAction (Restricted caps at L1).
        assert_eq!(s.effective_autonomy(), AutonomyLevel::AssistedAction);
    }

    #[test]
    fn supervised_preset_caps_to_supervised() {
        let s = WorkerScope::supervised(20);
        assert_eq!(s.max_turns, Some(20));
        assert_eq!(s.risk_class, RiskClass::Medium);
        assert_eq!(s.effective_autonomy(), AutonomyLevel::SupervisedTask);
    }

    #[test]
    fn risk_caps_autonomy_to_supervised_for_high_target() {
        // L4 ScheduledWorker requested with High risk → caps to L2 SupervisedTask.
        let s = WorkerScope {
            max_turns: None,
            max_cost_micros: None,
            allowed_tool_kinds: Vec::new(),
            risk_class: RiskClass::High,
            autonomy_target: AutonomyLevel::ScheduledWorker,
        };
        assert_eq!(s.effective_autonomy(), AutonomyLevel::SupervisedTask);
    }

    #[test]
    fn allows_tool_kind_empty_allows_all() {
        let s = WorkerScope::supervised(10);
        assert!(s.allows_tool_kind("shell"));
        assert!(s.allows_tool_kind("anything"));
    }

    #[test]
    fn allows_tool_kind_with_explicit_list() {
        let mut s = WorkerScope::supervised(10);
        s.allowed_tool_kinds = vec!["builtin".into(), "memory".into()];
        assert!(s.allows_tool_kind("builtin"));
        assert!(s.allows_tool_kind("memory"));
        assert!(!s.allows_tool_kind("shell"));
    }

    // ── WorkerSpec ────────────────────────────────────────────────

    #[test]
    fn worker_spec_new() {
        let s = WorkerSpec::new(
            wid("w1"),
            WorkerRole::Researcher,
            "find docs about M2-D",
            WorkerScope::supervised(10),
            "task-parent",
            "2026-05-21T00:00:00Z",
        );
        assert_eq!(s.id.as_str(), "w1");
        assert_eq!(s.role, WorkerRole::Researcher);
        assert_eq!(s.parent_task_id, "task-parent");
    }

    #[test]
    fn worker_spec_serde_camel_case() {
        let s = WorkerSpec::new(
            wid("w1"),
            WorkerRole::Reviewer,
            "review M2-G PR",
            WorkerScope::supervised(5),
            "task-rev",
            "2026-05-21T00:00:00Z",
        );
        let v = serde_json::to_value(&s).unwrap();
        assert_eq!(v["parentTaskId"], "task-rev");
        assert_eq!(v["createdAt"], "2026-05-21T00:00:00Z");
        // WorkerId is transparent → bare string.
        assert_eq!(v["id"], "w1");
    }

    // ── WorkerLifecycleEvent ───────────────────────────────────────

    #[test]
    fn lifecycle_event_tag_snake_case() {
        let e = WorkerLifecycleEvent::WorkerSpawned {
            worker_id: wid("w1"),
            role: WorkerRole::Synthesizer,
        };
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(v["event"], "worker_spawned");
    }

    #[test]
    fn lifecycle_event_roundtrip() {
        let e = WorkerLifecycleEvent::WorkerCompleted {
            worker_id: wid("w1"),
            reason: WorkerTerminationReason::MaxTurnsReached,
        };
        let json = serde_json::to_string(&e).unwrap();
        let back: WorkerLifecycleEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
