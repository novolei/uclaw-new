use std::sync::Arc;
use rusqlite::params;
use tracing::{debug, info};

use super::models::*;

// ─── Store ──────────────────────────────────────────────────────────────

/// Graph-based memory store backed by SQLite.
pub struct MemoryGraphStore {
    pub(crate) conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl MemoryGraphStore {
    pub fn new(conn: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { conn }
    }

    /// Ensure graph tables exist (V4 migration covers this; kept for safety).
    pub fn ensure_tables(&self) {
        if let Ok(conn) = self.conn.lock() {
            let _ = conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH);
            debug!("memory_graph: ensured tables exist");
        }
    }

    // ── Node CRUD ───────────────────────────────────────────────────────

    pub fn create_node(&self, node: &MemoryNode) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let metadata_str = node.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                node.id,
                node.space_id,
                node.kind.as_str(),
                node.title,
                metadata_str,
                node.created_at,
                node.updated_at,
            ],
        ).map_err(crate::error::Error::Database)?;
        debug!(id = %node.id, title = %node.title, "memory_graph: created node");
        Ok(())
    }

    pub fn get_node(&self, id: &str) -> Result<Option<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes WHERE id = ?1"
        ).map_err(crate::error::Error::Database)?;

        let result = stmt.query_row(params![id], |row| Self::row_to_node(row)).ok();
        Ok(result)
    }

    pub fn get_node_detail(&self, id: &str) -> Result<Option<MemoryNodeDetail>, crate::error::Error> {
        let node = match self.get_node(id)? {
            Some(n) => n,
            None => return Ok(None),
        };
        let active_version = self.get_active_version(id)?;
        let routes = self.get_routes_for_node(id)?;
        let keywords = self.get_keywords_for_node(id)?;
        Ok(Some(MemoryNodeDetail { node, active_version, routes, keywords }))
    }

    pub fn update_node(
        &self,
        id: &str,
        title: Option<&str>,
        kind: Option<MemoryNodeKind>,
        metadata: Option<&serde_json::Value>,
    ) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let now = chrono::Utc::now().to_rfc3339();

        let mut sets = vec!["updated_at = ?1".to_string()];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(now)];
        let mut idx = 2;

        if let Some(t) = title {
            sets.push(format!("title = ?{idx}"));
            param_values.push(Box::new(t.to_string()));
            idx += 1;
        }
        if let Some(k) = kind {
            sets.push(format!("kind = ?{idx}"));
            param_values.push(Box::new(k.as_str().to_string()));
            idx += 1;
        }
        if let Some(m) = metadata {
            sets.push(format!("metadata_json = ?{idx}"));
            param_values.push(Box::new(serde_json::to_string(m).unwrap_or_default()));
            idx += 1;
        }
        let _ = idx;

        let sql = format!("UPDATE memory_nodes SET {} WHERE id = ?{}", sets.join(", "), param_values.len() + 1);
        param_values.push(Box::new(id.to_string()));

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice()).map_err(crate::error::Error::Database)?;
        debug!(id, "memory_graph: updated node");
        Ok(())
    }

    pub fn delete_node(&self, id: &str) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        // Delete related data first
        conn.execute("DELETE FROM memory_keywords WHERE node_id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        conn.execute("DELETE FROM memory_routes WHERE node_id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        conn.execute("DELETE FROM memory_edges WHERE parent_node_id = ?1 OR child_node_id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        conn.execute("DELETE FROM memory_versions WHERE node_id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        // Delete FTS entries
        conn.execute("DELETE FROM memory_fts WHERE node_id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        conn.execute("DELETE FROM memory_nodes WHERE id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        info!(id, "memory_graph: deleted node and related data");
        Ok(())
    }

    pub fn list_nodes_by_kind(
        &self,
        space_id: &str,
        kind: MemoryNodeKind,
        limit: usize,
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes WHERE space_id = ?1 AND kind = ?2
             ORDER BY updated_at DESC LIMIT ?3"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![space_id, kind.as_str(), limit as i64], |row| {
            Self::row_to_node(row)
        }).map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }

    pub fn list_boot_nodes(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNodeDetail>, crate::error::Error> {
        let nodes = self.list_nodes_by_kind(space_id, MemoryNodeKind::Boot, limit)?;
        let mut details = Vec::with_capacity(nodes.len());
        for node in nodes {
            let active_version = self.get_active_version(&node.id)?;
            let routes = self.get_routes_for_node(&node.id)?;
            let keywords = self.get_keywords_for_node(&node.id)?;
            details.push(MemoryNodeDetail { node, active_version, routes, keywords });
        }
        Ok(details)
    }

    /// List the top-N enabled `learned` skills for boot-layer auto-mount.
    ///
    /// Filter: kind=Procedure, metadata.skill_type='learned',
    /// metadata.enabled (default true).
    /// Order: usage_count DESC NULLS LAST, then updated_at DESC.
    /// Excludes nodes already in the regular boot set (kind=Boot).
    pub fn list_top_learned_skills(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNodeDetail>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        // SQLite has limited JSON support — order by raw json_extract(...) so
        // we don't need to deserialize/sort in Rust. NULL coerces low.
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes
             WHERE space_id = ?1 AND kind = ?2
               AND COALESCE(json_extract(metadata_json, '$.skill_type'), '') = 'learned'
               AND COALESCE(json_extract(metadata_json, '$.enabled'), 1) <> 0
             ORDER BY COALESCE(json_extract(metadata_json, '$.usage_count'), 0) DESC,
                      updated_at DESC
             LIMIT ?3"
        ).map_err(crate::error::Error::Database)?;

        let nodes: Vec<MemoryNode> = stmt
            .query_map(
                params![space_id, MemoryNodeKind::Procedure.as_str(), limit as i64],
                |row| Self::row_to_node(row),
            )
            .map_err(crate::error::Error::Database)?
            .flatten()
            .collect();
        // Drop the lock before fetching versions/routes/keywords (which
        // re-lock per call). Avoids self-deadlock.
        drop(stmt);
        drop(conn);

        let mut details = Vec::with_capacity(nodes.len());
        for node in nodes {
            let active_version = self.get_active_version(&node.id)?;
            let routes = self.get_routes_for_node(&node.id)?;
            let keywords = self.get_keywords_for_node(&node.id)?;
            details.push(MemoryNodeDetail { node, active_version, routes, keywords });
        }
        Ok(details)
    }

    /// Increment usage_count on the given skill nodes by 1 each.
    ///
    /// Best-effort: failures are returned but the caller usually ignores
    /// them — usage_count is a soft signal for ranking, not correctness.
    pub fn bump_skill_usage(&self, node_ids: &[&str]) -> Result<(), crate::error::Error> {
        if node_ids.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let now = chrono::Utc::now().to_rfc3339();
        // json_set + COALESCE so first-time usage_count gets initialized
        // from 0 rather than NULL+1=NULL.
        let mut stmt = conn.prepare(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.usage_count',
                     COALESCE(json_extract(metadata_json, '$.usage_count'), 0) + 1
                 ),
                 updated_at = ?1
             WHERE id = ?2"
        ).map_err(crate::error::Error::Database)?;
        for id in node_ids {
            stmt.execute(params![now, id]).map_err(crate::error::Error::Database)?;
        }
        Ok(())
    }

    pub fn list_recent_nodes(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes WHERE space_id = ?1
             ORDER BY updated_at DESC LIMIT ?2"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![space_id, limit as i64], |row| {
            Self::row_to_node(row)
        }).map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }

    /// List all nodes in the memory graph (up to `limit`).
    pub fn list_all_nodes(&self, limit: usize) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes ORDER BY updated_at DESC LIMIT ?1"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![limit as i64], |row| {
            Self::row_to_node(row)
        }).map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }

    // ── Version CRUD ────────────────────────────────────────────────────

    pub fn create_version(&self, version: &MemoryVersion) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let metadata_str = version.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
        conn.execute(
            "INSERT INTO memory_versions (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                version.id,
                version.node_id,
                version.supersedes_version_id,
                version.status.as_str(),
                version.content,
                metadata_str,
                version.embedding_json,
                version.created_at,
            ],
        ).map_err(crate::error::Error::Database)?;

        // Update FTS index: get node title for combined search
        let title: String = conn.query_row(
            "SELECT title FROM memory_nodes WHERE id = ?1",
            params![version.node_id],
            |row| row.get(0),
        ).unwrap_or_default();

        // Upsert FTS entry
        let _ = conn.execute(
            "DELETE FROM memory_fts WHERE node_id = ?1",
            params![version.node_id],
        );
        let _ = conn.execute(
            "INSERT INTO memory_fts (node_id, title, content) VALUES (?1, ?2, ?3)",
            params![version.node_id, title, version.content],
        );

        debug!(id = %version.id, node_id = %version.node_id, "memory_graph: created version");
        Ok(())
    }

    pub fn get_active_version(&self, node_id: &str) -> Result<Option<MemoryVersion>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at
             FROM memory_versions WHERE node_id = ?1 AND status = 'active'
             ORDER BY created_at DESC LIMIT 1"
        ).map_err(crate::error::Error::Database)?;

        let result = stmt.query_row(params![node_id], |row| Self::row_to_version(row)).ok();
        Ok(result)
    }

    pub fn get_versions(&self, node_id: &str) -> Result<Vec<MemoryVersion>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at
             FROM memory_versions WHERE node_id = ?1 ORDER BY created_at DESC"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| {
            Self::row_to_version(row)
        }).map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
    }

    pub fn deprecate_version(&self, id: &str) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute(
            "UPDATE memory_versions SET status = 'deprecated' WHERE id = ?1",
            params![id],
        ).map_err(crate::error::Error::Database)?;
        debug!(id, "memory_graph: deprecated version");
        Ok(())
    }

    // ── Edge CRUD ───────────────────────────────────────────────────────

    pub fn create_edge(&self, edge: &MemoryEdge) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute(
            "INSERT INTO memory_edges (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, trigger_text, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                edge.id,
                edge.space_id,
                edge.parent_node_id,
                edge.child_node_id,
                edge.relation_kind.as_str(),
                edge.visibility.as_str(),
                edge.priority,
                edge.trigger_text,
                edge.created_at,
                edge.updated_at,
            ],
        ).map_err(crate::error::Error::Database)?;
        debug!(id = %edge.id, "memory_graph: created edge");
        Ok(())
    }

    pub fn get_edges_from(&self, node_id: &str) -> Result<Vec<MemoryEdge>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, trigger_text, created_at, updated_at
             FROM memory_edges WHERE parent_node_id = ?1"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| Self::row_to_edge(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn get_edges_to(&self, node_id: &str) -> Result<Vec<MemoryEdge>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, trigger_text, created_at, updated_at
             FROM memory_edges WHERE child_node_id = ?1"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| Self::row_to_edge(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn get_parent_nodes(&self, node_id: &str) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT n.id, n.space_id, n.kind, n.title, n.metadata_json, n.created_at, n.updated_at
             FROM memory_nodes n
             INNER JOIN memory_edges e ON e.parent_node_id = n.id
             WHERE e.child_node_id = ?1"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| Self::row_to_node(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn get_child_nodes(&self, node_id: &str, limit: usize) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT n.id, n.space_id, n.kind, n.title, n.metadata_json, n.created_at, n.updated_at
             FROM memory_nodes n
             INNER JOIN memory_edges e ON e.child_node_id = n.id
             WHERE e.parent_node_id = ?1
             ORDER BY e.priority DESC LIMIT ?2"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id, limit as i64], |row| Self::row_to_node(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn delete_edge(&self, id: &str) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute("DELETE FROM memory_edges WHERE id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        debug!(id, "memory_graph: deleted edge");
        Ok(())
    }

    /// List all edges in the memory graph.
    pub fn list_all_edges(&self) -> Result<Vec<MemoryEdge>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, trigger_text, created_at, updated_at
             FROM memory_edges ORDER BY created_at DESC"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map([], |row| Self::row_to_edge(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    // ── Route CRUD ──────────────────────────────────────────────────────

    pub fn create_route(&self, route: &MemoryRoute) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute(
            "INSERT INTO memory_routes (id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                route.id,
                route.space_id,
                route.edge_id,
                route.node_id,
                route.domain,
                route.path,
                route.is_primary as i32,
                route.created_at,
                route.updated_at,
            ],
        ).map_err(crate::error::Error::Database)?;
        debug!(id = %route.id, domain = %route.domain, path = %route.path, "memory_graph: created route");
        Ok(())
    }

    pub fn get_route_by_uri(
        &self,
        space_id: &str,
        domain: &str,
        path: &str,
    ) -> Result<Option<MemoryRoute>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at
             FROM memory_routes WHERE space_id = ?1 AND domain = ?2 AND path = ?3"
        ).map_err(crate::error::Error::Database)?;

        let result = stmt.query_row(params![space_id, domain, path], |row| Self::row_to_route(row)).ok();
        Ok(result)
    }

    pub fn get_primary_route(&self, node_id: &str) -> Result<Option<MemoryRoute>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at
             FROM memory_routes WHERE node_id = ?1 AND is_primary = 1 LIMIT 1"
        ).map_err(crate::error::Error::Database)?;

        let result = stmt.query_row(params![node_id], |row| Self::row_to_route(row)).ok();
        Ok(result)
    }

    pub fn get_routes_for_node(&self, node_id: &str) -> Result<Vec<MemoryRoute>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at
             FROM memory_routes WHERE node_id = ?1"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| Self::row_to_route(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn delete_route(&self, id: &str) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute("DELETE FROM memory_routes WHERE id = ?1", params![id])
            .map_err(crate::error::Error::Database)?;
        debug!(id, "memory_graph: deleted route");
        Ok(())
    }

    /// List all routes in the memory graph.
    pub fn list_all_routes(&self) -> Result<Vec<MemoryRoute>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at
             FROM memory_routes ORDER BY domain, path"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map([], |row| Self::row_to_route(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    // ── Keyword CRUD ────────────────────────────────────────────────────

    pub fn create_keyword(&self, keyword: &MemoryKeyword) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute(
            "INSERT INTO memory_keywords (id, space_id, node_id, keyword, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                keyword.id,
                keyword.space_id,
                keyword.node_id,
                keyword.keyword,
                keyword.created_at,
            ],
        ).map_err(crate::error::Error::Database)?;
        debug!(node_id = %keyword.node_id, keyword = %keyword.keyword, "memory_graph: created keyword");
        Ok(())
    }

    pub fn get_keywords_for_node(&self, node_id: &str) -> Result<Vec<String>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT keyword FROM memory_keywords WHERE node_id = ?1 ORDER BY keyword"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![node_id], |row| row.get::<_, String>(0))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn search_by_keyword(
        &self,
        space_id: &str,
        keyword: &str,
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT DISTINCT n.id, n.space_id, n.kind, n.title, n.metadata_json, n.created_at, n.updated_at
             FROM memory_nodes n
             INNER JOIN memory_keywords k ON k.node_id = n.id
             WHERE k.space_id = ?1 AND k.keyword LIKE ?2"
        ).map_err(crate::error::Error::Database)?;

        let pattern = format!("%{}%", keyword);
        let rows = stmt.query_map(params![space_id, pattern], |row| Self::row_to_node(row))
            .map_err(crate::error::Error::Database)?;
        Ok(rows.flatten().collect())
    }

    pub fn delete_keywords_for_node(&self, node_id: &str) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute("DELETE FROM memory_keywords WHERE node_id = ?1", params![node_id])
            .map_err(crate::error::Error::Database)?;
        debug!(node_id, "memory_graph: deleted keywords for node");
        Ok(())
    }

    // ── Boot 集管理 ─────────────────────────────────────────────────────

    pub fn add_to_boot(
        &self,
        space_id: &str,
        node_id: &str,
        priority: i32,
    ) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let now = chrono::Utc::now().to_rfc3339();
        // Update the node kind to Boot
        conn.execute(
            "UPDATE memory_nodes SET kind = 'boot', updated_at = ?1 WHERE id = ?2 AND space_id = ?3",
            params![now, node_id, space_id],
        ).map_err(crate::error::Error::Database)?;
        // Create a boot edge (root → node) with given priority
        let edge_id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT OR REPLACE INTO memory_edges (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at)
             VALUES (?1, ?2, NULL, ?3, 'contains', 'shared', ?4, ?5, ?5)",
            params![edge_id, space_id, node_id, priority, now],
        ).map_err(crate::error::Error::Database)?;
        info!(space_id, node_id, priority, "memory_graph: added to boot set");
        Ok(())
    }

    pub fn remove_from_boot(
        &self,
        space_id: &str,
        node_id: &str,
    ) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let now = chrono::Utc::now().to_rfc3339();
        // Remove boot kind → revert to reference
        conn.execute(
            "UPDATE memory_nodes SET kind = 'reference', updated_at = ?1 WHERE id = ?2 AND space_id = ?3 AND kind = 'boot'",
            params![now, node_id, space_id],
        ).map_err(crate::error::Error::Database)?;
        // Remove the boot edge
        conn.execute(
            "DELETE FROM memory_edges WHERE space_id = ?1 AND child_node_id = ?2 AND parent_node_id IS NULL AND relation_kind = 'contains'",
            params![space_id, node_id],
        ).map_err(crate::error::Error::Database)?;
        info!(space_id, node_id, "memory_graph: removed from boot set");
        Ok(())
    }

    // ── Private helpers ─────────────────────────────────────────────────

    fn row_to_node(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryNode> {
        let kind_str: String = row.get(2)?;
        let metadata_str: Option<String> = row.get(4)?;
        let metadata: Option<serde_json::Value> = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(MemoryNode {
            id: row.get(0)?,
            space_id: row.get(1)?,
            kind: MemoryNodeKind::from_str(&kind_str),
            title: row.get(3)?,
            metadata,
            created_at: row.get(5)?,
            updated_at: row.get(6)?,
        })
    }

    fn row_to_version(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryVersion> {
        let status_str: String = row.get(3)?;
        let metadata_str: Option<String> = row.get(5)?;
        let metadata: Option<serde_json::Value> = metadata_str.and_then(|s| serde_json::from_str(&s).ok());

        Ok(MemoryVersion {
            id: row.get(0)?,
            node_id: row.get(1)?,
            supersedes_version_id: row.get(2)?,
            status: MemoryVersionStatus::from_str(&status_str),
            content: row.get(4)?,
            metadata,
            embedding_json: row.get(6)?,
            created_at: row.get(7)?,
        })
    }

    fn row_to_edge(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEdge> {
        let relation_str: String = row.get(4)?;
        let visibility_str: String = row.get(5)?;

        Ok(MemoryEdge {
            id: row.get(0)?,
            space_id: row.get(1)?,
            parent_node_id: row.get(2)?,
            child_node_id: row.get(3)?,
            relation_kind: MemoryRelationKind::from_str(&relation_str),
            visibility: MemoryVisibility::from_str(&visibility_str),
            priority: row.get(6)?,
            trigger_text: row.get(7)?,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
        })
    }

    fn row_to_route(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRoute> {
        let is_primary_int: i32 = row.get(6)?;

        Ok(MemoryRoute {
            id: row.get(0)?,
            space_id: row.get(1)?,
            edge_id: row.get(2)?,
            node_id: row.get(3)?,
            domain: row.get(4)?,
            path: row.get(5)?,
            is_primary: is_primary_int != 0,
            created_at: row.get(7)?,
            updated_at: row.get(8)?,
        })
    }
}
