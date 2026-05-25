use super::*;
use std::fs;

#[test]
fn default_manifest_carries_pinned_runtime_versions_and_release_metadata() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    assert_eq!(manifest.pack_version, "browser-runtime-pack-v1");
    assert_eq!(manifest.node_version, "22.16.0");
    assert_eq!(manifest.playwright_version, "1.53.0");
    assert_eq!(manifest.playwright_mcp_version, "0.0.75");
    assert_eq!(manifest.worker_version, "0.1.0");
    assert_eq!(manifest.chromium_revision, "1178");
    assert_eq!(manifest.minimum_app_version, "0.1.0");
    assert!(manifest.download_url.contains("browser-runtime-pack-v1"));
    assert!(manifest.sha256.starts_with("sha256-"));
    assert_eq!(
        manifest.rollback_pack_version,
        Some("browser-runtime-pack-v0".to_string())
    );
    assert_eq!(
        manifest.release_channel,
        BrowserRuntimePackReleaseChannel::Stable
    );
}

#[test]
fn path_policy_derives_uclaw_managed_runtime_root_and_playwright_browsers_env() {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root("/tmp/uclaw-runtime", &manifest);

    assert_eq!(
        paths.current_pack_dir,
        PathBuf::from("/tmp/uclaw-runtime")
            .join("packs")
            .join("browser-runtime-pack-v1")
    );
    assert_eq!(
        paths.manifest_path,
        paths.current_pack_dir.join("runtime-pack.manifest.json")
    );
    assert_eq!(
        paths.node_binary_path,
        paths.current_pack_dir.join("node").join("bin").join("node")
    );
    assert_eq!(
        paths.playwright_package_dir,
        paths
            .current_pack_dir
            .join("node_modules")
            .join("playwright")
    );
    assert_eq!(
        paths.playwright_mcp_package_dir,
        paths
            .current_pack_dir
            .join("node_modules")
            .join("@playwright")
            .join("mcp")
    );
    assert_eq!(
        paths.worker_script_path,
        paths
            .current_pack_dir
            .join("worker")
            .join("uclaw-playwright-worker.mjs")
    );

    let (env_name, env_value) = paths.playwright_browsers_env();
    assert_eq!(env_name, "PLAYWRIGHT_BROWSERS_PATH");
    assert_eq!(env_value, paths.playwright_browsers_path);
    assert!(paths
        .chromium_binary_path
        .to_string_lossy()
        .contains("chromium-1178"));
}

#[test]
fn doctor_classifies_missing_manifest_as_prepare() {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let probe = BrowserRuntimePackProbe {
        manifest_present: false,
        ..BrowserRuntimePackProbe::ready()
    };

    let outcome = diagnose_runtime_pack(&manifest, &probe);

    assert_eq!(outcome.status, BrowserRuntimePackDoctorStatus::NeedsPrepare);
    assert_eq!(
        outcome.issue,
        Some(BrowserRuntimePackIssue::MissingManifest)
    );
    assert_eq!(outcome.actions, vec![BrowserRuntimePackAction::Prepare]);
    assert!(!outcome.ready);
}

#[test]
fn doctor_classifies_missing_runtime_components_as_prepare_or_repair() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    let missing_node = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            node_present: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(
        missing_node.issue,
        Some(BrowserRuntimePackIssue::MissingNodeRuntime)
    );
    assert!(missing_node
        .actions
        .contains(&BrowserRuntimePackAction::Repair));

    let missing_playwright = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            playwright_package_present: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(
        missing_playwright.issue,
        Some(BrowserRuntimePackIssue::MissingPlaywrightPackage)
    );

    let missing_mcp = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            playwright_mcp_package_present: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert!(missing_mcp.ready);
    assert_eq!(missing_mcp.issue, None);

    let missing_browser = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            browser_binary_present: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(
        missing_browser.issue,
        Some(BrowserRuntimePackIssue::MissingBrowserBinary)
    );
}

#[test]
fn doctor_classifies_corrupt_cache_and_worker_failures_with_rollback_when_available() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    let corrupt = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            cache_corrupt: true,
            previous_pack_available: true,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(corrupt.status, BrowserRuntimePackDoctorStatus::NeedsRepair);
    assert_eq!(corrupt.issue, Some(BrowserRuntimePackIssue::CorruptCache));
    assert_eq!(
        corrupt.actions,
        vec![
            BrowserRuntimePackAction::Repair,
            BrowserRuntimePackAction::Rollback,
            BrowserRuntimePackAction::Cleanup,
        ]
    );
    assert!(corrupt.rollback_available);

    let worker = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            worker_startup_ok: false,
            previous_pack_available: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(
        worker.issue,
        Some(BrowserRuntimePackIssue::WorkerStartupFailure)
    );
    assert!(!worker.actions.contains(&BrowserRuntimePackAction::Rollback));
}

