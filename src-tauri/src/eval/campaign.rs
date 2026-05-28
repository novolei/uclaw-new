//! PR11 model-free eval campaign manifests inspired by jcode's CLI
//! smoke test runner.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::eval::artifacts::{ArtifactStoreError, EvalArtifact};
use crate::eval::case::{
    EvalAssertion, EvalBudget, EvalCase, EvalFixture, EvalPolicy, EvalSubject,
};
use crate::eval::performance_scorecard::PerformanceThreshold;
use crate::eval::runtime::EvalRuntime;

pub const HARNESS_CAMPAIGN_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCampaignKind {
    ToolSmoke,
    BrowserReadiness,
    SoftInterruptCheckpoint,
    ScheduledWorker,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvalCampaignCadence {
    PerPr,
    Nightly,
    PrePromotion,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCampaignCase {
    pub case: EvalCase,
    pub source_reference: String,
    pub required_event_kinds: Vec<String>,
    pub required_artifacts: Vec<String>,
    pub performance_metrics: Vec<String>,
    pub model_free: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvalCampaign {
    pub schema_version: u32,
    pub campaign_id: String,
    pub title: String,
    pub kind: EvalCampaignKind,
    pub summary: String,
    pub model_free: bool,
    pub cadence: EvalCampaignCadence,
    pub cases: Vec<EvalCampaignCase>,
    pub performance_thresholds: Vec<PerformanceThreshold>,
    pub required_artifacts: Vec<String>,
    pub promotion_gate: bool,
}

impl EvalCampaign {
    pub fn to_json_value(&self) -> Result<Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    pub fn case_count(&self) -> usize {
        self.cases.len()
    }
}

pub fn agent_os_harness_campaigns() -> Vec<EvalCampaign> {
    vec![
        jcode_tool_smoke_campaign(false),
        browser_provider_readiness_campaign(),
        soft_interrupt_checkpoint_campaign(),
        scheduled_worker_campaign(),
    ]
}

pub fn jcode_tool_smoke_campaign(include_network: bool) -> EvalCampaign {
    let mut cases = vec![
        tool_case(
            "tool_smoke.write_file",
            "Write workspace file",
            "write_file",
            json!({"path": "sample.txt", "content": "alpha\nbeta\n"}),
            true,
            false,
        ),
        tool_case(
            "tool_smoke.read_file",
            "Read workspace file",
            "read_file",
            json!({"path": "sample.txt"}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.edit",
            "Edit workspace file",
            "edit",
            json!({"path": "sample.txt", "oldString": "alpha", "newString": "alpha1"}),
            true,
            false,
        ),
        tool_case(
            "tool_smoke.patch_equivalent",
            "Patch-like edit smoke",
            "edit",
            json!({"path": "sample.txt", "oldString": "beta", "newString": "beta1"}),
            true,
            false,
        ),
        tool_case(
            "tool_smoke.grep",
            "Search file contents",
            "grep",
            json!({"pattern": "beta1", "path": "."}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.glob",
            "Search file names",
            "glob",
            json!({"pattern": "*.txt", "path": "."}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.bash",
            "Run safe shell command",
            "bash",
            json!({"command": "pwd"}),
            true,
            false,
        ),
        tool_case(
            "tool_smoke.plan_write",
            "Create model-free task ledger",
            "plan_write",
            json!({"title": "harness task", "steps": ["inspect", "verify"]}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.plan_update",
            "Update model-free task ledger",
            "plan_update",
            json!({"step": "inspect", "done": true}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.batch_equivalent",
            "Batch-equivalent ordered tool smoke",
            "plan_update",
            json!({"orderedTools": ["glob", "read_file"]}),
            false,
            false,
        ),
        tool_case(
            "tool_smoke.invalid_tool",
            "Invalid tool call fails closed",
            "invalid_tool",
            json!({"tool": "unknown", "error": "missing required field"}),
            false,
            false,
        ),
    ];

    if include_network {
        cases.push(tool_case(
            "tool_smoke.web_fetch",
            "Fetch a public page",
            "web_fetch",
            json!({"url": "https://example.com", "format": "text"}),
            false,
            true,
        ));
        cases.push(tool_case(
            "tool_smoke.web_search",
            "Search the web",
            "web_search",
            json!({"query": "rust async await"}),
            false,
            true,
        ));
    }

    EvalCampaign {
        schema_version: HARNESS_CAMPAIGN_SCHEMA_VERSION,
        campaign_id: "agent_os.tool_smoke".into(),
        title: "Agent OS Tool Smoke Campaign".into(),
        kind: EvalCampaignKind::ToolSmoke,
        summary: "Model-free tool smoke cases adapted from jcode's CLI harness.".into(),
        model_free: true,
        cadence: EvalCampaignCadence::PerPr,
        cases,
        performance_thresholds: vec![
            PerformanceThreshold::new("tool.latency_ms", 250.0, 1_000.0),
            PerformanceThreshold::new("tool.output_bytes", 64_000.0, 256_000.0),
        ],
        required_artifacts: vec!["tool_result".into(), "performance_scorecard".into()],
        promotion_gate: true,
    }
}

pub fn browser_provider_readiness_campaign() -> EvalCampaign {
    EvalCampaign {
        schema_version: HARNESS_CAMPAIGN_SCHEMA_VERSION,
        campaign_id: "agent_os.browser_provider_readiness".into(),
        title: "Browser Provider Readiness Campaign".into(),
        kind: EvalCampaignKind::BrowserReadiness,
        summary: "Model-free browser provider status/setup/probe campaign from PR9.".into(),
        model_free: true,
        cadence: EvalCampaignCadence::PerPr,
        cases: ["ready", "degraded", "needs_setup"]
            .into_iter()
            .map(browser_readiness_case)
            .collect(),
        performance_thresholds: vec![
            PerformanceThreshold::new("browser.provider.status_latency_ms", 100.0, 500.0),
            PerformanceThreshold::new("browser.provider.probe_latency_ms", 500.0, 2_000.0),
        ],
        required_artifacts: vec![
            "browser_provider_status".into(),
            "performance_scorecard".into(),
        ],
        promotion_gate: true,
    }
}

pub fn soft_interrupt_checkpoint_campaign() -> EvalCampaign {
    EvalCampaign {
        schema_version: HARNESS_CAMPAIGN_SCHEMA_VERSION,
        campaign_id: "agent_os.soft_interrupt_checkpoint".into(),
        title: "Soft Interrupt And Checkpoint Campaign".into(),
        kind: EvalCampaignKind::SoftInterruptCheckpoint,
        summary: "Agent-loop campaign requiring visible boundaries and resumable checkpoints."
            .into(),
        model_free: true,
        cadence: EvalCampaignCadence::PrePromotion,
        cases: vec![
            runtime_case(
                "soft_interrupt.boundary_yield",
                EvalSubject::AgentLoop,
                "Soft interrupt yields at a human boundary",
                vec!["run_started", "boundary_event", "run_finished"],
                vec!["soft_interrupt_trace"],
            ),
            runtime_case(
                "soft_interrupt.checkpoint_resume",
                EvalSubject::AgentLoop,
                "Checkpoint resumes after interruption",
                vec!["run_started", "checkpoint", "run_finished"],
                vec!["checkpoint_trace"],
            ),
        ],
        performance_thresholds: vec![PerformanceThreshold::new(
            "soft_interrupt.resume_latency_ms",
            500.0,
            2_000.0,
        )],
        required_artifacts: vec![
            "soft_interrupt_trace".into(),
            "performance_scorecard".into(),
        ],
        promotion_gate: true,
    }
}

pub fn scheduled_worker_campaign() -> EvalCampaign {
    EvalCampaign {
        schema_version: HARNESS_CAMPAIGN_SCHEMA_VERSION,
        campaign_id: "agent_os.scheduled_worker".into(),
        title: "Scheduled Worker Campaign".into(),
        kind: EvalCampaignKind::ScheduledWorker,
        summary: "Automation/scheduled-worker campaign anchored by PR10 ambient mapping.".into(),
        model_free: true,
        cadence: EvalCampaignCadence::PrePromotion,
        cases: vec![
            runtime_case(
                "scheduled_worker.completed_report",
                EvalSubject::Tasks,
                "Scheduled worker completes with a report",
                vec!["run_started", "checkpoint", "run_finished"],
                vec!["automation_activity_trace", "worker_heartbeat"],
            ),
            runtime_case(
                "scheduled_worker.permission_boundary",
                EvalSubject::Tasks,
                "Scheduled worker yields for permission",
                vec!["run_started", "boundary_event", "run_finished"],
                vec!["automation_activity_trace", "permission_boundary"],
            ),
        ],
        performance_thresholds: vec![
            PerformanceThreshold::new("scheduled_worker.visible_progress_ms", 1_000.0, 5_000.0),
            PerformanceThreshold::new("scheduled_worker.resume_latency_ms", 1_000.0, 5_000.0),
        ],
        required_artifacts: vec![
            "automation_activity_trace".into(),
            "worker_heartbeat".into(),
            "performance_scorecard".into(),
        ],
        promotion_gate: true,
    }
}

pub fn attach_harness_campaign_manifest(
    runtime: &EvalRuntime,
    run_id: &str,
    campaign: &EvalCampaign,
) -> Result<Option<EvalArtifact>, ArtifactStoreError> {
    let value = campaign
        .to_json_value()
        .map_err(ArtifactStoreError::Serde)?;
    runtime.attach_json_artifact(run_id, "harness_campaign_manifest", &value)
}

fn tool_case(
    id: &'static str,
    title: &'static str,
    tool_name: &'static str,
    input: Value,
    risky: bool,
    allow_network: bool,
) -> EvalCampaignCase {
    let expected_ok = tool_name != "invalid_tool";
    EvalCampaignCase {
        case: EvalCase {
            id: id.into(),
            subject: EvalSubject::Tools,
            title: title.into(),
            prompt: format!("Run {tool_name} with deterministic fixture input."),
            setup: vec![EvalFixture {
                id: "tool-input".into(),
                kind: "tool_input".into(),
                config: json!({
                    "toolName": tool_name,
                    "input": input,
                    "expectedOk": expected_ok,
                }),
            }],
            policy: EvalPolicy {
                permission_mode: if risky { "ask" } else { "bypass" }.into(),
                allowed_tools: vec![tool_name.into()],
                allow_network,
                allow_memory_writes: false,
            },
            budgets: EvalBudget {
                max_steps: 4,
                max_seconds: 30,
                max_tokens: None,
            },
            assertions: vec![EvalAssertion {
                id: "tool-result".into(),
                kind: if expected_ok {
                    "tool_result_ok".into()
                } else {
                    "tool_result_error".into()
                },
                expected: json!({ "toolName": tool_name }),
            }],
            graders: Vec::new(),
        },
        source_reference: "jcode/src/bin/harness.rs".into(),
        required_event_kinds: vec!["tool_call".into(), "tool_result".into()],
        required_artifacts: vec!["tool_result".into()],
        performance_metrics: vec!["tool.latency_ms".into(), "tool.output_bytes".into()],
        model_free: true,
    }
}

fn browser_readiness_case(status: &str) -> EvalCampaignCase {
    EvalCampaignCase {
        case: EvalCase {
            id: format!("browser_provider.{status}"),
            subject: EvalSubject::Browser,
            title: format!("Browser provider {status} status"),
            prompt: format!("Evaluate browser provider readiness fixture: {status}."),
            setup: vec![EvalFixture {
                id: "provider-fixture".into(),
                kind: "browser_provider_probe".into(),
                config: json!({ "status": status }),
            }],
            policy: EvalPolicy {
                permission_mode: "bypass".into(),
                allowed_tools: vec!["browser_provider_status".into()],
                allow_network: false,
                allow_memory_writes: false,
            },
            budgets: EvalBudget {
                max_steps: 3,
                max_seconds: 15,
                max_tokens: None,
            },
            assertions: vec![EvalAssertion {
                id: "readiness-status".into(),
                kind: "browser_provider_readiness".into(),
                expected: json!({ "status": status }),
            }],
            graders: Vec::new(),
        },
        source_reference: "docs/superpowers/plans/2026-05-23-pr9-browser-provider-probe.md".into(),
        required_event_kinds: vec!["run_started".into(), "run_finished".into()],
        required_artifacts: vec!["browser_provider_status".into()],
        performance_metrics: vec![
            "browser.provider.status_latency_ms".into(),
            "browser.provider.probe_latency_ms".into(),
        ],
        model_free: true,
    }
}

fn runtime_case(
    id: &'static str,
    subject: EvalSubject,
    title: &'static str,
    required_event_kinds: Vec<&'static str>,
    required_artifacts: Vec<&'static str>,
) -> EvalCampaignCase {
    EvalCampaignCase {
        case: EvalCase {
            id: id.into(),
            subject,
            title: title.into(),
            prompt: title.into(),
            setup: Vec::new(),
            policy: EvalPolicy::default(),
            budgets: EvalBudget {
                max_steps: 8,
                max_seconds: 60,
                max_tokens: Some(4_000),
            },
            assertions: required_event_kinds
                .iter()
                .map(|kind| EvalAssertion {
                    id: format!("has-{kind}"),
                    kind: "event_exists".into(),
                    expected: json!({ "kind": kind }),
                })
                .collect(),
            graders: Vec::new(),
        },
        source_reference:
            "docs/superpowers/specs/2026-05-23-agent-os-spine-jcode-absorption-design.md".into(),
        required_event_kinds: required_event_kinds
            .into_iter()
            .map(str::to_string)
            .collect(),
        required_artifacts: required_artifacts.into_iter().map(str::to_string).collect(),
        performance_metrics: vec!["runtime.progress_latency_ms".into()],
        model_free: true,
    }
}

#[cfg(test)]
#[path = "campaign_tests.rs"]
mod tests;
