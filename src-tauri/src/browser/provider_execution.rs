use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde_json::json;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::action_registry::BrowserActionRegistry;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::playwright_cli::{
    execute_official_playwright_cli_provider_action, playwright_cli_provider_status,
    PlaywrightCliAction, PlaywrightCliAddress, PlaywrightCliOfficialCommandConfig,
    PlaywrightCliProviderExecutionResult, PlaywrightCliProviderExecutionStatus,
    PLAYWRIGHT_CLI_PROVIDER_ID,
};
use crate::browser::playwright_mcp::{
    playwright_mcp_provider_result_from_adapter_call, playwright_mcp_provider_status,
    PlaywrightMcpAction, PlaywrightMcpProviderArtifactRef, PlaywrightMcpProviderExecutionError,
    PlaywrightMcpProviderExecutionResult, PlaywrightMcpProviderExecutionStatus,
    PLAYWRIGHT_MCP_PROVIDER_ID,
};
use crate::browser::playwright_mcp_adapter::PlaywrightMcpAdapterToolCall;
use crate::browser::provider::{
    local_chromium_status, BrowserCapabilityProbe, BrowserProviderReadinessProbe,
    BrowserProviderRouteDecision, BrowserProviderRouteDecisionStatus,
    BrowserProviderRouteSkippedProvider, BrowserProviderRouter, BrowserSetupCheck,
    LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{
    BrowserProviderSelectionRequest, BrowserRuntimeFeatureFlags, BrowserTaskEventName,
};
use crate::browser::runtime_pack::BrowserRuntimePackStatusReport;
use crate::mcp::{CallToolResult, ContentBlock, SharedMcpManager};

static NEXT_PROVIDER_ACTION_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub struct BrowserProviderActionExecutor {
    action_registry: BrowserActionRegistry,
    route_options: BrowserProviderActionRouteOptions,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProviderSkippedReason {
    pub provider_id: String,
    pub reason: String,
}

#[derive(Clone)]
pub struct BrowserProviderActionRouteOptions {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub runtime_report: Option<BrowserRuntimePackStatusReport>,
    pub disabled_provider_ids: Vec<String>,
    pub active_provider_id: Option<String>,
    pub skipped_provider_reasons: Vec<BrowserProviderSkippedReason>,
    pub capability_override_reason: Option<String>,
    pub playwright_cli_command: Option<std::path::PathBuf>,
    pub playwright_cli_cwd: Option<std::path::PathBuf>,
    pub mcp_manager: Option<SharedMcpManager>,
}

impl Default for BrowserProviderActionRouteOptions {
    fn default() -> Self {
        Self {
            feature_flags: BrowserRuntimeFeatureFlags::safe_defaults(),
            runtime_report: None,
            disabled_provider_ids: Vec::new(),
            active_provider_id: None,
            skipped_provider_reasons: Vec::new(),
            capability_override_reason: None,
            playwright_cli_command: None,
            playwright_cli_cwd: None,
            mcp_manager: None,
        }
    }
}

const MCP_CAPABILITY_OVERRIDES: &[&str] = &[
    "accessibility_snapshot_needed",
    "locator_discovery_needed",
    "trace_exploration_needed",
    "retryable_with_mcp",
];

impl BrowserProviderActionRouteOptions {
    pub fn with_feature_flags(mut self, feature_flags: BrowserRuntimeFeatureFlags) -> Self {
        self.feature_flags = feature_flags;
        self
    }

    pub fn with_runtime_report(mut self, runtime_report: BrowserRuntimePackStatusReport) -> Self {
        self.runtime_report = Some(runtime_report);
        self
    }

    pub fn with_disabled_provider(mut self, provider_id: impl Into<String>) -> Self {
        let provider_id = provider_id.into();
        if !self
            .disabled_provider_ids
            .iter()
            .any(|disabled| disabled == &provider_id)
        {
            self.disabled_provider_ids.push(provider_id);
            self.disabled_provider_ids.sort();
        }
        self
    }

    pub fn with_active_control_center_route(
        mut self,
        active_provider_id: impl Into<String>,
        skipped: Vec<BrowserProviderSkippedReason>,
    ) -> Self {
        self.active_provider_id = Some(active_provider_id.into());
        self.skipped_provider_reasons = skipped;
        self
    }

    pub fn with_capability_override_reason(mut self, reason: impl Into<String>) -> Self {
        self.capability_override_reason = Some(reason.into());
        self
    }

    pub fn with_playwright_cli_command(mut self, command: impl Into<std::path::PathBuf>) -> Self {
        self.playwright_cli_command = Some(command.into());
        self
    }

    pub fn with_playwright_cli_cwd(mut self, cwd: impl Into<std::path::PathBuf>) -> Self {
        self.playwright_cli_cwd = Some(cwd.into());
        self
    }

    pub fn with_mcp_manager(mut self, mcp_manager: SharedMcpManager) -> Self {
        self.mcp_manager = Some(mcp_manager);
        self
    }
}

#[derive(Debug, Clone)]
pub struct BrowserProviderActionExecution {
    pub route_decision: BrowserProviderRouteDecision,
    pub outcome: BrowserProviderActionExecutionOutcome,
}

#[derive(Debug, Clone)]
pub enum BrowserProviderActionExecutionOutcome {
    Executed(BrowserActionResult),
    Blocked(BrowserProviderActionBlocked),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserProviderActionBlocked {
    pub selected_provider_id: Option<String>,
    pub message: String,
}

impl BrowserProviderActionExecutor {
    pub fn new(ctx_mgr: Arc<BrowserContextManager>) -> Self {
        Self {
            action_registry: BrowserActionRegistry::new(ctx_mgr),
            route_options: BrowserProviderActionRouteOptions::default(),
        }
    }

    pub fn with_route_options(mut self, route_options: BrowserProviderActionRouteOptions) -> Self {
        self.route_options = route_options;
        self
    }

    pub fn route_action(&self, action: &BrowserAction) -> BrowserProviderRouteDecision {
        route_live_browser_action_provider_with_options(action, &self.route_options)
    }

    pub async fn execute_routed_with_identity(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
        action: BrowserAction,
        route_decision: BrowserProviderRouteDecision,
    ) -> Result<BrowserProviderActionExecution> {
        if route_decision.selected_provider_id.as_deref() == Some(PLAYWRIGHT_CLI_PROVIDER_ID) {
            let cli_execution = self
                .execute_playwright_cli_route(
                    session_id,
                    identity_profile_id,
                    action,
                    route_decision,
                )
                .await;
            return Ok(self
                .retry_failed_cli_with_mcp(session_id, cli_execution)
                .await);
        }

        if route_decision.selected_provider_id.as_deref() == Some(PLAYWRIGHT_MCP_PROVIDER_ID) {
            return Ok(self
                .execute_playwright_mcp_route(session_id, action, route_decision)
                .await);
        }

        if provider_route_blocks_local_action(&route_decision) {
            return Ok(BrowserProviderActionExecution {
                route_decision: route_decision.clone(),
                outcome: BrowserProviderActionExecutionOutcome::Blocked(
                    BrowserProviderActionBlocked {
                        selected_provider_id: route_decision.selected_provider_id.clone(),
                        message: "Browser provider route is not executable by the local Chromium action registry."
                            .to_string(),
                    },
                ),
            });
        }

        let result = self
            .action_registry
            .execute_with_identity(session_id, identity_profile_id, action)
            .await?;
        Ok(BrowserProviderActionExecution {
            route_decision,
            outcome: BrowserProviderActionExecutionOutcome::Executed(result),
        })
    }

    async fn execute_playwright_cli_route(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
        action: BrowserAction,
        route_decision: BrowserProviderRouteDecision,
    ) -> BrowserProviderActionExecution {
        let input_action = action.clone();
        let cli_action = match playwright_cli_action_for_browser_action(action) {
            Ok(cli_action) => cli_action,
            Err(blocked) => {
                return BrowserProviderActionExecution {
                    route_decision,
                    outcome: BrowserProviderActionExecutionOutcome::Blocked(blocked),
                };
            }
        };

        let started = Instant::now();
        let mut command_config = PlaywrightCliOfficialCommandConfig::for_uclaw_session(session_id);
        if let Some(command) = self.route_options.playwright_cli_command.clone() {
            command_config = command_config.with_command(command);
        }
        if let Some(cwd) = self.route_options.playwright_cli_cwd.clone() {
            command_config = command_config.with_cwd(cwd);
        }
        let request_id = next_provider_action_request_id(session_id);
        let provider_result = execute_official_playwright_cli_provider_action(
            request_id,
            self.route_options.feature_flags,
            cli_action,
            command_config,
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;
        let mut action_result =
            browser_action_result_from_playwright_cli(provider_result, duration_ms, &input_action);
        self.mirror_playwright_cli_preview(
            session_id,
            identity_profile_id,
            &input_action,
            &mut action_result,
        )
        .await;
        BrowserProviderActionExecution {
            route_decision,
            outcome: BrowserProviderActionExecutionOutcome::Executed(action_result),
        }
    }

    async fn mirror_playwright_cli_preview(
        &self,
        session_id: &str,
        identity_profile_id: Option<&str>,
        input_action: &BrowserAction,
        provider_result: &mut BrowserActionResult,
    ) {
        if !provider_result.ok || !self.action_registry.supports_live_preview_events() {
            return;
        }
        let Some(mirror_action) = playwright_cli_preview_mirror_action(input_action) else {
            return;
        };
        let provider_tab_id = provider_result.tab_id.clone();
        match self
            .action_registry
            .execute_with_identity(session_id, identity_profile_id, mirror_action)
            .await
        {
            Ok(preview_result) => {
                let preview_tab_id = preview_result
                    .tab_id
                    .clone()
                    .or_else(|| tab_id_from_browser_action(input_action));
                if let Some(preview_tab_id) = preview_tab_id {
                    apply_preview_mirror_success(provider_result, provider_tab_id, preview_tab_id);
                }
            }
            Err(error) => {
                tracing::warn!(
                    session_id,
                    provider_tab_id = provider_tab_id.as_deref().unwrap_or(""),
                    error = %error,
                    "Playwright CLI preview mirror failed; provider action remains successful"
                );
                apply_preview_mirror_failure(provider_result, provider_tab_id, error.to_string());
            }
        }
    }

    async fn retry_failed_cli_with_mcp(
        &self,
        session_id: &str,
        cli_execution: BrowserProviderActionExecution,
    ) -> BrowserProviderActionExecution {
        let BrowserProviderActionExecution {
            route_decision,
            outcome,
        } = cli_execution;
        let BrowserProviderActionExecutionOutcome::Executed(ref result) = outcome else {
            return BrowserProviderActionExecution {
                route_decision,
                outcome,
            };
        };
        if result.ok || !self.can_retry_cli_failure_with_mcp() {
            return BrowserProviderActionExecution {
                route_decision,
                outcome,
            };
        }
        let Some(original_action) = result
            .observation_json
            .as_ref()
            .and_then(|value| value.get("inputAction"))
            .and_then(|value| serde_json::from_value::<BrowserAction>(value.clone()).ok())
        else {
            return BrowserProviderActionExecution {
                route_decision,
                outcome,
            };
        };
        if playwright_mcp_action_for_browser_action(original_action.clone()).is_err() {
            return BrowserProviderActionExecution {
                route_decision,
                outcome,
            };
        }

        let previous_error = result
            .error
            .clone()
            .unwrap_or_else(|| "Playwright CLI execution failed.".to_string());
        let fallback_decision =
            mcp_retry_route_decision(&route_decision, "playwright_cli_execution_failed");
        let mut mcp_execution = self
            .execute_playwright_mcp_route(session_id, original_action, fallback_decision)
            .await;
        if let BrowserProviderActionExecutionOutcome::Executed(ref mut fallback_result) =
            mcp_execution.outcome
        {
            merge_cli_fallback_route_evidence(fallback_result, &route_decision, &previous_error);
        }
        mcp_execution
    }

    fn can_retry_cli_failure_with_mcp(&self) -> bool {
        self.route_options.feature_flags.playwright_mcp
            && !self
                .route_options
                .disabled_provider_ids
                .iter()
                .any(|provider_id| provider_id == PLAYWRIGHT_MCP_PROVIDER_ID)
            && !self
                .route_options
                .skipped_provider_reasons
                .iter()
                .any(|item| item.provider_id == PLAYWRIGHT_MCP_PROVIDER_ID)
    }

    async fn execute_playwright_mcp_route(
        &self,
        session_id: &str,
        action: BrowserAction,
        route_decision: BrowserProviderRouteDecision,
    ) -> BrowserProviderActionExecution {
        let mcp_action = match playwright_mcp_action_for_browser_action(action) {
            Ok(mcp_action) => mcp_action,
            Err(blocked) => {
                return BrowserProviderActionExecution {
                    route_decision,
                    outcome: BrowserProviderActionExecutionOutcome::Blocked(blocked),
                };
            }
        };
        let call = match PlaywrightMcpAdapterToolCall::from_action(&mcp_action) {
            Ok(call) => call,
            Err(_) => {
                return BrowserProviderActionExecution {
                    route_decision: route_decision.clone(),
                    outcome: BrowserProviderActionExecutionOutcome::Blocked(
                        BrowserProviderActionBlocked {
                            selected_provider_id: route_decision.selected_provider_id.clone(),
                            message: "Selected Playwright MCP route is not allowlisted by the Browser Runtime adapter."
                                .to_string(),
                        },
                    ),
                };
            }
        };
        let started = Instant::now();
        let request_id = next_provider_action_request_id(session_id);
        let provider_result = if let Some(mcp_manager) = self.route_options.mcp_manager.as_ref() {
            execute_playwright_mcp_adapter_call(
                mcp_manager,
                request_id,
                session_id,
                &call,
                &route_decision,
            )
            .await
        } else {
            playwright_mcp_provider_result_from_adapter_call(
                request_id,
                &call,
                playwright_mcp_adapter_evidence_output(session_id, &call, &route_decision, None),
            )
        };
        let duration_ms = started.elapsed().as_millis() as u64;

        BrowserProviderActionExecution {
            route_decision,
            outcome: BrowserProviderActionExecutionOutcome::Executed(
                browser_action_result_from_playwright_mcp(provider_result, duration_ms),
            ),
        }
    }
}

async fn execute_playwright_mcp_adapter_call(
    mcp_manager: &SharedMcpManager,
    request_id: String,
    session_id: &str,
    call: &PlaywrightMcpAdapterToolCall,
    route_decision: &BrowserProviderRouteDecision,
) -> PlaywrightMcpProviderExecutionResult {
    let call_result = {
        let manager = mcp_manager.read().await;
        manager
            .call_tool(&call.server_id, &call.tool_name, call.arguments.clone())
            .await
    };

    match call_result {
        Ok(result) if !result.is_error => playwright_mcp_provider_result_from_adapter_call(
            request_id,
            call,
            playwright_mcp_adapter_evidence_output(session_id, call, route_decision, Some(result)),
        ),
        Ok(result) => playwright_mcp_provider_failure_from_adapter_call(
            request_id,
            call,
            "mcp_tool_error",
            call_tool_result_text(&result),
            true,
        ),
        Err(error) => playwright_mcp_provider_failure_from_adapter_call(
            request_id,
            call,
            "mcp_transport_error",
            error.to_string(),
            true,
        ),
    }
}

fn playwright_mcp_adapter_evidence_output(
    session_id: &str,
    call: &PlaywrightMcpAdapterToolCall,
    route_decision: &BrowserProviderRouteDecision,
    call_result: Option<CallToolResult>,
) -> serde_json::Value {
    let content = call_result
        .as_ref()
        .map(|result| {
            result
                .content
                .iter()
                .map(content_block_to_json)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content_text = call_result
        .as_ref()
        .map(call_tool_result_text)
        .unwrap_or_default();
    let mut output = json!({
        "providerId": PLAYWRIGHT_MCP_PROVIDER_ID,
        "serverId": call.server_id,
        "toolName": call.tool_name,
        "arguments": call.arguments,
        "content": content,
        "contentText": content_text,
        "routeEvidence": {
            "source": "browser_runtime_adapter",
            "rawToolsExposed": false,
            "routeStatus": route_decision.status,
            "eventIntents": route_decision.event_intents,
            "skippedProviders": route_decision.skipped_providers,
        },
    });
    if call.action_kind == crate::browser::playwright_mcp::PlaywrightMcpActionKind::Navigate {
        output["tabId"] = json!(format!("playwright-mcp:{session_id}"));
    }
    output
}

fn content_block_to_json(block: &ContentBlock) -> serde_json::Value {
    match block {
        ContentBlock::Text { text } => json!({ "type": "text", "text": text }),
        ContentBlock::Image { data, mime_type } => {
            json!({ "type": "image", "data": data, "mimeType": mime_type })
        }
        ContentBlock::Resource { resource } => json!({ "type": "resource", "resource": resource }),
    }
}

fn call_tool_result_text(result: &CallToolResult) -> String {
    result
        .content
        .iter()
        .filter_map(|block| match block {
            ContentBlock::Text { text } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn playwright_mcp_provider_failure_from_adapter_call(
    request_id: String,
    call: &PlaywrightMcpAdapterToolCall,
    code: impl Into<String>,
    message: impl Into<String>,
    retryable: bool,
) -> PlaywrightMcpProviderExecutionResult {
    let code = code.into();
    let message = message.into();
    PlaywrightMcpProviderExecutionResult {
        provider_id: PLAYWRIGHT_MCP_PROVIDER_ID.to_string(),
        request_id,
        action_kind: call.action_kind,
        status: PlaywrightMcpProviderExecutionStatus::Failed,
        summary: format!("Playwright MCP {} failed: {message}", call.tool_name),
        mcp_tool_name: Some(call.tool_name.clone()),
        read_only: call.read_only,
        raw_tools_exposed: false,
        artifact_refs: Vec::<PlaywrightMcpProviderArtifactRef>::new(),
        event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
        output: Some(json!({
            "providerId": PLAYWRIGHT_MCP_PROVIDER_ID,
            "serverId": call.server_id,
            "toolName": call.tool_name,
            "arguments": call.arguments,
        })),
        error: Some(PlaywrightMcpProviderExecutionError {
            code,
            message,
            retryable,
            event_name: BrowserTaskEventName::ProviderDegraded.as_str(),
            artifact_recommended: true,
        }),
    }
}

fn playwright_cli_action_for_browser_action(
    action: BrowserAction,
) -> Result<PlaywrightCliAction, BrowserProviderActionBlocked> {
    match action {
        BrowserAction::Navigate { url, .. } => Ok(PlaywrightCliAction::Navigate { url }),
        BrowserAction::Click { index, .. } => Ok(PlaywrightCliAction::Click {
            target: PlaywrightCliAddress::UclawDomElementId {
                element_id: index.to_string(),
            },
        }),
        BrowserAction::Type { index, text, .. } => Ok(PlaywrightCliAction::Type {
            target: PlaywrightCliAddress::UclawDomElementId {
                element_id: index.to_string(),
            },
            text,
        }),
        BrowserAction::GetState {
            include_screenshot, ..
        } => {
            if include_screenshot {
                Ok(PlaywrightCliAction::Screenshot { full_page: false })
            } else {
                Ok(PlaywrightCliAction::Extract { target: None })
            }
        }
        BrowserAction::Scroll { .. }
        | BrowserAction::SendKeys { .. }
        | BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. }
        | BrowserAction::UploadFile { .. } => Err(BrowserProviderActionBlocked {
            selected_provider_id: Some(PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
            message: "Selected Playwright CLI route does not support this browser action yet."
                .to_string(),
        }),
    }
}

fn playwright_mcp_action_for_browser_action(
    action: BrowserAction,
) -> Result<PlaywrightMcpAction, BrowserProviderActionBlocked> {
    match action {
        BrowserAction::Navigate { url, .. } => Ok(PlaywrightMcpAction::Navigate { url }),
        BrowserAction::Click { index, .. } => Ok(PlaywrightMcpAction::Click {
            locator: index.to_string(),
        }),
        BrowserAction::Type { index, text, .. } => Ok(PlaywrightMcpAction::Type {
            locator: index.to_string(),
            text,
        }),
        BrowserAction::GetState { .. } => {
            Ok(PlaywrightMcpAction::AccessibilitySnapshot { url: None })
        }
        BrowserAction::Scroll { .. }
        | BrowserAction::SendKeys { .. }
        | BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. }
        | BrowserAction::UploadFile { .. } => Err(BrowserProviderActionBlocked {
            selected_provider_id: Some(PLAYWRIGHT_MCP_PROVIDER_ID.to_string()),
            message: "Selected Playwright MCP route does not support this browser action yet."
                .to_string(),
        }),
    }
}

fn browser_action_result_from_playwright_cli(
    result: PlaywrightCliProviderExecutionResult,
    duration_ms: u64,
    input_action: &BrowserAction,
) -> BrowserActionResult {
    let ok = result.status == PlaywrightCliProviderExecutionStatus::Succeeded;
    let error = result.error.as_ref().map(|error| {
        if error.code.is_empty() {
            error.message.clone()
        } else {
            format!("{}: {}", error.code, error.message)
        }
    });
    let tab_id = result
        .output
        .as_ref()
        .and_then(|output| output.get("tabId"))
        .and_then(|value| value.as_str())
        .map(str::to_string);
    BrowserActionResult {
        ok,
        action_name: format!("browser_playwright_cli_{}", result.action_kind.as_str()),
        message: Some(result.summary.clone()),
        tab_id,
        observation_json: Some(serde_json::json!({
            "providerId": result.provider_id,
            "requestId": result.request_id,
            "status": result.status,
            "actionKind": result.action_kind,
            "summary": result.summary,
            "inputAction": input_action,
            "artifactRefs": result.artifact_refs,
            "output": result.output,
            "error": result.error,
            "routeEvidence": {
                "source": "official_playwright_cli",
                "officialCli": true,
                "runtimePackRequired": false,
            },
        })),
        error,
        duration_ms,
    }
}

fn playwright_cli_preview_mirror_action(action: &BrowserAction) -> Option<BrowserAction> {
    match action {
        BrowserAction::Navigate { .. }
        | BrowserAction::Click { .. }
        | BrowserAction::Type { .. } => Some(action.clone()),
        BrowserAction::GetState { .. }
        | BrowserAction::Scroll { .. }
        | BrowserAction::SendKeys { .. }
        | BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. }
        | BrowserAction::UploadFile { .. } => None,
    }
}

fn tab_id_from_browser_action(action: &BrowserAction) -> Option<String> {
    match action {
        BrowserAction::Navigate { tab_id, .. } => tab_id.clone(),
        BrowserAction::Click { tab_id, .. }
        | BrowserAction::Type { tab_id, .. }
        | BrowserAction::Scroll { tab_id, .. }
        | BrowserAction::SendKeys { tab_id, .. }
        | BrowserAction::Evaluate { tab_id, .. }
        | BrowserAction::GetState { tab_id, .. }
        | BrowserAction::SwitchTab { tab_id }
        | BrowserAction::CloseTab { tab_id }
        | BrowserAction::UploadFile { tab_id, .. } => Some(tab_id.clone()),
        BrowserAction::ListTabs => None,
    }
}

fn apply_preview_mirror_success(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    preview_tab_id: String,
) {
    result.tab_id = Some(preview_tab_id.clone());
    annotate_preview_mirror(
        result,
        provider_tab_id,
        json!({
            "status": "mirrored",
            "source": "local_chromium_mirror",
            "previewTabId": preview_tab_id,
        }),
    );
}

fn apply_preview_mirror_failure(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    error: String,
) {
    annotate_preview_mirror(
        result,
        provider_tab_id,
        json!({
            "status": "failed",
            "source": "local_chromium_mirror",
            "error": error,
        }),
    );
}

fn annotate_preview_mirror(
    result: &mut BrowserActionResult,
    provider_tab_id: Option<String>,
    preview: serde_json::Value,
) {
    let Some(observation) = result.observation_json.as_mut() else {
        return;
    };
    observation["providerTabId"] = json!(provider_tab_id.clone());
    observation["previewMirror"] = preview.clone();
    if let Some(output) = observation
        .get_mut("output")
        .and_then(|value| value.as_object_mut())
    {
        output.insert("providerTabId".to_string(), json!(provider_tab_id.clone()));
        output.insert("previewMirror".to_string(), preview.clone());
        if let Some(preview_tab_id) = preview
            .get("previewTabId")
            .and_then(|value| value.as_str())
            .map(str::to_string)
        {
            output.insert("previewTabId".to_string(), json!(preview_tab_id));
        }
    }
}

fn browser_action_result_from_playwright_mcp(
    result: PlaywrightMcpProviderExecutionResult,
    duration_ms: u64,
) -> BrowserActionResult {
    let ok = result.status == PlaywrightMcpProviderExecutionStatus::Succeeded;
    let error = result.error.as_ref().map(|error| {
        if error.code.is_empty() {
            error.message.clone()
        } else {
            format!("{}: {}", error.code, error.message)
        }
    });
    BrowserActionResult {
        ok,
        action_name: format!("browser_playwright_mcp_{}", result.action_kind.as_str()),
        message: Some(result.summary.clone()),
        tab_id: result
            .output
            .as_ref()
            .and_then(|output| output.get("tabId"))
            .and_then(|value| value.as_str())
            .map(str::to_string),
        observation_json: Some(serde_json::json!({
            "providerId": result.provider_id,
            "requestId": result.request_id,
            "status": result.status,
            "actionKind": result.action_kind,
            "summary": result.summary,
            "mcpToolName": result.mcp_tool_name,
            "readOnly": result.read_only,
            "rawToolsExposed": result.raw_tools_exposed,
            "artifactRefs": result.artifact_refs,
            "output": result.output,
            "error": result.error,
            "routeEvidence": result
                .output
                .as_ref()
                .and_then(|output| output.get("routeEvidence"))
                .cloned(),
        })),
        error,
        duration_ms,
    }
}

fn mcp_retry_route_decision(
    original: &BrowserProviderRouteDecision,
    reason: &str,
) -> BrowserProviderRouteDecision {
    let mut event_intents = original.event_intents.clone();
    event_intents.push(crate::browser::provider::BrowserProviderRouteEventIntent {
        event_name: BrowserTaskEventName::ProviderRolledBack,
        provider_id: Some(PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
        reason: reason.to_string(),
    });
    event_intents.push(crate::browser::provider::BrowserProviderRouteEventIntent {
        event_name: BrowserTaskEventName::ProviderSelected,
        provider_id: Some(PLAYWRIGHT_MCP_PROVIDER_ID.to_string()),
        reason: "mcp_retry_after_cli_failure".to_string(),
    });
    BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::RolledBack,
        selected_provider_id: Some(PLAYWRIGHT_MCP_PROVIDER_ID.to_string()),
        candidates: original.candidates.clone(),
        event_intents,
        skipped_providers: original.skipped_providers.clone(),
    }
}

fn merge_cli_fallback_route_evidence(
    result: &mut BrowserActionResult,
    previous_route_decision: &BrowserProviderRouteDecision,
    previous_error: &str,
) {
    let Some(observation) = result.observation_json.as_mut() else {
        return;
    };
    let Some(object) = observation.as_object_mut() else {
        return;
    };
    let evidence = object
        .entry("routeEvidence")
        .or_insert_with(|| json!({}))
        .as_object_mut();
    if let Some(evidence) = evidence {
        evidence.insert(
            "fallbackFromProviderId".to_string(),
            json!(PLAYWRIGHT_CLI_PROVIDER_ID),
        );
        evidence.insert(
            "fallbackReason".to_string(),
            json!("playwright_cli_execution_failed"),
        );
        evidence.insert("previousProviderError".to_string(), json!(previous_error));
        evidence.insert(
            "previousRouteDecision".to_string(),
            json!(previous_route_decision),
        );
    }
    if let Some(output) = object
        .get_mut("output")
        .and_then(|value| value.as_object_mut())
    {
        if let Some(route_evidence) = output
            .get_mut("routeEvidence")
            .and_then(|value| value.as_object_mut())
        {
            route_evidence.insert(
                "fallbackFromProviderId".to_string(),
                json!(PLAYWRIGHT_CLI_PROVIDER_ID),
            );
            route_evidence.insert(
                "fallbackReason".to_string(),
                json!("playwright_cli_execution_failed"),
            );
            route_evidence.insert("previousProviderError".to_string(), json!(previous_error));
        }
    }
}

fn next_provider_action_request_id(session_id: &str) -> String {
    let sequence = NEXT_PROVIDER_ACTION_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    format!("{session_id}-provider-action-{sequence}")
}

pub fn provider_selection_request_for_action(
    action: &BrowserAction,
) -> BrowserProviderSelectionRequest {
    match action {
        BrowserAction::Navigate { .. } => BrowserProviderSelectionRequest {
            action: Some("navigate".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::Click { .. } => BrowserProviderSelectionRequest {
            action: Some("click".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::Type { .. } => BrowserProviderSelectionRequest {
            action: Some("type".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::Scroll { .. } => BrowserProviderSelectionRequest {
            action: Some("scroll".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::SendKeys { .. } => BrowserProviderSelectionRequest {
            action: Some("send_keys".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::GetState {
            include_screenshot, ..
        } => BrowserProviderSelectionRequest {
            action: Some(
                if *include_screenshot {
                    "screenshot"
                } else {
                    "extract"
                }
                .to_string(),
            ),
            observation_mode: Some(
                if *include_screenshot {
                    "screenshot"
                } else {
                    "dom_snapshot"
                }
                .to_string(),
            ),
            requires_mcp_specific_capability: false,
        },
        BrowserAction::UploadFile { .. } => BrowserProviderSelectionRequest {
            action: Some("file_upload".to_string()),
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
        BrowserAction::Evaluate { .. }
        | BrowserAction::ListTabs
        | BrowserAction::SwitchTab { .. }
        | BrowserAction::CloseTab { .. } => BrowserProviderSelectionRequest {
            action: None,
            observation_mode: None,
            requires_mcp_specific_capability: false,
        },
    }
}

pub fn route_live_browser_action_provider(action: &BrowserAction) -> BrowserProviderRouteDecision {
    route_live_browser_action_provider_with_options(
        action,
        &BrowserProviderActionRouteOptions::default(),
    )
}

pub fn route_live_browser_action_provider_with_options(
    action: &BrowserAction,
    options: &BrowserProviderActionRouteOptions,
) -> BrowserProviderRouteDecision {
    let capability_override_reason = options
        .capability_override_reason
        .as_deref()
        .filter(|reason| MCP_CAPABILITY_OVERRIDES.contains(reason))
        .or_else(|| {
            if options.feature_flags.playwright_mcp {
                inferred_mcp_capability_override_reason(action)
            } else {
                None
            }
        });
    if options.active_provider_id.as_deref() == Some(PLAYWRIGHT_CLI_PROVIDER_ID)
        && playwright_cli_action_for_browser_action(action.clone()).is_err()
    {
        return BrowserProviderRouteDecision {
            status: BrowserProviderRouteDecisionStatus::Selected,
            selected_provider_id: Some(PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
            candidates: Vec::new(),
            event_intents: vec![crate::browser::provider::BrowserProviderRouteEventIntent {
                event_name:
                    crate::browser::runtime_contracts::BrowserTaskEventName::ProviderSelected,
                provider_id: Some(PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
                reason: "control_center_active_provider_selected".to_string(),
            }],
            skipped_providers: skipped_providers_from_options(options),
        };
    }

    let mut selection = provider_selection_request_for_action(action);
    if capability_override_reason.is_some() {
        selection.action = None;
        selection.requires_mcp_specific_capability = true;
        selection.observation_mode = Some("accessibility_snapshot".to_string());
    }
    let probe_action = selection
        .action
        .clone()
        .unwrap_or_else(|| "local_action_registry".to_string());
    let mut router = BrowserProviderRouter::new();
    router.upsert_status(local_chromium_status(BrowserProviderReadinessProbe {
        provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
        setup_checks: vec![BrowserSetupCheck::passed(
            "browser_provider_executor",
            "Browser provider executor local action registry",
        )],
        capability_probes: vec![BrowserCapabilityProbe::passed(probe_action, true)],
        active_contexts: 0,
        notes: vec!["live_browser_provider_execution_route".to_string()],
    }));
    if options.feature_flags.playwright_cli {
        router.upsert_status(playwright_cli_provider_status(options.feature_flags, true));
    }
    if options.feature_flags.playwright_mcp {
        router.upsert_status(playwright_mcp_provider_status(options.feature_flags, true));
    }
    for provider_id in &options.disabled_provider_ids {
        router.disable_provider(provider_id);
    }
    if let Some(active_provider_id) = options.active_provider_id.as_deref() {
        if capability_override_reason.is_some() && options.feature_flags.playwright_mcp {
            for provider_id in [LOCAL_CHROMIUM_PROVIDER_ID, PLAYWRIGHT_CLI_PROVIDER_ID] {
                router.disable_provider(provider_id);
            }
        } else if active_provider_id != LOCAL_CHROMIUM_PROVIDER_ID {
            for provider_id in [
                LOCAL_CHROMIUM_PROVIDER_ID,
                PLAYWRIGHT_CLI_PROVIDER_ID,
                crate::browser::playwright_mcp::PLAYWRIGHT_MCP_PROVIDER_ID,
            ] {
                if provider_id != active_provider_id {
                    router.disable_provider(provider_id);
                }
            }
        }
    } else if capability_override_reason.is_some() && options.feature_flags.playwright_mcp {
        for provider_id in [LOCAL_CHROMIUM_PROVIDER_ID, PLAYWRIGHT_CLI_PROVIDER_ID] {
            router.disable_provider(provider_id);
        }
    }
    let mut decision = router.route(selection);
    decision.skipped_providers = skipped_providers_from_options(options);
    if let (Some(reason), Some(selected)) = (
        capability_override_reason,
        decision.selected_provider_id.as_deref(),
    ) {
        if selected == PLAYWRIGHT_MCP_PROVIDER_ID {
            decision.event_intents.push(
                crate::browser::provider::BrowserProviderRouteEventIntent {
                    event_name:
                        crate::browser::runtime_contracts::BrowserTaskEventName::ProviderSelected,
                    provider_id: Some(PLAYWRIGHT_MCP_PROVIDER_ID.to_string()),
                    reason: reason.to_string(),
                },
            );
        }
    }
    decision
}

fn inferred_mcp_capability_override_reason(action: &BrowserAction) -> Option<&'static str> {
    match action {
        BrowserAction::GetState {
            include_screenshot: false,
            ..
        } => Some("accessibility_snapshot_needed"),
        _ => None,
    }
}

fn skipped_providers_from_options(
    options: &BrowserProviderActionRouteOptions,
) -> Vec<BrowserProviderRouteSkippedProvider> {
    options
        .skipped_provider_reasons
        .iter()
        .map(|item| BrowserProviderRouteSkippedProvider {
            provider_id: item.provider_id.clone(),
            reason: item.reason.clone(),
        })
        .collect()
}

pub fn provider_route_blocks_local_action(decision: &BrowserProviderRouteDecision) -> bool {
    decision.status == BrowserProviderRouteDecisionStatus::Blocked
        || decision.selected_provider_id.as_deref() != Some(LOCAL_CHROMIUM_PROVIDER_ID)
}

#[cfg(test)]
#[path = "provider_execution_tests.rs"]
mod tests;
