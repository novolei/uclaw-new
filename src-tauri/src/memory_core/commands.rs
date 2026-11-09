//! Tauri commands for memory_core (status for 设置 → 记忆).

use serde::Serialize;
use tauri::State;

use super::{LastSync, MemoryCore};
use crate::app::AppState;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IndexStatus {
    pub docs: i64,
    pub last_sync: Option<LastSync>,
}

pub(crate) fn index_status(core: &MemoryCore) -> anyhow::Result<IndexStatus> {
    let docs: i64 = {
        let conn = core.db().lock().map_err(|e| anyhow::anyhow!("db lock: {e}"))?;
        conn.query_row("SELECT COUNT(*) FROM memory_search_docs", [], |r| r.get(0))?
    };
    Ok(IndexStatus { docs, last_sync: core.last_sync() })
}

#[tauri::command]
pub async fn get_memory_index_status(state: State<'_, AppState>) -> Result<IndexStatus, String> {
    index_status(&state.memory_core).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    #[test]
    fn status_reports_doc_count() {
        let core = MemoryCore::new(Arc::new(Mutex::new(super::super::index::tests::db())));
        assert_eq!(index_status(&core).unwrap(), IndexStatus { docs: 0, last_sync: None });
        {
            let conn = core.db().lock().unwrap();
            super::super::index::sync(
                &conn,
                &[super::super::index::SourceDoc {
                    doc_id: "graph:a".into(),
                    source: "graph",
                    source_id: "a".into(),
                    title: "t".into(),
                    content: "c".into(),
                }],
            )
            .unwrap();
        }
        let stats = super::super::SyncStats { upserted: 1, deleted: 0, unchanged: 0 };
        core.set_last_sync(LastSync { at_ms: 42, stats });
        let s = index_status(&core).unwrap();
        assert_eq!(s.docs, 1);
        assert_eq!(s.last_sync.unwrap().at_ms, 42);
        let json = serde_json::to_value(&s).unwrap();
        assert_eq!(json["lastSync"]["stats"]["upserted"], 1);
    }
}
