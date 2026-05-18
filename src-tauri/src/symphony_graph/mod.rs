//! Symphony — DAG-of-agent-runs runtime.
//!
//! Symphony is uClaw's third top-level execution runtime, parallel to Chat
//! and Agent. It orchestrates a directed acyclic graph of `HeadlessDelegate`
//! invocations, with edges describing handoffs, a visual canvas as the
//! authoring surface, and the same cost / safety / memory machinery the
//! Chat and Automation runtimes already use.
//!
//! Design spec: `docs/superpowers/specs/2026-05-17-symphony-runtime-design.md`.
//! Implementation plan: `docs/superpowers/plans/symphony-runtime.md`.
//!
//! ## Module map
//!
//! - `protocol/` — types (`SymphonyWorkflowDef`, `SymphonyNode`, `SymphonyEdge`,
//!   `NodeStatus`, `RunStatus`, …), the WORKFLOW.md parser (YAML front matter
//!   + Markdown prompt body), and the def-↔-DB-row normalizer.
//! - `manager.rs` (T5) — CRUD over `symphony_workflows` and
//!   `symphony_workflow_versions`.
//! - `runtime/` (T6–T12) — the live execution layer: cost caps, retry, per-
//!   node executor, DAG scheduler, stall detection, recovery, the
//!   `SymphonyService: ManagedService` impl.
//! - `tools/` (Phase 2) — Symphony-specific tools such as `record_handoff`.
//! - `sources/` (Phase 2) — workflow trigger sources (manual today; Linear /
//!   GitHub Issues / cron later).

pub mod manager;
pub mod protocol;
pub mod runtime;

#[cfg(test)]
mod integration_test {
    //! End-to-end persistence test that exercises:
    //! - V33 migrations
    //! - SymphonyManager save / get
    //! - Workflow versioning
    //! - DAG validation
    //! - run + node_run persistence shape
    //!
    //! Does NOT exercise the live `run_agentic_loop` path (requires a real
    //! LLM provider). That path is covered indirectly by per-module unit
    //! tests in `runtime/node_run.rs` and `runtime/run_actor.rs`.

    use std::sync::{Arc, Mutex};

    use rusqlite::Connection;

    use super::manager::SymphonyManager;
    use super::protocol::types::{
        FailureMode, NodeKind, NodeStatus, RetryPolicy, RunStatus, SymphonyEdge, SymphonyNode,
        SymphonyWorkflowDef,
    };

    fn db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn linear_three_node_workflow() -> SymphonyWorkflowDef {
        SymphonyWorkflowDef {
            id: "wf-smoke".into(),
            name: "Smoke Test".into(),
            description: Some("3-node linear chain for integration test".into()),
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: FailureMode::Abort,
            nodes: vec![
                SymphonyNode {
                    id: "fetch".into(),
                    label: "Fetch".into(),
                    kind: NodeKind::Agent,
                    prompt: "Fetch the data".into(),
                    deps: vec![],
                    cost_cap_usd: None,
                    max_iterations: None,
                    retry: RetryPolicy::default(),
                    after_create_command: None,
                    after_run_command: None,
                    model: None,
                },
                SymphonyNode {
                    id: "process".into(),
                    label: "Process".into(),
                    kind: NodeKind::Agent,
                    prompt: "Process: {{ upstream.fetch.output }}".into(),
                    deps: vec!["fetch".into()],
                    cost_cap_usd: None,
                    max_iterations: None,
                    retry: RetryPolicy::default(),
                    after_create_command: None,
                    after_run_command: None,
                    model: None,
                },
                SymphonyNode {
                    id: "report".into(),
                    label: "Report".into(),
                    kind: NodeKind::Agent,
                    prompt: "Report on: {{ upstream.process.output }}".into(),
                    deps: vec!["process".into()],
                    cost_cap_usd: None,
                    max_iterations: None,
                    retry: RetryPolicy::default(),
                    after_create_command: None,
                    after_run_command: None,
                    model: None,
                },
            ],
            edges: vec![],
        }
    }

    #[test]
    fn save_three_node_workflow_then_load_back() {
        let mgr = SymphonyManager::new(db());
        let def = linear_three_node_workflow();
        let (id, v) = mgr.save_workflow(&def, "raw md".into()).unwrap();
        assert_eq!(id, "wf-smoke");
        assert_eq!(v, 1);

        let detail = mgr.get_workflow("wf-smoke").unwrap();
        assert_eq!(detail.def.nodes.len(), 3);
        // Effective edges = 2 (fetch→process, process→report).
        assert_eq!(detail.def.effective_edges().len(), 2);
    }

