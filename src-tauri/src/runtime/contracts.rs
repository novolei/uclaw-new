//! M1-T1 — `IntentSpec` / `TaskSpec` / `TaskEvent` and supporting enums.
//!
//! This module defines the **runtime contracts** that the Agent OS v2
//! kernel revolves around. Each type is `Serialize + Deserialize` because
//! every IntentSpec ends up in `thread_goals` (V42), every TaskEvent ends
//! up in `task_events_rollout` (V44) + `~/.uclaw/sessions/rollout-<TS>-<UUID>.jsonl`,
//! and every TaskSpec is checkpointed across restarts.
//!
//! Nothing here is wired into the agent loop yet — M1-T2 is the first PR
//! that consumes these types. See
//! `docs/adr/2026-05-20-uclaw-agent-platform-north-star.md` for the
//! end-to-end design.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ────────────────────────────────────────────────────────────────────────
// Autonomy ladder + risk classification
// ────────────────────────────────────────────────────────────────────────

/// Autonomy ladder per ADR §"Autonomy Ladder L0–L6".
///
/// Each level corresponds to how independently an agent can act before
/// hitting a human boundary. `RiskClass::High` automatically caps the
/// effective level at [`AutonomyLevel::SupervisedTask`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// L0 — chat assist; agent only answers or proposes.
    ChatAssist,
    /// L1 — agent prepares actions; user approves each step.
    AssistedAction,
    /// L2 — bounded task with frequent checkpoints; user oversees.
    SupervisedTask,
    /// L3 — delegated task; agent runs to completion, pausing only at
    ///   declared human boundaries.
    DelegatedTask,
    /// L4 — scheduled worker; trigger / schedule wakes the agent to
    ///   execute a workflow.
    ScheduledWorker,
    /// L5 — agent team; multiple agents collaborate under a coordinator.
    AgentTeam,
    /// L6 — distributed cluster; work routes across local / remote
    ///   workers and machines.
    DistributedCluster,
}

impl AutonomyLevel {
    /// Map to the integer rung for ordering and storage.
    pub const fn rung(self) -> u8 {
        match self {
            Self::ChatAssist => 0,
            Self::AssistedAction => 1,
            Self::SupervisedTask => 2,
            Self::DelegatedTask => 3,
            Self::ScheduledWorker => 4,
            Self::AgentTeam => 5,
            Self::DistributedCluster => 6,
        }
    }

    /// Apply the policy cap for a given risk class.
    ///
    /// High-risk tasks are clamped to L2 (supervised); restricted tasks
    /// are clamped to L1 (each step approved). Low/Medium pass through.
    pub fn cap_for_risk(self, risk: RiskClass) -> Self {
        match risk {
            RiskClass::Low | RiskClass::Medium => self,
            RiskClass::High if self.rung() > Self::SupervisedTask.rung() => Self::SupervisedTask,
            RiskClass::High => self,
            RiskClass::Restricted if self.rung() > Self::AssistedAction.rung() => {
                Self::AssistedAction
            }
            RiskClass::Restricted => self,
        }
    }
}

/// Risk classification per ADR §"Hook policies and risk classes".
///
/// Determines the autonomy cap (see [`AutonomyLevel::cap_for_risk`]) and
/// which `HookBus` events fire for audit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskClass {
    Low,
    Medium,
    High,
    Restricted,
}

// ────────────────────────────────────────────────────────────────────────
// IntentSpec — what the user / trigger wants done
// ────────────────────────────────────────────────────────────────────────

/// Where the intent originated.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentOrigin {
    /// Direct chat input from the user.
    Chat,
    /// Triggered by an automation rule.
    Automation,
    /// Triggered via an instant-messaging channel binding.
    Im,
    /// Triggered by a peer agent in a team.
    Team,
    /// Triggered by a peer agent in a cluster.
    Cluster,
    /// Triggered by uclaw itself (e.g. proactive scenarios, cron).
    System,
}

