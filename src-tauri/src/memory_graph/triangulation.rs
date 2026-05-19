//! Memory OS L3 §4.12.5 RETAINED — Cross-Source Triangulation.
//!
//! When multiple independent sources agree on a claim, the agent's
//! confidence in that claim should be boosted. When sources
//! disagree, surface the contradiction. gbrain has no equivalent —
//! it stores per-page sources but doesn't aggregate agreement across
//! them.
//!
//! ## V1 scope (this PR)
//!
//! - V47 migration: `triangulation_evidence` table with UNIQUE
//!   (claim_id, source_node_id) — one row per claim/source pair
//! - `record_evidence` / `record_evidence_upsert` — write helpers
//! - `summarize_claim` — read aggregation returning agreement counts
//! - `compute_confidence_boost` — pure function from agreement
//!   summary to a [0, 1] boost magnitude
//!
//! ## V2 scope (future PR)
//!
//! - Scheduler that scans EntityPages for sources, records evidence
//!   rows on each new compile
//! - LLM-driven "does source X support claim Y?" classifier (this
//!   PR assumes the caller already knows agree/disagree)
//! - UI surfacing "this claim has 5 supporting + 1 contradicting
//!   source" on EntityPage cards
//! - Integration with recall.rs to fold the boost into recall scores
//!
//! ## Boost formula (from spec §4.12.5)
//!
//! - 1 source agrees: boost = 0.0 (single-source ≠ triangulation)
//! - 2 sources agree, no disagreement: boost = 0.20
//! - 3+ sources agree, no disagreement: boost = 0.30
//! - Any disagreement present: boost = 0.0 (contradiction blocks
//!   the boost; surface for review instead)
//!
//! Weights from `weight` column allow callers to mark a source as
//! more authoritative (default 1.0). The summary returns weighted
//! agree/disagree counts; threshold logic uses those.

use rusqlite::{params, Connection};

/// Above-this confidence boost from triangulation. Caller multiplies
/// into the recall score (or adds to existing confidence).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidenceBoost {
    pub boost: f64,
    pub agree_count: u32,
    pub disagree_count: u32,
    pub total_weight: f64,
}

/// Summary of evidence rows for one claim, suitable for the
/// boost computation or surfacing in UI.
#[derive(Debug, Clone, PartialEq)]
pub struct ClaimSummary {
    pub claim_id: String,
    pub agree_count: u32,
    pub disagree_count: u32,
    pub agree_weight: f64,
    pub disagree_weight: f64,
}

/// Record (or upsert) one evidence row. `agrees=true` means the
/// source supports the claim; `false` means it contradicts.
/// `weight` defaults to 1.0; pass higher (e.g. 2.0) for
/// authoritative sources.
///
/// Idempotent via the V47 UNIQUE(claim_id, source_node_id) +
/// INSERT ... ON CONFLICT ... DO UPDATE: re-recording for the same
/// (claim, source) updates the agrees/weight in place.
pub fn record_evidence(
    conn: &Connection,
    id: &str,
    claim_id: &str,
    source_node_id: &str,
    agrees: bool,
    weight: f64,
    note: Option<&str>,
    computed_at_ms: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO triangulation_evidence
             (id, claim_id, source_node_id, agrees, weight, note, computed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
         ON CONFLICT(claim_id, source_node_id) DO UPDATE SET
             agrees = excluded.agrees,
             weight = excluded.weight,
             note = excluded.note,
             computed_at = excluded.computed_at",
        params![
            id,
            claim_id,
            source_node_id,
            if agrees { 1_i32 } else { 0 },
            weight,
            note,
            computed_at_ms,
        ],
    )?;
    Ok(())
}

