//! Memory system — persistent key-value store with namespace isolation and TTL.
//!
//! Provides a memory API for the Agent to store and retrieve
//! facts, preferences, and context across conversations.
//!
//! ## Namespace hierarchy
//! - `global` — shared across all Spaces
//! - `space:<space_id>` — scoped to a single Space
//! - `session:<session_id>` — scoped to a single conversation session

use std::sync::Arc;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ─── Types ──────────────────────────────────────────────────────────────

/// A single memory entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryEntry {
    pub id: String,
    pub space_id: String,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub kind: String,
    pub tags: Vec<String>,
    pub metadata: Option<serde_json::Value>,
    pub created_at: String,
    pub updated_at: String,
    pub expires_at: Option<String>,
}

/// Memory kinds
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    Fact,
    Preference,
    Context,
    Procedure,
    Note,
}

impl MemoryKind {
    pub fn as_str(&self) -> &str {
        match self {
            MemoryKind::Fact => "fact",
            MemoryKind::Preference => "preference",
            MemoryKind::Context => "context",
            MemoryKind::Procedure => "procedure",
            MemoryKind::Note => "note",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "fact" => MemoryKind::Fact,
            "preference" => MemoryKind::Preference,
            "context" => MemoryKind::Context,
            "procedure" => MemoryKind::Procedure,
            _ => MemoryKind::Note,
        }
    }
}

/// Options for creating / updating a memory entry.
pub struct SetMemoryOpts {
    pub space_id: String,
    pub namespace: String,
    pub key: String,
    pub value: serde_json::Value,
    pub kind: MemoryKind,
    pub tags: Vec<String>,
    pub metadata: Option<serde_json::Value>,
    pub ttl_seconds: Option<u64>,
}

/// Filter for listing memories.
#[derive(Debug, Default)]
pub struct ListFilter {
    pub space_id: Option<String>,
    pub namespace: Option<String>,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

/// Result for bulk import
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BulkImportResult {
    pub imported: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

// ─── Store ──────────────────────────────────────────────────────────────

/// Memory store backed by SQLite
pub struct MemoryStore {
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
}

impl MemoryStore {
    pub fn new(db: Arc<std::sync::Mutex<rusqlite::Connection>>) -> Self {
        Self { db }
    }

    /// Ensure the memories table exists (called once at startup, V3 migration
    /// already handles this; kept for backwards-compat with pre-migration DBs).
    pub fn ensure_table(&self) {
        if let Ok(conn) = self.db.lock() {
            let _ = conn.execute_batch(crate::db::migrations::V3_MEMORIES);
            debug!("memory: ensured memories table exists");
        }
    }

    // ── CRUD ────────────────────────────────────────────────────────────

    /// Create or update (upsert) a memory entry.
    pub fn set(&self, key: &str, value: serde_json::Value, kind: MemoryKind, namespace: &str, ttl_seconds: Option<u64>) -> Result<MemoryEntry, crate::error::Error> {
        self.set_full(SetMemoryOpts {
            space_id: "global".into(),
            namespace: namespace.into(),
            key: key.into(),
            value,
            kind,
            tags: Vec::new(),
            metadata: None,
            ttl_seconds,
        })
    }

    /// Full-featured upsert with all options.
    pub fn set_full(&self, opts: SetMemoryOpts) -> Result<MemoryEntry, crate::error::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        let expires_at = opts.ttl_seconds.map(|s| {
            (chrono::Utc::now() + chrono::Duration::seconds(s as i64)).to_rfc3339()
        });
        let value_str = serde_json::to_string(&opts.value)?;
        let tags_str = opts.tags.join(",");
        let metadata_str = opts.metadata.as_ref().map(|m| serde_json::to_string(m).unwrap_or_default());
        let id = uuid::Uuid::new_v4().to_string();

        let conn = self.db.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // Check if entry already exists to preserve created_at and id
        let existing: Option<(String, String)> = conn
            .prepare("SELECT id, created_at FROM memories WHERE space_id = ?1 AND namespace = ?2 AND key = ?3")
            .ok()
            .and_then(|mut stmt| {
                stmt.query_row(params![opts.space_id, opts.namespace, opts.key], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                }).ok()
            });

        let (final_id, created_at) = existing.unwrap_or_else(|| (id, now.clone()));

        conn.execute(
            "INSERT INTO memories (id, space_id, namespace, key, value, kind, tags, metadata_json, created_at, updated_at, expires_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(space_id, namespace, key) DO UPDATE SET
                 value = excluded.value,
                 kind = excluded.kind,
                 tags = excluded.tags,
                 metadata_json = excluded.metadata_json,
                 updated_at = excluded.updated_at,
                 expires_at = excluded.expires_at",
            params![
                final_id,
                opts.space_id,
                opts.namespace,
                opts.key,
                value_str,
                opts.kind.as_str(),
                tags_str,
                metadata_str,
                created_at,
                now,
                expires_at,
            ],
        ).map_err(crate::error::Error::Database)?;

        info!(key = %opts.key, namespace = %opts.namespace, space_id = %opts.space_id, "memory: set");

        Ok(MemoryEntry {
            id: final_id,
            space_id: opts.space_id,
            namespace: opts.namespace,
            key: opts.key,
            value: opts.value,
            kind: opts.kind.as_str().into(),
            tags: opts.tags,
            metadata: opts.metadata,
            created_at,
            updated_at: now,
            expires_at,
        })
    }

