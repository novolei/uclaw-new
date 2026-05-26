use super::super::playwright_discovery::{
    ExperimentalNodeBootstrapStatus, PlaywrightCommandStatus, PlaywrightSystemDiscoveryReport,
    PlaywrightSystemStatus,
};
use super::*;

#[test]
fn setup_plan_installs_cli_skills_and_probes_mcp_when_node_stack_is_ready() {
    let report = PlaywrightSystemDiscoveryReport {
        status: PlaywrightSystemStatus::NeedsSetup,
        node: present("node", "v22.0.0"),
        npm: present("npm", "10.0.0"),
        npx: present("npx", "10.0.0"),
        playwright_cli: missing("playwright-cli"),
        can_run_global_install: true,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: false,
            method: None,
            reason: Some("node_stack_present".to_string()),
        },
    };

    let plan = plan_playwright_setup(&report, PlaywrightSetupAction::AutoSetup);
    assert_eq!(plan.blocked_reason, None);
    assert_eq!(plan.steps[0].command, "npm");
    assert_eq!(
        plan.steps[0].args,
        vec!["install", "-g", "@playwright/cli@latest"]
    );
    assert_eq!(plan.steps[1].command, "playwright-cli");
    assert_eq!(plan.steps[1].args, vec!["install", "--skills"]);
    assert_eq!(plan.steps[2].command, "npx");
    assert_eq!(plan.steps[2].args, vec!["@playwright/mcp@latest", "--help"]);
    assert!(plan
        .steps
        .iter()
        .all(|step| step.command != elevated_command()));
}

#[test]
fn auto_setup_blocks_when_node_is_missing() {
    let report = PlaywrightSystemDiscoveryReport {
        status: PlaywrightSystemStatus::NeedsNode,
        node: missing("node"),
        npm: missing("npm"),
        npx: missing("npx"),
        playwright_cli: missing("playwright-cli"),
        can_run_global_install: false,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: false,
            method: None,
            reason: Some("homebrew_not_found".to_string()),
        },
    };

    let plan = plan_playwright_setup(&report, PlaywrightSetupAction::AutoSetup);
    assert_eq!(plan.blocked_reason.as_deref(), Some("node_required"));
    assert!(plan.steps.is_empty());
}

#[test]
fn node_bootstrap_plan_is_homebrew_only() {
    let report = PlaywrightSystemDiscoveryReport {
        status: PlaywrightSystemStatus::NeedsNode,
        node: missing("node"),
        npm: missing("npm"),
        npx: missing("npx"),
        playwright_cli: missing("playwright-cli"),
        can_run_global_install: false,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: true,
            method: Some("homebrew".to_string()),
            reason: None,
        },
    };

    let plan = plan_playwright_setup(&report, PlaywrightSetupAction::InstallNodeWithHomebrew);
    assert_eq!(plan.steps[0].command, "brew");
    assert_eq!(plan.steps[0].args, vec!["install", "node"]);
    assert!(plan.steps.iter().all(|step| !step.destructive));
}

#[test]
fn refresh_skills_requires_playwright_cli() {
    let report = PlaywrightSystemDiscoveryReport {
        status: PlaywrightSystemStatus::NeedsSetup,
        node: present("node", "v22.0.0"),
        npm: present("npm", "10.0.0"),
        npx: present("npx", "10.0.0"),
        playwright_cli: missing("playwright-cli"),
        can_run_global_install: true,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: false,
            method: None,
            reason: Some("node_stack_present".to_string()),
        },
    };

    let plan = plan_playwright_setup(&report, PlaywrightSetupAction::RefreshSkills);
    assert_eq!(
        plan.blocked_reason.as_deref(),
        Some("playwright_cli_required")
    );
    assert!(plan.steps.is_empty());
}

#[test]
fn mcp_probe_requires_npx() {
    let report = PlaywrightSystemDiscoveryReport {
        status: PlaywrightSystemStatus::NeedsNode,
        node: present("node", "v22.0.0"),
        npm: present("npm", "10.0.0"),
        npx: missing("npx"),
        playwright_cli: present("playwright-cli", "0.0.1"),
        can_run_global_install: false,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: false,
            method: None,
            reason: Some("unsupported_platform".to_string()),
        },
    };

    let plan = plan_playwright_setup(&report, PlaywrightSetupAction::ProbeMcp);
    assert_eq!(plan.blocked_reason.as_deref(), Some("npx_required"));
    assert!(plan.steps.is_empty());
}

