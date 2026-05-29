//! Persistence for score artefacts — per-chunk score rationale.
//!
//! Slim port of `openhuman::memory::tree::score::store`. Keeps only:
//! - `ScoreRow` struct
//! - `upsert_score` / `get_score` / `count_scores`
//!
//! Entity-index methods (`index_entity`, `lookup_entity`, `EntityHit`, etc.)
//! are deferred to PR8+. Schema is declared in
//! `memory_bucket_seal::store::SCHEMA`; this file only owns the CRUD ops.
//!
//! All functions operate on a `&BucketSealStore` (shared Arc<Mutex<Connection>>)
//! rather than a `Config` — the connection is already open from `BucketSealStore::open`.

use anyhow::{Context, Result};
use rusqlite::{params, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::memory_bucket_seal::score::signals::ScoreSignals;
use crate::memory_bucket_seal::store::BucketSealStore;

/// Serialized per-chunk score rationale. Mirrors the `mem_tree_score` row.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreRow {
    pub chunk_id: String,
    pub total: f32,
    pub signals: ScoreSignals,
    pub dropped: bool,
    pub reason: Option<String>,
    pub computed_at_ms: i64,
}

const SCORE_UPSERT_SQL: &str = "INSERT OR REPLACE INTO mem_tree_score (
    chunk_id, total,
    token_count_signal, unique_words_signal,
    metadata_weight, source_weight, interaction_weight, entity_density,
    llm_importance,
    dropped, reason, computed_at_ms
 ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)";

/// Upsert one score rationale row, replacing any existing entry for `chunk_id`.
pub fn upsert_score(store: &BucketSealStore, row: &ScoreRow) -> Result<()> {
    let conn = store.lock_conn()?;
    conn.execute(
        SCORE_UPSERT_SQL,
        params![
            row.chunk_id,
            row.total,
            row.signals.token_count,
            row.signals.unique_words,
            row.signals.metadata_weight,
            row.signals.source_weight,
            row.signals.interaction,
            row.signals.entity_density,
            row.signals.llm_importance,
            i32::from(row.dropped),
            row.reason,
            row.computed_at_ms,
        ],
    )
    .context("upsert_score")?;
    Ok(())
}

/// Fetch one chunk's score rationale. Returns `None` if no row exists.
pub fn get_score(store: &BucketSealStore, chunk_id: &str) -> Result<Option<ScoreRow>> {
    let conn = store.lock_conn()?;
    conn.query_row(
        "SELECT chunk_id, total,
                token_count_signal, unique_words_signal,
                metadata_weight, source_weight, interaction_weight, entity_density,
                llm_importance,
                dropped, reason, computed_at_ms
         FROM mem_tree_score WHERE chunk_id = ?1",
        params![chunk_id],
        |row| {
            Ok(ScoreRow {
                chunk_id: row.get(0)?,
                total: row.get(1)?,
                signals: ScoreSignals {
                    token_count: row.get(2)?,
                    unique_words: row.get(3)?,
                    metadata_weight: row.get(4)?,
                    source_weight: row.get(5)?,
                    interaction: row.get(6)?,
                    entity_density: row.get(7)?,
                    llm_importance: row.get::<_, Option<f32>>(8)?.unwrap_or(0.0),
                },
                dropped: row.get::<_, i32>(9)? != 0,
                reason: row.get(10)?,
                computed_at_ms: row.get(11)?,
            })
        },
    )
    .optional()
    .context("get_score")
}

