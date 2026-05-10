//! `ask_user` built-in tool — agent pauses execution and asks the user
//! clarifying questions with structured options or free-form text.
//!
//! Available in all SafetyModes. Reuses the PendingAskUsers oneshot
//! pattern (mirrors PendingApprovals from the approval flow).
//!
//! Flow:
//!   1. Agent calls ask_user({ questions: [...] })
//!   2. Backend register oneshot + emit `agent:ask_user_request` IPC event
//!   3. Loop blocks awaiting the oneshot
//!   4. Frontend AskUserBanner renders questions + answer UI
//!   5. User answers → respond_ask_user IPC command resolves oneshot
//!   6. Agent receives answers as tool result, continues

use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::app::PendingAskUsers;
use crate::ipc::{AskUserQuestion, AskUserRequestPayload};
use tauri::{AppHandle, Emitter};

pub struct AskUserTool {
    app_handle: AppHandle,
    pending: Arc<PendingAskUsers>,
    session_id: String,
}

impl AskUserTool {
    pub fn new(app_handle: AppHandle, pending: Arc<PendingAskUsers>, session_id: String) -> Self {
        Self { app_handle, pending, session_id }
    }
}

#[async_trait]
impl Tool for AskUserTool {
    fn name(&self) -> &str { "ask_user" }
    fn description(&self) -> &str {
        "Pause execution and ask the user one or more clarifying questions \
         with optional multiple-choice options. Returns the user's answers."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "questions": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "question": {"type": "string"},
                            "header":   {"type": "string"},
                            "multi_select": {"type": "boolean", "default": false},
                            "options": {
                                "type": "array",
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "label":       {"type": "string"},
                                        "description": {"type": "string"},
                                        "preview":     {"type": "string"}
                                    },
                                    "required": ["label"]
                                }
                            }
                        },
                        "required": ["question", "multi_select"]
                    }
                }
            },
            "required": ["questions"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        // Asking the user for input is intrinsically safe — no need for the
        // approval modal on top of the question banner.
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let questions: Vec<AskUserQuestion> = serde_json::from_value(
            params.get("questions").cloned().unwrap_or_default()
        ).map_err(|e| ToolError::InvalidParams(format!("questions: {}", e)))?;

        if questions.is_empty() {
            return Err(ToolError::InvalidParams("questions array cannot be empty".into()));
        }

        let request_id = uuid::Uuid::new_v4().to_string();
        let rx = self.pending.register(request_id.clone());

        let payload = AskUserRequestPayload {
            request_id: request_id.clone(),
            session_id: self.session_id.clone(),
            questions,
        };
        let _ = self.app_handle.emit("agent:ask_user_request", &payload);

        let result = rx.await.map_err(|_| {
            ToolError::Execution("ask_user channel dropped — user closed without answering".into())
        })?;

        let result_json = serde_json::json!({ "answers": result.answers });
        Ok(ToolOutput::success(
            &serde_json::to_string(&result_json).unwrap_or_default(),
            start.elapsed().as_millis() as u64,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{AskUserResult, PendingAskUsers};
    use std::collections::HashMap;
    use std::sync::Arc;

    /// We can't easily test `execute` end-to-end (needs AppHandle). But we
    /// can verify the registry round-trip — the same primitive the tool uses.
    #[tokio::test]
    async fn pending_ask_users_register_and_resolve() {
        let pending = Arc::new(PendingAskUsers::new());
        let rx = pending.register("req-1".into());
        let mut answers = HashMap::new();
        answers.insert("question_0".into(), serde_json::Value::String("A".into()));
        let resolved = pending.resolve("req-1", AskUserResult { answers: answers.clone() });
        assert!(resolved);
        let result = rx.await.unwrap();
        assert_eq!(result.answers, answers);
    }

    #[tokio::test]
    async fn pending_ask_users_resolve_unknown_returns_false() {
        let pending = Arc::new(PendingAskUsers::new());
        let resolved = pending.resolve("unknown", AskUserResult { answers: HashMap::new() });
        assert!(!resolved);
    }
}
