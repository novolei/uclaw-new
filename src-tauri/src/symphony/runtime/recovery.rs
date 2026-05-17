//! Restart reconciliation for Symphony runs.
//!
//! Called from `SymphonyService::start()` BEFORE the trigger channel opens
//! so the system reaches a consistent state before accepting new work.
//!
//! Strategy:
//!
//! 1. Any `symphony_node_runs` row with status `running` or `ready` and a
//!    stale `last_heartbeat_ms` (older than `stall_timeout_ms`) is marked
//!    `stalled`. This is the only state the in-process run loops cannot
//!    transition to themselves — if the app died, in-flight nodes are stuck
//!    in `running` forever without this sweep.
//! 2. Every `symphony_runs` row in `queued` or `running` is returned as a
//!    `RunResumeBlueprint`. `SymphonyService::start()` then constructs a
//!    `RunActor` for each one (T12).
//!
//! Note: a `running` node-run that crossed `stall_timeout_ms` becomes
//! `stalled`, not `failed`. The retry policy decides whether to fail
//! permanently — that decision happens inside `RunActor::apply_outcome`
//! the first time the resumed run loop ticks. We deliberately don't make
//! the call here because the retry counter (`attempt`) lives with the
//! workflow def, not in the recovery sweep.

use rusqlite::{params, Connection};

/// One run resumed by `SymphonyService::start()`. Just the identifying
/// columns — the workflow def + per-node state are reloaded by the actor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunResumeBlueprint {
    pub run_id: String,
    pub workflow_id: String,
    pub workflow_version: i64,
}

/// Mark stalled rows + return blueprints for all in-flight runs.
///
/// `now_ms` and `stall_threshold_ms` are passed in (rather than read from
/// system clock + config) so tests can drive the sweep deterministically.
pub fn reconcile(
    conn: &Connection,
    now_ms: i64,
    stall_threshold_ms: u64,
) -> rusqlite::Result<Vec<RunResumeBlueprint>> {
    // 1. Stall sweep on node-runs.
    let cutoff = now_ms.saturating_sub(stall_threshold_ms as i64);
    conn.execute(
        "UPDATE symphony_node_runs \
         SET status = 'stalled' \
         WHERE status IN ('running', 'ready') \
           AND COALESCE(last_heartbeat_ms, started_at_ms, 0) < ?1",
        params![cutoff],
    )?;

    // 2. Collect blueprints for in-flight runs.
    let mut stmt = conn.prepare(
        "SELECT id, workflow_id, workflow_version \
         FROM symphony_runs \
         WHERE status IN ('queued', 'running') \
         ORDER BY queued_at ASC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(RunResumeBlueprint {
            run_id: r.get(0)?,
            workflow_id: r.get(1)?,
            workflow_version: r.get(2)?,
        })
    })?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn insert_workflow(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO symphony_workflows \
             (id, name, current_version, enabled, created_at, updated_at) \
             VALUES (?1, 'wf', 1, 1, 1, 1)",
            [id],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO symphony_workflow_versions \
             (workflow_id, version, definition_yaml, definition_md, nodes_json, edges_json, created_at) \
             VALUES (?1, 1, 'y', 'm', '[]', '[]', 1)",
            [id],
        )
        .unwrap();
    }

    fn insert_run(conn: &Connection, run_id: &str, workflow_id: &str, status: &str, queued_at: i64) {
        conn.execute(
            "INSERT INTO symphony_runs \
             (id, workflow_id, workflow_version, trigger_kind, status, queued_at) \
             VALUES (?1, ?2, 1, 'manual', ?3, ?4)",
            params![run_id, workflow_id, status, queued_at],
        )
        .unwrap();
    }

    fn insert_node_run(
        conn: &Connection,
        nr_id: &str,
        run_id: &str,
        node_id: &str,
        status: &str,
        last_heartbeat_ms: Option<i64>,
    ) {
        conn.execute(
            "INSERT INTO symphony_node_runs \
             (id, run_id, node_id, attempt, status, last_heartbeat_ms, started_at_ms) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5, 0)",
            params![nr_id, run_id, node_id, status, last_heartbeat_ms],
        )
        .unwrap();
    }

    #[test]
    fn marks_stalled_when_heartbeat_too_old() {
        let conn = db();
        insert_workflow(&conn, "wf");
        insert_run(&conn, "r1", "wf", "running", 0);
        // alive: heartbeat just now
        insert_node_run(&conn, "nr-alive", "r1", "alive", "running", Some(900));
        // stalled: heartbeat way before cutoff
        insert_node_run(&conn, "nr-stale", "r1", "stale", "running", Some(100));
        // ready node with old heartbeat also stalls
        insert_node_run(&conn, "nr-ready", "r1", "ready", "ready", Some(100));

        reconcile(&conn, 1_000, 200).unwrap();

        let alive: String = conn
            .query_row(
                "SELECT status FROM symphony_node_runs WHERE id = 'nr-alive'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(alive, "running", "alive node must not be stalled");

        let stale: String = conn
            .query_row(
                "SELECT status FROM symphony_node_runs WHERE id = 'nr-stale'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(stale, "stalled");

        let ready: String = conn
            .query_row(
                "SELECT status FROM symphony_node_runs WHERE id = 'nr-ready'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(ready, "stalled");
    }

    #[test]
    fn does_not_touch_terminal_nodes() {
        let conn = db();
        insert_workflow(&conn, "wf");
        insert_run(&conn, "r1", "wf", "running", 0);
        insert_node_run(&conn, "nr-done", "r1", "done", "succeeded", Some(0));
        insert_node_run(&conn, "nr-fail", "r1", "fail", "failed", Some(0));

        reconcile(&conn, 1_000_000_000, 1).unwrap();

        let done: String = conn
            .query_row(
                "SELECT status FROM symphony_node_runs WHERE id = 'nr-done'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(done, "succeeded");

        let fail: String = conn
            .query_row(
                "SELECT status FROM symphony_node_runs WHERE id = 'nr-fail'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(fail, "failed");
    }

    #[test]
    fn returns_in_flight_runs_oldest_first() {
        let conn = db();
        insert_workflow(&conn, "wf");
        insert_run(&conn, "newer", "wf", "queued", 100);
        insert_run(&conn, "older", "wf", "running", 50);
        insert_run(&conn, "done", "wf", "completed", 60);
        insert_run(&conn, "failed", "wf", "failed", 70);

        let blueprints = reconcile(&conn, 1_000_000, 200).unwrap();
        assert_eq!(blueprints.len(), 2);
        assert_eq!(blueprints[0].run_id, "older");
        assert_eq!(blueprints[1].run_id, "newer");
        // Done / failed runs are skipped.
    }

    #[test]
    fn empty_db_returns_empty_blueprints() {
        let conn = db();
        let bp = reconcile(&conn, 1, 1).unwrap();
        assert!(bp.is_empty());
    }
}
