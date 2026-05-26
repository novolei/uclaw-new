# Browser Automation Official Runtime PR3 Playwright MCP Via McpManager Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Register Playwright MCP as a built-in uClaw MCP server using existing `McpManager`, with official `npx @playwright/mcp@latest`, tool allowlist, and no raw tool exposure by default.

**Architecture:** MCP lifecycle belongs to `src-tauri/src/mcp.rs`. Browser Runtime owns only a Playwright MCP adapter that seeds config and calls allowlisted tools through `McpManager::call_tool`.

**Tech Stack:** Rust MCP manager, serde config DTOs, Browser Runtime MCP adapter tests.

---

## File Structure

- Modify: `src-tauri/src/mcp.rs`
- Create: `src-tauri/src/browser/playwright_mcp_adapter.rs`
- Modify: `src-tauri/src/browser/playwright_mcp.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Test: `src-tauri/src/browser/playwright_mcp_adapter_tests.rs`
- Test: `src-tauri/src/mcp.rs` tests.

## Task 1: Add Built-In Playwright MCP Config

**Files:**
- Modify: `src-tauri/src/mcp.rs`

- [ ] **Step 1: Add tests**

Add to `mcp.rs` tests:

```rust
#[test]
fn seed_builtin_playwright_mcp_adds_official_npx_server() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut mgr = McpManager::new(dir.path());

    let seeded = mgr.seed_builtin_playwright_mcp().expect("seed");
    assert!(seeded);

    let cfg = mgr.server_config("playwright").expect("config");
    assert_eq!(cfg.command, "npx");
    assert_eq!(cfg.args[0], "@playwright/mcp@latest");
    assert_eq!(
        cfg.tool_allowlist.as_deref(),
        Some(playwright_mcp_tool_allowlist().as_slice())
    );
}

#[test]
fn seed_builtin_playwright_mcp_refreshes_managed_entry() {
    let dir = tempfile::tempdir().expect("temp dir");
    let mut mgr = McpManager::new(dir.path());
    mgr.seed_builtin_playwright_mcp().expect("seed");

    let refreshed = mgr.seed_builtin_playwright_mcp().expect("refresh");
    assert!(!refreshed);
}
```

- [ ] **Step 2: Run tests to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib mcp::tests::seed_builtin_playwright_mcp
```

Expected: FAIL because functions do not exist.

- [ ] **Step 3: Implement config helpers**

Add near bundled gbrain helpers:

```rust
pub fn playwright_mcp_tool_allowlist() -> Vec<String> {
    vec![
        "browser_snapshot".to_string(),
        "browser_navigate".to_string(),
        "browser_click".to_string(),
        "browser_type".to_string(),
        "browser_take_screenshot".to_string(),
        "browser_start_tracing".to_string(),
        "browser_stop_tracing".to_string(),
    ]
}

fn builtin_playwright_mcp_config() -> McpServerConfig {
    McpServerConfig {
        id: "playwright".to_string(),
        name: "Playwright MCP (built-in)".to_string(),
        description: "Official Playwright MCP server managed by uClaw Browser Automation.".to_string(),
        transport_type: TransportType::Stdio,
        command: "npx".to_string(),
        args: vec!["@playwright/mcp@latest".to_string()],
        env: HashMap::new(),
        url: None,
        enabled: true,
        auto_approve: false,
        tool_allowlist: Some(playwright_mcp_tool_allowlist()),
    }
}
```

Add to `impl McpManager`:

```rust
pub fn seed_builtin_playwright_mcp(&mut self) -> Result<bool, String> {
    let config = builtin_playwright_mcp_config();
    if let Some(state) = self.servers.get_mut("playwright") {
        state.config = config;
        self.save_config();
        return Ok(false);
    }
    self.add_server(config)?;
    Ok(true)
}
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib mcp
```

Expected: PASS.

## Task 2: Add Browser Playwright MCP Adapter

**Files:**
- Create: `src-tauri/src/browser/playwright_mcp_adapter.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Test: `src-tauri/src/browser/playwright_mcp_adapter_tests.rs`

- [ ] **Step 1: Add adapter tests**

Create `src-tauri/src/browser/playwright_mcp_adapter_tests.rs`:

```rust
use super::playwright_mcp_adapter::*;

