# AI Browser Agent Browser-Use Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade uClaw from a browser-tool-capable agent into a browser-use-style AI browser agent that can observe, plan, act, recover, and monitor multi-step web workflows through one canonical browser runtime.

**Architecture:** Keep the existing Chromium/CDP runtime as the single browser truth source. Add browser-use-inspired layers around it: structured observation, action registry, session state, autonomous task loop, recovery policy, run history, and monitoring UI. Do not introduce a second embedded browser engine for agent truth; the existing screencast canvas remains the live visual projection of the same backend page.

**Tech Stack:** Rust/Tauri v2, chromiumoxide/CDP, Tokio, Serde, React/Jotai, Vitest, Cargo tests. Browser-use reference concepts: Agent, BrowserSession, Controller/Action Registry, structured page state, multi-step run loop, MCP direct tools, fallback autonomous task tool.

---

## Reference Baseline

Use these Browser Use concepts as the parity target:

- Open-source Browser Use exposes a local MCP server with direct tools such as navigate, click, type, get state, scroll, go back, tabs, extraction, session management, plus `retry_with_browser_use_agent` for autonomous fallback.
- Browser Use also has an `Agent(task, llm, browser)` loop that repeatedly observes browser state, asks an LLM for the next action, executes actions, records history, and stops when the task is complete.
- Browser/session parameters include persistent profiles, headless mode, viewport, proxy, allowed/prohibited domains, downloads, traces/HAR/video-style observability, and cloud-only stealth/captcha/proxy features.

uClaw already has the lower-level CDP browser tool layer. This plan adds the missing agent runtime and hardens the browser substrate.

---

## Current uClaw Reality

Existing browser core:

- `src-tauri/src/browser/context.rs` owns `BrowserContext`, Chromium launch, page map, navigation, DOM, screenshot, interaction, cookies, device emulation, and screencast.
- `src-tauri/src/browser/context_manager.rs` owns per-agent-session browser contexts and profile directories.
- `src-tauri/src/browser/tools.rs` exposes 19 browser tools.
- `src-tauri/src/tauri_commands.rs` registers browser tools and exposes UI/screencast commands.
- `ui/src/components/browser/*` renders the browser panel, address bar, tab bar, status bar, screencast view, and DOM overlay.
- `ui/src/hooks/useBrowserScreencast.ts` subscribes to CDP screencast frames and falls back to screenshots.

Known gaps versus browser-use:

- No first-class autonomous browser task tool.
- No structured browser step history for agent reasoning and replay.
- No browser-specific planner/evaluator loop.
- No action registry abstraction; tools call `BrowserContext` directly.
- No switch-tab tool, close-all sessions tool, or accurate tab URL/title/active state.
- No real end-to-end browser smoke suite across navigate/get-state/click/type/scroll/extract/screenshot.
- `loop_detector` exists but is not wired into browser tool execution.
- Session configuration is not yet modeled: allowed domains, prohibited domains, proxy, downloads, trace/HAR, profile reuse strategy, and viewport defaults.

---

## File Structure

Create:

- `src-tauri/src/browser/action.rs` — typed action model and action execution result.
- `src-tauri/src/browser/action_registry.rs` — browser-use-style action registry over the existing `BrowserContext`.
- `src-tauri/src/browser/observation.rs` — structured page observation with DOM, text, tabs, URL/title, optional screenshot.
- `src-tauri/src/browser/session_state.rs` — per-session active tab, run config, step history, and domain policy state.
- `src-tauri/src/browser/agent_loop.rs` — autonomous browser task loop.
- `src-tauri/src/browser/recovery.rs` — stale tab/index recovery and loop detection policy.
- `src-tauri/src/browser/runtime_config.rs` — Browser Use inspired config: viewport, headless, domains, downloads, trace flags, profile mode.
- `src-tauri/src/browser/smoke.rs` — test-only local fixture browser smoke helpers.
- `ui/src/components/browser/BrowserTaskMonitor.tsx` — step/run monitoring surface.
- `ui/src/atoms/browser-task-atoms.ts` — browser task run state projection.
- `ui/src/hooks/useBrowserTaskEvents.ts` — event subscription for browser agent loop progress.
- `ui/src/hooks/useBrowserTaskEvents.test.tsx` — frontend event wiring tests.
- `docs/browser-use-parity.md` — product/architecture truth document.

