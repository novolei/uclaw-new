use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use rusqlite::params;
use tracing::{debug, info};

use super::models::*;

/// Half-life (days) for Gaussian time-decay in skill ranking.
const SKILL_DECAY_HALF_LIFE_DAYS: f64 = 30.0;

// ─── Store ──────────────────────────────────────────────────────────────

/// Graph-based memory store backed by SQLite.
pub struct MemoryGraphStore {
    pub(crate) conn: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl MemoryGraphStore {
    pub fn new(conn: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        // Enable SQLite foreign key enforcement (required for ON DELETE CASCADE)
        if let Ok(c) = conn.lock() {
            let _ = c.execute_batch("PRAGMA foreign_keys = ON;");
        }
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
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at
             FROM memory_nodes WHERE space_id = ?1 AND kind = ?2
             ORDER BY updated_at DESC LIMIT ?3"
        ).map_err(crate::error::Error::Database)?;
        let nodes: Vec<MemoryNode> = stmt
            .query_map(params![space_id, MemoryNodeKind::Boot.as_str(), limit as i64], |row| {
                Self::row_to_node(row)
            })
            .map_err(crate::error::Error::Database)?
            .flatten()
            .collect();
        drop(stmt);
        Self::batch_hydrate_details(&conn, nodes)
    }

    /// List the top-N enabled `learned` skills for boot-layer auto-mount.
    ///
    /// Filter: kind=Procedure, metadata.skill_type='learned',
    /// metadata.enabled (default true).
    ///
    /// Order: composite score using Gaussian time-decay (consistent with
    /// recall.rs) on last_cited_at, weighted by cited_count and usage_count.
    pub fn list_top_learned_skills(
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
             ORDER BY updated_at DESC
             LIMIT 500"
        ).map_err(crate::error::Error::Database)?;

        let mut nodes: Vec<MemoryNode> = stmt
            .query_map(
                params![space_id, MemoryNodeKind::Procedure.as_str()],
                |row| Self::row_to_node(row),
            )
            .map_err(crate::error::Error::Database)?
            .flatten()
            .collect();
        drop(stmt);

        // Rank in Rust using Gaussian time-decay (consistent with recall.rs)
        nodes.sort_by(|a, b| {
            let sa = Self::compute_skill_score(a);
            let sb = Self::compute_skill_score(b);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        nodes.truncate(limit);

        Self::batch_hydrate_details(&conn, nodes)
    }

    /// Manifest-only variant of [`list_top_learned_skills`] that excludes
    /// `draft` and `deprecated` lifecycle stages.
    ///
    /// Only `promoted` skills enter the manifest. Pre-PR rows missing the
    /// `lifecycle` field are treated as `'promoted'` (grandfathered).
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
             ORDER BY updated_at DESC
             LIMIT 500"
        ).map_err(crate::error::Error::Database)?;

        let mut nodes: Vec<MemoryNode> = stmt
            .query_map(
                params![space_id, MemoryNodeKind::Procedure.as_str()],
                |row| Self::row_to_node(row),
            )
            .map_err(crate::error::Error::Database)?
            .flatten()
            .collect();
        drop(stmt);

        // Rank in Rust using Gaussian time-decay (consistent with recall.rs)
        nodes.sort_by(|a, b| {
            let sa = Self::compute_skill_score(a);
            let sb = Self::compute_skill_score(b);
            sb.partial_cmp(&sa).unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });
        nodes.truncate(limit);

