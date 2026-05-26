# Browser Automation Official Runtime PR1 System Discovery And Setup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add official Playwright system discovery and setup planning for Node/npm/npx, global `@playwright/cli@latest`, Playwright skills, MCP probe prerequisites, and macOS Homebrew-only experimental Node bootstrap.

**Architecture:** Introduce read-only discovery and mutating setup modules under `src-tauri/src/browser/`. Discovery never mutates state. Setup runs controlled official commands with captured stdout/stderr and explicit action reports.

**Tech Stack:** Rust, Tokio process execution, serde DTOs, existing Browser Runtime report patterns.

---

## File Structure

- Create: `src-tauri/src/browser/playwright_discovery.rs`
- Create: `src-tauri/src/browser/playwright_setup.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Test: `src-tauri/src/browser/playwright_discovery_tests.rs`
- Test: `src-tauri/src/browser/playwright_setup_tests.rs`

## Task 1: Add Discovery DTOs And Command Detector

**Files:**
- Create: `src-tauri/src/browser/playwright_discovery.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Test: `src-tauri/src/browser/playwright_discovery_tests.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/browser/playwright_discovery_tests.rs`:

```rust
use super::playwright_discovery::*;

#[test]
fn missing_node_reports_needs_node() {
    let report = inspect_playwright_system_with_detector(&StaticDetector {
        node: None,
        npm: None,
        npx: None,
        playwright_cli: None,
        npm_prefix_writable: false,
        brew: None,
    });

    assert_eq!(report.status, PlaywrightSystemStatus::NeedsNode);
    assert!(!report.node.present);
    assert!(!report.npm.present);
    assert!(!report.npx.present);
}

#[test]
fn available_tools_without_cli_reports_needs_setup() {
    let report = inspect_playwright_system_with_detector(&StaticDetector {
        node: Some("v22.0.0"),
        npm: Some("10.0.0"),
        npx: Some("10.0.0"),
        playwright_cli: None,
        npm_prefix_writable: true,
        brew: Some("Homebrew 4.0.0"),
    });

    assert_eq!(report.status, PlaywrightSystemStatus::NeedsSetup);
    assert!(report.can_run_global_install);
    assert!(report.experimental_node_bootstrap.available);
}

struct StaticDetector {
    node: Option<&'static str>,
    npm: Option<&'static str>,
    npx: Option<&'static str>,
    playwright_cli: Option<&'static str>,
    npm_prefix_writable: bool,
    brew: Option<&'static str>,
}

impl PlaywrightCommandDetector for StaticDetector {
    fn command_version(&self, command: &str, _args: &[&str]) -> Option<String> {
        match command {
            "node" => self.node.map(str::to_string),
            "npm" => self.npm.map(str::to_string),
            "npx" => self.npx.map(str::to_string),
            "playwright-cli" => self.playwright_cli.map(str::to_string),
            "brew" => self.brew.map(str::to_string),
            _ => None,
        }
    }

    fn npm_global_prefix_writable(&self) -> bool {
        self.npm_prefix_writable
    }
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_discovery
```

Expected: FAIL because module/types do not exist.

- [ ] **Step 3: Implement discovery module**

