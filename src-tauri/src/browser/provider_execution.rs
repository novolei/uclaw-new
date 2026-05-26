use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::action_registry::BrowserActionRegistry;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::playwright_cli::{
    execute_playwright_cli_provider_action, playwright_cli_provider_status, PlaywrightCliAction,
    PlaywrightCliAddress, PlaywrightCliProviderExecutionResult,
    PlaywrightCliProviderExecutionStatus, PLAYWRIGHT_CLI_PROVIDER_ID,
};
use crate::browser::playwright_mcp::playwright_mcp_provider_status;
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
}

impl Default for BrowserProviderActionRouteOptions {
    fn default() -> Self {
        Self {
            feature_flags: BrowserRuntimeFeatureFlags::safe_defaults(),
            runtime_report: None,
            disabled_provider_ids: Vec::new(),
            active_provider_id: None,
            skipped_provider_reasons: Vec::new(),
        }
    }
}

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
        let runtime_report = match self.route_options.runtime_report.as_ref() {
            Some(runtime_report) => runtime_report,
            None => {
                return BrowserProviderActionExecution {
                    route_decision: route_decision.clone(),
                    outcome: BrowserProviderActionExecutionOutcome::Blocked(
                        BrowserProviderActionBlocked {
                            selected_provider_id: route_decision.selected_provider_id.clone(),
                            message: "Playwright CLI provider was selected, but no runtime-pack readiness report is available.".to_string(),
                        },
                    ),
                };
            }
        };

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
        let result = execute_playwright_cli_provider_action(
            next_provider_action_request_id(session_id),
            self.route_options.feature_flags,
            cli_action,
            runtime_report,
        )
        .await;
        let duration_ms = started.elapsed().as_millis() as u64;

        match result.status {
            PlaywrightCliProviderExecutionStatus::Blocked => BrowserProviderActionExecution {
                route_decision: route_decision.clone(),
                outcome: BrowserProviderActionExecutionOutcome::Blocked(
                    BrowserProviderActionBlocked {
                        selected_provider_id: route_decision.selected_provider_id.clone(),
                        message: playwright_cli_blocked_message(&result),
                    },
                ),
            },
            PlaywrightCliProviderExecutionStatus::Succeeded
            | PlaywrightCliProviderExecutionStatus::Failed => BrowserProviderActionExecution {
                route_decision,
                outcome: BrowserProviderActionExecutionOutcome::Executed(
                    browser_action_result_from_playwright_cli(result, duration_ms),
                ),
            },
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
    let mut action_result = BrowserActionResult {
        ok,
        action_name: format!("browser_playwright_cli_{}", result.action_kind.as_str()),
        message: Some(result.summary.clone()),
        tab_id: None,
        observation_json: Some(serde_json::json!({
            "providerId": result.provider_id,
            "requestId": result.request_id,
            "status": result.status,
            "actionKind": result.action_kind,
            "summary": result.summary,
            "artifactRefs": result.artifact_refs,
            "output": result.output,
            "error": result.error,
        })),
        error,
        duration_ms,
    };
    if !ok && action_result.error.is_none() {
        action_result.error = Some("Playwright CLI provider action failed".to_string());
    }
    action_result
}

fn playwright_cli_blocked_message(result: &PlaywrightCliProviderExecutionResult) -> String {
    result
        .error
        .as_ref()
        .map(|error| format!("{}: {}", error.code, error.message))
        .unwrap_or_else(|| result.summary.clone())
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
            action: Some("dom_snapshot".to_string()),
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

    let selection = provider_selection_request_for_action(action);
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
    }
    let mut decision = router.route(selection);
    decision.skipped_providers = skipped_providers_from_options(options);
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
