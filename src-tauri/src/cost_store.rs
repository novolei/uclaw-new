//! Write-side persistence for per-turn LLM cost records.
//!
//! Called from the agent dispatcher's `emit_turn_cost` to capture usage
//! synchronously alongside the IPC event. No frontend dependency — events
//! are fire-and-forget from the listener's POV; persistence is the source
//! of truth for the dashboard.

use crate::app::AppState;
use crate::agent::types::calculate_cost;
use rusqlite::params;

/// Insert one cost record without prompt-cache detail (cache semantics
/// unknown → `input_includes_cache` NULL). Errors are logged and swallowed —
/// cost capture is best-effort and must never fail the agent loop.
pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32) {
    record_usage(state, session_id, model, input_tokens, output_tokens, 0, 0, None);
}

/// Insert one cost record including prompt-cache reads
/// (`cost_records.cached_input_tokens`, V49), prompt-cache writes
/// (`cost_records.cache_creation_tokens`, V67) and the wire protocol's cache
/// semantics (`cost_records.input_includes_cache`, V67): `Some(false)` for
/// the Anthropic Messages wire (input excludes cache reads), `Some(true)` for
/// OpenAI-compatible wires (prompt tokens include them), `None` = unknown.
/// Best-effort like [`record`].
pub fn record_usage(
    state: &AppState,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
    cache_creation_tokens: u32,
    input_includes_cache: Option<bool>,
) {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cost_store: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = insert_usage(
        &conn,
        session_id,
        model,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_creation_tokens,
        input_includes_cache,
    ) {
        tracing::warn!("cost_store: INSERT failed: {}", e);
    }
}

/// Testable core of [`record_usage`]: one `cost_records` row stamped now.
#[allow(clippy::too_many_arguments)]
pub(crate) fn insert_usage(
    conn: &rusqlite::Connection,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
    cache_creation_tokens: u32,
    input_includes_cache: Option<bool>,
) -> rusqlite::Result<()> {
    let now = chrono::Utc::now().timestamp_millis();
    insert_usage_at(
        conn,
        session_id,
        model,
        input_tokens,
        output_tokens,
        cached_input_tokens,
        cache_creation_tokens,
        input_includes_cache,
        now,
    )
}

#[allow(clippy::too_many_arguments)]
fn insert_usage_at(
    conn: &rusqlite::Connection,
    session_id: &str,
    model: &str,
    input_tokens: u32,
    output_tokens: u32,
    cached_input_tokens: u32,
    cache_creation_tokens: u32,
    input_includes_cache: Option<bool>,
    created_at: i64,
) -> rusqlite::Result<()> {
    let cost = calculate_cost(model, input_tokens, output_tokens);
    let id = uuid::Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at,
                                   cached_input_tokens, input_includes_cache, cache_creation_tokens)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id,
            session_id,
            model,
            input_tokens as i64,
            output_tokens as i64,
            cost,
            created_at,
            cached_input_tokens as i64,
            input_includes_cache,
            cache_creation_tokens as i64
        ],
    )?;
    Ok(())
}

// ─── Prompt-cache hit rate (context continuity P3, Decision 18 amended) ─

/// Whether a call's input/prompt token count already includes cache reads.
/// The wire protocol recorded at write time decides (`Some`); legacy rows
/// and unknown providers (`None`) fall back to the model-name heuristic
/// (`claude*` → Anthropic semantics, input excludes cache).
fn includes_cache(input_includes_cache: Option<bool>, model: &str) -> bool {
    input_includes_cache.unwrap_or_else(|| !model.trim().to_ascii_lowercase().starts_with("claude"))
}

/// Total prompt tokens of one call, cache traffic included: `input` when the
/// wire's input already includes cache reads (OpenAI-compatible wires report
/// no separate cache writes), else `input + cache reads + cache writes` (the
/// Anthropic Messages wire bills the three disjointly). Legacy rows carry 0
/// cache writes, so they keep the `input + cached` fallback.
pub fn prompt_tokens_total(
    input_includes_cache: Option<bool>,
    model: &str,
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_tokens: u64,
) -> u64 {
    if includes_cache(input_includes_cache, model) {
        input_tokens
    } else {
        input_tokens
            .saturating_add(cached_input_tokens)
            .saturating_add(cache_creation_tokens)
    }
}

