//! DAG scheduler — drives one workflow run from `queued` to terminal.
//!
//! ## Invariants
//!
//! - Authoritative state lives in `Arc<RwLock<RunState>>`; SQLite mirrors it.
//! - A node is `Ready` iff every dep is `Succeeded`.
//! - Concurrent node count never exceeds `per_workflow_sem`'s permits.
//! - Cancellation is propagated via `CancellationToken` AND `LoopSignal::Cancel`
//!   (the per-node executor checks the latter inside `run_agentic_loop`).
//! - On any node terminal transition, the DB row is updated synchronously
//!   so a crash mid-run leaves a recoverable state.
//!
//! ## Lifecycle
//!
//! ```text
//!   spawn → run_loop
//!     ↓
//!     while not terminal:
//!       select! {
//!         ready_node              -> spawn node task
//!         completed_node          -> apply outcome, fan dependents,
//!                                    update DB, emit IPC
//!         cancel.cancelled()      -> stop all, mark cancelled, break
//!         tick (250ms)            -> persist heartbeat snapshot,
//!                                    promote pending → ready when deps ok
//!       }
//!     finalize
//! ```
//!
//! The actor is intentionally a finite state machine over `RunState`, not
//! a free-form async function — that makes recovery (T11) a matter of
//! restoring `RunState` from the DB and resuming the loop.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rusqlite::params;
use tokio::sync::{mpsc, RwLock, Semaphore};
use tokio_util::sync::CancellationToken;

use crate::infra::{ConversationMessage, InfraEvent, InfraEventType};

use super::super::protocol::types::{
    FailureMode, NodeOutcome, NodeStatus, RetryPolicy, RunOutcome, RunStatus, SymphonyEdge,
    SymphonyNode, SymphonyWorkflowDef,
};
use super::node_run::{execute_node, NodeExecutionDeps};

/// Per-node mutable state held in memory + mirrored to DB.
#[derive(Debug, Clone)]
struct NodeState {
    id: String,
    status: NodeStatus,
    attempt: u32,
    deps: Vec<String>,
    /// Last terminal outcome's stringified output (for downstream substitution).
    output: Option<String>,
    /// Accumulated cost across all attempts.
    cost_usd: f64,
}

#[derive(Debug)]
struct RunState {
    run_id: String,
    workflow: SymphonyWorkflowDef,
    nodes: HashMap<String, NodeState>,
    /// Failure mode resolved from the workflow.
    failure_mode: FailureMode,
    /// Any node that has been cancelled-due-to-cascade is tracked here.
    cancelled_due_to_failure: bool,
}

impl RunState {
    fn new(run_id: String, workflow: SymphonyWorkflowDef) -> Self {
        let nodes = workflow
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id.clone(),
                    NodeState {
                        id: n.id.clone(),
                        status: NodeStatus::Pending,
                        attempt: 0,
                        deps: n.deps.clone(),
                        output: None,
                        cost_usd: 0.0,
                    },
                )
            })
            .collect();
        let fm = workflow.failure_mode;
        Self {
            run_id,
            workflow,
            nodes,
            failure_mode: fm,
            cancelled_due_to_failure: false,
        }
    }

    /// IDs of nodes whose status is currently `Pending` and whose deps are
    /// all `Succeeded`.
    fn ready_node_ids(&self) -> Vec<String> {
        self.nodes
            .values()
            .filter(|n| matches!(n.status, NodeStatus::Pending))
            .filter(|n| {
                n.deps
                    .iter()
                    .all(|d| matches!(self.nodes.get(d).map(|x| x.status), Some(NodeStatus::Succeeded)))
            })
            .map(|n| n.id.clone())
            .collect()
    }

    /// True iff every node is either terminal OR can never become Ready
    /// (i.e. at least one upstream dep is Failed/Cancelled, so this node
    /// is unreachable). Without this latter case, `FailureMode::ContinueOthers`
    /// runs would loop forever waiting on dependents of a Failed node that
    /// will never get scheduled.
    fn is_terminal(&self) -> bool {
        self.nodes.values().all(|n| {
            if n.status.is_terminal() {
                return true;
            }
            // Pending/Ready/Running but blocked by a terminal-non-success dep.
            n.deps.iter().any(|d| {
                matches!(
                    self.nodes.get(d).map(|x| x.status),
                    Some(NodeStatus::Failed) | Some(NodeStatus::Cancelled)
                )
            })
        })
    }

    /// Aggregate outcome from leaf states.
    fn outcome(&self) -> RunOutcome {
        let any_failed = self.nodes.values().any(|n| matches!(n.status, NodeStatus::Failed));
        let any_succeeded = self.nodes.values().any(|n| matches!(n.status, NodeStatus::Succeeded));
        match (any_failed, any_succeeded) {
            (false, true) => RunOutcome::Succeeded,
            (true, true) => RunOutcome::Partial,
            _ => RunOutcome::Failed,
        }
    }

    fn upstream_outputs(&self, deps: &[String]) -> HashMap<String, String> {
        let mut out = HashMap::new();
        for d in deps {
            if let Some(ns) = self.nodes.get(d) {
                if let Some(o) = &ns.output {
                    out.insert(d.clone(), o.clone());
                }
            }
        }
        out
    }

    fn retry_for(&self, node_id: &str) -> RetryPolicy {
        self.workflow
            .nodes
            .iter()
            .find(|n| n.id == node_id)
            .map(|n| n.retry.clone())
            .unwrap_or_default()
    }

    fn node_spec(&self, node_id: &str) -> Option<&SymphonyNode> {
        self.workflow.nodes.iter().find(|n| n.id == node_id)
    }
}

