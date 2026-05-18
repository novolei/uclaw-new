//! Symphony protocol types — pure data, no I/O.
//!
//! These are the over-the-wire shapes for workflows and runs. Serde tags
//! match what the canvas atoms expect on the frontend (see
//! `ui/src/atoms/symphony.ts` — landed in T15).

use serde::{Deserialize, Serialize};

// ─── lifecycle enums ──────────────────────────────────────────────────────────

/// Status of a single node attempt.
///
/// Symphony's 5-state machine generalized to include UI-friendly transient
/// states. Ordering matches SPEC.md's `active_states → terminal_states` flow.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    /// Upstream deps not yet satisfied.
    Pending,
    /// All deps satisfied; waiting for a concurrency permit.
    Ready,
    /// `HeadlessDelegate` driving `run_agentic_loop`.
    Running,
    /// No heartbeat within `stall_timeout_ms`. Becomes a retry candidate.
    Stalled,
    /// Reached terminal completion successfully.
    Succeeded,
    /// All retries exhausted or hard failure.
    Failed,
    /// Cancelled by user, parent failure cascade, or cost cap.
    Cancelled,
}

impl NodeStatus {
    /// Whether this status is terminal (no further state transitions).
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Cancelled
        )
    }

    /// Whether this status counts as "active" (occupies a concurrency slot).
    pub fn is_active(self) -> bool {
        matches!(self, NodeStatus::Ready | NodeStatus::Running)
    }

    /// Serializable string for the `symphony_node_runs.status` column.
    pub fn as_db_str(self) -> &'static str {
        match self {
            NodeStatus::Pending => "pending",
            NodeStatus::Ready => "ready",
            NodeStatus::Running => "running",
            NodeStatus::Stalled => "stalled",
            NodeStatus::Succeeded => "succeeded",
            NodeStatus::Failed => "failed",
            NodeStatus::Cancelled => "cancelled",
        }
    }

    /// Inverse of `as_db_str` — returns `None` for unrecognized strings.
    pub fn from_db_str(s: &str) -> Option<Self> {
        Some(match s {
            "pending" => NodeStatus::Pending,
            "ready" => NodeStatus::Ready,
            "running" => NodeStatus::Running,
            "stalled" => NodeStatus::Stalled,
            "succeeded" => NodeStatus::Succeeded,
            "failed" => NodeStatus::Failed,
            "cancelled" => NodeStatus::Cancelled,
            _ => return None,
        })
    }
}

/// Status of a whole run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Cancelled,
    /// Day-level cost cap tripped before the run could start (or mid-flight).
    QuotaExceeded,
}

impl RunStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            RunStatus::Completed
                | RunStatus::Failed
                | RunStatus::Cancelled
                | RunStatus::QuotaExceeded
        )
    }

    pub fn as_db_str(self) -> &'static str {
        match self {
            RunStatus::Queued => "queued",
            RunStatus::Running => "running",
            RunStatus::Completed => "completed",
            RunStatus::Failed => "failed",
            RunStatus::Cancelled => "cancelled",
            RunStatus::QuotaExceeded => "quota_exceeded",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        Some(match s {
            "queued" => RunStatus::Queued,
            "running" => RunStatus::Running,
            "completed" => RunStatus::Completed,
            "failed" => RunStatus::Failed,
            "cancelled" => RunStatus::Cancelled,
            "quota_exceeded" => RunStatus::QuotaExceeded,
            _ => return None,
        })
    }
}

/// Outcome label for a finished run (independent of `RunStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunOutcome {
    /// All leaf nodes succeeded.
    Succeeded,
    /// Some succeeded, some failed; workflow's `failure_mode` allowed continuation.
    Partial,
    /// Critical-path failure or aborted run.
    Failed,
}

impl RunOutcome {
    pub fn as_db_str(self) -> &'static str {
        match self {
            RunOutcome::Succeeded => "succeeded",
            RunOutcome::Partial => "partial",
            RunOutcome::Failed => "failed",
        }
    }
}

