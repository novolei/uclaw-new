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
        in_memory_db_with_migrations_and_activity("spec-1", "aaaaaaaa-0000-0000-0000-000000000001")
    }

    fn in_memory_db_with_migrations_and_activity(
        spec_id: &str,
        activity_id: &str,
    ) -> Arc<std::sync::Mutex<rusqlite::Connection>> {
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
             VALUES (?1, 'test', '1.0.0', 'tester', 'test spec', 'You are test.', '', '{}', ?2, ?2)",
            rusqlite::params![spec_id, now],
        ).unwrap();
        // automation_activities: use UUID-shape id to match production.
        conn.execute(
            "INSERT INTO automation_activities \
             (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at) \
             VALUES (?1, ?2, 'manual', '{}', 'running', ?3)",
            rusqlite::params![activity_id, spec_id, now],
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

    // -----------------------------------------------------------------------
    // Integration test: end-to-end chokepoint wire-up
    //
    // This is the FIRST test that proves the production chokepoint path works
    // end-to-end (Slice 1b only tested individual pieces; production wiring
    // was None=dead-code throughout until this follow-up PR).
    //
    // Because build_automation_chokepoint is typed to tauri::Wry (production),
    // we build the components inline using tauri::test::MockRuntime — same
    // approach as agent::tool_dispatch tests. The SafetyManager is set to Ask
    // mode so an uncovered tool (bash with only Filesystem in PermissionSet)
    // reaches handle_ask → AutomationApprovalHandler → DB writes + Escalated.
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn chokepoint_wired_dispatch_escalates_uncovered_tool() {
        use crate::agent::tool_dispatch::{
            ApprovalOriginKind, ToolDispatchContext, ToolDispatcher,
        };
        use crate::agent::hook_bus::HookBus;
        use crate::automation::runtime::PermissionSet;
        use crate::automation::protocol::humane_v1::Permission;
        use crate::safety::{SafetyMode, SafetyManager};
        use crate::app::PendingApprovals;
        use uclaw_tool_types::ToolCall;
        use tauri::test::MockRuntime;

        let activity_id = "bbbbbbbb-0000-0000-0000-000000000002";
        let db = in_memory_db_with_migrations_and_activity("spec-int", activity_id);

        // Build the approval handler — same as production but with no app_handle
        // (Tauri event emission is skipped; DB writes still happen).
        let approval_handler: Arc<dyn crate::safety::ApprovalHandler> =
            Arc::new(AutomationApprovalHandler::new(db.clone(), None));

        // Wire SafetyManager in Ask mode so the bash tool (not in PermissionSet)
        // falls through → SafetyManager → RequireApproval → handle_ask.
        let mut mgr = SafetyManager::new(&std::env::temp_dir());
        let _ = mgr.set_global_mode(SafetyMode::Ask);
        let safety_manager = Arc::new(tokio::sync::RwLock::new(mgr));
        let pending_approvals = Arc::new(PendingApprovals::new());
        let hook_bus = Arc::new(HookBus::new());

        // Register a "bash" stub with ApprovalRequirement::Always so the tool is
        // found in the registry and reaches the safety gate (not-found exits early).
        // We use a minimal inline stub rather than importing AlwaysApprovalTool from
        // agent::tool_dispatch::tests (which is a cfg(test)-only pub(crate) module).
        use crate::agent::tools::tool::{Tool, ToolOutput, ToolError, ToolRegistry, ApprovalRequirement};

        struct BashStub;
        #[async_trait::async_trait]
        impl Tool for BashStub {
            fn name(&self) -> &str { "bash" }
            fn description(&self) -> &str { "stub" }
            fn parameters_schema(&self) -> serde_json::Value { serde_json::json!({}) }
            fn requires_approval(&self, _: &serde_json::Value) -> ApprovalRequirement {
                ApprovalRequirement::Always
            }
            async fn execute(&self, _: serde_json::Value) -> Result<ToolOutput, ToolError> {
                // Should never be reached — escalation fires before execution.
                panic!("BashStub::execute must not be called during escalation test")
            }
        }

        let mut reg = ToolRegistry::new();
        reg.register(BashStub);

        let app = tauri::test::mock_app();
        let dispatcher = Arc::new(ToolDispatcher::<MockRuntime>::new_with_approval_handler(
            Arc::new(reg),
            app.handle().clone(),
            safety_manager,
            approval_handler,
            pending_approvals,
            None,  // infra_service
            None,  // trajectory_store
            None,  // tool_budget
            hook_bus,
            None,  // heartbeat
        ));

        // PermissionSet: only Filesystem granted — bash (Shell) is NOT covered,
        // so the dispatcher falls through to SafetyManager Ask → RequireApproval
        // → handle_ask → AutomationApprovalHandler → Escalated outcome.
        let perms = PermissionSet {
            spec: vec![Permission::Filesystem],
            granted: vec![],
            denied: vec![],
        };
        let ctx = ToolDispatchContext {
            session_id: "sess-int".to_string(),
            conversation_id: activity_id.to_string(),
            workspace_root: None,
            attached_dirs: vec![],
            safety_mode: Some(SafetyMode::Ask),
            iteration: 1,
            cancel: None,
            permissions: Some(perms),
            origin_kind: ApprovalOriginKind::Automation {
                activity_id: activity_id.to_string(),
            },
        };

        let tool_call = ToolCall {
            id: "tc-001".to_string(),
            name: "bash".to_string(),
            arguments: serde_json::json!({ "command": "ls -la" }),
        };

        let outcomes = dispatcher.dispatch(vec![tool_call], &ctx).await;

        // ── Assert outcome shape ─────────────────────────────────────────────
        assert_eq!(outcomes.len(), 1, "expected one outcome");
        let o = &outcomes[0];
        assert!(!o.rejected, "escalation should NOT set rejected");
        assert!(o.is_error, "escalation IS an error");
        assert_eq!(
            o.message_content, "Error: awaiting user approval",
            "message_content must match the literal used in HeadlessDelegate detection"
        );

        // ── Assert DB rows ───────────────────────────────────────────────────
        let conn = db.lock().unwrap();

        let (req_tool, req_status): (String, String) = conn.query_row(
            "SELECT tool_name, status FROM automation_approval_requests \
             WHERE activity_id = ?1",
            rusqlite::params![activity_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).expect("automation_approval_requests row must exist after escalation");
        assert_eq!(req_tool, "bash");
        assert_eq!(req_status, "pending");

        let (act_status, pending_req_id): (String, Option<i64>) = conn.query_row(
            "SELECT status, pending_approval_request_id \
             FROM automation_activities WHERE id = ?1",
            rusqlite::params![activity_id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).expect("automation_activities row must exist");
        assert_eq!(act_status, "paused_pending_approval");
        assert!(
            pending_req_id.is_some(),
            "pending_approval_request_id must be set on the activity"
        );
    }
}
