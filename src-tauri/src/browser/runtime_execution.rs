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
use crate::mcp::SharedMcpManager;

pub type BrowserRuntimeActionBlocked = BrowserProviderActionBlocked;
pub type BrowserRuntimeActionExecutionOutcome = BrowserProviderActionExecutionOutcome;

pub struct BrowserRuntimeActionRequest {
    pub session_id: String,
    pub identity_profile_id: Option<String>,
    pub task_id: String,
    pub action: BrowserAction,
}

/// Identifiers needed by the Evaluate-gate to build an `ApprovalOrigin`.
#[derive(Debug, Clone)]
pub struct EvaluateApprovalContext {
    pub conversation_id: String,
    pub browser_task_id: String,
}

pub struct BrowserRuntimeActionExecutor {
    ctx_mgr: Arc<BrowserContextManager>,
    runtime_status_service: Option<Arc<BrowserRuntimeStatusService>>,
    provider_config: BrowserRuntimeProviderConfig,
    feature_flags: Option<BrowserRuntimeFeatureFlags>,
    disabled_provider_ids: Vec<String>,
    mcp_manager: Option<SharedMcpManager>,
    /// Slice 1b follow-up — shared SafetyManager from AppState. Used by the
    /// Evaluate-gate. `None` when the executor is constructed in test/legacy
    /// contexts without a SafetyManager wired; in that case Evaluate runs
    /// without the gate (preserves pre-Task-B behavior).
    pub(crate) safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    /// Slice 1b follow-up — approval handler for `RequireApproval`. For browser
    /// sub-loops this is a `ChatApprovalHandler` (user in chat).
    pub(crate) approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
    /// Slice 1b follow-up — opaque conversation + browser_task identifiers,
    /// used to construct the `ApprovalOrigin::BrowserSubLoop { .. }` passed
    /// to `handle_ask`. Both must be set together; `None` means no gate.
    pub(crate) approval_context: Option<EvaluateApprovalContext>,
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
            mcp_manager: None,
            // Slice 1b follow-up — None by default; set via with_* builders.
            safety_manager: None,
            approval_handler: None,
            approval_context: None,
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

    pub fn with_mcp_manager(mut self, mcp_manager: SharedMcpManager) -> Self {
        self.mcp_manager = Some(mcp_manager);
        self
    }

    pub fn with_safety_manager(
        mut self,
        safety_manager: Option<Arc<tokio::sync::RwLock<crate::safety::SafetyManager>>>,
    ) -> Self {
        self.safety_manager = safety_manager;
        self
    }

    pub fn with_approval_handler(
        mut self,
        approval_handler: Option<Arc<dyn crate::safety::ApprovalHandler>>,
    ) -> Self {
        self.approval_handler = approval_handler;
        self
    }

    pub fn with_approval_context(
        mut self,
        approval_context: Option<EvaluateApprovalContext>,
    ) -> Self {
        self.approval_context = approval_context;
        self
    }

