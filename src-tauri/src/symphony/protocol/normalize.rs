//! `SymphonyWorkflowDef` ↔ DB-row normalization + DAG validation.
//!
//! Two responsibilities:
//!
//! 1. **Validate** — reject malformed workflows BEFORE they hit the DB:
//!    - missing node ids referenced by deps or edges,
//!    - cycles (Kahn's algorithm),
//!    - duplicate node ids,
//!    - empty workflow id / name.
//! 2. **Normalize** — convert between the canonical YAML/JSON shape and
//!    the columns of `symphony_workflow_versions` (one row per version).

use std::collections::{HashMap, HashSet, VecDeque};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::types::{SymphonyEdge, SymphonyWorkflowDef};

#[derive(Debug, Error)]
pub enum NormalizeError {
    #[error("workflow id is empty")]
    EmptyWorkflowId,
    #[error("workflow name is empty")]
    EmptyWorkflowName,
    #[error("duplicate node id: {0}")]
    DuplicateNodeId(String),
    #[error("dep references unknown node id: {0}")]
    UnknownDep(String),
    #[error("edge references unknown node id: {0}")]
    UnknownEdgeNode(String),
    #[error("cycle detected involving node(s): {0:?}")]
    Cycle(Vec<String>),
    #[error("nodes_json/edges_json must be valid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// A row staged for `INSERT INTO symphony_workflow_versions`. Numeric and
/// JSON fields are pre-serialized so the caller only does parameter binding.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NormalizedVersionRow {
    pub workflow_id: String,
    pub version: i64,
    pub definition_yaml: String,
    pub definition_md: String,
    pub nodes_json: String,
    pub edges_json: String,
}

/// Validate that the def is a non-empty DAG.
///
/// Performs four checks:
/// - id + name are non-empty,
/// - node ids are unique,
/// - every dep / edge endpoint references an existing node id,
/// - no cycle (Kahn's topological sort succeeds).
pub fn validate_dag(def: &SymphonyWorkflowDef) -> Result<(), NormalizeError> {
    if def.id.trim().is_empty() {
        return Err(NormalizeError::EmptyWorkflowId);
    }
    if def.name.trim().is_empty() {
        return Err(NormalizeError::EmptyWorkflowName);
    }

    // Unique node ids.
    let mut seen: HashSet<&str> = HashSet::new();
    for n in &def.nodes {
        if !seen.insert(n.id.as_str()) {
            return Err(NormalizeError::DuplicateNodeId(n.id.clone()));
        }
    }

    // Build adjacency from BOTH explicit edges and deps so both surfaces
    // are validated.
    let edges = def.effective_edges();
    let node_ids: HashSet<&str> = def.nodes.iter().map(|n| n.id.as_str()).collect();

    // Dep references must resolve to known nodes.
    for n in &def.nodes {
        for d in &n.deps {
            if !node_ids.contains(d.as_str()) {
                return Err(NormalizeError::UnknownDep(d.clone()));
            }
        }
    }
    for e in &edges {
        if !node_ids.contains(e.from.as_str()) {
            return Err(NormalizeError::UnknownEdgeNode(e.from.clone()));
        }
        if !node_ids.contains(e.to.as_str()) {
            return Err(NormalizeError::UnknownEdgeNode(e.to.clone()));
        }
    }

    // Kahn's topo sort. Start with in-degree counters.
    let mut indeg: HashMap<&str, usize> =
        def.nodes.iter().map(|n| (n.id.as_str(), 0usize)).collect();
    let mut adj: HashMap<&str, Vec<&str>> =
        def.nodes.iter().map(|n| (n.id.as_str(), Vec::new())).collect();

    for e in &edges {
        adj.entry(e.from.as_str()).or_default().push(e.to.as_str());
        *indeg.entry(e.to.as_str()).or_insert(0) += 1;
    }

    let mut queue: VecDeque<&str> =
        indeg.iter().filter(|(_, &d)| d == 0).map(|(k, _)| *k).collect();
    let mut popped = 0usize;
    while let Some(n) = queue.pop_front() {
        popped += 1;
        if let Some(succs) = adj.get(n) {
            for &s in succs {
                if let Some(d) = indeg.get_mut(s) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push_back(s);
                    }
                }
            }
        }
    }
    if popped != def.nodes.len() {
        let stuck: Vec<String> = indeg
            .iter()
            .filter(|(_, &d)| d > 0)
            .map(|(k, _)| (*k).to_string())
            .collect();
        return Err(NormalizeError::Cycle(stuck));
    }

    Ok(())
}

