//! M1-T2a — `SessionTask` trait + preemptive task scheduling scaffold.
//!
//! This is the dispatch layer that M1-T2c will wrap
//! `agent::agentic_loop::run_agentic_loop` in. Pure scaffolding for now:
//!
//! - [`SessionTask`] trait — the contract every long-running session
//!   activity implements (model+tool loops, review tasks, compaction).
//! - [`TaskKind`] enum — discriminates the three current task shapes.
//! - [`TaskScheduler`] — holds in-flight tasks, can preempt them all
//!   with a 100ms graceful-shutdown window before hard-aborting.
//! - [`TaskTermination`] — how each task ended.
//!
//! `M1-T2c` wires the existing 882-line `run_agentic_loop` into a
//! `RegularTask` implementation of this trait. Nothing here calls into
//! the agent loop yet.

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use crate::runtime::contracts::{TaskEvent, TaskSpec};

/// Kind of task in flight on a session. Each kind has different lifecycle
/// guarantees:
///
/// - `Regular`: model + tool loop driving an agent turn. Replaced by a
///   fresh `Regular` when the user submits a new message — the
///   in-flight one is preempted via [`TaskScheduler::abort_all_tasks`].
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

/// How a task ended. Returned by [`TaskScheduler::abort_all_tasks`] and
/// surfaced into the rollout JSONL once M1-T5 lands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskTermination {
    /// Task ran to completion before being preempted.
    Completed,
    /// Task observed its cancellation token and wound down gracefully
    /// within the [`GRACEFUL_SHUTDOWN_TIMEOUT`] window.
    Cancelled,
    /// Task didn't finish within the graceful window after cancellation
    /// — `tokio::task::JoinHandle::abort()` was called.
    GracefulDeadlineExceeded,
    /// Task panicked.
    Panicked(String),
}

/// Maximum time a task is given to wind down after cancellation before
/// the scheduler hard-aborts it.
///
/// Codex uses the same 100ms budget (`GRACEFUL_INTERRUPTION_TIMEOUT_MS`)
/// — short enough that the user doesn't notice, long enough that any
/// HTTP response in flight has a chance to close cleanly.
pub const GRACEFUL_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(100);

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
/// Implementations should be cheap to construct (heavy work happens in
/// `run`) since the scheduler holds them behind an [`Arc`].
#[async_trait]
pub trait SessionTask: Send + Sync {
    /// The task's identity for logging / rollout / preemption lookup.
    fn task_id(&self) -> &str;

    /// The kind discriminator. Drives [`is_user_preemptible`] etc.
    ///
    /// [`is_user_preemptible`]: TaskKind::is_user_preemptible
    fn kind(&self) -> TaskKind;

    /// The parsed [`TaskSpec`] this task is executing. Stored once at
    /// construction time; the scheduler reads it for budget / policy
    /// checks.
    fn task_spec(&self) -> &TaskSpec;

    /// Run the task to completion or cancellation.
    ///
    /// MUST be cancellation-aware: when `token.is_cancelled()` becomes
    /// true the task should clean up and return within
    /// [`GRACEFUL_SHUTDOWN_TIMEOUT`].
    async fn run(self: Arc<Self>, token: CancellationToken) -> Vec<TaskEvent>;
}

/// A handle to a task running on a scheduler. The owning scheduler
/// retains a copy of the cancellation token so it can preempt the task.
pub struct SpawnedTask {
    pub task_id: String,
    pub kind: TaskKind,
    pub token: CancellationToken,
    pub handle: JoinHandle<Vec<TaskEvent>>,
}

/// Per-session task scheduler. Tracks every in-flight task so the
/// session can preempt them on a new user message or shutdown.
///
/// This is the type [`crate::agent::session::Session`] will hold once
/// M1-T2c wires it in. Today it's freestanding.
#[derive(Default)]
pub struct TaskScheduler {
    in_flight: Mutex<Vec<SpawnedTask>>,
}

impl TaskScheduler {
    /// Empty scheduler.
    pub fn new() -> Self {
        Self::default()
    }

    /// Spawn `task` on the current tokio runtime and track its handle.
    ///
    /// Returns the cancellation token so callers can later preempt this
    /// specific task without affecting siblings. The same token is
    /// retained inside the scheduler, so [`Self::abort_all_tasks`]
    /// preempts every in-flight task in one call.
    pub async fn spawn_task(&self, task: Arc<dyn SessionTask>) -> CancellationToken {
        let token = CancellationToken::new();
        let task_id = task.task_id().to_string();
        let kind = task.kind();
        let token_for_task = token.clone();
        let handle = tokio::spawn(async move { task.run(token_for_task).await });

        let mut lock = self.in_flight.lock().await;
        lock.push(SpawnedTask {
            task_id,
            kind,
            token: token.clone(),
            handle,
        });
        token
    }