        Self::batch_hydrate_details(&conn, nodes)
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
                    row.get::<_, String>(0)?,         // child_node_id (NOT NULL)
                    row.get::<_, Option<String>>(1)?,  // parent_node_id (nullable)
                    row.get::<_, String>(2)?,          // relation_kind
                    row.get::<_, Option<i64>>(3)?,     // priority
                ))
            }).map_err(crate::error::Error::Database)?;

            for row in rows.flatten() {
                let (row_child_id, row_parent_id, rel_kind, priority) = row;
                // Determine the neighbor: the other end of the edge from current node
                let neighbor_id = if row_parent_id.as_deref() == Some(node_id.as_str()) {
                    row_child_id
                } else if row_child_id == node_id {
                    match row_parent_id {
                        Some(pid) => pid,
                        None => continue, // NULL parent pointing to us — skip
                    }
                } else {
                    continue; // shouldn't happen, but safe to skip
                };

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

    // ── EntityPage CRUD ─────────────────────────────────────────────────
    //
    // Higher-level helpers for the Memory OS Foundation Phase 1 EntityPage
    // abstraction. These compose the existing node + version + (optional)
    // route writes into a single atomic operation, and apply the
    // EntityPageMetadata schema (`memory_graph::entity_page`) to the JSON
    // column. None of this is a new table — EntityPage is just a node
    // with `kind = MemoryNodeKind::EntityPage` and a structured metadata
    // convention.

    /// Create a new EntityPage in one atomic transaction.
    ///
    /// Workflow:
    /// 1. Verify no existing EntityPage in `space_id` already has this slug
    ///    (case-insensitive lookup on `metadata_json.$.slug`). If one
    ///    exists, returns `Error::Internal` with a descriptive message;
    ///    callers should fall back to `find_entity_page_by_slug` + update.
    /// 2. Insert a new row into `memory_nodes` with `kind = entity_page`.
    /// 3. Insert the initial active version into `memory_versions` with
    ///    `content = compiled_truth` (so FTS5 indexes it automatically).
    /// 4. Insert a primary route at `domain="entity"`, `path=<slug>` so
    ///    `[[entity:<slug>]]` resolution (Phase 2 / Phase 15) has a stable
    ///    handle.
    /// 5. Re-hydrate to [`MemoryNodeDetail`] and return.
    ///
    /// The `slug` is normalized to lowercase before storage; callers that
    /// pass arbitrary case will see the lowercased form on read.
    pub fn create_entity_page(
        &self,
        space_id: &str,
        slug: &str,
        title: &str,
        compiled_truth: &str,
        mut metadata: super::entity_page::EntityPageMetadata,
    ) -> Result<MemoryNodeDetail, crate::error::Error> {
        let normalized_slug = slug.trim().to_lowercase();
        if normalized_slug.is_empty() {
            return Err(crate::error::Error::Internal(
                "create_entity_page: slug must not be empty".into(),
            ));
        }

        // Persist the canonical slug back into metadata so reads observe it
        // even if the caller forgot to set it. We DO NOT overwrite an
        // explicit caller-set slug if it matches the lowercased form;
        // callers that intentionally pass a mixed-case slug see a soft
        // normalization (which is the documented contract).
        metadata.slug = Some(normalized_slug.clone());
        let metadata_value = metadata.to_value();
        let metadata_str = if metadata_value.is_null() {
            None
        } else {
            Some(serde_json::to_string(&metadata_value).unwrap_or_default())
        };

        let now = chrono::Utc::now().to_rfc3339();
        let node_id = uuid::Uuid::new_v4().to_string();
        let version_id = uuid::Uuid::new_v4().to_string();
        let route_id = uuid::Uuid::new_v4().to_string();
        let route_path = normalized_slug.clone();

        // Clone everything the closure needs (the `move` keyword consumes
        // captured values; we still need `node_id` outside the closure to
        // re-hydrate the detail at the end).
        let node_id_for_closure = node_id.clone();
        let space_id_owned = space_id.to_string();
        let title_owned = title.to_string();
        let title_for_fts = title.to_string();
        let content_owned = compiled_truth.to_string();
        let content_for_fts = compiled_truth.to_string();
        let slug_for_check = normalized_slug.clone();

        // Run the whole sequence in one transaction so partial writes
        // (e.g. node created without route) never leak.
        self.with_transaction(move |conn| {
            // 1. Uniqueness check on (space_id, slug). Use json_extract so
            //    we don't depend on a new index — slug lookups are rare on
            //    the write path (creation only) and the table is small.
            let existing: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_nodes \
                     WHERE space_id = ?1 \
                       AND kind = 'entity_page' \
                       AND LOWER(COALESCE(json_extract(metadata_json, '$.slug'), '')) = ?2",
                    params![space_id_owned, slug_for_check],
                    |row| row.get::<_, i64>(0),
                )
                .map_err(crate::error::Error::Database)?;
            if existing > 0 {
                return Err(crate::error::Error::Internal(format!(
                    "EntityPage with slug '{}' already exists in space '{}'",
                    slug_for_check, space_id_owned
                )));
            }

            // 2. Insert node row.
            conn.execute(
                "INSERT INTO memory_nodes \
                 (id, space_id, kind, title, metadata_json, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                params![
                    node_id_for_closure,
                    space_id_owned,
                    MemoryNodeKind::EntityPage.as_str(),
                    title_owned,
                    metadata_str,
                    now,
                ],
            )
            .map_err(crate::error::Error::Database)?;

            // 3. Insert initial active version + FTS row (mirrors
            //    `create_version` but inline because we're inside the txn).
            conn.execute(
                "INSERT INTO memory_versions \
                 (id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at) \
                 VALUES (?1, ?2, NULL, 'active', ?3, NULL, NULL, ?4)",
                params![version_id, node_id_for_closure, content_owned, now],
            )
            .map_err(crate::error::Error::Database)?;
            // Clear any stale FTS row for this node_id, then insert the new one.
            let _ = conn.execute(
                "DELETE FROM memory_fts WHERE node_id = ?1",
                params![node_id_for_closure],
            );
            let _ = conn.execute(
                "INSERT INTO memory_fts (node_id, title, content) VALUES (?1, ?2, ?3)",
                params![node_id_for_closure, title_for_fts, content_for_fts],
            );

            // 4. Insert primary route (entity/<slug>). edge_id is NULL —
            //    routes can hang off either an edge or a node; for an
            //    EntityPage the route is node-anchored.
            conn.execute(
                "INSERT INTO memory_routes \
                 (id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at) \
                 VALUES (?1, ?2, NULL, ?3, 'entity', ?4, 1, ?5, ?5)",
                params![route_id, space_id_owned, node_id_for_closure, route_path, now],
            )
            .map_err(crate::error::Error::Database)?;

            Ok(())
        })?;

        // 5. Re-hydrate from the canonical path so the return value reflects
        //    exactly what's on disk (including default fields filled in by
        //    `from_value` round-trips elsewhere).
        self.get_node_detail(&node_id)?
            .ok_or_else(|| {
                crate::error::Error::Internal(
                    "create_entity_page: node disappeared between insert and read".into(),
                )
            })
    }

    /// Find an EntityPage by its slug (case-insensitive) within a space.
    /// Returns `None` if no match. Slug lookup is on
    /// `metadata_json.$.slug`; pages created via [`create_entity_page`]
    /// always set this field.
    pub fn find_entity_page_by_slug(
        &self,
        space_id: &str,
        slug: &str,
    ) -> Result<Option<MemoryNodeDetail>, crate::error::Error> {
        let normalized = slug.trim().to_lowercase();
        if normalized.is_empty() {
            return Ok(None);
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let mut stmt = conn
            .prepare(
                "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at \
                 FROM memory_nodes \
                 WHERE space_id = ?1 \
                   AND kind = 'entity_page' \
                   AND LOWER(COALESCE(json_extract(metadata_json, '$.slug'), '')) = ?2 \
                 LIMIT 1",
            )
            .map_err(crate::error::Error::Database)?;
        let node = stmt
            .query_row(params![space_id, normalized], |row| Self::row_to_node(row))
            .ok();
        let node = match node {
            Some(n) => n,
            None => return Ok(None),
        };
        let details = Self::batch_hydrate_details(&conn, vec![node])?;
        Ok(details.into_iter().next())
    }

    /// List EntityPage nodes within a space, optionally filtered by
    /// `subkind` (`metadata.subkind`, e.g. `"entity"`, `"concept"`).
    /// Ordered by `updated_at` DESC.
    ///
    /// When `subkind_filter` is `None`, returns every EntityPage. When
    /// `Some`, applies a `json_extract` equality predicate; pages whose
    /// metadata lacks the subkind field never match.
    pub fn list_entity_pages(
        &self,
        space_id: &str,
        subkind_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<MemoryNodeDetail>, crate::error::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // E0597 fix (same shape as symphony/run_session.rs:173):
        // The `MappedRows` returned by `query_map` borrows `&mut stmt`.
        // Chaining `.flatten().collect()` on the same line as `query_map`
        // causes Rust's borrow-checker to extend the `stmt` borrow through
        // to the end of the block, after `stmt` itself goes out of scope.
        //
        // Solution: bind the `MappedRows` to its own `let` so it's dropped
        // before `stmt` does. Then collecting from the bound iterator only
        // borrows `stmt` for as long as the binding lives — which is
        // strictly less than the block.
        let nodes: Vec<MemoryNode> = if let Some(subkind) = subkind_filter {
            let mut stmt = conn
                .prepare(
                    "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at \
                     FROM memory_nodes \
                     WHERE space_id = ?1 \
                       AND kind = 'entity_page' \
                       AND COALESCE(json_extract(metadata_json, '$.subkind'), '') = ?2 \
                     ORDER BY updated_at DESC \
                     LIMIT ?3",
                )
                .map_err(crate::error::Error::Database)?;
            let rows = stmt
                .query_map(params![space_id, subkind, limit as i64], |row| {
                    Self::row_to_node(row)
                })
                .map_err(crate::error::Error::Database)?;
            rows.flatten().collect()
        } else {
            let mut stmt = conn
                .prepare(
                    "SELECT id, space_id, kind, title, metadata_json, created_at, updated_at \
                     FROM memory_nodes \
                     WHERE space_id = ?1 AND kind = 'entity_page' \
                     ORDER BY updated_at DESC \
                     LIMIT ?2",
                )
                .map_err(crate::error::Error::Database)?;
            let rows = stmt
                .query_map(params![space_id, limit as i64], |row| Self::row_to_node(row))
                .map_err(crate::error::Error::Database)?;
            rows.flatten().collect()
        };

        Self::batch_hydrate_details(&conn, nodes)
    }

    /// Append a single timeline entry to an EntityPage's metadata.
    ///
    /// Read-modify-write on `memory_nodes.metadata_json`: decode the
    /// existing JSON (or default if missing), push the entry, encode and
    /// `UPDATE`. The node's `updated_at` is bumped so subsequent
    /// `list_entity_pages` calls float the page to the top.
    ///
    /// Errors if the node doesn't exist or isn't an EntityPage — callers
    /// should `find_entity_page_by_slug` first if they have a slug.
    pub fn append_timeline_entry(
        &self,
        node_id: &str,
        entry: super::entity_page::TimelineEntry,
    ) -> Result<(), crate::error::Error> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // Fetch current row + verify kind.
        let row: Option<(String, Option<String>)> = conn
            .query_row(
                "SELECT kind, metadata_json FROM memory_nodes WHERE id = ?1",
                params![node_id],
                |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
            )
            .ok();
        let (kind_str, metadata_raw) = match row {
            Some(r) => r,
            None => {
                return Err(crate::error::Error::Internal(format!(
                    "append_timeline_entry: node '{}' not found",
                    node_id
                )));
            }
        };
        if kind_str != MemoryNodeKind::EntityPage.as_str() {
            return Err(crate::error::Error::Internal(format!(
                "append_timeline_entry: node '{}' is kind '{}', not entity_page",
                node_id, kind_str
            )));
        }

        // Decode → push → encode.
        let value_opt: Option<serde_json::Value> = metadata_raw
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let mut meta = super::entity_page::EntityPageMetadata::from_optional(&value_opt);
        meta.push_timeline(entry);
        let new_json = serde_json::to_string(&meta.to_value())
            .map_err(crate::error::Error::Serde)?;

        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "UPDATE memory_nodes \
             SET metadata_json = ?1, updated_at = ?2 \
             WHERE id = ?3",
            params![new_json, now, node_id],
        )
        .map_err(crate::error::Error::Database)?;
        debug!(node_id, "memory_graph: appended timeline entry");
        Ok(())
    }

    // ── Transaction helper ───────────────────────────────────────────────

    /// Run a closure inside a BEGIN / COMMIT transaction.
    /// Rolls back automatically on error.
    pub fn with_transaction<F, T>(&self, f: F) -> Result<T, crate::error::Error>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<T, crate::error::Error>,
    {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        conn.execute_batch("BEGIN;").map_err(crate::error::Error::Database)?;
        match f(&conn) {
            Ok(val) => {
                conn.execute_batch("COMMIT;").map_err(crate::error::Error::Database)?;
                Ok(val)
            }
            Err(e) => {
                let _ = conn.execute_batch("ROLLBACK;");
                Err(e)
            }
        }
    }

    // ── Private helpers ─────────────────────────────────────────────────

    /// Batch-load active versions, routes, and keywords for a set of nodes.
    /// Eliminates the N+1 query pattern by using IN-clause batch queries.
    fn batch_hydrate_details(
        conn: &rusqlite::Connection,
        nodes: Vec<MemoryNode>,
    ) -> Result<Vec<MemoryNodeDetail>, crate::error::Error> {
        if nodes.is_empty() {
            return Ok(vec![]);
        }
        let node_ids: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();
        let placeholders = (1..=node_ids.len())
            .map(|i| format!("?{}", i))
            .collect::<Vec<_>>()
            .join(",");
        let sql_params: Vec<&dyn rusqlite::types::ToSql> =
            node_ids.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

        // 1. Batch active versions (latest per node)
        let ver_sql = format!(
            "SELECT id, node_id, supersedes_version_id, status, content, metadata_json, embedding_json, created_at
             FROM memory_versions
             WHERE node_id IN ({placeholders}) AND status = 'active'
             ORDER BY created_at DESC"
        );
        let mut ver_stmt = conn.prepare(&ver_sql).map_err(crate::error::Error::Database)?;
        let ver_rows = ver_stmt
            .query_map(sql_params.as_slice(), |row| Self::row_to_version(row))
            .map_err(crate::error::Error::Database)?;
        let mut version_map: HashMap<String, MemoryVersion> = HashMap::new();
        for ver in ver_rows.flatten() {
            // First inserted wins = latest due to ORDER BY created_at DESC
            version_map.entry(ver.node_id.clone()).or_insert(ver);
        }
        drop(ver_stmt);

        // 2. Batch routes
        let route_sql = format!(
            "SELECT id, space_id, edge_id, node_id, domain, path, is_primary, created_at, updated_at
             FROM memory_routes WHERE node_id IN ({placeholders})"
        );
        let mut route_stmt = conn.prepare(&route_sql).map_err(crate::error::Error::Database)?;
        let route_rows = route_stmt
            .query_map(sql_params.as_slice(), |row| Self::row_to_route(row))
            .map_err(crate::error::Error::Database)?;
        let mut route_map: HashMap<String, Vec<MemoryRoute>> = HashMap::new();
        for route in route_rows.flatten() {
            route_map.entry(route.node_id.clone()).or_default().push(route);
        }
        drop(route_stmt);

        // 3. Batch keywords
        let kw_sql = format!(
            "SELECT node_id, keyword FROM memory_keywords WHERE node_id IN ({placeholders}) ORDER BY keyword"
        );
        let mut kw_stmt = conn.prepare(&kw_sql).map_err(crate::error::Error::Database)?;
        let kw_rows = kw_stmt
            .query_map(sql_params.as_slice(), |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(crate::error::Error::Database)?;
        let mut kw_map: HashMap<String, Vec<String>> = HashMap::new();
        for (nid, kw) in kw_rows.flatten() {
            kw_map.entry(nid).or_default().push(kw);
        }
        drop(kw_stmt);

        // Assemble
        let details = nodes
            .into_iter()
            .map(|node| {
                let active_version = version_map.remove(&node.id);
                let routes = route_map.remove(&node.id).unwrap_or_default();
                let keywords = kw_map.remove(&node.id).unwrap_or_default();
                MemoryNodeDetail { node, active_version, routes, keywords }
            })
            .collect();
        Ok(details)
    }

    /// Compute composite ranking score for a learned skill using Gaussian decay.
    fn compute_skill_score(node: &MemoryNode) -> f64 {
        let meta = match &node.metadata {
            Some(m) => m,
            None => return 0.0,
        };
        let cited_count = meta.get("cited_count").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let usage_count = meta.get("usage_count").and_then(|v| v.as_f64()).unwrap_or(0.0);
        let last_cited_at = meta
            .get("last_cited_at")
            .and_then(|v| v.as_str())
            .unwrap_or("1970-01-01T00:00:00Z");
        let recency = super::recall::time_decay_score(last_cited_at, SKILL_DECAY_HALF_LIFE_DAYS) as f64;
        // Same weight formula: cited_count * recency * 10.0 + usage_count * 3.0
        cited_count * recency * 10.0 + usage_count * 3.0
    }

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

    // ─── EntityPage CRUD (Memory OS Foundation Phase 1) ───────────────────

    use super::super::entity_page::{EntityPageMetadata, TimelineEntry};

    #[test]
    fn entity_page_create_round_trip() {
        let store = fresh_test_store();
        let meta = EntityPageMetadata {
            subkind: Some("entity".into()),
            aliases: vec!["Zhang San".into(), "张三".into()],
            ..Default::default()
        };
        let detail = store
            .create_entity_page("default", "Zhang-San", "Zhang San", "## Summary\nA test entity.", meta)
            .expect("create");
        assert_eq!(detail.node.kind, MemoryNodeKind::EntityPage);
        assert_eq!(detail.node.title, "Zhang San");

        // Slug is normalized to lowercase on write.
        let stored_meta = EntityPageMetadata::from_optional(&detail.node.metadata);
        assert_eq!(stored_meta.slug.as_deref(), Some("zhang-san"));
        assert_eq!(stored_meta.subkind.as_deref(), Some("entity"));
        assert_eq!(stored_meta.aliases.len(), 2);

        // Active version content lands in memory_versions.
        let ver = detail.active_version.expect("active version");
        assert_eq!(ver.content, "## Summary\nA test entity.");
        assert!(matches!(ver.status, MemoryVersionStatus::Active));

        // Primary route lives at entity/<slug>.
        let route = detail.routes.iter().find(|r| r.is_primary).expect("primary route");
        assert_eq!(route.domain, "entity");
        assert_eq!(route.path, "zhang-san");
    }

    #[test]
    fn entity_page_rejects_duplicate_slug() {
        let store = fresh_test_store();
        store
            .create_entity_page("default", "acme", "Acme Inc.", "first", EntityPageMetadata::default())
            .expect("first create");
        let err = store
            .create_entity_page("default", "ACME", "Acme Inc Reloaded", "second", EntityPageMetadata::default())
            .expect_err("should reject case-insensitive duplicate");
        let msg = format!("{}", err);
        assert!(msg.contains("already exists"), "got: {}", msg);
    }

    #[test]
    fn entity_page_rejects_empty_slug() {
        let store = fresh_test_store();
        let err = store
            .create_entity_page("default", "   ", "x", "x", EntityPageMetadata::default())
            .expect_err("should reject empty slug");
        let msg = format!("{}", err);
        assert!(msg.contains("slug must not be empty"), "got: {}", msg);
    }

    #[test]
    fn entity_page_slug_isolated_across_spaces() {
        // Same slug in two different spaces is allowed — uniqueness is
        // scoped per `space_id`.
        let store = fresh_test_store();
        let a = store.create_entity_page("space-a", "shared", "A", "a", EntityPageMetadata::default());
        let b = store.create_entity_page("space-b", "shared", "B", "b", EntityPageMetadata::default());
        assert!(a.is_ok());
        assert!(b.is_ok());
    }

    #[test]
    fn find_entity_page_by_slug_is_case_insensitive() {
        let store = fresh_test_store();
        store
            .create_entity_page("default", "John-Smith", "John", "...", EntityPageMetadata::default())
            .unwrap();
        let by_exact = store.find_entity_page_by_slug("default", "john-smith").unwrap();
        let by_mixed = store.find_entity_page_by_slug("default", "JOHN-Smith").unwrap();
        let by_spaced = store.find_entity_page_by_slug("default", "  john-smith  ").unwrap();
        assert!(by_exact.is_some());
        assert!(by_mixed.is_some());
        assert!(by_spaced.is_some());
        assert_eq!(by_exact.unwrap().node.id, by_mixed.unwrap().node.id);

        let miss = store.find_entity_page_by_slug("default", "nobody").unwrap();
        assert!(miss.is_none());
        // Empty slug never matches.
        let empty = store.find_entity_page_by_slug("default", "").unwrap();
        assert!(empty.is_none());
    }

    #[test]
    fn list_entity_pages_filters_by_subkind() {
        let store = fresh_test_store();
        let entity_meta = EntityPageMetadata {
            subkind: Some("entity".into()),
            ..Default::default()
        };
        let concept_meta = EntityPageMetadata {
            subkind: Some("concept".into()),
            ..Default::default()
        };
        store
            .create_entity_page("default", "alice", "Alice", "x", entity_meta.clone())
            .unwrap();
        store
            .create_entity_page("default", "bob", "Bob", "x", entity_meta)
            .unwrap();
        store
            .create_entity_page("default", "rag", "RAG", "x", concept_meta)
            .unwrap();

        let entities = store.list_entity_pages("default", Some("entity"), 10).unwrap();
        assert_eq!(entities.len(), 2);
        for d in &entities {
            let m = EntityPageMetadata::from_optional(&d.node.metadata);
            assert_eq!(m.subkind.as_deref(), Some("entity"));
        }

        let concepts = store.list_entity_pages("default", Some("concept"), 10).unwrap();
        assert_eq!(concepts.len(), 1);
        assert_eq!(concepts[0].node.title, "RAG");

        let all = store.list_entity_pages("default", None, 10).unwrap();
        assert_eq!(all.len(), 3);
    }

    #[test]
    fn list_entity_pages_orders_by_updated_at_desc() {
        let store = fresh_test_store();
        store
            .create_entity_page("default", "first", "First", "x", EntityPageMetadata::default())
            .unwrap();
        // Sleep a moment so updated_at strictly differs (RFC3339 has
        // millisecond precision; a 1ms gap is enough).
        std::thread::sleep(std::time::Duration::from_millis(5));
        store
            .create_entity_page("default", "second", "Second", "x", EntityPageMetadata::default())
            .unwrap();

        let all = store.list_entity_pages("default", None, 10).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].node.title, "Second"); // newest first
        assert_eq!(all[1].node.title, "First");
    }

    #[test]
    fn append_timeline_entry_persists_and_orders() {
        let store = fresh_test_store();
        let detail = store
            .create_entity_page("default", "acme", "Acme", "x", EntityPageMetadata::default())
            .unwrap();

        store
            .append_timeline_entry(
                &detail.node.id,
                TimelineEntry {
                    date: "2026-05-01".into(),
                    text: "First mention".into(),
                    source_node_id: None,
                    source_session_id: None,
                },
            )
            .expect("first append");
        store
            .append_timeline_entry(
                &detail.node.id,
                TimelineEntry {
                    date: "2026-05-15".into(),
                    text: "Second event".into(),
                    source_node_id: Some("ep-1".into()),
                    source_session_id: Some("sess-1".into()),
                },
            )
            .expect("second append");

        // Re-fetch and decode metadata to verify both entries persisted in order.
        let again = store.get_node_detail(&detail.node.id).unwrap().expect("node");
        let m = EntityPageMetadata::from_optional(&again.node.metadata);
        assert_eq!(m.timeline.len(), 2);
        assert_eq!(m.timeline[0].date, "2026-05-01");
        assert_eq!(m.timeline[1].date, "2026-05-15");
        assert_eq!(m.timeline[1].source_session_id.as_deref(), Some("sess-1"));
    }

    #[test]
    fn append_timeline_entry_rejects_non_entity_page() {
        let store = fresh_test_store();
        // Create a Procedure node via the existing test helper.
        make_node_with(
            &store,
            "some-skill",
            serde_json::json!({"skill_type": "learned"}),
        );
        // Look it up.
        let nodes = store
            .list_nodes_by_kind("default", MemoryNodeKind::Procedure, 5)
            .unwrap();
        assert_eq!(nodes.len(), 1);
        let err = store
            .append_timeline_entry(
                &nodes[0].id,
                TimelineEntry {
                    date: "2026-05-18".into(),
                    text: "should fail".into(),
                    source_node_id: None,
                    source_session_id: None,
                },
            )
            .expect_err("expected kind mismatch error");
        let msg = format!("{}", err);
        assert!(msg.contains("not entity_page"), "got: {}", msg);
    }

    #[test]
    fn append_timeline_entry_errors_on_missing_node() {
        let store = fresh_test_store();
        let err = store
            .append_timeline_entry(
                "no-such-id",
                TimelineEntry {
                    date: "2026-05-18".into(),
                    text: "doesn't matter".into(),
                    source_node_id: None,
                    source_session_id: None,
                },
            )
            .expect_err("expected not found");
        let msg = format!("{}", err);
        assert!(msg.contains("not found"), "got: {}", msg);
    }

    #[test]
    fn entity_page_fts_is_searchable() {
        let store = fresh_test_store();
        // The V4_MEMORY_GRAPH schema creates memory_fts with unicode61 (V31
        // upgrades to trigram, but tests run V4 only). Either way the
        // INSERT path is identical; we just check the row landed.
        store
            .create_entity_page(
                "default",
                "test-entity",
                "Test Entity",
                "Searchable content goes here.",
                EntityPageMetadata::default(),
            )
            .unwrap();

        let conn = store.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_fts WHERE content LIKE '%Searchable%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "FTS row should have been inserted by create_entity_page");
    }
}