/// Stage a validated def into a row ready for the DB.
///
/// `version` is supplied by the caller (the manager increments it).
/// `definition_yaml` is what the caller (manager) re-serialized from the
/// def itself, `definition_md` is the full WORKFLOW.md (yaml + body) the
/// user authored. Both are stored so re-edits round-trip cleanly.
pub fn def_to_version_row(
    def: &SymphonyWorkflowDef,
    version: i64,
    definition_yaml: String,
    definition_md: String,
) -> Result<NormalizedVersionRow, NormalizeError> {
    validate_dag(def)?;
    Ok(NormalizedVersionRow {
        workflow_id: def.id.clone(),
        version,
        definition_yaml,
        definition_md,
        nodes_json: serde_json::to_string(&def.nodes)?,
        edges_json: serde_json::to_string(&def.effective_edges())?,
    })
}

/// Restore a def from a row. Inverse of `def_to_version_row`.
///
/// Round-trips `nodes` exactly; reconstructs the `edges` field from
/// `edges_json` rather than the deps-derived synthesis.
pub fn version_row_to_def(
    workflow_id: String,
    workflow_name: String,
    description: Option<String>,
    space_id: Option<String>,
    nodes_json: &str,
    edges_json: &str,
) -> Result<SymphonyWorkflowDef, NormalizeError> {
    let nodes = serde_json::from_str(nodes_json)?;
    let edges: Vec<SymphonyEdge> = serde_json::from_str(edges_json)?;
    Ok(SymphonyWorkflowDef {
        id: workflow_id,
        name: workflow_name,
        description,
        space_id,
        default_model: None,
        per_run_cost_cap_usd: None,
        max_concurrent_nodes: None,
        failure_mode: Default::default(),
        nodes,
        edges,
    })
}

#[cfg(test)]
mod tests {
    use super::super::types::{NodeKind, RetryPolicy, SymphonyNode};
    use super::*;

    fn node(id: &str, deps: &[&str]) -> SymphonyNode {
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

    fn def_with(nodes: Vec<SymphonyNode>) -> SymphonyWorkflowDef {
        SymphonyWorkflowDef {
            id: "wf".into(),
            name: "Demo".into(),
            description: None,
            space_id: None,
            default_model: None,
            per_run_cost_cap_usd: None,
            max_concurrent_nodes: None,
            failure_mode: Default::default(),
            nodes,
            edges: vec![],
        }
    }

    #[test]
    fn validates_linear_chain() {
        let def = def_with(vec![node("a", &[]), node("b", &["a"]), node("c", &["b"])]);
        validate_dag(&def).unwrap();
    }

    #[test]
    fn validates_diamond() {
        let def = def_with(vec![
            node("a", &[]),
            node("b", &["a"]),
            node("c", &["a"]),
            node("d", &["b", "c"]),
        ]);
        validate_dag(&def).unwrap();
    }

    #[test]
    fn rejects_cycle_in_deps() {
        let def = def_with(vec![node("a", &["b"]), node("b", &["a"])]);
        let err = validate_dag(&def).unwrap_err();
        match err {
            NormalizeError::Cycle(stuck) => {
                assert!(stuck.contains(&"a".to_string()));
                assert!(stuck.contains(&"b".to_string()));
            }
            other => panic!("expected Cycle, got {:?}", other),
        }
    }

    #[test]
    fn rejects_self_loop() {
        let def = def_with(vec![node("a", &["a"])]);
        assert!(matches!(
            validate_dag(&def).unwrap_err(),
            NormalizeError::Cycle(_)
        ));
    }

    #[test]
    fn rejects_unknown_dep() {
        let def = def_with(vec![node("a", &["ghost"])]);
        assert!(matches!(
            validate_dag(&def).unwrap_err(),
            NormalizeError::UnknownDep(_)
        ));
    }

    #[test]
    fn rejects_duplicate_id() {
        let def = def_with(vec![node("a", &[]), node("a", &[])]);
        assert!(matches!(
            validate_dag(&def).unwrap_err(),
            NormalizeError::DuplicateNodeId(_)
        ));
    }

    #[test]
    fn rejects_empty_id() {
        let mut def = def_with(vec![node("a", &[])]);
        def.id = "".into();
        assert!(matches!(
            validate_dag(&def).unwrap_err(),
            NormalizeError::EmptyWorkflowId
        ));
    }

    #[test]
    fn def_row_roundtrips() {
        let def = def_with(vec![node("a", &[]), node("b", &["a"])]);
        let row = def_to_version_row(&def, 1, "yaml here".into(), "md here".into()).unwrap();
        let back = version_row_to_def(
            row.workflow_id.clone(),
            "Demo".into(),
            None,
            None,
            &row.nodes_json,
            &row.edges_json,
        )
        .unwrap();
        assert_eq!(back.nodes.len(), 2);
        assert_eq!(back.edges.len(), 1);
        assert_eq!(back.edges[0].from, "a");
        assert_eq!(back.edges[0].to, "b");
    }
}