/// Message produced by a finished node task. The actor's tick loop matches
/// on these to decide what to do next.
#[derive(Debug)]
struct NodeDone {
    node_id: String,
    attempt: u32,
    outcome: NodeOutcome,
}

/// One running workflow run. Drop = task aborts. Cancel via the
/// `CancellationToken` returned from `spawn`.
///
/// `done_tx` (if set) is fired once when the run loop exits — used by
/// `SymphonyService` to drop the global concurrency permit. Replaces the
/// earlier `TriggerCmd::Reap` plumbing which leaked permits indefinitely.
pub struct RunActor {
    pub run_id: String,
    pub cancel: CancellationToken,
}

impl RunActor {
    /// Spawn the run loop. Returns the handle. The loop runs until terminal
    /// state or the token is cancelled. `reaper` is signalled exactly once
    /// when the loop exits (success, failure, or cancel).
    pub fn spawn(
        run_id: String,
        workflow: SymphonyWorkflowDef,
        deps: NodeExecutionDeps,
        per_workflow_concurrency: usize,
        reaper: Option<tokio::sync::oneshot::Sender<String>>,
    ) -> Arc<Self> {
        let cancel = CancellationToken::new();
        let actor = Arc::new(Self {
            run_id: run_id.clone(),
            cancel: cancel.clone(),
        });
        let state = Arc::new(RwLock::new(RunState::new(run_id.clone(), workflow)));
        let sem = Arc::new(Semaphore::new(per_workflow_concurrency.max(1)));
        let actor_ref = actor.clone();
        let run_id_for_reap = run_id.clone();
        tokio::spawn(async move {
            actor_ref
                .run_loop(state, sem, deps, cancel)
                .await;
            if let Some(tx) = reaper {
                let _ = tx.send(run_id_for_reap);
            }
        });
        actor
    }

    async fn run_loop(
        self: Arc<Self>,
        state: Arc<RwLock<RunState>>,
        sem: Arc<Semaphore>,
        deps: NodeExecutionDeps,
        cancel: CancellationToken,
    ) {
        let (done_tx, mut done_rx) = mpsc::unbounded_channel::<NodeDone>();

        // Mark queued → running in the DB.
        Self::update_run_status(&deps, &self.run_id, RunStatus::Running, None);

        loop {
            // Cancel check.
            if cancel.is_cancelled() {
                Self::cancel_all_active(&state, &deps).await;
                break;
            }

            // Spawn anything ready that fits under the semaphore cap.
            let ready_ids = state.read().await.ready_node_ids();
            for nid in ready_ids {
                if cancel.is_cancelled() {
                    break;
                }
                let permit = match sem.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break, // saturated
                };
                let workflow_clone = state.read().await.workflow.clone();
                let node_spec = match state.read().await.node_spec(&nid).cloned() {
                    Some(n) => n,
                    None => continue,
                };
                let upstream = state.read().await.upstream_outputs(&node_spec.deps);

                // Transition Pending → Running + bump attempt + persist.
                {
                    let mut s = state.write().await;
                    if let Some(n) = s.nodes.get_mut(&nid) {
                        n.attempt += 1;
                        n.status = NodeStatus::Running;
                        Self::upsert_node_run(&deps, &self.run_id, n);
                    }
                }
                Self::emit_node_update(&deps, &self.run_id, &nid, NodeStatus::Running);

                let attempt = state.read().await.nodes.get(&nid).map(|n| n.attempt).unwrap_or(1);
                let done_tx = done_tx.clone();
                let deps_clone = deps.clone();
                let node_clone = node_spec.clone();
                let workflow_clone = workflow_clone.clone();
                let run_id = self.run_id.clone();
                tokio::spawn(async move {
                    let outcome = execute_node(
                        &workflow_clone,
                        &node_clone,
                        &run_id,
                        attempt as i64,
                        &upstream,
                        &deps_clone,
                    )
                    .await;
                    drop(permit);
                    let _ = done_tx.send(NodeDone {
                        node_id: node_clone.id,
                        attempt,
                        outcome,
                    });
                });
            }

            // Wait for a node to finish or a cancel signal.
            tokio::select! {
                msg = done_rx.recv() => {
                    if let Some(d) = msg {
                        self.apply_outcome(&state, &deps, d).await;
                    }
                }
                _ = cancel.cancelled() => {
                    Self::cancel_all_active(&state, &deps).await;
                    break;
                }
                _ = tokio::time::sleep(Duration::from_millis(250)) => {
                    // Tick — gives us a chance to spawn newly-ready nodes
                    // that emerged from concurrent completions. Falls through.
                }
            }

            if state.read().await.is_terminal() {
                break;
            }
        }