/// Prompt-cache hit ratio in `0.0..=1.0`: `cached / prompt_tokens_total`.
/// Zero denominator → 0.0.
pub fn cache_hit_ratio(
    input_includes_cache: Option<bool>,
    model: &str,
    input_tokens: u64,
    cached_input_tokens: u64,
    cache_creation_tokens: u64,
) -> f64 {
    ratio(
        cached_input_tokens,
        prompt_tokens_total(
            input_includes_cache,
            model,
            input_tokens,
            cached_input_tokens,
            cache_creation_tokens,
        ),
    )
}

fn ratio(cached: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (cached as f64 / total as f64).clamp(0.0, 1.0)
    }
}

fn nonneg(v: i64) -> u64 {
    v.max(0) as u64
}

/// One session's prompt-cache totals: every `cost_records` row of the
/// session, each (model, wire) group normalized with [`prompt_tokens_total`].
pub fn session_cache_stats(
    conn: &rusqlite::Connection,
    session_id: &str,
) -> rusqlite::Result<crate::ipc::SessionCacheStats> {
    let mut stmt = conn.prepare(
        "SELECT model, input_includes_cache, SUM(input_tokens), SUM(cached_input_tokens), COUNT(*),
                COALESCE(SUM(cache_creation_tokens), 0)
         FROM cost_records WHERE session_id = ?1
         GROUP BY model, input_includes_cache",
    )?;
    let rows = stmt.query_map(params![session_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, Option<bool>>(1)?,
            r.get::<_, i64>(2)?,
            r.get::<_, i64>(3)?,
            r.get::<_, i64>(4)?,
            r.get::<_, i64>(5)?,
        ))
    })?;
    let (mut calls, mut total, mut cached) = (0u64, 0u64, 0u64);
    for row in rows {
        let (model, wire, input, cache, n, creation) = row?;
        calls += nonneg(n);
        total += prompt_tokens_total(wire, &model, nonneg(input), nonneg(cache), nonneg(creation));
        cached += nonneg(cache);
    }
    Ok(crate::ipc::SessionCacheStats {
        calls: calls.min(u32::MAX as u64) as u32,
        prompt_tokens_total: total,
        cached_input_tokens: cached,
        hit_ratio: ratio(cached, total),
    })
}

/// One `(group, model, wire)` aggregate row of a rollup query.
struct RollupRow {
    group: String,
    model: String,
    wire: Option<bool>,
    input: i64,
    output: i64,
    cost: f64,
    turns: i64,
    cached: i64,
    creation: i64,
}

impl RollupRow {
    fn prompt_total(&self) -> i64 {
        prompt_tokens_total(
            self.wire,
            &self.model,
            nonneg(self.input),
            nonneg(self.cached),
            nonneg(self.creation),
        ) as i64
    }
}

/// Aggregate `cost_records` since `cutoff_ms` by `group_expr` + model + wire.
fn rollup_rows(conn: &rusqlite::Connection, group_expr: &str, cutoff_ms: i64) -> rusqlite::Result<Vec<RollupRow>> {
    let sql = format!(
        "SELECT {group_expr} AS grp, model, input_includes_cache,
                SUM(input_tokens), SUM(output_tokens), COALESCE(SUM(cost_usd), 0),
                COUNT(*), SUM(cached_input_tokens), COALESCE(SUM(cache_creation_tokens), 0)
         FROM cost_records
         WHERE created_at >= ?1
         GROUP BY grp, model, input_includes_cache"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![cutoff_ms], |r| {
        Ok(RollupRow {
            group: r.get(0)?,
            model: r.get(1)?,
            wire: r.get(2)?,
            input: r.get(3)?,
            output: r.get(4)?,
            cost: r.get(5)?,
            turns: r.get(6)?,
            cached: r.get(7)?,
            creation: r.get(8)?,
        })
    })?;
    rows.collect()
}