Create `src-tauri/src/browser/playwright_discovery.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaywrightSystemStatus {
    Ready,
    NeedsNode,
    NeedsSetup,
    NeedsPermission,
    Degraded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCommandStatus {
    pub command: String,
    pub present: bool,
    pub version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExperimentalNodeBootstrapStatus {
    pub available: bool,
    pub method: Option<String>,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightSystemDiscoveryReport {
    pub status: PlaywrightSystemStatus,
    pub node: PlaywrightCommandStatus,
    pub npm: PlaywrightCommandStatus,
    pub npx: PlaywrightCommandStatus,
    pub playwright_cli: PlaywrightCommandStatus,
    pub can_run_global_install: bool,
    pub experimental_node_bootstrap: ExperimentalNodeBootstrapStatus,
}

pub trait PlaywrightCommandDetector {
    fn command_version(&self, command: &str, args: &[&str]) -> Option<String>;
    fn npm_global_prefix_writable(&self) -> bool;
}

pub struct SystemPlaywrightCommandDetector;

impl PlaywrightCommandDetector for SystemPlaywrightCommandDetector {
    fn command_version(&self, command: &str, args: &[&str]) -> Option<String> {
        std::process::Command::new(command)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
            .filter(|text| !text.is_empty())
    }

    fn npm_global_prefix_writable(&self) -> bool {
        let Some(prefix) = self.command_version("npm", &["prefix", "-g"]) else {
            return false;
        };
        let path = std::path::Path::new(prefix.trim());
        path.exists() && path.metadata().map(|m| !m.permissions().readonly()).unwrap_or(false)
    }
}

pub fn inspect_playwright_system() -> PlaywrightSystemDiscoveryReport {
    inspect_playwright_system_with_detector(&SystemPlaywrightCommandDetector)
}

pub fn inspect_playwright_system_with_detector(
    detector: &dyn PlaywrightCommandDetector,
) -> PlaywrightSystemDiscoveryReport {
    let node = command_status(detector, "node", &["--version"]);
    let npm = command_status(detector, "npm", &["--version"]);
    let npx = command_status(detector, "npx", &["--version"]);
    let playwright_cli = command_status(detector, "playwright-cli", &["--version"]);
    let brew = command_status(detector, "brew", &["--version"]);
    let has_node_stack = node.present && npm.present && npx.present;
    let prefix_writable = detector.npm_global_prefix_writable();
    let status = if !has_node_stack {
        PlaywrightSystemStatus::NeedsNode
    } else if !prefix_writable {
        PlaywrightSystemStatus::NeedsPermission
    } else if !playwright_cli.present {
        PlaywrightSystemStatus::NeedsSetup
    } else {
        PlaywrightSystemStatus::Ready
    };

    PlaywrightSystemDiscoveryReport {
        status,
        node,
        npm,
        npx,
        playwright_cli,
        can_run_global_install: has_node_stack && prefix_writable,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: !has_node_stack && brew.present,
            method: if !has_node_stack && brew.present {
                Some("homebrew".to_string())
            } else {
                None
            },
            reason: if has_node_stack {
                Some("node_stack_present".to_string())
            } else if brew.present {
                None
            } else {
                Some("homebrew_not_found".to_string())
            },
        },
    }
}

fn command_status(
    detector: &dyn PlaywrightCommandDetector,
    command: &str,
    args: &[&str],
) -> PlaywrightCommandStatus {
    let version = detector.command_version(command, args);
    PlaywrightCommandStatus {
        command: command.to_string(),
        present: version.is_some(),
        version,
    }
}

#[cfg(test)]
#[path = "playwright_discovery_tests.rs"]
mod playwright_discovery_tests;
```

Update `src-tauri/src/browser/mod.rs`:

```rust
pub mod playwright_discovery;
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_discovery
```

Expected: PASS.

## Task 2: Add Setup Planner

**Files:**
- Create: `src-tauri/src/browser/playwright_setup.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Test: `src-tauri/src/browser/playwright_setup_tests.rs`

- [ ] **Step 1: Write failing tests**

Create `src-tauri/src/browser/playwright_setup_tests.rs`:

```rust
use super::playwright_discovery::*;
use super::playwright_setup::*;

