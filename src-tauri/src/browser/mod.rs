pub mod action;
pub mod action_registry;
pub mod agent_loop;
pub mod boundary;
pub mod context;
pub mod context_manager;
pub mod decision;
pub mod dom_state;
pub mod identity;
pub mod identity_ipc;
pub mod identity_tasks;
pub mod intervention_bridge;
pub mod loop_detector; // stub — full implementation in Plan 2 Task 15
pub mod memory_adapter;
pub mod observation;
pub mod perception;
pub mod playwright_cli;
pub mod playwright_mcp;
pub mod provider;
pub mod recovery;
pub mod runtime_contracts;
pub mod runtime_pack;
pub mod runtime_pack_ipc;
pub mod runtime_pack_runner;
pub mod runtime_supervisor;
pub mod script_runner;
pub mod session_state;
pub mod task_store;
pub mod tools;
pub mod types;

// M1-T4c — bridge BrowserTaskRun to runtime::contracts::TaskEvent.
pub mod rollout_bridge;
// Re-export the two primary public types so callers can write
// `crate::browser::BrowserContextManager` without the extra path.
pub use context_manager::BrowserContextManager;
pub use playwright_cli::{
    build_playwright_cli_request_envelope, playwright_cli_capabilities,
    playwright_cli_provider_status, run_playwright_cli_child_worker, PlaywrightCliAction,
    PlaywrightCliActionKind, PlaywrightCliAddress, PlaywrightCliChildWorkerConfig,
    PlaywrightCliEnvelopeError, PlaywrightCliRequestEnvelope, PlaywrightCliWorkerError,
    PlaywrightCliWorkerErrorEnvelope, PlaywrightCliWorkerResultEnvelope,
    PlaywrightCliWorkerStatus,
    PlaywrightCliRuntimeEnv, DEFAULT_PLAYWRIGHT_CLI_ACTION_TIMEOUT_MS,
    PLAYWRIGHT_CLI_DECLARATIVE_ACTIONS, PLAYWRIGHT_CLI_ENVELOPE_SCHEMA_VERSION,
    PLAYWRIGHT_CLI_PROVIDER_ID,
};
pub use playwright_mcp::{
    build_playwright_mcp_request_envelope, build_playwright_mcp_sidecar_spec,
    playwright_mcp_capabilities, playwright_mcp_provider_status, PlaywrightMcpAction,
    PlaywrightMcpActionKind, PlaywrightMcpBrowserName, PlaywrightMcpCapability,
    PlaywrightMcpEnvelopeError, PlaywrightMcpProfileMode, PlaywrightMcpRequestEnvelope,
    PlaywrightMcpSidecarSpec, PlaywrightMcpSidecarSpecError, PlaywrightMcpSidecarSpecRequest,
    DEFAULT_PLAYWRIGHT_MCP_ACTION_TIMEOUT_MS, DEFAULT_PLAYWRIGHT_MCP_NAVIGATION_TIMEOUT_MS,
    PLAYWRIGHT_MCP_DEFAULT_CAPABILITIES, PLAYWRIGHT_MCP_ENVELOPE_SCHEMA_VERSION,
    PLAYWRIGHT_MCP_PACKAGE_NAME, PLAYWRIGHT_MCP_PROVIDER_ID, PLAYWRIGHT_MCP_UCLAW_ACTIONS,
};
pub use provider::{
    local_chromium_capabilities, local_chromium_status, BrowserCapabilityProbe,
    BrowserProbeStatus, BrowserProviderCapabilities, BrowserProviderReadiness,
    BrowserProviderReadinessProbe, BrowserProviderStatus, BrowserSetupCheck,
    LOCAL_CHROMIUM_PROVIDER_ID,
};
pub use runtime_contracts::{
    browser_provider_capability_card, browser_provider_capability_cards, browser_task_event_names,
    is_allowed_browser_runtime_transition, BrowserIdentityMode, BrowserIdentityProjection,
    BrowserProviderCapabilityCard, BrowserProviderLane, BrowserRuntimeFeatureFlags,
    BrowserRuntimeProjection, BrowserRuntimeState, BrowserRuntimeTransition,
    BrowserStartupDoctorProjection, BrowserTaskBoundaryProjection, BrowserTaskBoundaryStatus,
    BrowserTaskEventName, BrowserWorldProjectionSummary, StartupDoctorStatus,
};
pub use runtime_pack::{
    decide_runtime_pack_update, diagnose_runtime_pack, execute_runtime_pack_plan_dry_run,
    execute_runtime_pack_plan_with_runner, inspect_runtime_pack_status, load_runtime_pack_manifest,
    plan_runtime_pack_operation, probe_runtime_pack_filesystem, BrowserRuntimePackAction,
    BrowserRuntimePackDoctorOutcome, BrowserRuntimePackDoctorStatus, BrowserRuntimePackEnvVar,
    BrowserRuntimePackExecutionMode, BrowserRuntimePackExecutionReport,
    BrowserRuntimePackExecutionStatus, BrowserRuntimePackExecutorPolicy,
    BrowserRuntimePackFilesystemProbeOptions, BrowserRuntimePackFilesystemProbeReport,
    BrowserRuntimePackFilesystemSnapshot, BrowserRuntimePackIssue, BrowserRuntimePackManifest,
    BrowserRuntimePackManifestLoadOutcome, BrowserRuntimePackManifestLoadStatus,
    BrowserRuntimePackNetworkState,
    BrowserRuntimePackOperation, BrowserRuntimePackOperationPlan,
    BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths, BrowserRuntimePackPlanStatus,
    BrowserRuntimePackPlanStep, BrowserRuntimePackPlanStepKind, BrowserRuntimePackPlanTrigger,
    BrowserRuntimePackProbe, BrowserRuntimePackReleaseChannel,
    BrowserRuntimePackStatusReport, BrowserRuntimePackStatusRequest,
    BrowserRuntimePackStepExecutionReport, BrowserRuntimePackStepExecutionStatus,
    BrowserRuntimePackStepRunOutcome, BrowserRuntimePackStepRunner,
    BrowserRuntimePackUpdateDecision, BrowserRuntimePackUpdateKind, BrowserRuntimePackUpdatePolicy,
};
pub use runtime_pack_runner::BrowserRuntimePackLocalStepRunner;
pub use runtime_supervisor::{
    BrowserRuntimeArtifactPack, BrowserRuntimeDeadlineProfile, BrowserRuntimeDegradation,
    BrowserRuntimeDoctorOutcome, BrowserRuntimeSessionSummary, BrowserRuntimeSupervisor,
};
pub use types::{DOMState, ScreencastFramePayload};

