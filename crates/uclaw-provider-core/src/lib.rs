//! Provider readiness and routing metadata for uClaw Agent OS.
//!
//! This crate is intentionally pure DTO/helper code. It does not own provider
//! credentials, HTTP clients, runtime failover, or provider request execution.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderApiFamily {
    #[serde(rename = "openai_completions")]
    OpenAiCompletions,
    #[serde(rename = "anthropic_messages")]
    AnthropicMessages,
    #[serde(rename = "openai_responses")]
    OpenAiResponses,
    #[serde(rename = "openai_codex_responses")]
    OpenAiCodexResponses,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAuthRequirement {
    ApiKey,
    #[serde(rename = "oauth")]
    OAuth,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCredentialStatus {
    NotRequired,
    Present,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderReadinessState {
    Ready,
    NeedsCredentials,
    NeedsConfiguration,
    NeedsModel,
    Unprobed,
    ProbeFailed,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ProviderProbeStatus {
    NotRun,
    Passed {
        #[serde(rename = "latencyMs", skip_serializing_if = "Option::is_none")]
        latency_ms: Option<u64>,
    },
    Failed {
        reason: String,
        #[serde(rename = "latencyMs", skip_serializing_if = "Option::is_none")]
        latency_ms: Option<u64>,
    },
}

impl Default for ProviderProbeStatus {
    fn default() -> Self {
        Self::NotRun
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderStreamingStatus {
    Supported,
    Assumed,
    Unprobed,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderIssueSeverity {
    Info,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReadinessIssue {
    pub code: String,
    pub message: String,
    pub severity: ProviderIssueSeverity,
}

impl ProviderReadinessIssue {
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        severity: ProviderIssueSeverity,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            severity,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCapabilityFlags {
    pub supports_model_listing: bool,
    pub supports_streaming: bool,
    pub supports_image_input: bool,
    pub supports_reasoning_effort: bool,
    pub supports_prompt_cache: bool,
    pub supports_split_prompt: bool,
}

impl Default for ProviderCapabilityFlags {
    fn default() -> Self {
        Self {
            supports_model_listing: false,
            supports_streaming: true,
            supports_image_input: false,
            supports_reasoning_effort: false,
            supports_prompt_cache: false,
            supports_split_prompt: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRuntimeHints {
    pub api_family: ProviderApiFamily,
    pub auth_requirement: ProviderAuthRequirement,
    pub streaming_status: ProviderStreamingStatus,
    pub capabilities: ProviderCapabilityFlags,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCostSource {
    StaticCatalog,
    UserConfigured,
    ProviderCatalog,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderCostConfidence {
    Exact,
    Estimated,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderCostProfile {
    pub source: ProviderCostSource,
    pub confidence: ProviderCostConfidence,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_micros_per_million_tokens: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_micros_per_million_tokens: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderFallbackDecision {
    None,
    RetrySameProvider,
    RetryNextProvider,
    RetryAndMarkUnavailable,
    AskUserBeforeResend,
}

impl ProviderFallbackDecision {
    pub const fn requires_user_boundary(self) -> bool {
        matches!(self, Self::AskUserBeforeResend)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderRoute {
    pub provider_id: String,
    pub model_id: String,
    pub api_family: ProviderApiFamily,
    pub readiness: ProviderReadinessState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub availability_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cost_profile: Option<ProviderCostProfile>,
    pub capabilities: ProviderCapabilityFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReadinessInput {
    pub provider_id: String,
    pub display_name: String,
    pub api_family: ProviderApiFamily,
    pub auth_requirement: ProviderAuthRequirement,
    pub credential_status: ProviderCredentialStatus,
    pub configured: bool,
    pub requires_base_url: bool,
    pub has_base_url: bool,
    pub supports_models: bool,
    #[serde(default)]
    pub selected_models: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model: Option<String>,
    #[serde(default)]
    pub probe_status: ProviderProbeStatus,
    pub capabilities: ProviderCapabilityFlags,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderReadinessReport {
    pub provider_id: String,
    pub display_name: String,
    pub state: ProviderReadinessState,
    pub credential_status: ProviderCredentialStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_model: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub selected_models: Vec<String>,
    pub runtime_hints: ProviderRuntimeHints,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<ProviderReadinessIssue>,
}

impl ProviderReadinessReport {
    pub fn new(input: ProviderReadinessInput) -> Self {
        let mut issues = Vec::new();
        let state = derive_readiness_state(&input, &mut issues);

        Self {
            provider_id: input.provider_id,
            display_name: input.display_name,
            state,
            credential_status: input.credential_status,
            active_model: input.active_model,
            selected_models: input.selected_models,
            runtime_hints: ProviderRuntimeHints {
                api_family: input.api_family,
                auth_requirement: input.auth_requirement,
                streaming_status: if input.capabilities.supports_streaming {
                    ProviderStreamingStatus::Assumed
                } else {
                    ProviderStreamingStatus::Unprobed
                },
                capabilities: input.capabilities,
            },
            issues,
        }
    }

    pub fn with_issue(mut self, issue: ProviderReadinessIssue) -> Self {
        self.issues.push(issue);
        self
    }

    pub fn is_usable(&self) -> bool {
        matches!(
            self.state,
            ProviderReadinessState::Ready | ProviderReadinessState::Unprobed
        )
    }

    pub fn redacted(&self) -> Self {
        self.clone()
    }
}

pub fn assess_provider_readiness(input: ProviderReadinessInput) -> ProviderReadinessReport {
    ProviderReadinessReport::new(input)
}

fn derive_readiness_state(
    input: &ProviderReadinessInput,
    issues: &mut Vec<ProviderReadinessIssue>,
) -> ProviderReadinessState {
    if input.provider_id.trim().is_empty() {
        issues.push(ProviderReadinessIssue::new(
            "provider.unknown",
            "Provider id is empty.",
            ProviderIssueSeverity::Error,
        ));
        return ProviderReadinessState::Unknown;
    }

    if matches!(input.api_family, ProviderApiFamily::Unknown) {
        issues.push(ProviderReadinessIssue::new(
            "provider.unsupported_api",
            "Provider API family is unknown.",
            ProviderIssueSeverity::Error,
        ));
        return ProviderReadinessState::Unsupported;
    }

    if !input.configured {
        issues.push(ProviderReadinessIssue::new(
            "provider.config_missing",
            "Provider configuration has not been saved.",
            ProviderIssueSeverity::Error,
        ));
        return ProviderReadinessState::NeedsConfiguration;
    }

    if matches!(input.credential_status, ProviderCredentialStatus::Missing) {
        issues.push(ProviderReadinessIssue::new(
            "provider.credentials_missing",
            "Provider credentials are required before this provider can be used.",
            ProviderIssueSeverity::Error,
        ));
        return ProviderReadinessState::NeedsCredentials;
    }

    if input.requires_base_url && !input.has_base_url {
        issues.push(ProviderReadinessIssue::new(
            "provider.base_url_missing",
            "Provider base URL is missing.",
            ProviderIssueSeverity::Error,
        ));
        return ProviderReadinessState::NeedsConfiguration;
    }

    let has_model = input.active_model.is_some() || !input.selected_models.is_empty();
    if input.supports_models && !has_model {
        issues.push(ProviderReadinessIssue::new(
            "provider.model_missing",
            "No model is selected for this provider.",
            ProviderIssueSeverity::Warning,
        ));
        return ProviderReadinessState::NeedsModel;
    }

    match &input.probe_status {
        ProviderProbeStatus::Failed { reason, .. } => {
            issues.push(ProviderReadinessIssue::new(
                "provider.probe_failed",
                reason.clone(),
                ProviderIssueSeverity::Error,
            ));
            ProviderReadinessState::ProbeFailed
        }
        ProviderProbeStatus::NotRun => ProviderReadinessState::Unprobed,
        ProviderProbeStatus::Passed { .. } => ProviderReadinessState::Ready,
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
