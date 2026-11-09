//! The derived search index (V66): one doc per graph fact and per
//! bucket_seal chunk, its text pre-segmented into an FTS5 `unicode61`
//! column. `sync` brings it in line with the full corpus by content hash.

use std::collections::{HashMap, HashSet};

use rusqlite::{params, Connection};
use sha2::{Digest, Sha256};

use super::segment::segmented_text;

/// Per-doc content cap (characters).
pub const MAX_CONTENT_CHARS: usize = 2000;

#[derive(Debug, Clone, PartialEq)]
pub struct SourceDoc {
    pub doc_id: String,
    /// "graph" | "chunk"
    pub source: &'static str,
    pub source_id: String,
    pub title: String,
    pub content: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStats {
    pub upserted: usize,
    pub deleted: usize,
    pub unchanged: usize,
}

fn truncate_chars(s: String) -> String {
    if s.chars().count() <= MAX_CONTENT_CHARS {
        s
    } else {
        s.chars().take(MAX_CONTENT_CHARS).collect()
    }
}

/// One doc per not-archived graph node (all spaces) with an active version,
/// using its latest active version.
pub fn graph_docs(conn: &Connection) -> rusqlite::Result<Vec<SourceDoc>> {
    let mut stmt = conn.prepare(
        "SELECT n.id, n.title, v.content
           FROM memory_nodes n
           JOIN memory_versions v ON v.node_id = n.id AND v.status = 'active'
          WHERE n.archived_at IS NULL
          ORDER BY n.id, v.created_at DESC",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?))
    })?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for row in rows {
        let (id, title, content) = row?;
        if !seen.insert(id.clone()) {
            continue;
        }
        out.push(SourceDoc {
            doc_id: format!("graph:{id}"),
            source: "graph",
            source_id: id,
            title: title.unwrap_or_default(),
            content: truncate_chars(content.unwrap_or_default()),
        });
    }
    Ok(out)
}

/// One doc per bucket_seal chunk (pass the chunks.db connection).
/// `read_full(source_id, chunk_id)` returns the full body; the 500-char SQL
/// preview is the fallback.
pub fn chunk_docs(chunks: &Connection, read_full: impl Fn(&str, &str) -> Option<String>) -> rusqlite::Result<Vec<SourceDoc>> {
    let mut stmt = chunks.prepare("SELECT id, source_id, source_ref, content FROM mem_tree_chunks ORDER BY id")?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<String>>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
        ))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, source_id, source_ref, preview) = row?;
        let source_id = source_id.unwrap_or_default();
        let content = read_full(&source_id, &id).unwrap_or_else(|| preview.unwrap_or_default());
        out.push(SourceDoc {
            doc_id: format!("chunk:{id}"),
            source: "chunk",
            source_id: id,
            title: source_ref.unwrap_or_default(),
            content: truncate_chars(content),
        });
    }
    Ok(out)
}

fn content_hash(title: &str, content: &str) -> String {
    let mut h = Sha256::new();
    h.update(title.as_bytes());
    h.update(b"\n");
    h.update(content.as_bytes());
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// What a sync will change — computed without a DB connection, so the
/// (slow) segmentation runs outside the app-wide DB lock.
pub struct SyncPlan {
    deletes: Vec<String>,
    /// (doc, content hash, segmented text, already indexed?)
    writes: Vec<(SourceDoc, String, String, bool)>,
    unchanged: usize,
}

pub fn existing_hashes(conn: &Connection) -> rusqlite::Result<HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT doc_id, content_hash FROM memory_search_docs")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    rows.collect()
}

/// Diff `docs` (the COMPLETE corpus) against the indexed hashes.
pub fn plan_sync(existing: &HashMap<String, String>, docs: Vec<SourceDoc>) -> SyncPlan {
    let wanted: HashSet<&str> = docs.iter().map(|d| d.doc_id.as_str()).collect();
    let deletes = existing.keys().filter(|id| !wanted.contains(id.as_str())).cloned().collect();
    let mut writes = Vec::new();
    let mut unchanged = 0;
    for d in docs {
        let hash = content_hash(&d.title, &d.content);
        match existing.get(&d.doc_id) {
            Some(h) if *h == hash => unchanged += 1,
            prev => {
                let seg = segmented_text(&format!("{}\n{}", d.title, d.content));
                let existed = prev.is_some();
                writes.push((d, hash, seg, existed));
            }
        }
    }
    SyncPlan { deletes, writes, unchanged }
}

/// Write a plan in one transaction.
pub fn apply_sync(conn: &Connection, plan: SyncPlan) -> anyhow::Result<SyncStats> {
    let now = chrono::Utc::now().timestamp_millis();
    let tx = conn.unchecked_transaction()?;
    for id in &plan.deletes {
        tx.execute("DELETE FROM memory_search_docs WHERE doc_id = ?1", [id])?;
        tx.execute("DELETE FROM memory_search_fts WHERE doc_id = ?1", [id])?;
    }
    for (d, hash, seg, existed) in &plan.writes {
        tx.execute(
            "INSERT INTO memory_search_docs (doc_id, source, source_id, title, content, content_hash, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(doc_id) DO UPDATE SET
               source = excluded.source, source_id = excluded.source_id, title = excluded.title,
               content = excluded.content, content_hash = excluded.content_hash, indexed_at = excluded.indexed_at",
            params![d.doc_id, d.source, d.source_id, d.title, d.content, hash, now],
        )?;
        // doc_id is UNINDEXED in FTS, so this DELETE is a full scan: only pay
        // for it on changed docs, never on first build (else O(N^2)).
        if *existed {
            tx.execute("DELETE FROM memory_search_fts WHERE doc_id = ?1", [&d.doc_id])?;
        }
        tx.execute("INSERT INTO memory_search_fts (doc_id, segmented) VALUES (?1, ?2)", params![d.doc_id, seg])?;
    }
    tx.commit()?;
    Ok(SyncStats { upserted: plan.writes.len(), deleted: plan.deletes.len(), unchanged: plan.unchanged })
}