// ── Legacy BrowserService ─────────────────────────────────────────────
// Kept as-is to power the four existing backward-compat Tauri commands:
// browser_get_state, browser_launch, browser_shutdown, browser_take_screenshot.
// Do NOT add new features here — use BrowserContext / BrowserContextManager.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use futures::StreamExt;
use chromiumoxide::{Browser, Page};
use chromiumoxide::browser::BrowserConfig;
use crate::error::Error;
use self::types::{BrowserState, BrowserTab, ScreenshotResult};

struct BrowserInner {
    browser: Browser,
    pages: HashMap<String, Page>,
}

pub struct BrowserService {
    inner: Arc<RwLock<Option<BrowserInner>>>,
}

impl BrowserService {
    pub fn new() -> Self {
        Self { inner: Arc::new(RwLock::new(None)) }
    }

    pub async fn get_state(&self) -> BrowserState {
        let guard = self.inner.read().await;
        match guard.as_ref() {
            None => BrowserState { running: false, tabs: vec![], active_tab_id: None },
            Some(inner) => {
                let tabs: Vec<BrowserTab> = inner.pages.iter().map(|(id, _)| BrowserTab {
                    tab_id: id.clone(), url: String::new(), title: String::new(),
                }).collect();
                let active_tab_id = tabs.first().map(|t| t.tab_id.clone());
                BrowserState { running: true, tabs, active_tab_id }
            }
        }
    }

    pub async fn launch(&self) -> Result<(), Error> {
        let mut guard = self.inner.write().await;
        if guard.is_some() { return Ok(()); }
        let profile_dir = uclaw_utils_home::uclaw_home_pathbuf()
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/.uclaw"))
            .join("browser-profile");
        if let Err(e) = std::fs::create_dir_all(&profile_dir) {
            tracing::warn!("Could not create browser profile dir: {}", e);
        }
        for lock in &["SingletonLock", "SingletonCookie", "SingletonSocket"] {
            let path = profile_dir.join(lock);
            if path.exists() { let _ = std::fs::remove_file(&path); }
        }
        let config = BrowserConfig::builder()
            .no_sandbox().user_data_dir(&profile_dir)
            .launch_timeout(Duration::from_secs(60))
            .args(["--no-first-run","--disable-default-apps","--disable-infobars",
                   "--disable-notifications","--disable-translate","--disable-extensions"])
            .build()
            .map_err(|e| Error::Internal(format!("Browser config error: {}", e)))?;
        let (browser, mut handler) = Browser::launch(config).await
            .map_err(|e| Error::Internal(format!("Failed to launch browser: {}", e)))?;
        tokio::spawn(async move { while let Some(_) = handler.next().await {} });
        *guard = Some(BrowserInner { browser, pages: HashMap::new() });
        Ok(())
    }

    pub async fn shutdown(&self) -> Result<(), Error> {
        *self.inner.write().await = None;
        Ok(())
    }

