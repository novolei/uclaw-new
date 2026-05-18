//! Tier escalator scenario — Memory OS Foundation Phase 6.1.
//!
//! Promotes / demotes the `enrichment_tier` field on EntityPage
//! metadata based on inbound backlink count, so that downstream
//! enrichers (Phase 6.2 `EntitySynthesizer`, future LLM re-compilers)
//! know which pages are heavyweight enough to deserve expensive
//! re-synthesis.
//!
//! ## Tiers (per spec §3.2 B3)
//!
//! - **Tier 3 (stub)**  — 1-2 mentions. Default; one-sentence
//!   `compiled_truth`.
//! - **Tier 2 (rich)**  — 3-7 mentions. LLM writes 200-500 chars.
//! - **Tier 1 (full)**  — ≥ 8 mentions. LLM writes a full profile
//!   with cross-source synthesis.
//!
//! Lower tier number = more important. "Upgrade" moves to a lower
//! number (Tier 3 → 2 → 1); "downgrade" moves the other way. A page
//! that has lost backlinks (e.g. its referring Episodes were
//! compacted) gracefully demotes itself so we don't waste tokens
//! re-synthesizing it.
//!
//! ## Cost
//!
//! Zero LLM. Pure SQL + a `memory_versions`-free metadata UPDATE per
//! page that actually changes tier. Caller (ProactiveService tick)
//! gates frequency — typical cadence ~every 240 ticks (≈ 2h).
//!
//! ## Daily upgrade cap
//!
//! Upgrades are capped (`daily_upgrade_cap`, default 10) to bound the
//! token cost of downstream `EntitySynthesizer` invocations they
//! trigger. We track today's upgrade count by counting
//! `cost_records.model = 'memory_tier:upgrade'` rows since UTC
//! midnight — same shape as Phase 5's `memory_lint%` cost guard, so
//! the rollup queries stay uniform.
//!
//! Downgrades are NOT capped: they're free signals and they SAVE
//! token spend on the next synthesis pass.

use rusqlite::params;
use serde::Serialize;
use std::sync::Arc;

use crate::memory_graph::entity_page::EntityPageMetadata;
use crate::memory_graph::store::MemoryGraphStore;

// ─── Config + outcome ──────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct TierEscalatorConfig {
    /// Inclusive lower bound for Tier 2.
    pub tier_2_min_backlinks: i64,
    /// Inclusive lower bound for Tier 1.
    pub tier_1_min_backlinks: i64,
    /// Max number of upgrades per UTC day across all spaces.
    pub daily_upgrade_cap: u32,
    /// Hard ceiling on rows examined per scan. Protects against
    /// runaway scans on misconfigured DBs.
    pub max_pages_per_scan: usize,
}

