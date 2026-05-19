use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserInterventionKind {
    Captcha,
    Login,
    TwoFactor,
    Paywall,
    Consent,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserInterventionBoundary {
    pub kind: BrowserInterventionKind,
    pub reason: String,
}

pub fn detect_intervention_boundary(
    observation_json: &serde_json::Value,
) -> Option<BrowserInterventionBoundary> {
    let haystack = format!(
        "{}\n{}\n{}",
        observation_json.get("url").and_then(|v| v.as_str()).unwrap_or_default(),
        observation_json.get("title").and_then(|v| v.as_str()).unwrap_or_default(),
        observation_json.get("pageText").and_then(|v| v.as_str()).unwrap_or_default(),
    ).to_lowercase();

    if contains_any(&haystack, &["captcha", "recaptcha", "hcaptcha", "verify you are human"]) {
        return Some(boundary(
            BrowserInterventionKind::Captcha,
            "Page appears to require a CAPTCHA or human verification challenge.",
        ));
    }
    if contains_any(&haystack, &["two-factor", "two factor", "2fa", "verification code", "one-time code", "otp"]) {
        return Some(boundary(
            BrowserInterventionKind::TwoFactor,
            "Page appears to require a one-time code or two-factor authentication.",
        ));
    }
    if has_password_input(observation_json)
        && contains_any(&haystack, &["login", "log in", "sign in", "signin", "account"])
    {
        return Some(boundary(
            BrowserInterventionKind::Login,
            "Page appears to require account login credentials.",
        ));
    }
    if contains_any(&haystack, &["subscribe to continue", "paywall", "subscription required"]) {
        return Some(boundary(
            BrowserInterventionKind::Paywall,
            "Page appears to be behind a paywall or subscription boundary.",
        ));
    }
    if contains_any(&haystack, &["accept cookies", "cookie consent", "privacy choices"]) {
        return Some(boundary(
            BrowserInterventionKind::Consent,
            "Page appears to require cookie or privacy consent.",
        ));
    }
    None
}

fn boundary(kind: BrowserInterventionKind, reason: &str) -> BrowserInterventionBoundary {
    BrowserInterventionBoundary { kind, reason: reason.to_string() }
}

fn contains_any(value: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| value.contains(needle))
}

fn has_password_input(observation_json: &serde_json::Value) -> bool {
    observation_json
        .get("elements")
        .and_then(|v| v.as_array())
        .map(|elements| {
            elements.iter().any(|element| {
                element
                    .get("attributes")
                    .and_then(|attrs| attrs.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|value| value.eq_ignore_ascii_case("password"))
                    .unwrap_or(false)
            })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_captcha_boundary_from_page_text() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "url": "https://example.test",
            "title": "Verification",
            "pageText": "Please complete the reCAPTCHA challenge to continue",
            "elements": []
        })).expect("captcha boundary");
        assert_eq!(boundary.kind, BrowserInterventionKind::Captcha);
    }

    #[test]
    fn detects_login_boundary_when_password_field_is_present() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "url": "https://example.test/login",
            "title": "Sign in",
            "pageText": "Sign in to continue",
            "elements": [{
                "index": 2,
                "tag": "input",
                "attributes": {"type": "password"}
            }]
        })).expect("login boundary");
        assert_eq!(boundary.kind, BrowserInterventionKind::Login);
    }
}
