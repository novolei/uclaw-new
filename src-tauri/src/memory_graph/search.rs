use rusqlite::params;
use serde::{Deserialize, Serialize};
use tracing::debug;

use super::models::*;
use super::store::MemoryGraphStore;

// ─── Search result type ─────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MemorySearchResult {
    pub node_id: String,
    pub title: String,
    pub content_snippet: String,
    pub kind: MemoryNodeKind,
    pub score: f32,
    pub matched_keywords: Vec<String>,
}

// ─── Search implementations ─────────────────────────────────────────────

impl MemoryGraphStore {
    /// FTS5 full-text search across node titles and version content.
    pub fn fts_search(
        &self,
        space_id: &str,
        query: &str,
        limit: usize,
    ) -> Result<Vec<MemorySearchResult>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // Use FTS5 MATCH with rank scoring
        let mut stmt = conn.prepare(
            "SELECT f.node_id, f.title, snippet(memory_fts, 2, '<b>', '</b>', '...', 32) as snippet,
                    n.kind, rank
             FROM memory_fts f
             INNER JOIN memory_nodes n ON n.id = f.node_id
             WHERE memory_fts MATCH ?1 AND n.space_id = ?2
             ORDER BY rank
             LIMIT ?3"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![query, space_id, limit as i64], |row| {
            let kind_str: String = row.get(3)?;
            let rank: f64 = row.get(4)?;
            Ok(MemorySearchResult {
                node_id: row.get(0)?,
                title: row.get(1)?,
                content_snippet: row.get(2)?,
                kind: MemoryNodeKind::from_str(&kind_str),
                score: (-rank as f32), // FTS5 rank is negative; lower = better
                matched_keywords: Vec::new(),
            })
        }).map_err(crate::error::Error::Database)?;

        let results: Vec<MemorySearchResult> = rows.flatten().collect();
        debug!(query, count = results.len(), "memory_graph: fts_search");
        Ok(results)
    }

    /// Keyword-based triggered search — matches input words against stored keywords.
    pub fn keyword_search(
        &self,
        space_id: &str,
        input_words: &[&str],
    ) -> Result<Vec<MemoryNode>, crate::error::Error> {
        if input_words.is_empty() {
            return Ok(Vec::new());
        }

        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // Build OR conditions for each input word
        let placeholders: Vec<String> = (0..input_words.len())
            .map(|i| format!("k.keyword LIKE ?{}", i + 2))
            .collect();
        let where_clause = placeholders.join(" OR ");

        let sql = format!(
            "SELECT DISTINCT n.id, n.space_id, n.kind, n.title, n.metadata_json, n.created_at, n.updated_at
             FROM memory_nodes n
             INNER JOIN memory_keywords k ON k.node_id = n.id
             WHERE k.space_id = ?1 AND ({})
             ORDER BY n.updated_at DESC",
            where_clause,
        );

        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
        param_values.push(Box::new(space_id.to_string()));
        for word in input_words {
            param_values.push(Box::new(format!("%{}%", word)));
        }
        let params_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();

        let mut stmt = conn.prepare(&sql).map_err(crate::error::Error::Database)?;
        let rows = stmt.query_map(params_refs.as_slice(), |row| {
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
        }).map_err(crate::error::Error::Database)?;

        let results: Vec<MemoryNode> = rows.flatten().collect();
        debug!(word_count = input_words.len(), matches = results.len(), "memory_graph: keyword_search");
        Ok(results)
    }

    /// Trigger text matching — finds edges whose trigger_text matches user input.
    pub fn trigger_text_search(
        &self,
        space_id: &str,
        user_input: &str,
    ) -> Result<Vec<MemoryEdge>, crate::error::Error> {
        let conn = self.conn.lock().map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;

        // Find edges where trigger_text is contained in the user input
        let mut stmt = conn.prepare(
            "SELECT id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, trigger_text, created_at, updated_at
             FROM memory_edges
             WHERE space_id = ?1
               AND relation_kind = 'trigger'
               AND trigger_text IS NOT NULL
               AND ?2 LIKE '%' || trigger_text || '%'"
        ).map_err(crate::error::Error::Database)?;

        let rows = stmt.query_map(params![space_id, user_input], |row| {
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
        }).map_err(crate::error::Error::Database)?;

        let results: Vec<MemoryEdge> = rows.flatten().collect();
        debug!(matches = results.len(), "memory_graph: trigger_text_search");
        Ok(results)
    }
}
