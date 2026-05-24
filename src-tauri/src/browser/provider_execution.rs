use std::sync::Arc;

use anyhow::Result;

use crate::browser::action::{BrowserAction, BrowserActionResult};
use crate::browser::action_registry::BrowserActionRegistry;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::playwright_cli::playwright_cli_provider_status;
use crate::browser::playwright_mcp::playwright_mcp_provider_status;
use crate::browser::provider::{
    local_chromium_status, BrowserCapabilityProbe, BrowserProviderReadinessProbe,
    BrowserProviderRouteDecision, BrowserProviderRouteDecisionStatus, BrowserProviderRouter,
    BrowserSetupCheck, LOCAL_CHROMIUM_PROVIDER_ID,
};
use crate::browser::runtime_contracts::{
    BrowserProviderSelectionRequest, BrowserRuntimeFeatureFlags,
};
use crate::browser::runtime_pack::BrowserRuntimePackStatusReport;

pub struct BrowserProviderActionExecutor {
    action_registry: BrowserActionRegistry,
    route_options: BrowserProviderActionRouteOptions,
}

#[derive(Debug, Clone)]
pub struct BrowserProviderActionRouteOptions {
    pub feature_flags: BrowserRuntimeFeatureFlags,
    pub runtime_report: Option<BrowserRuntimePackStatusReport>,
    pub disabled_provider_ids: Vec<String>,
}

impl Default for BrowserProviderActionRouteOptions {
    fn default() -> Self {
        Self {
            feature_flags: BrowserRuntimeFeatureFlags::safe_defaults(),
            runtime_report: None,
            disabled_provider_ids: Vec::new(),
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
        router.upsert_status(playwright_cli_provider_status(
            options.feature_flags,
            options.runtime_report.as_ref(),
        ));
    }
    if options.feature_flags.playwright_mcp {
        let runtime_ready = options
            .runtime_report
            .as_ref()
            .is_some_and(|report| report.ready && report.can_run_browser_tasks);
        router.upsert_status(playwright_mcp_provider_status(
            options.feature_flags,
            runtime_ready,
        ));
    }
    for provider_id in &options.disabled_provider_ids {
        router.disable_provider(provider_id);
    }
    router.route(selection)
}

pub fn provider_route_blocks_local_action(decision: &BrowserProviderRouteDecision) -> bool {
    decision.status == BrowserProviderRouteDecisionStatus::Blocked
        || decision.selected_provider_id.as_deref() != Some(LOCAL_CHROMIUM_PROVIDER_ID)
}

#[cfg(test)]
#[path = "provider_execution_tests.rs"]
mod tests;
