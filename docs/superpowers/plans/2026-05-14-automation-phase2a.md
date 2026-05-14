# Automation Phase 2a — 打通执行墙 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `AppRuntimeService::execute_run` to actually invoke the agent loop, so a triggered automation run executes a real `run_agentic_loop`, persists its transcript as an `agent_session`, remembers, produces artifacts, reports back, and is cost-bounded.

**Architecture:** An automation run *is* an `agent_session` (origin metadata distinguishes it). `automation_activities` becomes a thin ledger that links to the session. A new headless `AutomationDelegate` drives `run_agentic_loop`; the LLM-streaming logic currently inline in `ChatDelegate::call_llm` is extracted into a shared `agent/llm_stream.rs` helper so both delegates share it. Cost is bounded by per-run and per-day USD caps. Spec: `docs/superpowers/specs/2026-05-14-automation-phase2a-design.md`.

**Tech Stack:** Rust (`uclaw_core` crate, Tauri v2 backend, rusqlite, tokio, async-trait), React 18 + TypeScript + Jotai + Tailwind (frontend). Tests: inline `#[cfg(test)]` (Rust), Vitest + jsdom (frontend).

**Reference — read before starting:** the design spec above, plus `CLAUDE.md` Part 1 (uClaw working style, especially *Adjacent edits* and *Active migration registry*).

**Migration number:** V24 (V23a is the highest merged; no open PR has claimed V24). Update the *Active migration registry* table in `CLAUDE.md` as part of Task A1.

---

## File Structure

**New files:**
- `src-tauri/src/agent/llm_stream.rs` — shared LLM streaming helper (`stream_completion` + `StreamSink` trait), extracted from `ChatDelegate::call_llm`.
- `src-tauri/src/automation/runtime/cost.rs` — `CostCapConfig` + per-run/per-day cap evaluation helpers.
- `src-tauri/src/automation/runtime/run_session.rs` — run-session lifecycle: home-space ensure, `agent_session` row creation, transcript persistence, retention prune.

**Modified files (backend):**
- `src-tauri/src/db/migrations.rs` — V24 migration.
- `src-tauri/src/automation/activity.rs` — `AutomationActivity` struct: drop `tool_calls_json`, add `session_id` + `report_artifacts_json`.
- `src-tauri/src/agent/mod.rs` — register `llm_stream` module.
- `src-tauri/src/agent/dispatcher.rs` — `ChatDelegate::call_llm` refactored to call `stream_completion`; `ChatDelegate` impls `StreamSink`.
- `src-tauri/src/automation/runtime/mod.rs` — register `cost` + `run_session` modules; re-exports.
- `src-tauri/src/automation/runtime/execute.rs` — `AutomationDelegate` gains LLM/tools/cost fields; `call_llm`, `execute_tool_calls` fall-through, `notify_user`, `report_to_user`, `on_usage`, `before_llm_call`, `handle_text_response`, `after_iteration` all implemented.
- `src-tauri/src/automation/runtime/service.rs` — `AppRuntimeService` gains `provider_service`; `execute_run` replaces the `deferred_phase_2` stub with `run_agentic_loop`.
- `src-tauri/src/automation/runtime/prompt.rs` — `build_initial_message` pre-loads Tier 1 memory.
- `src-tauri/src/automation/memory/compact.rs` — fill in the per-spec retention prune (currently a Phase 1 no-op comment file).
- `src-tauri/src/memubot_config.rs` — add an `automation: AutomationConfig` section (cost caps + retention N).
- `src-tauri/src/app.rs` — pass `provider_service` into `AppRuntimeService::new`.

**Modified files (frontend):**
- `ui/src/lib/tauri-bridge.ts` — `AutomationActivity` interface: drop `toolCallsJson`, add `sessionId` + `reportArtifactsJson`.
- `ui/src/components/automation/AutomationHub.tsx` — activity row → click opens the run-session in the Agent view.
- `ui/src/components/agent/AgentView.tsx` — automation run banner.
- `ui/src/components/app-shell/RightSidePanel.tsx` — tab visibility by run capability.
- `ui/src/atoms/agent-atoms.ts` — `ActiveTab` re-import stays in sync if touched.
- `ui/src/components/workspace/WorkspaceRail.tsx` — filter out `origin = automation` sessions by default.

---

## Milestone A — V24 Migration & Data Model

### Task A1: V24 migration + `AutomationActivity` schema sync

V24 adds `automation_activities.session_id` + `automation_activities.report_artifacts_json`, drops `automation_activities.tool_calls_json`, and adds `agent_sessions.archived_at`. Because `tool_calls_json` is removed, the `AutomationActivity` Rust struct, its row mapper, its `insert_activity`, `service.rs::execute_run`, every test fixture that inserts an activity, and the frontend `AutomationActivity` interface must all change together — they will not compile otherwise. This is one cohesive task.

**Files:**
- Modify: `src-tauri/src/db/migrations.rs` (add `V24_*` const + invoke in `run()` near line 1325)
- Modify: `src-tauri/src/automation/activity.rs` (struct, `SELECT_COLS`, `row_to_activity`, `insert_activity`, test fixtures)
- Modify: `src-tauri/src/automation/runtime/service.rs` (`execute_run` activity literal at lines 364–386; test inserts at lines 1298–1305, 1333–1339, 1344–1353, 1400–1417)
- Modify: `ui/src/lib/tauri-bridge.ts` (`AutomationActivity` interface, lines 1218–1239)
- Modify: `CLAUDE.md` (Active migration registry table)

- [ ] **Step 1: Write the failing migration test**

In `src-tauri/src/db/migrations.rs`, find the `#[cfg(test)] mod tests` block and add:

```rust
    #[test]
    fn v24_adds_session_columns_and_drops_tool_calls_json() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        run(&conn).unwrap();

        // session_id + report_artifacts_json exist on automation_activities
        let has_session_id: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'session_id'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_session_id, 1, "session_id column missing");

        let has_artifacts: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'report_artifacts_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_artifacts, 1, "report_artifacts_json column missing");

        // tool_calls_json is gone
        let has_tool_calls: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('automation_activities') WHERE name = 'tool_calls_json'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_tool_calls, 0, "tool_calls_json should have been dropped");

        // agent_sessions.archived_at exists
        let has_archived_at: i64 = conn.query_row(
            "SELECT COUNT(*) FROM pragma_table_info('agent_sessions') WHERE name = 'archived_at'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(has_archived_at, 1, "agent_sessions.archived_at column missing");
    }

    #[test]
    fn v24_is_idempotent() {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        // Running the whole migration set again must not error.
        run(&conn).unwrap();
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib migrations::tests::v24`
Expected: FAIL — `session_id column missing` (V24 not written yet).

- [ ] **Step 3: Add the V24 SQL constant**

In `src-tauri/src/db/migrations.rs`, after the `V23A_MARKETPLACE_CACHE` const + `run_v23a` function (around line 1154), add:

```rust
/// V24 — automation run = agent_session ownership model.
/// `automation_activities` gains `session_id` (nullable link to the run's
/// agent_session) + `report_artifacts_json` (declared products), and drops
/// `tool_calls_json` (per-tool breakdown now lives in agent_messages).
/// `agent_sessions` gains `archived_at` for retention ordering.
/// All statements are individually error-tolerant: a re-run hits
/// "duplicate column" / "no such column" and is skipped (same pattern as
/// V9–V19). DROP COLUMN requires SQLite >= 3.35 (rusqlite bundles a newer one).
const V24_AUTOMATION_RUN_SESSIONS: &str = "
ALTER TABLE automation_activities ADD COLUMN session_id TEXT;
ALTER TABLE automation_activities ADD COLUMN report_artifacts_json TEXT NOT NULL DEFAULT '[]';
ALTER TABLE automation_activities DROP COLUMN tool_calls_json;
ALTER TABLE agent_sessions ADD COLUMN archived_at INTEGER;
CREATE INDEX IF NOT EXISTS idx_act_session ON automation_activities(session_id);
";
```

- [ ] **Step 4: Invoke V24 in `run()`**

In `src-tauri/src/db/migrations.rs`, in `pub fn run(...)`, find the closing of the V23a block and the final log line:

```rust
    if let Err(e) = run_v23a(conn) {
        tracing::error!(error = %e, "V23a FAILED — marketplace cache unavailable");
        return Err(e);
    }
    tracing::info!("Database migrations complete");
    Ok(())
}
```

Replace with:

```rust
    if let Err(e) = run_v23a(conn) {
        tracing::error!(error = %e, "V23a FAILED — marketplace cache unavailable");
        return Err(e);
    }
    // V24: automation run = agent_session. Statement-split tolerant style —
    // ADD/DROP COLUMN are not transactional-schema-replacing, so partial
    // application is fine and a re-run's "duplicate/no such column" is benign.
    tracing::debug!("Running migration V24: automation run-session columns");
    for stmt in V24_AUTOMATION_RUN_SESSIONS.split(';').map(|s| s.trim()).filter(|s| !s.is_empty()) {
        if let Err(e) = conn.execute(stmt, []) {
            tracing::warn!("V24 stmt skipped: {} :: {}", e, stmt);
        }
    }
    tracing::info!("Database migrations complete");
    Ok(())
}
```

- [ ] **Step 5: Run the migration test to verify it passes**

Run: `cd src-tauri && cargo test --lib migrations::tests::v24`
Expected: PASS (both `v24_adds_session_columns_and_drops_tool_calls_json` and `v24_is_idempotent`).

- [ ] **Step 6: Update the `AutomationActivity` struct + mappers in `activity.rs`**

In `src-tauri/src/automation/activity.rs`:

In the `AutomationActivity` struct, remove the `pub tool_calls_json: String,` line and add two fields after `llm_tokens_out`:

```rust
    pub llm_iterations: i64,
    pub llm_tokens_in: i64,
    pub llm_tokens_out: i64,
    /// Nullable link to the run's agent_session (V24). None for runs that
    /// never reached the loop (filtered out, deduped, rejected).
    pub session_id: Option<String>,
    /// JSON array of declared products from report_to_user.artifacts (V24).
    pub report_artifacts_json: String,
    pub report_text: Option<String>,
```

Update `SELECT_COLS` — replace `tool_calls_json` with `session_id, report_artifacts_json`:

```rust
const SELECT_COLS: &str =
    "id, spec_id, subscription_id, trigger_source_type, trigger_payload_json,
     status, error_text, queued_at, started_at, completed_at, duration_ms,
     llm_iterations, llm_tokens_in, llm_tokens_out, session_id, report_artifacts_json,
     report_text, report_outcome, escalation_id,
     resumed_from_activity_id, resumed_from_escalation_id";
```

In `row_to_activity`, the column indices shift. Replace the body from `llm_tokens_out` onward to match the new `SELECT_COLS` order:

```rust
        llm_iterations:              r.get(11)?,
        llm_tokens_in:               r.get(12)?,
        llm_tokens_out:              r.get(13)?,
        session_id:                  r.get(14)?,
        report_artifacts_json:       r.get(15)?,
        report_text:                 r.get(16)?,
        report_outcome:              r.get(17)?,
        escalation_id:               r.get(18)?,
        resumed_from_activity_id:    r.get(19)?,
        resumed_from_escalation_id:  r.get(20)?,
```

In `insert_activity`, update the column list + params. Replace `tool_calls_json` in the column list with `session_id, report_artifacts_json` and in the `params!` macro replace `a.tool_calls_json` with `a.session_id, a.report_artifacts_json`:

```rust
pub fn insert_activity(conn: &rusqlite::Connection, a: &AutomationActivity) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO automation_activities (
            id, spec_id, subscription_id, trigger_source_type, trigger_payload_json,
            status, error_text, queued_at, started_at, completed_at, duration_ms,
            llm_iterations, llm_tokens_in, llm_tokens_out, session_id, report_artifacts_json,
            report_text, report_outcome, escalation_id,
            resumed_from_activity_id, resumed_from_escalation_id
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21
        )",
        rusqlite::params![
            a.id, a.spec_id, a.subscription_id,
            a.trigger_source_type.as_db_str(), a.trigger_payload_json,
            a.status.as_db_str(), a.error_text,
            a.queued_at, a.started_at, a.completed_at, a.duration_ms,
            a.llm_iterations, a.llm_tokens_in, a.llm_tokens_out, a.session_id, a.report_artifacts_json,
            a.report_text, a.report_outcome, a.escalation_id,
            a.resumed_from_activity_id, a.resumed_from_escalation_id,
        ],
    )?;
    Ok(())
}
```

In `activity.rs`'s `#[cfg(test)] mod tests`, in `make_activity`, replace `tool_calls_json: "[]".into(),` with:

```rust
            session_id:                  None,
            report_artifacts_json:       "[]".into(),
```

And in the test `roundtrip_activity`, replace `assert_eq!(loaded.tool_calls_json, "[]");` with `assert_eq!(loaded.report_artifacts_json, "[]");`.

- [ ] **Step 7: Update `service.rs::execute_run` + its test fixtures**

In `src-tauri/src/automation/runtime/service.rs`, in `execute_run`, the `AutomationActivity { ... }` literal (lines 364–386): remove `tool_calls_json: "[]".into(),` and add:

```rust
            llm_tokens_in:               0,
            llm_tokens_out:              0,
            session_id:                  None,
            report_artifacts_json:       "[]".into(),
            report_text:                 None,
```

In `service.rs`'s test module, the raw-SQL inserts at lines ~1298–1305, ~1333–1339, ~1344 (the `automation_activities` INSERTs in `uninstall_removes_spec_and_cascades`, `list_pending_escalations_filters_by_status`, `resolve_escalation_updates_row`) all list `tool_calls_json` in their column lists with a `'[]'` value. Remove `tool_calls_json` from each column list and remove the corresponding `'[]'` value. Example — change:

```rust
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json,
                  status, queued_at, duration_ms, llm_iterations,
                  llm_tokens_in, llm_tokens_out, tool_calls_json)
                 VALUES ('act-del','del-spec','manual','{}','queued',1,0,0,0,0,'[]')",
```

to:

```rust
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json,
                  status, queued_at, duration_ms, llm_iterations,
                  llm_tokens_in, llm_tokens_out)
                 VALUES ('act-del','del-spec','manual','{}','queued',1,0,0,0,0)",
```

Apply the same edit to all three test INSERTs (drop `tool_calls_json` column + its `'[]'` value).

- [ ] **Step 8: Update the frontend `AutomationActivity` interface**

In `ui/src/lib/tauri-bridge.ts`, in the `AutomationActivity` interface (lines 1218–1239), remove `toolCallsJson: string` and add after `llmTokensOut`:

