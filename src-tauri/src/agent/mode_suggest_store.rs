//! SQLite CRUD for plan_suggest_events (V34).
//! Each row = one banner-fire + its eventual user outcome.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SuggestSource {
    Keyword,
    Agent,
}

impl SuggestSource {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::Agent => "agent",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Outcome {
    Pending,
    Accepted,
    Skipped,
    Silenced,
    Aborted,
}

impl Outcome {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Accepted => "accepted",
            Self::Skipped => "skipped",
            Self::Silenced => "silenced",
            Self::Aborted => "aborted",
        }
    }
}

#[derive(Debug, Clone)]
pub struct FireRecord<'a> {
    pub id: &'a str,
    pub session_id: &'a str,
    pub message_id: &'a str,
    pub source: SuggestSource,
    pub matched_pattern: Option<&'a str>,
    pub reason: Option<&'a str>,
    pub user_msg_preview: &'a str,
    pub fired_at: i64,
}

pub fn record_fired(conn: &Connection, r: FireRecord<'_>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO plan_suggest_events
         (id, session_id, message_id, source, matched_pattern, reason,
          user_msg_preview, outcome, fired_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, 'pending', ?)",
        params![
            r.id,
            r.session_id,
            r.message_id,
            r.source.as_str(),
            r.matched_pattern,
            r.reason,
            r.user_msg_preview,
            r.fired_at,
        ],
    )?;
    Ok(())
}

pub fn record_outcome(
    conn: &Connection,
    id: &str,
    outcome: Outcome,
    decline_reason: Option<&str>,
    decided_at: i64,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE plan_suggest_events
         SET outcome = ?, decline_reason = ?, decided_at = ?
         WHERE id = ?",
        params![outcome.as_str(), decline_reason, decided_at, id],
    )?;
    Ok(())
}

#[derive(Debug, Clone, PartialEq)]
pub struct PatternStats {
    pub pattern: String,
    pub firings: u32,
    pub accepted: u32,
    pub skipped: u32,
    pub silenced: u32,
}

impl PatternStats {
    pub fn accept_rate(&self) -> f32 {
        let decided = self.accepted + self.skipped + self.silenced;
        if decided == 0 {
            0.0
        } else {
            self.accepted as f32 / decided as f32
        }
    }
}

pub fn query_per_pattern_stats(
    conn: &Connection,
    since_ms: i64,
) -> rusqlite::Result<Vec<PatternStats>> {
    let mut stmt = conn.prepare(
        "SELECT matched_pattern,
                COUNT(*) AS firings,
                SUM(CASE WHEN outcome = 'accepted' THEN 1 ELSE 0 END) AS accepted,
                SUM(CASE WHEN outcome = 'skipped' THEN 1 ELSE 0 END) AS skipped,
                SUM(CASE WHEN outcome = 'silenced' THEN 1 ELSE 0 END) AS silenced
         FROM plan_suggest_events
         WHERE source = 'keyword' AND matched_pattern IS NOT NULL AND fired_at >= ?
         GROUP BY matched_pattern",
    )?;
    let rows = stmt.query_map([since_ms], |r| {
        Ok(PatternStats {
            pattern: r.get(0)?,
            firings: r.get::<_, i64>(1)? as u32,
            accepted: r.get::<_, i64>(2)? as u32,
            skipped: r.get::<_, i64>(3)? as u32,
            silenced: r.get::<_, i64>(4)? as u32,
        })
    })?;
    rows.collect()
}

/// Stub returning an empty list. Filled in by the
/// plan_mode_calibration scenario (Task 10) which writes per-pattern
/// silence flags into a sibling table.
pub fn query_disabled_patterns(_conn: &Connection) -> rusqlite::Result<Vec<String>> {
    Ok(Vec::new())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run;

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        // Insert a fake session so the FK doesn't reject our test rows.
        conn.execute(
            "INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)",
            [],
        )
        .unwrap();
        conn
    }

    #[test]
    fn record_fired_then_outcome_roundtrip() {
        let conn = fresh_db();
        record_fired(
            &conn,
            FireRecord {
                id: "e1",
                session_id: "s1",
                message_id: "m1",
                source: SuggestSource::Keyword,
                matched_pattern: Some("计划"),
                reason: None,
                user_msg_preview: "做个五子棋计划",
                fired_at: 1_000,
            },
        )
        .unwrap();
        record_outcome(&conn, "e1", Outcome::Accepted, None, 2_000).unwrap();

        let stats = query_per_pattern_stats(&conn, 0).unwrap();
        assert_eq!(stats.len(), 1);
        assert_eq!(stats[0].pattern, "计划");
        assert_eq!(stats[0].firings, 1);
        assert_eq!(stats[0].accepted, 1);
        assert!((stats[0].accept_rate() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn accept_rate_with_mixed_outcomes() {
        let conn = fresh_db();
        for (i, outcome) in [
            Outcome::Accepted,
            Outcome::Skipped,
            Outcome::Skipped,
            Outcome::Silenced,
            Outcome::Pending,
        ]
        .iter()
        .enumerate()
        {
            let id = format!("e{}", i);
            record_fired(
                &conn,
                FireRecord {
                    id: &id,
                    session_id: "s1",
                    message_id: "m1",
                    source: SuggestSource::Keyword,
                    matched_pattern: Some("plan"),
                    reason: None,
                    user_msg_preview: "x",
                    fired_at: 1_000 + i as i64,
                },
            )
            .unwrap();
            record_outcome(&conn, &id, outcome.clone(), None, 2_000).unwrap();
        }
        let stats = query_per_pattern_stats(&conn, 0).unwrap();
        // 5 firings, 1 accepted, 2 skipped, 1 silenced (pending excluded from rate denom)
        assert_eq!(stats[0].firings, 5);
        assert_eq!(stats[0].accepted, 1);
        // accept_rate = 1 / (1+2+1) = 0.25
        assert!((stats[0].accept_rate() - 0.25).abs() < 1e-6);
    }

    #[test]
    fn agent_source_excluded_from_per_pattern_stats() {
        let conn = fresh_db();
        record_fired(
            &conn,
            FireRecord {
                id: "e_agent",
                session_id: "s1",
                message_id: "m1",
                source: SuggestSource::Agent,
                matched_pattern: None,
                reason: Some("LLM says so"),
                user_msg_preview: "x",
                fired_at: 1_000,
            },
        )
        .unwrap();
        record_outcome(&conn, "e_agent", Outcome::Accepted, None, 2_000).unwrap();
        // No keyword pattern → empty result
        assert!(query_per_pattern_stats(&conn, 0).unwrap().is_empty());
    }

    #[test]
    fn since_ms_filter_excludes_old_events() {
        let conn = fresh_db();
        record_fired(
            &conn,
            FireRecord {
                id: "old",
                session_id: "s1",
                message_id: "m1",
                source: SuggestSource::Keyword,
                matched_pattern: Some("plan"),
                reason: None,
                user_msg_preview: "x",
                fired_at: 100,
            },
        )
        .unwrap();
        record_outcome(&conn, "old", Outcome::Accepted, None, 200).unwrap();
        // since_ms = 1000 → old event (fired_at=100) filtered out
        assert!(query_per_pattern_stats(&conn, 1_000).unwrap().is_empty());
    }
}
