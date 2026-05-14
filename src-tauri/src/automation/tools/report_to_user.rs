// wired in Task 15 (AutomationDelegate)
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize)]
pub struct ReportInput {
    pub text: String,
    pub outcome: String, // "useful" | "noop" | "error" | "skipped"
    #[serde(default)]
    pub artifacts: Vec<ReportArtifact>,
}

/// A declared product of an automation run. Persisted as JSON into
/// automation_activities.report_artifacts_json (V24).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ReportArtifact {
    /// "file" | "text" | "url"
    pub kind: String,
    /// Relative path under the spec's working dir, for kind = "file".
    #[serde(default)]
    pub path: Option<String>,
    pub title: String,
}

pub fn schema() -> serde_json::Value {
    serde_json::json!({
        "name": "report_to_user",
        "description": "Mark this automation run as complete and deliver a final report to the user. THIS IS THE ONLY WAY TO END A RUN — without calling this, the run will retry up to 10 times.",
        "input_schema": {
            "type": "object",
            "required": ["text", "outcome"],
            "properties": {
                "text":      { "type": "string" },
                "outcome":   { "enum": ["useful", "noop", "error", "skipped"] },
                "artifacts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["kind", "title"],
                        "properties": {
                            "kind":  { "enum": ["file", "text", "url"] },
                            "path":  { "type": "string" },
                            "title": { "type": "string" }
                        }
                    }
                }
            }
        }
    })
}
