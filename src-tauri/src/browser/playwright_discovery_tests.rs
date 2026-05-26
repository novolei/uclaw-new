use super::*;

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
    assert!(!report.can_run_global_install);
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
    assert!(!report.experimental_node_bootstrap.available);
    assert_eq!(
        report.experimental_node_bootstrap.reason.as_deref(),
        Some("node_stack_present")
    );
}

#[test]
fn missing_node_with_homebrew_enables_macos_bootstrap_only() {
    let report = inspect_playwright_system_with_detector(&StaticDetector {
        node: None,
        npm: None,
        npx: None,
        playwright_cli: None,
        npm_prefix_writable: false,
        brew: Some("Homebrew 4.0.0"),
    });

    assert_eq!(
        report.experimental_node_bootstrap.available,
        cfg!(target_os = "macos")
    );
    assert_eq!(
        report.experimental_node_bootstrap.method.as_deref(),
        cfg!(target_os = "macos").then_some("homebrew")
    );
}

#[test]
fn ready_requires_cli_and_writable_global_prefix() {
    let report = inspect_playwright_system_with_detector(&StaticDetector {
        node: Some("v22.0.0"),
        npm: Some("10.0.0"),
        npx: Some("10.0.0"),
        playwright_cli: Some("0.0.1"),
        npm_prefix_writable: true,
        brew: None,
    });

    assert_eq!(report.status, PlaywrightSystemStatus::Ready);
    assert!(report.playwright_cli.present);
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
