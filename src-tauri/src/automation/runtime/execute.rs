pub use crate::agent::headless::HeadlessDelegate;

/// Backward-compatible alias — callers migrate to `HeadlessDelegate` directly;
/// this shim will be removed once all call sites are updated.
pub type AutomationDelegate = HeadlessDelegate;


#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{LoopDelegate, ContentBlock, ReasoningContext, ToolCall, LoopOutcome};
    use crate::automation::memory::MemoryStore;
    use crate::automation::protocol::humane_v1::Permission;
    use crate::automation::runtime::{AutoContinueConfig, CompletionGate, PermissionSet};
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;
    use tokio::sync::Mutex;

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
        use crate::automation::runtime::cost::{CostCapConfig, CostCapState};
        AutomationDelegate {
            spec_id: spec_id.to_string(),
            activity_id: activity_id.to_string(),
            session_id: format!("sess-{}", activity_id),
            permissions: perms,
            memory: Arc::new(MemoryStore::new(tmp.path().to_path_buf())),
            db: Arc::new(std::sync::Mutex::new(conn)),
            gate: Arc::new(Mutex::new(None)),
            auto_continue: AutoContinueConfig::default(),
            llm: test_support::fake_llm(),
            model: "claude-sonnet-4-6".to_string(),
            tools: test_support::empty_tool_registry(),
            cost: Arc::new(CostCapState::new(CostCapConfig {
                per_run_usd: 1.00,
                per_day_usd: 10.00,
            })),
            workspace_root: tmp.path().to_path_buf(),
            app_handle: None,
            channel_manager: None,
            reply_handle: None,
            streaming_handle: None,
            system_prompt_override: None,
            safety_manager: None,
            tool_dispatcher: None,
            approval_handler: None,
        }
    }

    mod test_support {
        use std::sync::Arc;
        use async_trait::async_trait;
        use crate::agent::tools::tool::ToolRegistry;
        use crate::agent::types::{ChatMessage, RespondOutput, StreamDelta, ToolDefinition};
        use crate::llm::{CompletionConfig, LlmProvider};
        use crate::error::Error;

        /// Minimal LlmProvider stub for tests that exercise execute_tool_calls
        /// (which never calls call_llm). Only `stream` needs a real body;
        /// `complete` is stubbed with unimplemented!() because none of the 4
        /// existing execute.rs tests ever reach call_llm.
        struct NoopLlm;

        #[async_trait]
        impl LlmProvider for NoopLlm {
            async fn complete(
                &self,
                _messages: Vec<ChatMessage>,
                _tools: Vec<ToolDefinition>,
                _config: &CompletionConfig,
            ) -> Result<RespondOutput, Error> {
                unimplemented!("NoopLlm::complete not called by execute.rs tests")
            }

            async fn stream(
                &self,
                _messages: Vec<ChatMessage>,
                _tools: Vec<ToolDefinition>,
                _config: &CompletionConfig,
            ) -> Result<Box<dyn futures::Stream<Item = Result<StreamDelta, Error>> + Send + Unpin>, Error> {
                Ok(Box::new(futures::stream::iter(Vec::<Result<StreamDelta, Error>>::new())))
            }
        }

        pub fn fake_llm() -> Arc<dyn LlmProvider> {
            Arc::new(NoopLlm)
        }

        pub fn empty_tool_registry() -> Arc<ToolRegistry> {
            Arc::new(ToolRegistry::new())
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
    // Test C4a: before_llm_call aborts when per-run cost cap is exceeded
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn before_llm_call_aborts_when_cost_cap_exceeded() {
        use crate::automation::runtime::cost::{CostCapConfig, CostCapState};
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-cc", "act-cc");
        let mut delegate = make_delegate("spec-cc", "act-cc", &tmp, conn, PermissionSet::default());
        // Replace cost state with a tiny cap and push it over.
        delegate.cost = Arc::new(CostCapState::new(CostCapConfig {
            per_run_usd: 0.10,
            per_day_usd: 10.0,
        }));
        delegate.cost.add(0.50); // 0.50 >= 0.10 → exceeded

        let mut ctx = ReasoningContext::new("sys".into());
        let outcome = delegate.before_llm_call(&mut ctx, 1).await;
        assert!(matches!(outcome, Some(LoopOutcome::Failure { .. })));

        // Gate should be ErrorTerminal.
        let gate = delegate.gate.lock().await;
        assert!(matches!(*gate, Some(CompletionGate::ErrorTerminal(_))));
    }

    // -----------------------------------------------------------------------
    // Test C4b: on_usage accumulates cost correctly
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn on_usage_accumulates_cost() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-ou", "act-ou");
        let delegate = make_delegate("spec-ou", "act-ou", &tmp, conn, PermissionSet::default());
        let ctx = ReasoningContext::new("sys".into());
        let usage = crate::agent::types::TokenUsage {
            input_tokens: 1_000_000, output_tokens: 0, ..Default::default()
        };
        delegate.on_usage(&usage, &ctx).await;
        // claude-sonnet pricing: $3 / 1M input → total ~= 3.0
        assert!(delegate.cost.total_usd() > 2.9);
    }

    // -----------------------------------------------------------------------
    // Test C5: report_to_user persists artifacts into report_artifacts_json
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn report_to_user_persists_artifacts_json() {
        let tmp = TempDir::new().unwrap();
        let conn = setup_db("spec-art", "act-art");
        let delegate = make_delegate("spec-art", "act-art", &tmp, conn, PermissionSet::default());
        let mut ctx = ReasoningContext::new("sys".into());

        let calls = vec![make_tool_call(
            "report_to_user",
            json!({
                "text": "done",
                "outcome": "useful",
                "artifacts": [
                    { "kind": "file", "path": "out/report.md", "title": "Report" }
                ]
            }),
        )];
        delegate.execute_tool_calls(calls, &mut ctx).await.unwrap();

        let conn2 = delegate.db.lock().unwrap();
        let artifacts_json: String = conn2
            .query_row(
                "SELECT report_artifacts_json FROM automation_activities WHERE id='act-art'",
                [], |r| r.get(0))
            .unwrap();
        assert!(artifacts_json.contains("report.md"));
        assert!(artifacts_json.contains("\"kind\":\"file\""));
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

    // -----------------------------------------------------------------------
    // Test 5: notify_user dispatches without panic when handles are None
    // -----------------------------------------------------------------------
    #[tokio::test]
    async fn notify_user_dispatches_without_panic() {
        let spec_id = "spec-notify";
        let act_id  = "act-notify";
        let tmp  = TempDir::new().unwrap();
        let conn = setup_db(spec_id, act_id);
        // Both optional handles are None in unit tests — system emit is skipped,
        // channel dispatch is skipped. The tool result must say "dispatched".
        // Grant Notification permission so notify_user passes the permission gate.
        let perms = PermissionSet {
            spec: vec![],
            granted: vec![Permission::Notification],
            denied: vec![],
        };
        let delegate = make_delegate(spec_id, act_id, &tmp, conn, perms);
        let call = ToolCall {
            id: "c1".into(),
            name: "notify_user".into(),
            arguments: serde_json::json!({
                "channels": ["system", "wecom"],
                "title": "test alert",
                "body":  "hello world",
                "level": "info"
            }),
        };
        let mut ctx = ReasoningContext::new(String::new());
        let result = delegate.execute_tool_calls(vec![call], &mut ctx).await;
        assert!(result.is_ok(), "execute_tool_calls should not error: {:?}", result);
        let last = ctx.messages.last().expect("tool result pushed");
        // Check that the content contains a ToolResult with "dispatched" text.
        let found_dispatched = last.content.iter().any(|block| {
            matches!(block, ContentBlock::ToolResult { content, .. } if content.contains("dispatched"))
        });
        assert!(
            found_dispatched,
            "expected 'dispatched' in tool result, got: {:?}",
            last.content
        );
    }
}
