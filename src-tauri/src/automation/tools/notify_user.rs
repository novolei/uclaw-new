// wired in Task 15 (AutomationDelegate)
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct NotifyInput {
    pub channels: Vec<String>, // "system" | "wecom" | "email"
    pub title: String,
    pub body: String,
    pub level: String, // "info" | "important" | "critical"
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "notify_user",
        "description": "Send a side-channel notification to the user via one or more channels. Does NOT mark the run complete — you must still call report_to_user. Requires Notification permission.",
        "input_schema": {
            "type": "object",
            "required": ["channels", "title", "body", "level"],
            "properties": {
                "channels": {
                    "type": "array",
                    "items": { "enum": ["system", "wecom", "email"] }
                },
                "title": { "type": "string" },
                "body":  { "type": "string" },
                "level": { "enum": ["info", "important", "critical"] }
            }
        }
    })
}