#[test]
fn execution_stops_after_failed_step_and_captures_artifacts() {
    let plan = PlaywrightSetupPlan {
        action: PlaywrightSetupAction::AutoSetup,
        blocked_reason: None,
        steps: vec![
            PlaywrightSetupCommandStep {
                id: "first".to_string(),
                command: "ok".to_string(),
                args: Vec::new(),
                timeout_ms: 1,
                destructive: false,
            },
            PlaywrightSetupCommandStep {
                id: "second".to_string(),
                command: "fail".to_string(),
                args: Vec::new(),
                timeout_ms: 1,
                destructive: false,
            },
            PlaywrightSetupCommandStep {
                id: "third".to_string(),
                command: "skip".to_string(),
                args: Vec::new(),
                timeout_ms: 1,
                destructive: false,
            },
        ],
    };
    let mut runner = FakeRunner::new(vec![
        PlaywrightSetupStepExecutionStatus::Succeeded,
        PlaywrightSetupStepExecutionStatus::Failed,
    ]);

    let report = execute_playwright_setup_plan_with_runner(&plan, &mut runner);

    assert_eq!(report.status, PlaywrightSetupExecutionStatus::Failed);
    assert_eq!(report.step_reports.len(), 2);
    assert_eq!(report.step_reports[0].stdout, "out:first");
    assert_eq!(report.step_reports[1].stderr, "err:second");
}

#[test]
fn blocked_plan_does_not_run_steps() {
    let plan = PlaywrightSetupPlan {
        action: PlaywrightSetupAction::AutoSetup,
        blocked_reason: Some("node_required".to_string()),
        steps: vec![PlaywrightSetupCommandStep {
            id: "should_not_run".to_string(),
            command: "npm".to_string(),
            args: Vec::new(),
            timeout_ms: 1,
            destructive: false,
        }],
    };
    let mut runner = FakeRunner::new(vec![PlaywrightSetupStepExecutionStatus::Succeeded]);

    let report = execute_playwright_setup_plan_with_runner(&plan, &mut runner);

    assert_eq!(report.status, PlaywrightSetupExecutionStatus::Blocked);
    assert_eq!(report.blocked_reason.as_deref(), Some("node_required"));
    assert!(report.step_reports.is_empty());
    assert_eq!(runner.calls, 0);
}

fn present(command: &str, version: &str) -> PlaywrightCommandStatus {
    PlaywrightCommandStatus {
        command: command.to_string(),
        present: true,
        version: Some(version.to_string()),
    }
}

fn missing(command: &str) -> PlaywrightCommandStatus {
    PlaywrightCommandStatus {
        command: command.to_string(),
        present: false,
        version: None,
    }
}

struct FakeRunner {
    statuses: Vec<PlaywrightSetupStepExecutionStatus>,
    calls: usize,
}

impl FakeRunner {
    fn new(statuses: Vec<PlaywrightSetupStepExecutionStatus>) -> Self {
        Self { statuses, calls: 0 }
    }
}

impl PlaywrightSetupCommandRunner for FakeRunner {
    fn run_step(
        &mut self,
        step: &PlaywrightSetupCommandStep,
    ) -> PlaywrightSetupStepExecutionReport {
        let status = self.statuses[self.calls];
        self.calls += 1;
        PlaywrightSetupStepExecutionReport {
            step_id: step.id.clone(),
            command: step.command.clone(),
            args: step.args.clone(),
            status,
            exit_code: Some(if status == PlaywrightSetupStepExecutionStatus::Succeeded {
                0
            } else {
                1
            }),
            stdout: format!("out:{}", step.id),
            stderr: format!("err:{}", step.id),
            error: (status != PlaywrightSetupStepExecutionStatus::Succeeded)
                .then(|| format!("failed:{}", step.id)),
        }
    }
}

fn elevated_command() -> String {
    ["su", "do"].concat()
}
