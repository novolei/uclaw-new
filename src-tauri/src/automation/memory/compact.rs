//! automation_memory table bookkeeping (V21 table). MemoryStore::compact
//! does the file-side work (rename memory.md → archives/{ISO8601}.md); this
//! module records the archive in the DB so the UI / future promotion logic
//! can see the compaction history.

use rusqlite::{Connection, OptionalExtension};

/// Record a compaction: append `archive_path` to compacted_archives_json and
/// refresh last_updated_at. Idempotent-insert (UPSERT) on spec_id.
pub fn record_compaction(
    conn: &Connection,
    spec_id: &str,
    archive_path: &str,
) -> rusqlite::Result<()> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let existing: Option<String> = conn
        .query_row(
            "SELECT compacted_archives_json FROM automation_memory WHERE spec_id = ?1",
            [spec_id],
            |r| r.get(0),
        )
        .optional()?;

    let mut archives: Vec<String> = existing
        .and_then(|j| {
            serde_json::from_str::<Vec<String>>(&j)
                .map_err(|e| tracing::warn!(spec_id, error = %e, "compacted_archives_json malformed — resetting"))
                .ok()
        })
        .unwrap_or_default();
    archives.push(archive_path.to_string());
    let archives_json = serde_json::to_string(&archives).unwrap_or_else(|_| "[]".into());

    // NOTE(2b): `bytes` is left at 0 — current-memory-size tracking is a
    // Phase 2b refinement; nothing reads `bytes` today.
    conn.execute(
        "INSERT INTO automation_memory (spec_id, last_updated_at, compacted_archives_json, bytes)
         VALUES (?1, ?2, ?3, 0)
         ON CONFLICT(spec_id) DO UPDATE SET
            last_updated_at = ?2,
            compacted_archives_json = ?3",
        rusqlite::params![spec_id, now_ms, archives_json],
    )?;
    tracing::debug!(spec_id, archive_path, "compaction recorded");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        // automation_memory FKs to automation_specs — insert a minimal row.
        // Column set matches execute.rs::setup_db (no `enabled` column in V20 schema).
        let now = chrono::Utc::now().timestamp_millis();
        conn.execute(
            "INSERT INTO automation_specs \
             (id, name, version, author, description, system_prompt, spec_yaml, spec_json, created_at, updated_at) \
             VALUES ('s', 'test', '1.0.0', 'tester', 'test spec', 'You are test.', '', '{}', ?1, ?1)",
            rusqlite::params![now],
        )
        .unwrap();
        conn
    }

    #[test]
    fn record_compaction_appends_archive() {
        let conn = db();
        record_compaction(&conn, "s", "archives/2026-05-14T00-00-00Z.md").unwrap();
        record_compaction(&conn, "s", "archives/2026-05-15T00-00-00Z.md").unwrap();
        let json: String = conn
            .query_row(
                "SELECT compacted_archives_json FROM automation_memory WHERE spec_id = 's'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let archives: Vec<String> = serde_json::from_str(&json).unwrap();
        assert_eq!(archives.len(), 2);
        assert!(archives[1].contains("2026-05-15"));
    }
}