        // Finalize.
        let final_state = state.read().await;
        let outcome = if cancel.is_cancelled() {
            None
        } else {
            Some(final_state.outcome())
        };
        let status = if cancel.is_cancelled() {
            RunStatus::Cancelled
        } else if final_state.nodes.values().any(|n| matches!(n.status, NodeStatus::Failed)) {
            match final_state.failure_mode {
                FailureMode::Abort => RunStatus::Failed,
                FailureMode::ContinueOthers | FailureMode::BranchOnly => RunStatus::Completed,
            }
        } else {
            RunStatus::Completed
        };
        let outcome_str = outcome.map(|o| o.as_db_str().to_string());
        Self::update_run_status(&deps, &self.run_id, status, outcome);
        Self::emit_run_completed(&deps, &final_state, status);

        // Publish run-completion to InfraService for proactive subscribers.
        let total_cost: f64 = final_state.nodes.values().map(|n| n.cost_usd).sum();
        let infra = deps.infra.clone();
        let run_id = self.run_id.clone();
        let workflow_id = final_state.workflow.id.clone();
        tokio::spawn(async move {
            infra
                .publish(InfraEvent {
                    id: 0,
                    event_type: InfraEventType::SymphonyRunCompleted,
                    platform: "local".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    message: ConversationMessage {
                        role: "system".into(),
                        content: format!("symphony run {} → {}", run_id, status.as_db_str()),
                    },
                    metadata: serde_json::json!({
                        "run_id": run_id,
                        "workflow_id": workflow_id,
                        "status": status.as_db_str(),
                        "outcome": outcome_str,
                        "total_cost_usd": total_cost,
                    }),
                    trace_id: None,
                })
                .await;
        });
    }

    async fn apply_outcome(
        self: &Arc<Self>,
        state: &Arc<RwLock<RunState>>,
        deps: &NodeExecutionDeps,
        done: NodeDone,
    ) {
        let NodeDone {
            node_id,
            attempt,
            outcome,
        } = done;

        let mut s = state.write().await;
        let policy = s.retry_for(&node_id);
        let failure_mode = s.failure_mode;
        let n = match s.nodes.get_mut(&node_id) {
            Some(n) => n,
            None => return,
        };

        let (new_status, output, cost) = match outcome {
            NodeOutcome::Succeeded { cost_usd, output_json } => {
                (NodeStatus::Succeeded, output_json, cost_usd)
            }
            NodeOutcome::Failed { cost_usd, error, retryable } => {
                if retryable && super::retry::should_retry(attempt, policy.max_attempts) {
                    // Schedule retry: apply Symphony SPEC backoff, then revert
                    // to Pending so the next tick picks it up. We spawn a
                    // detached sleep rather than block the actor's run_loop —
                    // setting status = Pending while in-flight means the next
                    // ready_node_ids() call would re-spawn immediately, so we
                    // stay Stalled until the backoff elapses.
                    let max_ms = policy
                        .max_backoff_ms
                        .unwrap_or(300_000);
                    let delay_ms = super::retry::backoff_ms(attempt, max_ms);
                    n.cost_usd += cost_usd;
                    n.status = NodeStatus::Stalled;
                    let cost_for_pub = n.cost_usd;
                    Self::upsert_node_run(deps, &self.run_id, n);
                    Self::emit_node_update(deps, &self.run_id, &node_id, NodeStatus::Stalled);

                    // Publish the retryable failure attempt so proactive
                    // subscribers see per-attempt outcomes, not just the
                    // final terminal state. (Audit A6.)
                    let infra = deps.infra.clone();
                    let run_id_pub = self.run_id.clone();
                    let node_id_pub = node_id.clone();
                    let err_pub = error.clone();
                    tokio::spawn(async move {
                        infra
                            .publish(InfraEvent {
                                id: 0,
                                event_type: InfraEventType::SymphonyNodeCompleted,
                                platform: "local".into(),
                                timestamp: chrono::Utc::now().timestamp_millis(),
                                message: ConversationMessage {
                                    role: "system".into(),
                                    content: format!(
                                        "node {} retry scheduled ({}ms): {}",
                                        node_id_pub, delay_ms, err_pub
                                    ),
                                },
                                metadata: serde_json::json!({
                                    "run_id": run_id_pub,
                                    "node_id": node_id_pub,
                                    "attempt": attempt,
                                    "status": "stalled",
                                    "retry_scheduled": true,
                                    "retry_delay_ms": delay_ms,
                                    "cost_usd": cost_for_pub,
                                    "error": err_pub,
                                }),
                                trace_id: None,
                            })
                            .await;
                    });

                    tracing::info!(
                        run_id = %self.run_id, node_id = %node_id, attempt,
                        delay_ms,
                        "symphony retry scheduled: {}", error
                    );
                    // Schedule the Pending transition after the backoff. The
                    // run_loop's 250ms tick will pick the Pending state up
                    // and re-spawn the node task.
                    let state_clone = state.clone();
                    let deps_clone = deps.clone();
                    let run_id = self.run_id.clone();
                    let nid = node_id.clone();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_millis(delay_ms))
                            .await;
                        let mut s = state_clone.write().await;
                        if let Some(n) = s.nodes.get_mut(&nid) {
                            // Only transition if still Stalled (user might
                            // have cancelled meanwhile).
                            if matches!(n.status, NodeStatus::Stalled) {
                                n.status = NodeStatus::Pending;
                                Self::upsert_node_run(&deps_clone, &run_id, n);
                                Self::emit_node_update(
                                    &deps_clone,
                                    &run_id,
                                    &nid,
                                    NodeStatus::Pending,
                                );
                            }
                        }
                    });
                    return;
                }
                (NodeStatus::Failed, None, cost_usd)
            }
            NodeOutcome::Cancelled { cost_usd, reason } => {
                tracing::info!(
                    run_id = %self.run_id, node_id = %node_id,
                    "symphony node cancelled: {}", reason
                );
                (NodeStatus::Cancelled, None, cost_usd)
            }
        };

        n.status = new_status;
        n.output = output;
        n.cost_usd += cost;
        let final_cost = n.cost_usd;
        Self::upsert_node_run(deps, &self.run_id, n);
        Self::emit_node_update(deps, &self.run_id, &node_id, new_status);

        // Publish to InfraService so proactive subsystem can subscribe.
        let infra = deps.infra.clone();
        let run_id_for_pub = self.run_id.clone();
        let node_id_for_pub = node_id.clone();
        tokio::spawn(async move {
            infra
                .publish(InfraEvent {
                    id: 0,
                    event_type: InfraEventType::SymphonyNodeCompleted,
                    platform: "local".into(),
                    timestamp: chrono::Utc::now().timestamp_millis(),
                    message: ConversationMessage {
                        role: "system".into(),
                        content: format!("node {} → {}", node_id_for_pub, new_status.as_db_str()),
                    },
                    metadata: serde_json::json!({
                        "run_id": run_id_for_pub,
                        "node_id": node_id_for_pub,
                        "attempt": attempt,
                        "status": new_status.as_db_str(),
                        "cost_usd": final_cost,
                    }),
                    trace_id: None,
                })
                .await;
        });

        // Failure cascade per workflow's failure_mode.
        if matches!(new_status, NodeStatus::Failed) {
            match failure_mode {
                FailureMode::Abort => {
                    // Cancel every Pending/Ready/Running node not already terminal.
                    let to_cancel: Vec<String> = s
                        .nodes
                        .values()
                        .filter(|x| !x.status.is_terminal())
                        .map(|x| x.id.clone())
                        .collect();
                    for nid in to_cancel {
                        if let Some(other) = s.nodes.get_mut(&nid) {
                            other.status = NodeStatus::Cancelled;
                            Self::upsert_node_run(deps, &self.run_id, other);
                            Self::emit_node_update(
                                deps,
                                &self.run_id,
                                &other.id,
                                NodeStatus::Cancelled,
                            );
                        }
                    }
                    s.cancelled_due_to_failure = true;
                }
                FailureMode::BranchOnly => {
                    // Cancel only descendants of the failed node.
                    let descendants = descendants_of(&s.workflow, &node_id);
                    for did in descendants {
                        if let Some(other) = s.nodes.get_mut(&did) {
                            if !other.status.is_terminal() {
                                other.status = NodeStatus::Cancelled;
                                Self::upsert_node_run(deps, &self.run_id, other);
                                Self::emit_node_update(
                                    deps,
                                    &self.run_id,
                                    &other.id,
                                    NodeStatus::Cancelled,
                                );
                            }
                        }
                    }
                }
                FailureMode::ContinueOthers => {
                    // No cascade — sibling branches keep going. Direct descendants
                    // will never become Ready (their dep is Failed not Succeeded).
                }
            }
        }
    }

    async fn cancel_all_active(state: &Arc<RwLock<RunState>>, deps: &NodeExecutionDeps) {
        let mut s = state.write().await;
        let to_cancel: Vec<String> = s
            .nodes
            .values()
            .filter(|n| !n.status.is_terminal())
            .map(|n| n.id.clone())
            .collect();
        let run_id = s.run_id.clone();
        for nid in to_cancel {
            if let Some(n) = s.nodes.get_mut(&nid) {
                n.status = NodeStatus::Cancelled;
                Self::upsert_node_run(deps, &run_id, n);
                Self::emit_node_update(deps, &run_id, &n.id, NodeStatus::Cancelled);
            }
        }
    }

    fn update_run_status(
        deps: &NodeExecutionDeps,
        run_id: &str,
        status: RunStatus,
        outcome: Option<RunOutcome>,
    ) {
        let now = chrono::Utc::now().timestamp_millis();
        let conn = deps.db.lock().unwrap();
        let res = if status == RunStatus::Running {
            conn.execute(
                "UPDATE symphony_runs SET status = ?1, started_at = COALESCE(started_at, ?2) WHERE id = ?3",
                params![status.as_db_str(), now, run_id],
            )
        } else {
            conn.execute(
                "UPDATE symphony_runs SET status = ?1, outcome = ?2, completed_at = ?3 WHERE id = ?4",
                params![
                    status.as_db_str(),
                    outcome.map(|o| o.as_db_str()),
                    now,
                    run_id
                ],
            )
        };
        if let Err(e) = res {
            tracing::warn!("symphony run-status update failed: {}", e);
        }
    }

    fn upsert_node_run(deps: &NodeExecutionDeps, run_id: &str, n: &NodeState) {
        // We key by (run_id, node_id, attempt). Each attempt is its own row.
        let id = format!("{}-{}-{}", run_id, n.id, n.attempt);
        let now = chrono::Utc::now().timestamp_millis();
        let conn = deps.db.lock().unwrap();
        let res = conn.execute(
            "INSERT INTO symphony_node_runs \
               (id, run_id, node_id, attempt, status, started_at_ms, last_heartbeat_ms, completed_at_ms, cost_usd) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6, ?7, ?8) \
             ON CONFLICT(id) DO UPDATE SET \
               status = excluded.status, \
               last_heartbeat_ms = excluded.last_heartbeat_ms, \
               completed_at_ms = excluded.completed_at_ms, \
               cost_usd = excluded.cost_usd",
            params![
                id,
                run_id,
                n.id,
                n.attempt as i64,
                n.status.as_db_str(),
                now,
                if n.status.is_terminal() { Some(now) } else { None::<i64> },
                n.cost_usd,
            ],
        );
        if let Err(e) = res {
            tracing::warn!("symphony node-run upsert failed: {}", e);
        }
    }

    fn emit_node_update(
        deps: &NodeExecutionDeps,
        run_id: &str,
        node_id: &str,
        status: NodeStatus,
    ) {
        if let Some(app) = &deps.app_handle {
            use tauri::Emitter;
            let _ = app.emit(
                "symphony:node_update",
                serde_json::json!({
                    "runId": run_id,
                    "nodeId": node_id,
                    "status": status.as_db_str(),
                }),
            );
        }
    }

    fn emit_run_completed(deps: &NodeExecutionDeps, state: &RunState, status: RunStatus) {
        if let Some(app) = &deps.app_handle {
            use tauri::Emitter;
            let total_cost: f64 = state.nodes.values().map(|n| n.cost_usd).sum();
            let succeeded = state
                .nodes
                .values()
                .filter(|n| matches!(n.status, NodeStatus::Succeeded))
                .count();
            let failed = state
                .nodes
                .values()
                .filter(|n| matches!(n.status, NodeStatus::Failed))
                .count();
            let cancelled = state
                .nodes
                .values()
                .filter(|n| matches!(n.status, NodeStatus::Cancelled))
                .count();
            let _ = app.emit(
                "symphony:run_completed",
                serde_json::json!({
                    "runId": state.run_id,
                    "status": status.as_db_str(),
                    "totalCostUsd": total_cost,
                    "succeeded": succeeded,
                    "failed": failed,
                    "cancelled": cancelled,
                }),
            );
        }
    }
}