#[test]
fn doctor_defers_offline_download_and_degrades_failed_real_page_probe() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    let offline = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            offline: true,
            browser_binary_present: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(offline.status, BrowserRuntimePackDoctorStatus::Deferred);
    assert_eq!(
        offline.issue,
        Some(BrowserRuntimePackIssue::OfflineDownload)
    );
    assert_eq!(
        offline.actions,
        vec![
            BrowserRuntimePackAction::RetryWhenOnline,
            BrowserRuntimePackAction::Defer,
        ]
    );

    let failed_page = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            real_page_probe_ok: false,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(failed_page.status, BrowserRuntimePackDoctorStatus::Degraded);
    assert_eq!(
        failed_page.issue,
        Some(BrowserRuntimePackIssue::FailedRealPageProbe)
    );
}

#[test]
fn doctor_defers_version_mismatch_during_active_tasks_and_prepares_when_idle() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    let active = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            versions_match: false,
            active_tasks: 1,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(active.status, BrowserRuntimePackDoctorStatus::NeedsUpdate);
    assert_eq!(active.issue, Some(BrowserRuntimePackIssue::VersionMismatch));
    assert_eq!(
        active.actions,
        vec![
            BrowserRuntimePackAction::KeepCurrent,
            BrowserRuntimePackAction::Defer,
        ]
    );

    let idle = diagnose_runtime_pack(
        &manifest,
        &BrowserRuntimePackProbe {
            versions_match: false,
            active_tasks: 0,
            ..BrowserRuntimePackProbe::ready()
        },
    );
    assert_eq!(idle.actions, vec![BrowserRuntimePackAction::Prepare]);
}

#[test]
fn doctor_reports_ready_when_all_runtime_probe_inputs_pass() {
    let manifest = BrowserRuntimePackManifest::v1_default();
    let outcome = diagnose_runtime_pack(&manifest, &BrowserRuntimePackProbe::ready());

    assert_eq!(outcome.status, BrowserRuntimePackDoctorStatus::Ready);
    assert!(outcome.ready);
    assert_eq!(outcome.issue, None);
    assert_eq!(outcome.actions, vec![BrowserRuntimePackAction::KeepCurrent]);
    assert_eq!(outcome.manifest_pack_version, manifest.pack_version);
}

#[test]
fn update_policy_prioritizes_security_and_defers_ordinary_updates() {
    let security = decide_runtime_pack_update(BrowserRuntimePackUpdatePolicy {
        update_kind: BrowserRuntimePackUpdateKind::Security,
        active_tasks: 1,
        app_idle: false,
        rollback_available: true,
        offline: false,
    });
    assert_eq!(security.action, BrowserRuntimePackAction::Prepare);
    assert!(security.prompt_user);
    assert!(security.keep_current_pack);

    let ordinary_active = decide_runtime_pack_update(BrowserRuntimePackUpdatePolicy {
        update_kind: BrowserRuntimePackUpdateKind::Ordinary,
        active_tasks: 1,
        app_idle: false,
        rollback_available: true,
        offline: false,
    });
    assert_eq!(ordinary_active.action, BrowserRuntimePackAction::Defer);
    assert!(!ordinary_active.prompt_user);
    assert!(ordinary_active.keep_current_pack);

    let ordinary_idle = decide_runtime_pack_update(BrowserRuntimePackUpdatePolicy {
        update_kind: BrowserRuntimePackUpdateKind::Ordinary,
        active_tasks: 0,
        app_idle: true,
        rollback_available: true,
        offline: false,
    });
    assert_eq!(ordinary_idle.action, BrowserRuntimePackAction::Prepare);
    assert!(ordinary_idle.keep_current_pack);
}

#[test]
fn update_policy_keeps_current_pack_when_current_or_offline() {
    let current = decide_runtime_pack_update(BrowserRuntimePackUpdatePolicy {
        update_kind: BrowserRuntimePackUpdateKind::None,
        active_tasks: 0,
        app_idle: true,
        rollback_available: false,
        offline: false,
    });
    assert_eq!(current.action, BrowserRuntimePackAction::KeepCurrent);
    assert!(!current.prompt_user);

    let offline = decide_runtime_pack_update(BrowserRuntimePackUpdatePolicy {
        update_kind: BrowserRuntimePackUpdateKind::Security,
        active_tasks: 0,
        app_idle: true,
        rollback_available: true,
        offline: true,
    });
    assert_eq!(offline.action, BrowserRuntimePackAction::RetryWhenOnline);
    assert!(offline.keep_current_pack);
}

