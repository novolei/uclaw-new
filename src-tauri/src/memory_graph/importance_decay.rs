//! Memory OS L3 §4.12.1 — Importance-Aware Decay (RETAINED per ADR 2026-05-20 §8).
//!
//! gbrain has lifecycle stages (Active/Dormant/Archived) but no actual
//! decay algorithm. This module computes an `importance` score per
//! `memory_node` and persists it into `memory_importance_scores`
//! (table created in V44, see [`crate::db::migrations::V44_L3_RETAINED_SCHEMA`]).
//!
//! Once populated, the score drives:
//!   - half-life tuning (important nodes get 45-day half-life, low-value 15-day)
//!   - low-value archival (when importance drops below threshold for
//!     N consecutive days the `archive_pending_since` column starts
//!     ticking)
//!   - recall ranking boost / penalty
//!
//! ## Scope of this PR (P2)
//!
//! This module ships **the formula + the upsert helper + tests**. It
//! does NOT yet:
//! - Schedule itself in `proactive::service`
//! - Hook into `tier_escalator` (the closest existing scenario)
//! - Iterate over the full `memory_nodes` table
//! - Expose a Tauri command
//!
//! The followup PRs (and the eventual Timeline + Dream Cycle work)
//! will wire those in. Today the module is dormant: nothing in the
//! crate calls it yet, but the formula is correct, tested, and ready
//! for the scheduler.
//!
//! ## Formula (from spec §4.12.1)
//!
//! ```text
//! importance = clamp(
//!     0.5                                          // base_value
//!   + log(1 + cited_count)        * 0.20           // citation_factor
//!   + log(1 + edge_count)         * 0.15           // edge_factor
//!   + recency_factor(updated, h)  * 0.20           // recency_factor
//!   + status_bonus(status)        * 0.15           // status_bonus
//!   + tier_bonus(tier)            * 0.10           // tier_bonus
//!   + boot_bonus                  * 0.20           // +0.2 if kind=Boot
//!   - low_value_penalty           * 0.30           // short + no edges
//!   , 0.0, 1.0)
//!
//! half_life_days = 30.0 * (0.5 + importance)       // 15 ~ 45 days
//! ```

use rusqlite::{params, Connection};

/// Default base half-life in days when computing decay. Importance
/// multiplies this by `0.5 + importance` so the actual half-life
/// ranges roughly 15–45 days depending on per-node score.
pub const BASE_HALF_LIFE_DAYS: f64 = 30.0;

/// Inputs to the importance formula — all the per-node facts the
/// computation needs. Filled by a future "collect from DB" helper
/// (not in this PR); for now, tests construct it directly with
/// synthetic values.
#[derive(Debug, Clone, Copy)]
pub struct NodeImportanceInputs {
    /// Number of edges pointing AT this node (incoming references).
    pub cited_count: u32,
    /// Total edges (incoming + outgoing) touching this node.
    pub edge_count: u32,
    /// Wall-clock age in days since the node's last update. Used by
    /// `recency_factor` with the node's current half-life.
    pub age_days: f64,
    /// Node's status (parsed from metadata_json.status); `Verified`
    /// > `Draft` > `Inferred` > `Unknown`.
    pub status: NodeStatus,
    /// Enrichment tier (parsed from metadata_json.enrichment_tier);
    /// 1 = highest care, 3 = lowest. Tier 1 gets a small bonus, tier 3
    /// none.
    pub tier: u8,
    /// `true` if the node's kind is `Boot` (the agent's foundational
    /// identity / values / directives — never decay these).
    pub is_boot: bool,
    /// Content length in characters. Used by `low_value_penalty`.
    pub content_chars: u32,
    /// Half-life to use for the `recency_factor`. Pass the previous
    /// computation's half-life so the score is self-stabilizing across
    /// runs; pass [`BASE_HALF_LIFE_DAYS`] on first computation.
    pub current_half_life_days: f64,
}

/// Status classes recognized by the formula. Parser maps the
/// metadata_json string values to this enum; unknown / missing maps
/// to `Unknown` (zero bonus).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeStatus {
    /// Human-verified or LLM-verified with high confidence.
    Verified,
    /// Active draft, recently edited.
    Draft,
    /// LLM-inferred without explicit source.
    Inferred,
    /// Status not recorded.
    Unknown,
}