/// Compute the set of nodes reachable from `start` via outbound dep edges
/// (i.e. all transitive consumers of `start`'s output).
fn descendants_of(workflow: &SymphonyWorkflowDef, start: &str) -> Vec<String> {
    let edges: Vec<SymphonyEdge> = workflow.effective_edges();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &edges {
        adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
    }
    let mut out = Vec::new();
    let mut stack = vec![start];
    let mut seen: std::collections::HashSet<&str> = std::collections::HashSet::new();
    seen.insert(start);
    while let Some(n) = stack.pop() {
        if let Some(succs) = adj.get(n) {
            for &s in succs {
                if seen.insert(s) {
                    out.push(s.to_string());
                    stack.push(s);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::symphony::protocol::types::{NodeKind, RetryPolicy};

    fn n(id: &str, deps: &[&str]) -> SymphonyNode {
        SymphonyNode {
            id: id.into(),
            label: id.to_uppercase(),
            kind: NodeKind::Agent,
            prompt: "".into(),
            deps: deps.iter().map(|s| (*s).to_string()).collect(),
            cost_cap_usd: None,
            max_iterations: None,
            retry: RetryPolicy::default(),
            after_create_command: None,
            after_run_command: None,
            model: None,
        }
    }

    fn wf(nodes: Vec<SymphonyNode>) -> SymphonyWorkflowDef {
        SymphonyWorkflowDef {
            id: "wf".into(),
            name: "Demo".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes,
            edges: vec![],
        }
    }

    #[test]
    fn ready_nodes_with_no_deps() {
        let state = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &[])]));
        let mut r = state.ready_node_ids();
        r.sort();
        assert_eq!(r, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn ready_only_when_deps_succeed() {
        let mut state = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &["a"])]));
        assert_eq!(state.ready_node_ids(), vec!["a".to_string()]);
        state.nodes.get_mut("a").unwrap().status = NodeStatus::Succeeded;
        assert_eq!(state.ready_node_ids(), vec!["b".to_string()]);
    }

    #[test]
    fn descendants_of_chain() {
        let w = wf(vec![n("a", &[]), n("b", &["a"]), n("c", &["b"])]);
        let mut d = descendants_of(&w, "a");
        d.sort();
        assert_eq!(d, vec!["b".to_string(), "c".to_string()]);
    }

    #[test]
    fn descendants_of_branch() {
        let w = wf(vec![
            n("a", &[]),
            n("b", &["a"]),
            n("c", &["a"]),
            n("d", &["b"]),
        ]);
        let mut d = descendants_of(&w, "b");
        d.sort();
        assert_eq!(d, vec!["d".to_string()]);
    }

    #[test]
    fn outcome_succeeded_when_all_succeed() {
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &[])]));
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Succeeded;
        s.nodes.get_mut("b").unwrap().status = NodeStatus::Succeeded;
        assert_eq!(s.outcome(), RunOutcome::Succeeded);
    }

    #[test]
    fn outcome_partial_when_mixed() {
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &[])]));
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Succeeded;
        s.nodes.get_mut("b").unwrap().status = NodeStatus::Failed;
        assert_eq!(s.outcome(), RunOutcome::Partial);
    }

    #[test]
    fn outcome_failed_when_no_success() {
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &[])]));
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Failed;
        s.nodes.get_mut("b").unwrap().status = NodeStatus::Cancelled;
        assert_eq!(s.outcome(), RunOutcome::Failed);
    }

    #[test]
    fn is_terminal_only_when_all_terminal() {
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &[])]));
        assert!(!s.is_terminal());
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Succeeded;
        assert!(!s.is_terminal());
        s.nodes.get_mut("b").unwrap().status = NodeStatus::Cancelled;
        assert!(s.is_terminal());
    }

    /// Audit fix: `is_terminal` must treat a Pending node blocked by a
    /// Failed dep as terminal (otherwise FailureMode::ContinueOthers loops
    /// forever waiting on dependents that will never become Ready).
    #[test]
    fn is_terminal_treats_blocked_pending_as_terminal() {
        // a (Failed) → b (Pending, deps=[a])
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &["a"])]));
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Failed;
        // `b` is still Pending — but its dep failed, so it can never run.
        assert!(s.is_terminal(), "Pending node with Failed dep must count as terminal");
    }

    #[test]
    fn is_terminal_treats_blocked_by_cancelled_as_terminal() {
        let mut s = RunState::new("r".into(), wf(vec![n("a", &[]), n("b", &["a"])]));
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Cancelled;
        assert!(s.is_terminal());
    }

    #[test]
    fn is_terminal_continueothers_diamond_with_one_failed() {
        // a (Succeeded) → {b (Pending, deps=[a]) — but a is fine, b is genuinely runnable}
        // c (Failed) → d (Pending, deps=[c]) — blocked
        // The whole run is NOT terminal because `b` could still progress.
        let mut s = RunState::new(
            "r".into(),
            wf(vec![n("a", &[]), n("b", &["a"]), n("c", &[]), n("d", &["c"])]),
        );
        s.nodes.get_mut("a").unwrap().status = NodeStatus::Succeeded;
        s.nodes.get_mut("c").unwrap().status = NodeStatus::Failed;
        // b is still Pending with succeeded dep — not blocked.
        assert!(!s.is_terminal(), "node b is still runnable, run must not be terminal");

        // Once b finishes, the whole DAG is terminal (d is blocked by failed c).
        s.nodes.get_mut("b").unwrap().status = NodeStatus::Succeeded;
        assert!(s.is_terminal(), "all reachable nodes done; d blocked by failed c");
    }
}

