use super::*;

#[test]
fn default_manifest_carries_pinned_runtime_versions_and_release_metadata() {
    let manifest = BrowserRuntimePackManifest::v1_default();

    assert_eq!(manifest.pack_version, "browser-runtime-pack-v1");
    assert_eq!(manifest.node_version, "22.16.0");
    assert_eq!(manifest.playwright_version, "1.53.0");
    assert_eq!(manifest.worker_version, "0.1.0");
    assert_eq!(manifest.chromium_revision, "1181");
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
        .contains("chromium-1181"));
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