/// A constraint the user attached to the intent (e.g. cost ceiling,
/// time-of-day window). Free-form `key`/`value` until M1-T1 surfaces a
/// proper enum (deferred — most constraints today are budget + schedule).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Constraint {
    pub key: String,
    pub value: String,
}

/// A reference to a piece of context the intent wants pinned into the
/// task's context window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextRef {
    pub source: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// Request for a specific capability (tool / skill / plugin).
/// The capability mesh resolves these to concrete bindings at task time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityQuery {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub tags: BTreeMap<String, String>,
}

/// A typed user goal — the "what" without the "how".
///
/// Produced by chat input, automation triggers, IM bindings, etc. Consumed
/// by the planner / capability mesh to produce one or more `TaskSpec`s.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentSpec {
    /// Stable id (typically UUID v4 string).
    pub id: String,
    /// Where the intent originated.
    pub origin: IntentOrigin,
    /// One-line user goal in their words.
    pub user_goal: String,
    /// Concrete completion criteria the agent should treat as done-DEF.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_criteria: Vec<String>,
    /// Hard constraints (budget, deadline, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub constraints: Vec<Constraint>,
    /// What autonomy the user is asking for — capped by risk class.
    pub autonomy_target: AutonomyLevel,
    /// How risky the intent looks (pre-execution classification).
    pub risk_class: RiskClass,
    /// Context the user wants pinned in.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub context_refs: Vec<ContextRef>,
    /// Capabilities the user explicitly requested (e.g. "use browser",
    /// "use the deep-research skill").
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requested_capabilities: Vec<CapabilityQuery>,
}

// ────────────────────────────────────────────────────────────────────────
// TaskSpec — the executable form of an IntentSpec
// ────────────────────────────────────────────────────────────────────────

/// Reference to a plan that produced (or supplied) this task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanRef {
    pub plan_id: String,
    pub step_id: String,
}

/// Per-task policy — distilled from the intent's autonomy + risk + the
/// capability profile's defaults. Determines hook-bus event coverage and
/// the approval gating depth.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PolicySpec {
    /// Effective autonomy after `RiskClass` cap is applied.
    pub effective_autonomy: AutonomyLevel,
    /// Whether the task needs explicit per-step approval.
    pub require_step_approval: bool,
    /// Tool-call rules to apply. Free-form id of a `tool_permission_rules`
    /// row (V14). Empty = default profile.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_permission_rule_ids: Vec<String>,
}

/// Resource budget — every dimension is opt-in. `None` = no limit on that
/// axis (subject to top-level config caps).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetSpec {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_input_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_output_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_wallclock_seconds: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_cost_usd_micros: Option<u64>,
}

/// What format the task is expected to produce.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutputContract {
    /// Free-form natural-language reply.
    FreeText,
    /// JSON conforming to a named schema.
    JsonSchema { schema_id: String },
    /// Markdown document.
    Markdown,
    /// One or more files written into the workspace.
    Workspace,
    /// No structured output expected (e.g. fire-and-forget side-effects).
    Side {
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
    },
}

/// When the task may checkpoint (saves state for resume).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointPolicy {
    /// Checkpoint at every model turn boundary.
    PerTurn,
    /// Checkpoint at major task phase boundaries (planner-defined).
    PerPhase,
    /// Checkpoint when the agent yields to a human boundary.
    OnHumanBoundary,
    /// No checkpointing — task must complete in one run (e.g. dry-run).
    Never,
}

/// The executable form of an [`IntentSpec`]. One intent may resolve to
/// multiple task specs (e.g. plan with 5 steps → 5 task specs).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskSpec {
    /// Stable id (UUID v4 string).
    pub id: String,
    /// Parent intent.
    pub intent_id: String,
    /// One-line task goal (may be different from intent goal — e.g.
    /// plan step description).
    pub goal: String,
    /// Optional pointer back to the plan that produced this task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_ref: Option<PlanRef>,
    pub policy: PolicySpec,
    pub budget: BudgetSpec,
    /// Stable id of the capability profile (from `capability_profiles`).
    pub capability_profile: String,
    pub output_contract: OutputContract,
    pub checkpoint_policy: CheckpointPolicy,
}

