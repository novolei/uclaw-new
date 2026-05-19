use serde::{Deserialize, Serialize};

use crate::browser::types::{DOMElement, TabInfo};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserObservation {
    pub session_id: String,
    pub tab_id: String,
    pub url: String,
    pub title: String,
    pub page_text: String,
    pub elements: Vec<DOMElement>,
    pub tabs: Vec<TabInfo>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub screenshot_b64: Option<String>,
    pub timestamp_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn observation_serializes_camelcase() {
        let obs = BrowserObservation {
            session_id: "s1".into(),
            tab_id: "t1".into(),
            url: "https://example.com".into(),
            title: "Example".into(),
            page_text: "hello".into(),
            elements: vec![],
            tabs: vec![],
            screenshot_b64: Some("abc".into()),
            timestamp_ms: 123,
        };
        let json = serde_json::to_string(&obs).unwrap();
        assert!(json.contains("\"sessionId\":\"s1\""), "{json}");
        assert!(json.contains("\"screenshotB64\":\"abc\""), "{json}");
    }
}