    #[test]
    fn second_save_creates_new_version() {
        let mgr = SymphonyManager::new(db());
        let mut def = linear_three_node_workflow();
        let (_, v1) = mgr.save_workflow(&def, "v1".into()).unwrap();
        // Mutate the def and save again.
        def.description = Some("updated".into());
        let (_, v2) = mgr.save_workflow(&def, "v2".into()).unwrap();
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);
        let detail = mgr.get_workflow("wf-smoke").unwrap();
        assert_eq!(detail.row.current_version, 2);
        assert_eq!(detail.def.description, Some("updated".into()));
    }

    #[test]
    fn save_rejects_cycle_before_writing_anything() {
        let mgr = SymphonyManager::new(db());
        let mut def = linear_three_node_workflow();
        // Insert a cycle: report → fetch.
        def.edges = vec![
            SymphonyEdge { from: "fetch".into(), to: "process".into(), label: None },
            SymphonyEdge { from: "process".into(), to: "report".into(), label: None },
            SymphonyEdge { from: "report".into(), to: "fetch".into(), label: None },
        ];
        let err = mgr.save_workflow(&def, "bad".into()).unwrap_err();
        assert!(
            format!("{}", err).contains("Cycle") || format!("{}", err).contains("cycle"),
            "expected cycle error, got: {}",
            err
        );
    }

    #[test]
    fn ready_node_progression_through_dag() {
        // Simulates what RunActor's ready-set computation would surface
        // without spinning up the actor.
        let def = linear_three_node_workflow();
        // Stand-in for the runtime's RunState: track status per node id.
        let mut status: std::collections::HashMap<&str, NodeStatus> =
            std::collections::HashMap::new();
        for n in &def.nodes {
            status.insert(n.id.as_str(), NodeStatus::Pending);
        }
        let ready = |status: &std::collections::HashMap<&str, NodeStatus>| -> Vec<String> {
            def.nodes
                .iter()
                .filter(|n| matches!(status.get(n.id.as_str()), Some(NodeStatus::Pending)))
                .filter(|n| {
                    n.deps
                        .iter()
                        .all(|d| matches!(status.get(d.as_str()), Some(NodeStatus::Succeeded)))
                })
                .map(|n| n.id.clone())
                .collect()
        };
        // Initial: only `fetch` is ready.
        assert_eq!(ready(&status), vec!["fetch".to_string()]);
        // After fetch: `process` becomes ready.
        status.insert("fetch", NodeStatus::Succeeded);
        assert_eq!(ready(&status), vec!["process".to_string()]);
        // After process: `report` becomes ready.
        status.insert("process", NodeStatus::Succeeded);
        assert_eq!(ready(&status), vec!["report".to_string()]);
        // After report: nothing left.
        status.insert("report", NodeStatus::Succeeded);
        assert!(ready(&status).is_empty());
    }

    #[test]
    fn run_row_lifecycle_persists_through_phases() {
        let db_arc = db();
        let mgr = SymphonyManager::new(db_arc.clone());
        let def = linear_three_node_workflow();
        mgr.save_workflow(&def, "".into()).unwrap();
        let conn = db_arc.lock().unwrap();

        // Insert a queued run.
        crate::symphony_graph::runtime::service::SymphonyService::create_run_row(
            &conn,
            "run-test",
            "wf-smoke",
            1,
            "manual",
            "{}",
        )
        .unwrap();

        // Status should be queued.
        let s: String = conn
            .query_row(
                "SELECT status FROM symphony_runs WHERE id = 'run-test'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(s, "queued");

        // Update to running, then completed.
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "UPDATE symphony_runs SET status = 'running', started_at = ?1 WHERE id = 'run-test'",
            [&now],
        )
        .unwrap();
        conn.execute(
            "UPDATE symphony_runs SET status = ?1, outcome = ?2, completed_at = ?3 WHERE id = 'run-test'",
            rusqlite::params![
                RunStatus::Completed.as_db_str(),
                "succeeded",
                now + 1
            ],
        )
        .unwrap();
        let (final_status, outcome): (String, Option<String>) = conn
            .query_row(
                "SELECT status, outcome FROM symphony_runs WHERE id = 'run-test'",
                [],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .unwrap();
        assert_eq!(final_status, "completed");
        assert_eq!(outcome.as_deref(), Some("succeeded"));
    }

    #[test]
    fn delete_workflow_cascades_to_runs_and_node_runs() {
        let db_arc = db();
        let mgr = SymphonyManager::new(db_arc.clone());
        mgr.save_workflow(&linear_three_node_workflow(), "".into()).unwrap();
        {
            let conn = db_arc.lock().unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
            crate::symphony_graph::runtime::service::SymphonyService::create_run_row(
                &conn,
                "r-cascade",
                "wf-smoke",
                1,
                "manual",
                "{}",
            )
            .unwrap();
            conn.execute(
                "INSERT INTO symphony_node_runs (id, run_id, node_id, attempt, status) \
                 VALUES ('nr-1', 'r-cascade', 'fetch', 1, 'running')",
                [],
            )
            .unwrap();
        }

        mgr.delete_workflow("wf-smoke").unwrap();

        let conn = db_arc.lock().unwrap();
        let n_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_runs, 0, "runs must cascade-delete with workflow");
        let n_node_runs: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_node_runs", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n_node_runs, 0, "node_runs must cascade through runs");
    }
}
