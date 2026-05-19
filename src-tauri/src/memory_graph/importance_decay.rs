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

use rusqlite::{params, Connection, OptionalExtension};

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

// ─── DB collection (Q1a) ──────────────────────────────────────────────

/// Read the inputs needed by `compute_importance` from the existing
/// `memory_nodes` + `memory_versions` + `memory_edges` tables (V4
/// foundation) plus the prior `memory_importance_scores` row if any
/// (V44).
///
/// Returns `None` when the node doesn't exist or its required columns
/// can't be parsed — caller logs + skips.
///
/// Counts (`cited_count`, `edge_count`):
/// - `cited_count` = edges WHERE `child_node_id = node_id` (incoming
///   references from other nodes)
/// - `edge_count` = `cited_count` + edges where `parent_node_id = node_id`
///   (total edges touching this node)
///
/// Content length comes from the latest `memory_versions` row for this
/// node with `status = 'active'`; nodes with no active version are
/// treated as `content_chars = 0`.
///
/// Age is computed via SQLite's `julianday()` on the `updated_at` TEXT
/// column (uses the SQLite-native datetime format that V4 schema set).
pub fn collect_node_importance_inputs(
    conn: &Connection,
    node_id: &str,
) -> rusqlite::Result<Option<NodeImportanceInputs>> {
    // 1) Pull kind + metadata + age in one query against memory_nodes.
    let row = conn
        .query_row(
            "SELECT kind, COALESCE(metadata_json, ''),
                    COALESCE(julianday('now') - julianday(updated_at), 0.0)
             FROM memory_nodes WHERE id = ?1",
            [node_id],
            |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, f64>(2)?,
                ))
            },
        )
        .optional()?;
    let (kind_str, metadata_json, age_days) = match row {
        Some(t) => t,
        None => return Ok(None),
    };
    let is_boot = kind_str.eq_ignore_ascii_case("boot");

    // 2) Parse status + tier out of metadata_json (best-effort; missing
    //    keys map to Unknown / 3).
    let (status, tier) = parse_status_and_tier(&metadata_json);

    // 3) Edge counts (single query with conditional aggregation).
    let (cited_count, total_edges) = conn.query_row(
        "SELECT
             COALESCE(SUM(CASE WHEN child_node_id = ?1 THEN 1 ELSE 0 END), 0),
             COALESCE(SUM(CASE WHEN parent_node_id = ?1 OR child_node_id = ?1 THEN 1 ELSE 0 END), 0)
         FROM memory_edges",
        [node_id],
        |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
    )?;
    let cited_count = cited_count.max(0) as u32;
    let edge_count = total_edges.max(0) as u32;

    // 4) Content length from the latest active memory_versions row.
    let content_chars: u32 = conn
        .query_row(
            "SELECT COALESCE(LENGTH(content), 0)
             FROM memory_versions
             WHERE node_id = ?1 AND status = 'active'
             ORDER BY created_at DESC
             LIMIT 1",
            [node_id],
            |r| r.get::<_, i64>(0),
        )
        .optional()?
        .map(|n| n.max(0) as u32)
        .unwrap_or(0);

    // 5) Carry forward the existing half-life if we computed before.
    //    First-time computations start at the global base.
    let current_half_life_days: f64 = conn
        .query_row(
            "SELECT decay_half_life_days FROM memory_importance_scores WHERE node_id = ?1",
            [node_id],
            |r| r.get::<_, f64>(0),
        )
        .optional()?
        .unwrap_or(BASE_HALF_LIFE_DAYS);

    Ok(Some(NodeImportanceInputs {
        cited_count,
        edge_count,
        age_days,
        status,
        tier,
        is_boot,
        content_chars,
        current_half_life_days,
    }))
}

/// Node kinds that the Importance Decay batch loop considers "worth
/// scoring" by default. Boot / Identity / Value / Directive carry the
/// agent's long-term self-model; Curated / EntityPage are user-curated
/// knowledge. UserProfile / Episode / Procedure / Reference are kept
/// OUT of the default batch because they're either high-volume
/// (Reference, Episode), already managed elsewhere (UserProfile via
/// the facets store), or have a different durability semantics
/// (Procedure). Callers wanting to override this can pass their own
/// kind filter to `batch_recompute_importance`.
pub const DEFAULT_BATCH_KINDS: &[&str] = &[
    "boot",
    "identity",
    "value",
    "directive",
    "curated",
    "entity_page",
];