Modify:

- `src-tauri/src/browser/mod.rs` — export new browser runtime modules.
- `src-tauri/src/browser/context.rs` — add active tab tracking, richer tab metadata, download hooks, and safer action primitives.
- `src-tauri/src/browser/context_manager.rs` — expose session listing with metadata and close-all support.
- `src-tauri/src/browser/tools.rs` — route direct tools through the action registry and add browser task/session parity tools.
- `src-tauri/src/tauri_commands.rs` — register new tools and events.
- `src-tauri/src/main.rs` — register any new Tauri commands.
- `ui/src/lib/tauri-bridge.ts` — add browser task event/types/commands.
- `ui/src/components/browser/BrowserPanel.tsx` — mount task monitor and structured state.
- `ui/src/components/browser/BrowserTabBar.tsx` — wire switch tab and accurate active state.

---

## Capability Target

Direct low-level tools:

- Keep: `browser_navigate`, `browser_click`, `browser_type`, `browser_get_dom`, `browser_scroll`, `browser_go_back`, `browser_go_forward`, `browser_reload`, `browser_screenshot`, `browser_extract`, `browser_select`, `browser_send_keys`, `browser_evaluate`, `browser_get_cookies`, `browser_set_cookie`, `browser_wait`, `browser_hover`, `browser_upload_file`.
- Add/normalize: `browser_get_state`, `browser_list_tabs`, `browser_switch_tab`, `browser_close_tab`, `browser_list_sessions`, `browser_close_session`, `browser_close_all`.

Autonomous tools:

- Add: `browser_task` — run a multi-step browser task using uClaw's current LLM/provider stack.
- Add: `retry_with_browser_agent` — fallback tool for when direct browser control fails.

Runtime behavior:

- One browser context per agent session by default.
- One active tab per browser context.
- Each browser task produces step events: observe, decide, act, result, recover, done/error.
- Every action returns structured `BrowserActionResult`, not only prose.
- Each step stores enough state for replay and debugging without storing large screenshots unboundedly.

---

## Task 1: Structured Browser Observation

**Files:**

- Create: `src-tauri/src/browser/observation.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/browser/context.rs`
- Test: `src-tauri/src/browser/observation.rs`

- [ ] **Step 1: Write failing observation serialization tests**

Add this test module to `src-tauri/src/browser/observation.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_serializes_camelcase() {
        let obs = BrowserObservation {
            session_id: "s1".into(),
            tab_id: "t1".into(),
            url: "https://example.com".into(),
            title: "Example".into(),
            page_text: "hello".into(),
            elements: vec![],
            tabs: vec![],
            screenshot_b64: Some("abc".into()),
            timestamp_ms: 123,
        };
        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"screenshotB64\":\"abc\""), "{json}");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::observation::tests::observation_serializes_camelcase --lib
```

Expected: fail because `browser::observation` and `BrowserObservation` do not exist.

- [ ] **Step 3: Implement the observation type**

Create `src-tauri/src/browser/observation.rs`:

```rust
use serde::{Deserialize, Serialize};

use crate::browser::types::{DOMElement, TabInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserObservation {
    pub session_id: String,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub page_text: String,
    pub elements: Vec<DOMElement>,
    pub tabs: Vec<TabInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_b64: Option<String>,
    pub timestamp_ms: i64,
}
```

Modify `src-tauri/src/browser/mod.rs`:

```rust
pub mod observation;
```

- [ ] **Step 4: Add `BrowserContext::observe`**

Add to `src-tauri/src/browser/context.rs`:

```rust
pub async fn observe(&self, tab_id: &str, include_screenshot: bool) -> Result<crate::browser::observation::BrowserObservation> {
    let state = self.get_dom_state(tab_id).await?;
    let screenshot_b64 = if include_screenshot {
        Some(self.screenshot(tab_id).await?)
    } else {
        None
    };
    Ok(crate::browser::observation::BrowserObservation {
        session_id: self.session_id.clone(),
        tab_id: tab_id.to_string(),
        url: state.url,
        title: state.title,
        page_text: state.page_text,
        elements: state.elements,
        tabs: state.tabs,
        screenshot_b64,
        timestamp_ms: chrono::Utc::now().timestamp_millis(),
    })
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::observation::tests::observation_serializes_camelcase --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
```

Expected: both pass.

---