/// Per-node final outcome returned by the executor.
#[derive(Debug, Clone)]
pub enum NodeOutcome {
    Succeeded { cost_usd: f64, output_json: Option<String> },
    Failed { cost_usd: f64, error: String, retryable: bool },
    Cancelled { cost_usd: f64, reason: String },
}

// ─── workflow definition ──────────────────────────────────────────────────────

/// What kind of work a node represents. Today the executor only special-
/// cases `Agent` (one `HeadlessDelegate` run). Other variants are placeholders
/// for Phase 2 (shell-only, external HTTP, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    /// Default — one full `run_agentic_loop` call.
    Agent,
    /// Shell-only step (Phase 2). Runs `after_create_command` only, no LLM.
    Shell,
    /// External HTTP call (Phase 2). Driven by a tools/http step.
    Http,
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Agent
    }
}

/// Workflow-level failure mode when a node fails after retries are exhausted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    /// Cancel all remaining nodes and mark the run failed (default).
    Abort,
    /// Mark this node failed but keep running unrelated branches.
    ContinueOthers,
    /// Restrict execution to the surviving branch from the failed node's siblings.
    BranchOnly,
}

impl Default for FailureMode {
    fn default() -> Self {
        FailureMode::Abort
    }
}

/// Per-node retry policy. `max_attempts = 1` means "no retry".
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    /// Optional override of the global `SymphonyConfig.max_retry_backoff_ms`.
    pub max_backoff_ms: Option<u64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            max_backoff_ms: None,
        }
    }
}

/// A single node in a workflow DAG.
///
/// IDs are stable strings (workflow-version-scoped). Dependencies refer to
/// other nodes' `id` values within the same workflow version.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyNode {
    /// Stable id within this workflow version. UI also keys edges by it.
    pub id: String,
    /// Human-readable label rendered on the canvas.
    pub label: String,
    /// What kind of work this node performs.
    #[serde(default)]
    pub kind: NodeKind,
    /// Markdown prompt template. Supports `{{ upstream.<dep_id>.output }}`
    /// substitution in T8's `render_node_prompt`.
    #[serde(default)]
    pub prompt: String,
    /// IDs of upstream nodes that must reach `Succeeded` before this one
    /// can become `Ready`.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Per-node cost cap override (USD). Falls back to
    /// `SymphonyConfig.default_per_node_cost_cap_usd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_cap_usd: Option<f64>,
    /// Per-node iteration override. Falls back to
    /// `SymphonyConfig.default_max_iterations`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<usize>,
    /// Retry policy (default: no retry).
    #[serde(default)]
    pub retry: RetryPolicy,
    /// Optional shell command run inside the per-node workspace before the
    /// agent loop starts. Subject to `SafetyManager`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_create_command: Option<String>,
    /// Optional shell command run after the agent loop completes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_run_command: Option<String>,
    /// Optional model override (provider/model). Falls back to workflow
    /// default, then the global active model.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// An edge in the DAG. Edges are implicit in `SymphonyNode.deps`, but the
/// canvas also persists explicit edges so users can attach metadata
/// (e.g. labels, conditions) to them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyEdge {
    /// Source node id.
    pub from: String,
    /// Destination node id.
    pub to: String,
    /// Optional label rendered on the wire.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

/// The full definition of a workflow at one version. Persisted into
/// `symphony_workflow_versions` as `nodes_json` + `edges_json` columns.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyWorkflowDef {
    /// Stable workflow id (matches `symphony_workflows.id`).
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Optional home space override (otherwise `'symphonies'`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    /// Optional default model for all nodes (overridable per node).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    /// Workflow-level cost cap override. Falls back to
    /// `SymphonyConfig.default_per_run_cost_cap_usd`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_cost_cap_usd: Option<f64>,
    /// Workflow-level node concurrency. Falls back to
    /// `SymphonyConfig.default_max_concurrent_nodes`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_nodes: Option<usize>,
    /// Behavior when a node fails after retries are exhausted.
    #[serde(default)]
    pub failure_mode: FailureMode,
    /// Nodes in dependency-agnostic order; the executor topo-sorts at runtime.
    pub nodes: Vec<SymphonyNode>,
    /// Optional explicit edge list. Authoritative for graph structure when
    /// non-empty; otherwise `SymphonyNode.deps` is the only source.
    #[serde(default)]
    pub edges: Vec<SymphonyEdge>,
}

