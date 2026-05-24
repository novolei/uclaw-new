use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum BrowserIdentityError {
    #[error("browser identity io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("browser identity json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("browser identity keyring error: {0}")]
    Keyring(String),
    #[error("browser identity profile not found: {0}")]
    ProfileNotFound(String),
    #[error("browser identity profile is revoked: {0}")]
    ProfileRevoked(String),
    #[error("browser identity secret not found: {0}")]
    SecretNotFound(String),
    #[error("browser identity invalid input: {0}")]
    InvalidInput(String),
}

pub type BrowserIdentityResult<T> = Result<T, BrowserIdentityError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityKind {
    RealBrowserProfile,
    StorageState,
    CookieJar,
    BearerToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityProvider {
    SystemChrome,
    Playwright,
    BrowserUse,
    ManualImport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityScope {
    Workspace,
    Session,
    Global,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BrowserIdentityStatus {
    Live,
    Stale,
    Unknown,
    Revoked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserIdentityProfileInput {
    pub label: String,
    pub origin_pattern: String,
    pub kind: BrowserIdentityKind,
    pub provider: BrowserIdentityProvider,
    pub scope: BrowserIdentityScope,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BrowserIdentityProfile {
    pub id: String,
    pub label: String,
    pub origin_pattern: String,
    pub kind: BrowserIdentityKind,
    pub provider: BrowserIdentityProvider,
    pub scope: BrowserIdentityScope,
    pub secret_handle: String,
    pub created_at_ms: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_verified_at_ms: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expires_at_ms: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revoked_at_ms: Option<i64>,
    pub status: BrowserIdentityStatus,
}

impl BrowserIdentityProfile {
    pub fn is_revoked(&self) -> bool {
        self.status == BrowserIdentityStatus::Revoked || self.revoked_at_ms.is_some()
    }
}

impl BrowserIdentityProfileInput {
    pub fn validate(&self) -> BrowserIdentityResult<()> {
        if self.label.trim().is_empty() {
            return Err(BrowserIdentityError::InvalidInput(
                "profile label cannot be empty".to_string(),
            ));
        }
        if self.origin_pattern.trim().is_empty() {
            return Err(BrowserIdentityError::InvalidInput(
                "origin pattern cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}