    /// Get a single memory entry by key + namespace (respects TTL).
    pub fn get(&self, key: &str, namespace: &str) -> Option<MemoryEntry> {
        self.get_full(key, namespace, "global")
    }

    /// Get a memory entry scoped to a specific space.
    pub fn get_full(&self, key: &str, namespace: &str, space_id: &str) -> Option<MemoryEntry> {
        let conn = self.db.lock().ok()?;
        let mut stmt = conn.prepare(
            "SELECT id, space_id, namespace, key, value, kind, tags, metadata_json, created_at, updated_at, expires_at
             FROM memories
             WHERE key = ?1 AND namespace = ?2 AND space_id = ?3
             AND (expires_at IS NULL OR expires_at > datetime('now'))"
        ).ok()?;

        stmt.query_row(params![key, namespace, space_id], |row| Self::row_to_entry(row)).ok()
    }

    /// Update only the value of an existing memory entry.
    pub fn update_value(&self, key: &str, namespace: &str, space_id: &str, value: serde_json::Value) -> Result<bool, crate::error::Error> {
        let now = chrono::Utc::now().to_rfc3339();
        let value_str = serde_json::to_string(&value)?;
        let conn = self.db.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
        let n = conn.execute(
            "UPDATE memories SET value = ?1, updated_at = ?2
             WHERE key = ?3 AND namespace = ?4 AND space_id = ?5",
            params![value_str, now, key, namespace, space_id],
        ).map_err(crate::error::Error::Database)?;
        Ok(n > 0)
    }

    /// Delete a memory entry.
    pub fn delete(&self, key: &str, namespace: &str) -> bool {
        self.delete_full(key, namespace, "global")
    }

    /// Delete scoped to a specific space.
    pub fn delete_full(&self, key: &str, namespace: &str, space_id: &str) -> bool {
        if let Ok(conn) = self.db.lock() {
            if let Ok(n) = conn.execute(
                "DELETE FROM memories WHERE key = ?1 AND namespace = ?2 AND space_id = ?3",
                params![key, namespace, space_id],
            ) {
                debug!(key, namespace, space_id, "memory: deleted entry");
                return n > 0;
            }
        }
        false
    }

    /// Delete a memory entry by its unique id.
    pub fn delete_by_id(&self, id: &str) -> bool {
        if let Ok(conn) = self.db.lock() {
            if let Ok(n) = conn.execute("DELETE FROM memories WHERE id = ?1", params![id]) {
                return n > 0;
            }
        }
        false
    }

    // ── Search & List ───────────────────────────────────────────────────

    /// Search memories by key/value content (LIKE-based).
    pub fn search(&self, query: &str, namespace: Option<&str>, limit: usize) -> Vec<MemoryEntry> {
        self.search_full(query, namespace, None, None, limit)
    }