```ts
  llmIterations: number
  llmTokensIn: number
  llmTokensOut: number
  sessionId: string | null
  reportArtifactsJson: string
  reportText: string | null
```

- [ ] **Step 9: Run the full affected test set + typecheck**

Run: `cd src-tauri && cargo test --lib automation::activity && cargo test --lib automation::runtime::service && cargo test --lib migrations`
Expected: PASS for all.
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output (no compile errors).
Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors referencing `AutomationActivity` / `toolCallsJson`.

- [ ] **Step 10: Update the CLAUDE.md migration registry**

In `CLAUDE.md`, in the *Active migration registry* table, add a row after the V23a row:

```
| V24 | automation_activities +session_id +report_artifacts_json -tool_calls_json; agent_sessions +archived_at | this PR |
```

- [ ] **Step 11: Commit**

```bash
git add src-tauri/src/db/migrations.rs src-tauri/src/automation/activity.rs src-tauri/src/automation/runtime/service.rs ui/src/lib/tauri-bridge.ts CLAUDE.md
git commit -m "feat(db): V24 — automation run = agent_session data model

automation_activities gains session_id + report_artifacts_json, drops
tool_calls_json (per-tool breakdown now lives in agent_messages).
agent_sessions gains archived_at. Syncs the AutomationActivity Rust
struct + frontend interface to match."
```

---

## Milestone B — `agent/llm_stream.rs` Shared Helper

### Task B1: Define `StreamSink` trait + `stream_completion` helper

Extract the provider-streaming + retry logic so `ChatDelegate` and the new `AutomationDelegate` share one implementation. The streaming state machine and retry-budget logic are behaviour-preserving; the only new abstraction is `StreamSink`, a trait for the side-effects `call_llm` currently does directly on `ChatDelegate` (`emit_text_delta`, `emit_thinking`, `emit_thinking_done`, `emit_stream_reset`, `emit_retry_event`, `sleep_or_abort`).

**Files:**
- Create: `src-tauri/src/agent/llm_stream.rs`
- Modify: `src-tauri/src/agent/mod.rs` (register module)
- Test: inline `#[cfg(test)]` in `src-tauri/src/agent/llm_stream.rs`

- [ ] **Step 1: Register the module**

In `src-tauri/src/agent/mod.rs`, insert `pub mod llm_stream;` in alphabetical order between `pub mod dispatcher;` (line 4) and `pub mod mode_prompts;` (line 5):

```rust
pub mod agentic_loop;
pub mod code_rescue;
pub mod context;
pub mod dispatcher;
pub mod llm_stream;
pub mod mode_prompts;
pub mod plan_state;
pub mod retry;
pub mod session;
pub mod teams;
pub mod tools;
pub mod types;
```

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/agent/llm_stream.rs` with only the test module first, plus stub declarations so it compiles to a failing state:

```rust
//! Shared LLM streaming helper. Drives a provider stream to completion,
//! handling tiered-timeout retries via RetryBudget, and surfaces streaming
//! side-effects through the `StreamSink` trait so both the interactive
//! ChatDelegate (IPC-emitting) and the headless AutomationDelegate (no-op
//! sink) can reuse one implementation.

use crate::agent::retry::AgentRetryEvent;
use crate::agent::types::{ChatMessage, RespondOutput, ToolDefinition};
use crate::error::Error;
use crate::llm::CompletionConfig;
use crate::llm::LlmProvider;
use async_trait::async_trait;
use std::time::Duration;

/// Side-effects a streaming completion produces. The interactive delegate
/// emits IPC events; the headless automation delegate uses `NoopSink`.
#[async_trait]
pub trait StreamSink: Send + Sync {
    fn on_text_delta(&self, text: &str);
    fn on_thinking(&self, thinking: &str);
    fn on_thinking_done(&self, duration_ms: u64);
    fn on_stream_reset(&self);
    fn on_retry_event(&self, event: AgentRetryEvent);
    /// Sleep for `delay`, returning `true` if the caller should abort
    /// (e.g. a stop flag was set during the sleep).
    async fn sleep_or_abort(&self, delay: Duration) -> bool;
}

/// A `StreamSink` that does nothing and never aborts on sleep. Used by the
/// headless automation path, which has no frontend to emit to.
pub struct NoopSink;

#[async_trait]
impl StreamSink for NoopSink {
    fn on_text_delta(&self, _text: &str) {}
    fn on_thinking(&self, _thinking: &str) {}
    fn on_thinking_done(&self, _duration_ms: u64) {}
    fn on_stream_reset(&self) {}
    fn on_retry_event(&self, _event: AgentRetryEvent) {}
    async fn sleep_or_abort(&self, delay: Duration) -> bool {
        tokio::time::sleep(delay).await;
        false
    }
}

/// Drive `llm.stream(...)` to a `RespondOutput`, retrying transient/stalled
/// failures via a fresh `RetryBudget::for_agent_loop()`. All streaming
/// side-effects go through `sink`.
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    config: &CompletionConfig,
    sink: &dyn StreamSink,
) -> Result<RespondOutput, Error> {
    todo!("implemented in Step 4")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::types::{ResponseMetadata, StreamDelta, TokenUsage};
    use futures::stream;
    use std::sync::{Arc, Mutex};

    /// A fake LlmProvider that yields a scripted list of StreamDeltas.
    struct ScriptedProvider {
        deltas: Vec<Result<StreamDelta, Error>>,
    }

    #[async_trait]
    impl LlmProvider for ScriptedProvider {
        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<crate::llm::LlmStream, Error> {
            let items = self.deltas.clone();
            Ok(Box::pin(stream::iter(items)))
        }
        // NOTE: the implementer must also stub any other required
        // LlmProvider trait methods here with `unimplemented!()` — check
        // the trait definition in src-tauri/src/llm/mod.rs and mirror what
        // the existing dispatcher tests' mock provider does (grep
        // `impl LlmProvider for` under src-tauri/src/ for an existing test mock).
    }

    /// A sink that records what it received.
    #[derive(Default)]
    struct RecordingSink {
        text: Mutex<String>,
        thinking: Mutex<String>,
    }
    #[async_trait]
    impl StreamSink for RecordingSink {
        fn on_text_delta(&self, t: &str) { self.text.lock().unwrap().push_str(t); }
        fn on_thinking(&self, t: &str) { self.thinking.lock().unwrap().push_str(t); }
        fn on_thinking_done(&self, _d: u64) {}
        fn on_stream_reset(&self) {}
        fn on_retry_event(&self, _e: AgentRetryEvent) {}
        async fn sleep_or_abort(&self, _d: Duration) -> bool { false }
    }

    fn cfg() -> CompletionConfig {
        CompletionConfig {
            model: "claude-sonnet-4-6".into(),
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: Some("sys".into()),
            thinking_enabled: false,
        }
    }

    #[tokio::test]
    async fn text_response_assembles_full_text_and_emits_deltas() {
        let provider = ScriptedProvider {
            deltas: vec![
                Ok(StreamDelta::TextDelta { text: "Hello ".into() }),
                Ok(StreamDelta::TextDelta { text: "world".into() }),
                Ok(StreamDelta::Done {
                    finish_reason: Some("stop".into()),
                    usage: Some(TokenUsage { input_tokens: 10, output_tokens: 5, ..Default::default() }),
                }),
            ],
        };
        let sink = RecordingSink::default();
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink).await.unwrap();
        match out {
            RespondOutput::Text { text, .. } => assert_eq!(text, "Hello world"),
            other => panic!("expected Text, got {:?}", other),
        }
        assert_eq!(*sink.text.lock().unwrap(), "Hello world");
    }

    #[tokio::test]
    async fn tool_call_delta_assembles_tool_calls() {
        let provider = ScriptedProvider {
            deltas: vec![
                Ok(StreamDelta::ToolCallDelta {
                    id: "tc1".into(),
                    name: Some("bash".into()),
                    input_json: Some(r#"{"command":"ls"}"#.into()),
                }),
                Ok(StreamDelta::Done { finish_reason: Some("tool_use".into()), usage: None }),
            ],
        };
        let sink = RecordingSink::default();
        let out = stream_completion(&provider, vec![], vec![], &cfg(), &sink).await.unwrap();
        match out {
            RespondOutput::ToolCalls { tool_calls, .. } => {
                assert_eq!(tool_calls.len(), 1);
                assert_eq!(tool_calls[0].name, "bash");
            }
            other => panic!("expected ToolCalls, got {:?}", other),
        }
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cd src-tauri && cargo test --lib agent::llm_stream`
Expected: FAIL — `not yet implemented` panic from the `todo!()` in `stream_completion`.

> If compilation fails because `LlmProvider` / `LlmStream` / `CompletionConfig` paths are wrong, grep `src-tauri/src/llm/mod.rs` for the exact `pub use` paths and the `LlmProvider` trait's full method set, then fix the `use` lines and the `ScriptedProvider` stub. The mock must implement every required trait method.

- [ ] **Step 4: Implement `stream_completion`**

Replace the `todo!(...)` body of `stream_completion`. Port the streaming + retry state machine verbatim from `ChatDelegate::call_llm` (`src-tauri/src/agent/dispatcher.rs` lines 580–834 — the part from `let mut retry_budget = RetryBudget::for_agent_loop();` through the end of the `'stream_attempt` loop), substituting:
- `self.llm.stream(...)` → `llm.stream(...)`
- `self.emit_text_delta(&text)` → `sink.on_text_delta(&text)`
- `self.emit_thinking(&thinking)` → `sink.on_thinking(&thinking)`
- `self.emit_thinking_done(duration)` → `sink.on_thinking_done(duration)`
- `self.emit_stream_reset()` → `sink.on_stream_reset()`
- `self.emit_retry_event(ev)` → `sink.on_retry_event(ev)`
- `self.sleep_or_abort(delay).await` → `sink.sleep_or_abort(delay).await`
- `self.model.clone()` → `config.model.clone()`

The function body is:

```rust
pub async fn stream_completion(
    llm: &dyn LlmProvider,
    messages: Vec<ChatMessage>,
    tools: Vec<ToolDefinition>,
    config: &CompletionConfig,
    sink: &dyn StreamSink,
) -> Result<RespondOutput, Error> {
    use crate::agent::retry::{BudgetDecision, RetryBudget};
    use crate::agent::types::{ResponseMetadata, StreamDelta, ToolCall};
    use crate::llm::stream_error::{classify_stream_error, StreamErrorKind};
    use futures::StreamExt;

    let mut retry_budget = RetryBudget::for_agent_loop();
    'stream_attempt: loop {
        match llm.stream(messages.clone(), tools.clone(), config).await {
            Ok(mut stream) => {
                let mut full_text = String::new();
                let mut full_thinking = String::new();
                let mut full_thinking_signature: Option<String> = None;
                let mut tool_calls: Vec<ToolCall> = Vec::new();
                let mut current_tool: Option<(String, String, String)> = None;
                let mut thinking_started = false;
                let mut thinking_start_time: Option<std::time::Instant> = None;
                let mut metadata: Option<ResponseMetadata> = None;

                while let Some(item) = stream.next().await {
                    match item {
                        Ok(StreamDelta::TextDelta { text }) => {
                            if thinking_started {
                                thinking_started = false;
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            sink.on_text_delta(&text);
                            full_text.push_str(&text);
                        }
                        Ok(StreamDelta::ThinkingDelta { thinking }) => {
                            if !thinking_started {
                                thinking_started = true;
                                thinking_start_time = Some(std::time::Instant::now());
                            }
                            sink.on_thinking(&thinking);
                            full_thinking.push_str(&thinking);
                        }
                        Ok(StreamDelta::SignatureDelta { signature }) => {
                            full_thinking_signature = Some(signature);
                        }
                        Ok(StreamDelta::ToolCallDelta { id, name, input_json }) => {
                            if thinking_started {
                                thinking_started = false;
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            if let Some(n) = name {
                                if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                    if let Ok(args) = serde_json::from_str(&tc_args) {
                                        tool_calls.push(ToolCall { id: tc_id, name: tc_name, arguments: args });
                                    }
                                }
                                current_tool = Some((id, n, String::new()));
                            }
                            if let Some(args) = input_json {
                                if let Some((_, _, ref mut tc_args)) = current_tool {
                                    tc_args.push_str(&args);
                                }
                            }
                        }
                        Ok(StreamDelta::Done { finish_reason, usage }) => {
                            if thinking_started {
                                let duration = thinking_start_time
                                    .map(|t| t.elapsed().as_millis() as u64)
                                    .unwrap_or(0);
                                sink.on_thinking_done(duration);
                            }
                            if let Some((tc_id, tc_name, tc_args)) = current_tool.take() {
                                if let Ok(args) = serde_json::from_str(&tc_args) {
                                    tool_calls.push(ToolCall { id: tc_id, name: tc_name, arguments: args });
                                }
                            }
                            metadata = Some(ResponseMetadata {
                                model: config.model.clone(),
                                finish_reason,
                                usage,
                            });
                            let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };
                            let meta = metadata.unwrap();
                            if !tool_calls.is_empty() {
                                return Ok(RespondOutput::ToolCalls {
                                    tool_calls,
                                    text: if full_text.is_empty() { None } else { Some(full_text) },
                                    thinking,
                                    thinking_signature: full_thinking_signature,
                                    metadata: meta,
                                });
                            } else {
                                return Ok(RespondOutput::Text {
                                    text: full_text,
                                    thinking,
                                    thinking_signature: full_thinking_signature,
                                    metadata: meta,
                                });
                            }
                        }
                        Err(e) => {
                            let kind = classify_stream_error(&e);
                            match kind {
                                StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                                    match retry_budget.next_delay() {
                                        BudgetDecision::Sleep(delay) => {
                                            let reason = format!("{:?}: {}", kind, e);
                                            tracing::warn!(error = %e, kind = ?kind,
                                                attempt = retry_budget.attempts(),
                                                max = retry_budget.max_attempts(),
                                                delay_ms = delay.as_millis() as u64,
                                                "Stream interrupted, retrying with a fresh stream");
                                            sink.on_stream_reset();
                                            sink.on_retry_event(AgentRetryEvent::Starting {
                                                attempt: retry_budget.attempts(),
                                                max_attempts: retry_budget.max_attempts(),
                                                delay_seconds: delay.as_secs_f64(),
                                                reason: reason.clone(),
                                            });
                                            if sink.sleep_or_abort(delay).await {
                                                sink.on_stream_reset();
                                                return Err(e);
                                            }
                                            sink.on_retry_event(AgentRetryEvent::Attempt {
                                                attempt: retry_budget.attempts(),
                                                timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                                reason,
                                            });
                                            continue 'stream_attempt;
                                        }
                                        BudgetDecision::Exhausted => {
                                            tracing::error!(error = %e,
                                                attempts = retry_budget.attempts(),
                                                "Stream failed after exhausting retry budget");
                                            sink.on_stream_reset();
                                            sink.on_retry_event(AgentRetryEvent::Exhausted {
                                                total_attempts: retry_budget.attempts(),
                                                total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                            });
                                            return Err(e);
                                        }
                                    }
                                }
                                StreamErrorKind::Fatal => {
                                    tracing::error!(error = %e, "Stream failed with fatal error");
                                    sink.on_stream_reset();
                                    return Err(e);
                                }
                            }
                        }
                    }
                }

                // Stream ended without a Done delta.
                let meta = metadata.unwrap_or_else(|| ResponseMetadata {
                    model: config.model.clone(),
                    finish_reason: Some("stream_ended".into()),
                    usage: None,
                });
                let thinking = if full_thinking.is_empty() { None } else { Some(full_thinking) };
                if !tool_calls.is_empty() {
                    return Ok(RespondOutput::ToolCalls {
                        tool_calls,
                        text: if full_text.is_empty() { None } else { Some(full_text) },
                        thinking,
                        thinking_signature: full_thinking_signature,
                        metadata: meta,
                    });
                } else {
                    return Ok(RespondOutput::Text {
                        text: full_text,
                        thinking,
                        thinking_signature: full_thinking_signature,
                        metadata: meta,
                    });
                }
            }
            Err(e) => {
                let kind = classify_stream_error(&e);
                match kind {
                    StreamErrorKind::Stalled | StreamErrorKind::TransientNetwork => {
                        match retry_budget.next_delay() {
                            BudgetDecision::Sleep(delay) => {
                                let reason = format!("setup {:?}: {}", kind, e);
                                tracing::warn!(error = %e, kind = ?kind,
                                    attempt = retry_budget.attempts(),
                                    max = retry_budget.max_attempts(),
                                    delay_ms = delay.as_millis() as u64,
                                    "Stream setup failed transiently, retrying");
                                sink.on_retry_event(AgentRetryEvent::Starting {
                                    attempt: retry_budget.attempts(),
                                    max_attempts: retry_budget.max_attempts(),
                                    delay_seconds: delay.as_secs_f64(),
                                    reason: reason.clone(),
                                });
                                if sink.sleep_or_abort(delay).await {
                                    return Err(e);
                                }
                                sink.on_retry_event(AgentRetryEvent::Attempt {
                                    attempt: retry_budget.attempts(),
                                    timestamp_ms: chrono::Utc::now().timestamp_millis(),
                                    reason,
                                });
                                continue 'stream_attempt;
                            }
                            BudgetDecision::Exhausted => {
                                tracing::error!(error = %e, attempts = retry_budget.attempts(),
                                    "Stream setup failed after exhausting retry budget");
                                sink.on_retry_event(AgentRetryEvent::Exhausted {
                                    total_attempts: retry_budget.attempts(),
                                    total_wait_ms: retry_budget.elapsed_wait().as_millis() as u64,
                                });
                                return Err(e);
                            }
                        }
                    }
                    StreamErrorKind::Fatal => {
                        tracing::error!(error = %e, "Stream setup failed, surfacing error");
                        return Err(e);
                    }
                }
            }
        }
    }
}
```

> If the `use` paths (`crate::llm::stream_error::...`, `crate::agent::retry::...`, `crate::llm::LlmStream`) don't resolve, grep for the actual paths: `grep -rn "classify_stream_error\|pub fn for_agent_loop\|pub type LlmStream" src-tauri/src/` and fix the imports. The logic is a verbatim port — do not change behaviour.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd src-tauri && cargo test --lib agent::llm_stream`
Expected: PASS (`text_response_assembles_full_text_and_emits_deltas`, `tool_call_delta_assembles_tool_calls`).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/agent/llm_stream.rs src-tauri/src/agent/mod.rs
git commit -m "feat(agent): extract stream_completion + StreamSink shared helper

Pulls the provider-streaming + retry-budget state machine out of
ChatDelegate::call_llm into agent/llm_stream.rs so the headless
AutomationDelegate can reuse it. Behaviour-preserving; ChatDelegate
is rewired to call it in the next commit."
```

### Task B2: Rewire `ChatDelegate::call_llm` to use `stream_completion`

`ChatDelegate` implements `StreamSink` (its `emit_*` / `sleep_or_abort` methods become the trait impl), and `call_llm` keeps only the prompt-assembly prologue, then delegates to `stream_completion`.

**Files:**
- Modify: `src-tauri/src/agent/dispatcher.rs`

- [ ] **Step 1: Add the `StreamSink` impl for `ChatDelegate`**

In `src-tauri/src/agent/dispatcher.rs`, after the `impl ChatDelegate { ... }` block (the one holding `emit_text_delta`, `emit_thinking`, etc. — around lines 80–505), add a new `impl` block. The body of each method is the **existing body** of the corresponding `ChatDelegate::emit_*` / `sleep_or_abort` method — move/copy it unchanged:

```rust
#[async_trait::async_trait]
impl crate::agent::llm_stream::StreamSink for ChatDelegate {
    fn on_text_delta(&self, text: &str) {
        // body of the existing ChatDelegate::emit_text_delta (dispatcher.rs ~220–225)
        self.emit_text_delta(text);
    }
    fn on_thinking(&self, thinking: &str) {
        self.emit_thinking(thinking);
    }
    fn on_thinking_done(&self, duration_ms: u64) {
        self.emit_thinking_done(duration_ms);
    }
    fn on_stream_reset(&self) {
        self.emit_stream_reset();
    }
    fn on_retry_event(&self, event: crate::agent::retry::AgentRetryEvent) {
        self.emit_retry_event(event);
    }
    async fn sleep_or_abort(&self, delay: std::time::Duration) -> bool {
        self.sleep_or_abort(delay).await
    }
}
```

> This delegates to the existing private `emit_*` / `sleep_or_abort` methods rather than moving their bodies — they keep working, and the trait impl is a thin adapter. Keep the existing inherent methods; do not delete them.

- [ ] **Step 2: Rewrite `call_llm` to delegate**

In `src-tauri/src/agent/dispatcher.rs`, in `impl LoopDelegate for ChatDelegate`, replace the `call_llm` method (lines 520–835) — keep the prompt-assembly prologue (lines 520–579, ending with the `tracing::info!(... "Calling LLM")` call), then replace everything from `use futures::StreamExt;` (line 581) through the end of the method with a single call:

```rust
    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        let effective_mode = self.resolve_effective_mode().await;
        let effective_prompt = self.effective_system_prompt(&effective_mode);

        let mut messages = vec![ChatMessage::system(&effective_prompt)];
        messages.extend(reason_ctx.messages.clone());

        if let Some(last_user_idx) = messages.iter().rposition(|m| {
            matches!(m.role, crate::agent::types::MessageRole::User)
                && m.content.iter().any(|b| matches!(b, crate::agent::types::ContentBlock::Text { .. }))
        }) {
            let dyn_ctx = self.build_dynamic_context();
            if let Some(crate::agent::types::ContentBlock::Text { text }) =
                messages[last_user_idx].content.iter_mut().find(|b| {
                    matches!(b, crate::agent::types::ContentBlock::Text { .. })
                })
            {
                *text = format!("{}\n\n{}", dyn_ctx, text);
            }
        }

        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            self.tools.list_definitions()
        };
        let config = crate::llm::CompletionConfig {
            model: self.model.clone(),
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: Some(effective_prompt),
            thinking_enabled: self.thinking_enabled,
        };

        tracing::info!(
            model = %self.model,
            message_count = messages.len(),
            tool_count = tools.len(),
            force_text = reason_ctx.force_text,
            "Calling LLM"
        );

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            self,
        )
        .await
    }