#[test]
fn setup_plan_installs_cli_and_skills_when_node_stack_is_ready() {
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
    assert_eq!(plan.steps[0].command, "npm");
    assert_eq!(plan.steps[0].args, vec!["install", "-g", "@playwright/cli@latest"]);
    assert_eq!(plan.steps[1].command, "playwright-cli");
    assert_eq!(plan.steps[1].args, vec!["install", "--skills"]);
    assert!(plan.steps.iter().all(|step| step.command != "sudo"));
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
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_setup
```

Expected: FAIL because module/types do not exist.

- [ ] **Step 3: Implement setup planner**

Create `src-tauri/src/browser/playwright_setup.rs`:

```rust
use serde::{Deserialize, Serialize};

use super::playwright_discovery::{
    PlaywrightSystemDiscoveryReport, PlaywrightSystemStatus,
};

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

pub fn plan_playwright_setup(
    report: &PlaywrightSystemDiscoveryReport,
    action: PlaywrightSetupAction,
) -> PlaywrightSetupPlan {
    match action {
        PlaywrightSetupAction::AutoSetup => plan_auto_setup(report),
        PlaywrightSetupAction::InstallNodeWithHomebrew => plan_homebrew_node(report),
        PlaywrightSetupAction::RefreshSkills => PlaywrightSetupPlan {
            action,
            blocked_reason: None,
            steps: vec![skills_step()],
        },
        PlaywrightSetupAction::ProbeMcp => PlaywrightSetupPlan {
            action,
            blocked_reason: None,
            steps: vec![PlaywrightSetupCommandStep {
                id: "probe_playwright_mcp".to_string(),
                command: "npx".to_string(),
                args: vec!["@playwright/mcp@latest".to_string(), "--help".to_string()],
                timeout_ms: 60_000,
                destructive: false,
            }],
        },
    }
}

fn plan_auto_setup(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if report.status == PlaywrightSystemStatus::NeedsNode {
        return blocked(PlaywrightSetupAction::AutoSetup, "node_required");
    }
    if !report.can_run_global_install {
        return blocked(PlaywrightSetupAction::AutoSetup, "npm_global_prefix_not_writable");
    }
    PlaywrightSetupPlan {
        action: PlaywrightSetupAction::AutoSetup,
        blocked_reason: None,
        steps: vec![
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
            },
            skills_step(),
            PlaywrightSetupCommandStep {
                id: "probe_playwright_mcp".to_string(),
                command: "npx".to_string(),
                args: vec!["@playwright/mcp@latest".to_string(), "--help".to_string()],
                timeout_ms: 60_000,
                destructive: false,
            },
        ],
    }
}

fn plan_homebrew_node(report: &PlaywrightSystemDiscoveryReport) -> PlaywrightSetupPlan {
    if !report.experimental_node_bootstrap.available {
        return blocked(PlaywrightSetupAction::InstallNodeWithHomebrew, "homebrew_not_available");
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

fn skills_step() -> PlaywrightSetupCommandStep {
    PlaywrightSetupCommandStep {
        id: "install_playwright_skills".to_string(),
        command: "playwright-cli".to_string(),
        args: vec!["install".to_string(), "--skills".to_string()],
        timeout_ms: 120_000,
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

#[cfg(test)]
#[path = "playwright_setup_tests.rs"]
mod playwright_setup_tests;
```

Update `src-tauri/src/browser/mod.rs`:

```rust
pub mod playwright_setup;
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_setup
```

Expected: PASS.

## Task 3: Verify And Commit

**Files:**
- Modify: all PR1 files.

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_discovery browser::playwright_setup
```

Expected: PASS.

- [ ] **Step 2: Run rustfmt**

Run:

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/playwright_discovery.rs src-tauri/src/browser/playwright_discovery_tests.rs src-tauri/src/browser/playwright_setup.rs src-tauri/src/browser/playwright_setup_tests.rs
```

Expected: no output.

- [ ] **Step 3: Verify no sudo**

Run:

```bash
rg -n '"sudo"|sudo|su ' src-tauri/src/browser/playwright_discovery.rs src-tauri/src/browser/playwright_setup.rs src-tauri/src/browser/playwright_setup_tests.rs
```

Expected: no matches.

- [ ] **Step 4: Commit**

Run:

```bash
git add src-tauri/src/browser/playwright_discovery.rs src-tauri/src/browser/playwright_discovery_tests.rs src-tauri/src/browser/playwright_setup.rs src-tauri/src/browser/playwright_setup_tests.rs src-tauri/src/browser/mod.rs
git commit -m "feat(browser-runtime): discover and setup official Playwright runtime"
```

Expected: commit succeeds.
