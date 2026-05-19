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
    pub graders: Vec<crate::harness::graders::HarnessGraderSpec>,
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
