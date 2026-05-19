//! Memory OS L3 §3.2.2 RETAINED — Timeline daily/weekly/monthly
//! aggregator (per ADR 2026-05-20 §8).
//!
//! Q2a (PR #276) shipped the write path for `timeline_events`. This
//! module (Q2b) ships the **aggregation path**: scan the past 24h
//! (or week / month) of `timeline_events` and write a summary row
//! into `temporal_aggregates`.
//!
//! ## V1 scope (this PR)
//!
//! Zero-LLM SQL-only summary:
//! - Total event count for the period
//! - Top 5 entity IDs by mention frequency (across all events'
//!   `related_entity_ids` arrays)
//! - Top 5 event_kind labels by frequency as "themes"
//! - A short, deterministic markdown summary built from those counts
//!
//! ## V2 scope (future PR)
//!
//! Replace the deterministic summary with a Haiku LLM call that:
//! - Reads the event titles + top entities
//! - Writes a 2-3 sentence natural-language summary
//! - Records cost under `cost_records.model LIKE 'timeline_aggregate%'`
//! - Gated by a daily token budget config
//!
//! The V1 summary already gives the user real value ("you had 17
//! events today touching {Alice, ProjectFalcon, gbrain}"). V2 makes
//! it prose; V1 is the foundation for the schema + scheduler.

use rusqlite::{params, Connection};
use std::collections::HashMap;

/// Aggregation period granularity. Wire string matches the
/// `temporal_aggregates.grain` column convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggregationGrain {
    Day,
    Week,
    Month,
}

impl AggregationGrain {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Day => "day",
            Self::Week => "week",
            Self::Month => "month",
        }
    }
}

/// Outcome of one `aggregate_period` call. Captures what was computed
/// so the scheduler hook (and any future Tauri command) can log + return
/// progress.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AggregationOutcome {
    pub aggregate_id: String,
    pub grain: AggregationGrain,
    pub period_start: i64,
    pub period_end: i64,
    pub event_count: usize,
}

/// Run one aggregation pass over `timeline_events` in `[period_start,
/// period_end)` for the given `grain`. Idempotent: writes to
/// `temporal_aggregates` via INSERT OR REPLACE on the
/// `(space_id, grain, period_start)` unique key.
///
/// `space_id` filters which space's events are aggregated; pass
/// `"default"` for the standard single-workspace case.
///
/// Returns the inserted/updated aggregate's id + the event count
/// that drove the summary.
///
/// V1 implementation is SQL-only (zero LLM). V2 will pipe the same
/// inputs through a Haiku call to produce a natural-language summary;
/// the row shape doesn't change between versions.
pub fn aggregate_period(
    conn: &Connection,
    space_id: &str,
    grain: AggregationGrain,
    period_start: i64,
    period_end: i64,
) -> rusqlite::Result<AggregationOutcome> {
    // 1) Pull all events in the window.
    let mut stmt = conn.prepare(
        "SELECT event_kind, title, related_entity_ids
         FROM timeline_events
         WHERE space_id = ?1 AND occurred_at >= ?2 AND occurred_at < ?3
         ORDER BY occurred_at",
    )?;
    let rows = stmt.query_map(
        params![space_id, period_start, period_end],
        |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
            ))
        },
    )?;

    let mut event_kind_counts: HashMap<String, usize> = HashMap::new();
    let mut entity_counts: HashMap<String, usize> = HashMap::new();
    let mut event_titles: Vec<String> = Vec::new();
    let mut event_count: usize = 0;

    for row in rows {
        let (kind, title, related_json) = row?;
        event_count += 1;
        *event_kind_counts.entry(kind).or_insert(0) += 1;
        event_titles.push(title);
        if let Ok(entities) = serde_json::from_str::<Vec<String>>(&related_json) {
            for entity_id in entities {
                *entity_counts.entry(entity_id).or_insert(0) += 1;
            }
        }
    }
    drop(stmt);

    // 2) Pick top 5 themes (event_kind) + top 5 entities by frequency.
    let top_themes = top_n(&event_kind_counts, 5);
    let top_entities = top_n(&entity_counts, 5);

    // 3) Build deterministic markdown summary. V2 will replace this
    //    with an LLM-generated narrative.
    let summary_md = render_summary_md(grain, event_count, &top_themes, &top_entities);

    // 4) Upsert (the V44 UNIQUE(space_id, grain, period_start) makes
    //    INSERT OR REPLACE the natural idempotency primitive).
    let aggregate_id = format!("agg-{}-{}-{}", space_id, grain.as_str(), period_start);
    let top_themes_json = serde_json::to_string(&top_themes).unwrap_or_else(|_| "[]".into());
    let top_entities_json = serde_json::to_string(&top_entities).unwrap_or_else(|_| "[]".into());
    let now_ms = chrono::Utc::now().timestamp_millis();

    conn.execute(
        "INSERT INTO temporal_aggregates
             (id, space_id, grain, period_start, period_end, summary_md,
              event_count, top_themes, top_entities, llm_model,
              token_cost, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL, NULL, ?10)
         ON CONFLICT(space_id, grain, period_start) DO UPDATE SET
             period_end = excluded.period_end,
             summary_md = excluded.summary_md,
             event_count = excluded.event_count,
             top_themes = excluded.top_themes,
             top_entities = excluded.top_entities,
             created_at = excluded.created_at",
        params![
            aggregate_id,
            space_id,
            grain.as_str(),
            period_start,
            period_end,
            summary_md,
            event_count as i64,
            top_themes_json,
            top_entities_json,
            now_ms,
        ],
    )?;

    Ok(AggregationOutcome {
        aggregate_id,
        grain,
        period_start,
        period_end,
        event_count,
    })
}