```

- [ ] **Step 3: Verify the build + existing dispatcher tests still pass**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib agent::dispatcher`
Expected: PASS — all existing `dispatcher` tests still green (behaviour-preserving refactor).

> If a method-name collision occurs (the `StreamSink::sleep_or_abort` trait method vs the inherent `ChatDelegate::sleep_or_abort`), disambiguate the inherent call inside the trait impl with `ChatDelegate::sleep_or_abort(self, delay).await`. Same for any other emit_* name clash.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/agent/dispatcher.rs
git commit -m "refactor(agent): ChatDelegate::call_llm delegates to stream_completion

ChatDelegate now implements StreamSink (thin adapters over its existing
emit_* / sleep_or_abort methods); call_llm keeps only prompt assembly
then hands off to the shared helper. Behaviour-preserving."
```

---

## Milestone C — `AutomationConfig` + `AutomationDelegate` Core

### Task C1: `AutomationConfig` (cost caps + retention N) in `memubot_config.rs`

**Files:**
- Modify: `src-tauri/src/memubot_config.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/memubot_config.rs`, in `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn automation_config_has_defaults() {
        let c = AutomationConfig::default();
        assert!(c.per_run_cost_cap_usd > 0.0);
        assert!(c.per_day_cost_cap_usd > 0.0);
        assert!(c.retention_runs_per_spec >= 1);
    }

    #[test]
    fn memubot_config_includes_automation_section() {
        let config: MemubotConfig = serde_json::from_str("{}").unwrap();
        assert!(config.automation.per_run_cost_cap_usd > 0.0);
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib memubot_config::tests::automation`
Expected: FAIL — `AutomationConfig` not found.

- [ ] **Step 3: Add the `AutomationConfig` struct + wire it into `MemubotConfig`**

In `src-tauri/src/memubot_config.rs`, add the struct (after `ObservabilityConfig`, before `ScenariosConfig`):

```rust
/// Automation runtime configuration — cost guardrails + run-session retention.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AutomationConfig {
    /// Hard USD cap for a single run. When cumulative cost crosses this,
    /// the run terminates as ErrorTerminal.
    pub per_run_cost_cap_usd: f64,
    /// Hard USD cap for all automation runs in a calendar day (UTC). When
    /// the day's total is at/over this, new runs do not start.
    pub per_day_cost_cap_usd: f64,
    /// Per-spec, the number of most-recent run-session transcripts to keep.
    /// Older run-sessions are pruned (agent_messages + agent_session row
    /// deleted, automation_activities.session_id set NULL); the ledger row
    /// itself is never deleted.
    pub retention_runs_per_spec: u32,
    /// Max agentic-loop iterations for an automation run.
    pub max_iterations: usize,
}

impl Default for AutomationConfig {
    fn default() -> Self {
        Self {
            per_run_cost_cap_usd: 1.00,
            per_day_cost_cap_usd: 10.00,
            retention_runs_per_spec: 50,
            max_iterations: 50,
        }
    }
}
```

In the `MemubotConfig` struct, add a field after `scenarios`:

```rust
    /// Proactive 场景配置
    #[serde(default)]
    pub scenarios: ScenariosConfig,
    /// Automation runtime configuration (cost caps + retention).
    #[serde(default)]
    pub automation: AutomationConfig,
```

In `impl Default for MemubotConfig`, add `automation: AutomationConfig::default(),` to the struct literal.

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib memubot_config::tests::automation`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/memubot_config.rs
git commit -m "feat(config): add AutomationConfig — cost caps + retention N"
```

### Task C2: `cost.rs` — cost-cap evaluation helpers

**Files:**
- Create: `src-tauri/src/automation/runtime/cost.rs`
- Modify: `src-tauri/src/automation/runtime/mod.rs` (register module + re-export)

- [ ] **Step 1: Register the module**

In `src-tauri/src/automation/runtime/mod.rs`, add `pub mod cost;` after `pub mod auto_continue;` and add a re-export line after the existing `pub use auto_continue::{...}`:

```rust
pub mod auto_continue;
pub mod cost;
pub mod execute;
pub mod prompt;
pub mod run_session;
pub mod service;
```

(Note: `pub mod run_session;` is added here too — its file is created in Task D2; for now create an empty placeholder so this module list compiles: `touch src-tauri/src/automation/runtime/run_session.rs` and put `// implemented in Task D2` in it. The `cargo build` at the end of this task will pass with an empty module.)

Add after the `pub use auto_continue::{AutoContinueConfig, CompletionGate};` line:

```rust
pub use cost::{CostCapConfig, CostCapState, CostCapDecision};
```

- [ ] **Step 2: Write the failing test**

Create `src-tauri/src/automation/runtime/cost.rs`:

```rust
//! Cost guardrails for automation runs. Two hard caps (distinct from the
//! observational cost_records / V13): a per-run cap that terminates a run
//! mid-loop, and a per-day cap checked before a run starts.

use std::sync::atomic::{AtomicU64, Ordering};

/// Resolved cost caps for one run, sourced from MemubotConfig.automation.
#[derive(Debug, Clone, Copy)]
pub struct CostCapConfig {
    pub per_run_usd: f64,
    pub per_day_usd: f64,
}

/// Mutable per-run cost accumulator. `cents` is micro-dollars * 0 — we store
/// hundred-thousandths of a USD (USD * 100_000) in an AtomicU64 so the
/// delegate can accumulate cost across loop iterations without a Mutex<f64>.
#[derive(Debug)]
pub struct CostCapState {
    accumulated_micro: AtomicU64,
    per_run_micro: u64,
}

const MICRO_PER_USD: f64 = 100_000.0;

impl CostCapState {
    pub fn new(cap: CostCapConfig) -> Self {
        Self {
            accumulated_micro: AtomicU64::new(0),
            per_run_micro: (cap.per_run_usd * MICRO_PER_USD) as u64,
        }
    }

    /// Add `cost_usd` to the running total. Returns the new total in USD.
    pub fn add(&self, cost_usd: f64) -> f64 {
        let delta = (cost_usd.max(0.0) * MICRO_PER_USD) as u64;
        let prev = self.accumulated_micro.fetch_add(delta, Ordering::Relaxed);
        (prev + delta) as f64 / MICRO_PER_USD
    }

    /// Current accumulated cost in USD.
    pub fn total_usd(&self) -> f64 {
        self.accumulated_micro.load(Ordering::Relaxed) as f64 / MICRO_PER_USD
    }

    /// True once the per-run cap has been reached or exceeded.
    pub fn per_run_exceeded(&self) -> bool {
        self.accumulated_micro.load(Ordering::Relaxed) >= self.per_run_micro
    }
}

/// Result of a pre-run per-day cap check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CostCapDecision {
    /// Under the per-day cap — the run may start.
    Allow,
    /// Day total is at/over the per-day cap — do not start the run.
    DenyPerDay,
}

/// Decide whether a run may start given the day's spend so far.
pub fn check_per_day(day_total_usd: f64, cap: CostCapConfig) -> CostCapDecision {
    if day_total_usd >= cap.per_day_usd {
        CostCapDecision::DenyPerDay
    } else {
        CostCapDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cap() -> CostCapConfig {
        CostCapConfig { per_run_usd: 1.00, per_day_usd: 10.00 }
    }

    #[test]
    fn per_run_accumulates_and_trips_at_cap() {
        let state = CostCapState::new(cap());
        assert!(!state.per_run_exceeded());
        state.add(0.40);
        assert!(!state.per_run_exceeded());
        state.add(0.65); // total 1.05 >= 1.00
        assert!(state.per_run_exceeded());
        assert!((state.total_usd() - 1.05).abs() < 1e-6);
    }

    #[test]
    fn per_run_ignores_negative_cost() {
        let state = CostCapState::new(cap());
        state.add(-5.0);
        assert_eq!(state.total_usd(), 0.0);
    }

    #[test]
    fn per_day_allows_under_cap() {
        assert_eq!(check_per_day(9.99, cap()), CostCapDecision::Allow);
    }

    #[test]
    fn per_day_denies_at_or_over_cap() {
        assert_eq!(check_per_day(10.00, cap()), CostCapDecision::DenyPerDay);
        assert_eq!(check_per_day(12.50, cap()), CostCapDecision::DenyPerDay);
    }
}
```

