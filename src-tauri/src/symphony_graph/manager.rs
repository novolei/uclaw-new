//! `SymphonyManager` — CRUD over `symphony_workflows` and `symphony_workflow_versions`.
//!
//! Save semantics:
//! - Validates the DAG before any DB writes (`normalize::validate_dag`).
//! - On first save creates the `symphony_workflows` row with `current_version = 1`
//!   and writes a `symphony_workflow_versions` row at version 1.
//! - On re-save increments `current_version`, writes a new immutable
//!   `symphony_workflow_versions` row at the new version, and updates the
//!   pointer + `updated_at`.
//! - Returns `(workflow_id, version)` so the caller can immediately address
//!   the saved snapshot (used by `symphony_save_workflow` Tauri command in T14).

use std::sync::Mutex;

use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
use thiserror::Error;

use super::protocol::{
    def_to_version_row, parse_workflow_md, validate_dag, version_row_to_def, NormalizeError,
    ParseError, SymphonyWorkflowDef,
};

#[derive(Debug, Error)]
pub enum ManagerError {
    #[error("sqlite: {0}")]
    Sql(#[from] rusqlite::Error),
    #[error("workflow not found: {0}")]
    NotFound(String),
    #[error("normalize: {0}")]
    Normalize(#[from] NormalizeError),
    #[error("parse: {0}")]
    Parse(#[from] ParseError),
    #[error("re-serialization to yaml failed: {0}")]
    YamlEncode(#[from] serde_yml::Error),
}

/// Slim row returned by `list_workflows`.
#[derive(Debug, Clone)]
pub struct WorkflowRow {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub space_id: Option<String>,
    pub current_version: i64,
    pub enabled: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Detailed workflow + its current-version definition.
#[derive(Debug, Clone)]
pub struct WorkflowDetail {
    pub row: WorkflowRow,
    pub def: SymphonyWorkflowDef,
    pub definition_md: String,
}

/// `SymphonyManager` is a stateless wrapper around the DB connection
/// (`AppState.db` is a `Arc<Mutex<Connection>>` everywhere in uClaw — we
/// follow the same convention).
pub struct SymphonyManager {
    db: std::sync::Arc<Mutex<Connection>>,
}

impl SymphonyManager {
    pub fn new(db: std::sync::Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// List all workflows (ordered by `updated_at DESC`).
    pub fn list_workflows(&self) -> Result<Vec<WorkflowRow>, ManagerError> {
        let conn = self.db.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, name, description, space_id, current_version, enabled, created_at, updated_at \
             FROM symphony_workflows \
             ORDER BY updated_at DESC",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok(WorkflowRow {
                id: r.get(0)?,
                name: r.get(1)?,
                description: r.get(2)?,
                space_id: r.get(3)?,
                current_version: r.get(4)?,
                enabled: r.get::<_, i64>(5)? != 0,
                created_at: r.get(6)?,
                updated_at: r.get(7)?,
            })
        })?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Load a workflow + its current-version definition.
    pub fn get_workflow(&self, id: &str) -> Result<WorkflowDetail, ManagerError> {
        let conn = self.db.lock().unwrap();
        let row: WorkflowRow = conn
            .query_row(
                "SELECT id, name, description, space_id, current_version, enabled, created_at, updated_at \
                 FROM symphony_workflows WHERE id = ?1",
                params![id],
                |r| {
                    Ok(WorkflowRow {
                        id: r.get(0)?,
                        name: r.get(1)?,
                        description: r.get(2)?,
                        space_id: r.get(3)?,
                        current_version: r.get(4)?,
                        enabled: r.get::<_, i64>(5)? != 0,
                        created_at: r.get(6)?,
                        updated_at: r.get(7)?,
                    })
                },
            )
            .optional()?
            .ok_or_else(|| ManagerError::NotFound(id.to_string()))?;

        let (nodes_json, edges_json, definition_md): (String, String, String) = conn.query_row(
            "SELECT nodes_json, edges_json, definition_md \
             FROM symphony_workflow_versions \
             WHERE workflow_id = ?1 AND version = ?2",
            params![row.id, row.current_version],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;

        let def = version_row_to_def(
            row.id.clone(),
            row.name.clone(),
            row.description.clone(),
            row.space_id.clone(),
            &nodes_json,
            &edges_json,
        )?;

        Ok(WorkflowDetail {
            row,
            def,
            definition_md,
        })
    }

    /// Persist a workflow definition. Validates the DAG first; bumps the
    /// version on every successful save. Returns `(workflow_id, version)`.
    ///
    /// `definition_md` is the full WORKFLOW.md the user authored (YAML + body);
    /// we re-serialize the validated def to YAML for `definition_yaml` so the
    /// canonical structured form lives next to the user's text.
    pub fn save_workflow(
        &self,
        def: &SymphonyWorkflowDef,
        definition_md: String,
    ) -> Result<(String, i64), ManagerError> {
        validate_dag(def)?;
        let definition_yaml = serde_yml::to_string(def)?;

        let now = Utc::now().timestamp_millis();
        let mut conn = self.db.lock().unwrap();
        let tx = conn.transaction()?;

        let existing: Option<i64> = tx
            .query_row(
                "SELECT current_version FROM symphony_workflows WHERE id = ?1",
                params![def.id],
                |r| r.get(0),
            )
            .optional()?;

        let next_version = match existing {
            Some(v) => v + 1,
            None => 1,
        };

        let row = def_to_version_row(def, next_version, definition_yaml, definition_md)?;

        if existing.is_none() {
            tx.execute(
                "INSERT INTO symphony_workflows \
                 (id, name, description, space_id, current_version, enabled, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?6)",
                params![def.id, def.name, def.description, def.space_id, next_version, now],
            )?;
        } else {
            tx.execute(
                "UPDATE symphony_workflows \
                 SET name = ?2, description = ?3, space_id = ?4, current_version = ?5, updated_at = ?6 \
                 WHERE id = ?1",
                params![def.id, def.name, def.description, def.space_id, next_version, now],
            )?;
        }

        tx.execute(
            "INSERT INTO symphony_workflow_versions \
             (workflow_id, version, definition_yaml, definition_md, nodes_json, edges_json, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                row.workflow_id,
                row.version,
                row.definition_yaml,
                row.definition_md,
                row.nodes_json,
                row.edges_json,
                now,
            ],
        )?;

        tx.commit()?;
        Ok((def.id.clone(), next_version))
    }

    /// Delete a workflow. FKs cascade to `symphony_workflow_versions`,
    /// `symphony_runs`, and (transitively) `symphony_node_runs`.
    ///
    /// FK enforcement: production code paths route through `db::manager::
    /// Database::open` which sets `PRAGMA foreign_keys = ON` at connection
    /// open (manager.rs:14). In-memory test connections that use
    /// `Connection::open_in_memory()` directly must opt in via
    /// `PRAGMA foreign_keys = ON;` before exercising delete-cascade.
    /// `agent_sessions.id` rows referenced by `symphony_node_runs.session_id`
    /// survive (FK is `ON DELETE SET NULL`) so the Agent view's open
    /// transcripts don't disappear when a workflow is deleted.
    pub fn delete_workflow(&self, id: &str) -> Result<(), ManagerError> {
        let conn = self.db.lock().unwrap();
        let n = conn.execute("DELETE FROM symphony_workflows WHERE id = ?1", params![id])?;
        if n == 0 {
            return Err(ManagerError::NotFound(id.to_string()));
        }
        Ok(())
    }

    /// Import a WORKFLOW.md string → parse → validate → save.
    pub fn import_md(&self, source: &str) -> Result<(String, i64), ManagerError> {
        let def = parse_workflow_md(source)?;
        self.save_workflow(&def, source.to_string())
    }

    /// Export a workflow back to its original WORKFLOW.md text (the
    /// `definition_md` we persisted on the last save).
    pub fn export_md(&self, id: &str) -> Result<String, ManagerError> {
        let detail = self.get_workflow(id)?;
        Ok(detail.definition_md)
    }
}

#[cfg(test)]
mod tests {
    use super::super::protocol::types::{NodeKind, RetryPolicy, SymphonyNode};
    use super::*;
    use std::sync::Arc;

    fn db() -> Arc<Mutex<Connection>> {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn sample_def(id: &str) -> SymphonyWorkflowDef {
        SymphonyWorkflowDef {
            id: id.into(),
            name: "Demo".into(),
            description: Some("desc".into()),
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: Default::default(),
            nodes: vec![
                SymphonyNode {
                    id: "a".into(),
                    label: "A".into(),
                    kind: NodeKind::Agent,
                    prompt: "do A".into(),
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
                    prompt: "do B".into(),
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
        }
    }

    #[test]
    fn save_creates_v1_then_get_returns_def() {
        let mgr = SymphonyManager::new(db());
        let (id, v) = mgr.save_workflow(&sample_def("wf-1"), "raw md".into()).unwrap();
        assert_eq!(id, "wf-1");
        assert_eq!(v, 1);

        let detail = mgr.get_workflow("wf-1").unwrap();
        assert_eq!(detail.row.current_version, 1);
        assert_eq!(detail.def.nodes.len(), 2);
        assert_eq!(detail.definition_md, "raw md");
    }

    #[test]
    fn save_twice_increments_version() {
        let mgr = SymphonyManager::new(db());
        let (_, v1) = mgr.save_workflow(&sample_def("wf-x"), "v1 md".into()).unwrap();
        let (_, v2) = mgr.save_workflow(&sample_def("wf-x"), "v2 md".into()).unwrap();
        assert_eq!(v1, 1);
        assert_eq!(v2, 2);

        // Both rows persist in the versions table.
        let conn = mgr.db.lock().unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_workflow_versions WHERE workflow_id = 'wf-x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 2);

        // Pointer advances to v2.
        let cur: i64 = conn
            .query_row(
                "SELECT current_version FROM symphony_workflows WHERE id = 'wf-x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(cur, 2);
    }

    #[test]
    fn save_rejects_cycle_before_writing_anything() {
        let mgr = SymphonyManager::new(db());
        let mut def = sample_def("wf-bad");
        // introduce a cycle b → a → b
        def.nodes[0].deps = vec!["b".into()];
        let err = mgr.save_workflow(&def, "bad".into()).unwrap_err();
        assert!(matches!(err, ManagerError::Normalize(NormalizeError::Cycle(_))));

        // Nothing persisted.
        let conn = mgr.db.lock().unwrap();
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM symphony_workflows", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn list_returns_inserted_workflows() {
        let mgr = SymphonyManager::new(db());
        mgr.save_workflow(&sample_def("wf-a"), "".into()).unwrap();
        mgr.save_workflow(&sample_def("wf-b"), "".into()).unwrap();
        let all = mgr.list_workflows().unwrap();
        assert_eq!(all.len(), 2);
        let ids: Vec<_> = all.iter().map(|r| r.id.clone()).collect();
        assert!(ids.contains(&"wf-a".to_string()));
        assert!(ids.contains(&"wf-b".to_string()));
    }

    #[test]
    fn delete_workflow_cascades() {
        let mgr = SymphonyManager::new(db());
        mgr.save_workflow(&sample_def("wf-del"), "".into()).unwrap();
        mgr.save_workflow(&sample_def("wf-del"), "".into()).unwrap(); // v2
        // Enable FK and delete.
        {
            let conn = mgr.db.lock().unwrap();
            conn.execute_batch("PRAGMA foreign_keys = ON;").unwrap();
        }
        mgr.delete_workflow("wf-del").unwrap();
        let conn = mgr.db.lock().unwrap();
        let n_versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM symphony_workflow_versions WHERE workflow_id = 'wf-del'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n_versions, 0, "versions must cascade");
    }

    #[test]
    fn import_md_then_export_md_roundtrips() {
        let mgr = SymphonyManager::new(db());
        let src = "---\nid: wf-imp\nname: Imported\nnodes:\n  - id: a\n    label: A\n---\nshared prompt";
        let (id, v) = mgr.import_md(src).unwrap();
        assert_eq!(id, "wf-imp");
        assert_eq!(v, 1);
        let back = mgr.export_md("wf-imp").unwrap();
        assert_eq!(back, src);
    }
}