fn plan_fixture(
    operation: BrowserRuntimePackOperation,
    network_state: BrowserRuntimePackNetworkState,
) -> (
    BrowserRuntimePackManifest,
    BrowserRuntimePackPaths,
    BrowserRuntimePackDoctorOutcome,
    BrowserRuntimePackOperationRequest,
) {
    let manifest = BrowserRuntimePackManifest {
        archive_size_bytes: 120 * 1024 * 1024,
        ..BrowserRuntimePackManifest::v1_default()
    };
    let paths = BrowserRuntimePackPaths::from_root("/tmp/uclaw-runtime", &manifest);
    let doctor = diagnose_runtime_pack(&manifest, &BrowserRuntimePackProbe::ready());
    let request = BrowserRuntimePackOperationRequest {
        operation,
        trigger: BrowserRuntimePackPlanTrigger::StartupAuto,
        network_state,
        auto_prepare_enabled: true,
        user_confirmed: true,
        active_tasks: 0,
    };

    (manifest, paths, doctor, request)
}

#[test]
fn operation_planner_prepares_runtime_pack_with_verification_promotion_and_env() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );

    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    assert_eq!(plan.status, BrowserRuntimePackPlanStatus::Planned);
    assert!(plan.uses_network);
    assert!(!plan.requires_confirmation);
    assert_eq!(
        plan.env,
        vec![BrowserRuntimePackEnvVar {
            name: "PLAYWRIGHT_BROWSERS_PATH".to_string(),
            value: paths.playwright_browsers_path.clone(),
        }]
    );

    let step_kinds: Vec<_> = plan.steps.iter().map(|step| step.kind).collect();
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::DownloadArchive));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::VerifySha256));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::UnpackStaging));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::InstallPack));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::RunDoctor));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::PromotePack));
    assert!(step_kinds.contains(&BrowserRuntimePackPlanStepKind::RetainRollback));
    assert_eq!(
        plan.event_names,
        vec!["browser.runtime.prepare.planned".to_string()]
    );
}

#[test]
fn operation_planner_requires_confirmation_for_metered_or_large_downloads() {
    let (manifest, paths, doctor, mut request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Metered,
    );
    request.user_confirmed = false;

    let metered = plan_runtime_pack_operation(&manifest, &paths, &doctor, request.clone());
    assert_eq!(
        metered.status,
        BrowserRuntimePackPlanStatus::RequiresConfirmation
    );
    assert!(metered.requires_confirmation);
    assert!(metered
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::RequireUserConfirmation));

    let large_manifest = BrowserRuntimePackManifest {
        archive_size_bytes: 350 * 1024 * 1024,
        ..manifest
    };
    let large = plan_runtime_pack_operation(&large_manifest, &paths, &doctor, request);
    assert_eq!(
        large.status,
        BrowserRuntimePackPlanStatus::RequiresConfirmation
    );
}

#[test]
fn operation_planner_defers_offline_or_captive_downloads_without_network_steps() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Offline,
    );

    let offline = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    assert_eq!(offline.status, BrowserRuntimePackPlanStatus::Deferred);
    assert!(offline.keeps_current_pack);
    assert!(!offline
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::DownloadArchive));
    assert_eq!(
        offline.event_names,
        vec!["browser.runtime.prepare.deferred".to_string()]
    );
}

#[test]
fn operation_planner_respects_disabled_startup_auto_prepare_but_allows_task_time() {
    let (manifest, paths, doctor, mut request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );
    request.auto_prepare_enabled = false;
    request.user_confirmed = true;

    let startup = plan_runtime_pack_operation(&manifest, &paths, &doctor, request.clone());
    assert_eq!(startup.status, BrowserRuntimePackPlanStatus::Deferred);
    assert_eq!(
        startup.event_names,
        vec!["browser.runtime.prepare.deferred".to_string()]
    );

    request.trigger = BrowserRuntimePackPlanTrigger::TaskTime;
    let task_time = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);
    assert_eq!(task_time.status, BrowserRuntimePackPlanStatus::Planned);
    assert!(task_time
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::DownloadArchive));
}

#[test]
fn operation_planner_defers_cleanup_and_rollback_during_active_tasks() {
    let (manifest, paths, doctor, mut cleanup_request) = plan_fixture(
        BrowserRuntimePackOperation::Cleanup,
        BrowserRuntimePackNetworkState::Online,
    );
    cleanup_request.active_tasks = 2;

    let cleanup = plan_runtime_pack_operation(&manifest, &paths, &doctor, cleanup_request);
    assert_eq!(cleanup.status, BrowserRuntimePackPlanStatus::Deferred);
    assert!(cleanup.keeps_current_pack);
    assert!(!cleanup
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::CleanupOldPacks));

    let rollback_request = BrowserRuntimePackOperationRequest {
        operation: BrowserRuntimePackOperation::Rollback,
        trigger: BrowserRuntimePackPlanTrigger::Settings,
        network_state: BrowserRuntimePackNetworkState::Online,
        auto_prepare_enabled: true,
        user_confirmed: true,
        active_tasks: 1,
    };
    let rollback = plan_runtime_pack_operation(&manifest, &paths, &doctor, rollback_request);
    assert_eq!(rollback.status, BrowserRuntimePackPlanStatus::Deferred);
}