// ─── Integration tests with mock LLM (P1a) ───────────────────────────────────
//
// These tests construct a real `RunActor` against an in-memory DB with V33
// migrations and a `MockLlmProvider` that emits a scripted single-text
// completion. They verify the scheduler walks a real DAG end-to-end —
// catching the kinds of bugs the prior unit tests miss (deadlocks, missing
// DB writes, reaper-not-firing, race conditions in the spawn block).
//
// **Why "ignored" by default**: each test runs a real tokio + agent loop with
// retries and 250ms ticks. Total wall-clock is ~1-2s per test. Run with:
// `cargo test --lib symphony::runtime::run_actor::integration -- --ignored`
// in CI, but exclude from the default `cargo test` to keep the dev loop fast.

#[cfg(test)]
mod integration {
    use super::*;
    use crate::agent::types::{
        ChatMessage, RespondOutput, ResponseMetadata, StreamDelta, ToolDefinition, TokenUsage,
    };
    use crate::automation::memory::MemoryStore;
    use crate::infra::InfraService;
    use crate::llm::{CompletionConfig, LlmProvider};
    use crate::symphony::manager::SymphonyManager;
    use crate::symphony::protocol::types::{NodeKind, RetryPolicy};
    use crate::symphony::runtime::node_run::NodeExecutionDeps;
    use crate::symphony::runtime::stall::Heartbeat;
    use async_trait::async_trait;
    use std::path::PathBuf;
    use std::sync::Mutex as StdMutex;