impl Default for TierEscalatorConfig {
    fn default() -> Self {
        Self {
            tier_2_min_backlinks: 3,
            tier_1_min_backlinks: 8,
            daily_upgrade_cap: 10,
            max_pages_per_scan: 200,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct TierEscalatorOutcome {
    /// Number of pages whose tier number decreased (more important).
    pub upgraded: u32,
    /// Number of pages whose tier number increased (less important).
    pub downgraded: u32,
    /// Pages whose computed tier matched the existing tier.
    pub unchanged: u32,
    /// Pages that *would* have upgraded but we hit the daily cap.
    pub skipped_due_to_cap: u32,
    /// True if the run terminated upgrades early because the cap was
    /// hit (informational; the UI uses this to show "daily cap reached").
    pub daily_cap_hit: bool,
    /// Total entity_page rows examined (for telemetry).
    pub pages_scanned: u32,
}

// ─── Pure tier-decision helper ─────────────────────────────────────────

/// Map backlink count to target tier per the spec table. Pure function
/// so the threshold logic is testable in isolation from SQL.
///
/// Returns `1` / `2` / `3` (NOT zero). Zero is reserved in
/// [`EntityPageMetadata::enrichment_tier`] for "field absent on legacy
/// rows" and is normalised to 3 in [`compute_target_tier_for_existing`].
pub fn target_tier_for_backlinks(backlinks: i64, cfg: &TierEscalatorConfig) -> u8 {
    if backlinks >= cfg.tier_1_min_backlinks {
        1
    } else if backlinks >= cfg.tier_2_min_backlinks {
        2
    } else {
        3
    }
}

/// What target tier should the page have, given its current (possibly
/// missing) `enrichment_tier`? Equivalent to [`target_tier_for_backlinks`]
/// — the existing-tier argument is just here so callers don't have to
/// pass the same context twice. Kept as a separate helper for symmetry
/// with the test suite, which inspects both arms directly.
pub fn compute_target_tier_for_existing(
    backlinks: i64,
    _current_tier_or_3: u8,
    cfg: &TierEscalatorConfig,
) -> u8 {
    target_tier_for_backlinks(backlinks, cfg)
}

/// Classifier for the (current, target) tier pair. Used by the scan
/// loop to decide whether to update + count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierTransition {
    /// Target tier number is lower than current → page got more
    /// important.
    Upgrade,
    /// Target tier number is higher than current → page got less
    /// important.
    Downgrade,
    /// Already at target.
    NoChange,
}

pub fn classify_transition(current: u8, target: u8) -> TierTransition {
    use std::cmp::Ordering;
    match target.cmp(&current) {
        Ordering::Less => TierTransition::Upgrade,
        Ordering::Greater => TierTransition::Downgrade,
        Ordering::Equal => TierTransition::NoChange,
    }
}

// ─── Orchestrator ──────────────────────────────────────────────────────

/// Scan every EntityPage in `space_id`, recompute tier from backlink
/// count, and write any deltas back into `memory_nodes.metadata_json`.
///
/// `today_upgrades_so_far` is the count of upgrades already recorded
/// today for this space (caller-supplied so the orchestrator stays
/// testable without faking time). When this exceeds
/// `cfg.daily_upgrade_cap`, further upgrades are suppressed; downgrades
/// always proceed.
pub fn run_tier_escalator(
    store: Arc<MemoryGraphStore>,
    db: Arc<std::sync::Mutex<rusqlite::Connection>>,
    space_id: &str,
    cfg: &TierEscalatorConfig,
    today_upgrades_so_far: u32,
) -> Result<TierEscalatorOutcome, crate::error::Error> {
    let mut outcome = TierEscalatorOutcome::default();
    let mut upgrades_done: u32 = today_upgrades_so_far;

    // (id, current_metadata_json, backlink_count) tuples.
    let candidates = fetch_candidates(&store, space_id, cfg.max_pages_per_scan)?;
    outcome.pages_scanned = candidates.len() as u32;

    for (node_id, raw_metadata, backlinks) in candidates {
        let value: serde_json::Value = raw_metadata
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or(serde_json::Value::Null);
        let mut meta = EntityPageMetadata::from_value(&value);
        let current_tier = meta.enrichment_tier.unwrap_or(3);
        let target_tier = compute_target_tier_for_existing(backlinks, current_tier, cfg);
        let transition = classify_transition(current_tier, target_tier);

        match transition {
            TierTransition::NoChange => {
                outcome.unchanged += 1;
                continue;
            }
            TierTransition::Upgrade => {
                if upgrades_done >= cfg.daily_upgrade_cap {
                    outcome.skipped_due_to_cap += 1;
                    outcome.daily_cap_hit = true;
                    continue;
                }
                upgrades_done += 1;
                outcome.upgraded += 1;
                record_upgrade_cost(&db, current_tier, target_tier);
            }
            TierTransition::Downgrade => {
                outcome.downgraded += 1;
            }
        }

        meta.enrichment_tier = Some(target_tier);
        meta.last_escalated_at = Some(chrono::Utc::now().to_rfc3339());
        if let Err(e) = persist_metadata(&store, &node_id, &meta) {
            tracing::warn!(
                "tier_escalator: failed to persist new metadata for {}: {}",
                node_id,
                e
            );
            // Roll back the cost row? No — best-effort and the next
            // scan will retry anyway.
        }
    }

    Ok(outcome)
}

/// Sum today's `cost_records.model = 'memory_tier:upgrade'` rows to
/// recover the daily upgrade tally for the cap check.
pub fn count_todays_upgrades(
    db: &std::sync::Mutex<rusqlite::Connection>,
) -> Result<u32, crate::error::Error> {
    let today_start_ms = {
        use chrono::{Datelike, TimeZone, Utc};
        let now = Utc::now();
        Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
            .single()
            .map(|dt| dt.timestamp_millis())
            .unwrap_or(0)
    };
    let conn = db
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM cost_records \
             WHERE model LIKE 'memory_tier:upgrade%' AND created_at >= ?1",
            params![today_start_ms],
            |r| r.get(0),
        )
        .unwrap_or(0);
    Ok(count.max(0) as u32)
}

// ─── Implementation helpers ────────────────────────────────────────────

fn fetch_candidates(
    store: &MemoryGraphStore,
    space_id: &str,
    limit: usize,
) -> Result<Vec<(String, Option<String>, i64)>, crate::error::Error> {
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    let mut stmt = conn
        .prepare(
            "SELECT n.id, n.metadata_json, \
                    COALESCE(\
                        (SELECT COUNT(*) FROM memory_edges \
                         WHERE child_node_id = n.id), \
                        0\
                    ) AS backlinks \
             FROM memory_nodes n \
             WHERE n.space_id = ?1 AND n.kind = 'entity_page' \
             ORDER BY backlinks DESC, n.updated_at DESC \
             LIMIT ?2",
        )
        .map_err(crate::error::Error::Database)?;
    let rows = stmt
        .query_map(params![space_id, limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })
        .map_err(crate::error::Error::Database)?;
    Ok(rows.flatten().collect())
}

fn persist_metadata(
    store: &MemoryGraphStore,
    node_id: &str,
    meta: &EntityPageMetadata,
) -> Result<(), crate::error::Error> {
    let new_json = serde_json::to_string(&meta.to_value()).map_err(crate::error::Error::Serde)?;
    let now = chrono::Utc::now().to_rfc3339();
    let conn = store
        .conn
        .lock()
        .map_err(|e| crate::error::Error::Internal(format!("DB lock: {}", e)))?;
    conn.execute(
        "UPDATE memory_nodes SET metadata_json = ?1, updated_at = ?2 WHERE id = ?3",
        params![new_json, now, node_id],
    )
    .map_err(crate::error::Error::Database)?;
    Ok(())
}

/// Insert a zero-cost row into `cost_records` so the daily upgrade
/// tally can be reconstructed by `count_todays_upgrades`. Best-effort:
/// errors logged, swallowed.
fn record_upgrade_cost(db: &std::sync::Mutex<rusqlite::Connection>, from_tier: u8, to_tier: u8) {
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let model = format!("memory_tier:upgrade_{}_to_{}", from_tier, to_tier);
    let conn = match db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("tier_escalator: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, 'memory_os', ?2, 0, 0, 0.0, ?3)",
        params![id, model, now],
    ) {
        tracing::warn!("tier_escalator: INSERT cost row failed: {}", e);
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::entity_page::EntityPageMetadata;
    use crate::memory_graph::store::MemoryGraphStore;
    use rusqlite::Connection;
    use std::sync::Mutex;

    fn fresh_store() -> Arc<MemoryGraphStore> {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(crate::db::migrations::V4_MEMORY_GRAPH).unwrap();
        conn.execute_batch(crate::db::migrations::V13_COST_RECORDS).unwrap();
        conn.execute_batch(crate::db::migrations::V35_MEMORY_OS_PHASE_1).unwrap();
        conn.execute_batch("PRAGMA foreign_keys = ON;").ok();
        Arc::new(MemoryGraphStore::new(Arc::new(Mutex::new(conn))))
    }

    fn db_handle(store: &MemoryGraphStore) -> Arc<Mutex<Connection>> {
        store.conn.clone()
    }

    fn insert_entity_page(
        store: &MemoryGraphStore,
        id: &str,
        title: &str,
        meta: EntityPageMetadata,
    ) {
        let now = chrono::Utc::now().to_rfc3339();
        let meta_json = serde_json::to_string(&meta.to_value()).unwrap();
        let conn = store.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO memory_nodes (id, space_id, kind, title, metadata_json, created_at, updated_at) \
             VALUES (?1, 'default', 'entity_page', ?2, ?3, ?4, ?4)",
            params![id, title, meta_json, now],
        )
        .unwrap();
    }

    fn insert_backlinks(store: &MemoryGraphStore, target: &str, n: usize) {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = store.conn.lock().unwrap();
        // The referrers don't need to be entity_pages — any node will do.
        for i in 0..n {
            let parent_id = format!("ep-{}-{}", target, i);
            conn.execute(
                "INSERT INTO memory_nodes (id, space_id, kind, title, created_at, updated_at) \
                 VALUES (?1, 'default', 'episode', 'parent', ?2, ?2)",
                params![parent_id, now],
            )
            .unwrap();
            conn.execute(
                "INSERT INTO memory_edges \
                 (id, space_id, parent_node_id, child_node_id, relation_kind, visibility, priority, created_at, updated_at) \
                 VALUES (?1, 'default', ?2, ?3, 'mentions', 'private', 0, ?4, ?4)",
                params![format!("e-{}-{}", target, i), parent_id, target, now],
            )
            .unwrap();
        }
    }

    fn read_tier(store: &MemoryGraphStore, node_id: &str) -> Option<u8> {
        let conn = store.conn.lock().unwrap();
        let raw: Option<String> = conn
            .query_row(
                "SELECT metadata_json FROM memory_nodes WHERE id = ?1",
                params![node_id],
                |r| r.get(0),
            )
            .ok()
            .flatten();
        raw.as_deref()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(s).ok())
            .map(|v| EntityPageMetadata::from_value(&v))
            .and_then(|m| m.enrichment_tier)
    }

