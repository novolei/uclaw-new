use crate::app::AskUserResult;
use crate::app::PendingAskUsers;
use crate::ipc::{AskUserOption, AskUserQuestion, AskUserRequestPayload};
use anyhow::{anyhow, Result};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub const BROWSER_INTERVENTION_QUESTION: &str = "How should the browser task proceed?";

#[derive(Debug, Clone)]
pub struct BrowserInterventionPrompt {
    pub question: AskUserQuestion,
}

impl BrowserInterventionPrompt {
    pub fn human_boundary(run_id: &str, reason: &str) -> Self {
        Self {
            question: AskUserQuestion {
                question: BROWSER_INTERVENTION_QUESTION.to_string(),
                header: Some("Browser needs you".to_string()),
                multi_select: false,
                options: vec![
                    AskUserOption {
                        label: "I handled it, continue".to_string(),
                        description: Some(format!(
                            "Resume browser task {run_id} after the manual step: {reason}"
                        )),
                        preview: None,
                    },
                    AskUserOption {
                        label: "Stop task".to_string(),
                        description: Some(
                            "Leave the task paused for manual follow-up.".to_string(),
                        ),
                        preview: None,
                    },
                ],
            },
        }
    }

    pub fn checkpoint(run_id: &str) -> Self {
        Self {
            question: AskUserQuestion {
                question: BROWSER_INTERVENTION_QUESTION.to_string(),
                header: Some("Browser checkpoint saved".to_string()),
                multi_select: false,
                options: vec![
                    AskUserOption {
                        label: "Continue 8 steps".to_string(),
                        description: Some(format!(
                            "Resume browser task {run_id} for another short segment."
                        )),
                        preview: None,
                    },
                    AskUserOption {
                        label: "Continue 25 steps".to_string(),
                        description: Some(format!(
                            "Resume browser task {run_id} for a longer segment."
                        )),
                        preview: None,
                    },
                    AskUserOption {
                        label: "Stop task".to_string(),
                        description: Some(
                            "Keep the checkpoint without continuing now.".to_string(),
                        ),
                        preview: None,
                    },
                ],
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserInterventionDecision {
    Continue,
    ContinueWithSteps(u32),
    Stop,
}

impl BrowserInterventionDecision {
    pub fn from_result(result: &AskUserResult) -> Self {
        let Some(answer) = result.answers.get(BROWSER_INTERVENTION_QUESTION) else {
            return Self::Stop;
        };
        let answer = answer
            .as_str()
            .or_else(|| {
                answer
                    .as_array()
                    .and_then(|arr| arr.first())
                    .and_then(|v| v.as_str())
            })
            .unwrap_or_default();
        match answer {
            "I handled it, continue" => Self::Continue,
            "Continue 8 steps" => Self::ContinueWithSteps(8),
            "Continue 25 steps" => Self::ContinueWithSteps(25),
            _ => Self::Stop,
        }
    }
}

#[derive(Clone)]
pub struct BrowserAskUserBridge {
    app_handle: AppHandle,
    pending: Arc<PendingAskUsers>,
    session_id: String,
}

impl BrowserAskUserBridge {
    pub fn new(app_handle: AppHandle, pending: Arc<PendingAskUsers>, session_id: String) -> Self {
        Self {
            app_handle,
            pending,
            session_id,
        }
    }

    pub async fn ask(
        &self,
        prompt: BrowserInterventionPrompt,
    ) -> Result<BrowserInterventionDecision> {
        let request_id = uuid::Uuid::new_v4().to_string();
        let rx = self.pending.register(request_id.clone());
        let payload = AskUserRequestPayload {
            request_id,
            session_id: self.session_id.clone(),
            questions: vec![prompt.question],
        };

        self.app_handle
            .emit("agent:ask_user_request", &payload)
            .map_err(|e| anyhow!("failed to emit browser ask_user request: {e}"))?;

        let result = rx
            .await
            .map_err(|_| anyhow!("browser ask_user response channel closed"))?;
        Ok(BrowserInterventionDecision::from_result(&result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_human_intervention_question_for_login_boundary() {
        let prompt =
            BrowserInterventionPrompt::human_boundary("run-1", "Login required before continuing");
        assert_eq!(prompt.question.header.as_deref(), Some("Browser needs you"));
        assert_eq!(prompt.question.options[0].label, "I handled it, continue");
        assert_eq!(prompt.question.options[1].label, "Stop task");
    }

    #[test]
    fn parses_continue_answer_from_ask_user_result() {
        let mut answers = std::collections::HashMap::new();
        answers.insert(
            BROWSER_INTERVENTION_QUESTION.to_string(),
            serde_json::Value::String("I handled it, continue".to_string()),
        );
        let result = crate::app::AskUserResult { answers };
        assert_eq!(
            BrowserInterventionDecision::from_result(&result),
            BrowserInterventionDecision::Continue
        );
    }

    #[test]
    fn parses_checkpoint_step_count_from_ask_user_result() {
        let mut answers = std::collections::HashMap::new();
        answers.insert(
            BROWSER_INTERVENTION_QUESTION.to_string(),
            serde_json::Value::String("Continue 25 steps".to_string()),
        );
        let result = crate::app::AskUserResult { answers };
        assert_eq!(
            BrowserInterventionDecision::from_result(&result),
            BrowserInterventionDecision::ContinueWithSteps(25)
        );
    }
}