#[test]
fn operation_planner_blocks_rollback_when_no_previous_pack_exists() {
    let (manifest, paths, _doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Rollback,
        BrowserRuntimePackNetworkState::Online,
    );
    let doctor = BrowserRuntimePackDoctorOutcome {
        rollback_available: false,
        ..diagnose_runtime_pack(&manifest, &BrowserRuntimePackProbe::ready())
    };

    let rollback = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    assert_eq!(rollback.status, BrowserRuntimePackPlanStatus::Blocked);
    assert_eq!(
        rollback.event_names,
        vec!["browser.runtime.rollback.blocked".to_string()]
    );
    assert!(rollback
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::KeepCurrent));
}

#[test]
fn operation_planner_keep_current_is_ready_noop() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::KeepCurrent,
        BrowserRuntimePackNetworkState::Online,
    );

    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    assert_eq!(plan.status, BrowserRuntimePackPlanStatus::Ready);
    assert!(plan.keeps_current_pack);
    assert!(!plan.destructive);
    assert_eq!(
        plan.event_names,
        vec!["browser.runtime.keep_current.planned".to_string()]
    );
}

#[test]
fn dry_run_executor_reports_planned_prepare_steps_and_artifact() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );
    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    let report = execute_runtime_pack_plan_dry_run(&plan);

    assert_eq!(report.mode, BrowserRuntimePackExecutionMode::DryRun);
    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
    assert!(report.summary.starts_with("Dry-run succeeded:"));
    assert!(report
        .artifact_id
        .contains("browser-runtime-prepare-browser-runtime-pack-v1-succeeded"));
    assert!(report.uses_network);
    assert!(!report.destructive);
    assert_eq!(report.step_reports.len(), plan.steps.len());
    assert!(report.step_reports.iter().any(|step| step.step
        == BrowserRuntimePackPlanStepKind::DownloadArchive
        && step.status == BrowserRuntimePackStepExecutionStatus::WouldRun
        && step.uses_network));
    assert!(report
        .event_names
        .contains(&"browser.runtime.prepare.dry_run_succeeded".to_string()));
}

#[test]
fn dry_run_executor_blocks_confirmation_required_plan_without_steps() {
    let (manifest, paths, doctor, mut request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Metered,
    );
    request.user_confirmed = false;
    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    let report = execute_runtime_pack_plan_dry_run(&plan);

    assert_eq!(
        report.status,
        BrowserRuntimePackExecutionStatus::RequiresConfirmation
    );
    assert!(report.requires_confirmation);
    assert!(report.step_reports.is_empty());
    assert!(report
        .event_names
        .contains(&"browser.runtime.prepare.dry_run_confirmation_required".to_string()));
}

#[test]
fn dry_run_executor_preserves_deferred_and_blocked_policy_boundaries() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Offline,
    );
    let deferred_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);
    let deferred = execute_runtime_pack_plan_dry_run(&deferred_plan);
    assert_eq!(deferred.status, BrowserRuntimePackExecutionStatus::Deferred);
    assert!(deferred.keeps_current_pack);
    assert!(deferred.step_reports.is_empty());

    let rollback_request = BrowserRuntimePackOperationRequest {
        operation: BrowserRuntimePackOperation::Rollback,
        trigger: BrowserRuntimePackPlanTrigger::Settings,
        network_state: BrowserRuntimePackNetworkState::Online,
        auto_prepare_enabled: true,
        user_confirmed: true,
        active_tasks: 0,
    };
    let blocked_doctor = BrowserRuntimePackDoctorOutcome {
        rollback_available: false,
        ..diagnose_runtime_pack(&manifest, &BrowserRuntimePackProbe::ready())
    };
    let blocked_plan =
        plan_runtime_pack_operation(&manifest, &paths, &blocked_doctor, rollback_request);
    let blocked = execute_runtime_pack_plan_dry_run(&blocked_plan);
    assert_eq!(blocked.status, BrowserRuntimePackExecutionStatus::Blocked);
    assert!(blocked.step_reports.is_empty());
    assert!(blocked
        .event_names
        .contains(&"browser.runtime.rollback.dry_run_blocked".to_string()));
}

#[test]
fn dry_run_executor_reports_keep_current_as_noop_success() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::KeepCurrent,
        BrowserRuntimePackNetworkState::Online,
    );
    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);

    let report = execute_runtime_pack_plan_dry_run(&plan);

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::NoOp);
    assert!(report.keeps_current_pack);
    assert_eq!(report.step_reports.len(), plan.steps.len());
    assert!(report
        .event_names
        .contains(&"browser.runtime.keep_current.dry_run_noop".to_string()));
}

