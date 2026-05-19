use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate { url: String, tab_id: Option<String> },
    Click { tab_id: String, index: u32 },
    Type { tab_id: String, index: u32, text: String },
    Scroll { tab_id: String, direction: String, pixels: Option<u32>, index: Option<u32> },
    SendKeys { tab_id: String, keys: String },
    Evaluate { tab_id: String, script: String },
    GetState { tab_id: String, include_screenshot: bool },
    ListTabs,
    SwitchTab { tab_id: String },
    CloseTab { tab_id: String },
    UploadFile { tab_id: String, index: u32, file_path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserActionResult {
    pub ok: bool,
    pub action_name: String,
    pub message: Option<String>,
    pub tab_id: Option<String>,
    pub observation_json: Option<serde_json::Value>,
    pub error: Option<String>,
    pub duration_ms: u64,
}

impl BrowserActionResult {
    pub fn success(action_name: &str, message: Option<String>) -> Self {
        Self {
            ok: true,
            action_name: action_name.to_string(),
            message,
            tab_id: None,
            observation_json: None,
            error: None,
            duration_ms: 0,
        }
    }

    pub fn failure(action_name: &str, error: String) -> Self {
        Self {
            ok: false,
            action_name: action_name.to_string(),
            message: None,
            tab_id: None,
            observation_json: None,
            error: Some(error),
            duration_ms: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn action_result_serializes_camelcase() {
        let result = BrowserActionResult::success("browser_click", Some("Clicked".into()));
        let json = serde_json::to_string(&result).unwrap();
        assert!(json.contains("\"actionName\":\"browser_click\""), "{json}");
        assert!(json.contains("\"ok\":true"), "{json}");
    }
}
