//! Browser Runtime action execution seam.
//!
//! This module keeps task-time callers from knowing the ordering of runtime
//! status inspection, provider routing, rollout signal emission, and provider
//! execution. Lower-level provider executors remain adapters behind this
//! Browser Runtime interface.

use std::sync::Arc;

use anyhow::Result;

use crate::browser::action::BrowserAction;
use crate::browser::context_manager::BrowserContextManager;
use crate::browser::provider_execution::{
    BrowserProviderActionBlocked, BrowserProviderActionExecution,
    BrowserProviderActionExecutionOutcome, BrowserProviderActionExecutor,
    BrowserProviderActionRouteOptions,
};
use crate::browser::rollout_bridge::emit_browser_provider_route_into_session_dir;
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::browser::runtime_status::{BrowserRuntimeStatusReport, BrowserRuntimeStatusService};

pub type BrowserRuntimeActionBlocked = BrowserProviderActionBlocked;
pub type BrowserRuntimeActionExecutionOutcome = BrowserProviderActionExecutionOutcome;

pub struct BrowserRuntimeActionRequest {
    pub session_id: String,
    pub identity_profile_id: Option<String>,
    pub task_id: String,
    pub action: BrowserAction,
}

pub struct BrowserRuntimeActionExecutor {
    ctx_mgr: Arc<BrowserContextManager>,
    runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    feature_flags: BrowserRuntimeFeatureFlags,
    disabled_provider_ids: Vec<String>,
}

impl BrowserRuntimeActionExecutor {
    pub fn new(
        ctx_mgr: Arc<BrowserContextManager>,
        runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    ) -> Self {
        Self {
            ctx_mgr,
            runtime_status_service,
            feature_flags: BrowserRuntimeFeatureFlags::safe_defaults(),
            disabled_provider_ids: Vec::new(),
        }
    }

    pub fn with_feature_flags(mut self, feature_flags: BrowserRuntimeFeatureFlags) -> Self {
        self.feature_flags = feature_flags;
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

    pub async fn execute_action(
        &self,
        request: BrowserRuntimeActionRequest,
    ) -> Result<BrowserProviderActionExecution> {
        let route_options = self.current_route_options().await;
        let provider_executor = BrowserProviderActionExecutor::new(Arc::clone(&self.ctx_mgr))
            .with_route_options(route_options);
        let route_decision = provider_executor.route_action(&request.action);
        emit_browser_provider_route_into_session_dir(&route_decision, &request.task_id, None).await;

        provider_executor
            .execute_routed_with_identity(
                &request.session_id,
                request.identity_profile_id.as_deref(),
                request.action,
                route_decision,
            )
            .await
    }

    async fn current_route_options(&self) -> BrowserProviderActionRouteOptions {
        let mut route_options = match self.runtime_status_service.as_ref() {
            Some(runtime_status_service) => match runtime_status_service.inspect_default().await {
                Ok(status) => route_options_from_runtime_status(status),
                Err(error) => {
                    tracing::warn!(
                        error = %error,
                        "Browser Runtime status unavailable for task-time provider routing; using default provider route options"
                    );
                    BrowserProviderActionRouteOptions::default()
                }
            },
            None => BrowserProviderActionRouteOptions::default(),
        }
        .with_feature_flags(self.feature_flags);

        for provider_id in &self.disabled_provider_ids {
            route_options = route_options.with_disabled_provider(provider_id.clone());
        }

        route_options
    }
}

fn route_options_from_runtime_status(
    status: BrowserRuntimeStatusReport,
) -> BrowserProviderActionRouteOptions {
    BrowserProviderActionRouteOptions::default().with_runtime_report(status.runtime_pack)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::browser::provider::{
        BrowserProviderRouteDecisionStatus, LOCAL_CHROMIUM_PROVIDER_ID,
    };
    use crate::browser::runtime_pack::{
        inspect_runtime_pack_status, BrowserRuntimePackFilesystemProbeOptions,
        BrowserRuntimePackManifest, BrowserRuntimePackNetworkState, BrowserRuntimePackPaths,
        BrowserRuntimePackPlanTrigger, BrowserRuntimePackStatusRequest,
    };

    #[test]
    fn route_options_uses_runtime_pack_from_aggregate_status() {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths =
            BrowserRuntimePackPaths::from_root(temp_dir.path().join("browser-runtime"), &manifest);
        let runtime_pack = inspect_runtime_pack_status(
            &manifest,
            &paths,
            BrowserRuntimePackFilesystemProbeOptions::default(),
            BrowserRuntimePackStatusRequest {
                trigger: BrowserRuntimePackPlanTrigger::TaskTime,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
            },
        );
        let expected_current_pack_dir = runtime_pack.current_pack_dir.clone();
        let status = crate::browser::runtime_status::compose_browser_runtime_status(
            runtime_pack,
            Vec::new(),
        );

        let options = route_options_from_runtime_status(status);

        let route_runtime_pack = options
            .runtime_report
            .as_ref()
            .expect("runtime status should populate provider route runtime report");
        assert_eq!(
            route_runtime_pack.current_pack_dir,
            expected_current_pack_dir
        );
    }

    #[tokio::test]
    async fn runtime_action_executor_blocks_when_route_has_no_executable_provider() {
        let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
            "/tmp/uclaw-browser-runtime-action-executor-test".into(),
        ));
        let executor = BrowserRuntimeActionExecutor::new(ctx_mgr, None)
            .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);

        let execution = executor
            .execute_action(BrowserRuntimeActionRequest {
                session_id: "session-1".to_string(),
                identity_profile_id: None,
                task_id: "task-1".to_string(),
                action: BrowserAction::Navigate {
                    tab_id: Some("tab-1".to_string()),
                    url: "https://example.test".to_string(),
                },
            })
            .await
            .expect("blocked provider route should not call local browser");

        assert_eq!(
            execution.route_decision.status,
            BrowserProviderRouteDecisionStatus::Blocked
        );
        match execution.outcome {
            BrowserRuntimeActionExecutionOutcome::Blocked(blocked) => {
                assert_eq!(blocked.selected_provider_id, None);
                assert!(blocked.message.contains("local Chromium action registry"));
            }
            BrowserRuntimeActionExecutionOutcome::Executed(_) => {
                panic!("blocked provider route must not execute a browser action");
            }
        }
    }
}
