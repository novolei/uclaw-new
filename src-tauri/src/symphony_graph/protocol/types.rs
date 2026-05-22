//! Symphony protocol types — pure data, no I/O.
//!
//! Exposes both the legacy DAG node-canvas structures and the new high-efficiency
//! issue-centric vertical pipeline types matching the SymphonyMac architecture
//! and ZeptoBeam actor states.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// =================================================════========================
// SECTION 1: New SymphonyMac & ZeptoBeam Pipeline Types
// =================================================════========================

/// Status of an issue-pipeline runner agent execution.
#[derive(Debug, Clone, Serialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Preparing,
    Running,
    Completed,
    Failed,
    Stopped,
    Interrupted,
    AwaitingApproval,
}

impl<'de> Deserialize<'de> for AgentStatus {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        match value.as_str() {
            "preparing" => Ok(Self::Preparing),
            "running" => Ok(Self::Running),
            "completed" => Ok(Self::Completed),
            "failed" => Ok(Self::Failed),
            "stopped" => Ok(Self::Stopped),
            "interrupted" => Ok(Self::Interrupted),
            "awaiting_approval" | "awaitingapproval" => Ok(Self::AwaitingApproval),
            _ => Err(serde::de::Error::unknown_variant(
                &value,
                &[
                    "preparing",
                    "running",
                    "completed",
                    "failed",
                    "stopped",
                    "interrupted",
                    "awaiting_approval",
                ],
            )),
        }
    }
}

/// The chronological stages of the Symphony issue resolution pipeline.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PipelineStage {
    Implement,
    CodeReview,
    Testing,
    Merge,
    Done,
}

impl std::fmt::Display for PipelineStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PipelineStage::Implement => write!(f, "implement"),
            PipelineStage::CodeReview => write!(f, "code_review"),
            PipelineStage::Testing => write!(f, "testing"),
            PipelineStage::Merge => write!(f, "merge"),
            PipelineStage::Done => write!(f, "done"),
        }
    }
}

/// Structured context generated at the end of each pipeline stage,
/// injected into the next stage's prompt to provide continuity.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct StageContext {
    /// Which stage produced this context
    pub from_stage: String,
    /// Files that were modified (from git diff)
    pub files_changed: Vec<String>,
    /// Lines added in this stage
    pub lines_added: u32,
    /// Lines removed in this stage
    pub lines_removed: u32,
    /// PR number if one was created or exists
    pub pr_number: Option<u64>,
    /// Branch name for the PR
    pub branch_name: Option<String>,
    /// Key summary extracted from agent logs (review comments, test results, etc.)
    pub summary: String,
}

impl StageContext {
    /// Format this context as a concise section to append to a prompt.
    pub fn to_prompt_section(&self) -> String {
        let mut parts = Vec::new();
        parts.push(format!("## Context from {} stage", self.from_stage));

        if !self.files_changed.is_empty() {
            let files_list: String = self
                .files_changed
                .iter()
                .take(20)
                .map(|f| format!("  - {}", f))
                .collect::<Vec<_>>()
                .join("\n");
            parts.push(format!(
                "Files changed ({} added, {} removed):\n{}",
                self.lines_added, self.lines_removed, files_list
            ));
            if self.files_changed.len() > 20 {
                parts.push(format!(
                    "  ... and {} more files",
                    self.files_changed.len() - 20
                ));
            }
        }

        if let Some(pr) = self.pr_number {
            parts.push(format!("PR number: #{}", pr));
        }
        if let Some(ref branch) = self.branch_name {
            parts.push(format!("Branch: {}", branch));
        }

        if !self.summary.is_empty() {
            parts.push(format!("Summary: {}", self.summary));
        }

        parts.join("\n")
    }
}

/// A structured report for an individual stage execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StageReport {
    pub name: String,
    pub status: String,
    pub duration_secs: Option<i64>,
    pub duration_display: String,
    pub files_modified: Vec<String>,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub commands_executed: Vec<String>,
    pub summary: String,
    pub attempt: u32,
}

/// A full structured report compiled across all stages of an issue run.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PipelineReport {
    pub issue_number: u64,
    pub issue_title: String,
    pub repo: String,
    pub total_duration_secs: i64,
    pub total_duration_display: String,
    pub stages: Vec<StageReport>,
    pub pr_number: Option<u64>,
    pub pr_url: Option<String>,
    pub issue_url: String,
    pub code_review_summary: String,
    pub testing_summary: String,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
}

