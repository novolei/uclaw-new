//! Per-node executor — adapts one `SymphonyNode` into one
//! `HeadlessDelegate`-driven `run_agentic_loop` call.
//!
//! Inputs:
//! - the node definition (prompt template, deps, cost cap, retry, hooks),
//! - outputs from upstream nodes (used to render prompt placeholders),
//! - a `NodeExecutionDeps` bundle (services + config the run needs).
//!
//! Outputs:
//! - a `NodeOutcome` describing success / failure / cancellation + cost spent,
//! - side-effects: an `agent_sessions` row with the transcript persisted in
//!   `agent_messages`, and a `cost_records` row per LLM turn (the existing
//!   `HeadlessDelegate::on_usage` writes via `CostCapState`; the dashboard
//!   row goes through `cost_store::record` only when `app_handle` is set,
//!   matching the existing automation path).

use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use tokio::sync::{Mutex, RwLock};

use crate::agent::headless::HeadlessDelegate;
use crate::agent::types::{AgenticLoopConfig, ChatMessage, LoopOutcome, ReasoningContext};
use crate::automation::memory::MemoryStore;
use crate::automation::runtime::{
    cost::CostCapState, AutoContinueConfig, CompletionGate, PermissionSet,
};
use crate::channels::types::StreamingHandle;
use crate::channels::ChannelManager;
use crate::infra::InfraService;
use crate::llm::LlmProvider;
use crate::agent::tools::tool::ToolRegistry;

use super::super::protocol::types::{NodeOutcome, SymphonyNode, SymphonyWorkflowDef};
use super::run_session::{create_node_session, persist_transcript, resolve_home_space};
use super::stall::Heartbeat;

/// All the long-lived dependencies the per-node executor needs. Created once
/// per run by `RunActor::spawn` (T9) and shared across nodes.
#[derive(Clone)]
pub struct NodeExecutionDeps {
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    pub llm: Arc<dyn LlmProvider>,
    pub model: String,
    pub tools: Arc<ToolRegistry>,
    pub memory: Arc<MemoryStore>,
    pub workspace_root: PathBuf,
    pub heartbeat: Arc<Heartbeat>,
    pub app_handle: Option<tauri::AppHandle>,
    pub channel_manager: Option<Arc<RwLock<ChannelManager>>>,
    /// Shared `InfraService` so node + run lifecycle events reach the
    /// existing proactive subsystem (T12 spec §4.2).
    pub infra: Arc<InfraService>,
    /// Default max iterations from `SymphonyConfig`.
    pub default_max_iterations: usize,
    /// Default per-node cost cap from `SymphonyConfig`.
    pub default_per_node_cost_cap_usd: f64,
}

/// `StreamingHandle` impl that piggybacks on `HeadlessDelegate`'s existing
/// IM-streaming hook. Each partial-text chunk records a heartbeat against
/// the (run, node) pair AND writes the timestamp to `symphony_node_runs.
/// last_heartbeat_ms` so a crash-and-restart's `recovery::reconcile` sweep
/// can detect stalled nodes from the DB alone.
pub struct SymphonyHeartbeatSink {
    pub run_id: String,
    pub node_id: String,
    pub attempt: i64,
    pub heartbeat: Arc<Heartbeat>,
    pub app: Option<tauri::AppHandle>,
    pub db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl SymphonyHeartbeatSink {
    fn write_heartbeat_to_db(&self) {
        let now = chrono::Utc::now().timestamp_millis();
        if let Ok(conn) = self.db.lock() {
            let id = format!("{}-{}-{}", self.run_id, self.node_id, self.attempt);
            let _ = conn.execute(
                "UPDATE symphony_node_runs SET last_heartbeat_ms = ?1 WHERE id = ?2",
                rusqlite::params![now, id],
            );
        }
    }
}

#[async_trait]
impl StreamingHandle for SymphonyHeartbeatSink {
    async fn update(&self, partial: &str) -> anyhow::Result<()> {
        self.heartbeat.touch(&self.run_id, &self.node_id);
        self.write_heartbeat_to_db();
        if let Some(app) = &self.app {
            use tauri::Emitter;
            let _ = app.emit(
                "symphony:node_log",
                serde_json::json!({
                    "runId":  self.run_id,
                    "nodeId": self.node_id,
                    "role":   "assistant",
                    "content": partial,
                }),
            );
        }
        Ok(())
    }

