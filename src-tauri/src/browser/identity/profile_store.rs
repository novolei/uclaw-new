use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::playwright_state::PlaywrightStorageState;
use super::secret_store::BrowserSecretStore;
use super::types::{
    BrowserIdentityError, BrowserIdentityKind, BrowserIdentityProfile, BrowserIdentityProfileInput,
    BrowserIdentityResult, BrowserIdentityStatus,
};

#[derive(Clone)]
pub struct BrowserIdentityProfileStore {
    metadata_path: PathBuf,
    secret_store: Arc<dyn BrowserSecretStore>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct BrowserIdentityProfileIndex {
    #[serde(default)]
    profiles: Vec<BrowserIdentityProfile>,
}

impl BrowserIdentityProfileStore {
    pub fn new(
        metadata_path: impl Into<PathBuf>,
        secret_store: Arc<dyn BrowserSecretStore>,
    ) -> Self {
        Self {
            metadata_path: metadata_path.into(),
            secret_store,
        }
    }

    pub fn default_metadata_path() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("/tmp"))
            .join(".uclaw")
            .join("browser-identities")
            .join("profiles.json")
    }

    pub fn import_storage_state(
        &self,
        input: BrowserIdentityProfileInput,
        state: &PlaywrightStorageState,
    ) -> BrowserIdentityResult<BrowserIdentityProfile> {
        input.validate()?;
        if input.kind != BrowserIdentityKind::StorageState {
            return Err(BrowserIdentityError::InvalidInput(
                "import_storage_state requires kind=storage_state".to_string(),
            ));
        }
        if !state.has_auth_material() {
            return Err(BrowserIdentityError::InvalidInput(
                "storageState contains no cookies or localStorage entries".to_string(),
            ));
        }

        let mut index = self.load_index()?;
        let id = format!("auth-{}", Uuid::new_v4());
        let secret_handle = format!("browser-identity:{id}:storage-state");
        let profile = BrowserIdentityProfile {
            id,
            label: input.label.trim().to_string(),
            origin_pattern: input.origin_pattern.trim().to_string(),
            kind: input.kind,
            provider: input.provider,
            scope: input.scope,
            secret_handle: secret_handle.clone(),
            created_at_ms: Utc::now().timestamp_millis(),
            last_verified_at_ms: None,
            expires_at_ms: None,
            status: BrowserIdentityStatus::Unknown,
        };

        self.secret_store
            .put_secret(&secret_handle, &state.to_json_string()?)?;
        index.profiles.push(profile.clone());
        self.save_index(&index)?;
        Ok(profile)
    }

    pub fn list_profiles(&self) -> BrowserIdentityResult<Vec<BrowserIdentityProfile>> {
        Ok(self.load_index()?.profiles)
    }

    pub fn get_profile(&self, id: &str) -> BrowserIdentityResult<BrowserIdentityProfile> {
        self.load_index()?
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| BrowserIdentityError::ProfileNotFound(id.to_string()))
    }

    pub fn resolve_for_origin(
        &self,
        origin: &str,
    ) -> BrowserIdentityResult<Option<BrowserIdentityProfile>> {
        Ok(self
            .load_index()?
            .profiles
            .into_iter()
            .find(|profile| origin_pattern_matches(&profile.origin_pattern, origin)))
    }

    pub fn load_storage_state(&self, id: &str) -> BrowserIdentityResult<PlaywrightStorageState> {
        let profile = self.get_profile(id)?;
        let secret = self
            .secret_store
            .get_secret(&profile.secret_handle)?
            .ok_or_else(|| BrowserIdentityError::SecretNotFound(profile.secret_handle.clone()))?;
        PlaywrightStorageState::from_json_str(&secret)
    }

    pub fn delete_profile(&self, id: &str) -> BrowserIdentityResult<bool> {
        let mut index = self.load_index()?;
        let before = index.profiles.len();
        let mut removed_handles = Vec::new();
        index.profiles.retain(|profile| {
            let keep = profile.id != id;
            if !keep {
                removed_handles.push(profile.secret_handle.clone());
            }
            keep
        });
        if index.profiles.len() == before {
            return Ok(false);
        }
        self.save_index(&index)?;
        for handle in removed_handles {
            self.secret_store.delete_secret(&handle)?;
        }
        Ok(true)
    }

    fn load_index(&self) -> BrowserIdentityResult<BrowserIdentityProfileIndex> {
        if !self.metadata_path.exists() {
            return Ok(BrowserIdentityProfileIndex::default());
        }
        let raw = fs::read_to_string(&self.metadata_path)?;
        Ok(serde_json::from_str(&raw)?)
    }

    fn save_index(&self, index: &BrowserIdentityProfileIndex) -> BrowserIdentityResult<()> {
        if let Some(parent) = self.metadata_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp_path = tmp_path_for(&self.metadata_path);
        fs::write(&tmp_path, serde_json::to_vec_pretty(index)?)?;
        fs::rename(tmp_path, &self.metadata_path)?;
        Ok(())
    }
}