    /// Full-featured search with optional filters.
    pub fn search_full(
        &self,
        query: &str,
        namespace: Option<&str>,
        space_id: Option<&str>,
        kind: Option<&str>,
        limit: usize,
    ) -> Vec<MemoryEntry> {
        let mut results = Vec::new();
        let q = format!("%{}%", query);

        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        // Build dynamic SQL
        let mut conditions = vec![
            "(expires_at IS NULL OR expires_at > datetime('now'))".to_string(),
        ];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if !query.is_empty() {
            conditions.push(format!("(key LIKE ?{idx} OR value LIKE ?{idx} OR tags LIKE ?{idx})"));
            param_values.push(Box::new(q));
            idx += 1;
        }
        if let Some(ns) = namespace {
            conditions.push(format!("namespace = ?{idx}"));
            param_values.push(Box::new(ns.to_string()));
            idx += 1;
        }
        if let Some(sid) = space_id {
            conditions.push(format!("space_id = ?{idx}"));
            param_values.push(Box::new(sid.to_string()));
            idx += 1;
        }
        if let Some(k) = kind {
            conditions.push(format!("kind = ?{idx}"));
            param_values.push(Box::new(k.to_string()));
            idx += 1;
        }

        conditions.push(format!("1=1")); // terminator
        let _ = idx; // suppress unused warning

        let sql = format!(
            "SELECT id, space_id, namespace, key, value, kind, tags, metadata_json, created_at, updated_at, expires_at
             FROM memories WHERE {} ORDER BY updated_at DESC LIMIT {}",
            conditions.join(" AND "),
            limit,
        );

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(params_refs.as_slice(), |row| Self::row_to_entry(row)) {
                for row in rows.flatten() {
                    results.push(row);
                }
            }
        }

