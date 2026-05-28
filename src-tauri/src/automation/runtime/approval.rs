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

        // activity_id is a UUID string (production) or arbitrary test id; do NOT parse.
        let arguments_json = arguments.to_string();
        let tool_name_owned = tool_name.to_string();
        let activity_id_owned: String = activity_id.clone();

        let request_id: Option<i64> = match self.db.lock() {
            Ok(conn) => {
                match conn.execute(
                    "INSERT INTO automation_approval_requests \
                     (activity_id, tool_name, arguments_json, status) \
                     VALUES (?1, ?2, ?3, 'pending')",
                    rusqlite::params![activity_id_owned, tool_name_owned, arguments_json],
                ) {
                    Ok(_) => {
                        let id = conn.last_insert_rowid();
                        if let Err(e) = conn.execute(
                            "UPDATE automation_activities \
                             SET status = 'paused_pending_approval', \
                                 pending_approval_request_id = ?1 \
                             WHERE id = ?2",
                            rusqlite::params![id, activity_id_owned],
                        ) {
                            tracing::error!(
                                error = %e, activity_id = %activity_id_owned, request_id = id,
                                "[Slice 1b] failed to update automation_activities status after \
                                 inserting approval request — request row exists but activity \
                                 is not paused; user-resolve flow may not find it"
                            );
                        }
                        Some(id)
                    }
                    Err(e) => {
                        tracing::error!(
                            error = %e, activity_id = %activity_id_owned, tool = %tool_name_owned,
                            "[Slice 1b] failed to INSERT automation_approval_request — \
                             escalation will be reported to the loop but no DB record exists. \
                             Returning Denied so the loop doesn't orphan."
                        );
                        None
                    }
                }
            }
            Err(poison) => {
                tracing::error!(
                    error = %poison, activity_id = %activity_id_owned,
                    "[Slice 1b] db mutex poisoned during approval escalation — returning Denied"
                );
                None
            }
        };

        if let (Some(app), Some(req_id)) = (self.app_handle.as_ref(), request_id) {
            use tauri::Emitter;
            let _ = app.emit("automation:approval-needed", serde_json::json!({
                "activity_id": activity_id_owned,
                "request_id": req_id,
                "tool_name": tool_name_owned,
            }));
        }

        // If the DB write failed, return Denied so the loop doesn't pause an
        // activity that has no resolvable record. The model gets a "denied"
        // tool result and can recover or finish.
        match request_id {
            Some(_) => ApprovalOutcome::Escalated,
            None => ApprovalOutcome::Denied,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn in_memory_db_with_migrations() -> Arc<std::sync::Mutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run_migrations_up_to(&conn, 56).unwrap();
        // Enable FK enforcement so type mismatches (e.g. INTEGER vs TEXT) are caught in tests.
        conn.execute("PRAGMA foreign_keys = ON", []).unwrap();
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
        // automation_activities: use UUID-shape id to match production.
        conn.execute(
            "INSERT INTO automation_activities \
             (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at) \
             VALUES ('aaaaaaaa-0000-0000-0000-000000000001', 'spec-1', 'manual', '{}', 'running', ?1)",
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
            &ApprovalOrigin::Automation { activity_id: "aaaaaaaa-0000-0000-0000-000000000001".into() },
        ).await;
        assert_eq!(outcome, ApprovalOutcome::Escalated);

        let conn = db.lock().unwrap();
        let (tool, status): (String, String) = conn.query_row(
            "SELECT tool_name, status FROM automation_approval_requests \
             WHERE activity_id = 'aaaaaaaa-0000-0000-0000-000000000001'",
            [], |r| Ok((r.get(0)?, r.get(1)?))
        ).unwrap();
        assert_eq!(tool, "bash");
        assert_eq!(status, "pending");

        let activity_status: String = conn.query_row(
            "SELECT status FROM automation_activities WHERE id = 'aaaaaaaa-0000-0000-0000-000000000001'",
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