    // ─── Pure threshold logic ──────────────────────────────────────

    #[test]
    fn target_tier_for_backlinks_classifies_correctly() {
        let cfg = TierEscalatorConfig::default();
        // Default thresholds: T2≥3, T1≥8
        assert_eq!(target_tier_for_backlinks(0, &cfg), 3);
        assert_eq!(target_tier_for_backlinks(1, &cfg), 3);
        assert_eq!(target_tier_for_backlinks(2, &cfg), 3);
        assert_eq!(target_tier_for_backlinks(3, &cfg), 2);
        assert_eq!(target_tier_for_backlinks(5, &cfg), 2);
        assert_eq!(target_tier_for_backlinks(7, &cfg), 2);
        assert_eq!(target_tier_for_backlinks(8, &cfg), 1);
        assert_eq!(target_tier_for_backlinks(100, &cfg), 1);
    }

    #[test]
    fn classify_transition_covers_three_cases() {
        // Tier 3 (stub) → Tier 2 (rich) = upgrade
        assert_eq!(classify_transition(3, 2), TierTransition::Upgrade);
        // Tier 2 → Tier 1 = upgrade
        assert_eq!(classify_transition(2, 1), TierTransition::Upgrade);
        // Tier 1 → Tier 2 = downgrade
        assert_eq!(classify_transition(1, 2), TierTransition::Downgrade);
        // Tier 2 → Tier 3 = downgrade
        assert_eq!(classify_transition(2, 3), TierTransition::Downgrade);
        // Same = no change
        assert_eq!(classify_transition(3, 3), TierTransition::NoChange);
        assert_eq!(classify_transition(1, 1), TierTransition::NoChange);
    }

