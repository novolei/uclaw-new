//! Reusable browser identity authorization capture helpers.

use super::context::CookieInfo;
use super::identity::types::BrowserIdentityResult;
use super::identity::{
    BrowserAuthProfileBroker, BrowserIdentityError, BrowserIdentityKind, BrowserIdentityProfile,
    BrowserIdentityProfileInput, BrowserIdentityProvider, BrowserIdentityScope, PlaywrightCookie,
    PlaywrightStorageState,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserIdentityAuthorizationImport {
    pub label: String,
    pub url: String,
    pub scope: BrowserIdentityScope,
}

pub fn import_authorized_storage_state(
    broker: &BrowserAuthProfileBroker,
    input: BrowserIdentityAuthorizationImport,
    state: &PlaywrightStorageState,
) -> BrowserIdentityResult<BrowserIdentityProfile> {
    if !state.has_auth_material() {
        return Err(BrowserIdentityError::InvalidInput(
            "browser identity authorization requires captured auth material".to_string(),
        ));
    }
    let label = input.label.trim();
    let origin_pattern = origin_pattern_for_url(&input.url).ok_or_else(|| {
        BrowserIdentityError::InvalidInput(format!(
            "browser identity authorization url is invalid: {}",
            input.url
        ))
    })?;
    broker.import_playwright_storage_state(
        BrowserIdentityProfileInput {
            label: label.to_string(),
            origin_pattern,
            kind: BrowserIdentityKind::StorageState,
            provider: BrowserIdentityProvider::Playwright,
            scope: input.scope,
        },
        &state.to_json_string()?,
    )
}

pub fn origin_pattern_for_url(raw_url: &str) -> Option<String> {
    let parsed = url::Url::parse(raw_url).ok()?;
    let host = parsed.host_str()?;
    Some(format!("{}://{}", parsed.scheme(), host))
}

pub fn cookie_info_from_webview_cookie(
    cookie: &tauri::webview::Cookie<'_>,
    fallback_host: &str,
    fallback_secure: bool,
) -> CookieInfo {
    CookieInfo {
        name: cookie.name().to_string(),
        value: cookie.value().to_string(),
        domain: cookie
            .domain()
            .map(|domain| domain.to_string())
            .unwrap_or_else(|| fallback_host.to_string()),
        path: cookie.path().unwrap_or("/").to_string(),
        secure: cookie.secure().unwrap_or(fallback_secure),
        http_only: cookie.http_only().unwrap_or(false),
        same_site: cookie.same_site().map(|same_site| format!("{same_site:?}")),
        expires: 0.0,
    }
}

pub fn storage_state_from_cookies(cookies: Vec<CookieInfo>) -> PlaywrightStorageState {
    PlaywrightStorageState {
        cookies: cookies
            .into_iter()
            .filter(|cookie| !cookie.name.trim().is_empty())
            .map(|cookie| PlaywrightCookie {
                name: cookie.name,
                value: cookie.value,
                domain: cookie.domain,
                path: cookie.path,
                expires: None,
                http_only: cookie.http_only,
                secure: cookie.secure,
                same_site: cookie.same_site,
            })
            .collect(),
        origins: Vec::new(),
    }
}

pub fn cookie_matches_login_host(cookie_domain: &str, login_host: &str) -> bool {
    let cookie_domain = cookie_domain.trim_start_matches('.').to_ascii_lowercase();
    let login_host = login_host.to_ascii_lowercase();
    !cookie_domain.is_empty()
        && !login_host.is_empty()
        && (login_host == cookie_domain
            || login_host.ends_with(&format!(".{cookie_domain}"))
            || cookie_domain.ends_with(&format!(".{login_host}"))
            || same_site_suffix_matches(&cookie_domain, &login_host))
}

pub fn is_likely_authenticated_cookie(url: &str, cookies: &[CookieInfo]) -> bool {
    let host = url::Url::parse(url)
        .ok()
        .and_then(|parsed| parsed.host_str().map(|host| host.to_ascii_lowercase()))
        .unwrap_or_default();
    let strong_names: &[&str] = if host.contains("bilibili.com") {
        &["sessdata", "dedeuserid", "bili_jct"]
    } else if host.contains("douyin.com") {
        &[
            "sessionid",
            "sessionid_ss",
            "sid_guard",
            "uid_tt",
            "uid_tt_ss",
            "sid_tt",
            "sid_ucp_v1",
            "ssid_ucp_v1",
            "sid_ucp_sso_v1",
            "ssid_ucp_sso_v1",
            "passport_auth_status",
            "passport_auth_status_ss",
            "sso_uid_tt",
            "sso_uid_tt_ss",
            "toutiao_sso_user",
            "toutiao_sso_user_ss",
        ]
    } else {
        &["session", "auth", "token", "login", "user"]
    };

    cookies.iter().any(|cookie| {
        let name = cookie.name.to_ascii_lowercase();
        strong_names.iter().any(|needle| {
            if host.contains("bilibili.com") || host.contains("douyin.com") {
                name == *needle
            } else {
                name.contains(needle)
            }
        })
    })
}

fn same_site_suffix_matches(left: &str, right: &str) -> bool {
    fn site_suffix(host: &str) -> Option<String> {
        let mut parts = host.rsplit('.');
        let tld = parts.next()?;
        let registrable = parts.next()?;
        Some(format!("{registrable}.{tld}"))
    }
    site_suffix(left)
        .zip(site_suffix(right))
        .is_some_and(|(left, right)| left == right)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::super::identity::MemoryBrowserSecretStore;
    use super::*;

    fn cookie(name: &str) -> CookieInfo {
        CookieInfo {
            name: name.to_string(),
            value: "value".to_string(),
            domain: ".bilibili.com".to_string(),
            path: "/".to_string(),
            secure: true,
            http_only: true,
            same_site: None,
            expires: 0.0,
        }
    }

    #[test]
    fn derives_origin_pattern_from_url() {
        assert_eq!(
            origin_pattern_for_url("https://app.example.com/path?q=1").as_deref(),
            Some("https://app.example.com")
        );
        assert!(origin_pattern_for_url("not a url").is_none());
    }

    #[test]
    fn detects_authenticated_cookie_names() {
        assert!(is_likely_authenticated_cookie(
            "https://www.bilibili.com",
            &[cookie("SESSDATA")]
        ));
        assert!(!is_likely_authenticated_cookie(
            "https://www.bilibili.com",
            &[cookie("buvid3")]
        ));
        assert!(is_likely_authenticated_cookie(
            "https://www.douyin.com/",
            &[CookieInfo {
                name: "passport_auth_status".to_string(),
                domain: ".douyin.com".to_string(),
                ..cookie("passport_auth_status")
            }]
        ));
    }

    #[test]
    fn matches_same_site_cookie_domains_for_login_host() {
        assert!(cookie_matches_login_host(".douyin.com", "www.douyin.com"));
        assert!(cookie_matches_login_host(
            "passport.douyin.com",
            "www.douyin.com"
        ));
        assert!(!cookie_matches_login_host("example.com", "www.douyin.com"));
    }

    #[test]
    fn imports_authorized_storage_state_without_exposing_secret() {
        let temp = tempfile::tempdir().expect("temp dir");
        let broker = BrowserAuthProfileBroker::new_with_secret_store(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );
        let state = storage_state_from_cookies(vec![CookieInfo {
            name: "session".to_string(),
            value: "secret".to_string(),
            domain: ".example.com".to_string(),
            path: "/".to_string(),
            secure: true,
            http_only: true,
            same_site: None,
            expires: 0.0,
        }]);

        let profile = import_authorized_storage_state(
            &broker,
            BrowserIdentityAuthorizationImport {
                label: "Example".to_string(),
                url: "https://app.example.com/login".to_string(),
                scope: BrowserIdentityScope::Global,
            },
            &state,
        )
        .expect("profile import");

        assert_eq!(profile.label, "Example");
        assert_eq!(profile.origin_pattern, "https://app.example.com");
        assert_eq!(profile.scope, BrowserIdentityScope::Global);
        assert!(!profile.secret_handle.is_empty());
        assert_ne!(profile.secret_handle, "secret");
    }

    #[test]
    fn rejects_empty_captured_auth_material() {
        let temp = tempfile::tempdir().expect("temp dir");
        let broker = BrowserAuthProfileBroker::new_with_secret_store(
            temp.path().join("profiles.json"),
            Arc::new(MemoryBrowserSecretStore::default()),
        );
        let state = PlaywrightStorageState {
            cookies: Vec::new(),
            origins: Vec::new(),
        };

        let err = import_authorized_storage_state(
            &broker,
            BrowserIdentityAuthorizationImport {
                label: "Example".to_string(),
                url: "https://app.example.com/login".to_string(),
                scope: BrowserIdentityScope::Global,
            },
            &state,
        )
        .expect_err("empty auth should fail");

        assert!(err.to_string().contains("captured auth material"));
    }
}
