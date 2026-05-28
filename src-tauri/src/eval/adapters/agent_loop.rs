use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::eval::adapters::{HarnessAdapter, AGENT_LOOP_ADAPTER_ID};
use crate::eval::case::{HarnessBudget, HarnessCase, HarnessPolicy, HarnessSubject};
use crate::eval::episode::HarnessVerdict;
use crate::eval::runtime::EvalRuntime;
use crate::eval::trace::EvalEvent;

pub const BUILTIN_AGENT_CONTROL_PLANE_CASES: &[&str] = &[
    include_str!("../cases/agent_loop/tool-result-pairing.json"),
    include_str!("../cases/agent_loop/permission-boundary.json"),
    include_str!("../cases/agent_loop/checkpoint-resume.json"),
    include_str!("../cases/agent_loop/tool-failure-completes.json"),
];

#[derive(Debug, Default, Clone)]
pub struct AgentLoopControlPlaneEvalAdapter;

impl HarnessAdapter for AgentLoopControlPlaneEvalAdapter {
    fn subject(&self) -> HarnessSubject {
        HarnessSubject::AgentLoop
    }

    fn adapter_id(&self) -> &'static str {
        AGENT_LOOP_ADAPTER_ID
    }
}

impl AgentLoopControlPlaneEvalAdapter {
    pub fn load_builtin_cases() -> Result<Vec<AgentControlPlaneCase>, serde_json::Error> {
        BUILTIN_AGENT_CONTROL_PLANE_CASES
            .iter()
            .map(|raw| serde_json::from_str(raw))
            .collect()
    }

    pub fn run_fixture_suite(
        &self,
        runtime: &EvalRuntime,
    ) -> anyhow::Result<AgentControlPlaneSuiteReport> {
        let cases = Self::load_builtin_cases()?;
        let traces = cases
            .iter()
            .map(AgentControlPlaneTrace::fixture_for_case)
            .collect::<Vec<_>>();
        self.run_suite(runtime, cases, traces)
    }

