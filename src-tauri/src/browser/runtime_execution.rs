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
    BrowserProviderActionRouteOptions, BrowserProviderSkippedReason,
};
use crate::browser::rollout_bridge::emit_browser_provider_route_into_session_dir;
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;
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
    provider_config: BrowserRuntimeProviderConfig,
    feature_flags: Option<BrowserRuntimeFeatureFlags>,
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
            provider_config: BrowserRuntimeProviderConfig::default(),
            feature_flags: None,
            disabled_provider_ids: Vec::new(),
        }
    }

    pub fn with_provider_config(mut self, provider_config: BrowserRuntimeProviderConfig) -> Self {
        self.provider_config = provider_config;
        self
    }

    pub fn with_feature_flags(mut self, feature_flags: BrowserRuntimeFeatureFlags) -> Self {
        self.feature_flags = Some(feature_flags);
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
            Some(runtime_status_service) => {
                match runtime_status_service
                    .inspect_with_provider_config(self.provider_config.clone())
                    .await
                {
                    Ok(status) => route_options_from_runtime_status(status),
                    Err(error) => {
                        tracing::warn!(
                            error = %error,
                            "Browser Runtime status unavailable for task-time provider routing; using default provider route options"
                        );
                        BrowserProviderActionRouteOptions::default()
                    }
                }
            }
            None => BrowserProviderActionRouteOptions::default(),
        };

        if let Some(feature_flags) = self.feature_flags {
            route_options = route_options.with_feature_flags(feature_flags);
        }

        for provider_id in &self.disabled_provider_ids {
            route_options = route_options.with_disabled_provider(provider_id.clone());
        }

        route_options
    }
}