#[test]
fn dry_run_executor_surfaces_destructive_cleanup_and_rollback_after_confirmation() {
    let (manifest, paths, doctor, mut cleanup_request) = plan_fixture(
        BrowserRuntimePackOperation::Cleanup,
        BrowserRuntimePackNetworkState::Online,
    );
    cleanup_request.trigger = BrowserRuntimePackPlanTrigger::Settings;
    cleanup_request.user_confirmed = true;
    let cleanup_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, cleanup_request);
    let cleanup = execute_runtime_pack_plan_dry_run(&cleanup_plan);

    assert_eq!(cleanup.status, BrowserRuntimePackExecutionStatus::Succeeded);
    assert!(cleanup.destructive);
    assert!(cleanup.step_reports.iter().any(|step| step.step
        == BrowserRuntimePackPlanStepKind::CleanupOldPacks
        && step.destructive));

    let rollback_request = BrowserRuntimePackOperationRequest {
        operation: BrowserRuntimePackOperation::Rollback,
        trigger: BrowserRuntimePackPlanTrigger::Settings,
        network_state: BrowserRuntimePackNetworkState::Online,
        auto_prepare_enabled: true,
        user_confirmed: true,
        active_tasks: 0,
    };
    let rollback_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, rollback_request);
    let rollback = execute_runtime_pack_plan_dry_run(&rollback_plan);

    assert_eq!(
        rollback.status,
        BrowserRuntimePackExecutionStatus::Succeeded
    );
    assert!(rollback.destructive);
    assert!(rollback.step_reports.iter().any(|step| step.step
        == BrowserRuntimePackPlanStepKind::RestoreRollback
        && step.destructive));
}

#[derive(Default)]
struct RecordingRuntimePackStepRunner {
    calls: Vec<BrowserRuntimePackPlanStepKind>,
    fail_at: Option<BrowserRuntimePackPlanStepKind>,
}

impl BrowserRuntimePackStepRunner for RecordingRuntimePackStepRunner {
    fn run_step(&mut self, step: &BrowserRuntimePackPlanStep) -> BrowserRuntimePackStepRunOutcome {
        self.calls.push(step.kind);
        if self.fail_at == Some(step.kind) {
            BrowserRuntimePackStepRunOutcome::failed(format!(
                "simulated failure at {:?}",
                step.kind
            ))
        } else {
            BrowserRuntimePackStepRunOutcome::completed()
        }
    }
}

#[test]
fn managed_executor_blocks_network_or_destructive_policy_without_runner_calls() {
    let (manifest, paths, doctor, prepare_request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );
    let prepare_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, prepare_request);
    let mut runner = RecordingRuntimePackStepRunner::default();

    let blocked_network = execute_runtime_pack_plan_with_runner(
        &prepare_plan,
        BrowserRuntimePackExecutorPolicy::no_side_effects(),
        &mut runner,
    );

    assert_eq!(
        blocked_network.status,
        BrowserRuntimePackExecutionStatus::Blocked
    );
    assert_eq!(
        blocked_network.mode,
        BrowserRuntimePackExecutionMode::Managed
    );
    assert!(blocked_network.step_reports.is_empty());
    assert!(runner.calls.is_empty());
    assert!(blocked_network
        .event_names
        .contains(&"browser.runtime.prepare.execution_blocked".to_string()));

    let mut cleanup_request = BrowserRuntimePackOperationRequest {
        operation: BrowserRuntimePackOperation::Cleanup,
        trigger: BrowserRuntimePackPlanTrigger::Settings,
        network_state: BrowserRuntimePackNetworkState::Online,
        auto_prepare_enabled: true,
        user_confirmed: true,
        active_tasks: 0,
    };
    cleanup_request.user_confirmed = true;
    let cleanup_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, cleanup_request);
    let blocked_destructive = execute_runtime_pack_plan_with_runner(
        &cleanup_plan,
        BrowserRuntimePackExecutorPolicy::no_side_effects(),
        &mut runner,
    );

    assert_eq!(
        blocked_destructive.status,
        BrowserRuntimePackExecutionStatus::Blocked
    );
    assert!(runner.calls.is_empty());
    assert!(blocked_destructive
        .event_names
        .contains(&"browser.runtime.cleanup.execution_blocked".to_string()));
}

#[test]
fn managed_executor_runs_planned_steps_through_runner() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );
    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);
    let mut runner = RecordingRuntimePackStepRunner::default();

    let report = execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Succeeded);
    assert_eq!(report.mode, BrowserRuntimePackExecutionMode::Managed);
    assert_eq!(runner.calls.len(), plan.steps.len());
    assert_eq!(report.step_reports.len(), plan.steps.len());
    assert!(report
        .step_reports
        .iter()
        .all(|step| step.status == BrowserRuntimePackStepExecutionStatus::Completed));
    assert!(report
        .event_names
        .contains(&"browser.runtime.prepare.execution_succeeded".to_string()));
}