    pub async fn execute_action(
        &self,
        request: BrowserRuntimeActionRequest,
    ) -> Result<BrowserProviderActionExecution> {
        // Slice 1b follow-up — gate Evaluate (arbitrary JS) through the chokepoint.
        // Currently prompts per Evaluate; per-task `always_allow` caching is a
        // future enhancement.
        if let BrowserAction::Evaluate { ref script, .. } = request.action {
            if let (Some(safety), Some(handler), Some(approval_ctx)) = (
                self.safety_manager.as_ref(),
                self.approval_handler.as_ref(),
                self.approval_context.as_ref(),
            ) {
                let decision = {
                    let safety_read = safety.read().await;
                    safety_read.should_approve(
                        "browser_evaluate",
                        &serde_json::json!({"script": script.clone()}),
                        &crate::agent::tools::tool::ApprovalRequirement::Always,
                        None,
                    )
                };
                match decision {
                    crate::safety::ApprovalDecision::AutoApprove => {
                        tracing::debug!(
                            script_head = %script.chars().take(80).collect::<String>(),
                            "[Slice 1b] browser_evaluate auto-approved by SafetyMode"
                        );
                        // fall through to execute
                    }
                    crate::safety::ApprovalDecision::Block { reason } => {
                        tracing::warn!(
                            reason = %reason,
                            "[Slice 1b] browser_evaluate blocked by SafetyManager policy"
                        );
                        return Ok(BrowserProviderActionExecution {
                            route_decision: crate::browser::provider::BrowserProviderRouteDecision {
                                status: crate::browser::provider::BrowserProviderRouteDecisionStatus::Blocked,
                                selected_provider_id: None,
                                candidates: vec![],
                                event_intents: vec![],
                                skipped_providers: vec![],
                            },
                            outcome: BrowserProviderActionExecutionOutcome::Blocked(
                                BrowserProviderActionBlocked {
                                    selected_provider_id: None,
                                    message: format!("Evaluate blocked: {reason}"),
                                },
                            ),
                        });
                    }
                    crate::safety::ApprovalDecision::RequireApproval { .. } => {
                        let origin = crate::safety::ApprovalOrigin::BrowserSubLoop {
                            conversation_id: approval_ctx.conversation_id.clone(),
                            browser_task_id: approval_ctx.browser_task_id.clone(),
                        };
                        let outcome = handler
                            .handle_ask(
                                "browser_evaluate",
                                &serde_json::json!({"script": script.clone()}),
                                &origin,
                            )
                            .await;
                        match outcome {
                            crate::safety::ApprovalOutcome::Approved => {
                                tracing::debug!(
                                    script_head = %script.chars().take(80).collect::<String>(),
                                    "[Slice 1b] browser_evaluate approved by user"
                                );
                                // fall through to execute
                            }
                            crate::safety::ApprovalOutcome::Denied => {
                                tracing::info!("[Slice 1b] browser_evaluate denied by user");
                                return Ok(BrowserProviderActionExecution {
                                    route_decision: crate::browser::provider::BrowserProviderRouteDecision {
                                        status: crate::browser::provider::BrowserProviderRouteDecisionStatus::Blocked,
                                        selected_provider_id: None,
                                        candidates: vec![],
                                        event_intents: vec![],
                                        skipped_providers: vec![],
                                    },
                                    outcome: BrowserProviderActionExecutionOutcome::Blocked(
                                        BrowserProviderActionBlocked {
                                            selected_provider_id: None,
                                            message: "Evaluate denied by user".to_string(),
                                        },
                                    ),
                                });
                            }
                            crate::safety::ApprovalOutcome::Escalated => {
                                // browser sub-loop has no async-resume; treat Escalated
                                // as denial-with-explanation for now.
                                tracing::info!("[Slice 1b] browser_evaluate escalated; treating as denied (no async-resume in sub-loop)");
                                return Ok(BrowserProviderActionExecution {
                                    route_decision: crate::browser::provider::BrowserProviderRouteDecision {
                                        status: crate::browser::provider::BrowserProviderRouteDecisionStatus::Blocked,
                                        selected_provider_id: None,
                                        candidates: vec![],
                                        event_intents: vec![],
                                        skipped_providers: vec![],
                                    },
                                    outcome: BrowserProviderActionExecutionOutcome::Blocked(
                                        BrowserProviderActionBlocked {
                                            selected_provider_id: None,
                                            message: "Evaluate awaiting approval (escalated)".to_string(),
                                        },
                                    ),
                                });
                            }
                        }
                    }
                }
            }
        }

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

        if let Some(mcp_manager) = self.mcp_manager.as_ref() {
            route_options = route_options.with_mcp_manager(mcp_manager.clone());
        }

        route_options
    }
}