impl NodeStatus {
    /// Map status to a [-0.5, 1.0] additive bonus. `Verified` is the
    /// only one with a meaningfully positive bonus; `Inferred` is
    /// slightly penalized; `Unknown` is neutral.
    pub fn bonus(self) -> f64 {
        match self {
            Self::Verified => 1.0,
            Self::Draft => 0.3,
            Self::Inferred => -0.3,
            Self::Unknown => 0.0,
        }
    }

    /// Parse the metadata_json string. Tolerant of case + whitespace.
    pub fn from_metadata_str(s: Option<&str>) -> Self {
        match s.map(|x| x.trim().to_ascii_lowercase()).as_deref() {
            Some("verified") => Self::Verified,
            Some("draft") => Self::Draft,
            Some("inferred") => Self::Inferred,
            _ => Self::Unknown,
        }
    }
}

/// Output of the importance computation — every factor exposed so
/// the persisted row can be debugged after the fact.
///
/// **All factor fields are the WEIGHTED contribution** that gets
/// added (or for `low_value_penalty`, subtracted) from `base_value`
/// to produce the raw pre-clamp score. E.g. `citation_factor =
/// ln(1+cited) * 0.20`, not the raw `ln(1+cited)`. Storing weighted
/// values lets the persisted row debug what each contributor was
/// worth without redoing arithmetic.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NodeImportance {
    /// Base score before any factor adjustments. Always 0.5.
    pub base_value: f64,
    /// Weighted contribution from `ln(1 + cited_count)`.
    pub citation_factor: f64,
    /// Weighted contribution from `ln(1 + edge_count)`.
    pub edge_factor: f64,
    /// Weighted contribution from `2^(-age_days / half_life_days)`.
    pub recency_factor: f64,
    /// Weighted contribution from node status (Verified > Draft > Inferred).
    pub status_bonus: f64,
    /// Weighted contribution from enrichment tier (1 > 2 > 3).
    pub tier_bonus: f64,
    /// Boot node bonus (+0.20 when `is_boot=true`, else 0.0). Already weighted.
    pub boot_bonus: f64,
    /// Weighted **subtractive** penalty for short, isolated nodes.
    /// Range: 0.0 (no penalty) to 0.30 (max penalty when content
    /// is short AND no graph context). This value is SUBTRACTED
    /// from the sum of other factors.
    pub low_value_penalty: f64,
    /// Final clamped [0.0, 1.0] importance score.
    pub importance: f64,
    /// Updated half-life for use in NEXT cycle's `recency_factor`.
    pub decay_half_life_days: f64,
}

/// Compute the importance score per spec §4.12.1. Pure function;
/// trivially testable; no I/O.
///
/// All arithmetic is in `f64` to avoid `log(1 + huge_count)` underflow
/// at the f32 boundary; the final scoring is written back as `f64`
/// (matches `memory_importance_scores.importance REAL` SQLite type).
pub fn compute_importance(inputs: &NodeImportanceInputs) -> NodeImportance {
    let base_value = 0.5_f64;
    let citation_factor = ((1.0 + inputs.cited_count as f64).ln()) * 0.20;
    let edge_factor = ((1.0 + inputs.edge_count as f64).ln()) * 0.15;
    let recency_factor = recency_factor(inputs.age_days, inputs.current_half_life_days) * 0.20;
    let status_bonus = inputs.status.bonus() * 0.15;
    let tier_bonus = tier_bonus_for(inputs.tier) * 0.10;
    let boot_bonus = if inputs.is_boot { 0.20 } else { 0.0 };
    let low_value_penalty = low_value_penalty_for(inputs) * 0.30;

    let raw = base_value
        + citation_factor
        + edge_factor
        + recency_factor
        + status_bonus
        + tier_bonus
        + boot_bonus
        - low_value_penalty;

    let importance = raw.clamp(0.0, 1.0);

    // half_life = base * (0.5 + importance) → 15 days when importance=0,
    // 45 days when importance=1.
    let decay_half_life_days = BASE_HALF_LIFE_DAYS * (0.5 + importance);

    NodeImportance {
        base_value,
        citation_factor,
        edge_factor,
        recency_factor,
        status_bonus,
        tier_bonus,
        boot_bonus,
        low_value_penalty,
        importance,
        decay_half_life_days,
    }
}