/// Count score rows (for tests / diagnostics).
pub fn count_scores(store: &BucketSealStore) -> Result<u64> {
    let conn = store.lock_conn()?;
    let n: i64 = conn
        .query_row("SELECT COUNT(*) FROM mem_tree_score", [], |r| r.get(0))
        .context("count_scores")?;
    Ok(n.max(0) as u64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_bucket_seal::score::signals::ScoreSignals;
    use crate::memory_bucket_seal::types::{Chunk, Metadata, SourceKind};
    use crate::memory_bucket_seal::{stage_chunks, BucketSealStore};
    use chrono::{TimeZone, Utc};
    use tempfile::TempDir;

    fn fresh_store() -> (BucketSealStore, TempDir) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("chunks.db");
        let store = BucketSealStore::open(&db_path).unwrap();
        store.ensure_schema().unwrap();
        (store, dir)
    }

    fn sample_chunk(id: &str) -> Chunk {
        let ts = Utc.timestamp_millis_opt(1_700_000_000_000).unwrap();
        Chunk {
            id: id.to_string(),
            content: "A substantive message about the migration plan for Phoenix launch.".to_string(),
            metadata: Metadata {
                source_kind: SourceKind::Chat,
                source_id: "slack:#eng".into(),
                owner: "alice".into(),
                timestamp: ts,
                time_range: (ts, ts),
                tags: vec![],
                source_ref: None,
            },
            token_count: 15,
            seq_in_source: 0,
            created_at: ts,
            partial_message: false,
        }
    }

    fn seed_chunk(store: &BucketSealStore, dir: &TempDir, id: &str) {
        let chunk = sample_chunk(id);
        let staged = stage_chunks(dir.path(), &[chunk]).unwrap();
        store.upsert_staged_chunks(&staged).unwrap();
    }

    fn sample_row(chunk_id: &str) -> ScoreRow {
        ScoreRow {
            chunk_id: chunk_id.to_string(),
            total: 0.72,
            signals: ScoreSignals {
                token_count: 0.8,
                unique_words: 0.9,
                metadata_weight: 0.5,
                source_weight: 0.5,
                interaction: 0.5,
                entity_density: 0.0,
                llm_importance: 0.0,
            },
            dropped: false,
            reason: None,
            computed_at_ms: 1_700_000_001_000,
        }
    }

    #[test]
    fn upsert_then_get_round_trip() {
        let (store, dir) = fresh_store();
        seed_chunk(&store, &dir, "chunk_01");

        let row = sample_row("chunk_01");
        upsert_score(&store, &row).unwrap();

        let got = get_score(&store, "chunk_01").unwrap().expect("should find row");
        assert_eq!(got.chunk_id, "chunk_01");
        assert!((got.total - 0.72).abs() < 1e-6);
        assert!((got.signals.token_count - 0.8).abs() < 1e-6);
        assert!(!got.dropped);
        assert!(got.reason.is_none());
    }

    #[test]
    fn get_missing_returns_none() {
        let (store, _dir) = fresh_store();
        assert!(get_score(&store, "nonexistent").unwrap().is_none());
    }

    #[test]
    fn count_scores_reflects_writes() {
        let (store, dir) = fresh_store();
        assert_eq!(count_scores(&store).unwrap(), 0);

        seed_chunk(&store, &dir, "chunk_01");
        upsert_score(&store, &sample_row("chunk_01")).unwrap();
        assert_eq!(count_scores(&store).unwrap(), 1);

        seed_chunk(&store, &dir, "chunk_02");
        upsert_score(&store, &sample_row("chunk_02")).unwrap();
        assert_eq!(count_scores(&store).unwrap(), 2);
    }

    #[test]
    fn dropped_flag_round_trips() {
        let (store, dir) = fresh_store();
        seed_chunk(&store, &dir, "chunk_01");

        let mut row = sample_row("chunk_01");
        row.dropped = true;
        row.reason = Some("cheap-signals total 0.120 below drop_threshold 0.300".to_string());
        upsert_score(&store, &row).unwrap();

        let got = get_score(&store, "chunk_01").unwrap().unwrap();
        assert!(got.dropped);
        assert!(got.reason.as_deref().unwrap().contains("0.120"));
    }

    #[test]
    fn upsert_is_idempotent_on_chunk_id() {
        let (store, dir) = fresh_store();
        seed_chunk(&store, &dir, "chunk_01");

        let row = sample_row("chunk_01");
        upsert_score(&store, &row).unwrap();
        // Second upsert with different total — should replace
        let mut row2 = sample_row("chunk_01");
        row2.total = 0.55;
        upsert_score(&store, &row2).unwrap();

        assert_eq!(count_scores(&store).unwrap(), 1);
        let got = get_score(&store, "chunk_01").unwrap().unwrap();
        assert!((got.total - 0.55).abs() < 1e-6);
    }

    #[test]
    fn fk_enforced_without_seeded_chunk() {
        let (store, _dir) = fresh_store();
        // No chunk seeded — FK should fire
        let row = sample_row("ghost_chunk");
        let result = upsert_score(&store, &row);
        assert!(result.is_err(), "FK violation should propagate as error");
    }
}