fn tmp_path_for(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profiles.json");
    path.with_file_name(format!("{file_name}.tmp"))
}

fn origin_pattern_matches(pattern: &str, origin: &str) -> bool {
    let pattern = pattern.trim();
    if pattern == "*" || pattern == origin {
        return true;
    }
    if let Some(suffix) = pattern.strip_prefix("*.") {
        return origin_host(origin)
            .map(|host| host == suffix || host.ends_with(&format!(".{suffix}")))
            .unwrap_or(false);
    }
    if let Some((scheme, rest)) = pattern.split_once("://*.") {
        return origin
            .strip_prefix(&format!("{scheme}://"))
            .and_then(|candidate| origin_host(candidate))
            .map(|host| host == rest || host.ends_with(&format!(".{rest}")))
            .unwrap_or(false);
    }
    false
}

fn origin_host(origin: &str) -> Option<&str> {
    Some(
        origin
            .split_once("://")
            .map(|(_, rest)| rest)
            .unwrap_or(origin)
            .split('/')
            .next()?
            .split(':')
            .next()?,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::identity::{
        BrowserIdentityProvider, BrowserIdentityScope, MemoryBrowserSecretStore,
    };

    fn sample_state() -> PlaywrightStorageState {
        PlaywrightStorageState::from_json_str(
            r#"{
              "cookies": [{"name":"sid","value":"sensitive-token","domain":".example.com","path":"/"}],
              "origins": [{"origin":"https://app.example.com","localStorage":[]}]
            }"#,
        )
        .unwrap()
    }

    fn sample_input() -> BrowserIdentityProfileInput {
        BrowserIdentityProfileInput {
            label: "Example app".to_string(),
            origin_pattern: "https://*.example.com".to_string(),
            kind: BrowserIdentityKind::StorageState,
            provider: BrowserIdentityProvider::Playwright,
            scope: BrowserIdentityScope::Workspace,
        }
    }

    #[test]
    fn imports_metadata_without_exposing_secret() {
        let temp = tempfile::tempdir().unwrap();
        let store = BrowserIdentityProfileStore::new(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );

        let profile = store
            .import_storage_state(sample_input(), &sample_state())
            .unwrap();
        assert_eq!(profile.label, "Example app");
        assert!(profile.secret_handle.starts_with("browser-identity:auth-"));

        let metadata = fs::read_to_string(temp.path().join("profiles.json")).unwrap();
        assert!(!metadata.contains("sensitive-token"));
        assert_eq!(store.list_profiles().unwrap().len(), 1);
    }

    #[test]
    fn resolves_loads_and_deletes_profile() {
        let temp = tempfile::tempdir().unwrap();
        let store = BrowserIdentityProfileStore::new(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );
        let profile = store
            .import_storage_state(sample_input(), &sample_state())
            .unwrap();

        let resolved = store
            .resolve_for_origin("https://admin.example.com")
            .unwrap()
            .unwrap();
        assert_eq!(resolved.id, profile.id);
        assert!(store
            .load_storage_state(&profile.id)
            .unwrap()
            .matches_origin("https://app.example.com"));

        assert!(store.delete_profile(&profile.id).unwrap());
        assert!(!store.delete_profile(&profile.id).unwrap());
        assert!(matches!(
            store.load_storage_state(&profile.id).unwrap_err(),
            BrowserIdentityError::ProfileNotFound(_)
        ));
    }
}