- [ ] **Step 3: Run to verify it fails, then passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::cost`
Expected: FAIL first run only if the module wasn't registered — but since Step 1 + Step 2 are done together, it should compile and PASS. If it does not compile, fix the `mod.rs` registration.
Expected after: PASS (4 tests).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/runtime/cost.rs src-tauri/src/automation/runtime/mod.rs src-tauri/src/automation/runtime/run_session.rs
git commit -m "feat(automation): cost-cap helpers — per-run accumulator + per-day check"
```

### Task C3: Extend the `AutomationDelegate` struct

Add the LLM/tools/cost/session fields. This task only changes the struct + the test `make_delegate` helper; the new methods that use the fields come in C4–C6.

**Files:**
- Modify: `src-tauri/src/automation/runtime/execute.rs`

- [ ] **Step 1: Extend the struct**

In `src-tauri/src/automation/runtime/execute.rs`, replace the `AutomationDelegate` struct (lines 25–34) with:

```rust
/// LoopDelegate for automation runs — the first headless LoopDelegate.
///
/// Drives run_agentic_loop with no Tauri AppHandle: streaming uses a
/// NoopSink, the four Humane tools + the full base tool set are dispatched
/// here, and cost is bounded by a per-run cap. Terminal state lands in
/// `gate`; the transcript is persisted to agent_messages under `session_id`.
pub struct AutomationDelegate {
    pub spec_id: String,
    pub activity_id: String,
    /// The run's agent_session id — transcript rows are persisted under this.
    pub session_id: String,
    pub permissions: PermissionSet,
    pub memory: Arc<MemoryStore>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    /// Holds the terminal state once the run completes (report or escalation).
    pub gate: Arc<Mutex<Option<CompletionGate>>>,
    pub auto_continue: AutoContinueConfig,
    /// LLM provider resolved from the app's ProviderService.
    pub llm: Arc<dyn crate::llm::LlmProvider>,
    /// Model id for this run.
    pub model: String,
    /// Full base tool set + the four Humane tool schemas.
    pub tools: Arc<crate::agent::tools::ToolRegistry>,
    /// Per-run cost accumulator + cap.
    pub cost: Arc<crate::automation::runtime::cost::CostCapState>,
    /// Working directory the run operates in (file/edit/search base + shell cwd).
    pub workspace_root: std::path::PathBuf,
}
```

> Confirm the `ToolRegistry` path: `grep -rn "pub struct ToolRegistry" src-tauri/src/agent/tools/`. If it is `crate::agent::tools::registry::ToolRegistry` adjust the type path accordingly.

- [ ] **Step 2: Fix the test `make_delegate` helper**

In `execute.rs`'s `#[cfg(test)] mod tests`, `make_delegate` constructs the struct. Update it to supply the new fields. Add a minimal fake `LlmProvider` and an empty `ToolRegistry`:

```rust
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
            llm: crate::automation::runtime::execute::tests_support::fake_llm(),
            model: "claude-sonnet-4-6".to_string(),
            tools: crate::automation::runtime::execute::tests_support::empty_tool_registry(),
            cost: Arc::new(CostCapState::new(CostCapConfig {
                per_run_usd: 1.00,
                per_day_usd: 10.00,
            })),
            workspace_root: tmp.path().to_path_buf(),
        }
    }
```

Add a `tests_support` module inside the `tests` module providing `fake_llm()` and `empty_tool_registry()`:

```rust
    pub(super) mod tests_support {
        use std::sync::Arc;
        /// Minimal LlmProvider stub for tests that exercise execute_tool_calls
        /// (which never calls call_llm). All methods unimplemented!() — grep
        /// the LlmProvider trait in src-tauri/src/llm/mod.rs and stub every
        /// required method; mirror an existing test mock if one exists.
        pub fn fake_llm() -> Arc<dyn crate::llm::LlmProvider> {
            // The implementer fills this in by mirroring an existing
            // `impl LlmProvider for <TestMock>` in the codebase. If none
            // exists, define a local zero-method-behaviour struct here.
            unimplemented!("provide a no-op LlmProvider test stub")
        }
        pub fn empty_tool_registry() -> Arc<crate::agent::tools::ToolRegistry> {
            // Construct an empty ToolRegistry — check ToolRegistry's
            // constructor (grep `impl ToolRegistry`); likely
            // `Arc::new(ToolRegistry::new())` or `::default()`.
            unimplemented!("construct an empty ToolRegistry")
        }
    }
```

> The `unimplemented!()` stubs are acceptable here ONLY because the four existing `execute.rs` tests (`report_to_user_sets_gate_and_returns_response`, `memory_write_read_round_trip`, `request_escalation_inserts_row_and_sets_gate`, `permission_denial_pushes_error_result_and_continues`) never call `call_llm` and never dispatch a base tool — they only exercise the Humane-tool branches. The implementer must still make `fake_llm()` / `empty_tool_registry()` *construct* something (not panic) because they run at delegate-construction time. Mirror an existing mock — `grep -rn "impl crate::llm::LlmProvider\|impl LlmProvider for" src-tauri/src/` to find one.

- [ ] **Step 3: Verify the existing execute.rs tests still pass**

Run: `cd src-tauri && cargo test --lib automation::runtime::execute`
Expected: PASS — the four existing tests still green with the extended struct.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/automation/runtime/execute.rs
git commit -m "feat(automation): extend AutomationDelegate struct — llm/tools/cost/session"
```

### Task C4: Implement `AutomationDelegate::call_llm` + `on_usage` + `before_llm_call`

`call_llm` builds the message list from `reason_ctx` and delegates to `stream_completion` with a `NoopSink`. `on_usage` accumulates cost. `before_llm_call` aborts the run if the per-run cap is already exceeded.

**Files:**
- Modify: `src-tauri/src/automation/runtime/execute.rs`

- [ ] **Step 1: Write the failing test**

In `execute.rs` `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::execute::tests::before_llm_call_aborts`
Expected: FAIL — `before_llm_call` currently returns `None` unconditionally.

- [ ] **Step 3: Implement the three methods**

In `execute.rs`, in `impl crate::agent::types::LoopDelegate for AutomationDelegate`, replace the `before_llm_call` stub and `call_llm` stub, and add `on_usage`:

```rust
    async fn before_llm_call(
        &self,
        _reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Option<LoopOutcome> {
        // Per-run cost cap: if the accumulated cost already crossed the cap
        // (from a prior iteration's on_usage), abort before spending more.
        if self.cost.per_run_exceeded() {
            let msg = format!(
                "per-run cost cap exceeded (${:.4})",
                self.cost.total_usd()
            );
            tracing::warn!(spec_id = %self.spec_id, activity_id = %self.activity_id, "{}", msg);
            *self.gate.lock().await = Some(CompletionGate::ErrorTerminal(msg.clone()));
            return Some(LoopOutcome::Failure { error: msg });
        }
        None
    }

    async fn call_llm(
        &self,
        reason_ctx: &mut ReasoningContext,
        _iteration: usize,
    ) -> Result<RespondOutput, Error> {
        let mut messages = vec![ChatMessage::system(&reason_ctx.system_prompt)];
        messages.extend(reason_ctx.messages.clone());

        let tools = if reason_ctx.force_text {
            Vec::new()
        } else {
            self.tools.list_definitions()
        };
        let config = crate::llm::CompletionConfig {
            model: self.model.clone(),
            max_tokens: 8192,
            temperature: 0.7,
            system_prompt: Some(reason_ctx.system_prompt.clone()),
            thinking_enabled: false,
        };

        tracing::info!(
            spec_id = %self.spec_id,
            model = %self.model,
            message_count = messages.len(),
            tool_count = tools.len(),
            "automation run: calling LLM"
        );

        crate::agent::llm_stream::stream_completion(
            self.llm.as_ref(),
            messages,
            tools,
            &config,
            &crate::agent::llm_stream::NoopSink,
        )
        .await
    }

    async fn on_usage(
        &self,
        usage: &crate::agent::types::TokenUsage,
        _reason_ctx: &ReasoningContext,
    ) {
        let cost = crate::agent::types::calculate_cost(
            &self.model,
            usage.input_tokens,
            usage.output_tokens,
        );
        let total = self.cost.add(cost);
        tracing::debug!(
            spec_id = %self.spec_id,
            turn_cost_usd = cost,
            total_cost_usd = total,
            "automation run: cost accumulated"
        );
    }
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::execute`
Expected: PASS — including the two new tests and the four pre-existing ones.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/execute.rs
git commit -m "feat(automation): AutomationDelegate call_llm + on_usage + cost-cap guard"
```

### Task C5: Wire `execute_tool_calls` — base-tool fall-through, `notify_user`, `report_to_user` artifacts

Replace the `other =>` fall-through stub with real base-tool dispatch via `ToolRegistry`; wire `notify_user` to the outbound `ChannelManager`; persist `report_to_user.artifacts` into `report_artifacts_json`.

**Files:**
- Modify: `src-tauri/src/automation/runtime/execute.rs`
- Modify: `src-tauri/src/automation/tools/report_to_user.rs` (typed artifact shape)

- [ ] **Step 1: Add the typed artifact shape**

In `src-tauri/src/automation/tools/report_to_user.rs`, replace the `ReportInput` struct + add the artifact type:

```rust
// wired in Task 15 (AutomationDelegate)
#[derive(Debug, serde::Deserialize)]
pub struct ReportInput {
    pub text: String,
    pub outcome: String, // "useful" | "noop" | "error" | "skipped"
    #[serde(default)]
    pub artifacts: Vec<ReportArtifact>,
}

/// A declared product of an automation run. Persisted as JSON into
/// automation_activities.report_artifacts_json (V24).
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct ReportArtifact {
    /// "file" | "text" | "url"
    pub kind: String,
    /// Relative path under the spec's working dir, for kind = "file".
    #[serde(default)]
    pub path: Option<String>,
    pub title: String,
}
```

Update the `schema()` `artifacts` property to describe the shape (still permissive — the LLM sees a hint):

```rust
                "artifacts": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["kind", "title"],
                        "properties": {
                            "kind":  { "enum": ["file", "text", "url"] },
                            "path":  { "type": "string" },
                            "title": { "type": "string" }
                        }
                    }
                }
```

- [ ] **Step 2: Write the failing test**

In `execute.rs` `#[cfg(test)] mod tests`, add:

```rust
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
```

- [ ] **Step 3: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::execute::tests::report_to_user_persists_artifacts`
Expected: FAIL — the current `report_to_user` arm does not write `report_artifacts_json`.

- [ ] **Step 4: Update the `report_to_user` arm to persist artifacts**

In `execute.rs`, in `execute_tool_calls`, the `"report_to_user"` arm — replace the DB `UPDATE` block so it also writes `report_artifacts_json`:

```rust
                "report_to_user" => {
                    let input: ReportInput = serde_json::from_value(call.arguments.clone())?;
                    let artifacts_json = serde_json::to_string(&input.artifacts)
                        .unwrap_or_else(|_| "[]".into());
                    {
                        let conn = self.db.lock().unwrap();
                        conn.execute(
                            "UPDATE automation_activities \
                             SET status='completed', report_text=?1, report_outcome=?2, \
                                 report_artifacts_json=?3, completed_at=?4 \
                             WHERE id=?5",
                            rusqlite::params![
                                input.text,
                                input.outcome,
                                artifacts_json,
                                chrono::Utc::now().timestamp_millis(),
                                self.activity_id,
                            ],
                        )?;
                    }
                    *self.gate.lock().await = Some(CompletionGate::Reported {
                        text: input.text.clone(),
                        outcome: input.outcome.clone(),
                    });
                    tracing::info!(
                        spec_id = %self.spec_id,
                        activity_id = %self.activity_id,
                        outcome = %input.outcome,
                        artifact_count = input.artifacts.len(),
                        "automation run reported"
                    );
                    return Ok(Some(LoopOutcome::Response {
                        text: input.text,
                        usage: None,
                    }));
                }
```

- [ ] **Step 5: Wire `notify_user` to the outbound `ChannelManager`**

The `AutomationDelegate` does not currently hold a `ChannelManager`. For Phase 2a, `notify_user` is best-effort: log + (if a channel manager is reachable) broadcast. Since threading `Arc<RwLock<ChannelManager>>` into the delegate is a wider change, Phase 2a scope keeps `notify_user` as a structured log PLUS a `tracing` event that records intent, and defers real channel dispatch to 2b. Replace the `notify_user` arm's comment and keep the log, but make the log carry the channel list explicitly:

```rust
                "notify_user" => {
                    let input: NotifyInput = serde_json::from_value(call.arguments.clone())?;
                    // Phase 2a: structured log of the notification intent.
                    // Real channel dispatch (channels.rs broadcast) is wired
                    // in Phase 2b when the delegate gains a ChannelManager handle.
                    tracing::info!(
                        spec_id = %self.spec_id,
                        channels = ?input.channels,
                        title = %input.title,
                        level = %input.level,
                        "automation notify_user"
                    );
                    reason_ctx.messages.push(ChatMessage::user_tool_result(
                        &call.id,
                        "notification logged (channel dispatch lands in Phase 2b)",
                        false,
                    ));
                }
```

> Spec note: the design's §0.10 listed `notify_user` → `channels.rs` as in-2a-scope. During planning, threading `ChannelManager` into the headless delegate proved to be a wider `AppState`-plumbing change than the rest of 2a; this plan narrows 2a's `notify_user` to a structured-log stub and moves real dispatch to 2b alongside the chat/IM messaging system. Flag this scope narrowing in the PR description.

- [ ] **Step 6: Replace the `other =>` fall-through with real base-tool dispatch**

In `execute.rs`, in `execute_tool_calls`, replace the `other => { ... }` arm:

```rust
                other => {
                    // Dispatch to the base built-in tool set via ToolRegistry.
                    // Permission was already checked at the top of the loop.
                    match self.tools.execute(other, call.arguments.clone()).await {
                        Ok(result) => {
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &call.id, &result, false,
                            ));
                        }
                        Err(e) => {
                            reason_ctx.messages.push(ChatMessage::user_tool_result(
                                &call.id,
                                &format!("tool '{}' error: {}", other, e),
                                true,
                            ));
                        }
                    }
                }
