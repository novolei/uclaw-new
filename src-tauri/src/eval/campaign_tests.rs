use serde_json::json;

use super::*;
use crate::eval::case::EvalSubject;
use crate::eval::runtime::EvalRuntime;

#[test]
fn default_agent_os_campaign_pack_has_expected_order() {
    let campaigns = agent_os_harness_campaigns();
    let ids = campaigns
        .iter()
        .map(|campaign| campaign.campaign_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(
        ids,
        vec![
            "agent_os.tool_smoke",
            "agent_os.browser_provider_readiness",
            "agent_os.soft_interrupt_checkpoint",
            "agent_os.scheduled_worker",
        ]
    );
}

#[test]
fn tool_smoke_campaign_is_model_free_and_covers_jcode_patterns() {
    let campaign = jcode_tool_smoke_campaign(false);

    assert_eq!(campaign.kind, HarnessCampaignKind::ToolSmoke);
    assert!(campaign.model_free);
    assert!(campaign.promotion_gate);

    let tool_names = tool_names(&campaign);
    for expected in [
        "write_file",
        "read_file",
        "edit",
        "grep",
        "glob",
        "bash",
        "plan_write",
        "plan_update",
    ] {
        assert!(
            tool_names.contains(&expected),
            "missing tool-smoke case for {expected}: {tool_names:?}"
        );
    }
    assert!(!tool_names.contains(&"web_fetch"));
    assert!(!tool_names.contains(&"web_search"));
    assert!(campaign
        .cases
        .iter()
        .all(|case| case.case.subject == EvalSubject::Tools));
    assert!(campaign
        .cases
        .iter()
        .all(|case| case.required_event_kinds == ["tool_call", "tool_result"]));
}

#[test]
fn network_tool_smoke_cases_are_opt_in() {
    let campaign = jcode_tool_smoke_campaign(true);
    let tool_names = tool_names(&campaign);

    assert!(tool_names.contains(&"web_fetch"));
    assert!(tool_names.contains(&"web_search"));

    let web_fetch = campaign
        .cases
        .iter()
        .find(|case| case.case.id == "tool_smoke.web_fetch")
        .unwrap();
    assert!(web_fetch.case.policy.allow_network);
}

#[test]
fn browser_readiness_campaign_requires_provider_artifacts_and_thresholds() {
    let campaign = browser_provider_readiness_campaign();

    assert_eq!(campaign.kind, HarnessCampaignKind::BrowserReadiness);
    assert_eq!(campaign.cases.len(), 3);
    assert!(campaign
        .required_artifacts
        .contains(&"browser_provider_status".to_string()));
    assert!(campaign
        .required_artifacts
        .contains(&"performance_scorecard".to_string()));
    assert!(campaign
        .performance_thresholds
        .iter()
        .any(|threshold| threshold.metric == "browser.provider.probe_latency_ms"));
}

#[test]
fn soft_interrupt_and_scheduled_worker_campaigns_require_runtime_evidence() {
    let soft = soft_interrupt_checkpoint_campaign();
    let scheduled = scheduled_worker_campaign();

    assert_eq!(soft.kind, HarnessCampaignKind::SoftInterruptCheckpoint);
    assert!(soft.cases.iter().any(|case| case
        .required_event_kinds
        .contains(&"boundary_event".to_string())));
    assert!(soft.cases.iter().any(|case| case
        .required_event_kinds
        .contains(&"checkpoint".to_string())));

    assert_eq!(scheduled.kind, HarnessCampaignKind::ScheduledWorker);
    assert!(scheduled
        .required_artifacts
        .contains(&"automation_activity_trace".to_string()));
    assert!(scheduled
        .cases
        .iter()
        .all(|case| case.case.subject == EvalSubject::Tasks));
}

#[test]
fn campaign_manifest_serializes_camel_case() {
    let campaign = jcode_tool_smoke_campaign(false);

    let value = campaign.to_json_value().unwrap();

    assert_eq!(value["schemaVersion"], HARNESS_CAMPAIGN_SCHEMA_VERSION);
    assert_eq!(value["campaignId"], "agent_os.tool_smoke");
    assert_eq!(value["modelFree"], true);
    assert_eq!(value["cases"][0]["case"]["subject"], "tools");
    assert_eq!(
        value["cases"][0]["sourceReference"],
        "jcode/src/bin/harness.rs"
    );
    assert_ne!(value["performanceThresholds"], json!(null));
}

#[test]
fn campaign_manifest_attaches_as_harness_artifact() {
    let tmp = tempfile::tempdir().unwrap();
    let runtime = EvalRuntime::new(tmp.path());
    let campaign = jcode_tool_smoke_campaign(false);
    let episode = runtime.start_episode(&campaign.cases[0].case);

    let artifact = attach_harness_campaign_manifest(&runtime, &episode.run_id, &campaign)
        .unwrap()
        .unwrap();

    assert_eq!(artifact.kind, "harness_campaign_manifest");
    let body = std::fs::read_to_string(&artifact.path).unwrap();
    assert!(body.contains("agent_os.tool_smoke"), "{body}");
    let stored = runtime.get_episode(&episode.run_id).unwrap();
    assert_eq!(stored.artifacts.len(), 1);
}

fn tool_names(campaign: &HarnessCampaign) -> Vec<&str> {
    let mut names = campaign
        .cases
        .iter()
        .filter_map(|case| case.case.policy.allowed_tools.first().map(String::as_str))
        .collect::<Vec<_>>();
    names.sort();
    names.dedup();
    names
}
