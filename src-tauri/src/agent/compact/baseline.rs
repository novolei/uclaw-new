//! Bundle 17-B — per-session `StructuredFold` baseline storage.
//!
//! Persists the latest `StructuredFold` produced by `/compact` so the
//! next `/compact` invocation can diff against it and (when drift is
//! small) render a compact `<context_changes_since_last_fold>` block
//! on top of the *byte-stable* prior fold instead of replacing the
//! placeholder with a fresh full re-render.
//!
//! Storage: V52 `agent_fold_baselines` table (see
//! [`crate::db::migrations::V52_AGENT_FOLD_BASELINES`]). One row per
//! `agent_sessions.id`, upserted on every successful `/compact`
//! summarization.
//!
//! ## Why a dedicated table (vs. extending `compaction_markers`)
//!
//! `compaction_markers` (V29) is row-per-event — one row each time a
//! `/compact` fires. The baseline we need is row-per-session — the
//! *current* fold for diff purposes. Coupling those cardinalities would
//! require an `is_current` flag or a `MAX(created_at)` join on every
//! read. The dedicated table keeps the read path a single PK lookup.
//!
//! ## Soft-fail policy
//!
//! Both `load_baseline` and `upsert_baseline` swallow errors at the
//! caller boundary — `/compact` must never fail because the baseline
//! cache is broken. `load_baseline` returns `None` on any error
//! (missing row, malformed JSON, DB lock contention) so the caller
//! falls back to the full-rewrite path; `upsert_baseline` returns a
//! `Result` so the caller can log-and-continue.
//!
//! See spec
//! [`docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md`](../../../../docs/superpowers/specs/2026-05-22-bundle-17bc-wireup-design.md) §9.2 / §9.4.

use rusqlite::{params, Connection, OptionalExtension};

use crate::agent::compact::fold::StructuredFold;
use crate::error::Error;

/// Load the most recent `StructuredFold` baseline for `session_id`.
///
/// Returns `None` when:
/// - No row exists yet (first `/compact` on this session) — expected.
/// - The stored JSON fails to deserialize (e.g. older schema) — logged
///   at `warn` and treated as "no baseline" so the caller falls back
///   to a full rewrite rather than crashing.
/// - SQLite returns an error other than `QueryReturnedNoRows` — logged
///   at `warn`. We choose graceful degradation over surfacing the
///   error because `/compact` user-facing reliability outranks
///   visibility of an obscure cache miss.
pub fn load_baseline(conn: &Connection, session_id: &str) -> Option<StructuredFold> {
    let row: Result<Option<(String, String)>, rusqlite::Error> = conn
        .query_row(
            "SELECT fold_json, baseline_hash
             FROM agent_fold_baselines
             WHERE session_id = ?1",
            params![session_id],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
        )
        .optional();

    let (fold_json, expected_hash) = match row {
        Ok(Some(t)) => t,
        Ok(None) => return None, // first-compact on this session
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "[fold_baseline] load failed; treating as no-baseline (will fall back to full rewrite)",
            );
            return None;
        }
    };

    match serde_json::from_str::<StructuredFold>(&fold_json) {
        Ok(fold) => {
            // Defensive sanity check: if the persisted hash doesn't match
            // the recomputed one, the row is corrupt or was written by an
            // incompatible serializer. Log + drop — the upsert at the end
            // of the current compact will replace it with a fresh row.
            let recomputed = fold.baseline_hash();
            if recomputed != expected_hash {
                tracing::warn!(
                    session_id = %session_id,
                    expected = %expected_hash,
                    recomputed = %recomputed,
                    "[fold_baseline] baseline_hash mismatch; ignoring stored row",
                );
                None
            } else {
                Some(fold)
            }
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id,
                error = %e,
                "[fold_baseline] StructuredFold JSON deserialize failed; treating as no-baseline",
            );
            None
        }
    }
}