/// Ebbinghaus-style exponential decay: at the half-life mark the
/// factor is 0.5; after 2× half-life it's 0.25; etc.
///
/// Returns 1.0 for negative ages (defensive: should never happen but
/// don't blow up). Returns 0.0 for half_life ≤ 0 (also defensive).
fn recency_factor(age_days: f64, half_life_days: f64) -> f64 {
    if age_days <= 0.0 {
        return 1.0;
    }
    if half_life_days <= 0.0 {
        return 0.0;
    }
    // 2^(-age/half_life)
    (-age_days / half_life_days * std::f64::consts::LN_2).exp()
}

/// Map enrichment tier to additive bonus. Tier 1 is the "user
/// actively cares" tier; tier 2 mid; tier 3 lowest. Tiers outside
/// 1-3 are treated as tier 3 (no bonus).
fn tier_bonus_for(tier: u8) -> f64 {
    match tier {
        1 => 1.0,
        2 => 0.5,
        _ => 0.0,
    }
}

/// Penalty for "obviously low value" nodes — short content AND no
/// graph context. The 0.30 multiplier outside makes this max −0.30
/// when fully triggered.
fn low_value_penalty_for(inputs: &NodeImportanceInputs) -> f64 {
    // Boot nodes are never low value, regardless of length.
    if inputs.is_boot {
        return 0.0;
    }
    let short_content = inputs.content_chars < 50;
    let no_graph = inputs.cited_count == 0 && inputs.edge_count == 0;
    if short_content && no_graph {
        1.0
    } else if short_content {
        0.5
    } else if no_graph {
        0.3
    } else {
        0.0
    }
}

// ─── DB persistence ────────────────────────────────────────────────────

