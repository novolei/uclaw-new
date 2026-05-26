//! Official Playwright system discovery for Browser Automation.
//!
//! Discovery is read-only: it inspects command availability and setup
//! prerequisites, but never installs packages or mutates the user's shell.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Default)]
pub struct SystemPlaywrightCommandDetector;

impl PlaywrightCommandDetector for SystemPlaywrightCommandDetector {
    fn command_version(&self, command: &str, args: &[&str]) -> Option<String> {
        Command::new(command)
            .args(args)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .and_then(|output| {
                let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !stdout.is_empty() {
                    return Some(stdout);
                }
                let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                (!stderr.is_empty()).then_some(stderr)
            })
    }

    fn npm_global_prefix_writable(&self) -> bool {
        let Some(prefix) = self.command_version("npm", &["prefix", "-g"]) else {
            return false;
        };
        let path = Path::new(prefix.trim());
        path.exists()
            && path
                .metadata()
                .map(|metadata| !metadata.permissions().readonly())
                .unwrap_or(false)
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
    let prefix_writable = has_node_stack && detector.npm_global_prefix_writable();
    let status = if !has_node_stack {
        PlaywrightSystemStatus::NeedsNode
    } else if !prefix_writable {
        PlaywrightSystemStatus::NeedsPermission
    } else if !playwright_cli.present {
        PlaywrightSystemStatus::NeedsSetup
    } else {
        PlaywrightSystemStatus::Ready
    };
    let node_bootstrap_available = !has_node_stack && brew.present && cfg!(target_os = "macos");

    PlaywrightSystemDiscoveryReport {
        status,
        node,
        npm,
        npx,
        playwright_cli,
        can_run_global_install: has_node_stack && prefix_writable,
        experimental_node_bootstrap: ExperimentalNodeBootstrapStatus {
            available: node_bootstrap_available,
            method: node_bootstrap_available.then(|| "homebrew".to_string()),
            reason: node_bootstrap_reason(has_node_stack, brew.present),
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

fn node_bootstrap_reason(has_node_stack: bool, brew_present: bool) -> Option<String> {
    if has_node_stack {
        Some("node_stack_present".to_string())
    } else if !cfg!(target_os = "macos") {
        Some("unsupported_platform".to_string())
    } else if !brew_present {
        Some("homebrew_not_found".to_string())
    } else {
        None
    }
}

#[cfg(test)]
#[path = "playwright_discovery_tests.rs"]
mod playwright_discovery_tests;
