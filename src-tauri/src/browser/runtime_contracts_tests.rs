use super::*;

#[test]
fn safe_default_flags_keep_risky_lanes_off() {
    let flags = BrowserRuntimeFeatureFlags::safe_defaults();

    assert!(!flags.playwright_cli);
    assert!(!flags.playwright_mcp);
    assert!(!flags.hosted_providers);
    assert!(flags.runtime_auto_prepare);
    assert!(!flags.developer_upstream_fallback);
    assert!(!flags.external_real_profile_attach);
}

#[test]
fn state_transitions_allow_normal_recovery_and_stop_paths() {
    use BrowserRuntimeState::*;

    for (from, to) in [
        (Stopped, Starting),
        (Starting, Ready),
        (Ready, Acting),
        (Acting, Idle),
        (Idle, Acting),
        (Acting, Recovering),
        (Recovering, Ready),
        (Recovering, Degraded),
        (Degraded, Recovering),
        (Degraded, Stopped),
        (Ready, Stopped),
        (Idle, Stopped),
    ] {
        assert!(
            is_allowed_browser_runtime_transition(from, to),
            "{from:?} -> {to:?} should be allowed"
        );
    }

    for (from, to) in [
        (Stopped, Acting),
        (Ready, Starting),
        (Acting, Starting),
        (Degraded, Acting),
    ] {
        assert!(
            !is_allowed_browser_runtime_transition(from, to),
            "{from:?} -> {to:?} should be blocked"
        );
    }
}

#[test]
fn provider_cards_cover_all_phase0_lanes_with_safe_defaults() {
    let cards = browser_provider_capability_cards();
    let ids: Vec<&str> = cards.iter().map(|card| card.provider_id).collect();

    assert_eq!(
        ids,
        vec![
            "browser.local_chromium",
            "browser.playwright_cli",
            "browser.playwright_mcp",
            "browser.raw_cdp",
            "browser.hosted"
        ]
    );

    let local = browser_provider_capability_card("browser.local_chromium").unwrap();
    assert_eq!(local.lane, BrowserProviderLane::LocalChromium);
    assert!(local.enabled_by_default);
    assert!(local.supported_actions.contains(&"navigate"));
    assert!(local.supported_actions.contains(&"dom_snapshot"));
    assert_eq!(
        local.harness_score.source,
        "current_browser_agent_v2_regressions"
    );
    assert!(local.harness_score.tracked_metrics.contains(&"local_first"));
    assert!(local.harness_score.promotion_eligible);
    assert!(local.disable_path.contains("feature flag"));

    let cli = browser_provider_capability_card("browser.playwright_cli").unwrap();
    assert_eq!(cli.lane, BrowserProviderLane::PlaywrightCli);
    assert_eq!(cli.feature_flag, Some("playwright_cli"));
    assert!(!cli.enabled_by_default);
    assert!(cli.requires_runtime_pack);
    assert!(!cli.allows_raw_script_by_default);
    assert!(cli.supported_actions.contains(&"extract"));
    assert_eq!(cli.harness_score.source, "phase5_cli_fixture_gates");
    assert_eq!(
        cli.harness_score.fixture_cases_passed,
        cli.harness_score.fixture_cases_total
    );
    assert!(!cli.harness_score.promotion_eligible);

    let mcp = browser_provider_capability_card("browser.playwright_mcp").unwrap();
    assert_eq!(mcp.lane, BrowserProviderLane::PlaywrightMcp);
    assert_eq!(mcp.feature_flag, Some("playwright_mcp"));
    assert!(!mcp.enabled_by_default);
    assert_eq!(mcp.harness_score.source, "phase7_mcp_fixture_gates");
    assert!(mcp
        .harness_score
        .tracked_metrics
        .contains(&"artifact_completeness"));
    assert!(!mcp.harness_score.promotion_eligible);

    let hosted = browser_provider_capability_card("browser.hosted").unwrap();
    assert_eq!(hosted.lane, BrowserProviderLane::Hosted);
    assert_eq!(hosted.feature_flag, Some("hosted_providers"));
    assert!(!hosted.enabled_by_default);
    assert_eq!(hosted.harness_score.fixture_cases_total, 0);
    assert_eq!(
        hosted.harness_score.source,
        "not_harnessed_disabled_baseline"
    );
}

#[test]
fn provider_cards_require_explicit_harness_scorecards() {
    for card in browser_provider_capability_cards() {
        assert!(
            !card.harness_subjects.is_empty(),
            "{} must declare harness subjects",
            card.provider_id
        );
        assert!(
            !card.harness_score.source.is_empty(),
            "{} must declare scorecard source",
            card.provider_id
        );
        assert!(
            card.harness_score.fixture_cases_passed <= card.harness_score.fixture_cases_total,
            "{} passed cases cannot exceed total cases",
            card.provider_id
        );
        assert!(
            !card.harness_score.tracked_metrics.is_empty(),
            "{} must declare tracked metrics",
            card.provider_id
        );
    }

    let hosted = browser_provider_capability_card("browser.hosted").unwrap();
    assert_eq!(hosted.harness_score.fixture_cases_total, 0);
    assert!(!hosted.harness_score.promotion_eligible);
}