/// Pick the top N entries by count from a HashMap. Ties are broken
/// alphabetically by key so the result is deterministic (useful for
/// tests + cache-comparing).
fn top_n(counts: &HashMap<String, usize>, n: usize) -> Vec<String> {
    let mut pairs: Vec<(&String, &usize)> = counts.iter().collect();
    pairs.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));
    pairs.into_iter().take(n).map(|(k, _)| k.clone()).collect()
}

/// Render a deterministic markdown summary from the aggregated
/// counts. Reads naturally even though it's SQL-driven: e.g.
///
/// > 17 events in this day. Top themes: entity_page_created (12),
/// > session_start (3), skill_learned (2). Most-mentioned entities:
/// > node-alice, node-projectfalcon, node-gbrain.
fn render_summary_md(
    grain: AggregationGrain,
    event_count: usize,
    top_themes: &[String],
    top_entities: &[String],
) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{} events in this {}.",
        event_count,
        grain.as_str()
    ));
    if !top_themes.is_empty() {
        s.push_str("\n\nTop event kinds: ");
        s.push_str(&top_themes.join(", "));
        s.push('.');
    }
    if !top_entities.is_empty() {
        s.push_str("\n\nMost-mentioned entities: ");
        s.push_str(&top_entities.join(", "));
        s.push('.');
    }
    s
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory_graph::timeline_events::{insert_event, TimelineEvent, TimelineEventKind};

    fn fresh_conn() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        crate::db::migrations::run(&conn).unwrap();
        conn
    }

    fn seed_event(
        conn: &Connection,
        id: &str,
        space: &str,
        kind: TimelineEventKind,
        related: Vec<&str>,
        occurred_at: i64,
    ) {
        let evt = TimelineEvent {
            id: id.into(),
            space_id: space.into(),
            event_kind: kind,
            subject_id: None,
            title: format!("title for {}", id),
            payload_json: None,
            related_entity_ids: related.into_iter().map(String::from).collect(),
            occurred_at,
            importance: 0.5,
        };
        insert_event(conn, &evt).unwrap();
    }

    #[test]
    fn aggregate_empty_window_writes_row_with_zero_count() {
        let conn = fresh_conn();
        let outcome = aggregate_period(
            &conn,
            "default",
            AggregationGrain::Day,
            1_700_000_000_000,
            1_700_086_400_000,
        )
        .unwrap();
        assert_eq!(outcome.event_count, 0);
        let summary: String = conn
            .query_row(
                "SELECT summary_md FROM temporal_aggregates WHERE id = ?1",
                [&outcome.aggregate_id],
                |r| r.get(0),
            )
            .unwrap();
        assert!(summary.starts_with("0 events"));
    }

    #[test]
    fn aggregate_counts_only_events_within_window() {
        let conn = fresh_conn();
        // 3 events inside the window, 2 outside.
        seed_event(&conn, "e1", "default", TimelineEventKind::EntityPageCreated, vec![], 100);
        seed_event(&conn, "e2", "default", TimelineEventKind::EntityPageCreated, vec![], 200);
        seed_event(&conn, "e3", "default", TimelineEventKind::SessionStart, vec![], 300);
        seed_event(&conn, "e-before", "default", TimelineEventKind::EntityPageCreated, vec![], 50);
        seed_event(&conn, "e-after", "default", TimelineEventKind::EntityPageCreated, vec![], 500);

        let outcome =
            aggregate_period(&conn, "default", AggregationGrain::Day, 100, 400).unwrap();
        assert_eq!(outcome.event_count, 3);
    }

    #[test]
    fn aggregate_picks_top_themes_and_entities_by_frequency() {
        let conn = fresh_conn();
        // 4 EntityPageCreated, 1 SessionStart. Entities: alice ×3, bob ×2.
        seed_event(&conn, "e1", "default", TimelineEventKind::EntityPageCreated, vec!["alice"], 100);
        seed_event(&conn, "e2", "default", TimelineEventKind::EntityPageCreated, vec!["alice"], 101);
        seed_event(&conn, "e3", "default", TimelineEventKind::EntityPageCreated, vec!["alice", "bob"], 102);
        seed_event(&conn, "e4", "default", TimelineEventKind::EntityPageCreated, vec!["bob"], 103);
        seed_event(&conn, "e5", "default", TimelineEventKind::SessionStart, vec![], 104);

        aggregate_period(&conn, "default", AggregationGrain::Day, 100, 200).unwrap();
        let (themes_json, entities_json): (String, String) = conn
            .query_row(
                "SELECT top_themes, top_entities FROM temporal_aggregates LIMIT 1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        let themes: Vec<String> = serde_json::from_str(&themes_json).unwrap();
        let entities: Vec<String> = serde_json::from_str(&entities_json).unwrap();
        assert_eq!(themes[0], "entity_page_created");
        assert_eq!(entities[0], "alice", "alice (3x) should outrank bob (2x)");
        assert_eq!(entities[1], "bob");
    }

    #[test]
    fn aggregate_isolated_by_space_id() {
        let conn = fresh_conn();
        seed_event(&conn, "ev-a", "space-A", TimelineEventKind::EntityPageCreated, vec![], 100);
        seed_event(&conn, "ev-b", "space-B", TimelineEventKind::EntityPageCreated, vec![], 100);
        let a = aggregate_period(&conn, "space-A", AggregationGrain::Day, 0, 200).unwrap();
        let b = aggregate_period(&conn, "space-B", AggregationGrain::Day, 0, 200).unwrap();
        assert_eq!(a.event_count, 1);
        assert_eq!(b.event_count, 1);
        assert_ne!(a.aggregate_id, b.aggregate_id);
    }

    #[test]
    fn aggregate_is_idempotent_via_upsert() {
        // Running aggregate_period twice for the same (space, grain, period)
        // must NOT create duplicate rows. The V44 UNIQUE constraint +
        // INSERT OR REPLACE handle this; verify the row count stays 1.
        let conn = fresh_conn();
        seed_event(&conn, "e1", "default", TimelineEventKind::EntityPageCreated, vec![], 100);

        aggregate_period(&conn, "default", AggregationGrain::Day, 0, 200).unwrap();
        aggregate_period(&conn, "default", AggregationGrain::Day, 0, 200).unwrap();
        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM temporal_aggregates
                 WHERE space_id = 'default' AND grain = 'day' AND period_start = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "double-aggregation must upsert, not duplicate");
    }

    #[test]
    fn aggregate_updates_summary_when_new_events_arrive() {
        // First aggregation sees 1 event. Add 2 more in the same window.
        // Re-aggregate: the upserted row must reflect the new count.
        let conn = fresh_conn();
        seed_event(&conn, "e1", "default", TimelineEventKind::EntityPageCreated, vec![], 100);
        let o1 = aggregate_period(&conn, "default", AggregationGrain::Day, 0, 200).unwrap();
        assert_eq!(o1.event_count, 1);

        seed_event(&conn, "e2", "default", TimelineEventKind::EntityPageCreated, vec![], 150);
        seed_event(&conn, "e3", "default", TimelineEventKind::SessionStart, vec![], 160);
        let o2 = aggregate_period(&conn, "default", AggregationGrain::Day, 0, 200).unwrap();
        assert_eq!(o2.event_count, 3);

        let count: i64 = conn
            .query_row(
                "SELECT event_count FROM temporal_aggregates
                 WHERE space_id = 'default' AND grain = 'day' AND period_start = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 3, "stored event_count must reflect latest aggregation");
    }

    #[test]
    fn top_n_breaks_ties_alphabetically_for_determinism() {
        let mut h = HashMap::new();
        h.insert("zebra".to_string(), 2);
        h.insert("apple".to_string(), 2);
        h.insert("mango".to_string(), 1);
        let top = top_n(&h, 3);
        assert_eq!(top, vec!["apple", "zebra", "mango"]);
    }

    #[test]
    fn render_summary_md_includes_count_themes_and_entities() {
        let s = render_summary_md(
            AggregationGrain::Day,
            5,
            &["entity_page_created".into(), "session_start".into()],
            &["node-alice".into(), "node-bob".into()],
        );
        assert!(s.contains("5 events in this day"));
        assert!(s.contains("entity_page_created"));
        assert!(s.contains("node-alice"));
    }
}