/// Upsert a computed [`NodeImportance`] row into `memory_importance_scores`.
///
/// Uses `INSERT ... ON CONFLICT (node_id) DO UPDATE SET ...` so the
/// caller doesn't need to know whether the row exists. `archive_pending_since`
/// is updated externally by the archival logic (not this fn).
///
/// Returns the number of rows changed (1 for new, 1 for update — always 1
/// on success; 0 if the node_id doesn't exist in `memory_nodes` and FK
/// enforcement is on).
pub fn upsert_importance_score(
    conn: &Connection,
    node_id: &str,
    score: &NodeImportance,
    computed_at_ms: i64,
) -> rusqlite::Result<usize> {
    conn.execute(
        "INSERT INTO memory_importance_scores
             (node_id, base_value, citation_factor, edge_factor,
              recency_factor, status_bonus, penalty, importance,
              decay_half_life_days, last_computed_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
         ON CONFLICT(node_id) DO UPDATE SET
             base_value = excluded.base_value,
             citation_factor = excluded.citation_factor,
             edge_factor = excluded.edge_factor,
             recency_factor = excluded.recency_factor,
             status_bonus = excluded.status_bonus,
             penalty = excluded.penalty,
             importance = excluded.importance,
             decay_half_life_days = excluded.decay_half_life_days,
             last_computed_at = excluded.last_computed_at",
        params![
            node_id,
            score.base_value,
            score.citation_factor,
            score.edge_factor,
            score.recency_factor,
            score.status_bonus,
            // Note: schema column is named `penalty`; we persist the
            // already-weighted `low_value_penalty` (= raw * 0.30,
            // computed by `compute_importance`) so the DB row matches
            // what was actually subtracted from `importance`. Other
            // bonuses go in their named columns. Final `importance`
            // is the consolidated value the read-path uses.
            score.low_value_penalty,
            score.importance,
            score.decay_half_life_days,
            computed_at_ms,
        ],
    )
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn baseline_inputs() -> NodeImportanceInputs {
        NodeImportanceInputs {
            cited_count: 0,
            edge_count: 0,
            age_days: 0.0,
            status: NodeStatus::Unknown,
            tier: 3,
            is_boot: false,
            content_chars: 200,
            current_half_life_days: BASE_HALF_LIFE_DAYS,
        }
    }

    #[test]
    fn baseline_node_score_is_around_half_minus_no_graph_penalty() {
        // baseline: no citations, no edges, content 200 chars, no
        // status, tier 3, not boot, age 0. The "no_graph" penalty
        // (cited+edge both zero) fires at 0.3 weight = 0.09.
        // recency_factor at age=0 is 1.0 × 0.20 = 0.20.
        // Total: 0.5 + 0 + 0 + 0.20 + 0 + 0 + 0 - 0.09 = 0.61.
        let out = compute_importance(&baseline_inputs());
        assert!(
            (out.importance - 0.61).abs() < 0.01,
            "baseline should be ~0.61, got {}",
            out.importance
        );
    }

    #[test]
    fn boot_node_clamps_high_regardless_of_content() {
        // Boot nodes get +0.20 boot_bonus AND no low_value_penalty.
        let mut inputs = baseline_inputs();
        inputs.is_boot = true;
        inputs.content_chars = 1; // would normally penalize hard
        inputs.tier = 3;
        let out = compute_importance(&inputs);
        // 0.5 (base) + 0 (cit) + 0 (edge) + 0.20 (recency) + 0 (status)
        // + 0 (tier3) + 0.20 (boot) - 0 (boot bypasses penalty) = 0.90
        assert!(
            out.importance > 0.85,
            "boot node should be high-importance, got {}",
            out.importance
        );
        assert_eq!(out.low_value_penalty, 0.0, "boot bypasses low-value penalty");
    }

    #[test]
    fn well_cited_verified_tier1_entity_saturates_near_one() {
        let mut inputs = baseline_inputs();
        inputs.cited_count = 50;
        inputs.edge_count = 80;
        inputs.status = NodeStatus::Verified;
        inputs.tier = 1;
        let out = compute_importance(&inputs);
        // Bunch of positive factors → should clamp at 1.0 or near
        assert!(
            out.importance > 0.95,
            "well-cited verified tier-1 should be near 1.0, got {}",
            out.importance
        );
    }

    #[test]
    fn very_old_unverified_node_drops_below_baseline() {
        let mut inputs = baseline_inputs();
        inputs.age_days = 180.0; // 6 half-lives at 30-day default
        inputs.content_chars = 200;
        let out = compute_importance(&inputs);
        // recency_factor decays to ~0.015 → 0.003 weight; rest unchanged
        // Total: 0.5 + 0 + 0 + ~0 + 0 + 0 + 0 - 0.09 (no_graph) ≈ 0.41
        assert!(
            out.importance < 0.45,
            "very-old unverified should drop, got {}",
            out.importance
        );
        // And below the baseline-with-young recency_factor
        assert!(out.importance < compute_importance(&baseline_inputs()).importance);
    }

    #[test]
    fn short_isolated_node_takes_full_low_value_penalty() {
        let mut inputs = baseline_inputs();
        inputs.content_chars = 10; // short
        inputs.cited_count = 0;
        inputs.edge_count = 0;
        let out = compute_importance(&inputs);
        // Raw low_value_penalty triggers at 1.0 (short + no graph),
        // then weight × 0.30 yields the stored WEIGHTED value 0.30.
        // We assert on the weighted value because that's what the
        // struct exposes (consistent with citation_factor etc.).
        assert!(
            (out.low_value_penalty - 0.30).abs() < f64::EPSILON,
            "short + no graph should weighted-penalize 0.30, got {}",
            out.low_value_penalty
        );
        // Final: 0.5 + 0 + 0 + 0.20 + 0 + 0 + 0 - 0.30 = 0.40
        assert!(out.importance < 0.45);
    }

    #[test]
    fn importance_clamps_to_unit_interval() {
        // Construct extreme inputs to verify clamp boundaries.
        let mut inputs = baseline_inputs();
        inputs.cited_count = u32::MAX / 2;
        inputs.edge_count = u32::MAX / 2;
        inputs.status = NodeStatus::Verified;
        inputs.tier = 1;
        inputs.is_boot = true;
        let out_high = compute_importance(&inputs);
        assert!(out_high.importance <= 1.0, "must clamp at 1.0");
        assert!(out_high.importance >= 0.95);

        // Construct minimal inputs.
        let mut inputs = baseline_inputs();
        inputs.age_days = 10_000.0;
        inputs.status = NodeStatus::Inferred;
        inputs.content_chars = 1;
        let out_low = compute_importance(&inputs);
        assert!(out_low.importance >= 0.0, "must clamp at 0.0");
    }

    #[test]
    fn half_life_varies_within_15_to_45_day_band() {
        // Per spec: half_life ranges 15-45 days as importance varies 0-1.
        let lower = NodeImportance {
            base_value: 0.0,
            citation_factor: 0.0,
            edge_factor: 0.0,
            recency_factor: 0.0,
            status_bonus: 0.0,
            tier_bonus: 0.0,
            boot_bonus: 0.0,
            low_value_penalty: 0.0,
            importance: 0.0,
            decay_half_life_days: 0.0,
        };
        // Recompute with our formula directly
        let h_at_0 = BASE_HALF_LIFE_DAYS * (0.5 + 0.0);
        let h_at_1 = BASE_HALF_LIFE_DAYS * (0.5 + 1.0);
        assert!((h_at_0 - 15.0).abs() < f64::EPSILON);
        assert!((h_at_1 - 45.0).abs() < f64::EPSILON);
        // Defensive: ensure the struct's `decay_half_life_days` isn't
        // independently computed in a way that drifts.
        let inputs = NodeImportanceInputs {
            cited_count: 50,
            edge_count: 80,
            age_days: 0.0,
            status: NodeStatus::Verified,
            tier: 1,
            is_boot: true,
            content_chars: 200,
            current_half_life_days: BASE_HALF_LIFE_DAYS,
        };
        let out = compute_importance(&inputs);
        assert!(
            out.decay_half_life_days > 40.0 && out.decay_half_life_days <= 45.0,
            "high-importance half-life should be in [40,45], got {}",
            out.decay_half_life_days
        );
        let _ = lower; // suppress unused warning while keeping the doc-shape
    }

    #[test]
    fn status_parse_round_trip() {
        assert_eq!(NodeStatus::from_metadata_str(Some("verified")), NodeStatus::Verified);
        assert_eq!(NodeStatus::from_metadata_str(Some(" Draft ")), NodeStatus::Draft);
        assert_eq!(NodeStatus::from_metadata_str(Some("INFERRED")), NodeStatus::Inferred);
        assert_eq!(NodeStatus::from_metadata_str(Some("nonsense")), NodeStatus::Unknown);
        assert_eq!(NodeStatus::from_metadata_str(None), NodeStatus::Unknown);
        assert_eq!(NodeStatus::from_metadata_str(Some("")), NodeStatus::Unknown);
    }

    #[test]
    fn recency_factor_at_half_life_is_half() {
        // Defensive: ensure 2^(-age/h) hits 0.5 at age == half_life.
        let h = 30.0_f64;
        let r = super::recency_factor(h, h);
        assert!((r - 0.5).abs() < 1e-9, "expected 0.5 at half-life, got {}", r);
        // At 2× half-life → 0.25.
        let r2 = super::recency_factor(60.0, h);
        assert!((r2 - 0.25).abs() < 1e-9);
    }

    #[test]
    fn upsert_writes_then_overwrites_in_memory_score_table() {
        // V44 migration creates `memory_importance_scores`. Run full
        // migration so the table exists, then upsert twice and verify
        // the second value overwrites the first.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn.execute("PRAGMA foreign_keys = OFF", []).unwrap(); // skip FK for test

        let inputs = baseline_inputs();
        let score_v1 = compute_importance(&inputs);
        let n = upsert_importance_score(&conn, "node-x", &score_v1, 1_700_000_000_000)
            .expect("first upsert");
        assert_eq!(n, 1);

        let importance_v1: f64 = conn
            .query_row(
                "SELECT importance FROM memory_importance_scores WHERE node_id = 'node-x'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        // Re-compute with a much higher-importance input, overwrite.
        let mut inputs2 = baseline_inputs();
        inputs2.cited_count = 100;
        inputs2.status = NodeStatus::Verified;
        inputs2.tier = 1;
        let score_v2 = compute_importance(&inputs2);
        upsert_importance_score(&conn, "node-x", &score_v2, 1_700_000_010_000)
            .expect("second upsert");

        let importance_v2: f64 = conn
            .query_row(
                "SELECT importance FROM memory_importance_scores WHERE node_id = 'node-x'",
                [],
                |r| r.get(0),
            )
            .unwrap();

        assert!(
            importance_v2 > importance_v1,
            "v2 ({}) should be greater than v1 ({})",
            importance_v2,
            importance_v1
        );
        // Verify only one row exists (ON CONFLICT works, not INSERT
        // OR IGNORE which would silently drop the update).
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_importance_scores WHERE node_id = 'node-x'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "upsert must keep exactly one row per node_id");
    }
}
