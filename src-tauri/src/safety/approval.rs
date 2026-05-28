//! ApprovalHandler — origin-specific async behavior for `should_approve = RequireApproval`.
//!
//! `ChatApprovalHandler` (here) wraps the existing `PendingApprovals` IPC for
//! chat AND browser sub-loop (user is in the chat session either way).
//! `AutomationApprovalHandler` (in `automation/runtime/approval.rs`) escalates
//! via DB and returns `Escalated` so the run pauses for asynchronous resolution.

use std::sync::Arc;
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub enum ApprovalOrigin {
    Chat { conversation_id: String },
    Automation { activity_id: String },
    BrowserSubLoop { conversation_id: String, browser_task_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalOutcome {
    Approved,
    Denied,
    Escalated,
}

#[async_trait]
pub trait ApprovalHandler: Send + Sync {
    async fn handle_ask(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome;
}

pub struct ChatApprovalHandler {
    pending_approvals: Arc<crate::app::PendingApprovals>,
}

impl ChatApprovalHandler {
    pub fn new(pending_approvals: Arc<crate::app::PendingApprovals>) -> Self {
        Self { pending_approvals }
    }
}

#[async_trait]
impl ApprovalHandler for ChatApprovalHandler {
    async fn handle_ask(
        &self,
        _tool_name: &str,
        _arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome {
        let key = match origin {
            ApprovalOrigin::Chat { conversation_id } => format!("chat:{conversation_id}"),
            ApprovalOrigin::BrowserSubLoop { conversation_id, browser_task_id } => {
                format!("browser-sub:{conversation_id}:{browser_task_id}")
            }
            ApprovalOrigin::Automation { .. } => {
                return ApprovalOutcome::Denied;
            }
        };
        let rx = self.pending_approvals.register(key);
        match rx.await {
            Ok(result) if result.approved => ApprovalOutcome::Approved,
            Ok(_) => ApprovalOutcome::Denied,
            Err(_) => ApprovalOutcome::Denied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn chat_handler_returns_approved_when_pending_approvals_resolves_true() {
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa.clone());

        let origin = ApprovalOrigin::Chat { conversation_id: "c1".into() };
        let handler_task = tokio::spawn({
            let handler = Arc::new(handler);
            async move { handler.handle_ask("bash", &serde_json::json!({}), &origin).await }
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        let resolved = pa.resolve("chat:c1", crate::app::ApprovalResult {
            approved: true,
            always_allow: false,
            tool_name: None,
            path_scope: None,
            paths: None,
        });
        assert!(resolved);

        let outcome = handler_task.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Approved);
    }

    #[tokio::test]
    async fn chat_handler_returns_denied_when_pending_approvals_resolves_false() {
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa.clone());

        let origin = ApprovalOrigin::Chat { conversation_id: "c2".into() };
        let handler_task = tokio::spawn({
            let handler = Arc::new(handler);
            async move { handler.handle_ask("bash", &serde_json::json!({}), &origin).await }
        });

        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        pa.resolve("chat:c2", crate::app::ApprovalResult {
            approved: false,
            always_allow: false,
            tool_name: None,
            path_scope: None,
            paths: None,
        });

        let outcome = handler_task.await.unwrap();
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }

    #[tokio::test]
    async fn chat_handler_returns_denied_for_automation_origin() {
        let pa = Arc::new(crate::app::PendingApprovals::new());
        let handler = ChatApprovalHandler::new(pa);
        let origin = ApprovalOrigin::Automation { activity_id: "act-1".into() };
        let outcome = handler.handle_ask("bash", &serde_json::json!({}), &origin).await;
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }
}
