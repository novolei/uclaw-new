use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalSubject {
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCase {
    pub id: String,
    pub subject: EvalSubject,
    pub title: String,
    pub prompt: String,
    pub setup: Vec<EvalFixture>,
    pub policy: EvalPolicy,
    pub budgets: EvalBudget,
    pub assertions: Vec<EvalAssertion>,
    pub graders: Vec<crate::eval::graders::EvalGraderSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalFixture {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalPolicy {
    pub permission_mode: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_memory_writes: bool,
}

impl Default for EvalPolicy {
    fn default() -> Self {
        Self {
            permission_mode: "ask".to_string(),
            allowed_tools: Vec::new(),
            allow_network: false,
            allow_memory_writes: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalBudget {
    pub max_steps: u32,
    pub max_seconds: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl Default for EvalBudget {
    fn default() -> Self {
        Self {
            max_steps: 20,
            max_seconds: 120,
            max_tokens: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalAssertion {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub expected: Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn eval_case_serializes_subject_and_camelcase() {
        let case = EvalCase {
            id: "case-1".into(),
            subject: EvalSubject::Gbrain,
            title: "Recall structured fact".into(),
            prompt: "Recall Ryan's favorite language".into(),
            setup: vec![EvalFixture {
                id: "fixture-1".into(),
                kind: "memory_seed".into(),
                config: json!({ "fact": "Ryan likes Rust" }),
            }],
            policy: EvalPolicy {
                allow_memory_writes: true,
                ..EvalPolicy::default()
            },
            budgets: EvalBudget {
                max_steps: 8,
                max_seconds: 30,
                max_tokens: Some(4000),
            },
            assertions: vec![EvalAssertion {
                id: "assert-1".into(),
                kind: "contains_fact".into(),
                expected: json!({ "fact": "Rust" }),
            }],
            graders: vec![],
        };

        let value = serde_json::to_value(&case).unwrap();
        assert_eq!(value["subject"], "gbrain");
        assert_eq!(value["policy"]["permissionMode"], "ask");
        assert_eq!(value["policy"]["allowMemoryWrites"], true);
        assert_eq!(value["budgets"]["maxTokens"], 4000);
    }
}

// ────────────────────────────────────────────────────────────────────────
// M1-T3 — bridge to runtime::contracts::TaskEventSource
//
// EvalSubject and runtime::contracts::TaskEventSource were defined as
// parallel 12-variant enums by M1-T1 (PR #304). This module is the source
// of truth for the eval runner; runtime/contracts.rs is the source of truth
// for the rest of the system.
//
// Per `uclaw-upgrade-implementation-plan.md` M1-T3 + `docs/adr/2026-05-20-
// uclaw-agent-platform-north-star.md` §"Cross-domain rollout", the runtime
// enum is the canonical one. EvalSubject is kept as the input vocabulary
// for eval *cases* (because that's what their YAML defines) but converts
// 1:1 into TaskEventSource at the moment the eval runner emits an event for
// the rollout writer (M1-T5).
//
// EvalEvent → TaskEvent is *not* a 1:1 mapping (EvalEvent carries
// case_id; TaskEvent carries task_id + intent_id + source) so its
// conversion is deferred to M1-T5 once the rollout writer can supply
// the bridging context.

impl From<EvalSubject> for crate::runtime::contracts::TaskEventSource {
    fn from(s: EvalSubject) -> Self {
        use crate::runtime::contracts::TaskEventSource as T;
        match s {
            EvalSubject::AgentLoop   => T::AgentLoop,
            EvalSubject::Browser     => T::Browser,
            EvalSubject::Tools       => T::Tools,
            EvalSubject::Skills      => T::Skills,
            EvalSubject::Plugins     => T::Plugins,
            EvalSubject::Permissions => T::Permissions,
            EvalSubject::Hooks       => T::Hooks,
            EvalSubject::Memory      => T::Memory,
            EvalSubject::Gbrain      => T::Gbrain,
            EvalSubject::Tasks       => T::Tasks,
            EvalSubject::Coordinator => T::Coordinator,
            EvalSubject::Prompts     => T::Prompts,
        }
    }
}

pub trait TaskEventSourceEvalExt {
    /// Reverse direction: collapse `TaskEventSource` → `EvalSubject`.
    /// `TaskEventSource::Automation` has no eval equivalent (eval
    /// cases predate the unified runtime), so it maps to
    /// `EvalSubject::Tasks` as a documented fallback.
    fn to_eval_subject(self) -> EvalSubject;
}

impl TaskEventSourceEvalExt for crate::runtime::contracts::TaskEventSource {
    fn to_eval_subject(self) -> EvalSubject {
        use crate::runtime::contracts::TaskEventSource as T;
        match self {
            T::AgentLoop   => EvalSubject::AgentLoop,
            T::Browser     => EvalSubject::Browser,
            T::Tools       => EvalSubject::Tools,
            T::Skills      => EvalSubject::Skills,
            T::Plugins     => EvalSubject::Plugins,
            T::Permissions => EvalSubject::Permissions,
            T::Hooks       => EvalSubject::Hooks,
            T::Memory      => EvalSubject::Memory,
            T::Gbrain      => EvalSubject::Gbrain,
            T::Tasks       => EvalSubject::Tasks,
            T::Coordinator => EvalSubject::Coordinator,
            T::Prompts     => EvalSubject::Prompts,
            T::Automation  => EvalSubject::Tasks, // no eval equivalent yet
        }
    }
}

#[cfg(test)]
mod m1_t3_bridge_tests {
    use super::*;
    use crate::runtime::contracts::TaskEventSource;

    /// Every EvalSubject value round-trips through TaskEventSource and back.
    /// Covers all 12 enum variants explicitly via a known table; if a new
    /// variant lands on either side, this test must be updated alongside.
    #[test]
    fn eval_subject_round_trips_through_task_event_source() {
        let cases: [(EvalSubject, TaskEventSource); 12] = [
            (EvalSubject::AgentLoop,   TaskEventSource::AgentLoop),
            (EvalSubject::Browser,     TaskEventSource::Browser),
            (EvalSubject::Tools,       TaskEventSource::Tools),
            (EvalSubject::Skills,      TaskEventSource::Skills),
            (EvalSubject::Plugins,     TaskEventSource::Plugins),
            (EvalSubject::Permissions, TaskEventSource::Permissions),
            (EvalSubject::Hooks,       TaskEventSource::Hooks),
            (EvalSubject::Memory,      TaskEventSource::Memory),
            (EvalSubject::Gbrain,      TaskEventSource::Gbrain),
            (EvalSubject::Tasks,       TaskEventSource::Tasks),
            (EvalSubject::Coordinator, TaskEventSource::Coordinator),
            (EvalSubject::Prompts,     TaskEventSource::Prompts),
        ];
        for (subj, expected_src) in cases {
            let src: TaskEventSource = subj.into();
            assert_eq!(src, expected_src, "{subj:?} → {src:?} mismatch");
            // And the reverse direction recovers the subject.
            assert_eq!(src.to_eval_subject(), subj);
        }
    }

    /// TaskEventSource::Automation has no eval equivalent today; it
    /// must collapse to Tasks (documented fallback). When M1-T4 ships
    /// automation adapters, this expectation will likely change.
    #[test]
    fn automation_source_collapses_to_tasks_subject() {
        let src = TaskEventSource::Automation;
        assert_eq!(src.to_eval_subject(), EvalSubject::Tasks);
    }
}