/// Bring the index in line with `docs`, which is the COMPLETE corpus: any
/// indexed doc not in `docs` is deleted. Changed docs are rewritten; new
/// docs inserted. One transaction.
pub fn sync(conn: &Connection, docs: &[SourceDoc]) -> anyhow::Result<SyncStats> {
    let plan = plan_sync(&existing_hashes(conn)?, docs.to_vec());
    apply_sync(conn, plan)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    pub(crate) fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn doc(id: &str, content: &str) -> SourceDoc {
        SourceDoc { doc_id: id.into(), source: "graph", source_id: id.into(), title: String::new(), content: content.into() }
    }

    fn fts_count(conn: &Connection, id: &str) -> i64 {
        conn.query_row("SELECT COUNT(*) FROM memory_search_fts WHERE doc_id = ?1", [id], |r| r.get(0)).unwrap()
    }

    #[test]
    fn incremental_sync_upserts_and_removes() {
        let c = db();
        let first = sync(&c, &[doc("a", "城市是杭州"), doc("b", "喜欢打羽毛球")]).unwrap();
        assert_eq!(first, SyncStats { upserted: 2, deleted: 0, unchanged: 0 });
        let same = sync(&c, &[doc("a", "城市是杭州"), doc("b", "喜欢打羽毛球")]).unwrap();
        assert_eq!(same, SyncStats { upserted: 0, deleted: 0, unchanged: 2 });
        let changed = sync(&c, &[doc("a", "城市是济南")]).unwrap();
        assert_eq!(changed, SyncStats { upserted: 1, deleted: 1, unchanged: 0 });
        assert_eq!(fts_count(&c, "a"), 1, "changed doc replaced, not duplicated");
        assert_eq!(fts_count(&c, "b"), 0, "vanished doc removed from FTS");
        let docs: i64 = c.query_row("SELECT COUNT(*) FROM memory_search_docs", [], |r| r.get(0)).unwrap();
        assert_eq!(docs, 1);
        let hit: i64 = c
            .query_row("SELECT COUNT(*) FROM memory_search_fts WHERE memory_search_fts MATCH '\"济南\"'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(hit, 1);
    }

    #[test]
    fn graph_docs_picks_latest_active_version_and_skips_archived() {
        let c = db();
        c.execute_batch(
            "INSERT INTO memory_nodes (id, space_id, kind, title, created_at, updated_at) VALUES
               ('n1', 'default', 'user_profile', 'home', '2026-01-01', '2026-01-01'),
               ('n2', 'ws-2', 'user_profile', 'sport', '2026-01-01', '2026-01-01'),
               ('n3', 'default', 'user_profile', 'gone', '2026-01-01', '2026-01-01'),
               ('n4', 'default', 'episode', 'no version', '2026-01-01', '2026-01-01');
             UPDATE memory_nodes SET archived_at = 5 WHERE id = 'n3';
             INSERT INTO memory_versions (id, node_id, status, content, created_at) VALUES
               ('v1', 'n1', 'active', 'old text', '2026-01-01'),
               ('v2', 'n1', 'active', 'new text', '2026-02-01'),
               ('v0', 'n1', 'superseded', 'ancient', '2025-01-01'),
               ('v3', 'n2', 'active', 'ping pong', '2026-01-01'),
               ('v4', 'n3', 'active', 'archived', '2026-01-01');",
        )
        .unwrap();
        let mut docs = graph_docs(&c).unwrap();
        docs.sort_by(|a, b| a.doc_id.cmp(&b.doc_id));
        let got: Vec<(&str, &str, &str)> =
            docs.iter().map(|d| (d.doc_id.as_str(), d.title.as_str(), d.content.as_str())).collect();
        assert_eq!(got, vec![("graph:n1", "home", "new text"), ("graph:n2", "sport", "ping pong")]);
    }

    #[test]
    fn chunk_docs_prefers_full_body_and_truncates() {
        let chunks = Connection::open_in_memory().unwrap();
        chunks
            .execute_batch(
                "CREATE TABLE mem_tree_chunks (id TEXT PRIMARY KEY, source_id TEXT, source_ref TEXT, content TEXT);
                 INSERT INTO mem_tree_chunks VALUES ('c1', 'pages', 'k1', 'preview one'), ('c2', 'skills', NULL, 'preview two');",
            )
            .unwrap();
        let long = "长".repeat(MAX_CONTENT_CHARS + 50);
        let docs = chunk_docs(&chunks, |source, id| (source == "pages" && id == "c1").then(|| long.clone())).unwrap();
        assert_eq!(docs.len(), 2);
        let c1 = docs.iter().find(|d| d.doc_id == "chunk:c1").unwrap();
        assert_eq!(c1.content.chars().count(), MAX_CONTENT_CHARS, "full body, truncated");
        assert_eq!(c1.title, "k1");
        assert_eq!(c1.source, "chunk");
        let c2 = docs.iter().find(|d| d.doc_id == "chunk:c2").unwrap();
        assert_eq!(c2.content, "preview two", "falls back to the preview");
        assert_eq!(c2.title, "");
    }
}