/// Per-day (UTC `YYYY-MM-DD`) cost rollup since `cutoff_ms`, ascending.
/// `input_tokens` is the raw column sum; `prompt_tokens_total` normalizes
/// each (model, wire) group; `hit_ratio = cached / prompt_tokens_total`.
pub fn daily_rollups(
    conn: &rusqlite::Connection,
    cutoff_ms: i64,
) -> rusqlite::Result<Vec<crate::ipc::DailyCostRollup>> {
    let rows = rollup_rows(conn, "strftime('%Y-%m-%d', created_at / 1000, 'unixepoch')", cutoff_ms)?;
    let mut days: std::collections::BTreeMap<String, crate::ipc::DailyCostRollup> = Default::default();
    for row in rows {
        let prompt_total = row.prompt_total();
        let d = days.entry(row.group.clone()).or_insert_with(|| crate::ipc::DailyCostRollup {
            day: row.group.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            turn_count: 0,
            cached_input_tokens: 0,
            prompt_tokens_total: 0,
            hit_ratio: 0.0,
        });
        d.input_tokens += row.input;
        d.output_tokens += row.output;
        d.cost_usd += row.cost;
        d.turn_count += row.turns;
        d.cached_input_tokens += row.cached;
        d.prompt_tokens_total += prompt_total;
    }
    Ok(days
        .into_values()
        .map(|mut d| {
            d.hit_ratio = ratio(nonneg(d.cached_input_tokens), nonneg(d.prompt_tokens_total));
            d
        })
        .collect())
}

/// Per-model cost rollup since `cutoff_ms`, most expensive first. Same token
/// semantics as [`daily_rollups`].
pub fn model_rollups(
    conn: &rusqlite::Connection,
    cutoff_ms: i64,
) -> rusqlite::Result<Vec<crate::ipc::ModelCostRollup>> {
    let rows = rollup_rows(conn, "model", cutoff_ms)?;
    let mut models: std::collections::BTreeMap<String, crate::ipc::ModelCostRollup> = Default::default();
    for row in rows {
        let prompt_total = row.prompt_total();
        let m = models.entry(row.model.clone()).or_insert_with(|| crate::ipc::ModelCostRollup {
            model: row.model.clone(),
            input_tokens: 0,
            output_tokens: 0,
            cost_usd: 0.0,
            turn_count: 0,
            cached_input_tokens: 0,
            prompt_tokens_total: 0,
            hit_ratio: 0.0,
        });
        m.input_tokens += row.input;
        m.output_tokens += row.output;
        m.cost_usd += row.cost;
        m.turn_count += row.turns;
        m.cached_input_tokens += row.cached;
        m.prompt_tokens_total += prompt_total;
    }
    let mut out: Vec<_> = models
        .into_values()
        .map(|mut m| {
            m.hit_ratio = ratio(nonneg(m.cached_input_tokens), nonneg(m.prompt_tokens_total));
            m
        })
        .collect();
    out.sort_by(|a, b| b.cost_usd.partial_cmp(&a.cost_usd).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// SUM(cost_usd) for cost_records with created_at >= since_ms.
/// Returns 0.0 on any error (best-effort, matches the rest of this module).
pub fn monthly_total(state: &AppState, since_ms: i64) -> f64 {
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(_) => return 0.0,
    };
    conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
        params![since_ms],
        |row| row.get::<_, f64>(0),
    ).unwrap_or(0.0)
}