// ────────────────────────────────────────────────────────────────────────
// TaskEvent — observable events emitted while a task runs
// ────────────────────────────────────────────────────────────────────────

/// Which domain emitted the event. Mirrors `harness::case::HarnessSubject`
/// — M1-T3 will unify the two enums by promoting this one to be the
/// canonical source-dimension across the codebase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskEventSource {
    AgentLoop,
    Browser,
    Tools,
    Skills,
    Plugins,
    Permissions,
    Hooks,
    Memory,
    Gbrain,
    Tasks,
    Coordinator,
    Prompts,
    /// Automation runtime (cron / triggers / IM bindings).
    Automation,
}

/// Per-turn token accounting. The six fields cover Anthropic + OpenAI +
/// Gemini's reasoning models without losing fidelity.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenUsage {
    pub input_tokens: u32,
    /// Tokens served from the provider's prompt cache (cheaper).
    pub cached_input_tokens: u32,
    pub output_tokens: u32,
    /// Internal reasoning tokens (Claude extended thinking, o1, ...).
    pub reasoning_output_tokens: u32,
    pub total_tokens: u32,
    /// Estimated cost in 0.000001 USD units (avoid float drift in storage).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_usd_micros: Option<u64>,
}

/// Outcome of a permission request.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionDecision {
    Granted,
    Denied,
    Deferred,
}

/// Direction of a context-window operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextOp {
    Read,
    Write,
    Pinned,
    Released,
}

/// All observable events a task may emit.
///
/// Stored as a tagged enum (serde `tag = "kind"`) so the rollout JSONL +
/// `task_events_rollout` SQLite table can index on the kind string. Each
/// variant carries a precise timestamp + the source domain so cross-domain
/// traces (one chat → one browser → one automation event) can be
/// correlated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TaskEvent {
    TaskStarted {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        intent_id: String,
    },
    ModelTurn {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        provider: String,
        model: String,
        token_usage: TokenUsage,
    },
    ToolCall {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        tool_name: String,
        input_ref: String,
    },
    ToolResult {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        tool_name: String,
        output_ref: String,
        ok: bool,
    },
    PermissionRequested {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        request_id: String,
        reason: String,
    },
    PermissionDecided {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        request_id: String,
        decision: PermissionDecision,
    },
    ContextAccess {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        op: ContextOp,
        context_ref: ContextRef,
    },
    MemoryWrite {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        target: String,
        artifact_ref: String,
    },
    MemoryRecall {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        target: String,
        artifact_ref: String,
    },
    Checkpoint {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        checkpoint_ref: String,
    },
    BoundaryYield {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        /// Free-form reason (e.g. "high-risk tool call needs approval").
        reason: String,
    },
    /// Cost overrun, deadline reached, model error, ... — anything the
    /// task wants to surface without immediately failing.
    Warning {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        code: String,
        message: String,
    },
    TaskFinished {
        ts: String,
        source: TaskEventSource,
        task_id: String,
        verdict: TaskVerdict,
    },
}

/// Terminal outcome of a task.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum TaskVerdict {
    Completed {
        #[serde(skip_serializing_if = "Option::is_none")]
        summary: Option<String>,
    },
    Cancelled {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    Failed {
        error_code: String,
        message: String,
    },
    BudgetExhausted {
        dimension: String,
    },
}

