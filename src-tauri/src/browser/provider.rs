//! BrowserProvider readiness metadata for uClaw Browser Agent v2.
//!
//! This module is intentionally pure: it does not launch Chromium, mutate
//! profiles, call CDP, or run setup. It gives the runtime/UI a stable shape for
//! status, setup diagnostics, and capability probes.

use serde::{Deserialize, Serialize};

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