```

> Confirm `ToolRegistry`'s execute method name + signature: `grep -rn "impl ToolRegistry" -A40 src-tauri/src/agent/tools/`. The dispatch method may be named `execute`, `dispatch`, or `call`, and may take `(&str, serde_json::Value)` or a `ToolCall`. Match the actual signature; the result type is likely `Result<String, _>`. If the registry's executor needs a workspace root / context, pass `self.workspace_root`.

- [ ] **Step 7: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::execute`
Expected: PASS — the new `report_to_user_persists_artifacts_json` plus all prior tests.
Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/automation/runtime/execute.rs src-tauri/src/automation/tools/report_to_user.rs
git commit -m "feat(automation): wire base-tool dispatch + report artifacts + notify_user log

execute_tool_calls now dispatches non-Humane tools through ToolRegistry,
report_to_user persists declared artifacts into report_artifacts_json,
and notify_user emits a structured intent log (channel dispatch → 2b)."
```

### Task C6: Persist the transcript to `agent_messages` (`after_iteration` / `handle_text_response`)

The run-session's transcript must land in `agent_messages`. The simplest correct approach: after the loop ends, the caller (`execute_run`, Task D3) persists `reason_ctx.messages` in bulk. But to keep the delegate self-contained and the data fresh mid-run, the delegate persists incrementally. Phase 2a uses **bulk persist by the caller after the loop** (simpler, one code path, no partial-write races) — so `after_iteration` / `handle_text_response` keep their default/`Continue` behaviour and this task only adds the persistence helper used by D3.

**Files:**
- This task is folded into Task D2's `run_session.rs` (the `persist_transcript` function). No separate work here — it is called out so the plan's reader knows transcript persistence is **caller-side, post-loop**, not delegate-side. Skip to Milestone D.

---

## Milestone D — `execute_run` Wiring, Home Space, Memory, Retention

### Task D1: `AppRuntimeService` gains `provider_service`

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs`
- Modify: `src-tauri/src/app.rs`

- [ ] **Step 1: Add the field + constructor param**

In `src-tauri/src/automation/runtime/service.rs`, add to the `AppRuntimeService` struct (after `pub memory: Arc<AutomationMemoryStore>,`):

```rust
    /// Automation-scoped file-based memory store.
    pub memory: Arc<AutomationMemoryStore>,
    /// Provider service — resolves the LlmProvider + model for a run.
    pub provider_service: Arc<crate::providers::ProviderService>,
```

> Confirm the `ProviderService` path: `grep -rn "pub struct ProviderService" src-tauri/src/`. Adjust `crate::providers::ProviderService` if it differs.

In `AppRuntimeService::new`, add a `provider_service` parameter (after `memory`) and field-init it:

```rust
    pub fn new(
        db: Arc<StdMutex<rusqlite::Connection>>,
        schedule: Arc<ScheduleSource>,
        file: Arc<FileSource>,
        webhook: Arc<WebhookSource>,
        webpage: Arc<WebpageSource>,
        rss: Arc<RssSource>,
        wecom: Arc<WecomSource>,
        custom: Arc<CustomSource>,
        infra: Arc<InfraService>,
        memory: Arc<AutomationMemoryStore>,
        provider_service: Arc<crate::providers::ProviderService>,
    ) -> Arc<Self> {
        let svc = Arc::new(Self {
            db,
            schedule,
            file,
            webhook,
            webpage,
            rss,
            wecom,
            custom,
            infra,
            memory,
            provider_service,
            semaphores: Arc::new(RwLock::new(HashMap::new())),
            attached: Arc::new(TokioMutex::new(HashMap::new())),
            status: Arc::new(StdMutex::new(ServiceStatus::Stopped)),
            started_at: Arc::new(StdMutex::new(None)),
            self_weak: OnceLock::new(),
        });
        let _ = svc.self_weak.set(Arc::downgrade(&svc));
        svc
    }
```

- [ ] **Step 2: Update the `app.rs` construction site**

In `src-tauri/src/app.rs`, the `runtime_service` construction block (lines ~420–439) — add `provider_service.clone()` as the final argument to `AppRuntimeService::new(...)`:

```rust
            AppRuntimeService::new(
                db.clone(),
                Arc::new(ScheduleSource::new()),
                Arc::new(FileSource::new()),
                Arc::new(WebhookSource::with_global_registry()),
                Arc::new(WebpageSource::new()),
                Arc::new(RssSource::new()),
                Arc::new(WecomSource::new()),
                Arc::new(CustomSource::new()),
                infra_service.clone(),
                Arc::new(AutomationMemoryStore::new(automation_memory_root)),
                provider_service.clone(),
            )
```

> `provider_service` is constructed at `app.rs:348` (`let provider_service = Arc::new(ProviderService::new(&data_dir)?);`), which is *before* the `runtime_service` block at ~420 — so `provider_service.clone()` is in scope. Verify ordering holds after any rebase.

- [ ] **Step 3: Update the `service.rs` test `make_service` helper**

In `service.rs`'s `#[cfg(test)] mod tests`, `make_service` calls `AppRuntimeService::new(...)`. Add a `ProviderService` test instance as the final arg:

```rust
            Arc::new(InfraService::new()),
            Arc::new(crate::automation::memory::MemoryStore::new(memory_root)),
            Arc::new(crate::providers::ProviderService::new(&tmp).expect("test provider service")),
        )
```

> If `ProviderService::new` requires a populated config dir or returns an error in a bare temp dir, mirror however other tests construct a `ProviderService` — `grep -rn "ProviderService::new\|ProviderService::" src-tauri/src/ | grep -i test`. If no test constructor exists, add a `ProviderService::new_for_test()` or use `Default` — pick the lowest-friction path and note it in the commit.

- [ ] **Step 4: Verify build + tests**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib automation::runtime::service`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs src-tauri/src/app.rs
git commit -m "feat(automation): AppRuntimeService gains provider_service handle"
```

### Task D2: `run_session.rs` — home space, run-session creation, transcript persistence, retention

**Files:**
- Modify: `src-tauri/src/automation/runtime/run_session.rs` (created empty in Task C2)

- [ ] **Step 1: Write the failing tests**

Replace `src-tauri/src/automation/runtime/run_session.rs` with a module containing the test harness + stub signatures:

```rust
//! Run-session lifecycle for automation runs (Phase 2a, design §0).
//!
//! A run IS an agent_session. This module owns: ensuring the shared
//! "Automations" home space exists, creating the per-run agent_session row
//! (with origin + prev_run chain metadata), persisting the loop transcript
//! into agent_messages, and pruning old run-sessions per spec.

use crate::agent::types::{ChatMessage, ContentBlock, MessageRole};
use rusqlite::Connection;

/// The fixed id of the auto-created shared "Automations" home space.
pub const AUTOMATIONS_SPACE_ID: &str = "automations";

/// Ensure the shared "Automations" space row exists (idempotent).
pub fn ensure_automations_space(conn: &Connection) -> rusqlite::Result<()> {
    todo!("Step 3")
}

/// Resolve a spec's home space id: the spec's space_id if set, else the
/// shared "Automations" space.
pub fn resolve_home_space(conn: &Connection, spec_id: &str) -> rusqlite::Result<String> {
    todo!("Step 3")
}

/// Create the agent_session row for a run. `prev_run_session_id` chains
/// run history. Returns the new session id.
pub fn create_run_session(
    conn: &Connection,
    spec_id: &str,
    space_id: &str,
    trigger_tag: &str,
    activity_id: &str,
) -> rusqlite::Result<String> {
    todo!("Step 3")
}

/// Persist a finished run's transcript into agent_messages (bulk, post-loop).
pub fn persist_transcript(
    conn: &Connection,
    session_id: &str,
    messages: &[ChatMessage],
) -> rusqlite::Result<()> {
    todo!("Step 3")
}

/// Prune old run-sessions for a spec, keeping the most recent `keep` runs.
/// Deletes the agent_messages + agent_session rows of older runs and NULLs
/// their automation_activities.session_id. The ledger row is never deleted.
/// Returns the number of run-sessions pruned.
pub fn prune_old_run_sessions(
    conn: &Connection,
    spec_id: &str,
    keep: u32,
) -> rusqlite::Result<usize> {
    todo!("Step 3")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn insert_spec(conn: &Connection, id: &str, space_id: Option<&str>) {
        conn.execute(
            "INSERT INTO automation_specs
             (id, name, version, author, description, system_prompt,
              spec_yaml, spec_json, space_id, enabled, created_at, updated_at)
             VALUES (?1,'t','1.0','a','d','sys','','{}',?2,1,1,1)",
            rusqlite::params![id, space_id],
        ).unwrap();
    }

    #[test]
    fn ensure_automations_space_is_idempotent() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        ensure_automations_space(&conn).unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM spaces WHERE id = ?1",
            [AUTOMATIONS_SPACE_ID], |r| r.get(0)).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn resolve_home_space_uses_spec_space_else_automations() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        // Spec with an explicit space.
        conn.execute(
            "INSERT INTO spaces (id, name, created_at, updated_at)
             VALUES ('proj', 'Project', datetime('now'), datetime('now'))", []).unwrap();
        insert_spec(&conn, "spec-with-space", Some("proj"));
        insert_spec(&conn, "spec-no-space", None);
        assert_eq!(resolve_home_space(&conn, "spec-with-space").unwrap(), "proj");
        assert_eq!(resolve_home_space(&conn, "spec-no-space").unwrap(), AUTOMATIONS_SPACE_ID);
    }

    #[test]
    fn create_run_session_chains_prev_run() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        let s1 = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-1").unwrap();
        let s2 = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-2").unwrap();
        assert_ne!(s1, s2);
        // s2's metadata should reference s1 as prev_run_session_id.
        let meta: String = conn.query_row(
            "SELECT metadata_json FROM agent_sessions WHERE id = ?1",
            [&s2], |r| r.get(0)).unwrap();
        assert!(meta.contains(&s1), "s2 metadata should chain to s1");
        assert!(meta.contains("automation:manual"), "origin should be recorded");
    }

    #[test]
    fn persist_transcript_writes_agent_messages() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        let sid = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", "act-1").unwrap();
        let msgs = vec![
            ChatMessage::user("trigger"),
            ChatMessage::assistant("did the thing"),
        ];
        persist_transcript(&conn, &sid, &msgs).unwrap();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_messages WHERE session_id = ?1",
            [&sid], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn prune_keeps_most_recent_n_and_nulls_ledger_link() {
        let conn = db();
        ensure_automations_space(&conn).unwrap();
        insert_spec(&conn, "s", None);
        // 3 runs; keep 2.
        let mut sessions = vec![];
        for i in 0..3 {
            let act = format!("act-{}", i);
            conn.execute(
                "INSERT INTO automation_activities
                 (id, spec_id, trigger_source_type, trigger_payload_json, status, queued_at)
                 VALUES (?1, 's', 'manual', '{}', 'completed', ?2)",
                rusqlite::params![act, i as i64]).unwrap();
            let sid = create_run_session(&conn, "s", AUTOMATIONS_SPACE_ID, "manual", &act).unwrap();
            conn.execute(
                "UPDATE automation_activities SET session_id = ?1 WHERE id = ?2",
                rusqlite::params![sid, act]).unwrap();
            persist_transcript(&conn, &sid, &[ChatMessage::user("x")]).unwrap();
            sessions.push(sid);
        }
        let pruned = prune_old_run_sessions(&conn, "s", 2).unwrap();
        assert_eq!(pruned, 1);
        // Oldest session row gone.
        let gone: i64 = conn.query_row(
            "SELECT COUNT(*) FROM agent_sessions WHERE id = ?1",
            [&sessions[0]], |r| r.get(0)).unwrap();
        assert_eq!(gone, 0);
        // But its ledger row survives with session_id NULLed.
        let link: Option<String> = conn.query_row(
            "SELECT session_id FROM automation_activities WHERE id = 'act-0'",
            [], |r| r.get(0)).unwrap();
        assert!(link.is_none(), "pruned run's ledger link should be NULL");
        let ledger_alive: i64 = conn.query_row(
            "SELECT COUNT(*) FROM automation_activities WHERE id = 'act-0'",
            [], |r| r.get(0)).unwrap();
        assert_eq!(ledger_alive, 1, "ledger row must never be deleted");
    }
}
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::run_session`
Expected: FAIL — `todo!()` panics.

- [ ] **Step 3: Implement the five functions**

Replace each `todo!(...)` body:

