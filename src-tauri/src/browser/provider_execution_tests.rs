use super::*;
use std::path::Path;
use std::sync::Arc;

use crate::browser::context_manager::BrowserContextManager;
use crate::browser::provider::BrowserProviderRouteDecisionStatus;
use crate::browser::runtime_contracts::BrowserRuntimeFeatureFlags;
use crate::browser::runtime_pack::{
    diagnose_runtime_pack, plan_runtime_pack_operation, BrowserRuntimePackAction,
    BrowserRuntimePackFilesystemProbeReport, BrowserRuntimePackFilesystemSnapshot,
    BrowserRuntimePackManifest, BrowserRuntimePackManifestLoadOutcome,
    BrowserRuntimePackManifestLoadStatus, BrowserRuntimePackNetworkState,
    BrowserRuntimePackOperation, BrowserRuntimePackOperationRequest, BrowserRuntimePackPaths,
    BrowserRuntimePackPlanTrigger, BrowserRuntimePackProbe, BrowserRuntimePackStatusReport,
};

#[test]
fn live_provider_route_selects_local_chromium_for_click() {
    let action = BrowserAction::Click {
        tab_id: "tab-1".to_string(),
        index: 3,
    };

    let decision = route_live_browser_action_provider(&action);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
    assert!(decision.event_intents.iter().any(|intent| {
        intent.event_name.as_str() == "browser.provider.selected"
            && intent.provider_id.as_deref() == Some(LOCAL_CHROMIUM_PROVIDER_ID)
    }));
    assert!(!provider_route_blocks_local_action(&decision));
}

#[test]
fn provider_selection_maps_get_state_to_snapshot_request() {
    let action = BrowserAction::GetState {
        tab_id: "tab-1".to_string(),
        include_screenshot: true,
        include_visual: true,
    };

    let selection = provider_selection_request_for_action(&action);

    assert_eq!(selection.action.as_deref(), Some("dom_snapshot"));
    assert_eq!(selection.observation_mode.as_deref(), Some("screenshot"));
    assert!(!selection.requires_mcp_specific_capability);
}

#[test]
fn evaluate_route_preserves_local_registry_without_raw_provider_promotion() {
    let action = BrowserAction::Evaluate {
        tab_id: "tab-1".to_string(),
        script: "document.title".to_string(),
    };

    let selection = provider_selection_request_for_action(&action);
    let decision = route_live_browser_action_provider(&action);

    assert!(selection.action.is_none());
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
}

#[test]
fn non_local_provider_route_blocks_local_action_registry() {
    let decision = BrowserProviderRouteDecision {
        status: BrowserProviderRouteDecisionStatus::Selected,
        selected_provider_id: Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID.to_string()),
        candidates: Vec::new(),
        event_intents: Vec::new(),
    };

    assert!(provider_route_blocks_local_action(&decision));
}

#[test]
fn cli_candidate_can_be_selected_when_enabled_ready_and_local_disabled() {
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider_with_options(&action, &options);

    assert_eq!(
        decision.status,
        BrowserProviderRouteDecisionStatus::Selected
    );
    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    assert!(decision.candidates.iter().any(|candidate| {
        candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID && candidate.eligible
    }));
}

#[tokio::test]
async fn selected_cli_route_blocks_before_local_action_registry_execution() {
    let ctx_mgr = Arc::new(BrowserContextManager::new_for_test(
        "/tmp/uclaw-browser-provider-execution-test".into(),
    ));
    let mut flags = BrowserRuntimeFeatureFlags::safe_defaults();
    flags.playwright_cli = true;
    let options = BrowserProviderActionRouteOptions::default()
        .with_feature_flags(flags)
        .with_runtime_report(ready_runtime_report())
        .with_disabled_provider(LOCAL_CHROMIUM_PROVIDER_ID);
    let executor = BrowserProviderActionExecutor::new(ctx_mgr).with_route_options(options);
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };
    let route_decision = executor.route_action(&action);

    let execution = executor
        .execute_routed_with_identity("session-1", None, action, route_decision)
        .await
        .expect("blocked route should not call local registry");

    assert_eq!(
        execution.route_decision.selected_provider_id.as_deref(),
        Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
    );
    match execution.outcome {
        BrowserProviderActionExecutionOutcome::Blocked(blocked) => {
            assert_eq!(
                blocked.selected_provider_id.as_deref(),
                Some(crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID)
            );
            assert!(blocked.message.contains("local Chromium action registry"));
        }
        BrowserProviderActionExecutionOutcome::Executed(_) => {
            panic!("CLI route must not fall through to local Chromium execution");
        }
    }
}

#[test]
fn safe_default_route_options_do_not_make_playwright_candidates_eligible() {
    let action = BrowserAction::Navigate {
        tab_id: Some("tab-1".to_string()),
        url: "https://example.test".to_string(),
    };

    let decision = route_live_browser_action_provider(&action);

    assert_eq!(
        decision.selected_provider_id.as_deref(),
        Some(LOCAL_CHROMIUM_PROVIDER_ID)
    );
    assert!(decision.candidates.iter().any(|candidate| {
        candidate.provider_id == crate::browser::PLAYWRIGHT_CLI_PROVIDER_ID
            && !candidate.eligible
            && candidate.blocked_reason.as_deref() == Some("provider_status_missing")
    }));
}

fn ready_runtime_report() -> BrowserRuntimePackStatusReport {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths =
        BrowserRuntimePackPaths::from_root(Path::new("/tmp/uclaw-browser-runtime"), &manifest);
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
                previous_pack_dir: None,
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