pub(crate) fn route_options_from_runtime_status(
    status: BrowserRuntimeStatusReport,
) -> BrowserProviderActionRouteOptions {
    let skipped = status
        .control_center
        .provider_lanes
        .iter()
        .filter_map(|lane| {
            lane.fallback_reason
                .as_ref()
                .map(|reason| BrowserProviderSkippedReason {
                    provider_id: lane.provider_id.clone(),
                    reason: reason.clone(),
                })
        })
        .collect();
    let active_provider_id = status
        .control_center
        .active_provider_route
        .provider_id
        .clone();

    BrowserProviderActionRouteOptions::default()
        .with_feature_flags(status.control_center.feature_flags)
        .with_runtime_report(status.runtime_pack)
        .with_active_control_center_route(active_provider_id, skipped)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::browser::provider::{
        BrowserProviderRouteDecisionStatus, LOCAL_CHROMIUM_PROVIDER_ID,
    };
    use crate::browser::runtime_control_center::BrowserRuntimeProviderConfig;
    use crate::browser::runtime_pack::{
        diagnose_runtime_pack, inspect_runtime_pack_status, plan_runtime_pack_operation,
        BrowserRuntimePackAction, BrowserRuntimePackFilesystemProbeOptions,
        BrowserRuntimePackFilesystemProbeReport, BrowserRuntimePackFilesystemSnapshot,
        BrowserRuntimePackManifest, BrowserRuntimePackManifestLoadOutcome,
        BrowserRuntimePackManifestLoadStatus, BrowserRuntimePackNetworkState,
        BrowserRuntimePackOperation, BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths,
        BrowserRuntimePackPlanTrigger, BrowserRuntimePackProbe, BrowserRuntimePackStatusReport,
        BrowserRuntimePackStatusRequest,
    };
    use crate::browser::runtime_provider_probe::BrowserRuntimeProviderProbeSummary;
    use crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID;

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

    #[test]
    fn route_options_include_control_center_active_route() {
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        config.provider_probe_cache.insert(
            PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
            BrowserRuntimeProviderProbeSummary::passed(PLAYWRIGHT_CLI_PROVIDER_ID, 1),
        );
        let status = status_with_provider_config(config);

        let options = route_options_from_runtime_status(status);

        assert_eq!(
            options.active_provider_id.as_deref(),
            Some(PLAYWRIGHT_CLI_PROVIDER_ID)
        );
        assert!(!options
            .disabled_provider_ids
            .iter()
            .any(|id| id == PLAYWRIGHT_CLI_PROVIDER_ID));
    }

    #[test]
    fn failed_cli_probe_keeps_local_chromium_active_for_execution() {
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        config.provider_probe_cache.insert(
            PLAYWRIGHT_CLI_PROVIDER_ID.to_string(),
            BrowserRuntimeProviderProbeSummary::failed(
                PLAYWRIGHT_CLI_PROVIDER_ID,
                1,
                "worker_startup_timeout",
                "Worker startup timed out after 15s.",
            ),
        );
        let status = status_with_provider_config(config);

        let options = route_options_from_runtime_status(status);

        assert_eq!(
            options.active_provider_id.as_deref(),
            Some(LOCAL_CHROMIUM_PROVIDER_ID)
        );
        assert!(options.skipped_provider_reasons.iter().any(|item| {
            item.provider_id == PLAYWRIGHT_CLI_PROVIDER_ID && item.reason == "probe_failed"
        }));
    }

    #[test]
    fn route_options_include_control_center_feature_flags() {
        let mut config = BrowserRuntimeProviderConfig::default();
        config.playwright_cli_enabled = true;
        let status = status_with_provider_config(config);

        let options = route_options_from_runtime_status(status);

        assert!(options.feature_flags.playwright_cli);
        assert!(!options.feature_flags.playwright_mcp);
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

    fn status_with_provider_config(
        config: BrowserRuntimeProviderConfig,
    ) -> BrowserRuntimeStatusReport {
        let temp_dir = tempfile::tempdir().expect("temp dir");
        let runtime_pack = ready_runtime_pack_status(temp_dir.path());

        crate::browser::runtime_status::compose_browser_runtime_status_with_config(
            runtime_pack,
            Vec::new(),
            config,
        )
    }

    fn ready_runtime_pack_status(root: &std::path::Path) -> BrowserRuntimePackStatusReport {
        let manifest = BrowserRuntimePackManifest::v1_default();
        let paths = BrowserRuntimePackPaths::from_root(root.join("browser-runtime"), &manifest);
        let probe = BrowserRuntimePackProbe::ready();
        let doctor = diagnose_runtime_pack(&manifest, &probe);
        let primary_action = doctor
            .actions
            .first()
            .copied()
            .unwrap_or(BrowserRuntimePackAction::KeepCurrent);
        let operation_plan = plan_runtime_pack_operation(
            &manifest,
            &paths,
            &doctor,
            BrowserRuntimePackOperationRequest {
                operation: BrowserRuntimePackOperation::from_action(primary_action),
                trigger: BrowserRuntimePackPlanTrigger::TaskTime,
                network_state: BrowserRuntimePackNetworkState::Online,
                auto_prepare_enabled: true,
                user_confirmed: false,
                active_tasks: probe.active_tasks,
            },
        );

        BrowserRuntimePackStatusReport {
            manifest_pack_version: manifest.pack_version.clone(),
            runtime_root: paths.runtime_root.clone(),
            current_pack_dir: paths.current_pack_dir.clone(),
            filesystem: BrowserRuntimePackFilesystemProbeReport {
                snapshot: BrowserRuntimePackFilesystemSnapshot {
                    current_pack_dir: paths.current_pack_dir,
                    previous_pack_dir: Some(paths.packs_dir.join("browser-runtime-pack-v0")),
                    manifest_path: paths.manifest_path.clone(),
                    manifest_status: BrowserRuntimePackManifestLoadStatus::Loaded,
                    manifest_present: probe.manifest_present,
                    node_present: probe.node_present,
                    playwright_package_present: probe.playwright_package_present,
                    playwright_mcp_package_present: probe.playwright_mcp_package_present,
                    worker_script_present: true,
                    browser_binary_present: probe.browser_binary_present,
                    previous_pack_available: probe.previous_pack_available,
                    versions_match: probe.versions_match,
                    cache_corrupt: probe.cache_corrupt,
                    active_tasks: probe.active_tasks,
                    offline: probe.offline,
                },
                probe,
                manifest_load: BrowserRuntimePackManifestLoadOutcome {
                    status: BrowserRuntimePackManifestLoadStatus::Loaded,
                    path: paths.manifest_path,
                    manifest: Some(manifest),
                    error: None,
                },
            },
            doctor,
            primary_action,
            operation_plan,
            ready: true,
            can_run_browser_tasks: true,
            event_names: vec!["browser.runtime.status.reported".to_string()],
        }
    }
}