#[test]
fn managed_executor_stops_on_failed_step_with_artifact_event() {
    let (manifest, paths, doctor, request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Online,
    );
    let plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, request);
    let mut runner = RecordingRuntimePackStepRunner {
        fail_at: Some(BrowserRuntimePackPlanStepKind::VerifySha256),
        ..RecordingRuntimePackStepRunner::default()
    };

    let report = execute_runtime_pack_plan_with_runner(
        &plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );

    assert_eq!(report.status, BrowserRuntimePackExecutionStatus::Failed);
    assert_eq!(
        runner.calls,
        vec![
            BrowserRuntimePackPlanStepKind::CheckManifest,
            BrowserRuntimePackPlanStepKind::CheckNetworkPolicy,
            BrowserRuntimePackPlanStepKind::DownloadArchive,
            BrowserRuntimePackPlanStepKind::VerifySha256,
        ]
    );
    assert_eq!(
        report.step_reports.last().map(|step| step.status),
        Some(BrowserRuntimePackStepExecutionStatus::Failed)
    );
    assert!(report
        .step_reports
        .last()
        .and_then(|step| step.error.as_ref())
        .is_some());
    assert!(report
        .artifact_id
        .contains("browser-runtime-prepare-browser-runtime-pack-v1-failed"));
    assert!(report
        .event_names
        .contains(&"browser.runtime.prepare.execution_failed".to_string()));
}

#[test]
fn managed_executor_preserves_confirmation_and_deferred_boundaries() {
    let (manifest, paths, doctor, mut confirmation_request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Metered,
    );
    confirmation_request.user_confirmed = false;
    let confirmation_plan =
        plan_runtime_pack_operation(&manifest, &paths, &doctor, confirmation_request);
    let mut runner = RecordingRuntimePackStepRunner::default();

    let confirmation = execute_runtime_pack_plan_with_runner(
        &confirmation_plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );
    assert_eq!(
        confirmation.status,
        BrowserRuntimePackExecutionStatus::RequiresConfirmation
    );
    assert!(runner.calls.is_empty());
    assert!(confirmation.step_reports.is_empty());

    let (_, _, _, offline_request) = plan_fixture(
        BrowserRuntimePackOperation::Prepare,
        BrowserRuntimePackNetworkState::Offline,
    );
    let offline_plan = plan_runtime_pack_operation(&manifest, &paths, &doctor, offline_request);
    let deferred = execute_runtime_pack_plan_with_runner(
        &offline_plan,
        BrowserRuntimePackExecutorPolicy {
            allow_network: true,
            allow_destructive: false,
        },
        &mut runner,
    );
    assert_eq!(deferred.status, BrowserRuntimePackExecutionStatus::Deferred);
    assert!(runner.calls.is_empty());
}

#[test]
fn manifest_loader_reports_missing_loaded_and_invalid_json() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest_path = temp.path().join("runtime-pack.manifest.json");

    let missing = load_runtime_pack_manifest(&manifest_path);
    assert_eq!(
        missing.status,
        BrowserRuntimePackManifestLoadStatus::Missing
    );
    assert_eq!(missing.manifest, None);

    let manifest = BrowserRuntimePackManifest::v1_default();
    fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("write manifest");
    let loaded = load_runtime_pack_manifest(&manifest_path);
    assert_eq!(loaded.status, BrowserRuntimePackManifestLoadStatus::Loaded);
    assert_eq!(loaded.manifest, Some(manifest));

    fs::write(&manifest_path, "{not-json").expect("write invalid manifest");
    let invalid = load_runtime_pack_manifest(&manifest_path);
    assert_eq!(
        invalid.status,
        BrowserRuntimePackManifestLoadStatus::InvalidJson
    );
    assert!(invalid.error.is_some());
}

#[test]
fn legacy_manifest_without_mcp_version_stays_cli_ready() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);

    let mut legacy_manifest = serde_json::to_value(&manifest).expect("manifest value");
    legacy_manifest
        .as_object_mut()
        .expect("manifest object")
        .remove("playwrightMcpVersion");

    fs::create_dir_all(paths.node_binary_path.parent().expect("node parent")).expect("node dir");
    fs::write(&paths.node_binary_path, "").expect("node binary");
    fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
    fs::create_dir_all(paths.worker_script_path.parent().expect("worker parent"))
        .expect("worker dir");
    fs::write(&paths.worker_script_path, "").expect("worker script");
    fs::create_dir_all(
        paths
            .chromium_binary_path
            .parent()
            .expect("chromium parent"),
    )
    .expect("chromium dir");
    fs::write(&paths.chromium_binary_path, "").expect("chromium binary");
    fs::write(
        &paths.manifest_path,
        serde_json::to_string_pretty(&legacy_manifest).expect("serialize legacy manifest"),
    )
    .expect("manifest");

    let report = probe_runtime_pack_filesystem(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: true,
            real_page_probe_ok: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        },
    );

    assert_eq!(
        report.manifest_load.status,
        BrowserRuntimePackManifestLoadStatus::Loaded
    );
    assert_eq!(report.manifest_load.manifest, Some(manifest.clone()));
    assert!(!report.snapshot.cache_corrupt);
    assert!(report.snapshot.versions_match);
    assert!(!report.snapshot.playwright_mcp_package_present);
    assert!(!report.probe.playwright_mcp_package_present);

    let doctor = diagnose_runtime_pack(&manifest, &report.probe);
    assert!(doctor.ready);
    assert_eq!(doctor.issue, None);
}