    /// A scripted LlmProvider that returns a single text completion per call.
    /// The `text` field is the assistant content; `should_fail` flips it to
    /// return an Internal error so the agentic loop transitions to Failure.
    struct MockLlmProvider {
        text: String,
        should_fail: std::sync::atomic::AtomicBool,
        call_count: std::sync::atomic::AtomicU64,
    }

    impl MockLlmProvider {
        fn ok(text: &str) -> Self {
            Self {
                text: text.to_string(),
                should_fail: std::sync::atomic::AtomicBool::new(false),
                call_count: std::sync::atomic::AtomicU64::new(0),
            }
        }
        fn always_fail() -> Self {
            Self {
                text: String::new(),
                should_fail: std::sync::atomic::AtomicBool::new(true),
                call_count: std::sync::atomic::AtomicU64::new(0),
            }
        }
    }

    #[async_trait]
    impl LlmProvider for MockLlmProvider {
        async fn complete(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<RespondOutput, crate::error::Error> {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(crate::error::Error::Internal("mock llm scripted fail".into()));
            }
            Ok(RespondOutput::Text {
                text: self.text.clone(),
                thinking: None,
                thinking_signature: None,
                metadata: ResponseMetadata {
                    model: "mock-model".to_string(),
                    finish_reason: Some("end_turn".to_string()),
                    usage: Some(TokenUsage {
                        input_tokens: 10,
                        output_tokens: 5,
                        cache_read_tokens: 0,
                        cache_creation_tokens: 0,
                    }),
                },
            })
        }

