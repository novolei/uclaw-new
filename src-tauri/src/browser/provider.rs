//! BrowserProvider readiness metadata for uClaw Browser Agent v2.
//!
//! This module is intentionally pure: it does not launch Chromium, mutate
//! profiles, call CDP, or run setup. It gives the runtime/UI a stable shape for
//! status, setup diagnostics, and capability probes.

use serde::{Deserialize, Serialize};

use super::runtime_contracts::{
    browser_provider_capability_cards, rank_browser_provider_candidates,
    BrowserProviderSelectionRequest, BrowserTaskEventName,
};

pub const LOCAL_CHROMIUM_PROVIDER_ID: &str = "browser.local_chromium";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderReadiness {
    Ready,
    NeedsSetup,
    Degraded,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProbeStatus {
    Passed,
    Failed,
    Unsupported,
    Skipped,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserSetupCheck {
    pub id: String,
    pub label: String,
    pub status: BrowserProbeStatus,
    pub remediation: Option<String>,
}

impl BrowserSetupCheck {
    pub fn passed(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            status: BrowserProbeStatus::Passed,
            remediation: None,
        }
    }

    pub fn failed(
        id: impl Into<String>,
        label: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            status: BrowserProbeStatus::Failed,
            remediation: Some(remediation.into()),
        }
    }

    pub fn unsupported(
        id: impl Into<String>,
        label: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            status: BrowserProbeStatus::Unsupported,
            remediation: Some(remediation.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserCapabilityProbe {
    pub action: String,
    pub required: bool,
    pub status: BrowserProbeStatus,
    pub remediation: Option<String>,
}

impl BrowserCapabilityProbe {
    pub fn passed(action: impl Into<String>, required: bool) -> Self {
        Self {
            action: action.into(),
            required,
            status: BrowserProbeStatus::Passed,
            remediation: None,
        }
    }

    pub fn failed(
        action: impl Into<String>,
        required: bool,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            required,
            status: BrowserProbeStatus::Failed,
            remediation: Some(remediation.into()),
        }
    }

    pub fn unsupported(
        action: impl Into<String>,
        required: bool,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            action: action.into(),
            required,
            status: BrowserProbeStatus::Unsupported,
            remediation: Some(remediation.into()),
        }
    }

    pub fn skipped(action: impl Into<String>, required: bool) -> Self {
        Self {
            action: action.into(),
            required,
            status: BrowserProbeStatus::Skipped,
            remediation: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderCapabilities {
    pub provider_id: String,
    pub family: String,
    pub display_name: String,
    pub actions: Vec<String>,
    pub features: Vec<String>,
    pub harness_subjects: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderReadinessProbe {
    pub provider_id: String,
    pub setup_checks: Vec<BrowserSetupCheck>,
    pub capability_probes: Vec<BrowserCapabilityProbe>,
    pub active_contexts: usize,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderStatus {
    pub provider_id: String,
    pub family: String,
    pub display_name: String,
    pub readiness: BrowserProviderReadiness,
    pub ready: bool,
    pub setup_complete: bool,
    pub active_contexts: usize,
    pub capabilities: BrowserProviderCapabilities,
    pub setup_checks: Vec<BrowserSetupCheck>,
    pub capability_probes: Vec<BrowserCapabilityProbe>,
    pub remediation: Vec<String>,
    pub notes: Vec<String>,
}

pub fn local_chromium_capabilities() -> BrowserProviderCapabilities {
    BrowserProviderCapabilities {
        provider_id: LOCAL_CHROMIUM_PROVIDER_ID.to_string(),
        family: "browser".to_string(),
        display_name: "Local Chromium".to_string(),
        actions: vec![
            "navigate",
            "click",
            "type",
            "scroll",
            "send_keys",
            "evaluate",
            "list_tabs",
            "switch_tab",
            "close_tab",
            "dom_snapshot",
            "screenshot",
            "file_upload",
            "checkpoint_resume",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        features: vec![
            "per_session_profiles",
            "identity_scoped_profiles",
            "auth_profiles",
            "user_intervention",
            "task_store",
            "checkpoint_resume",
            "visual_observation",
            "browser_memory_adapter",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
        harness_subjects: vec![
            "browser.navigation",
            "browser.multitab",
            "browser.file_upload",
            "browser.auth_profile",
            "browser.boundary",
            "browser.checkpoint",
            "browser.recovery",
        ]
        .into_iter()
        .map(String::from)
        .collect(),
    }
}

pub fn local_chromium_status(probe: BrowserProviderReadinessProbe) -> BrowserProviderStatus {
    BrowserProviderStatus::from_probe(local_chromium_capabilities(), probe)
}

impl BrowserProviderStatus {
    pub fn from_probe(
        capabilities: BrowserProviderCapabilities,
        probe: BrowserProviderReadinessProbe,
    ) -> Self {
        let setup_complete = probe
            .setup_checks
            .iter()
            .all(|check| check.status == BrowserProbeStatus::Passed);
        let has_unsupported_setup = probe
            .setup_checks
            .iter()
            .any(|check| check.status == BrowserProbeStatus::Unsupported);
        let required_probe_failed = probe.capability_probes.iter().any(|capability| {
            capability.required
                && matches!(
                    capability.status,
                    BrowserProbeStatus::Failed | BrowserProbeStatus::Unsupported
                )
        });

        let readiness = if has_unsupported_setup {
            BrowserProviderReadiness::Unavailable
        } else if !setup_complete {
            BrowserProviderReadiness::NeedsSetup
        } else if required_probe_failed {
            BrowserProviderReadiness::Degraded
        } else {
            BrowserProviderReadiness::Ready
        };

        let remediation = collect_remediation(&probe);

        Self {
            provider_id: capabilities.provider_id.clone(),
            family: capabilities.family.clone(),
            display_name: capabilities.display_name.clone(),
            readiness,
            ready: readiness == BrowserProviderReadiness::Ready,
            setup_complete,
            active_contexts: probe.active_contexts,
            capabilities,
            setup_checks: probe.setup_checks,
            capability_probes: probe.capability_probes,
            remediation,
            notes: probe.notes,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderRouteRequest {
    pub selection: BrowserProviderSelectionRequest,
    pub disabled_provider_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub previous_provider_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserProviderRouteDecisionStatus {
    Selected,
    RolledBack,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderRouteCandidate {
    pub provider_id: String,
    pub rank: u16,
    pub readiness: BrowserProviderReadiness,
    pub eligible: bool,
    pub selection_reason: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocked_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderRouteEventIntent {
    pub event_name: BrowserTaskEventName,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub provider_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserProviderRouteDecision {
    pub status: BrowserProviderRouteDecisionStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_provider_id: Option<String>,
    pub candidates: Vec<BrowserProviderRouteCandidate>,
    pub event_intents: Vec<BrowserProviderRouteEventIntent>,
}

pub fn decide_browser_provider_route(
    request: &BrowserProviderRouteRequest,
    statuses: &[BrowserProviderStatus],
) -> BrowserProviderRouteDecision {
    let ranked_candidates =
        rank_browser_provider_candidates(&request.selection, browser_provider_capability_cards());
    let mut candidates = Vec::new();
    let mut event_intents = Vec::new();
    let mut selected_provider_id = None;

    for ranked in ranked_candidates {
        let Some(status) = statuses
            .iter()
            .find(|status| status.provider_id == ranked.provider_id)
        else {
            candidates.push(BrowserProviderRouteCandidate {
                provider_id: ranked.provider_id.to_string(),
                rank: ranked.rank,
                readiness: BrowserProviderReadiness::Unavailable,
                eligible: false,
                selection_reason: ranked.reason.to_string(),
                blocked_reason: Some("provider_status_missing".to_string()),
            });
            continue;
        };

        let disabled = request
            .disabled_provider_ids
            .iter()
            .any(|provider_id| provider_id == status.provider_id.as_str());
        let blocked_reason = if disabled {
            Some("provider_disabled".to_string())
        } else if !status.ready {
            Some(format!(
                "provider_readiness_{}",
                browser_provider_readiness_slug(status.readiness)
            ))
        } else {
            None
        };
        let eligible = blocked_reason.is_none();

        if !status.ready {
            event_intents.push(BrowserProviderRouteEventIntent {
                event_name: BrowserTaskEventName::ProviderDegraded,
                provider_id: Some(status.provider_id.clone()),
                reason: blocked_reason
                    .clone()
                    .unwrap_or_else(|| "provider_not_ready".to_string()),
            });
        }

        if eligible && selected_provider_id.is_none() {
            selected_provider_id = Some(status.provider_id.clone());
        }

        candidates.push(BrowserProviderRouteCandidate {
            provider_id: status.provider_id.clone(),
            rank: ranked.rank,
            readiness: status.readiness,
            eligible,
            selection_reason: ranked.reason.to_string(),
            blocked_reason,
        });
    }

    let status = match selected_provider_id.as_deref() {
        Some(selected)
            if request
                .previous_provider_id
                .as_deref()
                .is_some_and(|previous| previous != selected) =>
        {
            BrowserProviderRouteDecisionStatus::RolledBack
        }
        Some(_) => BrowserProviderRouteDecisionStatus::Selected,
        None => BrowserProviderRouteDecisionStatus::Blocked,
    };

    match status {
        BrowserProviderRouteDecisionStatus::Selected => {
            event_intents.push(BrowserProviderRouteEventIntent {
                event_name: BrowserTaskEventName::ProviderSelected,
                provider_id: selected_provider_id.clone(),
                reason: "provider_selected".to_string(),
            });
        }
        BrowserProviderRouteDecisionStatus::RolledBack => {
            event_intents.push(BrowserProviderRouteEventIntent {
                event_name: BrowserTaskEventName::ProviderRolledBack,
                provider_id: request.previous_provider_id.clone(),
                reason: "previous_provider_unavailable_or_disabled".to_string(),
            });
            event_intents.push(BrowserProviderRouteEventIntent {
                event_name: BrowserTaskEventName::ProviderSelected,
                provider_id: selected_provider_id.clone(),
                reason: "fallback_provider_selected".to_string(),
            });
        }
        BrowserProviderRouteDecisionStatus::Blocked => {
            event_intents.push(BrowserProviderRouteEventIntent {
                event_name: BrowserTaskEventName::ProviderDegraded,
                provider_id: None,
                reason: "no_eligible_provider".to_string(),
            });
        }
    }

    BrowserProviderRouteDecision {
        status,
        selected_provider_id,
        candidates,
        event_intents,
    }
}

const fn browser_provider_readiness_slug(readiness: BrowserProviderReadiness) -> &'static str {
    match readiness {
        BrowserProviderReadiness::Ready => "ready",
        BrowserProviderReadiness::NeedsSetup => "needs_setup",
        BrowserProviderReadiness::Degraded => "degraded",
        BrowserProviderReadiness::Unavailable => "unavailable",
    }
}

fn collect_remediation(probe: &BrowserProviderReadinessProbe) -> Vec<String> {
    let mut remediation = Vec::new();
    for check in &probe.setup_checks {
        if let Some(text) = &check.remediation {
            remediation.push(text.clone());
        }
    }
    for capability in &probe.capability_probes {
        if let Some(text) = &capability.remediation {
            remediation.push(text.clone());
        }
    }
    remediation.sort();
    remediation.dedup();
    remediation
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
