use super::*;

fn passing_probe() -> BrowserProviderReadinessProbe {
    BrowserProviderReadinessProbe {
        provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
        setup_checks: vec![
            BrowserSetupCheck::passed("context_manager", "BrowserContextManager registered"),
            BrowserSetupCheck::passed("profile_base", "Profile base directory configured"),
        ],
        capability_probes: vec![
            BrowserCapabilityProbe::passed("navigate", true),
            BrowserCapabilityProbe::passed("dom_snapshot", true),
            BrowserCapabilityProbe::passed("screenshot", true),
        ],
        active_contexts: 2,
        notes: vec!["warm contexts observed".to_string()],
    }
}

#[test]
fn local_chromium_capabilities_preserve_browser_agent_v2_strengths() {
    let capabilities = local_chromium_capabilities();

    assert_eq!(capabilities.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
    assert!(capabilities.actions.contains(&"navigate".to_string()));
    assert!(capabilities.actions.contains(&"dom_snapshot".to_string()));
    assert!(capabilities.actions.contains(&"screenshot".to_string()));
    assert!(capabilities
        .actions
        .contains(&"checkpoint_resume".to_string()));
    assert!(capabilities.features.contains(&"auth_profiles".to_string()));
    assert!(capabilities
        .features
        .contains(&"user_intervention".to_string()));
    assert!(capabilities.features.contains(&"task_store".to_string()));
}

#[test]
fn passing_setup_and_required_action_probes_are_ready() {
    let status = local_chromium_status(passing_probe());

    assert_eq!(status.provider_id, LOCAL_CHROMIUM_PROVIDER_ID);
    assert_eq!(status.readiness, BrowserProviderReadiness::Ready);
    assert!(status.ready);
    assert!(status.setup_complete);
    assert_eq!(status.active_contexts, 2);
    assert!(status.remediation.is_empty());
}

#[test]
fn failed_setup_check_needs_setup_with_remediation() {
    let mut probe = passing_probe();
    probe.setup_checks[1] = BrowserSetupCheck::failed(
        "profile_base",
        "Profile base directory configured",
        "Create or repair the uClaw browser profile directory.",
    );

    let status = local_chromium_status(probe);

    assert_eq!(status.readiness, BrowserProviderReadiness::NeedsSetup);
    assert!(!status.ready);
    assert!(!status.setup_complete);
    assert!(status
        .remediation
        .contains(&"Create or repair the uClaw browser profile directory.".to_string()));
}

#[test]
fn unsupported_required_setup_check_is_unavailable() {
    let mut probe = passing_probe();
    probe.setup_checks[0] = BrowserSetupCheck::unsupported(
        "context_manager",
        "BrowserContextManager registered",
        "Install a supported local Chromium runtime before browser tasks can run.",
    );

    let status = local_chromium_status(probe);

    assert_eq!(status.readiness, BrowserProviderReadiness::Unavailable);
    assert!(!status.ready);
    assert!(!status.setup_complete);
    assert!(status.remediation.contains(
        &"Install a supported local Chromium runtime before browser tasks can run.".to_string()
    ));
}

#[test]
fn failed_required_action_probe_is_degraded_not_ready() {
    let mut probe = passing_probe();
    probe.capability_probes[1] = BrowserCapabilityProbe::failed(
        "dom_snapshot",
        true,
        "Run a browser readiness probe before starting the task.",
    );

    let status = local_chromium_status(probe);

    assert_eq!(status.readiness, BrowserProviderReadiness::Degraded);
    assert!(!status.ready);
    assert!(status.setup_complete);
    assert!(status
        .remediation
        .contains(&"Run a browser readiness probe before starting the task.".to_string()));
}

#[test]
fn optional_failed_action_probe_does_not_block_ready_status() {
    let mut probe = passing_probe();
    probe.capability_probes.push(BrowserCapabilityProbe::failed(
        "file_upload",
        false,
        "Optional upload probe failed.",
    ));

    let status = local_chromium_status(probe);

    assert_eq!(status.readiness, BrowserProviderReadiness::Ready);
    assert!(status.ready);
    assert!(status
        .remediation
        .contains(&"Optional upload probe failed.".to_string()));
}