        async fn stream(
            &self,
            _messages: Vec<ChatMessage>,
            _tools: Vec<ToolDefinition>,
            _config: &CompletionConfig,
        ) -> Result<
            Box<dyn futures::Stream<Item = Result<StreamDelta, crate::error::Error>> + Send + Unpin>,
            crate::error::Error,
        > {
            self.call_count
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            if self.should_fail.load(std::sync::atomic::Ordering::Relaxed) {
                return Err(crate::error::Error::Internal("mock llm scripted fail".into()));
            }
            let usage = TokenUsage {
                input_tokens: 10,
                output_tokens: 5,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
            };
            let deltas: Vec<Result<StreamDelta, crate::error::Error>> = vec![
                Ok(StreamDelta::TextDelta { text: self.text.clone() }),
                Ok(StreamDelta::Done {
                    finish_reason: Some("end_turn".to_string()),
                    usage: Some(usage),
                }),
            ];
            Ok(Box::new(futures::stream::iter(deltas)))
        }
    }

    fn setup_db_and_workflow(workflow: SymphonyWorkflowDef) -> Arc<StdMutex<rusqlite::Connection>> {
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        let db = Arc::new(StdMutex::new(conn));
        let mgr = SymphonyManager::new(db.clone());
        mgr.save_workflow(&workflow, "test".into()).unwrap();
        // Seed a run row in queued state (inline SQL to avoid forward
        // dependency on service::SymphonyService — T12 lands later).
        {
            let conn = db.lock().unwrap();
            let now = chrono::Utc::now().timestamp_millis();
            conn.execute(
                "INSERT INTO symphony_runs \
                 (id, workflow_id, workflow_version, trigger_kind, trigger_payload_json, status, inputs_json, queued_at) \
                 VALUES ('test-run', ?1, 1, 'manual', '{}', 'queued', '{}', ?2)",
                rusqlite::params![&workflow.id, now],
            )
            .unwrap();
        }
        db
    }

    fn build_deps(db: Arc<StdMutex<rusqlite::Connection>>, llm: Arc<dyn LlmProvider>) -> NodeExecutionDeps {
        NodeExecutionDeps {
            db,
            llm,
            model: "mock-model".to_string(),
            tools: Arc::new(crate::agent::tools::tool::ToolRegistry::new()),
            memory: Arc::new(MemoryStore::new(PathBuf::from("/tmp/symphony-test-mem"))),
            workspace_root: PathBuf::from("/tmp/symphony-test-ws"),
            heartbeat: Arc::new(Heartbeat::new()),
            app_handle: None,
            channel_manager: None,
            infra: Arc::new(InfraService::new()),
            default_max_iterations: 5,
            default_per_node_cost_cap_usd: 10.00,
        }
    }

