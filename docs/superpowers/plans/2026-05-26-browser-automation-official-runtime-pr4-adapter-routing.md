# Browser Automation Official Runtime PR4 Adapter Routing Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Route browser actions through official Playwright CLI and MCP adapters, support user priority plus capability override, record route evidence, and delete the legacy Playwright MCP sidecar.

**Architecture:** `BrowserProviderActionExecutor` remains the execution boundary. CLI execution goes through `playwright_cli_adapter`; MCP execution goes through `playwright_mcp_adapter` and existing `McpManager`. The old `playwright_mcp_sidecar.rs` is removed after the new path passes tests.

**Tech Stack:** Rust Browser Runtime provider execution, MCP manager, route evidence tests.

---

## File Structure

- Create: `src-tauri/src/browser/playwright_cli_adapter.rs`
- Modify: `src-tauri/src/browser/provider_execution.rs`
- Modify: `src-tauri/src/browser/runtime_execution.rs`
- Modify: `src-tauri/src/browser/playwright_mcp_adapter.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Delete: `src-tauri/src/browser/playwright_mcp_sidecar.rs`
- Test: `src-tauri/src/browser/provider_execution_tests.rs`
- Test: `src-tauri/src/browser/runtime_execution.rs` tests.

## Task 1: Add Official CLI Adapter Shell

**Files:**
- Create: `src-tauri/src/browser/playwright_cli_adapter.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [ ] **Step 1: Add tests**

Create `src-tauri/src/browser/playwright_cli_adapter_tests.rs`:

```rust
use super::playwright_cli_adapter::*;

#[test]
fn setup_uses_official_playwright_cli_command() {
    let command = PlaywrightCliCommand::install_skills();
    assert_eq!(command.command, "playwright-cli");
    assert_eq!(command.args, vec!["install", "--skills"]);
}

#[test]
fn arbitrary_shell_command_is_not_a_cli_action() {
    let err = PlaywrightCliActionCommand::from_skill_command("rm -rf /").unwrap_err();
    assert_eq!(err, PlaywrightCliAdapterError::UnsupportedSkillCommand);
}
```

- [ ] **Step 2: Implement adapter shell**