        debug!(query, count = results.len(), "memory: search");
        results
    }

    /// List memories with filter options.
    pub fn list_filtered(&self, filter: &ListFilter) -> Vec<MemoryEntry> {
        let mut results = Vec::new();
        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(_) => return results,
        };

        let mut conditions = vec![
            "(expires_at IS NULL OR expires_at > datetime('now'))".to_string(),
        ];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref sid) = filter.space_id {
            conditions.push(format!("space_id = ?{idx}"));
            param_values.push(Box::new(sid.clone()));
            idx += 1;
        }
        if let Some(ref ns) = filter.namespace {
            conditions.push(format!("namespace = ?{idx}"));
            param_values.push(Box::new(ns.clone()));
            idx += 1;
        }
        if let Some(ref k) = filter.kind {
            conditions.push(format!("kind = ?{idx}"));
            param_values.push(Box::new(k.clone()));
            idx += 1;
        }
        if let Some(ref tag) = filter.tag {
            let pattern = format!("%{}%", tag);
            conditions.push(format!("tags LIKE ?{idx}"));
            param_values.push(Box::new(pattern));
            idx += 1;
        }
        let _ = idx;

        let limit = filter.limit.unwrap_or(100);
        let offset = filter.offset.unwrap_or(0);

        let sql = format!(
            "SELECT id, space_id, namespace, key, value, kind, tags, metadata_json, created_at, updated_at, expires_at
             FROM memories WHERE {} ORDER BY updated_at DESC LIMIT {} OFFSET {}",
            conditions.join(" AND "),
            limit,
            offset,
        );

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(params_refs.as_slice(), |row| Self::row_to_entry(row)) {
                for row in rows.flatten() {
                    results.push(row);
                }
            }
        }

        results
    }

    /// List all memories in a namespace (convenience wrapper).
    pub fn list(&self, namespace: &str) -> Vec<MemoryEntry> {
        self.list_filtered(&ListFilter {
            namespace: Some(namespace.into()),
            ..Default::default()
        })
    }

    /// Count memories matching a filter.
    pub fn count(&self, filter: &ListFilter) -> usize {
        let conn = match self.db.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };

        let mut conditions = vec![
            "(expires_at IS NULL OR expires_at > datetime('now'))".to_string(),
        ];
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        let mut idx = 1;

        if let Some(ref sid) = filter.space_id {
            conditions.push(format!("space_id = ?{idx}"));
            param_values.push(Box::new(sid.clone()));
            idx += 1;
        }
        if let Some(ref ns) = filter.namespace {
            conditions.push(format!("namespace = ?{idx}"));
            param_values.push(Box::new(ns.clone()));
            idx += 1;
        }
        let _ = idx;

        let sql = format!(
            "SELECT COUNT(*) FROM memories WHERE {}",
            conditions.join(" AND "),
        );

        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        conn.prepare(&sql)
            .ok()
            .and_then(|mut stmt| stmt.query_row(params_refs.as_slice(), |row| row.get::<_, i64>(0)).ok())
            .unwrap_or(0) as usize
    }

    // ── Expiration management ───────────────────────────────────────────

    /// Delete all expired memories and return the number of rows removed.
    pub fn prune_expired(&self) -> usize {
        if let Ok(conn) = self.db.lock() {
            if let Ok(n) = conn.execute(
                "DELETE FROM memories WHERE expires_at IS NOT NULL AND expires_at <= datetime('now')",
                [],
            ) {
                info!(pruned = n, "memory: pruned expired entries");
                return n;
            }
        }
        0
    }

    // ── Namespace / Bulk operations ─────────────────────────────────────

    /// Clear all memories in a namespace (optionally scoped to a space).
    pub fn clear_namespace(&self, namespace: &str, space_id: Option<&str>) -> usize {
        if let Ok(conn) = self.db.lock() {
            let n = if let Some(sid) = space_id {
                conn.execute(
                    "DELETE FROM memories WHERE namespace = ?1 AND space_id = ?2",
                    params![namespace, sid],
                ).unwrap_or(0)
            } else {
                conn.execute(
                    "DELETE FROM memories WHERE namespace = ?1",
                    params![namespace],
                ).unwrap_or(0)
            };
            info!(namespace, deleted = n, "memory: cleared namespace");
            return n;
        }
        0
    }

    /// Clear all memories for a specific space (all namespaces).
    pub fn clear_space(&self, space_id: &str) -> usize {
        if let Ok(conn) = self.db.lock() {
            let n = conn.execute(
                "DELETE FROM memories WHERE space_id = ?1",
                params![space_id],
            ).unwrap_or(0);
            info!(space_id, deleted = n, "memory: cleared space");
            return n;
        }
        0
    }

    /// Export all memories matching a filter (for backup / transfer).
    pub fn export(&self, filter: &ListFilter) -> Vec<MemoryEntry> {
        // export is basically list_filtered without limit cap
        let mut f = ListFilter {
            space_id: filter.space_id.clone(),
            namespace: filter.namespace.clone(),
            kind: filter.kind.clone(),
            tag: filter.tag.clone(),
            limit: Some(filter.limit.unwrap_or(10_000)),
            offset: filter.offset,
        };
        if f.limit.is_none() { f.limit = Some(10_000); }
        self.list_filtered(&f)
    }

    /// Bulk import memory entries; returns summary.
    pub fn bulk_import(&self, entries: Vec<SetMemoryOpts>) -> BulkImportResult {
        let mut imported = 0;
        let mut skipped = 0;
        let mut errors = Vec::new();

        for opts in entries {
            match self.set_full(opts) {
                Ok(_) => imported += 1,
                Err(e) => {
                    warn!(err = %e, "memory: bulk import entry failed");
                    errors.push(e.to_string());
                    skipped += 1;
                }
            }
        }

        info!(imported, skipped, "memory: bulk import complete");
        BulkImportResult { imported, skipped, errors }
    }

    /// List all distinct namespaces currently stored.
    pub fn list_namespaces(&self, space_id: Option<&str>) -> Vec<String> {
        let mut ns_list = Vec::new();
        if let Ok(conn) = self.db.lock() {
            let (sql, params_vec): (&str, Vec<Box<dyn rusqlite::types::ToSql>>) = if let Some(sid) = space_id {
                (
                    "SELECT DISTINCT namespace FROM memories WHERE space_id = ?1 ORDER BY namespace",
                    vec![Box::new(sid.to_string())],
                )
            } else {
                (
                    "SELECT DISTINCT namespace FROM memories ORDER BY namespace",
                    vec![],
                )
            };

            let params_refs: Vec<&dyn rusqlite::types::ToSql> = params_vec.iter().map(|p| p.as_ref()).collect();

            if let Ok(mut stmt) = conn.prepare(sql) {
                if let Ok(rows) = stmt.query_map(params_refs.as_slice(), |row| row.get::<_, String>(0)) {
                    for r in rows.flatten() {
                        ns_list.push(r);
                    }
                }
            }
        }
        ns_list
    }

    // ── Private helpers ─────────────────────────────────────────────────

    fn row_to_entry(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryEntry> {
        let value_str: String = row.get(4)?;
        let value: serde_json::Value = serde_json::from_str(&value_str).unwrap_or_default();
        let tags_str: String = row.get(6)?;
        let tags: Vec<String> = if tags_str.is_empty() {
            Vec::new()
        } else {
            tags_str.split(',').map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
        };
        let metadata_str: Option<String> = row.get(7)?;
        let metadata: Option<serde_json::Value> = metadata_str
            .and_then(|s| serde_json::from_str(&s).ok());

        Ok(MemoryEntry {
            id: row.get(0)?,
            space_id: row.get(1)?,
            namespace: row.get(2)?,
            key: row.get(3)?,
            value,
            kind: row.get(5)?,
            tags,
            metadata,
            created_at: row.get(8)?,
            updated_at: row.get(9)?,
            expires_at: row.get(10)?,
        })
    }
}