/// A single execution run instance of an issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentRun {
    pub id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub status: AgentStatus,
    pub stage: PipelineStage,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub logs: Vec<String>,
    pub workspace_path: String,
    pub error: Option<String>,
    pub attempt: u32,
    pub max_retries: u32,
    pub lines_added: u32,
    pub lines_removed: u32,
    pub files_modified_list: Vec<String>,
    pub report: Option<PipelineReport>,
    /// The CLI command invoked (e.g. "claude --print ...")
    pub command_display: Option<String>,
    /// Agent type used: "claude" or "codex"
    pub agent_type: String,
    /// Last line of output received
    pub last_log_line: Option<String>,
    /// Total number of log lines produced so far
    pub log_count: u32,
    /// Detected activity state from log content
    pub activity: Option<String>,
    /// Timestamp of the last log output (for stall detection)
    pub last_log_timestamp: Option<String>,
    /// Token usage from Claude result events
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost_usd: f64,
    /// Labels from the GitHub issue, used for stage-skip logic
    #[serde(default)]
    pub issue_labels: Vec<String>,
    /// Stages that were skipped for this issue based on label rules
    #[serde(default)]
    pub skipped_stages: Vec<String>,
    /// Structured context from the previous pipeline stage
    pub stage_context: Option<StageContext>,
    /// The next stage to advance to when approval is granted
    #[serde(default)]
    pub pending_next_stage: Option<String>,
}

/// A summary projection of an AgentRun used in sidebar listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RunSummary {
    pub id: String,
    pub repo: String,
    pub issue_number: u64,
    pub issue_title: String,
    pub status: AgentStatus,
    pub stage: PipelineStage,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub workspace_path: String,
    pub error: Option<String>,
    pub attempt: u32,
    pub max_retries: u32,
    pub command_display: Option<String>,
    pub agent_type: String,
    pub last_log_line: Option<String>,
    pub log_count: u32,
    pub activity: Option<String>,
    pub last_log_timestamp: Option<String>,
    #[serde(default)]
    pub skipped_stages: Vec<String>,
    #[serde(default)]
    pub pending_next_stage: Option<String>,
}

impl From<&AgentRun> for RunSummary {
    fn from(run: &AgentRun) -> Self {
        Self {
            id: run.id.clone(),
            repo: run.repo.clone(),
            issue_number: run.issue_number,
            issue_title: run.issue_title.clone(),
            status: run.status.clone(),
            stage: run.stage.clone(),
            started_at: run.started_at.clone(),
            finished_at: run.finished_at.clone(),
            workspace_path: run.workspace_path.clone(),
            error: run.error.clone(),
            attempt: run.attempt,
            max_retries: run.max_retries,
            command_display: run.command_display.clone(),
            agent_type: run.agent_type.clone(),
            last_log_line: run.last_log_line.clone(),
            log_count: run.log_count,
            activity: run.activity.clone(),
            last_log_timestamp: run.last_log_timestamp.clone(),
            skipped_stages: run.skipped_stages.clone(),
            pending_next_stage: run.pending_next_stage.clone(),
        }
    }
}

/// Hooks supporting command callbacks at lifecycle thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct LifecycleHooks {
    /// Runs after a new workspace is created (e.g., npm install). Failure aborts.
    pub after_create: Option<String>,
    /// Runs before each agent attempt (e.g., git pull). Failure aborts.
    pub before_run: Option<String>,
    /// Runs after each agent attempt, success or failure. Failure is logged but ignored.
    pub after_run: Option<String>,
    /// Runs before workspace deletion. Failure is logged but ignored.
    pub before_remove: Option<String>,
    /// Timeout in seconds for each hook (default 60).
    pub timeout_secs: u64,
}

impl Default for LifecycleHooks {
    fn default() -> Self {
        Self {
            after_create: None,
            before_run: None,
            after_run: None,
            before_remove: None,
            timeout_secs: 60,
        }
    }
}

/// Configuration settings governing the Symphony run orchestrator.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct RunConfig {
    pub agent_type: String,
    pub auto_approve: bool,
    pub max_concurrent: usize,
    pub poll_interval_secs: u64,
    pub issue_label: Option<String>,
    pub max_turns: u32,
    pub notifications_enabled: bool,
    pub notification_sound: bool,
    pub max_retries: u32,
    pub retry_backoff_secs: u64,
    #[serde(default = "default_retry_base_delay")]
    pub retry_base_delay_secs: u64,
    #[serde(default = "default_retry_max_backoff")]
    pub retry_max_backoff_secs: u64,
    pub cleanup_on_failure: bool,
    pub cleanup_on_stop: bool,
    pub workspace_ttl_days: u32,
    pub max_concurrent_by_stage: HashMap<String, usize>,
    pub stage_prompts: HashMap<String, String>,
    pub hooks: LifecycleHooks,
    #[serde(default = "default_priority_labels")]
    pub priority_labels: Vec<String>,
    #[serde(default = "default_stall_timeout")]
    pub stall_timeout_secs: u64,
    #[serde(default = "default_stage_skip_labels")]
    pub stage_skip_labels: HashMap<String, Vec<String>>,
    pub approval_gates: HashMap<String, bool>,
    pub local_repos: HashMap<String, String>,
    pub custom_agent_command: String,
}