    // ─── End-to-end SQL scan ──────────────────────────────────────

    #[test]
    fn run_tier_escalator_upgrades_stub_to_rich() {
        let store = fresh_store();
        let db = db_handle(&store);
        // Page starts as None tier (legacy / fresh-create row).
        insert_entity_page(&store, "alice", "Alice", EntityPageMetadata::default());
        insert_backlinks(&store, "alice", 4); // → Tier 2

        let outcome = run_tier_escalator(
            store.clone(),
            db,
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        assert_eq!(outcome.pages_scanned, 1);
        assert_eq!(outcome.upgraded, 1);
        assert_eq!(outcome.downgraded, 0);
        assert_eq!(outcome.unchanged, 0);
        assert_eq!(read_tier(&store, "alice"), Some(2));
    }

    #[test]
    fn run_tier_escalator_upgrades_to_tier_1_at_threshold() {
        let store = fresh_store();
        let db = db_handle(&store);
        insert_entity_page(&store, "hub", "Hub", EntityPageMetadata::default());
        insert_backlinks(&store, "hub", 8); // exactly at the Tier 1 threshold

        let outcome = run_tier_escalator(
            store.clone(),
            db,
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        assert_eq!(outcome.upgraded, 1);
        assert_eq!(read_tier(&store, "hub"), Some(1));
    }

    #[test]
    fn run_tier_escalator_downgrades_when_backlinks_drop() {
        let store = fresh_store();
        let db = db_handle(&store);
        // Previously a Tier 1 page; backlinks have since fallen to 2.
        let mut meta = EntityPageMetadata::default();
        meta.enrichment_tier = Some(1);
        insert_entity_page(&store, "fallen", "Fallen", meta);
        insert_backlinks(&store, "fallen", 2); // → Tier 3

        let outcome = run_tier_escalator(
            store.clone(),
            db,
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        assert_eq!(outcome.downgraded, 1);
        assert_eq!(outcome.upgraded, 0);
        assert_eq!(read_tier(&store, "fallen"), Some(3));
    }

    #[test]
    fn run_tier_escalator_leaves_unchanged_pages_alone() {
        let store = fresh_store();
        let db = db_handle(&store);
        let mut meta = EntityPageMetadata::default();
        meta.enrichment_tier = Some(2);
        insert_entity_page(&store, "stable", "Stable", meta);
        insert_backlinks(&store, "stable", 4); // Tier 2 already → no change

        let outcome = run_tier_escalator(
            store.clone(),
            db,
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        assert_eq!(outcome.unchanged, 1);
        assert_eq!(outcome.upgraded, 0);
        assert_eq!(outcome.downgraded, 0);
        // last_escalated_at should NOT have been written for no-change.
        let conn = store.conn.lock().unwrap();
        let meta_raw: String = conn
            .query_row(
                "SELECT metadata_json FROM memory_nodes WHERE id = 'stable'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let v: serde_json::Value = serde_json::from_str(&meta_raw).unwrap();
        let m = EntityPageMetadata::from_value(&v);
        assert!(m.last_escalated_at.is_none(), "unchanged should NOT bump last_escalated_at");
    }

    #[test]
    fn run_tier_escalator_respects_daily_upgrade_cap() {
        let store = fresh_store();
        let db = db_handle(&store);
        // 3 pages each warranting an upgrade.
        for (id, n) in &[("a", 4), ("b", 4), ("c", 4)] {
            insert_entity_page(&store, id, id, EntityPageMetadata::default());
            insert_backlinks(&store, id, *n);
        }
        let cfg = TierEscalatorConfig {
            daily_upgrade_cap: 2, // cap at 2 — third should be skipped
            ..Default::default()
        };
        let outcome = run_tier_escalator(store.clone(), db, "default", &cfg, 0).unwrap();
        assert_eq!(outcome.upgraded, 2);
        assert_eq!(outcome.skipped_due_to_cap, 1);
        assert!(outcome.daily_cap_hit);
    }

    #[test]
    fn run_tier_escalator_caps_account_for_today_already_spent() {
        let store = fresh_store();
        let db = db_handle(&store);
        insert_entity_page(&store, "alice", "Alice", EntityPageMetadata::default());
        insert_backlinks(&store, "alice", 4);

        let cfg = TierEscalatorConfig {
            daily_upgrade_cap: 5,
            ..Default::default()
        };
        // Caller says we've ALREADY spent 5 today → cap reached.
        let outcome = run_tier_escalator(store.clone(), db, "default", &cfg, 5).unwrap();
        assert_eq!(outcome.upgraded, 0);
        assert_eq!(outcome.skipped_due_to_cap, 1);
        assert!(outcome.daily_cap_hit);
        // Page tier must remain unchanged (still None == treated as Tier 3).
        assert_eq!(read_tier(&store, "alice"), None);
    }

    #[test]
    fn run_tier_escalator_downgrades_uncapped() {
        // Downgrades should bypass the cap entirely.
        let store = fresh_store();
        let db = db_handle(&store);
        let mut meta = EntityPageMetadata::default();
        meta.enrichment_tier = Some(1);
        insert_entity_page(&store, "fallen", "Fallen", meta);
        // No backlinks → target Tier 3.

        let cfg = TierEscalatorConfig {
            daily_upgrade_cap: 0, // cap is zero — but downgrade should still happen
            ..Default::default()
        };
        let outcome = run_tier_escalator(store.clone(), db, "default", &cfg, 0).unwrap();
        assert_eq!(outcome.downgraded, 1);
        assert_eq!(outcome.upgraded, 0);
        assert_eq!(outcome.skipped_due_to_cap, 0);
        assert!(!outcome.daily_cap_hit);
        assert_eq!(read_tier(&store, "fallen"), Some(3));
    }

    #[test]
    fn run_tier_escalator_writes_cost_row_for_upgrades_only() {
        let store = fresh_store();
        let db = db_handle(&store);
        insert_entity_page(&store, "up", "Up", EntityPageMetadata::default());
        insert_backlinks(&store, "up", 4); // → upgrade

        let mut meta = EntityPageMetadata::default();
        meta.enrichment_tier = Some(1);
        insert_entity_page(&store, "down", "Down", meta);
        // no backlinks for "down" → downgrade

        let _ = run_tier_escalator(
            store.clone(),
            db.clone(),
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        let conn = db.lock().unwrap();
        let upgrade_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM cost_records WHERE model LIKE 'memory_tier:upgrade%'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        let total_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM cost_records", [], |r| r.get(0))
            .unwrap();
        assert_eq!(upgrade_count, 1, "exactly one upgrade row");
        assert_eq!(total_count, 1, "no row for the downgrade");
    }

    #[test]
    fn count_todays_upgrades_reads_back_the_cost_rows() {
        let store = fresh_store();
        let db = db_handle(&store);
        insert_entity_page(&store, "a", "A", EntityPageMetadata::default());
        insert_entity_page(&store, "b", "B", EntityPageMetadata::default());
        insert_backlinks(&store, "a", 4);
        insert_backlinks(&store, "b", 9);

        let _ = run_tier_escalator(
            store.clone(),
            db.clone(),
            "default",
            &TierEscalatorConfig::default(),
            0,
        )
        .unwrap();

        let n = count_todays_upgrades(&db).unwrap();
        assert_eq!(n, 2, "two upgrades should round-trip into today's count");
    }
}