/// Compute the start of the current month (UTC) in epoch ms.
pub fn current_month_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Compute the start of the current day (UTC) in epoch ms.
/// Used by per-day spend caps (Sprint 2.1b learning, Phase 5 lint).
pub fn current_day_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// Memory OS Sprint 2.1b — SUM(input_tokens + output_tokens) for today's
/// cost_records where `model LIKE 'memory_learning%'`. Used by the chat-turn
/// extractor's daily-budget gate before invoking the LLM layer. Returns 0
/// on any error (best-effort; falling back to 0 lets the producer continue
/// running rather than blocking on transient DB issues).
///
/// Standalone function (not on AppState) so the agent dispatcher can call
/// it given only the raw `Arc<Mutex<Connection>>` — `set_learning_pipeline`
/// hands it the same db handle AppState owns.
pub fn today_learning_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'memory_learning%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// T4 memory growth — today's tokens spent by growth passes (cost rows with
/// `model LIKE 'memory_growth%'`). Returns 0 on any error.
pub fn today_growth_tokens(db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'memory_growth%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Sprint 2.4b — sibling of [`today_learning_tokens`] for the gbrain
/// chat-turn extractor's daily-budget gate. Sums today's `cost_records`
/// where `model LIKE 'gbrain_extract%'` (the prefix
/// `crate::memory_graph::chat_extractor` writes via the `"gbrain_extract"`
/// cost_tag passed to `MemoryOsLlm::complete_text`).
///
/// Standalone fn (not on AppState) so the agent dispatcher can call it
/// given only the raw `Arc<Mutex<Connection>>` — matches the shape
/// `set_gbrain_extractor_pipeline` hands the dispatcher.
pub fn today_gbrain_extract_tokens(
    db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
) -> u32 {
    let since_ms = current_day_start_ms();
    let conn = match db.lock() {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let n: i64 = conn
        .query_row(
            "SELECT COALESCE(SUM(input_tokens + output_tokens), 0)
             FROM cost_records
             WHERE model LIKE 'gbrain_extract%' AND created_at >= ?1",
            params![since_ms],
            |row| row.get::<_, i64>(0),
        )
        .unwrap_or(0);
    n.max(0) as u32
}

/// Pure helper: which threshold (if any) was crossed on this turn?
/// Fires 100 OR 80 (preferring 100 when both crossed in one turn), or None.
/// Never fires when budget <= 0.0.
pub fn fired_threshold(total_before: f64, total_after: f64, budget: f64) -> Option<u8> {
    if budget <= 0.0 { return None; }
    let crossed_100 = total_before / budget < 1.00 && total_after / budget >= 1.00;
    let crossed_80 = total_before / budget < 0.80 && total_after / budget >= 0.80;
    if crossed_100 { Some(100) }
    else if crossed_80 { Some(80) }
    else { None }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_threshold_when_under_80() {
        assert_eq!(fired_threshold(10.0, 50.0, 100.0), None);
    }
    #[test]
    fn fires_80_when_crossing_upward() {
        assert_eq!(fired_threshold(75.0, 85.0, 100.0), Some(80));
    }
    #[test]
    fn does_not_refire_80_when_already_above() {
        assert_eq!(fired_threshold(85.0, 90.0, 100.0), None);
    }
    #[test]
    fn fires_100_when_crossing_upward() {
        assert_eq!(fired_threshold(95.0, 105.0, 100.0), Some(100));
    }
    #[test]
    fn fires_100_not_80_when_crossing_both_at_once() {
        assert_eq!(fired_threshold(50.0, 150.0, 100.0), Some(100));
    }
    #[test]
    fn no_fire_when_budget_zero_or_negative() {
        assert_eq!(fired_threshold(50.0, 150.0, 0.0), None);
        assert_eq!(fired_threshold(50.0, 150.0, -10.0), None);
    }

    // ─── Sprint 2.4b — gbrain extractor budget query ─────────────

    fn open_test_db_with_cost_records() -> std::sync::Arc<std::sync::Mutex<rusqlite::Connection>> {
        use std::sync::{Arc, Mutex};
        let conn = rusqlite::Connection::open_in_memory().unwrap();
        // Minimal V13 cost_records schema — just enough to exercise the
        // SUM-by-prefix queries. Real schema has more columns but they
        // don't affect this test.
        conn.execute_batch(
            "CREATE TABLE cost_records (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 model TEXT NOT NULL,
                 input_tokens INTEGER NOT NULL,
                 output_tokens INTEGER NOT NULL,
                 cost_usd REAL,
                 created_at INTEGER NOT NULL,
                 cached_input_tokens INTEGER NOT NULL DEFAULT 0,
                 input_includes_cache INTEGER,
                 cache_creation_tokens INTEGER NOT NULL DEFAULT 0
             );",
        )
        .unwrap();
        Arc::new(Mutex::new(conn))
    }

    fn insert_cost(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        model: &str,
        input: i64,
        output: i64,
        at_ms: i64,
    ) {
        let conn = db.lock().unwrap();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, 'test', ?2, ?3, ?4, 0.0, ?5)",
            params![uuid::Uuid::new_v4().to_string(), model, input, output, at_ms],
        )
        .unwrap();
    }

    #[test]
    fn today_gbrain_extract_tokens_sums_only_gbrain_prefix() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        // Two gbrain rows today → should sum to 350
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 100, 50, now);
        insert_cost(&db, "gbrain_extract:other-model", 150, 50, now);
        // Memory_learning row today — must NOT count toward gbrain total
        insert_cost(&db, "memory_learning:claude-haiku-4-5", 999, 999, now);
        // Gbrain row from yesterday — must NOT count
        let yesterday = now - 26 * 60 * 60 * 1000;
        insert_cost(&db, "gbrain_extract:claude-haiku-4-5", 500, 500, yesterday);

        assert_eq!(today_gbrain_extract_tokens(&db), 350);
    }

    #[test]
    fn today_gbrain_extract_tokens_returns_zero_when_no_rows() {
        let db = open_test_db_with_cost_records();
        assert_eq!(today_gbrain_extract_tokens(&db), 0);
    }

    // ─── Context continuity P3 Task 11 — prompt-cache observability ─────
    // Cache semantics are classified by WIRE PROTOCOL recorded at write time
    // (`input_includes_cache`): Anthropic Messages → false, OpenAI-compatible
    // → true; NULL (legacy rows / unknown provider) → model-name heuristic.

    const ANTHROPIC_WIRE: Option<bool> = Some(false);
    const OPENAI_WIRE: Option<bool> = Some(true);

    fn insert_cached(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        session: &str,
        model: &str,
        input: u32,
        cached: u32,
        wire: Option<bool>,
        at_ms: i64,
    ) {
        insert_cached_w(db, session, model, input, cached, 0, wire, at_ms);
    }

    #[allow(clippy::too_many_arguments)]
    fn insert_cached_w(
        db: &std::sync::Arc<std::sync::Mutex<rusqlite::Connection>>,
        session: &str,
        model: &str,
        input: u32,
        cached: u32,
        creation: u32,
        wire: Option<bool>,
        at_ms: i64,
    ) {
        let conn = db.lock().unwrap();
        insert_usage_at(&conn, session, model, input, 10, cached, creation, wire, at_ms).unwrap();
    }

    #[test]
    fn record_usage_writes_cached_input_tokens() {
        let db = open_test_db_with_cost_records();
        let conn = db.lock().unwrap();
        insert_usage(&conn, "s1", "claude-sonnet-4-5", 600, 50, 400, 0, ANTHROPIC_WIRE).unwrap();
        insert_usage(&conn, "s2", "mock", 1, 1, 0, 0, None).unwrap();
        let (input, output, cached, wire): (i64, i64, i64, Option<i64>) = conn
            .query_row(
                "SELECT input_tokens, output_tokens, cached_input_tokens, input_includes_cache FROM cost_records WHERE session_id = 's1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .unwrap();
        assert_eq!((input, output, cached, wire), (600, 50, 400, Some(0)));
        let wire: Option<i64> = conn
            .query_row("SELECT input_includes_cache FROM cost_records WHERE session_id = 's2'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(wire, None, "unknown wire stays NULL");
    }

    #[test]
    fn cache_hit_ratio_anthropic_vs_openai_semantics() {
        // Anthropic wire: input excludes cache reads — whatever the model name.
        assert_eq!(prompt_tokens_total(ANTHROPIC_WIRE, "claude-sonnet-4-5", 600, 400, 0), 1000);
        assert!((cache_hit_ratio(ANTHROPIC_WIRE, "claude-sonnet-4-5", 600, 400, 0) - 0.4).abs() < 1e-9);
        assert_eq!(prompt_tokens_total(ANTHROPIC_WIRE, "kimi-k2", 600, 400, 0), 1000);
        assert!((cache_hit_ratio(ANTHROPIC_WIRE, "kimi-k2", 600, 400, 0) - 0.4).abs() < 1e-9);
        // OpenAI-compatible wire: prompt_tokens include cache — even for a claude name (proxy).
        assert_eq!(prompt_tokens_total(OPENAI_WIRE, "deepseek-chat", 1000, 400, 0), 1000);
        assert!((cache_hit_ratio(OPENAI_WIRE, "deepseek-chat", 1000, 400, 0) - 0.4).abs() < 1e-9);
        assert_eq!(prompt_tokens_total(OPENAI_WIRE, "claude-sonnet-4-5", 1000, 400, 0), 1000);
        assert!((cache_hit_ratio(OPENAI_WIRE, "claude-sonnet-4-5", 1000, 400, 0) - 0.4).abs() < 1e-9);
        // Unknown wire (legacy NULL): model-name heuristic.
        assert_eq!(prompt_tokens_total(None, "claude-sonnet-4-5", 600, 400, 0), 1000);
        assert_eq!(prompt_tokens_total(None, "deepseek-chat", 1000, 400, 0), 1000);
        // Zero denominator → 0.0; clamped to 1.0.
        assert_eq!(cache_hit_ratio(OPENAI_WIRE, "deepseek-chat", 0, 0, 0), 0.0);
        assert_eq!(cache_hit_ratio(ANTHROPIC_WIRE, "claude-x", 0, 0, 0), 0.0);
        assert_eq!(cache_hit_ratio(OPENAI_WIRE, "gpt-4o", 100, 500, 0), 1.0);
    }

    #[test]
    fn session_cache_stats_aggregates_per_session() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        // Mixed rows in one session: Anthropic-wire kimi, OpenAI-wire claude
        // name, legacy NULL claude row, legacy NULL deepseek row.
        insert_cached(&db, "s1", "kimi-k2", 600, 400, ANTHROPIC_WIRE, now);
        insert_cached(&db, "s1", "claude-sonnet-4-5", 1000, 400, OPENAI_WIRE, now);
        insert_cached(&db, "s1", "claude-sonnet-4-5", 600, 400, None, now);
        insert_cached(&db, "s1", "deepseek-chat", 1000, 400, None, now);
        insert_cached(&db, "s2", "deepseek-chat", 5000, 0, OPENAI_WIRE, now);
        let conn = db.lock().unwrap();
        let stats = session_cache_stats(&conn, "s1").unwrap();
        assert_eq!(stats.calls, 4);
        assert_eq!(stats.prompt_tokens_total, 4000);
        assert_eq!(stats.cached_input_tokens, 1600);
        assert!((stats.hit_ratio - 0.4).abs() < 1e-9);
        let json = serde_json::to_value(&stats).unwrap();
        for key in ["calls", "promptTokensTotal", "cachedInputTokens", "hitRatio"] {
            assert!(json.get(key).is_some(), "{key} missing: {json}");
        }

        let empty = session_cache_stats(&conn, "nope").unwrap();
        assert_eq!((empty.calls, empty.prompt_tokens_total, empty.hit_ratio), (0, 0, 0.0));
    }

    #[test]
    fn daily_rollup_includes_cached_and_ratio() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        insert_cached(&db, "s1", "kimi-k2", 600, 400, ANTHROPIC_WIRE, now);
        insert_cached(&db, "s1", "claude-sonnet-4-5", 1000, 400, OPENAI_WIRE, now);
        insert_cached(&db, "s1", "claude-sonnet-4-5", 600, 400, None, now);
        let conn = db.lock().unwrap();
        let days = daily_rollups(&conn, now - 86_400_000).unwrap();
        assert_eq!(days.len(), 1);
        let d = &days[0];
        assert_eq!(d.input_tokens, 2200, "raw input stays the column sum");
        assert_eq!(d.prompt_tokens_total, 3000);
        assert_eq!(d.cached_input_tokens, 1200);
        assert_eq!(d.turn_count, 3);
        assert!((d.hit_ratio - 0.4).abs() < 1e-9, "{}", d.hit_ratio);

        let models = model_rollups(&conn, now - 86_400_000).unwrap();
        assert_eq!(models.len(), 2, "one row per model across wire groups");
        let claude = models.iter().find(|m| m.model == "claude-sonnet-4-5").unwrap();
        assert_eq!(claude.input_tokens, 1600);
        assert_eq!(claude.prompt_tokens_total, 2000);
        assert_eq!(claude.cached_input_tokens, 800);
        assert_eq!(claude.turn_count, 2);
        assert!((claude.hit_ratio - 0.4).abs() < 1e-9);
        let kimi = models.iter().find(|m| m.model == "kimi-k2").unwrap();
        assert_eq!(kimi.prompt_tokens_total, 1000);
        assert!((kimi.hit_ratio - 0.4).abs() < 1e-9);
        let json = serde_json::to_value(&models[0]).unwrap();
        assert!(json.get("promptTokensTotal").is_some() && json.get("inputTokens").is_some(), "{json}");
    }

    // ─── Final review fix — Anthropic prompt total counts cache writes ──
    // On the Anthropic Messages wire the full prompt is
    // input + cache_read + cache_creation; omitting creation overstated the
    // hit ratio on warm caches (every turn writes the new suffix).

    #[test]
    fn anthropic_prompt_total_counts_cache_creation() {
        // Warm cache turn: 100 fresh, 800 read, 100 written → 80% hit.
        assert_eq!(prompt_tokens_total(ANTHROPIC_WIRE, "claude-sonnet-4-5", 100, 800, 100), 1000);
        assert!((cache_hit_ratio(ANTHROPIC_WIRE, "claude-sonnet-4-5", 100, 800, 100) - 0.8).abs() < 1e-9);
        // OpenAI wire: prompt_tokens already the whole prompt; creation not
        // reported separately → ignored.
        assert_eq!(prompt_tokens_total(OPENAI_WIRE, "deepseek-chat", 1000, 400, 50), 1000);
        assert!((cache_hit_ratio(OPENAI_WIRE, "deepseek-chat", 1000, 400, 50) - 0.4).abs() < 1e-9);
        // Legacy NULL claude row (creation column defaults to 0) → unchanged fallback.
        assert_eq!(prompt_tokens_total(None, "claude-sonnet-4-5", 600, 400, 0), 1000);
        assert_eq!(prompt_tokens_total(None, "deepseek-chat", 1000, 400, 0), 1000);
    }

    #[test]
    fn record_usage_writes_cache_creation_tokens() {
        let db = open_test_db_with_cost_records();
        let conn = db.lock().unwrap();
        insert_usage(&conn, "s1", "claude-sonnet-4-5", 100, 50, 800, 100, ANTHROPIC_WIRE).unwrap();
        let creation: i64 = conn
            .query_row("SELECT cache_creation_tokens FROM cost_records WHERE session_id = 's1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(creation, 100);
    }

    #[test]
    fn session_stats_and_rollups_count_anthropic_cache_writes() {
        let db = open_test_db_with_cost_records();
        let now = chrono::Utc::now().timestamp_millis();
        // Anthropic warm turn: 100 + 800 read + 100 written = 1000 prompt.
        insert_cached_w(&db, "s1", "claude-sonnet-4-5", 100, 800, 100, ANTHROPIC_WIRE, now);
        // OpenAI turn: creation ignored (prompt_tokens already the total).
        insert_cached_w(&db, "s1", "deepseek-chat", 1000, 400, 50, OPENAI_WIRE, now);
        // Legacy NULL claude row, creation 0.
        insert_cached(&db, "s1", "claude-sonnet-4-5", 600, 400, None, now);
        let conn = db.lock().unwrap();
        let stats = session_cache_stats(&conn, "s1").unwrap();
        assert_eq!(stats.prompt_tokens_total, 3000);
        assert_eq!(stats.cached_input_tokens, 1600);
        assert!((stats.hit_ratio - 1600.0 / 3000.0).abs() < 1e-9);

        let days = daily_rollups(&conn, now - 86_400_000).unwrap();
        assert_eq!(days[0].prompt_tokens_total, 3000);
        assert!((days[0].hit_ratio - 1600.0 / 3000.0).abs() < 1e-9);
        let models = model_rollups(&conn, now - 86_400_000).unwrap();
        let claude = models.iter().find(|m| m.model == "claude-sonnet-4-5").unwrap();
        assert_eq!(claude.prompt_tokens_total, 2000);
        assert!((claude.hit_ratio - 0.6).abs() < 1e-9);
    }
}
