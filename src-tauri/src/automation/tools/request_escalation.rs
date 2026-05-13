// wired in Task 15 (AutomationDelegate)
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct EscalationChoice {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
}

// wired in Task 15 (AutomationDelegate)
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct RequestEscalationInput {
    pub question: String,
    pub choices: Vec<EscalationChoice>,
    pub context_for_user: Option<String>,
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "request_escalation",
        "description": "Pause this run and ask the human a question that requires a decision. The run terminates immediately after this call; the user's choice will resume a new run with the escalation context injected.",
        "input_schema": {
            "type": "object",
            "required": ["question", "choices"],
            "properties": {
                "question": { "type": "string" },
                "choices": {
                    "type": "array",
                    "minItems": 2,
                    "items": {
                        "type": "object",
                        "required": ["id", "label"],
                        "properties": {
                            "id":          { "type": "string" },
                            "label":       { "type": "string" },
                            "description": { "type": "string" }
                        }
                    }
                },
                "context_for_user": { "type": "string" }
            }
        }
    })
}
