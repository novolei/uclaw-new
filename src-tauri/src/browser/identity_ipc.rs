//! IPC boundary for browser identity visibility and revocation.

use serde::Serialize;

use crate::error::Error;

use super::identity::{
    BrowserAuthProfileBroker, BrowserIdentityError, BrowserIdentityKind, BrowserIdentityProfile,
    BrowserIdentityProvider, BrowserIdentityScope, BrowserIdentityStatus,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentityProfileSummary {
    pub id: String,
    pub label: String,
    pub origin_pattern: String,
    pub kind: BrowserIdentityKind,
    pub provider: BrowserIdentityProvider,
    pub scope: BrowserIdentityScope,
    pub created_at_ms: i64,
    pub last_used_at_ms: Option<i64>,
    pub last_verified_at_ms: Option<i64>,
    pub expires_at_ms: Option<i64>,
    pub revoked_at_ms: Option<i64>,
    pub status: BrowserIdentityStatus,
    pub revoked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentityStatusReport {
    pub profiles: Vec<BrowserIdentityProfileSummary>,
    pub authorized_count: usize,
    pub revoked_count: usize,
    pub active_task_count: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserIdentityRevocationReport {
    pub profile: Option<BrowserIdentityProfileSummary>,
    pub revoked: bool,
    pub active_task_count: Option<usize>,
}

#[tauri::command]
pub async fn list_browser_identities() -> Result<BrowserIdentityStatusReport, Error> {
    let broker = BrowserAuthProfileBroker::system_default().map_err(identity_error_to_error)?;
    browser_identity_status_for_broker(&broker).map_err(identity_error_to_error)
}

#[tauri::command]
pub async fn revoke_browser_identity(
    profile_id: String,
) -> Result<BrowserIdentityRevocationReport, Error> {
    let broker = BrowserAuthProfileBroker::system_default().map_err(identity_error_to_error)?;
    revoke_browser_identity_for_broker(&broker, &profile_id).map_err(identity_error_to_error)
}

fn browser_identity_status_for_broker(
    broker: &BrowserAuthProfileBroker,
) -> Result<BrowserIdentityStatusReport, BrowserIdentityError> {
    let profiles: Vec<_> = broker
        .list_profiles()?
        .into_iter()
        .map(summarize_browser_identity_profile)
        .collect();
    let revoked_count = profiles.iter().filter(|profile| profile.revoked).count();
    let authorized_count = profiles.len().saturating_sub(revoked_count);

    Ok(BrowserIdentityStatusReport {
        profiles,
        authorized_count,
        revoked_count,
        active_task_count: None,
    })
}

fn revoke_browser_identity_for_broker(
    broker: &BrowserAuthProfileBroker,
    profile_id: &str,
) -> Result<BrowserIdentityRevocationReport, BrowserIdentityError> {
    let profile = broker
        .revoke_profile(profile_id)?
        .map(summarize_browser_identity_profile);

    Ok(BrowserIdentityRevocationReport {
        revoked: profile.is_some(),
        profile,
        active_task_count: None,
    })
}

fn summarize_browser_identity_profile(
    profile: BrowserIdentityProfile,
) -> BrowserIdentityProfileSummary {
    let revoked = profile.is_revoked();
    BrowserIdentityProfileSummary {
        id: profile.id,
        label: profile.label,
        origin_pattern: profile.origin_pattern,
        kind: profile.kind,
        provider: profile.provider,
        scope: profile.scope,
        created_at_ms: profile.created_at_ms,
        last_used_at_ms: profile.last_used_at_ms,
        last_verified_at_ms: profile.last_verified_at_ms,
        expires_at_ms: profile.expires_at_ms,
        revoked_at_ms: profile.revoked_at_ms,
        status: profile.status,
        revoked,
    }
}

fn identity_error_to_error(error: BrowserIdentityError) -> Error {
    match error {
        BrowserIdentityError::InvalidInput(message) => Error::InvalidInput(message),
        BrowserIdentityError::ProfileNotFound(id) => {
            Error::NotFound(format!("browser identity profile {id}"))
        }
        BrowserIdentityError::ProfileRevoked(id) => {
            Error::Auth(format!("browser identity profile revoked: {id}"))
        }
        other => Error::Internal(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::identity::{
        BrowserIdentityProfileInput, BrowserIdentityScope, MemoryBrowserSecretStore,
    };
    use super::*;
    use tempfile::TempDir;

    fn broker_with_profiles() -> (TempDir, BrowserAuthProfileBroker) {
        let temp = tempfile::tempdir().expect("temp dir");
        let broker = BrowserAuthProfileBroker::new_with_secret_store(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );

        broker
            .import_playwright_storage_state(
                BrowserIdentityProfileInput {
                    label: "Example".to_string(),
                    origin_pattern: "https://*.example.com".to_string(),
                    kind: BrowserIdentityKind::StorageState,
                    provider: BrowserIdentityProvider::Playwright,
                    scope: BrowserIdentityScope::Global,
                },
                r#"{
                  "cookies": [{"name":"sid","value":"token","domain":".example.com","path":"/"}],
                  "origins": [{"origin":"https://app.example.com","localStorage":[]}]
                }"#,
            )
            .expect("import profile");

        (temp, broker)
    }

    #[test]
    fn identity_status_hides_secret_handles_and_counts_revoked_profiles() {
        let (_temp, broker) = broker_with_profiles();
        let profile_id = broker.list_profiles().unwrap()[0].id.clone();
        broker.revoke_profile(&profile_id).unwrap().unwrap();

        let report = browser_identity_status_for_broker(&broker).unwrap();
        let value = serde_json::to_value(&report).expect("serialize report");

        assert_eq!(report.authorized_count, 0);
        assert_eq!(report.revoked_count, 1);
        assert_eq!(report.active_task_count, None);
        assert_eq!(value["profiles"][0]["id"], profile_id);
        assert_eq!(
            value["profiles"][0]["originPattern"],
            "https://*.example.com"
        );
        assert_eq!(value["profiles"][0]["status"], "revoked");
        assert_eq!(value["profiles"][0]["revoked"], true);
        assert!(value["profiles"][0].get("secretHandle").is_none());
    }

    #[test]
    fn revoke_report_marks_profile_revoked_without_deleting_metadata() {
        let (_temp, broker) = broker_with_profiles();
        let profile_id = broker.list_profiles().unwrap()[0].id.clone();

        let report = revoke_browser_identity_for_broker(&broker, &profile_id).unwrap();

        assert!(report.revoked);
        let profile = report.profile.expect("revoked profile summary");
        assert_eq!(profile.id, profile_id);
        assert_eq!(profile.status, BrowserIdentityStatus::Revoked);
        assert!(profile.revoked);
        assert!(profile.revoked_at_ms.is_some());

        let status = browser_identity_status_for_broker(&broker).unwrap();
        assert_eq!(status.profiles.len(), 1);
        assert_eq!(status.revoked_count, 1);
    }

    #[test]
    fn revoke_report_for_missing_profile_is_non_destructive() {
        let (_temp, broker) = broker_with_profiles();

        let report = revoke_browser_identity_for_broker(&broker, "missing").unwrap();

        assert!(!report.revoked);
        assert!(report.profile.is_none());
        assert_eq!(broker.list_profiles().unwrap().len(), 1);
    }
}
