//! Slice 1b — AutomationApprovalHandler.
//!
//! Routes `ApprovalDecision::RequireApproval` for automation origins to a
//! pause-and-resolve flow: writes the request to `automation_approval_requests`,
//! transitions the activity to `paused_pending_approval`, emits a Tauri event,
//! and returns `Escalated` so the dispatcher pauses the loop.

// SPDX-License-Identifier: UNLICENSED

use std::sync::Arc;
use async_trait::async_trait;
use crate::safety::{ApprovalHandler, ApprovalOrigin, ApprovalOutcome};

pub struct AutomationApprovalHandler {
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Optional Tauri app handle for emitting `automation:approval-needed`.
    /// None in tests (DB writes still happen; event is skipped).
    app_handle: Option<tauri::AppHandle>,
}

impl AutomationApprovalHandler {
    pub fn new(
        db: Arc<std::sync::Mutex<rusqlite::Connection>>,
        app_handle: Option<tauri::AppHandle>,
    ) -> Self {
        Self { db, app_handle }
    }
}

#[async_trait]
impl ApprovalHandler for AutomationApprovalHandler {
    async fn handle_ask(
        &self,
        tool_name: &str,
        arguments: &serde_json::Value,
        origin: &ApprovalOrigin,
    ) -> ApprovalOutcome {
        let ApprovalOrigin::Automation { activity_id } = origin else {
            tracing::error!(?origin, "AutomationApprovalHandler called for non-Automation origin");
            return ApprovalOutcome::Denied;
        };

        let activity_id_int: i64 = activity_id.parse().unwrap_or(0);
        let arguments_json = arguments.to_string();
        let tool_name_owned = tool_name.to_string();

        let request_id: Option<i64> = {
            let conn = self.db.lock().ok();
            conn.and_then(|conn| {
                conn.execute(
                    "INSERT INTO automation_approval_requests \
                     (activity_id, tool_name, arguments_json, status) \
                     VALUES (?1, ?2, ?3, 'pending')",
                    rusqlite::params![activity_id_int, tool_name_owned, arguments_json],
                ).ok()?;
                let id = conn.last_insert_rowid();
                conn.execute(
                    "UPDATE automation_activities \
                     SET status = 'paused_pending_approval', \
                         pending_approval_request_id = ?1 \
                     WHERE id = ?2",
                    rusqlite::params![id, activity_id_int],
                ).ok()?;
                Some(id)
            })
        };

        if let (Some(app), Some(req_id)) = (self.app_handle.as_ref(), request_id) {
            use tauri::Emitter;
            let _ = app.emit("automation:approval-needed", serde_json::json!({
                "activity_id": activity_id_int,
                "request_id": req_id,
                "tool_name": tool_name_owned,
            }));
        }

        ApprovalOutcome::Escalated
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db_with_migrations() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 56).unwrap();
        // Seed minimal spec + activity rows so FK constraints don't fire.
        // automation_specs requires: id, name, version, author, description,
        // system_prompt, spec_yaml, spec_json, created_at, updated_at (NOT NULL).
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO automation_specs \
             (id, name, version, author, description, system_prompt, spec_yaml, spec_json, created_at, updated_at) \
             VALUES ('spec-1', 'test', '1.0.0', 'tester', 'test spec', 'You are test.', '', '{}', ?1, ?1)",
            rusqlite::params![now],
        ).unwrap();
        // automation_activities requires: id, spec_id, queued_at (NOT NULL + FK).
        conn.execute(
            "INSERT INTO automation_activities \
             (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at) \
             VALUES ('1', 'spec-1', 'manual', '{}', 'running', ?1)",
            rusqlite::params![now],
        ).unwrap();
        Arc::new(std::sync::Mutex::new(conn))
    }

    #[tokio::test]
    async fn handle_ask_writes_request_and_transitions_activity() {
        let db = in_memory_db_with_migrations();
        let handler = AutomationApprovalHandler::new(db.clone(), None);
        let outcome = handler.handle_ask(
            "bash",
            &serde_json::json!({"command": "ls"}),
            &ApprovalOrigin::Automation { activity_id: "1".into() },
        ).await;
        assert_eq!(outcome, ApprovalOutcome::Escalated);

        let conn = db.lock().unwrap();
        let (tool, status): (String, String) = conn.query_row(
            "SELECT tool_name, status FROM automation_approval_requests WHERE activity_id = 1",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(tool, "bash");
        assert_eq!(status, "pending");

        let activity_status: String = conn.query_row(
            "SELECT status FROM automation_activities WHERE id = '1'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(activity_status, "paused_pending_approval");
    }

    #[tokio::test]
    async fn handle_ask_denies_for_non_automation_origin() {
        let db = in_memory_db_with_migrations();
        let handler = AutomationApprovalHandler::new(db, None);
        let outcome = handler.handle_ask(
            "bash",
            &serde_json::json!({}),
            &ApprovalOrigin::Chat { conversation_id: "c1".into() },
        ).await;
        assert_eq!(outcome, ApprovalOutcome::Denied);
    }
}
