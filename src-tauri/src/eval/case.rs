use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessSubject {
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
pub struct HarnessCase {
    pub id: String,
    pub subject: HarnessSubject,
    pub title: String,
    pub prompt: String,
    pub setup: Vec<HarnessFixture>,
    pub policy: HarnessPolicy,
    pub budgets: HarnessBudget,
    pub assertions: Vec<HarnessAssertion>,
    pub graders: Vec<crate::eval::graders::HarnessGraderSpec>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessFixture {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub config: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HarnessPolicy {
    pub permission_mode: String,
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    #[serde(default)]
    pub allow_network: bool,
    #[serde(default)]
    pub allow_memory_writes: bool,
}

impl Default for HarnessPolicy {
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
pub struct HarnessBudget {
    pub max_steps: u32,
    pub max_seconds: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
}

impl Default for HarnessBudget {
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
pub struct HarnessAssertion {
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
    fn harness_case_serializes_subject_and_camelcase() {
        let case = HarnessCase {
            id: "case-1".into(),
            subject: HarnessSubject::Gbrain,
            title: "Recall structured fact".into(),
            prompt: "Recall Ryan's favorite language".into(),
            setup: vec![HarnessFixture {
                id: "fixture-1".into(),
                kind: "memory_seed".into(),
                config: json!({ "fact": "Ryan likes Rust" }),
            }],
            policy: HarnessPolicy {
                allow_memory_writes: true,
                ..HarnessPolicy::default()
            },
            budgets: HarnessBudget {
                max_steps: 8,
                max_seconds: 30,
                max_tokens: Some(4000),
            },
            assertions: vec![HarnessAssertion {
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
// HarnessSubject and runtime::contracts::TaskEventSource were defined as
// parallel 12-variant enums by M1-T1 (PR #304). This module is the source
// of truth for the harness; runtime/contracts.rs is the source of truth
// for the rest of the system.
//
// Per `uclaw-upgrade-implementation-plan.md` M1-T3 + `docs/adr/2026-05-20-
// uclaw-agent-platform-north-star.md` §"Cross-domain rollout", the runtime
// enum is the canonical one. HarnessSubject is kept as the input vocabulary
// for harness *cases* (because that's what their YAML defines) but converts
// 1:1 into TaskEventSource at the moment the harness emits an event for the
// rollout writer (M1-T5).
//
// EvalEvent → TaskEvent is *not* a 1:1 mapping (EvalEvent carries
// case_id; TaskEvent carries task_id + intent_id + source) so its
// conversion is deferred to M1-T5 once the rollout writer can supply
// the bridging context.

impl From<HarnessSubject> for crate::runtime::contracts::TaskEventSource {
    fn from(s: HarnessSubject) -> Self {
        use crate::runtime::contracts::TaskEventSource as T;
        match s {
            HarnessSubject::AgentLoop   => T::AgentLoop,
            HarnessSubject::Browser     => T::Browser,
            HarnessSubject::Tools       => T::Tools,
            HarnessSubject::Skills      => T::Skills,
            HarnessSubject::Plugins     => T::Plugins,
            HarnessSubject::Permissions => T::Permissions,
            HarnessSubject::Hooks       => T::Hooks,
            HarnessSubject::Memory      => T::Memory,
            HarnessSubject::Gbrain      => T::Gbrain,
            HarnessSubject::Tasks       => T::Tasks,
            HarnessSubject::Coordinator => T::Coordinator,
            HarnessSubject::Prompts     => T::Prompts,
        }
    }
}

pub trait TaskEventSourceHarnessExt {
    /// Reverse direction: collapse `TaskEventSource` → `HarnessSubject`.
    /// `TaskEventSource::Automation` has no harness equivalent (harness
    /// cases predate the unified runtime), so it maps to
    /// `HarnessSubject::Tasks` as a documented fallback.
    fn to_harness_subject(self) -> HarnessSubject;
}

impl TaskEventSourceHarnessExt for crate::runtime::contracts::TaskEventSource {
    fn to_harness_subject(self) -> HarnessSubject {
        use crate::runtime::contracts::TaskEventSource as T;
        match self {
            T::AgentLoop   => HarnessSubject::AgentLoop,
            T::Browser     => HarnessSubject::Browser,
            T::Tools       => HarnessSubject::Tools,
            T::Skills      => HarnessSubject::Skills,
            T::Plugins     => HarnessSubject::Plugins,
            T::Permissions => HarnessSubject::Permissions,
            T::Hooks       => HarnessSubject::Hooks,
            T::Memory      => HarnessSubject::Memory,
            T::Gbrain      => HarnessSubject::Gbrain,
            T::Tasks       => HarnessSubject::Tasks,
            T::Coordinator => HarnessSubject::Coordinator,
            T::Prompts     => HarnessSubject::Prompts,
            T::Automation  => HarnessSubject::Tasks, // no harness equivalent yet
        }
    }
}

#[cfg(test)]
mod m1_t3_bridge_tests {
    use super::*;
    use crate::runtime::contracts::TaskEventSource;

    /// Every HarnessSubject value round-trips through TaskEventSource and back.
    /// Covers all 12 enum variants explicitly via a known table; if a new
    /// variant lands on either side, this test must be updated alongside.
    #[test]
    fn harness_subject_round_trips_through_task_event_source() {
        let cases: [(HarnessSubject, TaskEventSource); 12] = [
            (HarnessSubject::AgentLoop,   TaskEventSource::AgentLoop),
            (HarnessSubject::Browser,     TaskEventSource::Browser),
            (HarnessSubject::Tools,       TaskEventSource::Tools),
            (HarnessSubject::Skills,      TaskEventSource::Skills),
            (HarnessSubject::Plugins,     TaskEventSource::Plugins),
            (HarnessSubject::Permissions, TaskEventSource::Permissions),
            (HarnessSubject::Hooks,       TaskEventSource::Hooks),
            (HarnessSubject::Memory,      TaskEventSource::Memory),
            (HarnessSubject::Gbrain,      TaskEventSource::Gbrain),
            (HarnessSubject::Tasks,       TaskEventSource::Tasks),
            (HarnessSubject::Coordinator, TaskEventSource::Coordinator),
            (HarnessSubject::Prompts,     TaskEventSource::Prompts),
        ];
        for (subj, expected_src) in cases {
            let src: TaskEventSource = subj.into();
            assert_eq!(src, expected_src, "{subj:?} → {src:?} mismatch");
            // And the reverse direction recovers the subject.
            assert_eq!(src.to_harness_subject(), subj);
        }
    }

    /// TaskEventSource::Automation has no harness equivalent today; it
    /// must collapse to Tasks (documented fallback). When M1-T4 ships
    /// automation adapters, this expectation will likely change.
    #[test]
    fn automation_source_collapses_to_tasks_subject() {
        let src = TaskEventSource::Automation;
        assert_eq!(src.to_harness_subject(), HarnessSubject::Tasks);
    }
}