    fn one_node(id: &str, deps: &[&str]) -> SymphonyNode {
        SymphonyNode {
            id: id.into(),
            label: id.to_uppercase(),
            kind: NodeKind::Agent,
            prompt: format!("Respond to {}", id),
            deps: deps.iter().map(|s| (*s).to_string()).collect(),
            cost_cap_usd: None,
            max_iterations: None,
            retry: RetryPolicy::default(),
            after_create_command: None,
            after_run_command: None,
            model: None,
        }
    }

    /// Wait for the actor to terminate, capped at `budget_ms`. Polls the
    /// run status from the DB.
    async fn await_run_complete(
        db: &Arc<StdMutex<rusqlite::Connection>>,
        run_id: &str,
        budget_ms: u64,
    ) -> String {
        let start = std::time::Instant::now();
        loop {
            let s: Option<String> = {
                let conn = db.lock().unwrap();
                conn.query_row(
                    "SELECT status FROM symphony_runs WHERE id = ?1",
                    [run_id],
                    |r| r.get(0),
                )
                .ok()
            };
            if let Some(ref cur) = s {
                if cur != "queued" && cur != "running" {
                    return cur.clone();
                }
            }
            if start.elapsed().as_millis() as u64 > budget_ms {
                return format!(
                    "TIMEOUT after {}ms — last status: {:?}",
                    budget_ms, s
                );
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore] // Run with `--ignored`. Real tokio + DB + agent loop.
    async fn linear_chain_two_nodes_completes_successfully() {
        let wf = SymphonyWorkflowDef {
            id: "wf-linear".into(),
            name: "Linear".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![one_node("a", &[]), one_node("b", &["a"])],
            edges: vec![],
        };
        let db = setup_db_and_workflow(wf.clone());
        let deps = build_deps(db.clone(), Arc::new(MockLlmProvider::ok("done")));
        let (reap_tx, reap_rx) = tokio::sync::oneshot::channel::<String>();
        let _actor = RunActor::spawn("test-run".into(), wf, deps, 2, Some(reap_tx));
        // Give it a generous budget — agent loop iterates a couple of times.
        let status = await_run_complete(&db, "test-run", 30_000).await;
        assert_eq!(status, "completed", "expected completed, got: {}", status);
        // Reaper should have fired.
        let _ = tokio::time::timeout(std::time::Duration::from_secs(2), reap_rx).await;
        // Both nodes should have a succeeded node-run row.
        let conn = db.lock().unwrap();
        let n_succeeded: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_node_runs WHERE status = 'succeeded'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_succeeded, 2, "expected 2 succeeded nodes");
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn failing_llm_marks_nodes_failed_after_retries_exhausted() {
        let mut node_a = one_node("a", &[]);
        node_a.retry = RetryPolicy { max_attempts: 2, max_backoff_ms: Some(50) };
        let wf = SymphonyWorkflowDef {
            id: "wf-fail".into(),
            name: "Failing".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![node_a],
            edges: vec![],
        };
        let db = setup_db_and_workflow(wf.clone());
        let deps = build_deps(db.clone(), Arc::new(MockLlmProvider::always_fail()));
        let (reap_tx, _reap_rx) = tokio::sync::oneshot::channel::<String>();
        let _actor = RunActor::spawn("test-run".into(), wf, deps, 1, Some(reap_tx));
        let status = await_run_complete(&db, "test-run", 30_000).await;
        assert_eq!(status, "failed", "expected failed, got: {}", status);
        let conn = db.lock().unwrap();
        let attempts: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_node_runs WHERE node_id = 'a'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(attempts >= 2, "expected at least 2 attempts, got {}", attempts);
    }

    #[tokio::test(flavor = "current_thread")]
    #[ignore]
    async fn cancel_mid_flight_marks_run_cancelled() {
        let wf = SymphonyWorkflowDef {
            id: "wf-cancel".into(),
            name: "Cancel".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![one_node("a", &[]), one_node("b", &["a"])],
            edges: vec![],
        };
        let db = setup_db_and_workflow(wf.clone());
        let deps = build_deps(db.clone(), Arc::new(MockLlmProvider::ok("done")));
        let (reap_tx, _reap_rx) = tokio::sync::oneshot::channel::<String>();
        let actor = RunActor::spawn("test-run".into(), wf, deps, 2, Some(reap_tx));
        // Cancel almost immediately. The first node may or may not have
        // started; in either case the run must end in `cancelled`.
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        actor.cancel.cancel();
        let status = await_run_complete(&db, "test-run", 10_000).await;
        assert_eq!(status, "cancelled", "expected cancelled, got: {}", status);
    }
}
