//! `exit_plan_mode` built-in tool — agent declares "plan ready" with a
//! markdown plan + optional allowed_prompts list. User sees a confirmation
//! modal and can:
//!   - accept_and_auto  → backend switches session SafetyMode to Supervised
//!   - accept_keep_plan → backend writes allowed_prompts as V14 session
//!                        pattern rules (so e.g. `bash cargo test` becomes
//!                        auto-pass while staying in Plan mode)
//!   - reject           → tool returns error with user's feedback string

use async_trait::async_trait;
use std::sync::Arc;
use crate::agent::tools::tool::{ApprovalRequirement, Tool, ToolError, ToolOutput};
use crate::app::PendingExitPlans;
use crate::ipc::ExitPlanRequestPayload;
use tauri::{AppHandle, Emitter};

pub struct ExitPlanModeTool {
    app_handle: AppHandle,
    pending: Arc<PendingExitPlans>,
    session_id: String,
}

impl ExitPlanModeTool {
    pub fn new(app_handle: AppHandle, pending: Arc<PendingExitPlans>, session_id: String) -> Self {
        Self { app_handle, pending, session_id }
    }
}

#[async_trait]
impl Tool for ExitPlanModeTool {
    fn name(&self) -> &str { "exit_plan_mode" }
    fn description(&self) -> &str {
        "Submit your plan to the user for approval. The user will see a \
         confirmation modal and can accept (switching to Auto), accept but \
         stay in Plan mode (only the commands you list in allowed_prompts \
         will auto-pass), or reject with feedback."
    }
    fn parameters_schema(&self) -> serde_json::Value {
        serde_json::json!({
            "type": "object",
            "properties": {
                "plan": {
                    "type": "string",
                    "description": "Full plan in markdown format"
                },
                "allowed_prompts": {
                    "type": "array",
                    "items": { "type": "string" },
                    "description": "Optional list of specific commands (e.g. 'bash cargo build') that should auto-pass even if the user chooses to stay in Plan mode"
                }
            },
            "required": ["plan"]
        })
    }
    fn requires_approval(&self, _params: &serde_json::Value) -> ApprovalRequirement {
        ApprovalRequirement::Never
    }

    async fn execute(&self, params: serde_json::Value) -> Result<ToolOutput, ToolError> {
        let start = std::time::Instant::now();
        let plan = params.get("plan").and_then(|v| v.as_str())
            .ok_or_else(|| ToolError::InvalidParams("plan is required".into()))?
            .to_string();
        let allowed_prompts: Vec<String> = params.get("allowed_prompts")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|x| x.as_str().map(String::from)).collect())
            .unwrap_or_default();

        let request_id = uuid::Uuid::new_v4().to_string();
        let rx = self.pending.register(request_id.clone());

        // Clone allowed_prompts before moving into payload so we can include
        // them in the human-readable AcceptKeepPlan result text.
        let allowed_prompts_for_result = allowed_prompts.clone();
        let payload = ExitPlanRequestPayload {
            request_id: request_id.clone(),
            session_id: self.session_id.clone(),
            plan,
            allowed_prompts,
        };
        let _ = self.app_handle.emit("agent:exit_plan_request", &payload);

        let result = rx.await.map_err(|_| {
            ToolError::Execution("exit_plan_mode channel dropped — user closed without deciding".into())
        })?;

        match result.decision {
            crate::app::ExitPlanDecision::AcceptAndAuto => Ok(ToolOutput::success(
                "User accepted the plan and switched to Auto mode. Proceed with execution.",
                start.elapsed().as_millis() as u64,
            )),
            crate::app::ExitPlanDecision::AcceptKeepPlan => {
                let allowed_text = if allowed_prompts_for_result.is_empty() {
                    "(none declared)".to_string()
                } else {
                    allowed_prompts_for_result.join(", ")
                };
                Ok(ToolOutput::success(
                    &format!(
                        "User accepted the plan but kept Plan mode. Allowed commands: {}. Only those commands will auto-execute.",
                        allowed_text,
                    ),
                    start.elapsed().as_millis() as u64,
                ))
            }
            crate::app::ExitPlanDecision::Reject { feedback } => Err(ToolError::Execution(
                format!("User rejected the plan with feedback: \"{}\". Revise the plan and resubmit.", feedback),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{ExitPlanDecision, ExitPlanResult, PendingExitPlans};
    use std::sync::Arc;

    #[tokio::test]
    async fn pending_exit_plans_round_trip_accept_and_auto() {
        let pending = Arc::new(PendingExitPlans::new());
        let rx = pending.register("req-1".into());
        let resolved = pending.resolve("req-1", ExitPlanResult {
            decision: ExitPlanDecision::AcceptAndAuto,
        });
        assert!(resolved);
        let r = rx.await.unwrap();
        assert!(matches!(r.decision, ExitPlanDecision::AcceptAndAuto));
    }

    #[tokio::test]
    async fn pending_exit_plans_round_trip_reject_with_feedback() {
        let pending = Arc::new(PendingExitPlans::new());
        let rx = pending.register("req-2".into());
        pending.resolve("req-2", ExitPlanResult {
            decision: ExitPlanDecision::Reject { feedback: "missing test plan".into() },
        });
        let r = rx.await.unwrap();
        match r.decision {
            ExitPlanDecision::Reject { feedback } => assert_eq!(feedback, "missing test plan"),
            _ => panic!("expected Reject"),
        }
    }
}
