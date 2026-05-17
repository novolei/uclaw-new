//! LLM-facing tool: ask the user (via banner) whether they want to
//! switch to Plan mode. Fire-and-forget — does NOT block the agent.
//! The user clicks accept/skip asynchronously; the next agent
//! iteration sees the (possibly) updated effective mode.

use std::time::Instant;
use async_trait::async_trait;
use tauri::Emitter;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};

pub struct RequestPlanModeSwitchTool {
    app_handle: tauri::AppHandle,
    session_id: String,
}

impl RequestPlanModeSwitchTool {
    pub fn new(app_handle: tauri::AppHandle, session_id: String) -> Self {
        Self { app_handle, session_id }
    }
}

#[async_trait]
impl Tool for RequestPlanModeSwitchTool {
    fn name(&self) -> &str { "request_plan_mode_switch" }
    fn description(&self) -> &str {
        "Suggest the user switch to Plan mode for the current task. \
         Fire-and-forget — the user sees a banner and may accept or skip; \
         the agent continues regardless in the current mode."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "reason": {
                    "type": "string",
                    "description": "Why Plan mode would help here. 1-2 sentences."
                },
                "preview_steps": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional initial step sketch to show in the banner."
                }
            },
            "required": ["reason"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Asking the user a question is intrinsically safe.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = Instant::now();
        let reason = params["reason"].as_str()
            .ok_or_else(|| ToolError::InvalidParams("reason is required".into()))?;
        let preview_steps: Vec<String> = params["preview_steps"].as_array()
            .map(|a| a.iter().filter_map(|s| s.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let event_id = uuid::Uuid::new_v4().to_string();
        let payload = serde_json::json!({
            "id": event_id,
            "session_id": self.session_id,
            "source": "agent",
            "reason": reason,
            "preview_steps": preview_steps,
            "fired_at_ms": chrono::Utc::now().timestamp_millis(),
        });
        if let Err(e) = self.app_handle.emit("agent:plan_mode_suggest", payload) {
            tracing::warn!("emit agent:plan_mode_suggest failed: {}", e);
        }

        let duration = start.elapsed().as_millis() as u64;
        Ok(ToolOutput::success(
            "Plan-mode suggestion shown to user. Agent continues in current mode \
             until the user explicitly accepts.",
            duration,
        ))
    }
}
