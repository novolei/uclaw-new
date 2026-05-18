//! Symphony cost helpers.
//!
//! Reuses `automation::runtime::cost::{CostCapConfig, CostCapState}`
//! verbatim — the type is intentionally generic-shaped (per-run cap + per-day
//! cap accumulator), nothing automation-specific. We re-export rather than
//! re-declare so a future tightening of the type lands in one place.
//!
//! Adds one helper specific to Symphony: `day_total_usd(conn, since_ms)`,
//! which sums `cost_records` whose owning `agent_session` was created with
//! `metadata.origin = "symphony:<node-id>"` (set by `run_session::create_node_session`).

pub use crate::automation::runtime::cost::{CostCapConfig, CostCapState};

use rusqlite::{params, Connection};

/// SUM of cost_usd over Symphony-origin sessions whose `cost_records.created_at >= since_ms`.
/// Returns 0.0 on any DB error (best-effort, matches `cost_store::monthly_total`).
pub fn day_total_usd(conn: &Connection, since_ms: i64) -> f64 {
    conn.query_row(
        "SELECT COALESCE(SUM(cr.cost_usd), 0) \
         FROM cost_records cr \
         JOIN agent_sessions s ON s.id = cr.session_id \
         WHERE cr.created_at >= ?1 \
           AND json_extract(s.metadata_json, '$.origin') LIKE 'symphony:%'",
        params![since_ms],
        |r| r.get::<_, f64>(0),
    )
    .unwrap_or(0.0)
}

/// Start of "today" (local midnight, UTC for simplicity) in epoch ms.
/// Matches the day-boundary semantics the automation runtime uses.
pub fn current_day_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), now.day(), 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// `Decision` returned by `check_day_cap` so the caller can render a clean
/// reason on rejection (used by `symphony_trigger_run` Tauri command).
#[derive(Debug, Clone, PartialEq)]
pub enum DayCapDecision {
    Ok { used_usd: f64, cap_usd: f64 },
    Exceeded { used_usd: f64, cap_usd: f64 },
}

pub fn check_day_cap(conn: &Connection, cap_usd: f64) -> DayCapDecision {
    let since = current_day_start_ms();
    let used = day_total_usd(conn, since);
    if used >= cap_usd {
        DayCapDecision::Exceeded {
            used_usd: used,
            cap_usd,
        }
    } else {
        DayCapDecision::Ok {
            used_usd: used,
            cap_usd,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use rusqlite::params;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    /// Insert an agent_session with the given origin metadata, plus a cost record
    /// at `created_at` ms tagged to that session.
    fn insert_origin_session_and_cost(
        conn: &Connection,
        sid: &str,
        origin: &str,
        cost: f64,
        created_at_ms: i64,
    ) {
        let meta = serde_json::json!({ "origin": origin });
        conn.execute(
            "INSERT INTO agent_sessions \
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) \
             VALUES (?1, 'default', ?2, ?3, 0, 0, 0, ?4, ?4)",
            params![sid, format!("session-{}", sid), meta.to_string(), created_at_ms],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at) \
             VALUES (?1, ?2, 'test-model', 100, 50, ?3, ?4)",
            params![format!("c-{}", sid), sid, cost, created_at_ms],
        )
        .unwrap();
    }

    #[test]
    fn day_total_includes_symphony_origin_only() {
        let conn = db();
        let now = Utc::now().timestamp_millis();
        insert_origin_session_and_cost(&conn, "s-sym-1", "symphony:node-a", 1.50, now);
        insert_origin_session_and_cost(&conn, "s-sym-2", "symphony:node-b", 0.75, now);
        insert_origin_session_and_cost(&conn, "s-aut-1", "automation:manual", 99.0, now);
        // Random non-origin session.
        conn.execute(
            "INSERT INTO agent_sessions \
             (id, space_id, title, metadata_json, message_count, pinned, archived, created_at, updated_at) \
             VALUES ('s-bare', 'default', 'bare', '{}', 0, 0, 0, ?1, ?1)",
            params![now],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at) \
             VALUES ('c-bare', 's-bare', 'test', 1, 1, 5.0, ?1)",
            params![now],
        )
        .unwrap();

        let total = day_total_usd(&conn, current_day_start_ms());
        // 1.50 + 0.75 = 2.25, exclusive of the automation + bare records.
        assert!((total - 2.25).abs() < 1e-6, "expected 2.25, got {}", total);
    }

    #[test]
    fn day_total_excludes_records_before_since_ms() {
        let conn = db();
        let now = Utc::now().timestamp_millis();
        let yesterday = now - 26 * 3600 * 1000;
        insert_origin_session_and_cost(&conn, "s-old", "symphony:node", 10.00, yesterday);
        insert_origin_session_and_cost(&conn, "s-new", "symphony:node", 1.00, now);
        let total = day_total_usd(&conn, current_day_start_ms());
        assert!((total - 1.0).abs() < 1e-6);
    }

    #[test]
    fn check_day_cap_decides() {
        let conn = db();
        let now = Utc::now().timestamp_millis();
        insert_origin_session_and_cost(&conn, "s-cap", "symphony:node", 5.0, now);
        assert!(matches!(check_day_cap(&conn, 10.0), DayCapDecision::Ok { .. }));
        assert!(matches!(
            check_day_cap(&conn, 4.99),
            DayCapDecision::Exceeded { .. }
        ));
    }
}
