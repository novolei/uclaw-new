//! Controlled setup planning and execution for official Playwright tooling.
//!
//! This module owns official command plans such as global `@playwright/cli`
//! installation, skill refresh, MCP probing, and the macOS Homebrew-only Node
//! bootstrap. It deliberately does not run elevated commands.

use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use super::playwright_discovery::{PlaywrightSystemDiscoveryReport, PlaywrightSystemStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSetupAction {
    AutoSetup,
    InstallNodeWithHomebrew,
    RefreshSkills,
    ProbeMcp,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSetupCommandStep {
    pub id: String,
    pub command: String,
    pub args: Vec<String>,
    pub timeout_ms: u64,
    pub destructive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSetupPlan {
    pub action: PlaywrightSetupAction,
    pub blocked_reason: Option<String>,
    pub steps: Vec<PlaywrightSetupCommandStep>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSetupExecutionStatus {
    Succeeded,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSetupStepExecutionStatus {
    Succeeded,
    Failed,
    TimedOut,
    SpawnFailed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSetupStepExecutionReport {
    pub step_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub status: PlaywrightSetupStepExecutionStatus,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSetupExecutionReport {
    pub action: PlaywrightSetupAction,
    pub status: PlaywrightSetupExecutionStatus,
    pub blocked_reason: Option<String>,
    pub step_reports: Vec<PlaywrightSetupStepExecutionReport>,
}

pub trait PlaywrightSetupCommandRunner {
    fn run_step(&mut self, step: &PlaywrightSetupCommandStep)
        -> PlaywrightSetupStepExecutionReport;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemPlaywrightSetupCommandRunner;

impl PlaywrightSetupCommandRunner for SystemPlaywrightSetupCommandRunner {
    fn run_step(
        &mut self,
        step: &PlaywrightSetupCommandStep,
    ) -> PlaywrightSetupStepExecutionReport {
        run_system_step(step)
    }
}

pub fn plan_playwright_setup(
    report: &PlaywrightSystemDiscoveryReport,
    action: PlaywrightSetupAction,
) -> PlaywrightSetupPlan {
    match action {
        PlaywrightSetupAction::AutoSetup => plan_auto_setup(report),
        PlaywrightSetupAction::InstallNodeWithHomebrew => plan_homebrew_node(report),
        PlaywrightSetupAction::RefreshSkills => plan_refresh_skills(report),
        PlaywrightSetupAction::ProbeMcp => plan_probe_mcp(report),
    }
}

pub fn execute_playwright_setup_plan_with_runner(
    plan: &PlaywrightSetupPlan,
    runner: &mut dyn PlaywrightSetupCommandRunner,
) -> PlaywrightSetupExecutionReport {
    if let Some(reason) = plan.blocked_reason.clone() {
        return PlaywrightSetupExecutionReport {
            action: plan.action,
            status: PlaywrightSetupExecutionStatus::Blocked,
            blocked_reason: Some(reason),
            step_reports: Vec::new(),
        };
    }

    let mut step_reports = Vec::new();
    let mut status = PlaywrightSetupExecutionStatus::Succeeded;
    for step in &plan.steps {
        let report = runner.run_step(step);
        let succeeded = report.status == PlaywrightSetupStepExecutionStatus::Succeeded;
        step_reports.push(report);
        if !succeeded {
            status = PlaywrightSetupExecutionStatus::Failed;
            break;
        }
    }

    PlaywrightSetupExecutionReport {
        action: plan.action,
        status,
        blocked_reason: None,
        step_reports,
    }
}

fn plan_auto_setup(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if report.status == PlaywrightSystemStatus::NeedsNode {
        return blocked(PlaywrightSetupAction::AutoSetup, "node_required");
    }
    if !report.can_run_global_install {
        return blocked(
            PlaywrightSetupAction::AutoSetup,
            "npm_global_prefix_not_writable",
        );
    }
    PlaywrightSetupPlan {
        action: PlaywrightSetupAction::AutoSetup,
        blocked_reason: None,
        steps: vec![install_cli_step(), skills_step(), mcp_probe_step()],
    }
}

fn plan_homebrew_node(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if report.experimental_node_bootstrap.method.as_deref() != Some("homebrew") {
        return blocked(
            PlaywrightSetupAction::InstallNodeWithHomebrew,
            "homebrew_not_available",
        );
    }
    PlaywrightSetupPlan {
        action: PlaywrightSetupAction::InstallNodeWithHomebrew,
        blocked_reason: None,
        steps: vec![PlaywrightSetupCommandStep {
            id: "install_node_homebrew".to_string(),
            command: "brew".to_string(),
            args: vec!["install".to_string(), "node".to_string()],
            timeout_ms: 300_000,
            destructive: false,
        }],
    }
}

fn plan_refresh_skills(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if !report.playwright_cli.present {
        return blocked(
            PlaywrightSetupAction::RefreshSkills,
            "playwright_cli_required",
        );
    }
    PlaywrightSetupPlan {
        action: PlaywrightSetupAction::RefreshSkills,
        blocked_reason: None,
        steps: vec![skills_step()],
    }
}

fn plan_probe_mcp(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if !report.npx.present {
        return blocked(PlaywrightSetupAction::ProbeMcp, "npx_required");
    }
    PlaywrightSetupPlan {
        action: PlaywrightSetupAction::ProbeMcp,
        blocked_reason: None,
        steps: vec![mcp_probe_step()],
    }
}

fn install_cli_step() -> PlaywrightSetupCommandStep {
    PlaywrightSetupCommandStep {
        id: "install_playwright_cli".to_string(),
        command: "npm".to_string(),
        args: vec![
            "install".to_string(),
            "-g".to_string(),
            "@playwright/cli@latest".to_string(),
        ],
        timeout_ms: 180_000,
        destructive: false,
    }
}

fn skills_step() -> PlaywrightSetupCommandStep {
    PlaywrightSetupCommandStep {
        id: "install_playwright_skills".to_string(),
        command: "playwright-cli".to_string(),
        args: vec!["install".to_string(), "--skills".to_string()],
        timeout_ms: 120_000,
        destructive: false,
    }
}

fn mcp_probe_step() -> PlaywrightSetupCommandStep {
    PlaywrightSetupCommandStep {
        id: "probe_playwright_mcp".to_string(),
        command: "npx".to_string(),
        args: vec!["@playwright/mcp@latest".to_string(), "--help".to_string()],
        timeout_ms: 60_000,
        destructive: false,
    }
}

fn blocked(action: PlaywrightSetupAction, reason: &str) -> PlaywrightSetupPlan {
    PlaywrightSetupPlan {
        action,
        blocked_reason: Some(reason.to_string()),
        steps: Vec::new(),
    }
}

fn run_system_step(step: &PlaywrightSetupCommandStep) -> PlaywrightSetupStepExecutionReport {
    let mut child = match Command::new(&step.command)
        .args(&step.args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return spawn_failed(step, format!("spawn {}: {error}", step.command));
        }
    };

    let started = Instant::now();
    let timeout = Duration::from_millis(step.timeout_ms);
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => break,
            Ok(None) if started.elapsed() >= timeout => {
                let _ = child.kill();
                return match child.wait_with_output() {
                    Ok(output) => step_report_from_output(
                        step,
                        PlaywrightSetupStepExecutionStatus::TimedOut,
                        output.status.code(),
                        output.stdout,
                        output.stderr,
                        Some(format!("timed out after {} ms", step.timeout_ms)),
                    ),
                    Err(error) => step_report(
                        step,
                        PlaywrightSetupStepExecutionStatus::TimedOut,
                        None,
                        String::new(),
                        String::new(),
                        Some(format!(
                            "timed out after {} ms; wait failed: {error}",
                            step.timeout_ms
                        )),
                    ),
                };
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(error) => {
                let _ = child.kill();
                return step_report(
                    step,
                    PlaywrightSetupStepExecutionStatus::Failed,
                    None,
                    String::new(),
                    String::new(),
                    Some(format!("poll {}: {error}", step.command)),
                );
            }
        }
    }

    match child.wait_with_output() {
        Ok(output) => {
            let status = if output.status.success() {
                PlaywrightSetupStepExecutionStatus::Succeeded
            } else {
                PlaywrightSetupStepExecutionStatus::Failed
            };
            step_report_from_output(
                step,
                status,
                output.status.code(),
                output.stdout,
                output.stderr,
                None,
            )
        }
        Err(error) => step_report(
            step,
            PlaywrightSetupStepExecutionStatus::Failed,
            None,
            String::new(),
            String::new(),
            Some(format!("wait {}: {error}", step.command)),
        ),
    }
}

fn step_report_from_output(
    step: &PlaywrightSetupCommandStep,
    status: PlaywrightSetupStepExecutionStatus,
    exit_code: Option<i32>,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    error: Option<String>,
) -> PlaywrightSetupStepExecutionReport {
    step_report(
        step,
        status,
        exit_code,
        String::from_utf8_lossy(&stdout).to_string(),
        String::from_utf8_lossy(&stderr).to_string(),
        error,
    )
}

fn spawn_failed(
    step: &PlaywrightSetupCommandStep,
    error: String,
) -> PlaywrightSetupStepExecutionReport {
    step_report(
        step,
        PlaywrightSetupStepExecutionStatus::SpawnFailed,
        None,
        String::new(),
        String::new(),
        Some(error),
    )
}

fn step_report(
    step: &PlaywrightSetupCommandStep,
    status: PlaywrightSetupStepExecutionStatus,
    exit_code: Option<i32>,
    stdout: String,
    stderr: String,
    error: Option<String>,
) -> PlaywrightSetupStepExecutionReport {
    PlaywrightSetupStepExecutionReport {
        step_id: step.id.clone(),
        command: step.command.clone(),
        args: step.args.clone(),
        status,
        exit_code,
        stdout,
        stderr,
        error,
    }
}

#[cfg(test)]
#[path = "playwright_setup_tests.rs"]
mod playwright_setup_tests;
