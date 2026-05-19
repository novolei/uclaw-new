//! Browser identity and auth-profile primitives.
//!
//! This module owns durable metadata plus encrypted/OS-backed auth material
//! for browser automation. Raw cookies, tokens, and storage-state payloads
//! must stay behind a [`BrowserSecretStore`].

pub mod broker;
pub mod playwright_state;
pub mod profile_store;
pub mod secret_store;
pub mod types;

pub use broker::BrowserAuthProfileBroker;
pub use playwright_state::{
    PlaywrightCookie, PlaywrightLocalStorageEntry, PlaywrightOrigin, PlaywrightStorageState,
};
pub use profile_store::BrowserIdentityProfileStore;
pub use secret_store::{BrowserSecretStore, KeyringBrowserSecretStore, MemoryBrowserSecretStore};
pub use types::{
    BrowserIdentityError, BrowserIdentityKind, BrowserIdentityProfile, BrowserIdentityProfileInput,
    BrowserIdentityProvider, BrowserIdentityScope, BrowserIdentityStatus,
};
