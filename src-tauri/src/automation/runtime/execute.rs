use crate::agent::types::{
    ChatMessage, LoopOutcome, LoopSignal, RespondOutput, ReasoningContext, ResponseMetadata,
    TextAction, ToolCall,
};
use crate::automation::memory::MemoryStore;
use crate::automation::permissions;
use crate::automation::runtime::{AutoContinueConfig, CompletionGate, PermissionSet};
use crate::automation::tools::{
    memory::MemoryInput,
    notify_user::NotifyInput,
    report_to_user::ReportInput,
    request_escalation::RequestEscalationInput,
};
use crate::error::Error;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::Mutex;

/// LoopDelegate for automation runs.
///
/// Handles the four Humane-specific tools (report_to_user, notify_user,
/// request_escalation, memory) and stubs out fall-through for everything
/// else. call_llm is not implemented here — AppRuntimeService (Task 18)
/// injects the provider call.
pub struct AutomationDelegate {
    pub spec_id: String,
    pub activity_id: String,
    pub permissions: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Holds the terminal state once the run completes (report or escalation).
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
    pub auto_continue: AutoContinueConfig,
}

#[async_trait]
impl crate::agent::types::LoopDelegate for AutomationDelegate {
    async fn check_signals(&self) -> LoopSignal {
        LoopSignal::Continue
    }