Create `src-tauri/src/browser/playwright_cli_adapter.rs`:

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightCliAdapterError {
    UnsupportedSkillCommand,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlaywrightCliCommand {
    pub command: String,
    pub args: Vec<String>,
}

impl PlaywrightCliCommand {
    pub fn install_skills() -> Self {
        Self {
            command: "playwright-cli".to_string(),
            args: vec!["install".to_string(), "--skills".to_string()],
        }
    }
}

pub struct PlaywrightCliActionCommand;

impl PlaywrightCliActionCommand {
    pub fn from_skill_command(command: &str) -> Result<PlaywrightCliCommand, PlaywrightCliAdapterError> {
        match command.trim() {
            "playwright-cli install --skills" => Ok(PlaywrightCliCommand::install_skills()),
            _ => Err(PlaywrightCliAdapterError::UnsupportedSkillCommand),
        }
    }
}

#[cfg(test)]
#[path = "playwright_cli_adapter_tests.rs"]
mod playwright_cli_adapter_tests;
```

Update `mod.rs`:

```rust
pub mod playwright_cli_adapter;
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli_adapter
```

Expected: PASS.

## Task 2: Add MCP Route Execution Path

**Files:**
- Modify: `src-tauri/src/browser/provider_execution.rs`
- Modify: `src-tauri/src/browser/playwright_mcp_adapter.rs`

- [ ] **Step 1: Add failing provider test**

In `provider_execution_tests.rs`, add:

```rust
#[test]
fn mcp_selected_route_is_not_blocked_as_local_registry() {
    let mut options = BrowserProviderActionRouteOptions::default();
    options.active_provider_id = Some("browser.playwright_mcp".to_string());
    options.feature_flags.playwright_mcp = true;

    let decision = route_live_browser_action_provider_with_options(
        &BrowserAction::Navigate { url: "https://example.test".into() },
        &options,
    );

    assert_eq!(decision.selected_provider_id.as_deref(), Some("browser.playwright_mcp"));
}
```

Add async execution test with a fake MCP adapter if dependency injection already exists; otherwise first add route-level test and wire execution in the next step.

- [ ] **Step 2: Replace MCP block with adapter call**

In `execute_routed_with_identity`, before `provider_route_blocks_local_action`, add:

```rust
if route_decision.selected_provider_id.as_deref() == Some(PLAYWRIGHT_MCP_PROVIDER_ID) {
    return Ok(self.execute_playwright_mcp_route(session_id, action, route_decision).await);
}
```

Implement `execute_playwright_mcp_route` to map supported actions to `PlaywrightMcpAdapterToolCall` and return a structured provider result. Use a fake/no-op result first if `SharedMcpManager` is not yet available in this executor, then add the manager dependency in the same task:

```rust
BrowserProviderActionExecutionOutcome::Executed(BrowserActionResult {
    success: true,
    action_name: "browser_playwright_mcp_navigate".to_string(),
    summary: "Playwright MCP route selected".to_string(),
    data: serde_json::json!({ "provider": "browser.playwright_mcp" }),
    duration_ms,
})
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution
```

Expected: PASS.

## Task 3: Add Priority And Capability Override Evidence

**Files:**
- Modify: `src-tauri/src/browser/provider_execution.rs`
- Modify: route evidence DTOs if needed.

- [ ] **Step 1: Add tests for override reason**

Add provider execution test:

```rust
#[test]
fn capability_override_selects_mcp_for_snapshot_need() {
    let mut options = BrowserProviderActionRouteOptions::default();
    options.feature_flags.playwright_cli = true;
    options.feature_flags.playwright_mcp = true;
    options.capability_override_reason = Some("locator_discovery_needed".to_string());

    let decision = route_live_browser_action_provider_with_options(
        &BrowserAction::Observe,
        &options,
    );

    assert_eq!(decision.selected_provider_id.as_deref(), Some("browser.playwright_mcp"));
    assert_eq!(decision.route_reason.as_deref(), Some("locator_discovery_needed"));
}
```

- [ ] **Step 2: Add route option field**

Add:

```rust
pub capability_override_reason: Option<String>,
```

to `BrowserProviderActionRouteOptions`.

Use it only for known values:

```rust
const MCP_CAPABILITY_OVERRIDES: &[&str] = &[
    "accessibility_snapshot_needed",
    "locator_discovery_needed",
    "trace_exploration_needed",
    "retryable_with_mcp",
];
```

- [ ] **Step 3: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::provider_execution browser::runtime_execution
```

Expected: PASS.

## Task 4: Delete Legacy MCP Sidecar

**Files:**
- Delete: `src-tauri/src/browser/playwright_mcp_sidecar.rs`
- Modify: `src-tauri/src/browser/playwright_mcp.rs`
- Modify: `src-tauri/src/browser/mod.rs`

- [ ] **Step 1: Remove exports and imports**

Remove from `mod.rs`:

```rust
pub mod playwright_mcp_sidecar;
pub use playwright_mcp_sidecar::{...};
```

Remove sidecar imports from `playwright_mcp.rs` and replace sidecar-specific result types with adapter result types.

- [ ] **Step 2: Delete file**

Run:

```bash
git rm src-tauri/src/browser/playwright_mcp_sidecar.rs
```

Expected: file removed.

- [ ] **Step 3: Find stale sidecar references**

Run:

```bash
rg -n "playwright_mcp_sidecar|supervised_mcp_sidecar|mcp_sidecar|sidecar" src-tauri/src/browser docs ui/src
```

Expected: no production references except historical docs or explicit migration notes.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp browser::playwright_mcp_adapter browser::provider_execution
```

Expected: PASS.

## Task 5: Verify And Commit

- [ ] **Step 1: Run focused tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_cli_adapter browser::playwright_mcp_adapter browser::playwright_mcp browser::provider_execution browser::runtime_execution
```

Expected: PASS.

- [ ] **Step 2: Run rustfmt**

Run:

```bash
rustfmt --edition 2021 --check src-tauri/src/browser/playwright_cli_adapter.rs src-tauri/src/browser/playwright_mcp_adapter.rs src-tauri/src/browser/provider_execution.rs src-tauri/src/browser/runtime_execution.rs src-tauri/src/browser/playwright_mcp.rs
```

Expected: no output.

- [ ] **Step 3: Commit**

Run:

```bash
git add src-tauri/src/browser
git commit -m "feat(browser-runtime): route Playwright through official adapters"
```

Expected: commit succeeds.
