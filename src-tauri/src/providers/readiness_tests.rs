use uclaw_provider_core::{
    ProviderApiFamily, ProviderCredentialStatus, ProviderReadinessState, ProviderStreamingStatus,
};

use crate::providers::registry;
use crate::providers::service::ProviderService;
use crate::providers::types::{ApiType, ModelSelection, ProviderConfig};

use super::{
    api_family_from_api_type, assess_provider_readiness, credential_status_for, requires_base_url,
    runtime_hints_for,
};

fn config(provider_id: &str, api_key: Option<&str>, base_url: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        provider_id: provider_id.to_string(),
        display_name: provider_id.to_string(),
        api_key: api_key.map(ToString::to_string),
        base_url: base_url.map(ToString::to_string),
        api: None,
    }
}

#[test]
fn api_family_maps_existing_wire_protocols() {
    assert_eq!(
        api_family_from_api_type(&ApiType::OpenAiCompletions),
        ProviderApiFamily::OpenAiCompletions
    );
    assert_eq!(
        api_family_from_api_type(&ApiType::AnthropicMessages),
        ProviderApiFamily::AnthropicMessages
    );
    assert_eq!(
        api_family_from_api_type(&ApiType::OpenAiResponses),
        ProviderApiFamily::OpenAiResponses
    );
    assert_eq!(
        api_family_from_api_type(&ApiType::OpenAiCodexResponses),
        ProviderApiFamily::OpenAiCodexResponses
    );
}

#[test]
fn api_key_provider_without_key_needs_credentials() {
    let known = registry::find("openai").expect("openai provider");
    let cfg = config("openai", None, Some("https://api.openai.com/v1"));

    assert_eq!(
        credential_status_for(&known, Some(&cfg)),
        ProviderCredentialStatus::Missing
    );

    let report = assess_provider_readiness("openai", Some(&known), Some(&cfg), &[], None);

    assert_eq!(report.state, ProviderReadinessState::NeedsCredentials);
    assert_eq!(report.issues[0].code, "provider.credentials_missing");
}

#[test]
fn configured_provider_without_selected_model_needs_model() {
    let known = registry::find("openai").expect("openai provider");
    let cfg = config("openai", Some("sk-test"), Some("https://api.openai.com/v1"));

    let report = assess_provider_readiness("openai", Some(&known), Some(&cfg), &[], None);

    assert_eq!(report.state, ProviderReadinessState::NeedsModel);
    assert_eq!(report.issues[0].code, "provider.model_missing");
}

#[test]
fn local_ollama_does_not_require_api_key() {
    let known = registry::find("ollama").expect("ollama provider");
    let cfg = config("ollama", None, Some("http://localhost:11434/v1"));
    let selected = [ModelSelection {
        provider_id: "ollama".into(),
        model_id: "llama3".into(),
    }];

    let report = assess_provider_readiness(
        "ollama",
        Some(&known),
        Some(&cfg),
        &selected,
        selected.first(),
    );

    assert_eq!(
        report.credential_status,
        ProviderCredentialStatus::NotRequired
    );
    assert_eq!(report.state, ProviderReadinessState::Unprobed);
    assert!(report.is_usable());
}

#[test]
fn unconfigured_no_auth_provider_needs_configuration() {
    let known = registry::find("ollama").expect("ollama provider");
    let selected = [ModelSelection {
        provider_id: "ollama".into(),
        model_id: "llama3".into(),
    }];

    let report =
        assess_provider_readiness("ollama", Some(&known), None, &selected, selected.first());

    assert_eq!(report.state, ProviderReadinessState::NeedsConfiguration);
    assert!(!report.is_usable());
    assert_eq!(report.issues[0].code, "provider.config_missing");
}

#[test]
fn codex_oauth_does_not_require_base_url() {
    let known = registry::find("openai-codex-oauth").expect("codex oauth provider");
    let cfg = config("openai-codex-oauth", Some("oauth-token"), None);

    assert!(!requires_base_url(&known));

    let report =
        assess_provider_readiness("openai-codex-oauth", Some(&known), Some(&cfg), &[], None);

    assert_eq!(report.state, ProviderReadinessState::Unprobed);
    assert!(report.is_usable());
}

#[test]
fn unknown_provider_reports_unsupported() {
    let report = assess_provider_readiness("does-not-exist", None, None, &[], None);

    assert_eq!(report.state, ProviderReadinessState::Unsupported);
    assert_eq!(report.issues[0].code, "provider.unsupported_api");
}

#[test]
fn runtime_hints_do_not_claim_network_probe() {
    let known = registry::find("anthropic").expect("anthropic provider");
    let hints = runtime_hints_for(&known);

    assert_eq!(hints.api_family, ProviderApiFamily::AnthropicMessages);
    assert_eq!(hints.streaming_status, ProviderStreamingStatus::Assumed);
    assert!(hints.capabilities.supports_prompt_cache);
    assert!(hints.capabilities.supports_split_prompt);
}

#[tokio::test]
async fn provider_service_reports_do_not_expose_api_keys() {
    let temp = tempfile::tempdir().expect("tempdir");
    let service = ProviderService::new(temp.path()).expect("service");
    service
        .configure_provider_with_models(
            config(
                "openai",
                Some("sk-secret"),
                Some("https://api.openai.com/v1"),
            ),
            &[String::from("gpt-5")],
        )
        .await
        .expect("configure");

    let report = service.provider_readiness("openai").await;

    assert_eq!(report.state, ProviderReadinessState::Unprobed);
    let encoded = serde_json::to_string(&report).expect("serialize");
    assert!(!encoded.contains("sk-secret"));
    assert_eq!(report.active_model.as_deref(), Some("gpt-5"));
}