impl SymphonyWorkflowDef {
    /// Build an edge list from node deps if `edges` is empty.
    pub fn effective_edges(&self) -> Vec<SymphonyEdge> {
        if !self.edges.is_empty() {
            return self.edges.clone();
        }
        let mut out = Vec::new();
        for n in &self.nodes {
            for d in &n.deps {
                out.push(SymphonyEdge {
                    from: d.clone(),
                    to: n.id.clone(),
                    label: None,
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_status_db_roundtrip() {
        for s in [
            NodeStatus::Pending,
            NodeStatus::Ready,
            NodeStatus::Running,
            NodeStatus::Stalled,
            NodeStatus::Succeeded,
            NodeStatus::Failed,
            NodeStatus::Cancelled,
        ] {
            assert_eq!(NodeStatus::from_db_str(s.as_db_str()), Some(s));
        }
        assert_eq!(NodeStatus::from_db_str("nonsense"), None);
    }

    #[test]
    fn node_status_terminal_classification() {
        assert!(NodeStatus::Succeeded.is_terminal());
        assert!(NodeStatus::Failed.is_terminal());
        assert!(NodeStatus::Cancelled.is_terminal());
        assert!(!NodeStatus::Running.is_terminal());
        assert!(!NodeStatus::Pending.is_terminal());
        assert!(NodeStatus::Running.is_active());
        assert!(NodeStatus::Ready.is_active());
        assert!(!NodeStatus::Pending.is_active());
    }

    #[test]
    fn run_status_db_roundtrip() {
        for s in [
            RunStatus::Queued,
            RunStatus::Running,
            RunStatus::Completed,
            RunStatus::Failed,
            RunStatus::Cancelled,
            RunStatus::QuotaExceeded,
        ] {
            assert_eq!(RunStatus::from_db_str(s.as_db_str()), Some(s));
        }
    }

    #[test]
    fn effective_edges_synthesizes_from_deps_when_empty() {
        let def = SymphonyWorkflowDef {
            id: "wf".into(),
            name: "demo".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![
                SymphonyNode {
                    id: "a".into(),
                    label: "A".into(),
                    kind: NodeKind::Agent,
                    prompt: "".into(),
                    deps: vec![],
                    cost_cap_usd: None,
                    max_iterations: None,
                    retry: RetryPolicy::default(),
                    after_create_command: None,
                    after_run_command: None,
                    model: None,
                },
                SymphonyNode {
                    id: "b".into(),
                    label: "B".into(),
                    kind: NodeKind::Agent,
                    prompt: "".into(),
                    deps: vec!["a".into()],
                    cost_cap_usd: None,
                    max_iterations: None,
                    retry: RetryPolicy::default(),
                    after_create_command: None,
                    after_run_command: None,
                    model: None,
                },
            ],
            edges: vec![],
        };
        let edges = def.effective_edges();
        assert_eq!(edges.len(), 1);
        assert_eq!(edges[0].from, "a");
        assert_eq!(edges[0].to, "b");
    }

    #[test]
    fn effective_edges_returns_explicit_when_set() {
        let def = SymphonyWorkflowDef {
            id: "wf".into(),
            name: "demo".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![],
            edges: vec![SymphonyEdge {
                from: "x".into(),
                to: "y".into(),
                label: Some("labeled".into()),
            }],
        };
        assert_eq!(def.effective_edges().len(), 1);
        assert_eq!(def.effective_edges()[0].label.as_deref(), Some("labeled"));
    }

    #[test]
    fn retry_policy_defaults_to_single_attempt() {
        let r = RetryPolicy::default();
        assert_eq!(r.max_attempts, 1);
        assert!(r.max_backoff_ms.is_none());
    }
}
