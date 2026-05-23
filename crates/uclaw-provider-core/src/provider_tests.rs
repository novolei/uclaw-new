use super::*;

fn ready_input() -> ProviderReadinessInput {
    ProviderReadinessInput {
        provider_id: "openai".into(),
        display_name: "OpenAI".into(),
        api_family: ProviderApiFamily::OpenAiCompletions,
        auth_requirement: ProviderAuthRequirement::ApiKey,
        credential_status: ProviderCredentialStatus::Present,
        configured: true,
        requires_base_url: true,
        has_base_url: true,
        supports_models: true,
        selected_models: vec!["gpt-5".into()],
        active_model: Some("gpt-5".into()),
        probe_status: ProviderProbeStatus::Passed {
            latency_ms: Some(42),
        },
        capabilities: ProviderCapabilityFlags {
            supports_model_listing: true,
            supports_streaming: true,
            supports_image_input: true,
            supports_reasoning_effort: true,
            supports_prompt_cache: false,
            supports_split_prompt: false,
        },
    }
}

#[test]
fn readiness_report_marks_passed_probe_ready() {
    let report = assess_provider_readiness(ready_input());

    assert_eq!(report.state, ProviderReadinessState::Ready);
    assert!(report.is_usable());
    assert!(report.issues.is_empty());
    assert_eq!(report.active_model.as_deref(), Some("gpt-5"));
}

#[test]
fn readiness_report_requires_credentials_before_model() {
    let mut input = ready_input();
    input.credential_status = ProviderCredentialStatus::Missing;
    input.selected_models.clear();
    input.active_model = None;

    let report = assess_provider_readiness(input);

    assert_eq!(report.state, ProviderReadinessState::NeedsCredentials);
    assert_eq!(report.issues[0].code, "provider.credentials_missing");
}

#[test]
fn readiness_report_requires_saved_configuration_before_probe() {
    let mut input = ready_input();
    input.configured = false;

    let report = assess_provider_readiness(input);

    assert_eq!(report.state, ProviderReadinessState::NeedsConfiguration);
    assert!(!report.is_usable());
    assert_eq!(report.issues[0].code, "provider.config_missing");
}

#[test]
fn readiness_report_requires_model_after_credentials() {
    let mut input = ready_input();
    input.selected_models.clear();
    input.active_model = None;

    let report = assess_provider_readiness(input);

    assert_eq!(report.state, ProviderReadinessState::NeedsModel);
    assert_eq!(report.issues[0].code, "provider.model_missing");
}

#[test]
fn readiness_report_distinguishes_unprobed_from_failed_probe() {
    let mut input = ready_input();
    input.probe_status = ProviderProbeStatus::NotRun;
    let unprobed = assess_provider_readiness(input.clone());
    assert_eq!(unprobed.state, ProviderReadinessState::Unprobed);
    assert!(unprobed.is_usable());

    input.probe_status = ProviderProbeStatus::Failed {
        reason: "HTTP 401".into(),
        latency_ms: Some(100),
    };
    let failed = assess_provider_readiness(input);
    assert_eq!(failed.state, ProviderReadinessState::ProbeFailed);
    assert!(!failed.is_usable());
    assert_eq!(failed.issues[0].code, "provider.probe_failed");
}

#[test]
fn readiness_report_roundtrips_through_json() {
    let report = assess_provider_readiness(ready_input());

    let encoded = serde_json::to_string(&report).expect("serialize");
    assert!(encoded.contains(r#""apiFamily":"openai_completions""#));
    assert!(encoded.contains(r#""authRequirement":"api_key""#));
    let decoded: ProviderReadinessReport = serde_json::from_str(&encoded).expect("deserialize");

    assert_eq!(decoded, report);
}

#[test]
fn probe_status_serializes_latency_as_camel_case() {
    let encoded = serde_json::to_string(&ProviderProbeStatus::Passed {
        latency_ms: Some(42),
    })
    .expect("serialize");

    assert_eq!(encoded, r#"{"status":"passed","latencyMs":42}"#);
}

#[test]
fn readiness_report_serializes_oauth_without_acronym_drift() {
    let mut input = ready_input();
    input.api_family = ProviderApiFamily::OpenAiCodexResponses;
    input.auth_requirement = ProviderAuthRequirement::OAuth;
    input.requires_base_url = false;
    input.has_base_url = false;
    input.supports_models = false;
    input.selected_models.clear();
    input.active_model = None;

    let report = assess_provider_readiness(input);
    let encoded = serde_json::to_string(&report).expect("serialize");

    assert!(encoded.contains(r#""apiFamily":"openai_codex_responses""#));
    assert!(encoded.contains(r#""authRequirement":"oauth""#));
}

#[test]
fn fallback_decision_marks_user_boundary_only_for_resend() {
    assert!(ProviderFallbackDecision::AskUserBeforeResend.requires_user_boundary());
    assert!(!ProviderFallbackDecision::RetryNextProvider.requires_user_boundary());
}