    pub fn run_suite(
        &self,
        runtime: &EvalRuntime,
        cases: Vec<AgentControlPlaneCase>,
        traces: Vec<AgentControlPlaneTrace>,
    ) -> anyhow::Result<AgentControlPlaneSuiteReport> {
        let mut run_ids = Vec::new();
        let mut scorecards = Vec::new();

        for case in cases {
            let trace = traces
                .iter()
                .find(|trace| trace.case_id == case.id)
                .cloned()
                .unwrap_or_else(|| AgentControlPlaneTrace::empty(&case.id));
            let harness_case = case.to_harness_case();
            let episode = runtime.start_episode(&harness_case);
            run_ids.push(episode.run_id.clone());
            record_trace(runtime, &episode.run_id, &trace);
            runtime.attach_json_artifact(
                &episode.run_id,
                "agent_control_plane_trace",
                &serde_json::to_value(&trace)?,
            )?;
            let scorecard = score_agent_control_plane_case(case, &trace);
            runtime.attach_json_artifact(
                &episode.run_id,
                "agent_control_plane_scorecard",
                &serde_json::to_value(&scorecard)?,
            )?;
            runtime.finish_episode(
                &episode.run_id,
                if scorecard.passed {
                    HarnessVerdict::Pass
                } else {
                    HarnessVerdict::Fail
                },
            );
            scorecards.push(scorecard);
        }

        let average_score = if scorecards.is_empty() {
            0.0
        } else {
            scorecards.iter().map(|card| card.score).sum::<f64>() / scorecards.len() as f64
        };
        Ok(AgentControlPlaneSuiteReport {
            passed: scorecards.iter().all(|card| card.passed),
            average_score,
            run_ids,
            scorecards,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentControlPlaneCase {
    pub id: String,
    pub title: String,
    pub prompt: String,
    #[serde(default)]
    pub required_tools: Vec<String>,
    #[serde(default)]
    pub require_tool_results: bool,
    #[serde(default)]
    pub require_permission_request: bool,
    #[serde(default)]
    pub forbid_permission_request: bool,
    #[serde(default)]
    pub require_checkpoint: bool,
    #[serde(default)]
    pub require_non_running_final_status: bool,
    #[serde(default)]
    pub min_model_turns: u32,
}

impl AgentControlPlaneCase {
    fn to_harness_case(&self) -> HarnessCase {
        HarnessCase {
            id: self.id.clone(),
            subject: HarnessSubject::AgentLoop,
            title: self.title.clone(),
            prompt: self.prompt.clone(),
            setup: Vec::new(),
            policy: HarnessPolicy {
                permission_mode: "ask".to_string(),
                allowed_tools: self.required_tools.clone(),
                allow_network: false,
                allow_memory_writes: false,
            },
            budgets: HarnessBudget {
                max_steps: 8,
                max_seconds: 60,
                max_tokens: Some(4000),
            },
            assertions: Vec::new(),
            graders: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentControlPlaneTrace {
    pub case_id: String,
    pub model_turns: Vec<AgentModelTurnProbe>,
    pub tool_events: Vec<AgentToolProbe>,
    pub permission_requests: Vec<AgentPermissionProbe>,
    pub checkpoints: Vec<String>,
    pub final_status: AgentLoopFinalStatus,
}

impl AgentControlPlaneTrace {
    fn empty(case_id: &str) -> Self {
        Self {
            case_id: case_id.to_string(),
            model_turns: Vec::new(),
            tool_events: Vec::new(),
            permission_requests: Vec::new(),
            checkpoints: Vec::new(),
            final_status: AgentLoopFinalStatus::Running,
        }
    }

    fn fixture_for_case(case: &AgentControlPlaneCase) -> Self {
        match case.id.as_str() {
            "agent_loop.permission_boundary" => Self {
                case_id: case.id.clone(),
                model_turns: vec![AgentModelTurnProbe::fixture("model-1")],
                tool_events: vec![AgentToolProbe::call("shell", "tool-1")],
                permission_requests: vec![AgentPermissionProbe {
                    request_id: "perm-1".into(),
                    reason: "shell requires explicit approval".into(),
                }],
                checkpoints: Vec::new(),
                final_status: AgentLoopFinalStatus::Blocked,
            },
            "agent_loop.checkpoint_resume" => Self {
                case_id: case.id.clone(),
                model_turns: vec![AgentModelTurnProbe::fixture("model-1")],
                tool_events: vec![
                    AgentToolProbe::call("browser_task", "tool-1"),
                    AgentToolProbe::result("browser_task", "tool-1", true),
                ],
                permission_requests: Vec::new(),
                checkpoints: vec!["checkpoint-1".into()],
                final_status: AgentLoopFinalStatus::Completed,
            },
            "agent_loop.tool_failure_completes" => Self {
                case_id: case.id.clone(),
                model_turns: vec![AgentModelTurnProbe::fixture("model-1")],
                tool_events: vec![
                    AgentToolProbe::call("browser_task", "tool-1"),
                    AgentToolProbe::result("browser_task", "tool-1", false),
                ],
                permission_requests: Vec::new(),
                checkpoints: Vec::new(),
                final_status: AgentLoopFinalStatus::Failed,
            },
            _ => Self {
                case_id: case.id.clone(),
                model_turns: vec![AgentModelTurnProbe::fixture("model-1")],
                tool_events: vec![
                    AgentToolProbe::call("web_fetch", "tool-1"),
                    AgentToolProbe::result("web_fetch", "tool-1", true),
                ],
                permission_requests: Vec::new(),
                checkpoints: Vec::new(),
                final_status: AgentLoopFinalStatus::Completed,
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentModelTurnProbe {
    pub model: String,
    #[serde(default)]
    pub prompt_tokens: u32,
    #[serde(default)]
    pub completion_tokens: u32,
}

impl AgentModelTurnProbe {
    fn fixture(model: &str) -> Self {
        Self {
            model: model.to_string(),
            prompt_tokens: 100,
            completion_tokens: 25,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentToolProbe {
    pub call_id: String,
    pub tool_name: String,
    pub phase: AgentToolPhase,
    #[serde(default)]
    pub ok: bool,
}

impl AgentToolProbe {
    fn call(tool_name: &str, call_id: &str) -> Self {
        Self {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            phase: AgentToolPhase::Call,
            ok: false,
        }
    }

    fn result(tool_name: &str, call_id: &str, ok: bool) -> Self {
        Self {
            call_id: call_id.to_string(),
            tool_name: tool_name.to_string(),
            phase: AgentToolPhase::Result,
            ok,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentToolPhase {
    Call,
    Result,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentPermissionProbe {
    pub request_id: String,
    pub reason: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLoopFinalStatus {
    Completed,
    Blocked,
    Failed,
    Cancelled,
    Running,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentControlPlaneSuiteReport {
    pub passed: bool,
    pub average_score: f64,
    pub run_ids: Vec<String>,
    pub scorecards: Vec<AgentControlPlaneScorecard>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentControlPlaneScorecard {
    pub case_id: String,
    pub title: String,
    pub passed: bool,
    pub score: f64,
    pub checks: Vec<AgentControlPlaneCheck>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentControlPlaneCheck {
    pub id: String,
    pub passed: bool,
    pub score: f64,
    pub message: String,
}

pub fn score_agent_control_plane_case(
    case: AgentControlPlaneCase,
    trace: &AgentControlPlaneTrace,
) -> AgentControlPlaneScorecard {
    let mut checks = Vec::new();

    checks.push(check(
        "model_turns",
        trace.model_turns.len() as u32 >= case.min_model_turns,
        format!(
            "expected >= {} model turns, got {}",
            case.min_model_turns,
            trace.model_turns.len()
        ),
    ));

    let called_tools = trace
        .tool_events
        .iter()
        .filter(|event| event.phase == AgentToolPhase::Call)
        .map(|event| event.tool_name.as_str())
        .collect::<HashSet<_>>();
    for tool in &case.required_tools {
        checks.push(check(
            format!("tool_called:{tool}"),
            called_tools.contains(tool.as_str()),
            format!("required tool call observed: {tool}"),
        ));
    }

    if case.require_tool_results {
        let unresolved = unresolved_tool_calls(trace);
        checks.push(check(
            "tool_results_paired",
            unresolved.is_empty(),
            format!("unresolved tool calls: {:?}", unresolved),
        ));
    }

    if case.require_permission_request {
        checks.push(check(
            "permission_requested",
            !trace.permission_requests.is_empty(),
            format!("permission requests: {}", trace.permission_requests.len()),
        ));
    }
    if case.forbid_permission_request {
        checks.push(check(
            "permission_not_requested",
            trace.permission_requests.is_empty(),
            format!("permission requests: {}", trace.permission_requests.len()),
        ));
    }

    if case.require_checkpoint {
        checks.push(check(
            "checkpoint_recorded",
            !trace.checkpoints.is_empty(),
            format!("checkpoints: {:?}", trace.checkpoints),
        ));
    }

    if case.require_non_running_final_status {
        checks.push(check(
            "final_status_closed",
            trace.final_status != AgentLoopFinalStatus::Running,
            format!("final_status={:?}", trace.final_status),
        ));
    }

    let score = if checks.is_empty() {
        0.0
    } else {
        checks.iter().map(|check| check.score).sum::<f64>() / checks.len() as f64
    };
    AgentControlPlaneScorecard {
        case_id: case.id,
        title: case.title,
        passed: checks.iter().all(|check| check.passed),
        score,
        checks,
    }
}

fn unresolved_tool_calls(trace: &AgentControlPlaneTrace) -> Vec<String> {
    let results = trace
        .tool_events
        .iter()
        .filter(|event| event.phase == AgentToolPhase::Result)
        .map(|event| event.call_id.as_str())
        .collect::<HashSet<_>>();
    trace
        .tool_events
        .iter()
        .filter(|event| event.phase == AgentToolPhase::Call)
        .filter(|event| !results.contains(event.call_id.as_str()))
        .map(|event| event.call_id.clone())
        .collect()
}

fn record_trace(runtime: &EvalRuntime, run_id: &str, trace: &AgentControlPlaneTrace) {
    for turn in &trace.model_turns {
        runtime.append_event(
            run_id,
            EvalEvent::ModelTurn {
                ts: chrono::Utc::now().to_rfc3339(),
                model: turn.model.clone(),
                token_usage: Some(serde_json::json!({
                    "promptTokens": turn.prompt_tokens,
                    "completionTokens": turn.completion_tokens,
                })),
            },
        );
    }
    for event in &trace.tool_events {
        match event.phase {
            AgentToolPhase::Call => {
                runtime.append_event(
                    run_id,
                    EvalEvent::ToolCall {
                        ts: chrono::Utc::now().to_rfc3339(),
                        tool_name: event.tool_name.clone(),
                        input_ref: event.call_id.clone(),
                    },
                );
            }
            AgentToolPhase::Result => {
                runtime.append_event(
                    run_id,
                    EvalEvent::ToolResult {
                        ts: chrono::Utc::now().to_rfc3339(),
                        tool_name: event.tool_name.clone(),
                        output_ref: event.call_id.clone(),
                        ok: event.ok,
                    },
                );
            }
        }
    }
    for permission in &trace.permission_requests {
        runtime.append_event(
            run_id,
            EvalEvent::PermissionRequest {
                ts: chrono::Utc::now().to_rfc3339(),
                request_id: permission.request_id.clone(),
                reason: permission.reason.clone(),
            },
        );
    }
    for checkpoint in &trace.checkpoints {
        runtime.append_event(
            run_id,
            EvalEvent::Checkpoint {
                ts: chrono::Utc::now().to_rfc3339(),
                checkpoint_ref: checkpoint.clone(),
            },
        );
    }
}

fn check(
    id: impl Into<String>,
    passed: bool,
    message: impl Into<String>,
) -> AgentControlPlaneCheck {
    AgentControlPlaneCheck {
        id: id.into(),
        passed,
        score: if passed { 1.0 } else { 0.0 },
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn case() -> AgentControlPlaneCase {
        AgentControlPlaneCase {
            id: "agent_loop.tool_result_pairing".into(),
            title: "Tool result pairing".into(),
            prompt: "Run a tool and close it".into(),
            required_tools: vec!["web_fetch".into()],
            require_tool_results: true,
            require_permission_request: false,
            forbid_permission_request: true,
            require_checkpoint: false,
            require_non_running_final_status: true,
            min_model_turns: 1,
        }
    }

    #[test]
    fn loads_builtin_control_plane_cases() {
        let cases = AgentLoopControlPlaneEvalAdapter::load_builtin_cases().unwrap();
        assert_eq!(cases.len(), 4);
        assert!(cases.iter().any(|case| case.require_checkpoint));
    }

    #[test]
    fn catches_unresolved_tool_call_running_regression() {
        let mut trace = AgentControlPlaneTrace::fixture_for_case(&case());
        trace
            .tool_events
            .retain(|event| event.phase != AgentToolPhase::Result);
        trace.final_status = AgentLoopFinalStatus::Running;

        let scorecard = score_agent_control_plane_case(case(), &trace);

        assert!(!scorecard.passed);
        assert!(scorecard
            .checks
            .iter()
            .any(|check| check.id == "tool_results_paired" && !check.passed));
        assert!(scorecard
            .checks
            .iter()
            .any(|check| check.id == "final_status_closed" && !check.passed));
    }

    #[test]
    fn permission_boundary_requires_permission_request() {
        let case = AgentControlPlaneCase {
            id: "agent_loop.permission_boundary".into(),
            title: "Permission boundary".into(),
            prompt: "Try a guarded tool".into(),
            required_tools: vec!["shell".into()],
            require_tool_results: false,
            require_permission_request: true,
            forbid_permission_request: false,
            require_checkpoint: false,
            require_non_running_final_status: true,
            min_model_turns: 1,
        };
        let mut trace = AgentControlPlaneTrace::fixture_for_case(&case);
        trace.permission_requests.clear();

        let scorecard = score_agent_control_plane_case(case, &trace);

        assert!(!scorecard.passed);
        assert!(scorecard
            .checks
            .iter()
            .any(|check| check.id == "permission_requested" && !check.passed));
    }

    #[test]
    fn run_fixture_suite_records_trace_and_scorecard_artifacts() {
        let tmp = tempfile::tempdir().unwrap();
        let runtime = EvalRuntime::new(tmp.path());
        let adapter = AgentLoopControlPlaneEvalAdapter;

        let suite = adapter.run_fixture_suite(&runtime).unwrap();

        assert!(suite.passed, "{suite:#?}");
        let episode = runtime.get_episode(&suite.run_ids[0]).unwrap();
        assert_eq!(episode.verdict, HarnessVerdict::Pass);
        assert!(episode
            .artifacts
            .iter()
            .any(|artifact| artifact.kind == "agent_control_plane_scorecard"));
        assert!(episode
            .trace
            .iter()
            .any(|event| event.kind() == "tool_call"));
    }
}
