use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserBoundaryKind {
    LoginRequired,
    PasswordField,
    Totp2fa,
    EmailOrSms2fa,
    Captcha,
    Payment,
    PrivacySensitive,
    AuthProfileStale,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BrowserBoundaryRecommendedAction {
    AskUser,
    UseAuthorizedProfile,
    Checkpoint,
    Abort,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBoundaryEvidence {
    pub source: String,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub element_index: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserBoundaryEvent {
    pub id: String,
    pub session_id: String,
    pub tab_id: String,
    pub kind: BrowserBoundaryKind,
    pub url: String,
    pub title: String,
    pub reason: String,
    pub evidence: Vec<BrowserBoundaryEvidence>,
    pub recommended_action: BrowserBoundaryRecommendedAction,
    pub can_resume: bool,
}

pub fn detect_intervention_boundary(
    observation_json: &serde_json::Value,
) -> Option<BrowserBoundaryEvent> {
    let session_id = observation_json
        .get("sessionId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let tab_id = observation_json
        .get("tabId")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let url = observation_json
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let title = observation_json
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let page_text = observation_json
        .get("pageText")
        .and_then(|v| v.as_str())
        .unwrap_or_default();
    let visual_text = visual_text(observation_json);
    let haystack = format!("{url}\n{title}\n{page_text}\n{visual_text}").to_lowercase();

    if let Some(evidence) = keyword_evidence(
        &haystack,
        "captcha",
        &[
            "captcha",
            "recaptcha",
            "hcaptcha",
            "verify you are human",
            "human verification",
            "cloudflare challenge",
        ],
    ) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::Captcha,
            "Page appears to require a CAPTCHA or human verification challenge.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    if let Some(evidence) = keyword_evidence(
        &haystack,
        "2fa",
        &[
            "totp",
            "authenticator app",
            "two-factor",
            "two factor",
            "2fa",
            "one-time code",
            "otp",
        ],
    ) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::Totp2fa,
            "Page appears to require an authenticator app or one-time authentication code.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    if let Some(evidence) = keyword_evidence(
        &haystack,
        "email_or_sms_2fa",
        &[
            "verification code",
            "email code",
            "sms code",
            "text message",
            "sent to your phone",
            "sent to your email",
        ],
    ) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::EmailOrSms2fa,
            "Page appears to require a code delivered by email or SMS.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    if has_password_input(observation_json) {
        let mut evidence = vec![password_evidence(observation_json)];
        if let Some(text) = keyword_evidence(
            &haystack,
            "login",
            &["login", "log in", "sign in", "signin", "account"],
        ) {
            evidence.push(text);
            return Some(boundary(
                session_id,
                tab_id,
                url,
                title,
                BrowserBoundaryKind::LoginRequired,
                "Page appears to require account login credentials.",
                evidence,
                BrowserBoundaryRecommendedAction::UseAuthorizedProfile,
                true,
            ));
        }
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::PasswordField,
            "Page contains a password field. Browser autonomy requires user authorization before filling credentials.",
            evidence,
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    if let Some(evidence) = keyword_evidence(
        &haystack,
        "auth_profile_stale",
        &[
            "session expired",
            "sign in again",
            "login again",
            "reauthenticate",
            "authentication expired",
        ],
    ) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::AuthProfileStale,
            "Existing browser auth profile appears to be stale or expired.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::UseAuthorizedProfile,
            true,
        ));
    }

    if let Some(evidence) = payment_boundary_evidence(observation_json, &haystack) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::Payment,
            "Page appears to require payment, subscription, or billing information.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    if let Some(evidence) = keyword_evidence(
        &haystack,
        "privacy_sensitive",
        &[
            "accept cookies",
            "cookie consent",
            "privacy choices",
            "privacy settings",
            "share my personal information",
            "personal data",
        ],
    ) {
        return Some(boundary(
            session_id,
            tab_id,
            url,
            title,
            BrowserBoundaryKind::PrivacySensitive,
            "Page appears to require privacy, cookie, or personal-data consent.",
            vec![evidence],
            BrowserBoundaryRecommendedAction::AskUser,
            true,
        ));
    }

    None
}

fn payment_boundary_evidence(
    observation_json: &serde_json::Value,
    haystack: &str,
) -> Option<BrowserBoundaryEvidence> {
    if let Some(evidence) = payment_form_evidence(observation_json) {
        return Some(evidence);
    }
    keyword_evidence(
        haystack,
        "payment",
        &[
            "/checkout",
            " checkout",
            "complete purchase",
            "enter payment",
            "payment information",
            "payment method required",
            "billing information",
            "billing details",
            "subscribe to continue",
            "paywall",
            "subscription required",
        ],
    )
}

fn boundary(
    session_id: String,
    tab_id: String,
    url: String,
    title: String,
    kind: BrowserBoundaryKind,
    reason: &str,
    evidence: Vec<BrowserBoundaryEvidence>,
    recommended_action: BrowserBoundaryRecommendedAction,
    can_resume: bool,
) -> BrowserBoundaryEvent {
    BrowserBoundaryEvent {
        id: uuid::Uuid::new_v4().to_string(),
        session_id,
        tab_id,
        kind,
        url,
        title,
        reason: reason.to_string(),
        evidence,
        recommended_action,
        can_resume,
    }
}

fn keyword_evidence(
    haystack: &str,
    source: &str,
    needles: &[&str],
) -> Option<BrowserBoundaryEvidence> {
    needles
        .iter()
        .find(|needle| haystack.contains(**needle))
        .map(|needle| BrowserBoundaryEvidence {
            source: source.to_string(),
            text: (*needle).to_string(),
            element_index: None,
        })
}

