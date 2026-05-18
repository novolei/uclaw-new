use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// ── v2 types ─────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct BoundingBox {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DOMElement {
    pub index: u32,
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attributes: HashMap<String, Option<String>>,
    pub is_in_viewport: bool,
    pub xpath: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bounding_box: Option<BoundingBox>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TabInfo {
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DOMState {
    pub url: String,
    pub title: String,
    pub elements: Vec<DOMElement>,
    pub page_text: String,
    pub tabs: Vec<TabInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreencastFramePayload {
    pub session_id: String,
    pub tab_id: String,
    pub data_b64: String,
    pub page_width: u32,
    pub page_height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavStatePayload {
    pub session_id: String,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub is_loading: bool,
    pub can_go_back: bool,
    pub can_go_forward: bool,
}

// ── raw deserialization types (JS → Rust, before normalisation) ───────────────

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomElementRaw {
    pub index: u32,
    pub tag: String,
    pub text: String,
    #[serde(default)]
    pub attributes: HashMap<String, serde_json::Value>,
    pub is_in_viewport: bool,
    pub xpath: String,
    pub bounding_box: Option<BoundingBoxRaw>,
}

#[derive(Debug, Deserialize)]
pub struct BoundingBoxRaw {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DomStateRaw {
    pub url: String,
    pub title: String,
    pub elements: Vec<DomElementRaw>,
    pub page_text: String,
}

// ── legacy types (kept for backward compat with existing commands) ────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserTab {
    pub tab_id: String,
    pub url: String,
    pub title: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScreenshotResult {
    pub data: String,
    pub width: u32,
    pub height: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NavigateInput {
    pub tab_id: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClickInput {
    pub tab_id: String,
    pub selector: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillInput {
    pub tab_id: String,
    pub selector: String,
    pub value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EvaluateInput {
    pub tab_id: String,
    pub expression: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub running: bool,
    pub tabs: Vec<BrowserTab>,
    pub active_tab_id: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dom_element_serializes_camelcase() {
        let elem = DOMElement {
            index: 0,
            tag: "button".to_string(),
            text: "Click me".to_string(),
            attributes: HashMap::new(),
            is_in_viewport: true,
            xpath: "/html/body/button".to_string(),
            bounding_box: None,
        };
        let json = serde_json::to_string(&elem).unwrap();
        assert!(json.contains("\"isInViewport\":true"), "expected camelCase isInViewport, got: {json}");
        assert!(!json.contains("bounding_box"), "None bounding_box should be skipped, got: {json}");
    }

    #[test]
    fn screencast_payload_serializes_camelcase() {
        let payload = ScreencastFramePayload {
            session_id: "s1".to_string(),
            tab_id: "t1".to_string(),
            data_b64: "abc=".to_string(),
            page_width: 1280,
            page_height: 800,
        };
        let json = serde_json::to_string(&payload).unwrap();
        assert!(json.contains("\"dataB64\":\"abc=\""), "expected camelCase dataB64, got: {json}");
        assert!(json.contains("\"sessionId\":\"s1\""), "expected camelCase sessionId, got: {json}");
    }

    #[test]
    fn nav_state_payload_serializes_camelcase() {
        let p = NavStatePayload {
            session_id: "s1".to_string(),
            tab_id: "t1".to_string(),
            url: "https://example.com".to_string(),
            title: "Example".to_string(),
            is_loading: true,
            can_go_back: false,
            can_go_forward: false,
        };
        let json = serde_json::to_string(&p).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "got: {json}");
        assert!(json.contains("\"isLoading\":true"), "got: {json}");
        assert!(json.contains("\"canGoBack\":false"), "got: {json}");
    }
}