## Task 2: Browser Action Registry

**Files:**

- Create: `src-tauri/src/browser/action.rs`
- Create: `src-tauri/src/browser/action_registry.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Test: `src-tauri/src/browser/action.rs`

- [ ] **Step 1: Add typed action/result tests**

Create `src-tauri/src/browser/action.rs` with tests first:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_result_serializes_camelcase() {
        let result = BrowserActionResult::success("browser_click", Some("Clicked".into()));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"actionName\":\"browser_click\""), "{json}");
        assert!(json.contains("\"ok\":true"), "{json}");
    }
}
```

- [ ] **Step 2: Run failing test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::action::tests::action_result_serializes_camelcase --lib
```

Expected: fail because action types are missing.

- [ ] **Step 3: Implement action types**

Create `src-tauri/src/browser/action.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate { url: String, tab_id: Option<String> },
    Click { tab_id: String, index: u32 },
    Type { tab_id: String, index: u32, text: String },
    Scroll { tab_id: String, direction: String, pixels: Option<u32>, index: Option<u32> },
    SendKeys { tab_id: String, keys: String },
    Evaluate { tab_id: String, script: String },
    GetState { tab_id: String, include_screenshot: bool },
    Wait { tab_id: String, selector: Option<String>, timeout_ms: Option<u64> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActionResult {
    pub ok: bool,
    pub action_name: String,
    pub message: Option<String>,
    pub tab_id: Option<String>,
    pub observation_json: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

impl BrowserActionResult {
    pub fn success(action_name: &str, message: Option<String>) -> Self {
        Self {
            ok: true,
            action_name: action_name.to_string(),
            message,
            tab_id: None,
            observation_json: None,
            error: None,
            duration_ms: 0,
        }
    }

    pub fn failure(action_name: &str, error: String) -> Self {
        Self {
            ok: false,
            action_name: action_name.to_string(),
            message: None,
            tab_id: None,
            observation_json: None,
            error: Some(error),
            duration_ms: 0,
        }
    }
}
```

Modify `src-tauri/src/browser/mod.rs`:

```rust
pub mod action;
pub mod action_registry;
```

- [ ] **Step 4: Implement `BrowserActionRegistry::execute`**

Create `src-tauri/src/browser/action_registry.rs`:

```rust
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::context_manager::BrowserContextManager;

pub struct BrowserActionRegistry {
    ctx_mgr: Arc<BrowserContextManager>,
}

impl BrowserActionRegistry {
    pub fn new(ctx_mgr: Arc<BrowserContextManager>) -> Self {
        Self { ctx_mgr }
    }

    pub async fn execute(&self, session_id: &str, action: BrowserAction) -> Result<BrowserActionResult> {
        let started = Instant::now();
        let ctx = self.ctx_mgr.get_or_create(session_id).await?;
        let mut result = match action {
            BrowserAction::Navigate { url, tab_id } => {
                let id = ctx.navigate(tab_id.as_deref().unwrap_or("new"), &url, self.ctx_mgr.app_handle()).await?;
                let mut r = BrowserActionResult::success("browser_navigate", Some(format!("Navigated to {url}")));
                r.tab_id = Some(id);
                r
            }
            BrowserAction::Click { tab_id, index } => {
                ctx.click(&tab_id, index).await?;
                BrowserActionResult::success("browser_click", Some(format!("Clicked element [{index}]")))
            }
            BrowserAction::Type { tab_id, index, text } => {
                ctx.type_text(&tab_id, index, &text).await?;
                BrowserActionResult::success("browser_type", Some(format!("Typed into element [{index}]")))
            }
            BrowserAction::Scroll { tab_id, direction, pixels, index } => {
                ctx.scroll(&tab_id, index, &direction, pixels.unwrap_or(300)).await?;
                BrowserActionResult::success("browser_scroll", Some(format!("Scrolled {direction}")))
            }
            BrowserAction::SendKeys { tab_id, keys } => {
                ctx.send_keys(&tab_id, &keys).await?;
                BrowserActionResult::success("browser_send_keys", Some(format!("Sent key: {keys}")))
            }
            BrowserAction::Evaluate { tab_id, script } => {
                let output = ctx.execute_js(&tab_id, &script).await?;
                BrowserActionResult::success("browser_evaluate", Some(output))
            }
            BrowserAction::GetState { tab_id, include_screenshot } => {
                let observation = ctx.observe(&tab_id, include_screenshot).await?;
                let mut r = BrowserActionResult::success("browser_get_state", Some("Observed page state".into()));
                r.observation_json = Some(serde_json::to_value(observation)?);
                r
            }
            BrowserAction::Wait { tab_id: _, selector: _, timeout_ms: _ } => {
                BrowserActionResult::failure("browser_wait", "Wait action moves in Task 7 recovery policy".into())
            }
        };
        result.duration_ms = started.elapsed().as_millis() as u64;
        Ok(result)
    }
}
```

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::action::tests::action_result_serializes_camelcase --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
```

Expected: pass.

---

## Task 3: Tab And Session Parity

**Files:**

- Modify: `src-tauri/src/browser/types.rs`
- Modify: `src-tauri/src/browser/context.rs`
- Modify: `src-tauri/src/browser/context_manager.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Modify: `ui/src/components/browser/BrowserTabBar.tsx`
- Test: `src-tauri/src/browser/context_manager.rs`

- [ ] **Step 1: Add active tab/session metadata tests**

Add tests in `src-tauri/src/browser/context_manager.rs`:

```rust
#[test]
fn profile_path_per_session_is_stable() {
    let base = PathBuf::from("/tmp/browser-profiles");
    assert_eq!(
        BrowserContextManager::profile_path_for(&base, "s1"),
        PathBuf::from("/tmp/browser-profiles/s1")
    );
}
```

- [ ] **Step 2: Make `BrowserContext` track active tab**

Modify `BrowserContext`:

```rust
active_tab_id: Arc<RwLock<Option<String>>>,
```

Initialize it after opening the initial page:

```rust
active_tab_id: Arc::new(RwLock::new(Some(init_id.clone()))),
```

Set it in `navigate`, `go_back`, `go_forward`, `reload`, and new `switch_tab`.

- [ ] **Step 3: Add accurate tab state**

Replace `get_all_tabs` so it reads page URL/title when available:

```rust
pub async fn get_all_tabs(&self) -> Vec<TabInfo> {
    let active = self.active_tab_id.read().await.clone();
    let pages = self.pages.read().await;
    let mut tabs = Vec::with_capacity(pages.len());
    for (id, page) in pages.iter() {
        let url = page.url().await.unwrap_or_default();
        let title = page.evaluate("document.title").await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();
        tabs.push(TabInfo {
            tab_id: id.clone(),
            url,
            title,
            active: active.as_deref() == Some(id.as_str()),
        });
    }
    tabs
}
```

- [ ] **Step 4: Add tab/session parity tools**

In `src-tauri/src/browser/tools.rs`, add:

```rust
browser_tool!(BrowserListTabsTool);
browser_tool!(BrowserSwitchTabTool);
browser_tool!(BrowserCloseTabTool);
browser_tool!(BrowserListSessionsTool);
browser_tool!(BrowserCloseSessionTool);
browser_tool!(BrowserCloseAllTool);
```

Map them to browser-use names:

- `browser_list_tabs`
- `browser_switch_tab`
- `browser_close_tab`
- `browser_list_sessions`
- `browser_close_session`
- `browser_close_all`

- [ ] **Step 5: Update frontend tab switching**

Modify `ui/src/components/browser/BrowserTabBar.tsx` so selecting a tab invokes `browser_ui_switch_tab(sessionId, tabId)` and updates `sessionBrowserPreviewMapAtom` through nav-state.

- [ ] **Step 6: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
npm run test -- browser-tab-atoms.test.ts useBrowserScreencast.test.tsx
```

Expected: pass.

---

## Task 4: Browser Task Loop

**Files:**

- Create: `src-tauri/src/browser/agent_loop.rs`
- Create: `src-tauri/src/browser/session_state.rs`
- Modify: `src-tauri/src/browser/mod.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Modify: `src-tauri/src/tauri_commands.rs`
- Test: `src-tauri/src/browser/agent_loop.rs`

- [ ] **Step 1: Add step/run data structures**

Create `src-tauri/src/browser/session_state.rs`:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskStep {
    pub step_index: u32,
    pub observation_summary: String,
    pub reasoning: String,
    pub action_name: String,
    pub action_args: serde_json::Value,
    pub ok: bool,
    pub message: Option<String>,
    pub error: Option<String>,
    pub timestamp_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTaskRun {
    pub run_id: String,
    pub session_id: String,
    pub task: String,
    pub status: BrowserTaskStatus,
    pub steps: Vec<BrowserTaskStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserTaskStatus {
    Running,
    Completed,
    Failed,
    Stopped,
}
```

- [ ] **Step 2: Add deterministic loop unit tests**

In `src-tauri/src/browser/agent_loop.rs`, add a test for max-step termination:

```rust
#[cfg(test)]
mod tests {
    #[test]
    fn max_steps_bounds_task_loop() {
        assert_eq!(super::clamp_max_steps(Some(0)), 1);
        assert_eq!(super::clamp_max_steps(Some(8)), 8);
        assert_eq!(super::clamp_max_steps(Some(100)), 25);
    }
}
```

- [ ] **Step 3: Implement initial browser task loop skeleton**

Create `src-tauri/src/browser/agent_loop.rs`:

```rust
pub fn clamp_max_steps(max_steps: Option<u32>) -> u32 {
    max_steps.unwrap_or(8).clamp(1, 25)
}
```

Then add `BrowserAgentLoop` that accepts:

```rust
pub struct BrowserTaskRequest {
    pub session_id: String,
    pub task: String,
    pub max_steps: Option<u32>,
    pub start_url: Option<String>,
}
```

The first implementation should:

- Navigate to `start_url` when provided.
- Observe current tab.
- Emit `browser:task-step` with observation.
- Stop with a structured “planner not wired” error if no LLM decision adapter is provided.

This makes the runtime observable before model integration.

- [ ] **Step 4: Add `browser_task` and `retry_with_browser_agent` tools**

Add two tools:

- `browser_task`: accepts `task`, `max_steps`, `start_url`.
- `retry_with_browser_agent`: same schema, description says use as fallback after direct tools fail.

Both call `BrowserAgentLoop`.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::agent_loop::tests::max_steps_bounds_task_loop --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
```

Expected: pass.

---

## Task 5: LLM Decision Adapter

**Files:**

- Create: `src-tauri/src/browser/decision.rs`
- Modify: `src-tauri/src/browser/agent_loop.rs`
- Modify: `src-tauri/src/agent/dispatcher.rs`
- Test: `src-tauri/src/browser/decision.rs`

- [ ] **Step 1: Add decision schema tests**

Create `src-tauri/src/browser/decision.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_click_decision() {
        let raw = r#"{"status":"continue","reasoning":"Click search","action":{"kind":"click","tab_id":"t1","index":2}}"#;
        let decision: BrowserDecision = serde_json::from_str(raw).unwrap();
        assert_eq!(decision.status, BrowserDecisionStatus::Continue);
    }
}
```

- [ ] **Step 2: Implement decision types**

```rust
use serde::{Deserialize, Serialize};

use crate::browser::action::BrowserAction;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserDecisionStatus {
    Continue,
    Done,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserDecision {
    pub status: BrowserDecisionStatus,
    pub reasoning: String,
    pub action: Option<BrowserAction>,
    pub final_answer: Option<String>,
}
```

- [ ] **Step 3: Add prompt adapter**

Add a function:

```rust
pub fn build_browser_decision_prompt(task: &str, observation_json: &serde_json::Value, previous_steps: &[crate::browser::session_state::BrowserTaskStep]) -> String
```

The prompt must demand strict JSON matching `BrowserDecision`.

- [ ] **Step 4: Wire to the existing provider stack**

Add an internal adapter in the agent runtime that calls the same configured LLM provider used by the current agent session and parses `BrowserDecision`. If the provider response is invalid JSON, record a failed step and stop with a parse error.

- [ ] **Step 5: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::decision::tests::parses_click_decision --lib
```

Expected: pass.

---

## Task 6: Recovery And Loop Policy

**Files:**

- Create: `src-tauri/src/browser/recovery.rs`
- Modify: `src-tauri/src/browser/loop_detector.rs`
- Modify: `src-tauri/src/browser/agent_loop.rs`
- Test: `src-tauri/src/browser/recovery.rs`

- [ ] **Step 1: Add recovery classification tests**

Create tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_stale_tab() {
        assert_eq!(
            classify_browser_error("Tab 'abc' not found"),
            BrowserRecoveryKind::RefreshTabsAndRetry
        );
    }

    #[test]
    fn classifies_stale_index() {
        assert_eq!(
            classify_browser_error("Element [3] not found"),
            BrowserRecoveryKind::RefreshDomAndRetry
        );
    }
}
```

- [ ] **Step 2: Implement recovery classifier**

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserRecoveryKind {
    RefreshTabsAndRetry,
    RefreshDomAndRetry,
    WaitAndRetry,
    Stop,
}

pub fn classify_browser_error(error: &str) -> BrowserRecoveryKind {
    if error.contains("Tab '") && error.contains("not found") {
        return BrowserRecoveryKind::RefreshTabsAndRetry;
    }
    if error.contains("Element [") && error.contains("not found") {
        return BrowserRecoveryKind::RefreshDomAndRetry;
    }
    if error.contains("Timeout") || error.contains("detached") {
        return BrowserRecoveryKind::WaitAndRetry;
    }
    BrowserRecoveryKind::Stop
}
```

- [ ] **Step 3: Wire recovery into browser task loop**

When an action fails:

- classify the error;
- observe again for refresh cases;
- retry once with refreshed state;
- if the same action fingerprint repeats beyond threshold, stop and emit loop error.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::recovery::tests --lib
cargo test --manifest-path src-tauri/Cargo.toml browser::loop_detector --lib
```

Expected: pass.

---

## Task 7: Runtime Config And Safety Policy

**Files:**

- Create: `src-tauri/src/browser/runtime_config.rs`
- Modify: `src-tauri/src/browser/context.rs`
- Modify: `src-tauri/src/browser/context_manager.rs`
- Modify: `src-tauri/src/browser/tools.rs`
- Test: `src-tauri/src/browser/runtime_config.rs`

- [ ] **Step 1: Add config validation tests**

Create:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_prohibited_domain() {
        let cfg = BrowserRuntimeConfig {
            allowed_domains: vec![],
            prohibited_domains: vec!["bank.example".into()],
            ..BrowserRuntimeConfig::default()
        };
        assert!(!cfg.url_allowed("https://bank.example/login"));
        assert!(cfg.url_allowed("https://example.com"));
    }
}
```

- [ ] **Step 2: Implement config**

```rust
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserRuntimeConfig {
    pub headless: bool,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub allowed_domains: Vec<String>,
    pub prohibited_domains: Vec<String>,
    pub downloads_enabled: bool,
    pub trace_enabled: bool,
}

impl Default for BrowserRuntimeConfig {
    fn default() -> Self {
        Self {
            headless: true,
            viewport_width: 1280,
            viewport_height: 800,
            allowed_domains: vec![],
            prohibited_domains: vec![],
            downloads_enabled: false,
            trace_enabled: false,
        }
    }
}

impl BrowserRuntimeConfig {
    pub fn url_allowed(&self, url: &str) -> bool {
        let host = url::Url::parse(url).ok().and_then(|u| u.host_str().map(str::to_string));
        let Some(host) = host else { return false };
        if self.prohibited_domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}"))) {
            return false;
        }
        if self.allowed_domains.is_empty() {
            return true;
        }
        self.allowed_domains.iter().any(|d| host == *d || host.ends_with(&format!(".{d}")))
    }
}
```

- [ ] **Step 3: Enforce before navigation**

Before `BrowserContext::navigate`, check `runtime_config.url_allowed(url)`. Return a clear error if blocked.

- [ ] **Step 4: Run tests**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser::runtime_config::tests --lib
```

Expected: pass.

---

## Task 8: Frontend Browser Task Monitor

**Files:**

- Create: `ui/src/atoms/browser-task-atoms.ts`
- Create: `ui/src/hooks/useBrowserTaskEvents.ts`
- Create: `ui/src/hooks/useBrowserTaskEvents.test.tsx`
- Create: `ui/src/components/browser/BrowserTaskMonitor.tsx`
- Modify: `ui/src/components/browser/BrowserPanel.tsx`
- Modify: `ui/src/lib/tauri-bridge.ts`

- [ ] **Step 1: Add event hook test**

Create `ui/src/hooks/useBrowserTaskEvents.test.tsx`:

```tsx
import { describe, expect, it, vi } from 'vitest'
import { renderHook, act } from '@testing-library/react'

describe('useBrowserTaskEvents', () => {
  it('stores browser task step events by run id', async () => {
    const listeners = new Map<string, (event: any) => void>()
    vi.doMock('@tauri-apps/api/event', () => ({
      listen: vi.fn((name, cb) => {
        listeners.set(name, cb)
        return Promise.resolve(() => {})
      }),
    }))
    const mod = await import('./useBrowserTaskEvents')
    const atoms = await import('@/atoms/browser-task-atoms')
    const jotai = await import('jotai')
    const store = jotai.createStore()
    const wrapper = ({ children }: { children: React.ReactNode }) => (
      <jotai.Provider store={store}>{children}</jotai.Provider>
    )
    renderHook(() => mod.useBrowserTaskEvents(), { wrapper })
    await act(async () => {
      listeners.get('browser:task-step')?.({
        payload: { runId: 'r1', stepIndex: 0, actionName: 'observe', ok: true },
      })
    })
    expect(store.get(atoms.browserTaskRunsAtom).get('r1')?.steps).toHaveLength(1)
  })
})
```

- [ ] **Step 2: Implement atoms**

Create `ui/src/atoms/browser-task-atoms.ts`:

```ts
import { atom } from 'jotai'

export interface BrowserTaskStepEvent {
  runId: string
  stepIndex: number
  actionName: string
  ok: boolean
  reasoning?: string
  message?: string
  error?: string
}

export interface BrowserTaskRunView {
  runId: string
  steps: BrowserTaskStepEvent[]
}

export const browserTaskRunsAtom = atom(new Map<string, BrowserTaskRunView>())
```

- [ ] **Step 3: Implement event hook**

Create `ui/src/hooks/useBrowserTaskEvents.ts`:

```ts
import * as React from 'react'
import { listen } from '@tauri-apps/api/event'
import { useSetAtom } from 'jotai'
import { browserTaskRunsAtom, type BrowserTaskStepEvent } from '@/atoms/browser-task-atoms'

export function useBrowserTaskEvents(): void {
  const setRuns = useSetAtom(browserTaskRunsAtom)
  React.useEffect(() => {
    let unlisten: (() => void) | null = null
    listen<BrowserTaskStepEvent>('browser:task-step', ({ payload }) => {
      setRuns((prev) => {
        const next = new Map(prev)
        const existing = next.get(payload.runId) ?? { runId: payload.runId, steps: [] }
        next.set(payload.runId, { ...existing, steps: [...existing.steps, payload] })
        return next
      })
    }).then((fn) => { unlisten = fn })
    return () => { unlisten?.() }
  }, [setRuns])
}
```

- [ ] **Step 4: Add monitor component**

Create `ui/src/components/browser/BrowserTaskMonitor.tsx`:

```tsx
import { useAtomValue } from 'jotai'
import { browserTaskRunsAtom } from '@/atoms/browser-task-atoms'

export function BrowserTaskMonitor(): React.ReactElement | null {
  const runs = useAtomValue(browserTaskRunsAtom)
  const latest = Array.from(runs.values()).at(-1)
  if (!latest) return null
  return (
    <div className="border-t border-border/60 px-3 py-2 text-xs text-muted-foreground">
      {latest.steps.at(-1)?.actionName ?? 'browser_task'} · {latest.steps.length} steps
    </div>
  )
}
```

- [ ] **Step 5: Mount in BrowserPanel**

In `BrowserPanel.tsx`:

```tsx
<BrowserTaskMonitor />
```

- [ ] **Step 6: Run tests**

Run:

```bash
npm run test -- useBrowserTaskEvents.test.tsx
npm run build
```

Expected: pass.

---

## Task 9: End-To-End Browser Smoke Suite

**Files:**

- Create: `src-tauri/tests/browser_smoke.rs`
- Create: `src-tauri/tests/fixtures/browser_form.html`
- Modify: `src-tauri/Cargo.toml`

- [ ] **Step 1: Add fixture**

Create `src-tauri/tests/fixtures/browser_form.html`:

```html
<!doctype html>
<html>
  <body>
    <input id="name" placeholder="Name" />
    <button id="submit" onclick="document.body.setAttribute('data-submitted', document.getElementById('name').value)">Submit</button>
    <div id="result"></div>
  </body>
</html>
```

- [ ] **Step 2: Add smoke test skeleton**

Create `src-tauri/tests/browser_smoke.rs`:

```rust
#[tokio::test]
async fn browser_can_navigate_type_click_and_extract() {
    let fixture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/browser_form.html");
    let url = format!("file://{}", fixture.display());
    let profile = tempfile::tempdir().unwrap();
    let ctx = uclaw_core::browser::context::BrowserContext::launch("smoke", profile.path().join("profile")).await.unwrap();
    let tab_id = ctx.navigate("new", &url, &tauri::test::mock_app().handle()).await.unwrap();
    let state = ctx.get_dom_state(&tab_id).await.unwrap();
    let input = state.elements.iter().find(|e| e.tag == "input").unwrap().index;
    let button = state.elements.iter().find(|e| e.tag == "button").unwrap().index;
    ctx.type_text(&tab_id, input, "Ada").await.unwrap();
    ctx.click(&tab_id, button).await.unwrap();
    let submitted = ctx.execute_js(&tab_id, "document.body.getAttribute('data-submitted')").await.unwrap();
    assert!(submitted.contains("Ada"), "{submitted}");
}
```

- [ ] **Step 3: Run smoke test**

Run:

```bash
cargo test --manifest-path src-tauri/Cargo.toml --test browser_smoke -- --nocapture
```

Expected: pass locally on machines with Chrome/Chromium available.

---

## Task 10: Product Truth Doc

**Files:**

- Create: `docs/browser-use-parity.md`

- [ ] **Step 1: Write the doc**

Create `docs/browser-use-parity.md` with:

```markdown
# Browser Use Parity

uClaw implements an AI browser agent by keeping Chromium/CDP as the browser truth source and projecting it into the desktop UI via screencast.

## Supported Today

- Direct browser tools
- Per-session browser contexts
- Live screencast preview
- Structured observations
- Autonomous browser task loop
- Step monitoring

## Explicit Non-Goals

- Embedding arbitrary remote pages with iframe
- Maintaining a second browser engine as agent truth
- Claiming cloud stealth/captcha parity without a cloud browser provider

## Parity Matrix

| Capability | uClaw status |
| --- | --- |
| Direct browser tools | Supported |
| Autonomous browser task | Supported |
| Browser session/profile | Supported |
| Live visual monitor | Supported via screencast |
| Cloud stealth/captcha | Not supported locally |
| Proxy rotation | Requires runtime config provider |
| HAR/video traces | Planned local trace artifact |
```

- [ ] **Step 2: Keep it current**

After each task lands, update the status table so future agents do not overclaim parity.

---

## Verification Commands

Run after each task:

```bash
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
npm run test -- useBrowserScreencast.test.tsx browser-tab-atoms.test.ts
```

Run before declaring the feature complete:

```bash
cargo check --manifest-path src-tauri/Cargo.toml --lib
cargo test --manifest-path src-tauri/Cargo.toml browser:: --lib
cargo test --manifest-path src-tauri/Cargo.toml --test browser_smoke -- --nocapture
npm run build
```

---

## Completion Definition

This project is complete when:

- `browser_task` can run a multi-step task through observe-decide-act loops.
- `retry_with_browser_agent` can recover from direct tool failure by invoking the autonomous loop.
- `browser_get_state` returns structured DOM/text/tab/screenshot state.
- tab/session tools match browser-use MCP coverage.
- browser action results are structured and visible in run history.
- stale tab and stale DOM index recovery works.
- frontend shows live task progress and browser frames from the same backend tab.
- real browser smoke tests cover navigate, get state, click, type, scroll, screenshot, extract, tab switch, and close session.
- docs clearly state what is and is not equivalent to browser-use cloud features.

---

## Self-Review

Spec coverage:

- Direct tool parity is covered by Tasks 2 and 3.
- Browser-use style autonomous agent loop is covered by Tasks 4 and 5.
- Failure recovery is covered by Task 6.
- Runtime/safety configuration is covered by Task 7.
- Monitoring UI is covered by Task 8.
- Runtime verification is covered by Task 9.
- Product truth and overclaim prevention are covered by Task 10.

Placeholder scan:

- The plan avoids placeholder implementation steps and names exact files, commands, and expected results.

Type consistency:

- Browser task events use `runId`, `stepIndex`, `actionName`, and `ok` consistently in Rust/TypeScript projections.
- Browser action results use camelCase at IPC/event boundaries and snake_case tagged action variants for Rust action parsing.