/// Result of one batch run. Cheap to serialize; logged + persisted by
/// the scheduler hook (Q1c) so the user can see "decay ran today,
/// touched N nodes, skipped M".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BatchRecomputeOutcome {
    /// Nodes that successfully went through compute + upsert.
    pub recomputed: usize,
    /// Nodes returned from the SELECT but collect/upsert errored.
    /// Logged at warn; loop continues.
    pub errored: usize,
}

/// Q1b — recompute importance for up to `limit` nodes, preferring
/// those that haven't been computed yet (NULL `last_computed_at`) or
/// were computed longest ago. Designed to be called periodically by
/// the scheduler hook (Q1c).
///
/// `kinds` filters which `memory_nodes.kind` values are eligible.
/// Pass [`DEFAULT_BATCH_KINDS`] for the standard recipe (see its
/// doc for rationale).
///
/// `limit` bounds work per call; a value around 100-500 keeps the
/// daily batch cheap on machines with 10k+ memory_nodes. Set to 0
/// to disable without flipping the config flag.
///
/// `computed_at_ms` is the unix-ms timestamp written into every
/// upserted row's `last_computed_at`. Pass `chrono::Utc::now().timestamp_millis()`
/// at the call site.
///
/// Per-node errors are logged + counted but don't abort the batch —
/// one corrupt metadata_json shouldn't poison the run.
pub fn batch_recompute_importance(
    conn: &Connection,
    kinds: &[&str],
    limit: usize,
    computed_at_ms: i64,
) -> rusqlite::Result<BatchRecomputeOutcome> {
    if limit == 0 || kinds.is_empty() {
        return Ok(BatchRecomputeOutcome { recomputed: 0, errored: 0 });
    }

    // Build the `kind IN (?, ?, ...)` clause dynamically. SQLite
    // doesn't support array bind so we expand placeholders manually;
    // safe because `kinds` is callsite-controlled (no user input).
    let placeholders = std::iter::repeat("?").take(kinds.len()).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT n.id
         FROM memory_nodes n
         LEFT JOIN memory_importance_scores s ON s.node_id = n.id
         WHERE n.kind IN ({})
         ORDER BY (s.last_computed_at IS NULL) DESC, s.last_computed_at ASC, n.id
         LIMIT ?",
        placeholders
    );

    // Collect node IDs first (so we don't hold the statement open
    // while doing per-node work).
    let mut stmt = conn.prepare(&sql)?;
    let mut params: Vec<rusqlite::types::Value> = kinds
        .iter()
        .map(|k| rusqlite::types::Value::from((*k).to_string()))
        .collect();
    params.push(rusqlite::types::Value::from(limit as i64));
    let node_ids: Vec<String> = stmt
        .query_map(rusqlite::params_from_iter(params.iter()), |r| r.get::<_, String>(0))?
        .filter_map(Result::ok)
        .collect();
    drop(stmt);

    let mut recomputed = 0usize;
    let mut errored = 0usize;
    for node_id in node_ids {
        match collect_node_importance_inputs(conn, &node_id) {
            Ok(Some(inputs)) => {
                let score = compute_importance(&inputs);
                match upsert_importance_score(conn, &node_id, &score, computed_at_ms) {
                    Ok(_) => recomputed += 1,
                    Err(e) => {
                        tracing::warn!(
                            node_id = %node_id,
                            error = %e,
                            "importance_decay: upsert failed; skipping node"
                        );
                        errored += 1;
                    }
                }
            }
            Ok(None) => {
                // Node disappeared between SELECT and collect — race
                // with a concurrent delete. Not an error, but not
                // counted as recomputed either.
            }
            Err(e) => {
                tracing::warn!(
                    node_id = %node_id,
                    error = %e,
                    "importance_decay: collect failed; skipping node"
                );
                errored += 1;
            }
        }
    }

    Ok(BatchRecomputeOutcome { recomputed, errored })
}