    async fn finish(&self, _final_text: &str) -> anyhow::Result<()> {
        self.heartbeat.touch(&self.run_id, &self.node_id);
        self.write_heartbeat_to_db();
        Ok(())
    }
}

/// Render a node's prompt template by substituting `{{ upstream.<dep>.output }}`
/// placeholders with the deps' JSON-stringified outputs. Today's substitution
/// is intentionally minimal — Phase 2 may swap in a real template engine.
pub fn render_node_prompt(
    workflow: &SymphonyWorkflowDef,
    node: &SymphonyNode,
    upstream_outputs: &std::collections::HashMap<String, String>,
) -> String {
    let mut out = node.prompt.clone();
    out = out.replace("{{ workflow.name }}", &workflow.name);
    out = out.replace("{{ workflow.id }}", &workflow.id);
    for dep in &node.deps {
        let value = upstream_outputs
            .get(dep)
            .cloned()
            .unwrap_or_else(|| "<no output>".to_string());
        let needle = format!("{{{{ upstream.{}.output }}}}", dep);
        out = out.replace(&needle, &value);
    }
    out
}

/// Build the per-node `HeadlessDelegate`. Public for unit tests; the
/// `RunActor` calls `execute_node` which calls this internally.
pub fn build_delegate(
    workflow: &SymphonyWorkflowDef,
    node: &SymphonyNode,
    run_id: &str,
    attempt: i64,
    session_id: &str,
    rendered_prompt: String,
    deps: &NodeExecutionDeps,
) -> HeadlessDelegate {
    let per_node_cap = node
        .cost_cap_usd
        .unwrap_or(deps.default_per_node_cost_cap_usd);
    let cost = Arc::new(CostCapState::new(
        crate::automation::runtime::CostCapConfig {
            per_run_usd: per_node_cap,
            per_day_usd: per_node_cap,
        },
    ));

    let heartbeat_sink: Arc<dyn StreamingHandle> = Arc::new(SymphonyHeartbeatSink {
        run_id: run_id.to_string(),
        node_id: node.id.clone(),
        attempt,
        heartbeat: deps.heartbeat.clone(),
        app: deps.app_handle.clone(),
        db: deps.db.clone(),
    });

    HeadlessDelegate {
        // Spec/activity slots are repurposed as workflow/run identifiers
        // (mirrors automation, which uses spec_id/activity_id with its own
        // semantics; the Agent view treats both as opaque tags).
        spec_id: workflow.id.clone(),
        activity_id: run_id.to_string(),
        session_id: session_id.to_string(),
        permissions: PermissionSet::default(),
        memory: deps.memory.clone(),
        db: deps.db.clone(),
        gate: Arc::new(Mutex::new(None)),
        auto_continue: AutoContinueConfig::default(),
        llm: deps.llm.clone(),
        model: node.model.clone().unwrap_or_else(|| deps.model.clone()),
        tools: deps.tools.clone(),
        cost,
        workspace_root: deps.workspace_root.join(run_id).join(&node.id),
        app_handle: deps.app_handle.clone(),
        channel_manager: deps.channel_manager.clone(),
        reply_handle: None,
        streaming_handle: Some(heartbeat_sink),
        system_prompt_override: Some(rendered_prompt),
        safety_manager: None,
        tool_dispatcher: None,
        approval_handler: None,
    }
}

/// Execute one node attempt. Caller is responsible for retry / scheduling
/// (`RunActor`). Returns `NodeOutcome` describing the result.
pub async fn execute_node(
    workflow: &SymphonyWorkflowDef,
    node: &SymphonyNode,
    run_id: &str,
    attempt: i64,
    upstream_outputs: &std::collections::HashMap<String, String>,
    deps: &NodeExecutionDeps,
) -> NodeOutcome {
    // Resolve home space + create the agent_sessions row that will own the
    // transcript.
    let space_id = {
        let conn = deps.db.lock().unwrap();
        match resolve_home_space(&conn, &workflow.id) {
            Ok(s) => s,
            Err(e) => {
                return NodeOutcome::Failed {
                    cost_usd: 0.0,
                    error: format!("resolve_home_space failed: {}", e),
                    retryable: false,
                };
            }
        }
    };
    let session_id = {
        let conn = deps.db.lock().unwrap();
        match create_node_session(&conn, &space_id, &workflow.id, run_id, &node.id, attempt) {
            Ok(s) => s,
            Err(e) => {
                return NodeOutcome::Failed {
                    cost_usd: 0.0,
                    error: format!("create_node_session failed: {}", e),
                    retryable: false,
                };
            }
        }
    };

    // Ensure the per-node workspace directory exists.
    let workspace = deps.workspace_root.join(run_id).join(&node.id);
    if let Err(e) = std::fs::create_dir_all(&workspace) {
        return NodeOutcome::Failed {
            cost_usd: 0.0,
            error: format!("workspace mkdir failed: {}", e),
            retryable: false,
        };
    }

    // Initial heartbeat — covers the "agent never made an LLM call" case.
    deps.heartbeat.touch(run_id, &node.id);

    let rendered_prompt = render_node_prompt(workflow, node, upstream_outputs);

    // Build delegate + run agentic loop.
    let delegate = build_delegate(
        workflow,
        node,
        run_id,
        attempt,
        &session_id,
        rendered_prompt.clone(),
        deps,
    );
    let cost_state = delegate.cost.clone();

    let mut reason_ctx = ReasoningContext::new(rendered_prompt);
    reason_ctx
        .messages
        .push(ChatMessage::user(&format!("Execute node `{}` ({}) within workflow `{}`.", node.id, node.label, workflow.name)));

    let cfg = AgenticLoopConfig {
        max_iterations: node.max_iterations.unwrap_or(deps.default_max_iterations),
        ..AgenticLoopConfig::default()
    };

    let loop_outcome =
        crate::agent::agentic_loop::run_agentic_loop(&delegate, &mut reason_ctx, &cfg).await;

    // Persist transcript (single call per session).
    if let Err(e) = {
        let conn = deps.db.lock().unwrap();
        persist_transcript(&conn, &session_id, &reason_ctx.messages)
    } {
        tracing::warn!(
            run_id = %run_id,
            node_id = %node.id,
            "symphony persist_transcript failed: {}",
            e
        );
    }

    // Clear heartbeat — the node is no longer producing.
    deps.heartbeat.forget(run_id, &node.id);

    let cost_usd = cost_state.total_usd();

    // Map agent loop outcome → node outcome. See `agent::types::LoopOutcome`.
    //
    // - Response / ToolResult are "natural" loop endings — both treated as
    //   success for Symphony (the workflow's failure_mode handles whether
    //   to keep going; here we only report what happened).
    // - MaxIterations is a soft cap: the node didn't complete its work but
    //   it also didn't fail outright. Treat as retryable failure so the
    //   workflow's retry policy can decide.
    // - Failure is a hard agent-loop error (LLM stream, dispatcher panic).
    // - Cancelled / Stopped come from `LoopSignal::Cancel` / `Stop`, which
    //   is what the RunActor injects when the user cancels a run or when
    //   `failure_mode = Abort` is cascading.
    // - NeedApproval is treated as non-retryable: Symphony nodes run
    //   without an interactive approval UI, so a tool needing one is a
    //   workflow-configuration bug, not a transient failure.
    match loop_outcome {
        LoopOutcome::Response { .. } | LoopOutcome::ToolResult { .. } => {
            NodeOutcome::Succeeded {
                cost_usd,
                output_json: extract_last_text(&reason_ctx.messages),
            }
        }
        LoopOutcome::MaxIterations => NodeOutcome::Failed {
            cost_usd,
            error: "max_iterations reached".to_string(),
            retryable: true,
        },
        LoopOutcome::Failure { error } => NodeOutcome::Failed {
            cost_usd,
            error,
            retryable: true,
        },
        LoopOutcome::Cancelled { .. } => NodeOutcome::Cancelled {
            cost_usd,
            reason: "cancelled".to_string(),
        },
        LoopOutcome::Stopped => NodeOutcome::Cancelled {
            cost_usd,
            reason: "stopped".to_string(),
        },
        LoopOutcome::NeedApproval { tool_name, .. } => NodeOutcome::Failed {
            cost_usd,
            error: format!(
                "node requires approval for tool `{}` — symphony does not support interactive approvals",
                tool_name
            ),
            retryable: false,
        },
    }
}

/// Pull the last assistant text block (if any) as JSON-stringified output
/// for downstream consumers.
fn extract_last_text(messages: &[ChatMessage]) -> Option<String> {
    use crate::agent::types::{ContentBlock, MessageRole};
    for msg in messages.iter().rev() {
        if !matches!(msg.role, MessageRole::Assistant) {
            continue;
        }
        for block in &msg.content {
            if let ContentBlock::Text { text } = block {
                return Some(text.clone());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symphony_graph::protocol::types::{NodeKind, RetryPolicy};
    use std::collections::HashMap;

    fn sample_node() -> SymphonyNode {
        SymphonyNode {
            id: "n1".into(),
            label: "Node 1".into(),
            kind: NodeKind::Agent,
            prompt: "echo {{ upstream.parent.output }} for {{ workflow.name }}".into(),
            deps: vec!["parent".into()],
            cost_cap_usd: None,
            max_iterations: None,
            retry: RetryPolicy::default(),
            after_create_command: None,
            after_run_command: None,
            model: None,
        }
    }

    fn sample_workflow() -> SymphonyWorkflowDef {
        SymphonyWorkflowDef {
            id: "wf".into(),
            name: "Demo WF".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: Default::default(),
            nodes: vec![sample_node()],
            edges: vec![],
        }
    }

    #[test]
    fn render_substitutes_workflow_and_upstream() {
        let mut up = HashMap::new();
        up.insert("parent".to_string(), "hello world".to_string());
        let s = render_node_prompt(&sample_workflow(), &sample_node(), &up);
        assert_eq!(s, "echo hello world for Demo WF");
    }

    #[test]
    fn render_marks_missing_upstream() {
        let s = render_node_prompt(&sample_workflow(), &sample_node(), &HashMap::new());
        assert!(s.contains("<no output>"));
    }

    #[test]
    fn extract_last_text_finds_most_recent_assistant_text() {
        let msgs = vec![
            ChatMessage::user("hi"),
            ChatMessage::assistant("first reply"),
            ChatMessage::user("more"),
            ChatMessage::assistant("second reply"),
        ];
        assert_eq!(
            extract_last_text(&msgs),
            Some("second reply".to_string())
        );
    }

    #[test]
    fn extract_last_text_returns_none_for_no_assistant() {
        let msgs = vec![ChatMessage::user("hi")];
        assert!(extract_last_text(&msgs).is_none());
    }
}