    async fn before_llm_call(
        &self,
        _reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Option<LoopOutcome> {
        None
    }

    async fn call_llm(
        &self,
        _reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        // TODO(humane-phase-1-task-18): AppRuntimeService injects the real provider
        // call here. Until then this method is unreachable in production —
        // execute_tool_calls is exercised in unit tests by calling it directly.
        Err(Error::Internal("call_llm is wired by AppRuntimeService (Task 18)".into()))
    }

    async fn handle_text_response(
        &self,
        _text: &str,
        _metadata: ResponseMetadata,
        _reason_ctx: &mut ReasoningContext,
    ) -> TextAction {
        TextAction::Continue
    }

    async fn execute_tool_calls(
        &self,
        tool_calls: Vec<ToolCall>,
        reason_ctx: &mut ReasoningContext,
    ) -> Result<Option<LoopOutcome>, Error> {
        for call in tool_calls {
            // Permission gate: deny-list beats grant-list beats spec default.
            if let Err(e) = permissions::check(
                &self.permissions.spec,
                &self.permissions.granted,
                &self.permissions.denied,
                &call.name,
            ) {
                reason_ctx.messages.push(ChatMessage::user_tool_result(
                    &call.id,
                    &format!("permission error: {}", e),
                    true,
                ));
                continue;
            }

            match call.name.as_str() {
                "report_to_user" => {
                    let input: ReportInput = serde_json::from_value(call.arguments.clone())?;
                    // Persist completion to the activity row.
                    {
                        let conn = self.db.lock().unwrap();
                        conn.execute(
                            "UPDATE automation_activities \
                             SET status='completed', report_text=?1, report_outcome=?2, \
                                 completed_at=?3 \
                             WHERE id=?4",
                            rusqlite::params![
                                input.text,
                                input.outcome,
                                chrono::Utc::now().timestamp_millis(),
                                self.activity_id,
                            ],
                        )?;
                    }
                    *self.gate.lock().await = Some(CompletionGate::Reported {
                        text: input.text.clone(),
                        outcome: input.outcome.clone(),
                    });
                    // TODO(humane-phase-2): emit InfraEvent::AutomationRunReported once
                    // the InfraEventType enum is extended and bus subscribers designed.
                    tracing::info!(
                        spec_id = %self.spec_id,
                        activity_id = %self.activity_id,
                        outcome = %input.outcome,
                        "automation run reported"
                    );
                    return Ok(Some(LoopOutcome::Response {
                        text: input.text,
                        usage: None,
                    }));
                }

                "request_escalation" => {
                    let input: RequestEscalationInput =
                        serde_json::from_value(call.arguments.clone())?;
                    // Serialize choices by pulling the raw JSON array from the
                    // original arguments — avoids requiring Serialize on EscalationChoice.
                    let choices_json = call
                        .arguments
                        .get("choices")
                        .and_then(|v| serde_json::to_string(v).ok())
                        .unwrap_or_else(|| "[]".into());
                    let escalation_id = uuid::Uuid::new_v4().to_string();
                    {
                        let conn = self.db.lock().unwrap();
                        conn.execute(
                            "INSERT INTO automation_escalations \
                             (id, spec_id, activity_id, question, choices_json, status, created_at) \
                             VALUES (?1, ?2, ?3, ?4, ?5, 'waiting', ?6)",
                            rusqlite::params![
                                escalation_id,
                                self.spec_id,
                                self.activity_id,
                                input.question,
                                choices_json,
                                chrono::Utc::now().timestamp_millis(),
                            ],
                        )?;
                    }
                    *self.gate.lock().await = Some(CompletionGate::Escalated {
                        escalation_id: escalation_id.clone(),
                    });
                    // TODO(humane-phase-2): emit InfraEvent::AutomationRunEscalated
                    tracing::info!(
                        spec_id = %self.spec_id,
                        activity_id = %self.activity_id,
                        escalation_id = %escalation_id,
                        "automation escalation requested"
                    );
                    return Ok(Some(LoopOutcome::Response {
                        text: "escalated".into(),
                        usage: None,
                    }));
                }

                "memory" => {
                    let input: MemoryInput = serde_json::from_value(call.arguments.clone())?;
                    let result = match input.op.as_str() {
                        "read" => self.memory.read(&self.spec_id).await?,
                        "write" => {
                            let c = input.content.as_deref().unwrap_or("");
                            self.memory.write(&self.spec_id, c).await?;
                            "ok".into()
                        }
                        "append" => {
                            let c = input.content.as_deref().unwrap_or("");
                            self.memory.append(&self.spec_id, c).await?;
                            "ok".into()
                        }
                        "compact" => {
                            let p = self.memory.compact(&self.spec_id).await?;
                            p.to_string_lossy().into_owned()
                        }
                        _ => "unknown memory op".into(),
                    };
                    reason_ctx
                        .messages
                        .push(ChatMessage::user_tool_result(&call.id, &result, false));
                }

                "notify_user" => {
                    let input: NotifyInput = serde_json::from_value(call.arguments.clone())?;
                    // Phase 1: log only.
                    // TODO(humane-phase-2): wire Tauri notification + WeCom channel dispatch.
                    tracing::info!(
                        title = %input.title,
                        body  = %input.body,
                        level = %input.level,
                        "automation notify_user (channels not wired in Phase 1)"
                    );
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id,
                        "notified (phase 1: log only)",
                        false,
                    ));
                }

                other => {
                    // Phase 1 fall-through stub.
                    // TODO(humane-phase-2): dispatch to base built-in tools via dispatcher.
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id,
                        &format!(
                            "tool '{}' fall-through not implemented in Phase 1",
                            other
                        ),
                        true,
                    ));
                }
            }
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::LoopDelegate;
    use crate::automation::protocol::humane_v1::Permission;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Stand up an in-memory rusqlite DB with V20+V21 schemas applied, insert
    /// minimal spec + activity rows so FK constraints don't fire.
    fn setup_db(spec_id: &str, activity_id: &str) -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO automation_specs \
             (id, name, version, author, description, system_prompt, spec_yaml, spec_json, created_at, updated_at) \
             VALUES (?1, 'test', '1.0.0', 'tester', 'test spec', 'You are test.', '', '{}', ?2, ?2)",
            rusqlite::params![spec_id, now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO automation_activities \
             (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at) \
             VALUES (?1, ?2, 'manual', '{}', 'running', ?3)",
            rusqlite::params![activity_id, spec_id, now],
        )
        .unwrap();
        conn
    }

    fn make_delegate(
        spec_id: &str,
        activity_id: &str,
        tmp: &TempDir,
        conn: rusqlite::Connection,
        perms: PermissionSet,
    ) -> AutomationDelegate {
        AutomationDelegate {
            spec_id: spec_id.to_string(),
            activity_id: activity_id.to_string(),
            permissions: perms,
            memory: Arc::new(MemoryStore::new(tmp.path().to_path_buf())),
            db: Arc::new(std::sync::Mutex::new(conn)),
            gate: Arc::new(Mutex::new(None)),
            auto_continue: AutoContinueConfig::default(),
        }
    }

    fn make_tool_call(name: &str, args: serde_json::Value) -> ToolCall {
        ToolCall {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            arguments: args,
        }
    }

    // -----------------------------------------------------------------------
    // Test 1: report_to_user sets gate to Reported and returns LoopOutcome::Response
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn report_to_user_sets_gate_and_returns_response() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-1", "act-1");
        let delegate = make_delegate("spec-1", "act-1", &tmp, conn, PermissionSet::default());
        let mut ctx = ReasoningContext::new("sys".into());

        let calls = vec![make_tool_call(
            "report_to_user",
            json!({ "text": "All done.", "outcome": "useful" }),
        )];
        let outcome = delegate.execute_tool_calls(calls, &mut ctx).await.unwrap();

        // Gate should be set to Reported.
        let gate = delegate.gate.lock().await;
        assert_eq!(
            *gate,
            Some(CompletionGate::Reported {
                text: "All done.".into(),
                outcome: "useful".into(),
            })
        );

        // Should return Some(LoopOutcome::Response).
        assert!(matches!(
            outcome,
            Some(LoopOutcome::Response { text, .. }) if text == "All done."
        ));

        // DB should be updated.
        let conn2 = delegate.db.lock().unwrap();
        let status: String = conn2
            .query_row(
                "SELECT status FROM automation_activities WHERE id='act-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(status, "completed");
    }

    // -----------------------------------------------------------------------
    // Test 2: memory write + read round-trip
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn memory_write_read_round_trip() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-2", "act-2");
        let delegate = make_delegate("spec-2", "act-2", &tmp, conn, PermissionSet::default());
        let mut ctx = ReasoningContext::new("sys".into());

        // Write.
        let write_call = make_tool_call(
            "memory",
            json!({ "op": "write", "content": "hello memory" }),
        );
        let outcome = delegate
            .execute_tool_calls(vec![write_call], &mut ctx)
            .await
            .unwrap();
        assert!(outcome.is_none());
        // Should have pushed a tool_result message.
        assert!(!ctx.messages.is_empty());
        // The result content should be "ok".
        let last = ctx.messages.last().unwrap();
        let content_str = format!("{:?}", last);
        assert!(content_str.contains("ok"));

        // Read back using a fresh context.
        let mut ctx2 = ReasoningContext::new("sys".into());
        let read_call = make_tool_call("memory", json!({ "op": "read" }));
        let outcome2 = delegate
            .execute_tool_calls(vec![read_call], &mut ctx2)
            .await
            .unwrap();
        assert!(outcome2.is_none());
        let last2 = ctx2.messages.last().unwrap();
        let content_str2 = format!("{:?}", last2);
        assert!(content_str2.contains("hello memory"));
    }

    // -----------------------------------------------------------------------
    // Test 3: request_escalation inserts row and sets gate to Escalated
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn request_escalation_inserts_row_and_sets_gate() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-3", "act-3");
        let delegate = make_delegate("spec-3", "act-3", &tmp, conn, PermissionSet::default());
        let mut ctx = ReasoningContext::new("sys".into());

        let calls = vec![make_tool_call(
            "request_escalation",
            json!({
                "question": "Which branch?",
                "choices": [
                    {"id": "a", "label": "Alpha"},
                    {"id": "b", "label": "Beta"}
                ]
            }),
        )];
        let outcome = delegate.execute_tool_calls(calls, &mut ctx).await.unwrap();

        // Gate should be Escalated.
        let gate = delegate.gate.lock().await;
        assert!(matches!(*gate, Some(CompletionGate::Escalated { .. })));

        // Outcome should be Response { text: "escalated" }.
        assert!(matches!(
            outcome,
            Some(LoopOutcome::Response { text, .. }) if text == "escalated"
        ));

        // Extract the escalation_id from the gate.
        let escalation_id = match gate.as_ref().unwrap() {
            CompletionGate::Escalated { escalation_id } => escalation_id.clone(),
            _ => panic!("unexpected gate"),
        };
        drop(gate);

        // DB row should exist.
        let conn2 = delegate.db.lock().unwrap();
        let count: i64 = conn2
            .query_row(
                "SELECT COUNT(*) FROM automation_escalations WHERE id=?1",
                rusqlite::params![escalation_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify question was saved.
        let q: String = conn2
            .query_row(
                "SELECT question FROM automation_escalations WHERE id=?1",
                rusqlite::params![escalation_id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(q, "Which branch?");
    }

    // -----------------------------------------------------------------------
    // Test 4: permission denial pushes error tool_result and continues loop
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn permission_denial_pushes_error_result_and_continues() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-4", "act-4");

        // Deny the Notification permission explicitly.
        let perms = PermissionSet {
            spec: vec![],
            granted: vec![],
            denied: vec![Permission::Notification],
        };
        let delegate = make_delegate("spec-4", "act-4", &tmp, conn, perms);
        let mut ctx = ReasoningContext::new("sys".into());

        let calls = vec![make_tool_call(
            "notify_user",
            json!({
                "channels": ["system"],
                "title": "Hey",
                "body": "Hello",
                "level": "info"
            }),
        )];
        let outcome = delegate.execute_tool_calls(calls, &mut ctx).await.unwrap();

        // Should NOT terminate — permission error just pushes a result.
        assert!(outcome.is_none());

        // Gate should remain unset.
        let gate = delegate.gate.lock().await;
        assert!(gate.is_none());
        drop(gate);

        // A tool_result message should have been pushed with is_error=true.
        assert!(!ctx.messages.is_empty());
        let last = ctx.messages.last().unwrap();
        let content_str = format!("{:?}", last);
        assert!(content_str.contains("permission error"));
    }
}