    pub async fn navigate(&self, tab_id: &str, url: &str) -> Result<String, Error> {
        let mut guard = self.inner.write().await;
        let inner = guard.as_mut().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        if tab_id == "new" || !inner.pages.contains_key(tab_id) {
            let page = inner.browser.new_page(url).await
                .map_err(|e| Error::Internal(format!("Failed to open page: {}", e)))?;
            let new_id = uuid::Uuid::new_v4().to_string();
            inner.pages.insert(new_id.clone(), page);
            Ok(new_id)
        } else {
            let page = inner.pages.get(tab_id).unwrap();
            page.goto(url).await
                .map_err(|e| Error::Internal(format!("Navigation failed: {}", e)))?;
            Ok(tab_id.to_string())
        }
    }

    pub async fn screenshot(&self, tab_id: &str) -> Result<ScreenshotResult, Error> {
        use chromiumoxide::page::ScreenshotParams;
        use base64::{Engine, engine::general_purpose::STANDARD};
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(tab_not_found_message(tab_id, &inner.pages)))?;
        let png_bytes = page.screenshot(ScreenshotParams::default()).await
            .map_err(|e| Error::Internal(format!("Screenshot failed: {}", e)))?;
        Ok(ScreenshotResult { data: STANDARD.encode(&png_bytes), width: 1280, height: 800 })
    }

    pub async fn extract_text(&self, tab_id: &str) -> Result<String, Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(tab_not_found_message(tab_id, &inner.pages)))?;
        let text = page.evaluate("document.body.innerText").await
            .map_err(|e| Error::Internal(format!("Extract failed: {}", e)))?
            .into_value::<String>()
            .unwrap_or_default();
        Ok(text)
    }

    pub async fn click(&self, tab_id: &str, selector: &str) -> Result<(), Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(tab_not_found_message(tab_id, &inner.pages)))?;
        page.find_element(selector).await
            .map_err(|e| Error::Internal(format!("Element '{}' not found: {}", selector, e)))?
            .click().await
            .map_err(|e| Error::Internal(format!("Click failed: {}", e)))?;
        Ok(())
    }

    pub async fn type_text(&self, tab_id: &str, selector: &str, text: &str) -> Result<(), Error> {
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(tab_not_found_message(tab_id, &inner.pages)))?;
        page.find_element(selector).await
            .map_err(|e| Error::Internal(format!("Element '{}' not found: {}", selector, e)))?
            .type_str(text).await
            .map_err(|e| Error::Internal(format!("Type failed: {}", e)))?;
        Ok(())
    }

    pub async fn wait_for_selector(&self, tab_id: &str, selector: &str, timeout_ms: u64) -> Result<(), Error> {
        use tokio::time::{timeout, Duration};
        let guard = self.inner.read().await;
        let inner = guard.as_ref().ok_or_else(|| Error::Internal("Browser not launched".into()))?;
        let page = inner.pages.get(tab_id)
            .ok_or_else(|| Error::Internal(tab_not_found_message(tab_id, &inner.pages)))?;
        timeout(Duration::from_millis(timeout_ms), page.find_element(selector)).await
            .map_err(|_| Error::Internal(format!("Timeout waiting for '{}'", selector)))?
            .map_err(|e| Error::Internal(format!("Wait failed: {}", e)))?;
        Ok(())
    }
}

impl Default for BrowserService {
    fn default() -> Self { Self::new() }
}

/// Bundle 19 — build a "tab not found" error message that gives the
/// LLM a concrete recovery path instead of the bare `Tab 'X' not
/// found`. The LLM often invents `tab_id="new"` because
/// `browser_navigate`'s schema documents `new` as a sentinel; other
/// browser tools take the resulting real tab id and don't accept
/// `new`. The expanded message names what's wrong and what to do.
fn tab_not_found_message(tab_id: &str, available: &HashMap<String, Page>) -> String {
    let mut msg = format!("Tab '{}' not found.", tab_id);
    if tab_id == "new" {
        msg.push_str(
            " 'new' is only valid as the tab_id for `browser_navigate` \
             (where it opens a new tab and returns the real id). For \
             every other browser tool, pass a tab_id that came back \
             from a prior `browser_navigate` call."
        );
    } else {
        msg.push_str(
            " The tab_id must be a value returned by a prior \
             `browser_navigate` call."
        );
    }
    if !available.is_empty() {
        let mut ids: Vec<&str> = available.keys().map(String::as_str).collect();
        ids.sort();
        // Cap at 5 to keep the LLM-visible message short — the agent
        // gets a hint, not the whole tab inventory.
        let preview: Vec<&str> = ids.iter().take(5).copied().collect();
        if ids.len() > 5 {
            msg.push_str(&format!(
                " Open tabs (first 5 of {}): {}",
                ids.len(),
                preview.join(", "),
            ));
        } else {
            msg.push_str(&format!(" Open tabs: {}", preview.join(", ")));
        }
    } else {
        msg.push_str(" No tabs are currently open.");
    }
    msg
}