#[test]
fn maps_browser_action_to_allowlisted_mcp_tool() {
    let call = PlaywrightMcpAdapterToolCall::navigate("https://example.test");
    assert_eq!(call.tool_name, "browser_navigate");
    assert_eq!(call.arguments["url"], "https://example.test");
}

#[test]
fn rejects_unknown_raw_tool() {
    let err = validate_playwright_mcp_tool("browser_press_key").unwrap_err();
    assert_eq!(err, PlaywrightMcpAdapterError::RawToolNotAllowed);
}
```

- [ ] **Step 2: Run test to verify failure**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp_adapter
```

Expected: FAIL because module does not exist.

- [ ] **Step 3: Implement adapter types**

Create `src-tauri/src/browser/playwright_mcp_adapter.rs`:

```rust
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

pub const PLAYWRIGHT_MCP_SERVER_ID: &str = "playwright";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PlaywrightMcpAdapterError {
    RawToolNotAllowed,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightMcpAdapterToolCall {
    pub server_id: String,
    pub tool_name: String,
    pub arguments: Value,
}

impl PlaywrightMcpAdapterToolCall {
    pub fn navigate(url: &str) -> Self {
        Self {
            server_id: PLAYWRIGHT_MCP_SERVER_ID.to_string(),
            tool_name: "browser_navigate".to_string(),
            arguments: json!({ "url": url }),
        }
    }
}

pub fn validate_playwright_mcp_tool(tool_name: &str) -> Result<(), PlaywrightMcpAdapterError> {
    let allowed = [
        "browser_snapshot",
        "browser_navigate",
        "browser_click",
        "browser_type",
        "browser_take_screenshot",
        "browser_start_tracing",
        "browser_stop_tracing",
    ];
    if allowed.contains(&tool_name) {
        Ok(())
    } else {
        Err(PlaywrightMcpAdapterError::RawToolNotAllowed)
    }
}

#[cfg(test)]
#[path = "playwright_mcp_adapter_tests.rs"]
mod playwright_mcp_adapter_tests;
```

Update `mod.rs`:

```rust
pub mod playwright_mcp_adapter;
```

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib browser::playwright_mcp_adapter
```

Expected: PASS.

## Task 3: Wire Built-In Seed At App Startup

**Files:**
- Modify: the app boot path that currently seeds bundled gbrain.

- [ ] **Step 1: Locate gbrain seed**

Run:

```bash
rg -n "seed_bundled_gbrain|connect_all_enabled" src-tauri/src
```

Expected: identify the app startup location.

- [ ] **Step 2: Add Playwright MCP seed after MCP manager creation**

Add:

```rust
if let Err(error) = mcp_manager.write().await.seed_builtin_playwright_mcp() {
    tracing::warn!(error = %error, "failed to seed built-in Playwright MCP server");
}
```

Use the exact manager variable and lock pattern in that file.

- [ ] **Step 3: Run focused compile test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --lib mcp browser::playwright_mcp_adapter
```

Expected: PASS.

## Task 4: Verify And Commit

- [ ] **Step 1: Verify no sidecar changes yet**

Run:

```bash
git diff --name-only | rg "playwright_mcp_sidecar" || true
```

Expected: no output; deletion happens in PR4.

- [ ] **Step 2: Run rustfmt**

Run:

```bash
rustfmt --edition 2021 --check src-tauri/src/mcp.rs src-tauri/src/browser/playwright_mcp_adapter.rs src-tauri/src/browser/playwright_mcp_adapter_tests.rs
```

Expected: no output.

- [ ] **Step 3: Commit**

Run:

```bash
git add src-tauri/src/mcp.rs src-tauri/src/browser/playwright_mcp_adapter.rs src-tauri/src/browser/playwright_mcp_adapter_tests.rs src-tauri/src/browser/mod.rs
git commit -m "feat(browser-runtime): seed Playwright MCP through MCP manager"
```

Expected: commit succeeds.