fn route_options_from_runtime_status(
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
        assert!(options.feature_flags.playwright_mcp);
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

    /// Locks the contract: BrowserAction::Evaluate routes through the
    /// ApprovalHandler when safety fields are wired and SafetyMode is Ask.
    /// An ObservingHandler verifies via AtomicBool that handle_ask is called
    /// with tool_name="browser_evaluate", arguments.script is set, and origin
    /// is ApprovalOrigin::BrowserSubLoop{..}.
    /// Denied verdict → outcome is Blocked (gate returns without executing).
    #[tokio::test]
    async fn execute_action_evaluate_routes_through_approval_handler() {
        use std::sync::atomic::{AtomicBool, Ordering};
        use crate::safety::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome, SafetyMode};

        struct ObservingHandler {
            called: Arc<AtomicBool>,
            verdict: ApprovalOutcome,
        }

        #[async_trait::async_trait]
        impl ApprovalHandler for ObservingHandler {
            async fn handle_ask(
                &self,
                tool_name: &str,
                arguments: &serde_json::Value,
                origin: &ApprovalOrigin,
            ) -> ApprovalOutcome {
                self.called.store(true, Ordering::SeqCst);
                assert_eq!(tool_name, "browser_evaluate");
                assert!(
                    arguments.get("script").is_some(),
                    "script must be passed in arguments"
                );
                assert!(
                    matches!(origin, ApprovalOrigin::BrowserSubLoop { .. }),
                    "origin must be BrowserSubLoop, got: {origin:?}"
                );
                self.verdict.clone()
            }
        }

        let called = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(ObservingHandler {
            called: called.clone(),
            verdict: ApprovalOutcome::Denied,
        });

        let mut mgr = crate::safety::SafetyManager::new(&std::env::temp_dir());
        mgr.set_global_mode(SafetyMode::Ask).unwrap();
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));

        let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
            "/tmp/uclaw-evaluate-gate-test".into(),
        ));
        let executor = BrowserRuntimeActionExecutor::new(ctx_mgr, None)
            .with_safety_manager(Some(safety_manager))
            .with_approval_handler(Some(handler))
            .with_approval_context(Some(EvaluateApprovalContext {
                conversation_id: "c1".into(),
                browser_task_id: "bt-1".into(),
            }));

        let execution = executor
            .execute_action(BrowserRuntimeActionRequest {
                session_id: "session-1".to_string(),
                identity_profile_id: None,
                task_id: "task-1".to_string(),
                action: BrowserAction::Evaluate {
                    tab_id: "tab-1".to_string(),
                    script: "document.title".to_string(),
                },
            })
            .await
            .expect("gate must not bubble an error");

        // ApprovalHandler.handle_ask must have been called.
        assert!(
            called.load(Ordering::SeqCst),
            "ApprovalHandler.handle_ask must be invoked for Evaluate in Ask mode"
        );
        // Denied verdict → outcome is Blocked (gate returns without executing).
        match execution.outcome {
            BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
                assert!(
                    blocked.message.contains("denied"),
                    "blocked message should mention 'denied', got: {}",
                    blocked.message
                );
            }
            BrowserProviderActionExecutionOutcome::Executed(_) => {
                panic!("Denied verdict must produce Blocked outcome, not Executed");
            }
        }
    }

    /// Gate is skipped when safety fields are absent (preserves pre-Task-B behavior).
    /// Evaluate falls through to the normal provider path (Blocked by disabled provider).
    #[tokio::test]
    async fn execute_action_evaluate_skips_gate_when_no_safety_manager() {
        let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
            "/tmp/uclaw-evaluate-gate-noop-test".into(),
        ));
        // No safety fields wired — gate is a no-op.
        let executor = BrowserRuntimeActionExecutor::new(ctx_mgr, None)
            .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);

        let execution = executor
            .execute_action(BrowserRuntimeActionRequest {
                session_id: "session-1".to_string(),
                identity_profile_id: None,
                task_id: "task-1".to_string(),
                action: BrowserAction::Evaluate {
                    tab_id: "tab-1".to_string(),
                    script: "document.title".to_string(),
                },
            })
            .await
            .expect("fallthrough to provider layer must not error");

        // Without safety gate the action routes through provider layer (Blocked by
        // disabled-provider, not by the gate). Message differs from gate denial.
        match execution.outcome {
            BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
                assert!(
                    !blocked.message.contains("denied"),
                    "block must come from provider layer (not gate), got: {}",
                    blocked.message
                );
            }
            BrowserProviderActionExecutionOutcome::Executed(_) => {
                // Also acceptable — provider ran without a real browser in CI.
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
            true,
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