#[test]
fn provider_selection_keeps_mcp_behind_cli_for_generic_actions() {
    let request = BrowserProviderSelectionRequest {
        action: Some("click".into()),
        observation_mode: None,
        requires_mcp_specific_capability: false,
    };

    let candidates =
        rank_browser_provider_candidates(&request, browser_provider_capability_cards());
    let ids: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.provider_id)
        .collect();

    assert_eq!(
        ids,
        vec![
            "browser.local_chromium",
            "browser.playwright_cli",
            "browser.playwright_mcp",
            "browser.hosted",
        ]
    );
    assert_eq!(candidates[1].lane, BrowserProviderLane::PlaywrightCli);
    assert_eq!(candidates[1].rank, 10);
    assert_eq!(candidates[2].lane, BrowserProviderLane::PlaywrightMcp);
    assert_eq!(candidates[2].rank, 20);
    assert_eq!(candidates[2].reason, "mcp_feature_lane_after_cli");
}

#[test]
fn provider_selection_allows_mcp_to_outrank_cli_for_mcp_specific_needs() {
    let request = BrowserProviderSelectionRequest {
        action: Some("click".into()),
        observation_mode: Some("accessibility_snapshot".into()),
        requires_mcp_specific_capability: true,
    };

    let candidates =
        rank_browser_provider_candidates(&request, browser_provider_capability_cards());
    let ids: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.provider_id)
        .collect();

    assert_eq!(
        ids,
        vec!["browser.playwright_mcp", "browser.playwright_cli"]
    );
    assert_eq!(candidates[0].rank, 5);
    assert_eq!(candidates[0].reason, "mcp_specific_capability_required");
    assert_eq!(candidates[1].rank, 10);
}

#[test]
fn provider_selection_excludes_ineligible_observation_modes() {
    let request = BrowserProviderSelectionRequest {
        action: Some("navigate".into()),
        observation_mode: Some("network_console".into()),
        requires_mcp_specific_capability: true,
    };

    let candidates =
        rank_browser_provider_candidates(&request, browser_provider_capability_cards());

    assert_eq!(candidates.len(), 1);
    assert_eq!(candidates[0].provider_id, "browser.playwright_mcp");
    assert_eq!(candidates[0].lane, BrowserProviderLane::PlaywrightMcp);
}

#[test]
fn provider_selection_keeps_raw_and_hosted_after_local_lanes() {
    let request = BrowserProviderSelectionRequest {
        action: Some("screenshot".into()),
        observation_mode: None,
        requires_mcp_specific_capability: false,
    };

    let candidates =
        rank_browser_provider_candidates(&request, browser_provider_capability_cards());
    let ids: Vec<&str> = candidates
        .iter()
        .map(|candidate| candidate.provider_id)
        .collect();

    assert_eq!(
        ids,
        vec![
            "browser.local_chromium",
            "browser.playwright_cli",
            "browser.playwright_mcp",
            "browser.raw_cdp",
            "browser.hosted",
        ]
    );
    assert_eq!(candidates[3].rank, 80);
    assert_eq!(candidates[4].rank, 90);
}

#[test]
fn browser_event_names_cover_startup_runtime_provider_identity_and_boundaries() {
    let event_names = browser_task_event_names();
    let names: Vec<&str> = event_names.iter().map(|name| name.as_str()).collect();

    for expected in [
        "browser.startup_doctor.check",
        "browser.runtime.state_changed",
        "browser.provider.selected",
        "browser.provider.degraded",
        "browser.identity.authorized",
        "browser.identity.revoked",
        "browser.task.paused_waiting_for_runtime",
        "browser.task.paused_checkpointed",
        "browser.runtime.artifact_pack_created",
    ] {
        assert!(
            names.contains(&expected),
            "missing browser event name {expected}"
        );
    }
}

#[test]
fn projection_attention_reasons_are_derived_from_visible_browser_state() {
    let summary = BrowserWorldProjectionSummary {
        startup_doctor: BrowserStartupDoctorProjection {
            status: StartupDoctorStatus::Failed,
            last_check_at: Some("2026-05-23T12:00:00Z".into()),
            current_check: Some("runtime_manifest".into()),
            failure_code: Some("manifest_missing".into()),
            detail_visible: true,
        },
        runtime: BrowserRuntimeProjection {
            state: BrowserRuntimeState::Degraded,
            provider_id: Some("browser.playwright_cli".into()),
            active_session_id: Some("session-1".into()),
            active_task_id: Some("task-1".into()),
            degraded_reason: Some("worker_timeout".into()),
            last_artifact_pack_ref: Some("artifact-pack-1".into()),
        },
        identity: BrowserIdentityProjection {
            mode: BrowserIdentityMode::UclawManaged,
            authorized: false,
            last_used_at: Some("2026-05-23T11:58:00Z".into()),
            active_task_ids: vec!["task-1".into()],
            revoked: true,
        },
        task_boundary: BrowserTaskBoundaryProjection {
            task_id: Some("task-1".into()),
            status: BrowserTaskBoundaryStatus::PausedCheckpointed,
            reason: Some("identity_revoked".into()),
            checkpoint_ref: Some("checkpoint-1".into()),
        },
    };

    assert_eq!(
        summary.attention_reasons(),
        vec![
            "startup_doctor_failed",
            "runtime_degraded",
            "identity_revoked",
            "task_paused_checkpointed",
        ]
    );
}
