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
    PlaywrightMcpAction, PlaywrightMcpProviderExecutionResult,
    PlaywrightMcpProviderExecutionStatus, PLAYWRIGHT_MCP_PROVIDER_ID,
};
use crate::browser::playwright_mcp_adapter::PlaywrightMcpAdapterToolCall;
use crate::browser::provider::{
    local_chromium_status, BrowserCapabilityProbe, BrowserProviderReadinessProbe,
    BrowserProviderRouteDecision, BrowserProviderRouteDecisionStatus,
    BrowserProviderRouteSkippedProvider, BrowserProviderRouter, BrowserSetupCheck,
    LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{
    BrowserProviderSelectionRequest, BrowserRuntimeFeatureFlags,
};
use crate::browser::runtime_pack::BrowserRuntimePackStatusReport;

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

#[derive(Debug, Clone)]
pub struct BrowserProviderActionRouteOptions {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub runtime_report: Option<BrowserRuntimePackStatusReport>,
    pub disabled_provider_ids: Vec<String>,
    pub active_provider_id: Option<String>,
    pub skipped_provider_reasons: Vec<BrowserProviderSkippedReason>,
    pub capability_override_reason: Option<String>,
    pub playwright_cli_command: Option<std::path::PathBuf>,
    pub playwright_cli_cwd: Option<std::path::PathBuf>,
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
            return Ok(self
                .execute_playwright_cli_route(session_id, action, route_decision)
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
        action: BrowserAction,
        route_decision: BrowserProviderRouteDecision,
    ) -> BrowserProviderActionExecution {
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
        BrowserProviderActionExecution {
            route_decision,
            outcome: BrowserProviderActionExecutionOutcome::Executed(
                browser_action_result_from_playwright_cli(provider_result, duration_ms),
            ),
        }
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
        let provider_result = playwright_mcp_provider_result_from_adapter_call(
            request_id,
            &call,
            json!({
                "providerId": PLAYWRIGHT_MCP_PROVIDER_ID,
                "serverId": call.server_id,
                "toolName": call.tool_name,
                "arguments": call.arguments,
                "routeEvidence": {
                    "source": "browser_runtime_adapter",
                    "rawToolsExposed": false,
                },
            }),
        );
        let duration_ms = started.elapsed().as_millis() as u64;

        BrowserProviderActionExecution {
            route_decision,
            outcome: BrowserProviderActionExecutionOutcome::Executed(
                browser_action_result_from_playwright_mcp(provider_result, duration_ms),
            ),
        }
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
        tab_id: None,
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
        })),
        error,
        duration_ms,
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
        .filter(|reason| MCP_CAPABILITY_OVERRIDES.contains(reason));
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
        if active_provider_id != LOCAL_CHROMIUM_PROVIDER_ID {
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