/// Aggregate all evidence rows for one claim. Returns the
/// [`ClaimSummary`] used by [`compute_confidence_boost`] and
/// (in a future PR) the UI.
pub fn summarize_claim(conn: &Connection, claim_id: &str) -> rusqlite::Result<ClaimSummary> {
    let (agree_count, disagree_count, agree_weight, disagree_weight): (i64, i64, f64, f64) =
        conn.query_row(
            "SELECT
                 COALESCE(SUM(CASE WHEN agrees = 1 THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN agrees = 0 THEN 1 ELSE 0 END), 0),
                 COALESCE(SUM(CASE WHEN agrees = 1 THEN weight ELSE 0 END), 0.0),
                 COALESCE(SUM(CASE WHEN agrees = 0 THEN weight ELSE 0 END), 0.0)
             FROM triangulation_evidence WHERE claim_id = ?1",
            [claim_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )?;
    Ok(ClaimSummary {
        claim_id: claim_id.to_string(),
        agree_count: agree_count.max(0) as u32,
        disagree_count: disagree_count.max(0) as u32,
        agree_weight,
        disagree_weight,
    })
}

/// Compute the triangulation confidence boost from a summary. Pure
/// function; trivially testable.
///
/// Rules (per spec §4.12.5):
/// - Any disagreement → boost = 0.0 (contradiction kills the boost)
/// - 1 agreement → boost = 0.0 (single-source ≠ triangulation)
/// - 2 agreements → boost = 0.20
/// - 3+ agreements → boost = 0.30
pub fn compute_confidence_boost(summary: &ClaimSummary) -> ConfidenceBoost {
    let boost = if summary.disagree_count > 0 {
        0.0
    } else if summary.agree_count >= 3 {
        0.30
    } else if summary.agree_count == 2 {
        0.20
    } else {
        0.0
    };
    ConfidenceBoost {
        boost,
        agree_count: summary.agree_count,
        disagree_count: summary.disagree_count,
        total_weight: summary.agree_weight + summary.disagree_weight,
    }
}

/// One-shot convenience: read + compute the boost for a claim. The
/// recall path will call this once per candidate claim during scoring.
pub fn boost_for_claim(conn: &Connection, claim_id: &str) -> rusqlite::Result<ConfidenceBoost> {
    let summary = summarize_claim(conn, claim_id)?;
    Ok(compute_confidence_boost(&summary))
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap();
        conn
    }

    #[test]
    fn single_agreeing_source_yields_zero_boost() {
        let summary = ClaimSummary {
            claim_id: "c1".into(),
            agree_count: 1,
            disagree_count: 0,
            agree_weight: 1.0,
            disagree_weight: 0.0,
        };
        let b = compute_confidence_boost(&summary);
        assert_eq!(b.boost, 0.0, "1 source != triangulation");
    }

    #[test]
    fn two_agreeing_sources_yields_0_20_boost() {
        let summary = ClaimSummary {
            claim_id: "c1".into(),
            agree_count: 2,
            disagree_count: 0,
            agree_weight: 2.0,
            disagree_weight: 0.0,
        };
        let b = compute_confidence_boost(&summary);
        assert!((b.boost - 0.20).abs() < 1e-9);
    }

    #[test]
    fn three_plus_agreeing_sources_yield_0_30_boost() {
        let summary = ClaimSummary {
            claim_id: "c1".into(),
            agree_count: 3,
            disagree_count: 0,
            agree_weight: 3.0,
            disagree_weight: 0.0,
        };
        let b = compute_confidence_boost(&summary);
        assert!((b.boost - 0.30).abs() < 1e-9);
        // And 5 should also be 0.30 (no further escalation).
        let summary_5 = ClaimSummary {
            claim_id: "c1".into(),
            agree_count: 5,
            disagree_count: 0,
            agree_weight: 5.0,
            disagree_weight: 0.0,
        };
        let b5 = compute_confidence_boost(&summary_5);
        assert!((b5.boost - 0.30).abs() < 1e-9);
    }

    #[test]
    fn any_disagreement_zeros_the_boost() {
        // Even with 5 agreeing sources, a single contradicting
        // source kills the boost (surfaces for review instead).
        let summary = ClaimSummary {
            claim_id: "c1".into(),
            agree_count: 5,
            disagree_count: 1,
            agree_weight: 5.0,
            disagree_weight: 1.0,
        };
        let b = compute_confidence_boost(&summary);
        assert_eq!(b.boost, 0.0, "any disagreement blocks the boost");
    }

    #[test]
    fn record_evidence_writes_row_with_correct_agrees_flag() {
        let conn = fresh_conn();
        record_evidence(&conn, "t1", "claim-1", "src-1", true, 1.0, None, 1_700_000_000_000)
            .unwrap();
        record_evidence(&conn, "t2", "claim-1", "src-2", false, 2.0, Some("contradicts"), 1_700_000_001_000)
            .unwrap();
        let summary = summarize_claim(&conn, "claim-1").unwrap();
        assert_eq!(summary.agree_count, 1);
        assert_eq!(summary.disagree_count, 1);
        assert!((summary.agree_weight - 1.0).abs() < 1e-9);
        assert!((summary.disagree_weight - 2.0).abs() < 1e-9);
    }

    #[test]
    fn record_evidence_is_idempotent_on_same_claim_and_source() {
        // INSERT OR REPLACE behavior: re-recording for the same
        // (claim_id, source_node_id) overwrites the old row.
        let conn = fresh_conn();
        record_evidence(&conn, "t1", "claim-1", "src-1", true, 1.0, None, 1_700_000_000_000)
            .unwrap();
        // Same source, now disagrees with higher weight.
        record_evidence(&conn, "t2", "claim-1", "src-1", false, 3.0, Some("retracted"), 1_700_000_001_000)
            .unwrap();
        let summary = summarize_claim(&conn, "claim-1").unwrap();
        assert_eq!(summary.agree_count, 0, "agreement should be overwritten");
        assert_eq!(summary.disagree_count, 1);
        assert!((summary.disagree_weight - 3.0).abs() < 1e-9);
        // And we should still have exactly 1 row, not 2.
        let row_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM triangulation_evidence WHERE claim_id = 'claim-1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(row_count, 1);
    }

    #[test]
    fn summary_for_unknown_claim_returns_zeroes() {
        let conn = fresh_conn();
        let summary = summarize_claim(&conn, "ghost-claim").unwrap();
        assert_eq!(summary.agree_count, 0);
        assert_eq!(summary.disagree_count, 0);
        assert_eq!(summary.agree_weight, 0.0);
        assert_eq!(summary.disagree_weight, 0.0);
        let b = compute_confidence_boost(&summary);
        assert_eq!(b.boost, 0.0);
    }

    #[test]
    fn boost_for_claim_end_to_end() {
        let conn = fresh_conn();
        // 3 agreeing sources, no contradicts → 0.30 boost
        for (id, src) in [("t1", "s1"), ("t2", "s2"), ("t3", "s3")] {
            record_evidence(&conn, id, "claim-x", src, true, 1.0, None, 1_700_000_000_000)
                .unwrap();
        }
        let b = boost_for_claim(&conn, "claim-x").unwrap();
        assert!((b.boost - 0.30).abs() < 1e-9);
        assert_eq!(b.agree_count, 3);
        assert_eq!(b.disagree_count, 0);
    }

    #[test]
    fn boost_includes_total_weight_for_introspection() {
        // total_weight is the sum of agree + disagree weights, useful
        // for callers that want to surface "this claim has been
        // checked against N units of weighted evidence".
        let conn = fresh_conn();
        record_evidence(&conn, "t1", "claim-x", "s1", true, 2.0, None, 100).unwrap();
        record_evidence(&conn, "t2", "claim-x", "s2", false, 1.5, None, 200).unwrap();
        let b = boost_for_claim(&conn, "claim-x").unwrap();
        assert!((b.total_weight - 3.5).abs() < 1e-9);
    }

    #[test]
    fn different_claims_dont_cross_pollinate() {
        let conn = fresh_conn();
        record_evidence(&conn, "t1", "claim-A", "s1", true, 1.0, None, 100).unwrap();
        record_evidence(&conn, "t2", "claim-A", "s2", true, 1.0, None, 100).unwrap();
        record_evidence(&conn, "t3", "claim-B", "s1", true, 1.0, None, 100).unwrap();
        let summary_a = summarize_claim(&conn, "claim-A").unwrap();
        let summary_b = summarize_claim(&conn, "claim-B").unwrap();
        assert_eq!(summary_a.agree_count, 2);
        assert_eq!(summary_b.agree_count, 1);
    }
}
