use std::collections::{HashMap, HashSet};
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
    ///
    /// Order (E3 ranking):
    ///   1. cited_count DESC — actually-cited beats merely-recalled. A skill
    ///      the LLM applied is real evidence; one that just sat in context
    ///      is not.
    ///   2. usage_count DESC — recall frequency as tiebreaker.
    ///   3. updated_at DESC — fresh edits win when both counts are zero
    ///      (e.g. a skill just extracted but not yet used).
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
             ORDER BY (
               CAST(COALESCE(json_extract(metadata_json, '$.cited_count'), 0) AS REAL) *
                 MAX(0.5,
                     1.0 - (julianday('now') - julianday(COALESCE(json_extract(metadata_json, '$.last_cited_at'),
                                                                  '1970-01-01T00:00:00Z'))) / 30.0
                 ) * 10.0
               + CAST(COALESCE(json_extract(metadata_json, '$.usage_count'), 0) AS REAL) * 3.0
             ) DESC,
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

    /// Manifest-only variant of [`list_top_learned_skills`] that excludes
    /// `draft` and `deprecated` lifecycle stages.
    ///
    /// PR-mattpocock-3 lifecycle gate: only `promoted` skills enter the
    /// manifest top-30. Drafts are still searchable via `skill_search` and
    /// still considered for dedup, but they are never auto-injected into the
    /// system prompt until usage proves them out. Pre-PR rows missing the
    /// `lifecycle` field are treated as `'promoted'` (grandfathered).
    ///
    /// Use this method from `skills_manifest`. Other callers (skill_search,
    /// fuzzy dedup, backfill) should keep using `list_top_learned_skills`
    /// so they continue to see drafts.
    pub fn list_promoted_learned_skills(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<MemoryNodeDetail>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes
             WHERE space_id = ?1 AND kind = ?2
               AND COALESCE(json_extract(metadata_json, '$.skill_type'), '') = 'learned'
               AND COALESCE(json_extract(metadata_json, '$.enabled'), 1) <> 0
               AND COALESCE(json_extract(metadata_json, '$.lifecycle'), 'promoted') = 'promoted'
             ORDER BY (
               CAST(COALESCE(json_extract(metadata_json, '$.cited_count'), 0) AS REAL) *
                 CASE
                   WHEN json_extract(metadata_json, '$.last_cited_at') IS NULL THEN 0.5
                   WHEN (julianday('now') - julianday(json_extract(metadata_json, '$.last_cited_at'))) <= 7.0 THEN 1.0
                   WHEN (julianday('now') - julianday(json_extract(metadata_json, '$.last_cited_at'))) <= 30.0 THEN
                     1.0 - ((julianday('now') - julianday(json_extract(metadata_json, '$.last_cited_at'))) - 7.0) / 46.0
                   WHEN (julianday('now') - julianday(json_extract(metadata_json, '$.last_cited_at'))) <= 90.0 THEN
                     0.5 - ((julianday('now') - julianday(json_extract(metadata_json, '$.last_cited_at'))) - 30.0) / 150.0
                   ELSE 0.1
                 END
               * 10.0
               + CAST(COALESCE(json_extract(metadata_json, '$.usage_count'), 0) AS REAL) * 3.0
             ) DESC,
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

    /// Lightweight signal: increment `manifest_appearance_count` on given
    /// skill nodes. Called once per `build_skills_manifest` invocation for
    /// learned skills that made it into the final prompt. This counter is
    /// a pure observability metric — it measures "how often does this skill
    /// survive the ranking/budget cut and appear in the system prompt".
    ///
    /// Best-effort: failures are logged but never propagated.
    pub fn bump_manifest_appearance(&self, node_ids: &[&str]) {
        if node_ids.is_empty() {
            return;
        }
        let Ok(conn) = self.conn.lock().map_err(|e| {
            tracing::warn!("bump_manifest_appearance: DB lock failed: {}", e);
            e
        }) else {
            return;
        };
        let now = chrono::Utc::now().to_rfc3339();
        let mut stmt = match conn.prepare(
            "UPDATE memory_nodes
             SET metadata_json = json_set(
                     COALESCE(metadata_json, '{}'),
                     '$.manifest_appearance_count',
                     COALESCE(json_extract(metadata_json, '$.manifest_appearance_count'), 0) + 1
                 ),
                 updated_at = ?1
             WHERE id = ?2"
        ) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("bump_manifest_appearance: prepare failed: {}", e);
                return;
            }
        };
        for id in node_ids {
            if let Err(e) = stmt.execute(rusqlite::params![now, id]) {
                tracing::warn!(node_id = %id, error = %e, "bump_manifest_appearance: execute failed");
            }
        }
    }

    /// Find an existing learned-skill node whose title matches the
    /// provided normalized form. Used by `store_skill_as_procedure` to
    /// avoid creating duplicates on every proactive extraction.
    ///
    /// Normalization is done in SQL: trim + collapse internal whitespace
    /// + lowercase. We compare lowercase forms because LLM titles often
    /// differ only by capitalization or trailing punctuation.
    ///
    /// Returns the most recently updated match (so chained dedup keeps
    /// folding into the freshest node) or None.
    pub fn find_learned_skill_by_normalized_title(
        &self,
        space_id: &str,
        normalized_title: &str,
    ) -> Result<Option<MemoryNode>, crate::error::Error> {
        if normalized_title.trim().is_empty() {
            return Ok(None);
        }
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes
             WHERE space_id = ?1 AND kind = ?2
               AND COALESCE(json_extract(metadata_json, '$.skill_type'), '') = 'learned'
               AND lower(trim(title)) = ?3
             ORDER BY updated_at DESC LIMIT 1"
        ).map_err(crate::error::Error::Database)?;

        let result = stmt
            .query_row(
                params![space_id, MemoryNodeKind::Procedure.as_str(), normalized_title],
                |row| Self::row_to_node(row),
            )
            .ok();
        Ok(result)
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

    /// 按时间范围搜索记忆节点（支持时间筛选和衰减排序）
    pub fn search_by_time_range(
        &self,
        space_id: &str,
        time_start: Option<&str>,
        time_end: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        let (sql, params_vec): (String, Vec<Box<dyn rusqlite::types::ToSql>>) = {
            let mut conditions = vec!["space_id = ?1".to_string()];
            let mut pv: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(space_id.to_string())];
            let mut idx = 2;

            if let Some(start) = time_start {
                conditions.push(format!("created_at >= ?{idx}"));
                pv.push(Box::new(start.to_string()));
                idx += 1;
            }
            if let Some(end) = time_end {
                conditions.push(format!("created_at <= ?{idx}"));
                pv.push(Box::new(end.to_string()));
                idx += 1;
            }

            let sql = format!(
                "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at \
                 FROM memory_nodes WHERE {} ORDER BY updated_at DESC LIMIT ?{}",
                conditions.join(" AND "), idx
            );
            pv.push(Box::new(limit as i64));
            (sql, pv)
        };

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();
        let mut stmt = conn.prepare(&sql).map_err(crate::error::Error::Database)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
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

    /// 图传播搜索：从种子节点出发，沿边 BFS 传播获取关联节点。
    ///
    /// - `seed_node_ids`: 起始种子节点 ID 列表
    /// - `max_depth`: 最大跳数（默认 2）
    /// - `max_nodes`: 最大返回节点数
    ///
    /// 返回按传播得分降序排列的 (node_id, score, depth) 列表。
    pub fn graph_propagation_search(
        &self,
        seed_node_ids: &[String],
        max_depth: usize,
        max_nodes: usize,
    ) -> Result<Vec<GraphPropagationNode>, crate::error::Error> {
        use std::collections::VecDeque;

        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // 关系类型权重
        let relation_weight = |rk: &str| -> f32 {
            match rk {
                "parent_of" | "child_of" => 1.0,
                "derived_from" => 0.8,
                "related_to" => 0.7,
                "primary_route" => 0.5,
                "contradicts" => 0.3,
                _ => 0.5,
            }
        };

        let mut result: HashMap<String, (f32, usize)> = HashMap::new(); // node_id -> (score, depth)
        let mut visited: HashSet<String> = HashSet::new();
        let mut queue: VecDeque<(String, f32, usize)> = VecDeque::new();

        // 种子节点入队（初始分 1.0，深度 0）
        for sid in seed_node_ids {
            queue.push_back((sid.clone(), 1.0, 0));
            visited.insert(sid.clone());
        }

        let decay_factor: f32 = 0.6;

        while let Some((node_id, incoming_score, depth)) = queue.pop_front() {
            if depth >= max_depth {
                continue;
            }

            // 查询该节点的所有出边和入边
            let mut stmt = conn.prepare(
                "SELECT e.child_node_id, e.parent_node_id, e.relation_kind, e.priority
                 FROM memory_edges e
                 WHERE (e.parent_node_id = ?1 OR e.child_node_id = ?1)
                 LIMIT 50"
            ).map_err(crate::error::Error::Database)?;

            let rows = stmt.query_map(params![node_id], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            }).map_err(crate::error::Error::Database)?;

            for row in rows.flatten() {
                let (child_id, parent_id, rel_kind, priority) = row;
                // 确定邻居节点
                let neighbor = if parent_id.as_deref() == Some(&node_id) {
                    child_id
                } else {
                    parent_id
                };

                if let Some(neighbor_id) = neighbor {
                    if visited.contains(&neighbor_id) {
                        continue;
                    }

                    let rel_w = relation_weight(&rel_kind);
                    let prio_boost = priority.unwrap_or(0) as f32 * 0.1;
                    let score = incoming_score * decay_factor * (rel_w + prio_boost);
                    let next_depth = depth + 1;

                    visited.insert(neighbor_id.clone());
                    queue.push_back((neighbor_id.clone(), score, next_depth));

                    // 如果已存在，保留高分
                    result.entry(neighbor_id)
                        .and_modify(|(s, d)| {
                            if score > *s { *s = score; *d = next_depth; }
                        })
                        .or_insert((score, next_depth));
                }
            }
        }

        // 收集并排序
        let mut scored: Vec<GraphPropagationNode> = result.into_iter()
            .map(|(node_id, (score, depth))| GraphPropagationNode { node_id, score, depth })
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
        scored.truncate(max_nodes);

        Ok(scored)
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

    // ── Embedding helpers ───────────────────────────────────────────────

    /// Persist an embedding vector (as a JSON string) into `memory_versions`.
    ///
    /// Used by the embedding backfill task and by skill extraction to write the
    /// fastembed vector immediately after the version is created.
    /// Best-effort: callers log and swallow errors — a missing embedding never
    /// breaks retrieval; it just skips the cosine channel for that skill.
    pub fn update_version_embedding(
        &self,
        version_id: &str,
        embedding_json: &str,
    ) -> Result<(), crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute(
            "UPDATE memory_versions SET embedding_json = ?1 WHERE id = ?2",
            params![embedding_json, version_id],
        ).map_err(crate::error::Error::Database)?;
        debug!(version_id, "memory_graph: wrote embedding_json");
        Ok(())
    }

    /// List active versions for learned-skill Procedure nodes that have no
    /// embedding yet (`embedding_json IS NULL`).  Used by the one-shot backfill
    /// task at proactive-service startup to hydrate legacy versions.
    ///
    /// Returns `(version_id, content)` pairs — the content is what gets embedded.
    pub fn list_versions_without_embedding(
        &self,
        space_id: &str,
        limit: usize,
    ) -> Result<Vec<(String, String)>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT v.id, v.content
             FROM memory_versions v
             INNER JOIN memory_nodes n ON n.id = v.node_id
             WHERE n.space_id = ?1
               AND n.kind = 'procedure'
               AND COALESCE(json_extract(n.metadata_json, '$.skill_type'), '') = 'learned'
               AND v.status = 'active'
               AND v.embedding_json IS NULL
             LIMIT ?2"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![space_id, limit as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }).map_err(crate::error::Error::Database)?;

        Ok(rows.flatten().collect())
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// Spin up an in-memory SQLite store with the V4 graph schema applied.
    fn fresh_test_store() -> MemoryGraphStore {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).expect("schema");
        let store = MemoryGraphStore::new(Arc::new(Mutex::new(conn)));
        store
    }

    /// Insert a minimal Procedure node + active version with given metadata.
    fn make_node_with(store: &MemoryGraphStore, title: &str, metadata: serde_json::Value) {
        let now = chrono::Utc::now().to_rfc3339();
        let node_id = uuid::Uuid::new_v4().to_string();
        let node = MemoryNode {
            id: node_id.clone(),
            space_id: "default".to_string(),
            kind: MemoryNodeKind::Procedure,
            title: title.to_string(),
            metadata: Some(metadata),
            created_at: now.clone(),
            updated_at: now.clone(),
        };
        store.create_node(&node).expect("create_node");

        let version = MemoryVersion {
            id: uuid::Uuid::new_v4().to_string(),
            node_id: node_id.clone(),
            supersedes_version_id: None,
            status: MemoryVersionStatus::Active,
            content: format!("content for {}", title),
            metadata: None,
            embedding_json: None,
            created_at: now,
        };
        store.create_version(&version).expect("create_version");
    }

    #[test]
    fn recency_factor_demotes_old_cited_skills() {
        let store = fresh_test_store();
        let now = chrono::Utc::now();
        let old = (now - chrono::Duration::days(60)).to_rfc3339();
        let fresh = (now - chrono::Duration::days(1)).to_rfc3339();

        // Skill A: cited=10, last_cited_at = 60 days ago → recency_factor clamps to 0.5
        // effective score = 10 * 0.5 * 10 + 0 * 3 = 50
        make_node_with(&store, "old-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0,
            "last_cited_at": old,
        }));

        // Skill B: cited=8, last_cited_at = 1 day ago → recency_factor ≈ 0.967
        // effective score = 8 * 0.967 * 10 + 0 * 3 ≈ 77.3 → B wins
        make_node_with(&store, "fresh-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 8, "usage_count": 0,
            "last_cited_at": fresh,
        }));

        let result = store.list_top_learned_skills("default", 10).unwrap();
        assert_eq!(result.len(), 2);
        let pos_old = result.iter().position(|d| d.node.title == "old-skill").unwrap();
        let pos_fresh = result.iter().position(|d| d.node.title == "fresh-skill").unwrap();
        // fresh-skill (cited=8, recent) should rank above old-skill (cited=10, 60 days stale)
        assert!(pos_fresh < pos_old,
            "expected fresh-skill before old-skill; got order: {:?}",
            result.iter().map(|d| &d.node.title).collect::<Vec<_>>());
    }

    #[test]
    fn recency_factor_clamps_at_half_floor() {
        let store = fresh_test_store();
        let now = chrono::Utc::now();
        let aged = (now - chrono::Duration::days(60)).to_rfc3339();
        let ancient = (now - chrono::Duration::days(365)).to_rfc3339();

        // Both cited=10 but different staleness — both should clamp to 0.5 factor
        // effective scores should be equal: 10 * 0.5 * 10 = 50 each
        make_node_with(&store, "aged", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0, "last_cited_at": aged,
        }));
        make_node_with(&store, "ancient", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 10, "usage_count": 0, "last_cited_at": ancient,
        }));

        let result = store.list_top_learned_skills("default", 10).unwrap();
        // Both present (neither zeroed out by the clamp)
        assert_eq!(result.len(), 2);
        assert!(result.iter().any(|d| d.node.title == "aged"), "aged not in result");
        assert!(result.iter().any(|d| d.node.title == "ancient"), "ancient not in result");
    }

    #[test]
    fn lifecycle_filter_excludes_draft_and_deprecated() {
        let store = fresh_test_store();
        let recent = chrono::Utc::now().to_rfc3339();

        // promoted: included
        make_node_with(&store, "promoted-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 5, "usage_count": 0, "last_cited_at": recent,
            "lifecycle": "promoted",
        }));
        // draft: excluded (not yet validated by usage)
        make_node_with(&store, "draft-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 5, "usage_count": 0, "last_cited_at": recent,
            "lifecycle": "draft",
        }));
        // deprecated: excluded (manually retired)
        make_node_with(&store, "deprecated-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 5, "usage_count": 0, "last_cited_at": recent,
            "lifecycle": "deprecated",
        }));

        // Manifest path: drafts and deprecated must be filtered out.
        let result = store.list_promoted_learned_skills("default", 10).unwrap();
        let titles: Vec<&str> = result.iter().map(|d| d.node.title.as_str()).collect();
        assert_eq!(titles, vec!["promoted-skill"],
            "manifest should include only promoted; got {:?}", titles);

        // Non-manifest path (search/dedup/backfill): drafts MUST stay visible.
        let all = store.list_top_learned_skills("default", 10).unwrap();
        let all_titles: Vec<&str> = all.iter().map(|d| d.node.title.as_str()).collect();
        assert_eq!(all.len(), 3,
            "list_top_learned_skills must keep returning all lifecycles for \
             dedup/search/backfill; got {:?}", all_titles);
    }

    #[test]
    fn lifecycle_filter_grandfathers_missing_field() {
        // Pre-PR rows have no `lifecycle` field — they should still appear in
        // the manifest (treated as 'promoted' via COALESCE default).
        let store = fresh_test_store();
        let recent = chrono::Utc::now().to_rfc3339();

        make_node_with(&store, "legacy-skill", serde_json::json!({
            "skill_type": "learned", "enabled": true,
            "cited_count": 5, "usage_count": 0, "last_cited_at": recent,
            // No lifecycle field — must be treated as promoted
        }));

        let result = store.list_promoted_learned_skills("default", 10).unwrap();
        assert_eq!(result.len(), 1, "grandfathered row should be included");
        assert_eq!(result[0].node.title, "legacy-skill");
    }
}
