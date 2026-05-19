use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(target_os = "macos")]
use security_framework::os::macos::keychain::SecKeychain;
#[cfg(target_os = "macos")]
use security_framework::os::macos::passwords::find_generic_password;

use super::types::{BrowserIdentityError, BrowserIdentityResult};

pub trait BrowserSecretStore: Send + Sync {
    fn put_secret(&self, handle: &str, secret: &str) -> BrowserIdentityResult<()>;
    fn get_secret(&self, handle: &str) -> BrowserIdentityResult<Option<String>>;
    fn delete_secret(&self, handle: &str) -> BrowserIdentityResult<()>;
}

#[derive(Debug, Clone, Default)]
pub struct MemoryBrowserSecretStore {
    secrets: Arc<Mutex<HashMap<String, String>>>,
}

impl BrowserSecretStore for MemoryBrowserSecretStore {
    fn put_secret(&self, handle: &str, secret: &str) -> BrowserIdentityResult<()> {
        self.secrets
            .lock()
            .map_err(|_| BrowserIdentityError::Keyring("memory secret store poisoned".to_string()))?
            .insert(handle.to_string(), secret.to_string());
        Ok(())
    }

    fn get_secret(&self, handle: &str) -> BrowserIdentityResult<Option<String>> {
        Ok(self
            .secrets
            .lock()
            .map_err(|_| BrowserIdentityError::Keyring("memory secret store poisoned".to_string()))?
            .get(handle)
            .cloned())
    }

    fn delete_secret(&self, handle: &str) -> BrowserIdentityResult<()> {
        self.secrets
            .lock()
            .map_err(|_| BrowserIdentityError::Keyring("memory secret store poisoned".to_string()))?
            .remove(handle);
        Ok(())
    }
}

#[derive(Debug, Clone, Default)]
pub struct KeyringBrowserSecretStore;

impl KeyringBrowserSecretStore {
    const SERVICE: &'static str = "uclaw.browser.identity";

    pub fn new() -> BrowserIdentityResult<Self> {
        Ok(Self)
    }
}

#[cfg(target_os = "macos")]
impl BrowserSecretStore for KeyringBrowserSecretStore {
    fn put_secret(&self, handle: &str, secret: &str) -> BrowserIdentityResult<()> {
        SecKeychain::default()
            .map_err(|err| BrowserIdentityError::Keyring(err.to_string()))?
            .set_generic_password(Self::SERVICE, handle, secret.as_bytes())
            .map_err(|err| BrowserIdentityError::Keyring(err.to_string()))
    }

    fn get_secret(&self, handle: &str) -> BrowserIdentityResult<Option<String>> {
        match find_generic_password(None, Self::SERVICE, handle) {
            Ok((password, _)) => String::from_utf8(password.to_vec())
                .map(Some)
                .map_err(|err| BrowserIdentityError::Keyring(err.to_string())),
            Err(err) if keychain_error_is_not_found(&err) => Ok(None),
            Err(err) => Err(BrowserIdentityError::Keyring(err.to_string())),
        }
    }

    fn delete_secret(&self, handle: &str) -> BrowserIdentityResult<()> {
        match find_generic_password(None, Self::SERVICE, handle) {
            Ok((_, item)) => {
                item.delete();
                Ok(())
            }
            Err(err) if keychain_error_is_not_found(&err) => Ok(()),
            Err(err) => Err(BrowserIdentityError::Keyring(err.to_string())),
        }
    }
}

#[cfg(not(target_os = "macos"))]
impl BrowserSecretStore for KeyringBrowserSecretStore {
    fn put_secret(&self, _handle: &str, _secret: &str) -> BrowserIdentityResult<()> {
        Err(BrowserIdentityError::Keyring(
            "system keychain secret store is only implemented on macOS".to_string(),
        ))
    }

    fn get_secret(&self, _handle: &str) -> BrowserIdentityResult<Option<String>> {
        Err(BrowserIdentityError::Keyring(
            "system keychain secret store is only implemented on macOS".to_string(),
        ))
    }

    fn delete_secret(&self, _handle: &str) -> BrowserIdentityResult<()> {
        Err(BrowserIdentityError::Keyring(
            "system keychain secret store is only implemented on macOS".to_string(),
        ))
    }
}

#[cfg(target_os = "macos")]
fn keychain_error_is_not_found(err: &security_framework::base::Error) -> bool {
    // macOS errSecItemNotFound.
    err.code() == -25300
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_secret_store_round_trips_and_deletes() {
        let store = MemoryBrowserSecretStore::default();
        store.put_secret("profile-1", "secret").unwrap();
        assert_eq!(
            store.get_secret("profile-1").unwrap(),
            Some("secret".to_string())
        );
        store.delete_secret("profile-1").unwrap();
        assert_eq!(store.get_secret("profile-1").unwrap(), None);
    }
}