/// Upsert the baseline for `session_id` with `fold`. Always overwrites
/// the prior row — we want each `/compact` to baseline against its
/// *own* freshly-produced fold, so two consecutive small-delta
/// compacts don't compound drift against a stale anchor.
///
/// Returns a `Result` so the caller can choose between log-and-continue
/// (recommended for `/compact`'s soft-fail policy) and surfacing the
/// error (recommended for batch/scripted use).
pub fn upsert_baseline(
    conn: &Connection,
    session_id: &str,
    fold: &StructuredFold,
) -> Result<(), Error> {
    let fold_json = serde_json::to_string(fold)
        .map_err(|e| Error::Internal(format!("fold_baseline serialize: {e}")))?;
    let baseline_hash = fold.baseline_hash();
    let updated_at = chrono::Utc::now().timestamp_millis();

    conn.execute(
        "INSERT INTO agent_fold_baselines (session_id, fold_json, baseline_hash, updated_at)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(session_id) DO UPDATE SET
             fold_json = excluded.fold_json,
             baseline_hash = excluded.baseline_hash,
             updated_at = excluded.updated_at",
        params![session_id, fold_json, baseline_hash, updated_at],
    )
    .map_err(Error::Database)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::compact::fold::{DecisionWithRationale, FactWithEvidence, StructuredFold};

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        // V52 only — baseline storage is self-contained and doesn't
        // depend on agent_sessions for FK.
        conn.execute_batch(crate::db::migrations::V52_AGENT_FOLD_BASELINES)
            .unwrap();
        conn
    }

    fn sample_fold() -> StructuredFold {
        StructuredFold::default()
            .with_facts(vec![FactWithEvidence {
                statement: "uClaw uses rusqlite".into(),
                evidence: vec![],
                confidence: Some(0.9),
            }])
            .with_decisions(vec![DecisionWithRationale {
                decision: "Use V52 dedicated table".into(),
                rationale: "row-per-session cardinality differs from compaction_markers".into(),
                alternatives_considered: vec![
                    "Extend compaction_markers with is_current flag".into()
                ],
                evidence: vec![],
            }])
    }

    #[test]
    fn load_returns_none_when_table_empty() {
        let conn = fresh_db();
        assert!(load_baseline(&conn, "session-A").is_none());
    }

    #[test]
    fn upsert_then_load_roundtrips_fold() {
        let conn = fresh_db();
        let fold = sample_fold();
        upsert_baseline(&conn, "session-A", &fold).unwrap();

        let loaded = load_baseline(&conn, "session-A").expect("baseline should load");
        assert_eq!(
            loaded, fold,
            "load_baseline must return byte-identical fold"
        );
    }

    #[test]
    fn upsert_overwrites_existing_row() {
        let conn = fresh_db();
        upsert_baseline(&conn, "session-A", &StructuredFold::default()).unwrap();
        let fresh = sample_fold();
        upsert_baseline(&conn, "session-A", &fresh).unwrap();

        let loaded = load_baseline(&conn, "session-A").unwrap();
        assert_eq!(
            loaded, fresh,
            "second upsert must replace, not duplicate-insert"
        );

        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM agent_fold_baselines WHERE session_id = ?1",
                params!["session-A"],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 1, "PK conflict path must keep exactly one row");
    }

    #[test]
    fn distinct_sessions_get_distinct_baselines() {
        let conn = fresh_db();
        upsert_baseline(&conn, "session-A", &sample_fold()).unwrap();
        upsert_baseline(&conn, "session-B", &StructuredFold::default()).unwrap();

        assert_eq!(load_baseline(&conn, "session-A"), Some(sample_fold()));
        assert_eq!(
            load_baseline(&conn, "session-B"),
            Some(StructuredFold::default())
        );
    }

    #[test]
    fn corrupted_hash_returns_none() {
        let conn = fresh_db();
        // Insert a row whose stored baseline_hash doesn't match the
        // fold_json — simulates a corrupted row or an older serializer.
        let fold = sample_fold();
        let fold_json = serde_json::to_string(&fold).unwrap();
        conn.execute(
            "INSERT INTO agent_fold_baselines
                 (session_id, fold_json, baseline_hash, updated_at)
             VALUES ('session-C', ?1, 'definitely-wrong-hash', 0)",
            params![fold_json],
        )
        .unwrap();

        assert!(
            load_baseline(&conn, "session-C").is_none(),
            "hash mismatch must surface as no-baseline so caller falls back"
        );
    }

    #[test]
    fn corrupted_json_returns_none() {
        let conn = fresh_db();
        conn.execute(
            "INSERT INTO agent_fold_baselines
                 (session_id, fold_json, baseline_hash, updated_at)
             VALUES ('session-D', '{not valid fold json}', 'whatever', 0)",
            [],
        )
        .unwrap();
        assert!(load_baseline(&conn, "session-D").is_none());
    }
}