/// Pull `status` + `enrichment_tier` out of a node's `metadata_json`.
/// Tolerant of missing keys, missing object, malformed JSON.
fn parse_status_and_tier(metadata_json: &str) -> (NodeStatus, u8) {
    if metadata_json.trim().is_empty() {
        return (NodeStatus::Unknown, 3);
    }
    let v: serde_json::Value = match serde_json::from_str(metadata_json) {
        Ok(v) => v,
        Err(_) => return (NodeStatus::Unknown, 3),
    };
    let status = NodeStatus::from_metadata_str(v.get("status").and_then(|s| s.as_str()));
    let tier = v
        .get("enrichment_tier")
        .and_then(|t| t.as_u64())
        .and_then(|n| u8::try_from(n).ok())
        .filter(|&n| (1..=3).contains(&n))
        .unwrap_or(3);
    (status, tier)
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

    // ─── Q1a — collect_node_importance_inputs DB reader ────────────

    /// Helper: seed a memory_nodes row with given kind + metadata.
    fn seed_node(conn: &Connection, id: &str, kind: &str, metadata_json: &str) {
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json)
             VALUES (?1, 'default', ?2, 'test-title', ?3)",
            params![id, kind, metadata_json],
        )
        .unwrap();
    }

    /// Helper: seed a memory_versions row with given content.
    fn seed_version(conn: &Connection, node_id: &str, content: &str) {
        conn.execute(
            "INSERT INTO memory_versions (id, node_id, status, content)
             VALUES (?1, ?2, 'active', ?3)",
            params![format!("ver-{}", node_id), node_id, content],
        )
        .unwrap();
    }

    /// Helper: seed an edge between two nodes.
    fn seed_edge(conn: &Connection, parent: &str, child: &str) {
        conn.execute(
            "INSERT INTO memory_edges (id, space_id, parent_node_id, child_node_id)
             VALUES (?1, 'default', ?2, ?3)",
            params![format!("edge-{}-{}", parent, child), parent, child],
        )
        .unwrap();
    }

    #[test]
    fn collect_returns_none_for_unknown_node() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        let result = collect_node_importance_inputs(&conn, "ghost-node-id").unwrap();
        assert!(result.is_none(), "missing node should return None");
    }

    #[test]
    fn collect_reads_kind_metadata_content_and_edges() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();

        // Target node: a Boot node with status=verified, tier=1.
        seed_node(
            &conn,
            "n-target",
            "boot",
            r#"{"status":"verified","enrichment_tier":1}"#,
        );
        seed_version(
            &conn,
            "n-target",
            "This is some non-trivial content body that exceeds 50 characters easily.",
        );
        // 2 incoming citations + 1 outgoing edge → cited=2, total=3
        seed_node(&conn, "n-a", "reference", "{}");
        seed_node(&conn, "n-b", "reference", "{}");
        seed_node(&conn, "n-c", "reference", "{}");
        seed_edge(&conn, "n-a", "n-target");
        seed_edge(&conn, "n-b", "n-target");
        seed_edge(&conn, "n-target", "n-c");

        let inputs = collect_node_importance_inputs(&conn, "n-target")
            .unwrap()
            .expect("node exists");
        assert!(inputs.is_boot, "kind=boot must set is_boot");
        assert_eq!(inputs.status, NodeStatus::Verified);
        assert_eq!(inputs.tier, 1);
        assert_eq!(inputs.cited_count, 2);
        assert_eq!(inputs.edge_count, 3);
        assert!(
            inputs.content_chars >= 70,
            "content >= 70 chars expected, got {}",
            inputs.content_chars
        );
        // age_days near 0 because we just inserted; tolerate up to a
        // day for test-clock drift.
        assert!(
            inputs.age_days < 1.0,
            "freshly-seeded node age should be < 1 day, got {}",
            inputs.age_days
        );
        // First computation → carry forward the base half-life.
        assert!(
            (inputs.current_half_life_days - BASE_HALF_LIFE_DAYS).abs() < f64::EPSILON,
            "first-time should default to BASE_HALF_LIFE_DAYS"
        );
    }

    #[test]
    fn collect_carries_forward_existing_half_life() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-target", "reference", r#"{"status":"draft"}"#);
        seed_version(&conn, "n-target", "hi");

        // First pass: collect + compute + upsert.
        let inputs_v1 = collect_node_importance_inputs(&conn, "n-target")
            .unwrap()
            .unwrap();
        let score_v1 = compute_importance(&inputs_v1);
        upsert_importance_score(&conn, "n-target", &score_v1, 1_700_000_000_000).unwrap();

        // Second collection should pick up the persisted half-life,
        // not the global default.
        let inputs_v2 = collect_node_importance_inputs(&conn, "n-target")
            .unwrap()
            .unwrap();
        assert!(
            (inputs_v2.current_half_life_days - score_v1.decay_half_life_days).abs() < 1e-9,
            "second collection should carry the persisted half-life ({}); got {}",
            score_v1.decay_half_life_days,
            inputs_v2.current_half_life_days
        );
    }

    #[test]
    fn collect_treats_missing_metadata_keys_as_unknown_tier_3() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-bare", "reference", "{}");
        seed_version(&conn, "n-bare", "minimal content");
        let inputs = collect_node_importance_inputs(&conn, "n-bare")
            .unwrap()
            .unwrap();
        assert_eq!(inputs.status, NodeStatus::Unknown);
        assert_eq!(inputs.tier, 3);
    }

    #[test]
    fn collect_tolerates_malformed_metadata_json() {
        // Defensive: bad metadata_json should not panic / explode the
        // collector. Real DBs sometimes have legacy rows with quoted-
        // string metadata or partial JSON.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-bad", "reference", "{ this is not json");
        seed_version(&conn, "n-bad", "x");
        let inputs = collect_node_importance_inputs(&conn, "n-bad")
            .unwrap()
            .unwrap();
        assert_eq!(inputs.status, NodeStatus::Unknown);
        assert_eq!(inputs.tier, 3);
        assert!(!inputs.is_boot);
    }

    #[test]
    fn collect_handles_node_with_no_active_version() {
        // A node can exist without any active memory_versions row
        // (e.g. compacted, archival, or just-created without content).
        // Collector should still succeed with content_chars=0.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-empty", "reference", r#"{"status":"draft"}"#);
        let inputs = collect_node_importance_inputs(&conn, "n-empty")
            .unwrap()
            .unwrap();
        assert_eq!(inputs.content_chars, 0);
    }

    // ─── Q1b — batch_recompute_importance loop ─────────────────────

    #[test]
    fn batch_recompute_no_op_with_zero_limit_or_empty_kinds() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n", "entity_page", "{}");
        seed_version(&conn, "n", "x");

        let r1 = batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, 0, 1_700_000_000_000)
            .unwrap();
        assert_eq!(r1, BatchRecomputeOutcome { recomputed: 0, errored: 0 });

        let r2 = batch_recompute_importance(&conn, &[], 100, 1_700_000_000_000).unwrap();
        assert_eq!(r2, BatchRecomputeOutcome { recomputed: 0, errored: 0 });
    }

    #[test]
    fn batch_recompute_picks_only_eligible_kinds() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        // 3 eligible (entity_page, boot, curated) + 2 ineligible (reference, episode).
        seed_node(&conn, "n-page", "entity_page", "{}");
        seed_version(&conn, "n-page", "page");
        seed_node(&conn, "n-boot", "boot", "{}");
        seed_version(&conn, "n-boot", "boot identity");
        seed_node(&conn, "n-curated", "curated", "{}");
        seed_version(&conn, "n-curated", "curated note");
        seed_node(&conn, "n-ref", "reference", "{}");
        seed_version(&conn, "n-ref", "ref content");
        seed_node(&conn, "n-episode", "episode", "{}");
        seed_version(&conn, "n-episode", "ep content");

        let outcome =
            batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, 100, 1_700_000_000_000)
                .unwrap();
        assert_eq!(outcome.recomputed, 3, "exactly 3 eligible kinds");
        assert_eq!(outcome.errored, 0);

        // Verify the right rows are in memory_importance_scores.
        for eligible in ["n-page", "n-boot", "n-curated"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_importance_scores WHERE node_id = ?1",
                    [eligible],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 1, "{} should have a score row", eligible);
        }
        for ineligible in ["n-ref", "n-episode"] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM memory_importance_scores WHERE node_id = ?1",
                    [ineligible],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "{} should NOT have a score row", ineligible);
        }
    }

    #[test]
    fn batch_recompute_respects_limit() {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        for i in 0..10 {
            let id = format!("n-{}", i);
            seed_node(&conn, &id, "entity_page", "{}");
            seed_version(&conn, &id, "content");
        }
        let outcome =
            batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, 4, 1_700_000_000_000)
                .unwrap();
        assert_eq!(outcome.recomputed, 4, "limit must be respected");
    }

    #[test]
    fn batch_recompute_prefers_never_computed_nodes() {
        // Seed 2 nodes; pre-populate score for one (mimicking a prior
        // run) and leave the other untouched. The next batch with
        // limit=1 should pick the untouched one (NULL last_computed_at).
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-done", "entity_page", "{}");
        seed_version(&conn, "n-done", "done content");
        seed_node(&conn, "n-fresh", "entity_page", "{}");
        seed_version(&conn, "n-fresh", "fresh content");

        // Pre-populate n-done's score row.
        let inputs = collect_node_importance_inputs(&conn, "n-done")
            .unwrap()
            .unwrap();
        let score = compute_importance(&inputs);
        upsert_importance_score(&conn, "n-done", &score, 1_600_000_000_000).unwrap();

        // Batch with limit=1 should pick n-fresh.
        let outcome = batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, 1, 1_700_000_000_000)
            .unwrap();
        assert_eq!(outcome.recomputed, 1);
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM memory_importance_scores WHERE node_id = 'n-fresh'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "n-fresh should have been recomputed");
    }

    #[test]
    fn batch_recompute_orders_by_oldest_computed_first() {
        // Among already-scored nodes, prefer the one computed longest ago.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(&conn, "n-old", "entity_page", "{}");
        seed_version(&conn, "n-old", "old");
        seed_node(&conn, "n-recent", "entity_page", "{}");
        seed_version(&conn, "n-recent", "recent");

        // n-old computed at t=100, n-recent at t=200.
        for (id, ts) in [("n-old", 100_i64), ("n-recent", 200_i64)] {
            let inputs = collect_node_importance_inputs(&conn, id).unwrap().unwrap();
            let score = compute_importance(&inputs);
            upsert_importance_score(&conn, id, &score, ts).unwrap();
        }

        // Batch with limit=1 should pick n-old (oldest last_computed_at).
        let outcome = batch_recompute_importance(&conn, DEFAULT_BATCH_KINDS, 1, 300).unwrap();
        assert_eq!(outcome.recomputed, 1);
        // After the batch, n-old's last_computed_at should be 300; n-recent's 200.
        let old_ts: i64 = conn
            .query_row(
                "SELECT last_computed_at FROM memory_importance_scores WHERE node_id = 'n-old'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let recent_ts: i64 = conn
            .query_row(
                "SELECT last_computed_at FROM memory_importance_scores WHERE node_id = 'n-recent'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(old_ts, 300, "n-old should have been recomputed");
        assert_eq!(recent_ts, 200, "n-recent should NOT have been touched");
    }

    #[test]
    fn collect_then_compute_then_upsert_end_to_end() {
        // Full integration: collect inputs from a realistic node,
        // compute the score, persist, read back, confirm consistency.
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        seed_node(
            &conn,
            "n-e2e",
            "entity_page",
            r#"{"status":"verified","enrichment_tier":2}"#,
        );
        seed_version(
            &conn,
            "n-e2e",
            "This is a real-ish content body about something meaningful for the test.",
        );
        seed_node(&conn, "n-ref-1", "reference", "{}");
        seed_node(&conn, "n-ref-2", "reference", "{}");
        seed_edge(&conn, "n-ref-1", "n-e2e");
        seed_edge(&conn, "n-ref-2", "n-e2e");

        let inputs = collect_node_importance_inputs(&conn, "n-e2e")
            .unwrap()
            .unwrap();
        let score = compute_importance(&inputs);
        let n = upsert_importance_score(&conn, "n-e2e", &score, 1_700_000_000_000).unwrap();
        assert_eq!(n, 1);

        // Read-back: persisted importance equals computed importance.
        let stored: f64 = conn
            .query_row(
                "SELECT importance FROM memory_importance_scores WHERE node_id = 'n-e2e'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(
            (stored - score.importance).abs() < 1e-9,
            "stored ({}) should match computed ({})",
            stored,
            score.importance
        );
        // Cited node with verified + tier 2 status should clear baseline.
        assert!(stored > 0.60, "expected importance > 0.60, got {}", stored);
    }
}
