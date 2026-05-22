//! Symphony protocol — WORKFLOW.md schema, types, parser, validator.
//!
//! Mirrors the structure of `automation::protocol`:
//! - `types`     — pure data definitions, no I/O.
//! - `parse`     — YAML-front-matter + Markdown-body parser (`serde_yml`).
//! - `normalize` — `SymphonyWorkflowDef` ↔ DB-row conversions + cycle check.

pub mod normalize;
pub mod parse;
pub mod types;

pub use normalize::{
    def_to_version_row, validate_dag, version_row_to_def, NormalizeError,
    NormalizedVersionRow,
};
pub use parse::{parse_workflow_md, ParseError};
pub use types::{
    AgentStatus, AgentRun, FailureMode, Issue, LifecycleHooks, NodeKind, NodeOutcome,
    NodeStatus, OrchestratorOverview, PersistedState, PipelineReport, PipelineStage, PullRequest,
    Repo, RetryPolicy, RunConfig, RunOutcome, RunStatus, RunSummary, StageContext, StageReport,
    SymphonyEdge, SymphonyNode, SymphonyWorkflowDef,
};

