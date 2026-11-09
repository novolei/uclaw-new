//! Skills marketplace — search skills.sh / skillsmp, fetch a skill's files
//! from GitHub, and install it globally or into a workspace.
//!
//! Ported from uclaw-pi (T6 of docs/superpowers/plans/2026-11-09-uclaw-pi-backport.md);
//! plan: docs/superpowers/plans/2026-11-09-skills-marketplace-t6.md.

pub mod github;
pub mod skills_sh;
pub mod skillsmp;
pub mod types;

pub use skills_sh::SkillsShClient;
pub use skillsmp::SkillsmpClient;
pub use types::*;

/// User agent for every marketplace HTTP call.
pub(crate) const USER_AGENT: &str = "uClaw/0.1";
/// Timeout for every marketplace HTTP call.
pub(crate) const HTTP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// Shared reqwest client builder (timeout + user agent).
pub(crate) fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .timeout(HTTP_TIMEOUT)
        .user_agent(USER_AGENT)
        .build()
        .expect("reqwest client")
}

/// Map a non-success HTTP status to an error: 429 → `RateLimited`.
pub(crate) fn status_error(status: reqwest::StatusCode) -> MarketplaceError {
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        MarketplaceError::RateLimited
    } else {
        MarketplaceError::Http(format!("status {status}"))
    }
}

/// Whether an error is an auth rejection (401/403) worth retrying anonymously.
pub(crate) fn is_auth_rejection(e: &MarketplaceError) -> bool {
    matches!(e, MarketplaceError::Http(s) if s.contains("401") || s.contains("403"))
}
