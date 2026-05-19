use serde::{Deserialize, Serialize};

use super::types::{BrowserIdentityError, BrowserIdentityResult};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightStorageState {
    #[serde(default)]
    pub cookies: Vec<PlaywrightCookie>,
    #[serde(default)]
    pub origins: Vec<PlaywrightOrigin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightCookie {
    pub name: String,
    pub value: String,
    pub domain: String,
    pub path: String,
    #[serde(default)]
    pub expires: Option<f64>,
    #[serde(default)]
    pub http_only: bool,
    #[serde(default)]
    pub secure: bool,
    #[serde(default)]
    pub same_site: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaywrightOrigin {
    pub origin: String,
    #[serde(default)]
    pub local_storage: Vec<PlaywrightLocalStorageEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaywrightLocalStorageEntry {
    pub name: String,
    pub value: String,
}

impl PlaywrightStorageState {
    pub fn from_json_str(raw: &str) -> BrowserIdentityResult<Self> {
        let state: Self = serde_json::from_str(raw)?;
        state.validate()?;
        Ok(state)
    }

    pub fn to_json_string(&self) -> BrowserIdentityResult<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }

    pub fn validate(&self) -> BrowserIdentityResult<()> {
        for cookie in &self.cookies {
            if cookie.name.trim().is_empty() {
                return Err(BrowserIdentityError::InvalidInput(
                    "storageState cookie name cannot be empty".to_string(),
                ));
            }
            if cookie.domain.trim().is_empty() {
                return Err(BrowserIdentityError::InvalidInput(format!(
                    "storageState cookie '{}' domain cannot be empty",
                    cookie.name
                )));
            }
            if cookie.path.trim().is_empty() {
                return Err(BrowserIdentityError::InvalidInput(format!(
                    "storageState cookie '{}' path cannot be empty",
                    cookie.name
                )));
            }
        }
        for origin in &self.origins {
            if origin.origin.trim().is_empty() {
                return Err(BrowserIdentityError::InvalidInput(
                    "storageState origin cannot be empty".to_string(),
                ));
            }
        }
        Ok(())
    }

    pub fn has_auth_material(&self) -> bool {
        !self.cookies.is_empty()
            || self
                .origins
                .iter()
                .any(|origin| !origin.local_storage.is_empty())
    }

    pub fn matches_origin(&self, origin: &str) -> bool {
        self.origins.iter().any(|entry| entry.origin == origin)
            || self.cookies.iter().any(|cookie| {
                let host = origin
                    .split_once("://")
                    .map(|(_, rest)| rest)
                    .unwrap_or(origin)
                    .split('/')
                    .next()
                    .unwrap_or(origin)
                    .split(':')
                    .next()
                    .unwrap_or(origin);
                domain_matches(host, &cookie.domain)
            })
    }
}

fn domain_matches(host: &str, cookie_domain: &str) -> bool {
    let cookie_domain = cookie_domain.trim_start_matches('.');
    host == cookie_domain || host.ends_with(&format!(".{cookie_domain}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_playwright_storage_state() {
        let raw = r#"{
          "cookies": [{
            "name": "sid",
            "value": "secret",
            "domain": ".example.com",
            "path": "/",
            "expires": 1893456000,
            "httpOnly": true,
            "secure": true,
            "sameSite": "Lax"
          }],
          "origins": [{
            "origin": "https://app.example.com",
            "localStorage": [{"name": "token", "value": "abc"}]
          }]
        }"#;

        let state = PlaywrightStorageState::from_json_str(raw).unwrap();
        assert!(state.has_auth_material());
        assert!(state.matches_origin("https://app.example.com"));
        assert!(state.matches_origin("https://admin.example.com"));
        assert!(!state.matches_origin("https://example.org"));
        assert!(state.to_json_string().unwrap().contains("\"httpOnly\""));
    }

    #[test]
    fn rejects_empty_cookie_domain() {
        let raw = r#"{"cookies":[{"name":"sid","value":"x","domain":"","path":"/"}],"origins":[]}"#;
        let err = PlaywrightStorageState::from_json_str(raw).unwrap_err();
        assert!(err.to_string().contains("domain cannot be empty"));
    }
}
