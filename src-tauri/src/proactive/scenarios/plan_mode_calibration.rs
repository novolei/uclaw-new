//! Plan-mode auto-suggest calibration scenario.
//!
//! Reads plan_suggest_events, computes per-pattern accept rates, and
//! silences low-acceptance patterns for 14 days. Runs on the standard
//! proactive cadence; lightweight (one aggregate query + ≤K upserts).
//!
//! The scenario differs from LLM-backed scenarios: `build_context` runs
//! the calibration directly against the DB and returns an Ok result with
//! an empty context_messages list. The proactive service recognises the
//! "plan_mode_calibration" name and skips the LLM call.

use std::sync::{Arc, Mutex};
use async_trait::async_trait;
use rusqlite::Connection;
use super::types::{ProactiveScenario, ScenarioContext, ScenarioOutput};

const WINDOW_DAYS: i64 = 7;
const MIN_FIRINGS: u32 = 20;
const SILENCE_THRESHOLD: f32 = 0.30;
const SILENCE_DURATION_DAYS: i64 = 14;

/// Minimum interval between calibration runs (24 hours).
const MIN_INTERVAL_MS: u128 = 24 * 60 * 60 * 1000;

pub struct PlanModeCalibrationScenario {
    db: Arc<Mutex<Connection>>,
}

impl PlanModeCalibrationScenario {
    pub fn new(db: Arc<Mutex<Connection>>) -> Self {
        Self { db }
    }

    /// Core calibration logic — exposed as `pub(crate)` for unit tests.
    pub(crate) fn calibrate(&self, conn: &Connection) -> rusqlite::Result<usize> {
        let window_start = chrono::Utc::now().timestamp_millis()
            - WINDOW_DAYS * 24 * 60 * 60 * 1000;
        let stats = crate::agent::mode_suggest_store::query_per_pattern_stats(
            conn, window_start,
        )?;
        let mut silenced_count = 0usize;
        for s in stats {
            if s.firings < MIN_FIRINGS {
                continue;
            }
            let rate = s.accept_rate();
            if rate < SILENCE_THRESHOLD {
                let until = chrono::Utc::now().timestamp_millis()
                    + SILENCE_DURATION_DAYS * 24 * 60 * 60 * 1000;
                crate::agent::mode_suggest_store::upsert_disabled_pattern(
                    conn,
                    &s.pattern,
                    until,
                    &format!(
                        "accept_rate={:.2} < {:.2} after {} firings",
                        rate, SILENCE_THRESHOLD, s.firings
                    ),
                )?;
                silenced_count += 1;
                tracing::info!(
                    pattern = %s.pattern,
                    accept_rate = rate,
                    firings = s.firings,
                    "Plan-mode calibration silenced pattern for 14d"
                );
            }
        }
        Ok(silenced_count)
    }
}

#[async_trait]
impl ProactiveScenario for PlanModeCalibrationScenario {
    fn name(&self) -> &str {
        "plan_mode_calibration"
    }

    fn description(&self) -> &str {
        "Calibrate plan-mode keyword acceptance — silence low-accept patterns"
    }

    /// Trigger at most once per 24 hours, and only if plan_suggest_events
    /// actually has data (saves a lock acquisition on quiet installs).
    async fn should_trigger(&self, ctx: &ScenarioContext) -> bool {
        // Enforce minimum interval via last_trigger_at map injected by the service.
        if let Some(last) = ctx.last_trigger_at.get(self.name()) {
            if last.elapsed().as_millis() < MIN_INTERVAL_MS {
                return false;
            }
        }
        // Always eligible after the interval — calibrate() is fast.
        true
    }

    /// Runs the calibration and returns a no-op ScenarioOutput (no LLM
    /// call is made for this scenario — service.rs branches on the name).
    async fn build_context(&self, _ctx: &ScenarioContext) -> anyhow::Result<ScenarioOutput> {
        let conn = self.db
            .lock()
            .map_err(|_| anyhow::anyhow!("DB lock poisoned"))?;
        let silenced = self.calibrate(&conn)?;
        tracing::info!(
            silenced_count = silenced,
            "plan_mode_calibration: calibration complete"
        );
        Ok(ScenarioOutput {
            scenario_name: self.name().to_string(),
            system_prompt: String::new(),
            context_messages: vec![],
            memory_types: vec![],
            additional_instructions: None,
        })
    }

    fn system_prompt(&self) -> &str {
        ""
    }

    fn memory_types(&self) -> Vec<String> {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::migrations::run;
    use crate::agent::mode_suggest_store::{
        record_fired, record_outcome, FireRecord, SuggestSource, Outcome,
    };

    fn fresh_db() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        run(&conn).unwrap();
        conn.execute(
            "INSERT INTO agent_sessions (id, created_at, updated_at) VALUES ('s1', 0, 0)",
            [],
        )
        .unwrap();
        conn
    }

    fn fire_with_outcome(conn: &Connection, id: &str, pattern: &str, outcome: Outcome) {
        record_fired(
            conn,
            FireRecord {
                id,
                session_id: "s1",
                message_id: "m1",
                source: SuggestSource::Keyword,
                matched_pattern: Some(pattern),
                reason: None,
                user_msg_preview: "x",
                fired_at: chrono::Utc::now().timestamp_millis(),
            },
        )
        .unwrap();
        record_outcome(conn, id, outcome, None, chrono::Utc::now().timestamp_millis()).unwrap();
    }

    #[test]
    fn pattern_below_threshold_with_enough_firings_silenced() {
        let conn = fresh_db();
        // 20 firings: 4 accepted, 16 skipped → 20% accept rate (below 30%)
        for i in 0..4 {
            fire_with_outcome(&conn, &format!("a{}", i), "plan", Outcome::Accepted);
        }
        for i in 0..16 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(Arc::new(Mutex::new(conn)));
        let conn_g = scenario.db.lock().unwrap();
        let n = scenario.calibrate(&conn_g).unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn pattern_below_threshold_but_too_few_firings_not_silenced() {
        let conn = fresh_db();
        // Only 10 firings — below MIN_FIRINGS (20)
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(Arc::new(Mutex::new(conn)));
        let conn_g = scenario.db.lock().unwrap();
        assert_eq!(scenario.calibrate(&conn_g).unwrap(), 0);
    }

    #[test]
    fn pattern_above_threshold_not_silenced() {
        let conn = fresh_db();
        // 20 firings: 10 accepted, 10 skipped → 50% accept rate (above 30%)
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("a{}", i), "plan", Outcome::Accepted);
        }
        for i in 0..10 {
            fire_with_outcome(&conn, &format!("s{}", i), "plan", Outcome::Skipped);
        }
        let scenario = PlanModeCalibrationScenario::new(Arc::new(Mutex::new(conn)));
        let conn_g = scenario.db.lock().unwrap();
        assert_eq!(scenario.calibrate(&conn_g).unwrap(), 0);
    }
}