fn visual_text(observation_json: &serde_json::Value) -> String {
    observation_json
        .get("visualObservation")
        .and_then(|v| v.get("ocrText"))
        .and_then(|v| v.as_array())
        .map(|boxes| {
            boxes
                .iter()
                .filter_map(|text_box| text_box.get("text").and_then(|v| v.as_str()))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

fn has_password_input(observation_json: &serde_json::Value) -> bool {
    password_element_index(observation_json).is_some()
}

fn password_element_index(observation_json: &serde_json::Value) -> Option<u32> {
    observation_json
        .get("elements")
        .and_then(|v| v.as_array())
        .and_then(|elements| {
            elements.iter().find_map(|element| {
                let is_password = element
                    .get("attributes")
                    .and_then(|attrs| attrs.get("type"))
                    .and_then(|v| v.as_str())
                    .map(|value| value.eq_ignore_ascii_case("password"))
                    .unwrap_or(false);
                if !is_password {
                    return None;
                }
                element
                    .get("index")
                    .and_then(|v| v.as_u64())
                    .map(|index| index as u32)
            })
        })
}

fn password_evidence(observation_json: &serde_json::Value) -> BrowserBoundaryEvidence {
    BrowserBoundaryEvidence {
        source: "dom_element".to_string(),
        text: "input[type=password]".to_string(),
        element_index: password_element_index(observation_json),
    }
}

fn payment_form_evidence(observation_json: &serde_json::Value) -> Option<BrowserBoundaryEvidence> {
    observation_json
        .get("elements")
        .and_then(|v| v.as_array())
        .and_then(|elements| {
            elements.iter().find_map(|element| {
                let attrs = element.get("attributes");
                let field_text = [
                    "name",
                    "id",
                    "autocomplete",
                    "placeholder",
                    "aria-label",
                    "type",
                ]
                .iter()
                .filter_map(|key| {
                    attrs
                        .and_then(|a| a.get(*key))
                        .and_then(|value| value.as_str())
                })
                .chain(element.get("text").and_then(|value| value.as_str()))
                .collect::<Vec<_>>()
                .join(" ")
                .to_lowercase();
                let is_payment_field = [
                    "cc-number",
                    "cc-csc",
                    "cc-exp",
                    "credit card",
                    "card number",
                    "security code",
                    "cvv",
                    "cvc",
                    "billing address",
                ]
                .iter()
                .any(|needle| field_text.contains(needle));
                if !is_payment_field {
                    return None;
                }
                Some(BrowserBoundaryEvidence {
                    source: "payment_form".to_string(),
                    text: field_text,
                    element_index: element
                        .get("index")
                        .and_then(|v| v.as_u64())
                        .map(|index| index as u32),
                })
            })
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_captcha_boundary_from_page_text() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://example.test",
            "title": "Verification",
            "pageText": "Please complete the reCAPTCHA challenge to continue",
            "elements": []
        }))
        .expect("captcha boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::Captcha);
        assert_eq!(
            boundary.recommended_action,
            BrowserBoundaryRecommendedAction::AskUser
        );
        assert!(boundary.can_resume);
    }

    #[test]
    fn detects_login_boundary_when_password_field_is_present() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://example.test/login",
            "title": "Sign in",
            "pageText": "Sign in to continue",
            "elements": [{
                "index": 2,
                "tag": "input",
                "attributes": {"type": "password"}
            }]
        }))
        .expect("login boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::LoginRequired);
        assert_eq!(
            boundary.recommended_action,
            BrowserBoundaryRecommendedAction::UseAuthorizedProfile
        );
        assert_eq!(boundary.evidence[0].element_index, Some(2));
    }

    #[test]
    fn detects_visual_captcha_boundary_from_ocr_text() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://example.test",
            "title": "Empty DOM",
            "pageText": "",
            "elements": [],
            "visualObservation": {
                "ocrText": [{
                    "text": "Verify you are human",
                    "confidence": 0.93,
                    "box": {"x": 1, "y": 2, "width": 3, "height": 4},
                    "source": "mock"
                }]
            }
        }))
        .expect("visual captcha boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::Captcha);
    }

    #[test]
    fn detects_stale_auth_profile_boundary() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://app.example.test",
            "title": "Session expired",
            "pageText": "Your session expired. Please sign in again.",
            "elements": []
        }))
        .expect("stale auth boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::AuthProfileStale);
        assert_eq!(
            boundary.recommended_action,
            BrowserBoundaryRecommendedAction::UseAuthorizedProfile
        );
    }

    #[test]
    fn public_marketing_payment_mentions_are_not_boundaries() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://www.apple.com/",
            "title": "Apple",
            "pageText": "Apple Pay is an easy and secure way to make payments. Shop iPhone, Mac, iPad, and Watch.",
            "elements": []
        }));
        assert!(boundary.is_none());
    }

    #[test]
    fn detects_payment_boundary_from_checkout_context() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://shop.example.test/checkout",
            "title": "Checkout",
            "pageText": "Enter payment information to complete purchase",
            "elements": []
        }))
        .expect("payment boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::Payment);
    }

    #[test]
    fn detects_payment_boundary_from_card_form() {
        let boundary = detect_intervention_boundary(&serde_json::json!({
            "sessionId": "s1",
            "tabId": "t1",
            "url": "https://shop.example.test",
            "title": "Payment",
            "pageText": "",
            "elements": [{
                "index": 7,
                "tag": "input",
                "attributes": {"autocomplete": "cc-number", "placeholder": "Card number"}
            }]
        }))
        .expect("payment boundary");
        assert_eq!(boundary.kind, BrowserBoundaryKind::Payment);
        assert_eq!(boundary.evidence[0].element_index, Some(7));
    }
}