// ─── Scene-aware memU Bridge Types ──────────────────────────────────────

/// 场景化 memorize 的结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScenarioMemorizeResult {
    pub items_extracted: usize,
    pub categories_updated: Vec<String>,
    pub source_type: String,
}

/// 扩展的记忆项（包含分类信息）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnrichedMemoryItem {
    pub content: String,
    pub memory_type: String,
    pub relevance_score: f64,
    pub categories: Vec<String>,
    pub metadata: serde_json::Value,
    pub created_at: Option<String>,
}

// ─── Unit Tests ─────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memorize_result_serialization() {
        let result = ScenarioMemorizeResult {
            items_extracted: 3,
            categories_updated: vec!["profile".into(), "skill".into()],
            source_type: "conversation".into(),
        };

        let json = serde_json::to_value(&result).unwrap();
        assert_eq!(json["items_extracted"], 3);
        assert_eq!(json["categories_updated"][0], "profile");
        assert_eq!(json["source_type"], "conversation");

        // Roundtrip
        let deserialized: ScenarioMemorizeResult = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.items_extracted, 3);
        assert_eq!(deserialized.categories_updated.len(), 2);
    }

    #[test]
    fn test_enriched_memory_item_serialization() {
        let item = EnrichedMemoryItem {
            content: "User prefers dark mode".into(),
            memory_type: "preference".into(),
            relevance_score: 0.95,
            categories: vec!["ui".into(), "settings".into()],
            metadata: serde_json::json!({"source": "conversation"}),
            created_at: Some("2026-05-01T00:00:00Z".into()),
        };

        let json = serde_json::to_value(&item).unwrap();
        assert_eq!(json["content"], "User prefers dark mode");
        assert_eq!(json["relevance_score"], 0.95);
        assert_eq!(json["categories"].as_array().unwrap().len(), 2);

        // Roundtrip
        let deserialized: EnrichedMemoryItem = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.memory_type, "preference");
        assert!(deserialized.created_at.is_some());

        // Test with created_at = None
        let item_no_date = EnrichedMemoryItem {
            created_at: None,
            ..deserialized
        };
        let json2 = serde_json::to_value(&item_no_date).unwrap();
        assert!(json2["created_at"].is_null());
    }

    #[test]
    fn test_memorize_multimodal_content_format() {
        let caption = "A screenshot of the settings page";
        let text = "The settings page shows dark mode toggle and language selector.";
        let combined = format!("[Caption: {}]\n\n{}", caption, text);

        assert_eq!(
            combined,
            "[Caption: A screenshot of the settings page]\n\nThe settings page shows dark mode toggle and language selector."
        );

        // Verify the format is valid content for EnrichedMemoryItem
        let item = EnrichedMemoryItem {
            content: combined.clone(),
            memory_type: "multimodal".into(),
            relevance_score: 0.8,
            categories: vec!["visual".into()],
            metadata: serde_json::json!({"source_type": "image"}),
            created_at: None,
        };
        assert!(item.content.starts_with("[Caption:"));
        assert!(item.content.contains(text));
    }
}