```rust
pub fn ensure_automations_space(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO spaces (id, name, icon, path, created_at, updated_at)
         VALUES (?1, 'Automations', '🤖', NULL, datetime('now'), datetime('now'))",
        [AUTOMATIONS_SPACE_ID],
    )?;
    Ok(())
}

pub fn resolve_home_space(conn: &Connection, spec_id: &str) -> rusqlite::Result<String> {
    let space_id: Option<String> = conn.query_row(
        "SELECT space_id FROM automation_specs WHERE id = ?1",
        [spec_id],
        |r| r.get(0),
    )?;
    Ok(space_id.filter(|s| !s.is_empty()).unwrap_or_else(|| AUTOMATIONS_SPACE_ID.to_string()))
}

pub fn create_run_session(
    conn: &Connection,
    spec_id: &str,
    space_id: &str,
    trigger_tag: &str,
    activity_id: &str,
) -> rusqlite::Result<String> {
    let session_id = uuid::Uuid::new_v4().to_string();
    let now_ms = chrono::Utc::now().timestamp_millis();

    // Find this spec's most recent prior run-session to chain from.
    let prev: Option<String> = conn.query_row(
        "SELECT s.id FROM agent_sessions s
         JOIN automation_activities a ON a.session_id = s.id
         WHERE a.spec_id = ?1 AND s.id != ?2
         ORDER BY s.created_at DESC LIMIT 1",
        rusqlite::params![spec_id, session_id],
        |r| r.get(0),
    ).ok();

    let metadata = serde_json::json!({
        "origin": format!("automation:{}", trigger_tag),
        "spec_id": spec_id,
        "activity_id": activity_id,
        "prev_run_session_id": prev,
    });
    let title = format!("Automation run ({})", trigger_tag);

    conn.execute(
        "INSERT INTO agent_sessions
         (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at)
         VALUES (?1, ?2, ?3, ?4, 0, 0, 0, ?5, ?5)",
        rusqlite::params![session_id, space_id, title, metadata.to_string(), now_ms],
    )?;
    Ok(session_id)
}

pub fn persist_transcript(
    conn: &Connection,
    session_id: &str,
    messages: &[ChatMessage],
) -> rusqlite::Result<()> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    for (idx, msg) in messages.iter().enumerate() {
        let role = match msg.role {
            MessageRole::System => "system",
            MessageRole::User => "user",
            MessageRole::Assistant => "assistant",
        };
        // Serialize the content blocks as JSON — agent_messages.content is
        // the same JSON-array shape the chat path uses.
        let content = serde_json::to_string(&msg.content).unwrap_or_else(|_| "[]".into());
        let id = format!("{}-{}", session_id, idx);
        conn.execute(
            "INSERT INTO agent_messages (id, session_id, role, content, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, session_id, role, content, now_ms + idx as i64],
        )?;
    }
    conn.execute(
        "UPDATE agent_sessions SET message_count = ?1, updated_at = ?2 WHERE id = ?3",
        rusqlite::params![messages.len() as i64, now_ms, session_id],
    )?;
    Ok(())
}

pub fn prune_old_run_sessions(
    conn: &Connection,
    spec_id: &str,
    keep: u32,
) -> rusqlite::Result<usize> {
    // Run-sessions for this spec, newest first.
    let mut stmt = conn.prepare(
        "SELECT s.id FROM agent_sessions s
         JOIN automation_activities a ON a.session_id = s.id
         WHERE a.spec_id = ?1
         ORDER BY s.created_at DESC",
    )?;
    let ids: Vec<String> = stmt
        .query_map([spec_id], |r| r.get(0))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let to_prune = if ids.len() as u32 > keep {
        &ids[keep as usize..]
    } else {
        &[]
    };

    for sid in to_prune {
        // NULL the ledger link first (ledger row itself is never deleted).
        conn.execute(
            "UPDATE automation_activities SET session_id = NULL WHERE session_id = ?1",
            [sid],
        )?;
        // agent_messages FK is ON DELETE CASCADE, but delete explicitly so
        // the behaviour does not depend on PRAGMA foreign_keys being on.
        conn.execute("DELETE FROM agent_messages WHERE session_id = ?1", [sid])?;
        conn.execute("DELETE FROM agent_sessions WHERE id = ?1", [sid])?;
    }
    Ok(to_prune.len())
}
```

> Two integration points to verify against the live schema:
> 1. `agent_messages.content` — confirm the column stores a JSON array of content blocks (it does for the chat path). If the agent UI expects a different shape for `role='assistant'` rows, mirror what `create_agent_session` + the chat message-persist path write. `grep -rn "INSERT INTO agent_messages" src-tauri/src/`.
> 2. `agent_sessions` columns — the `INSERT` above uses the V8 base columns. If V5/V17/V18 added NOT-NULL columns without defaults (e.g. `workspace_id`, `attached_dirs`), the INSERT will fail. `grep -rn "INSERT INTO agent_sessions" src-tauri/src/` and mirror the most complete existing insert (the `create_agent_session` Tauri command).

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::run_session`
Expected: PASS (5 tests).

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/run_session.rs
git commit -m "feat(automation): run_session — home space, session creation, transcript, retention"
```

### Task D3: Tier 1 memory pre-load in `prompt.rs`

**Files:**
- Modify: `src-tauri/src/automation/runtime/prompt.rs`

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/automation/runtime/prompt.rs`, in `#[cfg(test)] mod tests`, add:

```rust
    #[test]
    fn initial_message_includes_memory_block_when_present() {
        let m = build_initial_message_with_memory(
            None, &json!({}), &json!({}), None, "remembered: the API key rotates monthly",
        );
        assert!(m.contains("## Memory"));
        assert!(m.contains("API key rotates monthly"));
        assert!(m.contains("## Trigger"));
    }

    #[test]
    fn initial_message_omits_memory_block_when_empty() {
        let m = build_initial_message_with_memory(None, &json!({}), &json!({}), None, "");
        assert!(!m.contains("## Memory"));
        assert!(m.contains("## Trigger"));
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::prompt`
Expected: FAIL — `build_initial_message_with_memory` not found.

- [ ] **Step 3: Add `build_initial_message_with_memory`**

In `src-tauri/src/automation/runtime/prompt.rs`, add (keep the existing `build_initial_message` — the new fn wraps it):

```rust
/// Like `build_initial_message`, but prepends a `## Memory` block with the
/// spec's Tier-1 persistent memory so the agent starts the run already
/// knowing its accumulated context (design §0.8). An empty `memory` string
/// produces no block.
pub fn build_initial_message_with_memory(
    subscription: Option<&Subscription>,
    trigger_payload: &serde_json::Value,
    user_config: &serde_json::Value,
    resumption: Option<&EscalationResolution>,
    memory: &str,
) -> String {
    let base = build_initial_message(subscription, trigger_payload, user_config, resumption);
    if memory.trim().is_empty() {
        base
    } else {
        format!("## Memory\n{}\n\n{}", memory.trim(), base)
    }
}
```

Add `build_initial_message_with_memory` to the `pub use prompt::{...}` re-export in `src-tauri/src/automation/runtime/mod.rs`:

```rust
pub use prompt::{build_initial_message, build_initial_message_with_memory, build_system_prompt, EscalationResolution};
```

- [ ] **Step 4: Run to verify it passes**

Run: `cd src-tauri && cargo test --lib automation::runtime::prompt`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/automation/runtime/prompt.rs src-tauri/src/automation/runtime/mod.rs
git commit -m "feat(automation): pre-load Tier 1 memory into the run's initial message"
```

### Task D4: Replace the `deferred_phase_2` stub with `run_agentic_loop`

This is the keystone task — `execute_run` now: checks the per-day cost cap, ensures the home space, creates the run-session, builds the delegate, runs the loop, maps the `CompletionGate` to the activity row, persists the transcript, and prunes old run-sessions.

**Files:**
- Modify: `src-tauri/src/automation/runtime/service.rs`

- [ ] **Step 1: Add the per-day cost helper to `cost.rs`**

In `src-tauri/src/automation/runtime/cost.rs`, add (above the tests module):

```rust
/// Sum of today's (UTC) automation cost from cost_records. Automation runs
/// record cost under their run-session id; we sum every cost_record from
/// the UTC day start. Best-effort — returns 0.0 on any error.
pub fn day_total_usd(conn: &rusqlite::Connection) -> f64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    let day_start = Utc
        .with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0);
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
        [day_start],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(0.0)
}
```

> Phase 2a note: this sums *all* `cost_records` for the day, not just automation-origin ones. A precise per-origin split needs a join from `cost_records.session_id` → `agent_sessions.metadata_json` origin, which is a 2b refinement. The day cap as a global automation+chat budget is acceptable for 2a; note it in the PR.

- [ ] **Step 2: Write the failing test**

In `service.rs`'s `#[cfg(test)] mod tests`, add:

```rust
    #[tokio::test]
    async fn execute_run_creates_run_session_and_links_activity() {
        let conn = open_test_db();
        insert_test_spec(&conn, "rs", minimal_spec_json());
        let svc = make_service(conn);
        svc.activate("rs").await.unwrap();
        svc.execute_run("rs", None, serde_json::json!({"trigger": "manual"}))
            .await
            .unwrap();

        let (status, session_id): (String, Option<String>) = {
            let db = svc.db.lock().unwrap();
            db.query_row(
                "SELECT status, session_id FROM automation_activities WHERE spec_id = 'rs'",
                [], |r| Ok((r.get(0)?, r.get(1)?)),
            ).unwrap()
        };
        // The run executes (no longer 'deferred_phase_2'). With the test's
        // fake provider it may end 'completed' or 'failed' — but it must
        // have a linked run-session either way.
        assert_ne!(status, "deferred_phase_2", "execute_run must invoke the loop");
        assert!(session_id.is_some(), "run-session must be created and linked");
    }
```

> This test requires `make_service` to build an `AppRuntimeService` whose `provider_service` yields a usable (even if minimal) `LlmProvider`. If the test provider cannot produce a real stream, the loop will end in `failed` — that is fine for this assertion (it checks the *wiring*, not a successful agent run). If `ProviderService` has no test-friendly constructor, this test may need `#[ignore]` with a comment, and the wiring is then verified by the C-milestone unit tests + manual `cargo tauri dev`. Decide based on what `ProviderService` actually supports; prefer a real assertion if possible.

- [ ] **Step 3: Run to verify it fails**

Run: `cd src-tauri && cargo test --lib automation::runtime::service::tests::execute_run_creates_run_session`
Expected: FAIL — `status` is still `"deferred_phase_2"`.

> Note: the existing `execute_run_inserts_activity_row` test asserts `status == "deferred_phase_2"` — that assertion is now wrong. Update it in this step to assert `status != "deferred_phase_2"` (or delete it, since the new test supersedes it). Update `execute_run_filters_out_when_filter_rejects` only if needed — it should still pass (filtering happens before the loop).

- [ ] **Step 4: Replace the stub in `execute_run`**

In `src-tauri/src/automation/runtime/service.rs`, in `execute_run`, replace the Phase 1 stub block (lines 426–443, from `// ── 5. acquire per-spec semaphore` through `Ok(())`) with:

```rust
        // ── 5. acquire per-spec semaphore ────────────────────────────────────
        let sem = self.semaphore_for(spec_id).await;
        let _permit = sem.acquire().await.map_err(|e| anyhow::anyhow!("semaphore: {}", e))?;

        // ── 6. cost guardrails — per-day cap check before starting ──────────
        let cost_cap = crate::automation::runtime::cost::CostCapConfig {
            per_run_usd: 1.00,  // overridden below from config if available
            per_day_usd: 10.00,
        };
        // NOTE: read the real caps from MemubotConfig.automation. AppRuntimeService
        // does not currently hold MemubotConfig — Phase 2a threads it the same way
        // provider_service was threaded (Task D1 pattern). If that plumbing is not
        // done, use AutomationConfig::default() values here and leave a TODO.
        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            let day_total = crate::automation::runtime::cost::day_total_usd(&conn);
            if crate::automation::runtime::cost::check_per_day(day_total, cost_cap)
                == crate::automation::runtime::cost::CostCapDecision::DenyPerDay
            {
                drop(conn);
                tracing::warn!(
                    spec_id, day_total_usd = day_total,
                    "automation run skipped — per-day cost cap reached"
                );
                self.update_activity_status(
                    &activity_id, "failed",
                    Some("per-day cost cap reached"),
                )?;
                return Ok(());
            }
        }

        // ── 7. create the run-session (design §0) ────────────────────────────
        let trigger_tag = trigger_source.as_db_str();
        let (session_id, workspace_root) = {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            crate::automation::runtime::run_session::ensure_automations_space(&conn)?;
            let space_id = crate::automation::runtime::run_session::resolve_home_space(&conn, spec_id)?;
            let session_id = crate::automation::runtime::run_session::create_run_session(
                &conn, spec_id, &space_id, trigger_tag, &activity_id,
            )?;
            // Link the ledger row to the run-session + mark running.
            conn.execute(
                "UPDATE automation_activities SET session_id = ?1, status = 'running', started_at = ?2 WHERE id = ?3",
                rusqlite::params![session_id, chrono::Utc::now().timestamp_millis(), activity_id],
            )?;
            // Resolve workspace_root: space.path else per-spec scratch dir.
            let space_path: Option<String> = conn.query_row(
                "SELECT path FROM spaces WHERE id = ?1", [&space_id], |r| r.get(0),
            ).ok().flatten();
            let workspace_root = match space_path.filter(|p| !p.is_empty()) {
                Some(p) => std::path::PathBuf::from(p),
                None => {
                    let dir = dirs::home_dir()
                        .unwrap_or_default()
                        .join("Documents/workground/automations")
                        .join(spec_id);
                    let _ = std::fs::create_dir_all(&dir);
                    dir
                }
            };
            (session_id, workspace_root)
        };

        // ── 8. resolve LLM provider + model ──────────────────────────────────
        // Mirror how ChatDelegate's caller resolves the provider+model from
        // ProviderService — grep `provider_service` usage in tauri_commands.rs
        // (send_agent_message path) for the exact call. The shape is roughly:
        //   let (llm, model) = self.provider_service.resolve_default()?;
        let (llm, model) = self.provider_service
            .resolve_default_for_agent()
            .map_err(|e| anyhow::anyhow!("resolve provider: {}", e))?;

        // ── 9. build the delegate + reasoning context ────────────────────────
        let spec_for_run = spec; // parsed HumaneAutomationSpec from step 3
        let memory_text = self.memory.read(spec_id).await.unwrap_or_default();
        let system_prompt = crate::automation::runtime::prompt::build_system_prompt(&spec_for_run);
        let initial_message = crate::automation::runtime::prompt::build_initial_message_with_memory(
            None, &payload, &serde_json::json!({}), None, &memory_text,
        );

        let mut reason_ctx = crate::agent::types::ReasoningContext::new(system_prompt);
        reason_ctx.messages.push(crate::agent::types::ChatMessage::user(&initial_message));

        let tools = self.build_automation_tool_registry(&workspace_root);
        let delegate = crate::automation::runtime::execute::AutomationDelegate {
            spec_id: spec_id.to_string(),
            activity_id: activity_id.clone(),
            session_id: session_id.clone(),
            permissions: self.load_permission_set(spec_id)?,
            memory: self.memory.clone(),
            db: self.db.clone(),
            gate: std::sync::Arc::new(tokio::sync::Mutex::new(None)),
            auto_continue: crate::automation::runtime::AutoContinueConfig::default(),
            llm,
            model,
            tools,
            cost: std::sync::Arc::new(
                crate::automation::runtime::cost::CostCapState::new(cost_cap),
            ),
            workspace_root,
        };

        // ── 10. run the agentic loop ─────────────────────────────────────────
        let loop_config = crate::agent::types::AgenticLoopConfig::default();
        let outcome = crate::agent::agentic_loop::run_agentic_loop(
            &delegate, &mut reason_ctx, &loop_config,
        ).await;

        // ── 11. map terminal state → activity row ────────────────────────────
        let gate = delegate.gate.lock().await.clone();
        {
            let conn = self.db.lock().map_err(|e| anyhow::anyhow!("db lock: {}", e))?;
            // Persist the transcript regardless of outcome.
            crate::automation::runtime::run_session::persist_transcript(
                &conn, &session_id, &reason_ctx.messages,
            )?;
            match &gate {
                Some(crate::automation::runtime::CompletionGate::Reported { .. }) => {
                    // report_to_user already set status='completed' in execute_tool_calls.
                }
                Some(crate::automation::runtime::CompletionGate::Escalated { escalation_id }) => {
                    conn.execute(
                        "UPDATE automation_activities SET status='waiting_user', escalation_id=?1 WHERE id=?2",
                        rusqlite::params![escalation_id, activity_id],
                    )?;
                }
                Some(crate::automation::runtime::CompletionGate::ErrorTerminal(msg)) => {
                    conn.execute(
                        "UPDATE automation_activities SET status='failed', error_text=?1, completed_at=?2 WHERE id=?3",
                        rusqlite::params![msg, chrono::Utc::now().timestamp_millis(), activity_id],
                    )?;
                }
                Some(crate::automation::runtime::CompletionGate::LoopExhausted) | None => {
                    // Loop ended without the agent calling report_to_user.
                    let err = match &outcome {
                        crate::agent::types::LoopOutcome::Failure { error } => error.clone(),
                        crate::agent::types::LoopOutcome::MaxIterations => "loop reached max iterations without report_to_user".to_string(),
                        other => format!("loop ended without report: {:?}", other),
                    };
                    conn.execute(
                        "UPDATE automation_activities SET status='failed', error_text=?1, completed_at=?2 WHERE id=?3",
                        rusqlite::params![err, chrono::Utc::now().timestamp_millis(), activity_id],
                    )?;
                }
            }
            // ── 12. retention prune (design §0.4) ────────────────────────────
            let keep = 50u32; // from MemubotConfig.automation.retention_runs_per_spec
            if let Err(e) = crate::automation::runtime::run_session::prune_old_run_sessions(
                &conn, spec_id, keep,
            ) {
                tracing::warn!(spec_id, "retention prune failed: {}", e);
            }
        }

        tracing::info!(spec_id, activity_id, session_id, ?outcome, "automation run finished");
        // ── 13. semaphore released (via _permit Drop) ────────────────────────
        Ok(())
```

