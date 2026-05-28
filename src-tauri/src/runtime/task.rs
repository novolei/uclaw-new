//! M1-T2a — `SessionTask` trait + `TaskKind` enum.
//!
//! Shared task-shape vocabulary for long-running session activities
//! (model+tool loops, review tasks, compaction). The `TaskScheduler`
//! preemption scaffold was removed in P2 of the 阶段 2 skeleton cleanup
//! (never wired into production; Slice 1a's `CancellationToken` covers
//! the cancellation surface it was designed for, and `agent/regular_task.rs`
//! drives `run_agentic_loop` directly).
//!
//! - [`TaskKind`] enum — discriminates the three current task shapes.
//! - [`SessionTask`] trait — the contract every long-running session
//!   activity implements.

use std::sync::Arc;

use async_trait::async_trait;
use tokio_util::sync::CancellationToken;

use crate::runtime::contracts::{TaskEvent, TaskSpec};

/// Kind of task in flight on a session. Each kind has different lifecycle
/// guarantees:
///
/// - `Regular`: model + tool loop driving an agent turn. Replaced by a
///   fresh `Regular` when the user submits a new message — the
///   in-flight one is cancelled via the `CancellationToken` installed on
///   the `ReasoningContext` (Slice 1a).
/// - `Review`: post-task self-evaluation. Runs to completion; **not**
///   cancellable by a new user message (so reviews are a faithful
///   record of what the agent actually did).
/// - `Compaction`: context summarization triggered when token budget
///   nears the cap. Runs in the background; new `Regular`s wait for
///   the in-flight compaction to land before starting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TaskKind {
    Regular,
    Review,
    Compaction,
}

impl TaskKind {
    /// Stable span name used for tracing / observability.
    pub fn span_name(self) -> &'static str {
        match self {
            Self::Regular => "session.task.regular",
            Self::Review => "session.task.review",
            Self::Compaction => "session.task.compaction",
        }
    }

    /// Whether this kind of task is replaceable by a fresh user message.
    /// `Regular` is, `Review` + `Compaction` aren't.
    pub fn is_user_preemptible(self) -> bool {
        matches!(self, Self::Regular)
    }
}

/// Contract every long-running session activity implements.
///
/// `run` MUST poll the cancellation token at every async await point.
/// The canonical pattern uses
/// `uclaw_utils_async_utils::OrCancelExt::or_cancel(&token)`:
///
/// ```ignore
/// use uclaw_utils_async_utils::OrCancelExt;
///
/// match some_future.or_cancel(&token).await {
///     Ok(value) => /* normal path */,
///     Err(_)    => return events, // cancelled — return what you have
/// }
/// ```
///
/// Implementations should be cheap to construct; heavy work happens in
/// `run`. The caller wraps each instance in an [`Arc`] before invoking.
#[async_trait]
pub trait SessionTask: Send + Sync {
    /// The task's identity for logging / rollout / preemption lookup.
    fn task_id(&self) -> &str;

    /// The kind discriminator. Drives [`is_user_preemptible`] etc.
    ///
    /// [`is_user_preemptible`]: TaskKind::is_user_preemptible
    fn kind(&self) -> TaskKind;

    /// The parsed [`TaskSpec`] this task is executing. Stored once at
    /// construction time; read by callers for budget / policy checks.
    fn task_spec(&self) -> &TaskSpec;

    /// Run the task to completion or cancellation.
    ///
    /// MUST be cancellation-aware: when `token.is_cancelled()` becomes
    /// true the task should clean up and return promptly (cancellation is
    /// delivered via the `CancellationToken` installed on the
    /// `ReasoningContext`, per Slice 1a).
    async fn run(self: Arc<Self>, token: CancellationToken) -> Vec<TaskEvent>;
}