#[test]
fn filesystem_probe_maps_present_runtime_pack_to_ready_probe() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);

    fs::create_dir_all(paths.node_binary_path.parent().expect("node parent")).expect("node dir");
    fs::write(&paths.node_binary_path, "").expect("node binary");
    fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
    fs::create_dir_all(&paths.playwright_mcp_package_dir).expect("playwright mcp package");
    fs::create_dir_all(paths.worker_script_path.parent().expect("worker parent"))
        .expect("worker dir");
    fs::write(&paths.worker_script_path, "").expect("worker script");
    fs::create_dir_all(
        paths
            .chromium_binary_path
            .parent()
            .expect("chromium parent"),
    )
    .expect("chromium dir");
    fs::write(&paths.chromium_binary_path, "").expect("chromium binary");
    fs::write(
        &paths.manifest_path,
        serde_json::to_string_pretty(&manifest).expect("serialize manifest"),
    )
    .expect("manifest");
    fs::create_dir_all(paths.packs_dir.join("browser-runtime-pack-v0")).expect("rollback pack");

    let default_report = probe_runtime_pack_filesystem(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
    );

    assert_eq!(
        default_report.manifest_load.status,
        BrowserRuntimePackManifestLoadStatus::Loaded
    );
    assert!(default_report.snapshot.manifest_present);
    assert!(default_report.snapshot.node_present);
    assert!(default_report.snapshot.playwright_package_present);
    assert!(default_report.snapshot.playwright_mcp_package_present);
    assert!(default_report.snapshot.worker_script_present);
    assert!(default_report.snapshot.browser_binary_present);
    assert!(default_report.snapshot.previous_pack_available);
    assert!(default_report.snapshot.versions_match);
    assert!(!default_report.probe.worker_startup_ok);
    assert!(!default_report.probe.real_page_probe_ok);

    let report = probe_runtime_pack_filesystem(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions {
            worker_startup_ok: true,
            real_page_probe_ok: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        },
    );
    assert_eq!(report.probe, BrowserRuntimePackProbe::ready());
}

#[test]
fn filesystem_probe_flags_version_mismatch_invalid_manifest_and_missing_worker() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);

    fs::create_dir_all(paths.node_binary_path.parent().expect("node parent")).expect("node dir");
    fs::write(&paths.node_binary_path, "").expect("node binary");
    fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
    fs::create_dir_all(&paths.playwright_mcp_package_dir).expect("playwright mcp package");
    fs::create_dir_all(
        paths
            .chromium_binary_path
            .parent()
            .expect("chromium parent"),
    )
    .expect("chromium dir");
    fs::write(&paths.chromium_binary_path, "").expect("chromium binary");

    let installed = BrowserRuntimePackManifest {
        playwright_version: "1.52.0".to_string(),
        ..manifest.clone()
    };
    fs::write(
        &paths.manifest_path,
        serde_json::to_string_pretty(&installed).expect("serialize manifest"),
    )
    .expect("manifest");
    let mismatch = probe_runtime_pack_filesystem(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions {
            active_tasks: 2,
            offline: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        },
    );
    assert!(mismatch.snapshot.manifest_present);
    assert!(!mismatch.snapshot.versions_match);
    assert!(!mismatch.probe.versions_match);
    assert!(!mismatch.probe.worker_startup_ok);
    assert_eq!(mismatch.probe.active_tasks, 2);
    assert!(mismatch.probe.offline);

    fs::write(&paths.manifest_path, "{not-json").expect("invalid manifest");
    let invalid = probe_runtime_pack_filesystem(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
    );
    assert_eq!(
        invalid.snapshot.manifest_status,
        BrowserRuntimePackManifestLoadStatus::InvalidJson
    );
    assert!(invalid.snapshot.cache_corrupt);
    assert!(invalid.probe.cache_corrupt);
    assert!(!invalid.probe.versions_match);
}

fn write_ready_runtime_pack(
    paths: &BrowserRuntimePackPaths,
    manifest: &BrowserRuntimePackManifest,
) {
    fs::create_dir_all(paths.node_binary_path.parent().expect("node parent")).expect("node dir");
    fs::write(&paths.node_binary_path, "").expect("node binary");
    fs::create_dir_all(&paths.playwright_package_dir).expect("playwright package");
    fs::create_dir_all(&paths.playwright_mcp_package_dir).expect("playwright mcp package");
    fs::create_dir_all(paths.worker_script_path.parent().expect("worker parent"))
        .expect("worker dir");
    fs::write(&paths.worker_script_path, "").expect("worker script");
    fs::create_dir_all(
        paths
            .chromium_binary_path
            .parent()
            .expect("chromium parent"),
    )
    .expect("chromium dir");
    fs::write(&paths.chromium_binary_path, "").expect("chromium binary");
    fs::write(
        &paths.manifest_path,
        serde_json::to_string_pretty(manifest).expect("serialize manifest"),
    )
    .expect("manifest");
    fs::create_dir_all(paths.packs_dir.join("browser-runtime-pack-v0")).expect("rollback pack");
}