    /// Number of tasks currently tracked.
    pub async fn in_flight_count(&self) -> usize {
        self.in_flight.lock().await.len()
    }

    /// Preempt + reap every in-flight task. For each task:
    ///
    /// 1. Cancel its token.
    /// 2. Wait up to [`GRACEFUL_SHUTDOWN_TIMEOUT`] for the task to
    ///    return (giving it a chance to flush events, close HTTP
    ///    streams, etc.).
    /// 3. If the deadline is exceeded, hard-abort the join handle.
    ///
    /// Returns `(task_id, termination)` pairs in spawn order.
    pub async fn abort_all_tasks(&self) -> Vec<(String, TaskTermination)> {
        let drained: Vec<SpawnedTask> = {
            let mut lock = self.in_flight.lock().await;
            lock.drain(..).collect()
        };

        let mut terminations = Vec::with_capacity(drained.len());
        for spawned in drained {
            let SpawnedTask {
                task_id,
                token,
                handle,
                ..
            } = spawned;
            token.cancel();
            match tokio::time::timeout(GRACEFUL_SHUTDOWN_TIMEOUT, handle).await {
                Ok(Ok(_events)) => {
                    terminations.push((task_id, TaskTermination::Cancelled));
                }
                Ok(Err(join_err)) => {
                    if join_err.is_panic() {
                        terminations
                            .push((task_id, TaskTermination::Panicked(format!("{join_err:?}"))));
                    } else {
                        terminations.push((task_id, TaskTermination::Cancelled));
                    }
                }
                Err(_elapsed) => {
                    // Deadline exceeded — the JoinHandle from the
                    // timeout is consumed, so the spawned future will
                    // be dropped on the next tokio poll. Record it.
                    terminations.push((task_id, TaskTermination::GracefulDeadlineExceeded));
                }
            }
        }
        terminations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::contracts::{
        AutonomyLevel, BudgetSpec, CheckpointPolicy, OutputContract, PolicySpec,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};
    use uclaw_utils_async_utils::OrCancelExt;

    fn make_task_spec(id: &str) -> TaskSpec {
        TaskSpec {
            id: id.into(),
            intent_id: format!("intent-{id}"),
            goal: "test task".into(),
            plan_ref: None,
            policy: PolicySpec {
                effective_autonomy: AutonomyLevel::SupervisedTask,
                require_step_approval: false,
                tool_permission_rule_ids: vec![],
            },
            budget: BudgetSpec::default(),
            capability_profile: "default".into(),
            output_contract: OutputContract::FreeText,
            checkpoint_policy: CheckpointPolicy::PerTurn,
        }
    }

    /// Test fixture: completes quickly, returns one TaskFinished event.
    struct FastTask {
        spec: TaskSpec,
    }

    #[async_trait]
    impl SessionTask for FastTask {
        fn task_id(&self) -> &str {
            &self.spec.id
        }
        fn kind(&self) -> TaskKind {
            TaskKind::Regular
        }
        fn task_spec(&self) -> &TaskSpec {
            &self.spec
        }
        async fn run(self: Arc<Self>, _token: CancellationToken) -> Vec<TaskEvent> {
            vec![]
        }
    }

    /// Test fixture: cooperative — checks the token at every yield point.
    /// Returns immediately on cancellation.
    struct CooperativeTask {
        spec: TaskSpec,
        cancellations_observed: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl SessionTask for CooperativeTask {
        fn task_id(&self) -> &str {
            &self.spec.id
        }
        fn kind(&self) -> TaskKind {
            TaskKind::Regular
        }
        fn task_spec(&self) -> &TaskSpec {
            &self.spec
        }
        async fn run(self: Arc<Self>, token: CancellationToken) -> Vec<TaskEvent> {
            for _ in 0..1000 {
                let sleep = tokio::time::sleep(Duration::from_millis(50));
                if sleep.or_cancel(&token).await.is_err() {
                    self.cancellations_observed.fetch_add(1, Ordering::SeqCst);
                    return vec![];
                }
            }
            vec![]
        }
    }

    /// Test fixture: ignores cancellation. Used to verify the
    /// graceful-deadline path hard-aborts.
    struct UncooperativeTask {
        spec: TaskSpec,
    }

    #[async_trait]
    impl SessionTask for UncooperativeTask {
        fn task_id(&self) -> &str {
            &self.spec.id
        }
        fn kind(&self) -> TaskKind {
            TaskKind::Regular
        }
        fn task_spec(&self) -> &TaskSpec {
            &self.spec
        }
        async fn run(self: Arc<Self>, _token: CancellationToken) -> Vec<TaskEvent> {
            // Sleep way longer than the graceful window without checking
            // the token. The scheduler must hard-abort us.
            tokio::time::sleep(Duration::from_secs(5)).await;
            vec![]
        }
    }

    // ── TaskKind ───────────────────────────────────────────────────

    #[test]
    fn task_kind_span_names_unique() {
        let kinds = [TaskKind::Regular, TaskKind::Review, TaskKind::Compaction];
        let names: Vec<&'static str> = kinds.iter().map(|k| k.span_name()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3);
    }

    #[test]
    fn only_regular_is_user_preemptible() {
        assert!(TaskKind::Regular.is_user_preemptible());
        assert!(!TaskKind::Review.is_user_preemptible());
        assert!(!TaskKind::Compaction.is_user_preemptible());
    }

    // ── TaskScheduler ─────────────────────────────────────────────

    #[tokio::test]
    async fn scheduler_tracks_in_flight_count() {
        let scheduler = TaskScheduler::new();
        assert_eq!(scheduler.in_flight_count().await, 0);

        let task = Arc::new(FastTask {
            spec: make_task_spec("fast-1"),
        });
        let _token = scheduler.spawn_task(task).await;
        assert_eq!(scheduler.in_flight_count().await, 1);

        // Reaping drains the in-flight queue.
        let _ = scheduler.abort_all_tasks().await;
        assert_eq!(scheduler.in_flight_count().await, 0);
    }

    #[tokio::test]
    async fn cooperative_task_observes_cancellation_within_graceful_window() {
        let scheduler = TaskScheduler::new();
        let observed = Arc::new(AtomicUsize::new(0));
        for n in 0..3 {
            let task = Arc::new(CooperativeTask {
                spec: make_task_spec(&format!("coop-{n}")),
                cancellations_observed: observed.clone(),
            });
            scheduler.spawn_task(task).await;
        }
        assert_eq!(scheduler.in_flight_count().await, 3);

        let started = std::time::Instant::now();
        let terminations = scheduler.abort_all_tasks().await;
        let elapsed = started.elapsed();

        assert_eq!(terminations.len(), 3);
        for (_, term) in &terminations {
            assert_eq!(
                *term,
                TaskTermination::Cancelled,
                "cooperative tasks should wind down gracefully, got {term:?}"
            );
        }
        // All 3 tasks observed the cancellation flag.
        assert_eq!(observed.load(Ordering::SeqCst), 3);
        // Should be well under 3 × GRACEFUL_SHUTDOWN_TIMEOUT since they
        // wind down cooperatively (~one sleep tick each).
        assert!(
            elapsed < Duration::from_millis(500),
            "graceful path was too slow: {elapsed:?}"
        );
    }

    #[tokio::test]
    async fn uncooperative_task_hits_graceful_deadline() {
        let scheduler = TaskScheduler::new();
        let task = Arc::new(UncooperativeTask {
            spec: make_task_spec("ignores-token"),
        });
        scheduler.spawn_task(task).await;

        let started = std::time::Instant::now();
        let terminations = scheduler.abort_all_tasks().await;
        let elapsed = started.elapsed();

        assert_eq!(terminations.len(), 1);
        assert_eq!(
            terminations[0].1,
            TaskTermination::GracefulDeadlineExceeded
        );
        // Should hard-abort within ~GRACEFUL_SHUTDOWN_TIMEOUT (plus some
        // scheduler overhead — 250ms is a comfortable upper bound).
        assert!(
            elapsed < Duration::from_millis(250),
            "deadline path took too long: {elapsed:?} — task may not have been aborted"
        );
    }

    #[tokio::test]
    async fn abort_drains_scheduler() {
        let scheduler = TaskScheduler::new();
        for n in 0..5 {
            let task = Arc::new(FastTask {
                spec: make_task_spec(&format!("fast-{n}")),
            });
            scheduler.spawn_task(task).await;
        }
        // Even though FastTask completes instantly, abort_all_tasks
        // should drain the in-flight queue.
        let _ = scheduler.abort_all_tasks().await;
        assert_eq!(scheduler.in_flight_count().await, 0);
    }
}
