use std::path::PathBuf;
use std::sync::Arc;

use super::playwright_state::PlaywrightStorageState;
use super::profile_store::BrowserIdentityProfileStore;
use super::secret_store::{BrowserSecretStore, KeyringBrowserSecretStore};
use super::types::{BrowserIdentityProfile, BrowserIdentityProfileInput, BrowserIdentityResult};

#[derive(Clone)]
pub struct BrowserAuthProfileBroker {
    profile_store: BrowserIdentityProfileStore,
}

impl BrowserAuthProfileBroker {
    pub fn new(profile_store: BrowserIdentityProfileStore) -> Self {
        Self { profile_store }
    }

    pub fn new_with_secret_store(
        metadata_path: impl Into<PathBuf>,
        secret_store: Arc<dyn BrowserSecretStore>,
    ) -> Self {
        Self::new(BrowserIdentityProfileStore::new(
            metadata_path,
            secret_store,
        ))
    }

    pub fn system_default() -> BrowserIdentityResult<Self> {
        Ok(Self::new_with_secret_store(
            BrowserIdentityProfileStore::default_metadata_path(),
            Arc::new(KeyringBrowserSecretStore::new()?),
        ))
    }

    pub fn import_playwright_storage_state(
        &self,
        input: BrowserIdentityProfileInput,
        raw_storage_state_json: &str,
    ) -> BrowserIdentityResult<BrowserIdentityProfile> {
        let state = PlaywrightStorageState::from_json_str(raw_storage_state_json)?;
        self.profile_store.import_storage_state(input, &state)
    }

    pub fn list_profiles(&self) -> BrowserIdentityResult<Vec<BrowserIdentityProfile>> {
        self.profile_store.list_profiles()
    }

    pub fn resolve_profile_for_origin(
        &self,
        origin: &str,
    ) -> BrowserIdentityResult<Option<BrowserIdentityProfile>> {
        self.profile_store.resolve_for_origin(origin)
    }

    pub fn resolve_storage_state_for_origin(
        &self,
        origin: &str,
    ) -> BrowserIdentityResult<Option<(BrowserIdentityProfile, PlaywrightStorageState)>> {
        let Some(profile) = self.profile_store.resolve_for_origin(origin)? else {
            return Ok(None);
        };
        let state = self.profile_store.load_storage_state(&profile.id)?;
        Ok(Some((profile, state)))
    }

    pub fn load_storage_state_for_profile(
        &self,
        id: &str,
    ) -> BrowserIdentityResult<(BrowserIdentityProfile, PlaywrightStorageState)> {
        let profile = self.profile_store.get_profile(id)?;
        let state = self.profile_store.load_storage_state(id)?;
        Ok((profile, state))
    }

    pub fn delete_profile(&self, id: &str) -> BrowserIdentityResult<bool> {
        self.profile_store.delete_profile(id)
    }

    pub fn revoke_profile(
        &self,
        id: &str,
    ) -> BrowserIdentityResult<Option<BrowserIdentityProfile>> {
        self.profile_store.revoke_profile(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::identity::{
        BrowserIdentityKind, BrowserIdentityProvider, BrowserIdentityScope,
        MemoryBrowserSecretStore,
    };

    #[test]
    fn broker_imports_and_resolves_playwright_state() {
        let temp = tempfile::tempdir().unwrap();
        let broker = BrowserAuthProfileBroker::new_with_secret_store(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );

        let profile = broker
            .import_playwright_storage_state(
                BrowserIdentityProfileInput {
                    label: "Example".to_string(),
                    origin_pattern: "https://*.example.com".to_string(),
                    kind: BrowserIdentityKind::StorageState,
                    provider: BrowserIdentityProvider::Playwright,
                    scope: BrowserIdentityScope::Workspace,
                },
                r#"{
                  "cookies": [{"name":"sid","value":"token","domain":".example.com","path":"/"}],
                  "origins": [{"origin":"https://app.example.com","localStorage":[]}]
                }"#,
            )
            .unwrap();

        let (resolved_profile, state) = broker
            .resolve_storage_state_for_origin("https://admin.example.com")
            .unwrap()
            .unwrap();
        assert_eq!(resolved_profile.id, profile.id);
        assert!(state.matches_origin("https://app.example.com"));
        assert_eq!(broker.list_profiles().unwrap().len(), 1);

        let revoked = broker.revoke_profile(&profile.id).unwrap().unwrap();
        assert_eq!(revoked.id, profile.id);
        assert!(broker
            .resolve_storage_state_for_origin("https://admin.example.com")
            .unwrap()
            .is_none());
    }
}