fn verified_probe_options() -> BrowserRuntimePackFilesystemProbeOptions {
    BrowserRuntimePackFilesystemProbeOptions {
        worker_startup_ok: true,
        real_page_probe_ok: true,
        ..BrowserRuntimePackFilesystemProbeOptions::default()
    }
}

fn status_request(
    trigger: BrowserRuntimePackPlanTrigger,
    network_state: BrowserRuntimePackNetworkState,
    user_confirmed: bool,
) -> BrowserRuntimePackStatusRequest {
    BrowserRuntimePackStatusRequest {
        trigger,
        network_state,
        auto_prepare_enabled: true,
        user_confirmed,
    }
}

#[test]
fn status_report_composes_ready_filesystem_doctor_and_keep_current_plan() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);
    write_ready_runtime_pack(&paths, &manifest);

    let report = inspect_runtime_pack_status(
        &manifest,
        &paths,
        verified_probe_options(),
        status_request(
            BrowserRuntimePackPlanTrigger::StartupAuto,
            BrowserRuntimePackNetworkState::Online,
            true,
        ),
    );

    assert!(report.ready);
    assert!(report.can_run_browser_tasks);
    assert_eq!(report.primary_action, BrowserRuntimePackAction::KeepCurrent);
    assert_eq!(report.doctor.status, BrowserRuntimePackDoctorStatus::Ready);
    assert_eq!(
        report.operation_plan.status,
        BrowserRuntimePackPlanStatus::Ready
    );
    assert!(report
        .event_names
        .contains(&"browser.runtime.manifest.checked".to_string()));
    assert!(report
        .event_names
        .contains(&"browser.runtime.keep_current.planned".to_string()));
}

#[test]
fn status_report_requires_worker_and_page_probe_before_ready() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);
    write_ready_runtime_pack(&paths, &manifest);

    let report = inspect_runtime_pack_status(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
        status_request(
            BrowserRuntimePackPlanTrigger::StartupAuto,
            BrowserRuntimePackNetworkState::Online,
            true,
        ),
    );

    assert!(!report.ready);
    assert!(!report.can_run_browser_tasks);
    assert_eq!(
        report.doctor.issue,
        Some(BrowserRuntimePackIssue::WorkerStartupFailure)
    );
    assert_eq!(report.primary_action, BrowserRuntimePackAction::Repair);
}

#[test]
fn status_report_defers_missing_pack_when_offline() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest::v1_default();
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);

    let report = inspect_runtime_pack_status(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions {
            offline: true,
            ..BrowserRuntimePackFilesystemProbeOptions::default()
        },
        status_request(
            BrowserRuntimePackPlanTrigger::StartupAuto,
            BrowserRuntimePackNetworkState::Offline,
            false,
        ),
    );

    assert!(!report.ready);
    assert!(!report.can_run_browser_tasks);
    assert_eq!(
        report.primary_action,
        BrowserRuntimePackAction::RetryWhenOnline
    );
    assert_eq!(
        report.doctor.status,
        BrowserRuntimePackDoctorStatus::Deferred
    );
    assert_eq!(
        report.operation_plan.status,
        BrowserRuntimePackPlanStatus::Deferred
    );
    assert!(!report
        .operation_plan
        .steps
        .iter()
        .any(|step| step.kind == BrowserRuntimePackPlanStepKind::DownloadArchive));
}

#[test]
fn status_report_surfaces_confirmation_required_for_metered_prepare() {
    let temp = tempfile::tempdir().expect("tempdir");
    let manifest = BrowserRuntimePackManifest {
        archive_size_bytes: 250 * 1024 * 1024,
        ..BrowserRuntimePackManifest::v1_default()
    };
    let paths = BrowserRuntimePackPaths::from_root(temp.path(), &manifest);

    let report = inspect_runtime_pack_status(
        &manifest,
        &paths,
        BrowserRuntimePackFilesystemProbeOptions::default(),
        status_request(
            BrowserRuntimePackPlanTrigger::TaskTime,
            BrowserRuntimePackNetworkState::Metered,
            false,
        ),
    );

    assert!(!report.ready);
    assert_eq!(report.primary_action, BrowserRuntimePackAction::Prepare);
    assert_eq!(
        report.operation_plan.status,
        BrowserRuntimePackPlanStatus::RequiresConfirmation
    );
    assert!(report.operation_plan.requires_confirmation);
    assert!(report
        .event_names
        .contains(&"browser.runtime.prepare.confirmation_required".to_string()));
}