> Three helper methods referenced above do not exist yet on `AppRuntimeService` and must be added in this task as small private helpers (they are mechanical):
> - `load_permission_set(&self, spec_id) -> anyhow::Result<PermissionSet>` — reads `permissions_granted` / `permissions_denied` (JSON arrays) + the spec's declared permissions from `automation_specs`, returns a `PermissionSet { spec, granted, denied }`. The columns + parse logic already exist in `set_permission` (lines 630–647) — mirror the parse, map strings → `Permission` via `serde_json::from_value` or the enum's `FromStr` (grep `enum Permission` in `automation/protocol/humane_v1.rs` for the deser path).
> - `build_automation_tool_registry(&self, workspace_root: &Path) -> Arc<ToolRegistry>` — constructs a `ToolRegistry` with the full base tool set rooted at `workspace_root` PLUS the four Humane tool schemas. Mirror how `ChatDelegate`'s caller builds its `ToolRegistry` (grep `ToolRegistry::` in `tauri_commands.rs`); append `crate::automation::tools::humane_tool_schemas()`.
> - `provider_service.resolve_default_for_agent()` — the method name is illustrative. Find the real one: grep how the agent send-message path resolves `(Arc<dyn LlmProvider>, String)` from `ProviderService` and call that. If it needs a model id argument, read it from the spec or the user config; default to the app's configured agent model.
>
> If any of these prove larger than a mechanical helper, the implementer should report BLOCKED with specifics rather than guessing.

- [ ] **Step 5: Run to verify it passes + full build**

Run: `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head`
Expected: no output.
Run: `cd src-tauri && cargo test --lib automation`
Expected: PASS — `execute_run_creates_run_session_and_links_activity` plus all prior automation tests (with the updated `execute_run_inserts_activity_row` assertion).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/runtime/service.rs src-tauri/src/automation/runtime/cost.rs
git commit -m "feat(automation): execute_run invokes run_agentic_loop — the execution wall

Replaces the deferred_phase_2 stub: per-day cost cap check, home-space
ensure, run-session creation + ledger link, AutomationDelegate build,
run_agentic_loop, CompletionGate → activity status mapping, transcript
persist, retention prune."
```

### Task D5: Fill in `compact.rs` retention + `automation_memory` bookkeeping

`compact.rs` is currently a 3-line no-op comment. The retention prune itself lives in `run_session.rs` (Task D2) and is already called from `execute_run` (Task D4). This task makes `compact.rs` the home for the `automation_memory` table bookkeeping that `MemoryStore::compact` should record.

**Files:**
- Modify: `src-tauri/src/automation/memory/compact.rs`
- Modify: `src-tauri/src/automation/memory/mod.rs` (export)

- [ ] **Step 1: Write the failing test**

Replace `src-tauri/src/automation/memory/compact.rs` with:

```rust
//! automation_memory table bookkeeping (V21 table). MemoryStore::compact
//! does the file-side work (rename memory.md → archives/{ISO8601}.md); this
//! module records the archive in the DB so the UI / future promotion logic
//! can see the compaction history.

use rusqlite::Connection;

/// Record a compaction: append `archive_path` to compacted_archives_json and
/// refresh last_updated_at. Idempotent-insert (UPSERT) on spec_id.
pub fn record_compaction(
    conn: &Connection,
    spec_id: &str,
    archive_path: &str,
) -> rusqlite::Result<()> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let existing: Option<String> = conn.query_row(
        "SELECT compacted_archives_json FROM automation_memory WHERE spec_id = ?1",
        [spec_id],
        |r| r.get(0),
    ).ok();

    let mut archives: Vec<String> = existing
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();
    archives.push(archive_path.to_string());
    let archives_json = serde_json::to_string(&archives).unwrap_or_else(|_| "[]".into());

    conn.execute(
        "INSERT INTO automation_memory (spec_id, last_updated_at, compacted_archives_json, bytes)
         VALUES (?1, ?2, ?3, 0)
         ON CONFLICT(spec_id) DO UPDATE SET
            last_updated_at = ?2,
            compacted_archives_json = ?3",
        rusqlite::params![spec_id, now_ms, archives_json],
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        // automation_memory FKs to automation_specs.
        conn.execute(
            "INSERT INTO automation_specs
             (id, name, version, author, description, system_prompt,
              spec_yaml, spec_json, enabled, created_at, updated_at)
             VALUES ('s','t','1.0','a','d','sys','','{}',1,1,1)", []).unwrap();
        conn
    }

    #[test]
    fn record_compaction_appends_archive() {
        let conn = db();
        record_compaction(&conn, "s", "archives/2026-05-14T00-00-00Z.md").unwrap();
        record_compaction(&conn, "s", "archives/2026-05-15T00-00-00Z.md").unwrap();
        let json: String = conn.query_row(
            "SELECT compacted_archives_json FROM automation_memory WHERE spec_id = 's'",
            [], |r| r.get(0)).unwrap();
        let archives: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(archives.len(), 2);
        assert!(archives[1].contains("2026-05-15"));
    }
}
```

- [ ] **Step 2: Register the function**

In `src-tauri/src/automation/memory/mod.rs`:

```rust
pub mod store;
pub mod compact;
pub use store::MemoryStore;
pub use compact::record_compaction;
```

- [ ] **Step 3: Run to verify it fails, then passes**

Run: `cd src-tauri && cargo test --lib automation::memory::compact`
Expected: PASS (`record_compaction_appends_archive`). If it fails to compile, fix the `mod.rs` export.

- [ ] **Step 4: Call `record_compaction` from the `memory` tool's `compact` op**

In `src-tauri/src/automation/runtime/execute.rs`, in `execute_tool_calls`, the `"memory"` arm's `"compact"` branch — after the `compact` call, record it:

```rust
                        "compact" => {
                            let p = self.memory.compact(&self.spec_id).await?;
                            let path_str = p.to_string_lossy().into_owned();
                            {
                                let conn = self.db.lock().unwrap();
                                let _ = crate::automation::memory::record_compaction(
                                    &conn, &self.spec_id, &path_str,
                                );
                            }
                            path_str
                        }
```

- [ ] **Step 5: Run the affected tests**

Run: `cd src-tauri && cargo test --lib automation::memory && cargo test --lib automation::runtime::execute`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/automation/memory/compact.rs src-tauri/src/automation/memory/mod.rs src-tauri/src/automation/runtime/execute.rs
git commit -m "feat(automation): record memory compactions in the automation_memory table"
```

---

## Milestone E — Frontend

### Task E1: AutomationHub activity row → opens the run-session in the Agent view

**Files:**
- Modify: `ui/src/components/automation/AutomationHub.tsx`
- Test: `ui/src/components/automation/AutomationHub.test.tsx` (create if absent)

- [ ] **Step 1: Write the failing test**

Create or extend `ui/src/components/automation/AutomationHub.test.tsx`:

```tsx
import { describe, it, expect, vi } from 'vitest'
import { render, screen, fireEvent } from '@testing-library/react'
import { ActivityRow } from './AutomationHub'
import type { AutomationActivity } from '@/lib/tauri-bridge'

const baseActivity: AutomationActivity = {
  id: 'act-1', specId: 's1', subscriptionId: null,
  triggerSourceType: 'manual', triggerPayloadJson: '{}',
  status: 'completed', errorText: null,
  queuedAt: 1, startedAt: 1, completedAt: 2, durationMs: 1000,
  llmIterations: 3, llmTokensIn: 100, llmTokensOut: 50,
  sessionId: 'sess-1', reportArtifactsJson: '[]',
  reportText: 'done', reportOutcome: 'useful',
  escalationId: null, resumedFromActivityId: null, resumedFromEscalationId: null,
}

describe('ActivityRow', () => {
  it('calls onOpen with the session id when a linked row is clicked', () => {
    const onOpen = vi.fn()
    render(<ActivityRow a={baseActivity} onOpen={onOpen} />)
    fireEvent.click(screen.getByText('manual'))
    expect(onOpen).toHaveBeenCalledWith('sess-1')
  })

  it('is not clickable when sessionId is null', () => {
    const onOpen = vi.fn()
    render(<ActivityRow a={{ ...baseActivity, sessionId: null }} onOpen={onOpen} />)
    fireEvent.click(screen.getByText('manual'))
    expect(onOpen).not.toHaveBeenCalled()
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd ui && npm test -- --run AutomationHub`
Expected: FAIL — `ActivityRow` is not exported / has no `onOpen` prop.

- [ ] **Step 3: Make `ActivityRow` exported + clickable**

In `ui/src/components/automation/AutomationHub.tsx`, change the `ActivityRow` function (lines 53–80) to be exported and take an `onOpen` callback:

```tsx
// ─── ActivityRow ──────────────────────────────────────────────────────────────

export function ActivityRow({
  a,
  onOpen,
}: {
  a: AutomationActivity
  onOpen: (sessionId: string) => void
}): React.ReactElement {
  const d = a.durationMs > 0 ? `${(a.durationMs / 1000).toFixed(1)}s` : ''
  const subtitle = a.reportText ?? a.errorText ?? ''
  const clickable = a.sessionId !== null
  return (
    <div
      className={`flex items-start gap-2 py-1 ${clickable ? 'cursor-pointer hover:bg-accent/40 rounded -mx-1 px-1' : ''}`}
      onClick={() => { if (a.sessionId) onOpen(a.sessionId) }}
      role={clickable ? 'button' : undefined}
      title={clickable ? '在 Agent 视图中查看此次运行' : undefined}
    >
      <div className="mt-0.5">{statusIcon(a.status)}</div>
      <div className="flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="text-[11px] font-medium">{a.triggerSourceType}</span>
          {d && <span className="text-[10px] text-muted-foreground">{d}</span>}
          {a.reportOutcome && (
            <span className="text-[10px] text-muted-foreground">[{a.reportOutcome}]</span>
          )}
        </div>
        {subtitle && (
          <p
            className={`text-[10px] truncate ${a.errorText ? 'text-red-400' : 'text-muted-foreground'}`}
            title={subtitle}
          >
            {subtitle}
          </p>
        )}
      </div>
    </div>
  )
}
```

- [ ] **Step 4: Wire `onOpen` in `AutomationCard`**

In `AutomationHub.tsx`, near the top of the file add the import:

```tsx
import { useOpenSession } from '@/hooks/useOpenSession'
```

In `AutomationCard`, add the hook + pass it down. At the activity-list render (lines 244–253), change `<ActivityRow key={a.id} a={a} />` to pass `onOpen`:

```tsx
function AutomationCard({ spec, onSpecsChange }: { /* existing props */ }): React.ReactElement {
  const openSession = useOpenSession()
  // ... existing hooks ...
```

```tsx
            specActivities.map((a) => (
              <ActivityRow
                key={a.id}
                a={a}
                onOpen={(sessionId) =>
                  openSession('agent', sessionId, `${spec.name} · 运行`)
                }
              />
            ))
```

- [ ] **Step 5: Run to verify it passes + typecheck**

Run: `cd ui && npm test -- --run AutomationHub`
Expected: PASS.
Run: `cd ui && npx tsc --noEmit 2>&1 | head -10`
Expected: no errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/automation/AutomationHub.tsx ui/src/components/automation/AutomationHub.test.tsx
git commit -m "feat(automation-ui): clicking a run opens its run-session in the Agent view"
```

### Task E2: Automation run banner in `AgentView`

**Files:**
- Create: `ui/src/components/agent/AutomationRunBanner.tsx`
- Modify: `ui/src/components/agent/AgentView.tsx`
- Test: `ui/src/components/agent/AutomationRunBanner.test.tsx`

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/agent/AutomationRunBanner.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { AutomationRunBanner } from './AutomationRunBanner'

describe('AutomationRunBanner', () => {
  it('renders nothing for a non-automation session', () => {
    const { container } = render(
      <AutomationRunBanner metadataJson={'{"origin":"human"}'} />,
    )
    expect(container.firstChild).toBeNull()
  })

  it('renders the trigger origin for an automation run session', () => {
    render(
      <AutomationRunBanner
        metadataJson={'{"origin":"automation:schedule","spec_id":"s1"}'}
      />,
    )
    expect(screen.getByText(/automation run/i)).toBeInTheDocument()
    expect(screen.getByText(/schedule/i)).toBeInTheDocument()
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd ui && npm test -- --run AutomationRunBanner`
Expected: FAIL — module does not exist.

