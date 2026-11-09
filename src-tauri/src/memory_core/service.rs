//! Background index sync: every 60 s, rebuild the corpus from the graph and
//! bucket_seal chunks and bring the index in line. The DB mutex is held
//! only to read the graph and existing hashes and to write the diff —
//! segmentation runs without it.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::{index, LastSync, MemoryCore, SyncStats};

pub const SYNC_INTERVAL: Duration = Duration::from_secs(60);

/// One sync pass. `Ok(None)`: chunks.db unreadable — skipped, because the
/// corpus must be complete or every chunk doc would be deleted.
pub fn run_sync_cycle(core: &MemoryCore, chunks_path: &Path, content_root: &Path) -> anyhow::Result<Option<SyncStats>> {
    let chunks = match rusqlite::Connection::open_with_flags(chunks_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(c) => c,
        Err(e) => {
            tracing::debug!(error = %e, "memory_core: chunks.db unreadable; sync skipped");
            return Ok(None);
        }
    };
    let chunk_docs = match index::chunk_docs(&chunks, |source_id, id| {
        let path = crate::memory_bucket_seal::paths::chunk_abs_path(content_root, "document", source_id, id);
        std::fs::read_to_string(path).ok()
    }) {
        Ok(d) => d,
        Err(e) => {
            tracing::debug!(error = %e, "memory_core: chunks unreadable; sync skipped");
            return Ok(None);
        }
    };
    let (mut docs, existing) = {
        let conn = core.db().lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        (index::graph_docs(&conn)?, index::existing_hashes(&conn)?)
    };
    docs.extend(chunk_docs);
    let plan = index::plan_sync(&existing, docs);
    let stats = {
        let conn = core.db().lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        index::apply_sync(&conn, plan)?
    };
    core.set_last_sync(LastSync { at_ms: chrono::Utc::now().timestamp_millis(), stats });
    Ok(Some(stats))
}

/// Spawn the 60 s sync loop (warms jieba first).
pub fn spawn(core: Arc<MemoryCore>, chunks_path: PathBuf, content_root: PathBuf) {
    tauri::async_runtime::spawn(async move {
        let _ = tokio::task::spawn_blocking(|| super::segment::segment("预热")).await;
        loop {
            let (c, p, r) = (Arc::clone(&core), chunks_path.clone(), content_root.clone());
            match tokio::task::spawn_blocking(move || run_sync_cycle(&c, &p, &r)).await {
                Ok(Ok(Some(stats))) if stats.upserted + stats.deleted > 0 => {
                    tracing::info!(?stats, "memory_core: index synced");
                }
                Ok(Err(e)) => tracing::warn!(error = %e, "memory_core: sync failed"),
                Err(e) => tracing::warn!(error = %e, "memory_core: sync task panicked"),
                _ => {}
            }
            tokio::time::sleep(SYNC_INTERVAL).await;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    fn chunks_db(path: &Path, rows: &[(&str, &str)]) {
        let c = rusqlite::Connection::open(path).unwrap();
        c.execute_batch("CREATE TABLE mem_tree_chunks (id TEXT PRIMARY KEY, source_id TEXT, source_ref TEXT, content TEXT);").unwrap();
        for (id, content) in rows {
            c.execute("INSERT INTO mem_tree_chunks VALUES (?1, 'pages', NULL, ?2)", [id, content]).unwrap();
        }
    }

    #[test]
    fn cycle_skips_when_chunks_unreadable_and_keeps_docs() {
        let tmp = tempfile::tempdir().unwrap();
        let chunks = tmp.path().join("chunks.db");
        chunks_db(&chunks, &[("c1", "城市是杭州")]);
        let core = MemoryCore::new(Arc::new(Mutex::new(super::super::index::tests::db())));
        let first = run_sync_cycle(&core, &chunks, tmp.path()).unwrap().expect("ran");
        assert_eq!(first.upserted, 1);
        assert_eq!(core.last_sync().unwrap().stats, first);
        assert_eq!(core.search("杭州", 6, "default").unwrap().len(), 1);

        let missing = tmp.path().join("nope.db");
        assert_eq!(run_sync_cycle(&core, &missing, tmp.path()).unwrap(), None, "skipped");
        assert_eq!(core.search("杭州", 6, "default").unwrap().len(), 1, "chunk docs kept when chunks.db is unreadable");
    }

    #[test]
    fn cycle_reads_full_chunk_body_from_content_root() {
        let tmp = tempfile::tempdir().unwrap();
        let chunks = tmp.path().join("chunks.db");
        chunks_db(&chunks, &[("c9", "short preview")]);
        let full = crate::memory_bucket_seal::paths::chunk_abs_path(tmp.path(), "document", "pages", "c9");
        std::fs::create_dir_all(full.parent().unwrap()).unwrap();
        std::fs::write(&full, "short preview 后面还有羽毛球").unwrap();
        let core = MemoryCore::new(Arc::new(Mutex::new(super::super::index::tests::db())));
        run_sync_cycle(&core, &chunks, tmp.path()).unwrap();
        assert_eq!(core.search("羽毛球", 6, "default").unwrap().len(), 1, "text beyond the preview is indexed");
    }
}