impl TaskEvent {
    /// Snake-case discriminator string. Used as the `kind` column on
    /// `task_events_rollout` (V44, M1-T5).
    pub fn kind(&self) -> &'static str {
        match self {
            Self::TaskStarted { .. } => "task_started",
            Self::ModelTurn { .. } => "model_turn",
            Self::ToolCall { .. } => "tool_call",
            Self::ToolResult { .. } => "tool_result",
            Self::PermissionRequested { .. } => "permission_requested",
            Self::PermissionDecided { .. } => "permission_decided",
            Self::ContextAccess { .. } => "context_access",
            Self::MemoryWrite { .. } => "memory_write",
            Self::MemoryRecall { .. } => "memory_recall",
            Self::Checkpoint { .. } => "checkpoint",
            Self::BoundaryYield { .. } => "boundary_yield",
            Self::Warning { .. } => "warning",
            Self::TaskFinished { .. } => "task_finished",
        }
    }

    /// The source domain. Convenient for cross-domain rollout queries.
    pub fn source(&self) -> TaskEventSource {
        match self {
            Self::TaskStarted { source, .. }
            | Self::ModelTurn { source, .. }
            | Self::ToolCall { source, .. }
            | Self::ToolResult { source, .. }
            | Self::PermissionRequested { source, .. }
            | Self::PermissionDecided { source, .. }
            | Self::ContextAccess { source, .. }
            | Self::MemoryWrite { source, .. }
            | Self::MemoryRecall { source, .. }
            | Self::Checkpoint { source, .. }
            | Self::BoundaryYield { source, .. }
            | Self::Warning { source, .. }
            | Self::TaskFinished { source, .. } => *source,
        }
    }

    /// The owning task id.
    pub fn task_id(&self) -> &str {
        match self {
            Self::TaskStarted { task_id, .. }
            | Self::ModelTurn { task_id, .. }
            | Self::ToolCall { task_id, .. }
            | Self::ToolResult { task_id, .. }
            | Self::PermissionRequested { task_id, .. }
            | Self::PermissionDecided { task_id, .. }
            | Self::ContextAccess { task_id, .. }
            | Self::MemoryWrite { task_id, .. }
            | Self::MemoryRecall { task_id, .. }
            | Self::Checkpoint { task_id, .. }
            | Self::BoundaryYield { task_id, .. }
            | Self::Warning { task_id, .. }
            | Self::TaskFinished { task_id, .. } => task_id,
        }
    }

    /// ISO-8601 timestamp.
    pub fn ts(&self) -> &str {
        match self {
            Self::TaskStarted { ts, .. }
            | Self::ModelTurn { ts, .. }
            | Self::ToolCall { ts, .. }
            | Self::ToolResult { ts, .. }
            | Self::PermissionRequested { ts, .. }
            | Self::PermissionDecided { ts, .. }
            | Self::ContextAccess { ts, .. }
            | Self::MemoryWrite { ts, .. }
            | Self::MemoryRecall { ts, .. }
            | Self::Checkpoint { ts, .. }
            | Self::BoundaryYield { ts, .. }
            | Self::Warning { ts, .. }
            | Self::TaskFinished { ts, .. } => ts,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── AutonomyLevel ───────────────────────────────────────────────

    #[test]
    fn autonomy_rungs_monotonic() {
        let levels = [
            AutonomyLevel::ChatAssist,
            AutonomyLevel::AssistedAction,
            AutonomyLevel::SupervisedTask,
            AutonomyLevel::DelegatedTask,
            AutonomyLevel::ScheduledWorker,
            AutonomyLevel::AgentTeam,
            AutonomyLevel::DistributedCluster,
        ];
        for w in levels.windows(2) {
            assert!(w[0].rung() < w[1].rung(), "{:?} < {:?}", w[0], w[1]);
        }
    }

    #[test]
    fn risk_caps_high_at_supervised() {
        assert_eq!(
            AutonomyLevel::DistributedCluster.cap_for_risk(RiskClass::High),
            AutonomyLevel::SupervisedTask
        );
        assert_eq!(
            AutonomyLevel::DelegatedTask.cap_for_risk(RiskClass::High),
            AutonomyLevel::SupervisedTask
        );
        // High doesn't elevate below-the-cap levels.
        assert_eq!(
            AutonomyLevel::ChatAssist.cap_for_risk(RiskClass::High),
            AutonomyLevel::ChatAssist
        );
    }

    #[test]
    fn risk_caps_restricted_at_assisted() {
        assert_eq!(
            AutonomyLevel::DelegatedTask.cap_for_risk(RiskClass::Restricted),
            AutonomyLevel::AssistedAction
        );
        // Restricted caps everything above L1 down to L1.
        assert_eq!(
            AutonomyLevel::SupervisedTask.cap_for_risk(RiskClass::Restricted),
            AutonomyLevel::AssistedAction
        );
    }

    #[test]
    fn risk_low_and_medium_pass_through() {
        for level in [
            AutonomyLevel::DelegatedTask,
            AutonomyLevel::AgentTeam,
            AutonomyLevel::DistributedCluster,
        ] {
            assert_eq!(level.cap_for_risk(RiskClass::Low), level);
            assert_eq!(level.cap_for_risk(RiskClass::Medium), level);
        }
    }

    // ── Serde round-trips ──────────────────────────────────────────

    #[test]
    fn intent_spec_roundtrip() {
        let intent = IntentSpec {
            id: "01HJZ8QK9X".into(),
            origin: IntentOrigin::Chat,
            user_goal: "summarize today's emails".into(),
            acceptance_criteria: vec!["≤ 200 words".into()],
            constraints: vec![Constraint {
                key: "deadline".into(),
                value: "2026-05-20T18:00:00Z".into(),
            }],
            autonomy_target: AutonomyLevel::DelegatedTask,
            risk_class: RiskClass::Low,
            context_refs: vec![ContextRef {
                source: "mail".into(),
                id: "thread/42".into(),
                label: None,
            }],
            requested_capabilities: vec![CapabilityQuery {
                kind: "tool".into(),
                name: Some("read_mail".into()),
                tags: Default::default(),
            }],
        };
        let json = serde_json::to_string(&intent).expect("serialize");
        let parsed: IntentSpec = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, intent);
        // Spot-check the camelCase wire shape.
        assert!(json.contains("\"userGoal\""));
        assert!(json.contains("\"autonomyTarget\""));
        assert!(json.contains("\"riskClass\":\"low\""));
    }

    #[test]
    fn task_spec_roundtrip_with_skipped_none_plan_ref() {
        let task = TaskSpec {
            id: "task-001".into(),
            intent_id: "intent-001".into(),
            goal: "Step 2: fetch the data".into(),
            plan_ref: None,
            policy: PolicySpec {
                effective_autonomy: AutonomyLevel::SupervisedTask,
                require_step_approval: false,
                tool_permission_rule_ids: vec![],
            },
            budget: BudgetSpec {
                max_input_tokens: Some(40_000),
                max_output_tokens: Some(2_000),
                max_wallclock_seconds: Some(120),
                max_cost_usd_micros: None,
            },
            capability_profile: "default".into(),
            output_contract: OutputContract::Markdown,
            checkpoint_policy: CheckpointPolicy::PerTurn,
        };
        let json = serde_json::to_string(&task).expect("serialize");
        let parsed: TaskSpec = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, task);
        // None plan_ref should not appear in the wire form.
        assert!(!json.contains("planRef"));
    }

    #[test]
    fn task_event_kind_strings() {
        let started = TaskEvent::TaskStarted {
            ts: "2026-05-20T12:00:00Z".into(),
            source: TaskEventSource::AgentLoop,
            task_id: "task-001".into(),
            intent_id: "intent-001".into(),
        };
        assert_eq!(started.kind(), "task_started");
        assert_eq!(started.source(), TaskEventSource::AgentLoop);
        assert_eq!(started.task_id(), "task-001");
        assert_eq!(started.ts(), "2026-05-20T12:00:00Z");
    }

    #[test]
    fn task_event_finished_with_completed_verdict_roundtrips() {
        let e = TaskEvent::TaskFinished {
            ts: "2026-05-20T12:30:00Z".into(),
            source: TaskEventSource::AgentLoop,
            task_id: "task-001".into(),
            verdict: TaskVerdict::Completed {
                summary: Some("done".into()),
            },
        };
        let json = serde_json::to_string(&e).expect("serialize");
        let parsed: TaskEvent = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, e);
        assert!(json.contains("\"kind\":\"task_finished\""));
        assert!(json.contains("\"outcome\":\"completed\""));
    }

    #[test]
    fn task_event_budget_exhausted_carries_dimension() {
        let e = TaskEvent::TaskFinished {
            ts: "2026-05-20T12:30:00Z".into(),
            source: TaskEventSource::AgentLoop,
            task_id: "task-001".into(),
            verdict: TaskVerdict::BudgetExhausted {
                dimension: "max_output_tokens".into(),
            },
        };
        let json = serde_json::to_string(&e).expect("serialize");
        let parsed: TaskEvent = serde_json::from_str(&json).expect("parse");
        assert_eq!(parsed, e);
        assert!(json.contains("budget_exhausted"));
    }

    #[test]
    fn task_event_kind_covers_all_thirteen_variants() {
        let ts = "2026-05-20T12:00:00Z".to_string();
        let src = TaskEventSource::AgentLoop;
        let tid = "task-001".to_string();
        let variants: Vec<TaskEvent> = vec![
            TaskEvent::TaskStarted {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                intent_id: "i".into(),
            },
            TaskEvent::ModelTurn {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                provider: "anthropic".into(),
                model: "claude-sonnet-4-6".into(),
                token_usage: TokenUsage::default(),
            },
            TaskEvent::ToolCall {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                tool_name: "read".into(),
                input_ref: "in/1".into(),
            },
            TaskEvent::ToolResult {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                tool_name: "read".into(),
                output_ref: "out/1".into(),
                ok: true,
            },
            TaskEvent::PermissionRequested {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                request_id: "r/1".into(),
                reason: "shell".into(),
            },
            TaskEvent::PermissionDecided {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                request_id: "r/1".into(),
                decision: PermissionDecision::Granted,
            },
            TaskEvent::ContextAccess {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                op: ContextOp::Read,
                context_ref: ContextRef {
                    source: "mail".into(),
                    id: "thread/42".into(),
                    label: None,
                },
            },
            TaskEvent::MemoryWrite {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                target: "gbrain".into(),
                artifact_ref: "art/1".into(),
            },
            TaskEvent::MemoryRecall {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                target: "gbrain".into(),
                artifact_ref: "art/1".into(),
            },
            TaskEvent::Checkpoint {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                checkpoint_ref: "ck/1".into(),
            },
            TaskEvent::BoundaryYield {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                reason: "needs approval".into(),
            },
            TaskEvent::Warning {
                ts: ts.clone(),
                source: src,
                task_id: tid.clone(),
                code: "cost_warning".into(),
                message: "approaching cap".into(),
            },
            TaskEvent::TaskFinished {
                ts,
                source: src,
                task_id: tid,
                verdict: TaskVerdict::Completed { summary: None },
            },
        ];
        assert_eq!(variants.len(), 13);
        let kinds: Vec<&'static str> = variants.iter().map(|v| v.kind()).collect();
        // All distinct + snake-case shape.
        let mut sorted = kinds.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 13, "all 13 variants must have distinct kind() strings");
        for k in &kinds {
            assert!(k.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "kind() must be snake_case: {k}");
        }
    }

    #[test]
    fn token_usage_default_serializes_zeros() {
        let t = TokenUsage::default();
        let json = serde_json::to_string(&t).unwrap();
        // costUsdMicros is Option<u64>, None → skipped.
        assert!(!json.contains("costUsdMicros"));
        assert!(json.contains("\"inputTokens\":0"));
        assert!(json.contains("\"reasoningOutputTokens\":0"));
    }
}