- [ ] **Step 3: Create the banner component**

Create `ui/src/components/agent/AutomationRunBanner.tsx`:

```tsx
import * as React from 'react'
import { Bot } from 'lucide-react'

interface RunMeta {
  origin?: string
  spec_id?: string
  prev_run_session_id?: string | null
}

/**
 * Shown at the top of the Agent view when the loaded session is an
 * automation run (origin starts with "automation:"). Identifies the run
 * as automation-produced and surfaces the trigger. Renders nothing for
 * ordinary human sessions.
 */
export function AutomationRunBanner({
  metadataJson,
}: {
  metadataJson: string | null | undefined
}): React.ReactElement | null {
  const meta = React.useMemo<RunMeta>(() => {
    if (!metadataJson) return {}
    try {
      return JSON.parse(metadataJson) as RunMeta
    } catch {
      return {}
    }
  }, [metadataJson])

  const origin = meta.origin ?? ''
  if (!origin.startsWith('automation:')) return null

  const trigger = origin.slice('automation:'.length)

  return (
    <div className="mx-4 mb-2 flex items-center gap-2 px-3 py-2 rounded-lg bg-primary/5 text-primary text-sm">
      <Bot className="size-4" />
      <span className="font-medium">Automation run</span>
      <span className="text-xs text-muted-foreground">
        触发源: {trigger || 'unknown'}
      </span>
    </div>
  )
}
```

- [ ] **Step 4: Mount it in `AgentView`**

In `ui/src/components/agent/AgentView.tsx`, add the import near the other agent-component imports:

```tsx
import { AutomationRunBanner } from './AutomationRunBanner'
```

The component needs the session's `metadataJson`. `AgentView` already resolves the current session — find where the session metadata is available (the `currentAgentSessionAtom` / the session object passed around; `AgentSessionMeta` likely carries `metadataJson` — grep `metadataJson` in `ui/src/atoms/agent-atoms.ts` to confirm the field name). Mount the banner right after `<AgentHeader sessionId={sessionId} />` (line 1395):

```tsx
        {/* Agent Header */}
        <AgentHeader sessionId={sessionId} />

        {/* Automation run context banner — only renders for origin=automation:* */}
        <AutomationRunBanner metadataJson={currentSessionMetaJson} />

        {/* 消息区域 */}
        <AgentMessages
```

> `currentSessionMetaJson` is illustrative — bind it to the actual session metadata field. If `AgentSessionMeta` does not expose `metadataJson`, the cheapest path is: the `AutomationRunBanner` takes `sessionId` and reads the session from `agentSessionsAtom` itself. Adjust the component's prop to `sessionId: string` and have it `useAtomValue(agentSessionsAtom)` + `.find(...)` if that is cleaner — decide based on what `AgentSessionMeta` actually carries.

- [ ] **Step 5: Run to verify it passes + typecheck**

Run: `cd ui && npm test -- --run AutomationRunBanner && npx tsc --noEmit 2>&1 | head -10`
Expected: test PASS, no TS errors.

- [ ] **Step 6: Commit**

```bash
git add ui/src/components/agent/AutomationRunBanner.tsx ui/src/components/agent/AutomationRunBanner.test.tsx ui/src/components/agent/AgentView.tsx
git commit -m "feat(automation-ui): automation run context banner in the Agent view"
```

### Task E3: Hide automation run-sessions from the workspace session rail

**Files:**
- Modify: `ui/src/components/workspace/WorkspaceRail.tsx`
- Test: extend an existing `WorkspaceRail` test or add a focused one

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/workspace/WorkspaceRail.filter.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { isAutomationSession } from './WorkspaceRail'

describe('isAutomationSession', () => {
  it('detects an automation-origin session from metadataJson', () => {
    expect(isAutomationSession({ metadataJson: '{"origin":"automation:schedule"}' })).toBe(true)
  })
  it('returns false for a human session', () => {
    expect(isAutomationSession({ metadataJson: '{"origin":"human"}' })).toBe(false)
  })
  it('returns false when metadataJson is missing or unparseable', () => {
    expect(isAutomationSession({ metadataJson: null })).toBe(false)
    expect(isAutomationSession({ metadataJson: 'not json' })).toBe(false)
    expect(isAutomationSession({})).toBe(false)
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd ui && npm test -- --run WorkspaceRail.filter`
Expected: FAIL — `isAutomationSession` not exported.

- [ ] **Step 3: Add `isAutomationSession` + apply the filter**

In `ui/src/components/workspace/WorkspaceRail.tsx`, add an exported helper near the top:

```tsx
/**
 * True when a session was produced by an automation run (origin metadata
 * starts with "automation:"). Such run-sessions are reached through the
 * AutomationHub activity list, not the workspace session rail (design §0.4).
 */
export function isAutomationSession(s: { metadataJson?: string | null }): boolean {
  if (!s.metadataJson) return false
  try {
    const meta = JSON.parse(s.metadataJson) as { origin?: string }
    return typeof meta.origin === 'string' && meta.origin.startsWith('automation:')
  } catch {
    return false
  }
}
```

Apply it in the `sessions` derivation (lines 93–103). Change:

```tsx
  const sessions = activeWorkspaceId
    ? (workspaceSessions[activeWorkspaceId] ?? [])
    : []
```

to:

```tsx
  const sessions = (
    activeWorkspaceId ? (workspaceSessions[activeWorkspaceId] ?? []) : []
  ).filter((s) => !isAutomationSession(s))
```

> Confirm the session objects in `workspaceSessionsAtom` carry a `metadataJson` field. If they do not, the filter cannot work from the rail's data — in that case, the backend `list_agent_sessions` / the workspace-sessions atom must surface `metadataJson` (or a derived `isAutomation` boolean). `grep -rn "metadataJson\|metadata_json" ui/src/atoms/workspace*` and `src-tauri/src/.../list_agent_sessions`. If the field is absent, add it to the backend session row + the TS type as a sub-step before the filter — note this in the commit.

- [ ] **Step 4: Run to verify it passes + typecheck**

Run: `cd ui && npm test -- --run WorkspaceRail && npx tsc --noEmit 2>&1 | head -10`
Expected: PASS, no TS errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/workspace/WorkspaceRail.tsx ui/src/components/workspace/WorkspaceRail.filter.test.tsx
git commit -m "feat(automation-ui): hide automation run-sessions from the workspace rail"
```

### Task E4: RightSidePanel tab visibility by run capability

For an automation run-session, show `files` / `plan` / `trajectory` always; show `teams` / `browser` only when the run used them. Detection: read the run-session's metadata (or, simplest for 2a, a `capabilities` array the run-session metadata could carry — but the run-session metadata written in Task D2 does not include capability info). Phase 2a keeps this minimal: gate `teams` / `browser` behind "is this an automation session" — for automation sessions, hide `teams` and `browser` (a 5-min spec rarely uses either, and the precise per-run capability map is a 2b refinement).

**Files:**
- Modify: `ui/src/components/app-shell/RightSidePanel.tsx`

- [ ] **Step 1: Write the failing test**

Create `ui/src/components/app-shell/RightSidePanel.tabs.test.tsx`:

```tsx
import { describe, it, expect } from 'vitest'
import { visibleTabs } from './RightSidePanel'

describe('visibleTabs', () => {
  it('shows all five tabs for a human session', () => {
    expect(visibleTabs(false)).toEqual(['files', 'teams', 'plan', 'trajectory', 'browser'])
  })
  it('hides teams + browser for an automation run session', () => {
    expect(visibleTabs(true)).toEqual(['files', 'plan', 'trajectory'])
  })
})
```

- [ ] **Step 2: Run to verify it fails**

Run: `cd ui && npm test -- --run RightSidePanel.tabs`
Expected: FAIL — `visibleTabs` not exported.

- [ ] **Step 3: Add `visibleTabs` + apply it to the tab bar**

In `ui/src/components/app-shell/RightSidePanel.tsx`, add an exported helper near the `ActiveTab` type (line 35):

```tsx
export type ActiveTab = 'files' | 'teams' | 'plan' | 'trajectory' | 'browser'

/**
 * Which right-panel tabs to show. For automation run-sessions, teams +
 * browser are hidden — files/plan/trajectory always matter for a run, but
 * a run rarely uses teams or browser, and the precise per-run capability
 * map is a Phase 2b refinement (design §0.6).
 */
export function visibleTabs(isAutomationRun: boolean): ActiveTab[] {
  return isAutomationRun
    ? ['files', 'plan', 'trajectory']
    : ['files', 'teams', 'plan', 'trajectory', 'browser']
}
```

In the component, determine `isAutomationRun` from the current session's metadata (reuse `isAutomationSession` from `WorkspaceRail.tsx` — `import { isAutomationSession } from '@/components/workspace/WorkspaceRail'` — applied to the current session object). Then guard each `<TabButton>` in the tab bar (lines 137–168). Wrap each tab button:

```tsx
      <div className="titlebar-no-drag flex items-center gap-1 px-2 pb-1.5 border-b border-border/40 flex-shrink-0">
        {tabs.includes('files') && (
          <TabButton
            isActive={activeTab === 'files'}
            onClick={() => setActiveTab('files')}
            icon={<FolderOpen size={13} />}
            label="Files"
          />
        )}
        {tabs.includes('teams') && (
          <TabButton
            isActive={activeTab === 'teams'}
            onClick={() => setActiveTab('teams')}
            icon={<Users size={13} />}
            label="Teams"
          />
        )}
        {tabs.includes('plan') && (
          <TabButton
            isActive={activeTab === 'plan'}
            onClick={() => setActiveTab('plan')}
            icon={<ListChecks size={13} />}
            label="Plan"
          />
        )}
        {tabs.includes('trajectory') && (
          <TabButton
            isActive={activeTab === 'trajectory'}
            onClick={() => setActiveTab('trajectory')}
            icon={<History size={13} />}
            label="Trajectory"
          />
        )}
        {tabs.includes('browser') && (
          <TabButton
            isActive={activeTab === 'browser'}
            onClick={() => setActiveTab('browser')}
            icon={<Globe size={13} />}
            label="Browser"
          />
        )}
      </div>
```

where `const tabs = visibleTabs(isAutomationRun)` is computed near the top of the component (after the existing `currentSessionId` resolution, before the gating `return null`). If `activeTab` is currently set to a now-hidden tab, fall back to `'files'`:

```tsx
  const isAutomationRun = /* resolve current session, then */ isAutomationSession(currentSession ?? {})
  const tabs = visibleTabs(isAutomationRun)
  const effectiveTab: ActiveTab = tabs.includes(activeTab) ? activeTab : 'files'
```

and use `effectiveTab` in place of `activeTab` in the tab-content switch (lines 187–207).

> Resolve `currentSession` from the existing atoms the component already reads (it has `currentSessionId`). `grep` how the component gets session objects; if it only has the id, add a `useAtomValue(agentSessionsAtom)` + `.find(...)`. Keep it minimal.

- [ ] **Step 4: Run to verify it passes + typecheck**

Run: `cd ui && npm test -- --run RightSidePanel && npx tsc --noEmit 2>&1 | head -10`
Expected: PASS, no TS errors.

- [ ] **Step 5: Commit**

```bash
git add ui/src/components/app-shell/RightSidePanel.tsx ui/src/components/app-shell/RightSidePanel.tabs.test.tsx
git commit -m "feat(automation-ui): RightSidePanel hides teams/browser tabs for automation runs"
```

---

## Final Verification

After all tasks, run the full verification suite:

- [ ] `cd src-tauri && cargo build 2>&1 | grep -E "^error" | head` — expected: no output
- [ ] `cd src-tauri && cargo test --lib 2>&1 | tail -15` — expected: all tests pass
- [ ] `cd ui && npx tsc --noEmit 2>&1 | head -10` — expected: no errors
- [ ] `cd ui && npm test -- --run 2>&1 | tail -10` — expected: all tests pass
- [ ] Manual smoke (`cargo tauri dev`): install a simple Humane spec, trigger it manually from AutomationHub, confirm the activity row turns `completed`/`failed` (not `deferred_phase_2`), click the row → the run-session opens in the Agent view with the automation banner, the transcript is visible, and the run-session does NOT appear in the workspace session rail.

## Adjacent edits — call out in the PR body (not scope creep)

- `AppRuntimeService` gained a `provider_service` field → constructor signature changed → `app.rs` construction site + the `service.rs` test `make_service` helper both updated (Task D1). Per CLAUDE.md *Adjacent edits*.
- `AutomationActivity` Rust struct + frontend TS interface changed together with the V24 migration (Task A1) — the column drop forces a synchronized struct change; it is one logical change, not scope creep.
- `notify_user` real channel dispatch was **narrowed out of 2a** to a structured-log stub (Task C5 Step 5) — threading `ChannelManager` into the headless delegate is wider than the rest of 2a. Real dispatch lands in 2b. Flag this deviation from spec §0.10.
- The `per-day` cost cap sums *all* `cost_records` for the day, not automation-only (Task D4 Step 1) — a precise per-origin split is a 2b refinement.

## Self-Review notes (resolved during planning)

- **Spec coverage:** §0.1 home space → D2/D4; §0.2 session granularity + chain → D2; §0.3 ledger/session_id/drop tool_calls_json → A1; §0.4 visibility filter → E3, retention → D2/D4; §0.5 cwd → D4; §0.6 run viewing UI + banner + tab filter → E1/E2/E4; §0.7 artifact provenance → C5; §0.8 Tier 1 preload → D3 (Tier 2 promotion correctly deferred to 2b, no task); §0.9 continuation → no task (escalation already wired; "read-only run-session" is the default — composer is untouched per spec §6 Adjacent edits); §0.10 unified substrate model → realized by the run=session data model (A1/D2), `report_to_user` → C5, `notify_user` → C5 (narrowed, flagged); §1 AutomationDelegate → C3/C4/C5 + D4; §2 cost caps → C1/C2/C4/D4; §3 llm_stream → B1/B2; §4 V24 → A1; §0.4 agent_sessions archive tech-debt → V24 adds `archived_at` (A1); the full archive-action UI wiring is **partially deferred** — `archived_at` column lands in 2a, the end-to-end archive-action UI is out of this plan's task list (the spec listed it as adjacent tech-debt; flag in the PR that only the schema half landed).
- **Type consistency:** `CostCapConfig` / `CostCapState` / `CostCapDecision` consistent C2→C4→D4; `AutomationActivity` field set consistent A1 (Rust + TS); `StreamSink` / `stream_completion` signature consistent B1→B2→C4; `isAutomationSession` reused E3→E4.