fn default_retry_base_delay() -> u64 {
    5
}

fn default_retry_max_backoff() -> u64 {
    300
}

fn default_stall_timeout() -> u64 {
    300
}

fn default_priority_labels() -> Vec<String> {
    vec![
        "priority:critical".to_string(),
        "priority:high".to_string(),
        "priority:medium".to_string(),
        "priority:low".to_string(),
    ]
}

fn default_stage_skip_labels() -> HashMap<String, Vec<String>> {
    let mut m = HashMap::new();
    m.insert("skip:code-review".to_string(), vec!["code_review".to_string()]);
    m.insert("skip:testing".to_string(), vec!["testing".to_string()]);
    m.insert("docs-only".to_string(), vec!["code_review".to_string(), "testing".to_string()]);
    m
}

impl Default for RunConfig {
    fn default() -> Self {
        Self {
            agent_type: "claude".to_string(),
            auto_approve: false,
            max_concurrent: 3,
            poll_interval_secs: 10,
            issue_label: None,
            max_turns: 30,
            notifications_enabled: true,
            notification_sound: true,
            max_retries: 3,
            retry_backoff_secs: 10,
            retry_base_delay_secs: default_retry_base_delay(),
            retry_max_backoff_secs: default_retry_max_backoff(),
            cleanup_on_failure: false,
            cleanup_on_stop: false,
            workspace_ttl_days: 7,
            max_concurrent_by_stage: HashMap::new(),
            stage_prompts: HashMap::new(),
            hooks: LifecycleHooks::default(),
            priority_labels: default_priority_labels(),
            stall_timeout_secs: default_stall_timeout(),
            stage_skip_labels: default_stage_skip_labels(),
            approval_gates: HashMap::new(),
            local_repos: HashMap::new(),
            custom_agent_command: String::new(),
        }
    }
}

/// Over-the-wire overview returned to update the frontend telemetry board.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OrchestratorOverview {
    pub is_running: bool,
    pub repos: Vec<String>,
    pub runs: Vec<RunSummary>,
    pub config: RunConfig,
    pub total_completed: usize,
    pub total_failed: usize,
    pub active_count: usize,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
}

/// The persistence state loaded and saved to the user's home space.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(default)]
pub struct PersistedState {
    pub repo: Option<String>,
    pub repos: Vec<String>,
    pub runs: HashMap<String, AgentRun>,
    pub total_input_tokens: u64,
    pub total_output_tokens: u64,
    pub total_cost_usd: f64,
    pub total_runtime_secs: f64,
    pub config: RunConfig,
}

/// GitHub repository mapping model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repo {
    pub full_name: String,
    pub name: String,
    pub owner: String,
    pub description: Option<String>,
    pub url: String,
    pub default_branch: String,
    pub is_private: bool,
}

/// GitHub issue mapping model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Issue {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub labels: Vec<String>,
    pub assignee: Option<String>,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
}

/// GitHub Pull Request mapping model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PullRequest {
    pub number: u64,
    pub title: String,
    pub body: Option<String>,
    pub state: String,
    pub head_branch: String,
    pub url: String,
    pub created_at: String,
    pub updated_at: String,
    pub author: Option<String>,
    pub closes_issue: Option<u64>,
}


// =================================================════========================
// SECTION 2: Legacy Canvas / DAG Types (Maintained for Compatibility)
// =================================================════========================

/// Status of a single node attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeStatus {
    Pending,
    Ready,
    Running,
    Stalled,
    Succeeded,
    Failed,
    Cancelled,
}

impl NodeStatus {
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            NodeStatus::Succeeded | NodeStatus::Failed | NodeStatus::Cancelled
        )
    }

    pub fn is_active(self) -> bool {
        matches!(self, NodeStatus::Ready | NodeStatus::Running)
    }

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
    Succeeded,
    Partial,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    Agent,
    Shell,
    Http,
}

impl Default for NodeKind {
    fn default() -> Self {
        NodeKind::Agent
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FailureMode {
    Abort,
    ContinueOthers,
    BranchOnly,
}

impl Default for FailureMode {
    fn default() -> Self {
        FailureMode::Abort
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RetryPolicy {
    pub max_attempts: u32,
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

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyNode {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: NodeKind,
    #[serde(default)]
    pub prompt: String,
    #[serde(default)]
    pub deps: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cost_cap_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_iterations: Option<usize>,
    #[serde(default)]
    pub retry: RetryPolicy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_create_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub after_run_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyEdge {
    pub from: String,
    pub to: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SymphonyWorkflowDef {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub per_run_cost_cap_usd: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_concurrent_nodes: Option<usize>,
    #[serde(default)]
    pub failure_mode: FailureMode,
    pub nodes: Vec<SymphonyNode>,
    #[serde(default)]
    pub edges: Vec<SymphonyEdge>,
}

impl SymphonyWorkflowDef {
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
