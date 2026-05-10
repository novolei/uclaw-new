//! Write-side persistence for per-turn LLM cost records.
//!
//! Called from the agent dispatcher's `emit_turn_cost` to capture usage
//! synchronously alongside the IPC event. No frontend dependency — events
//! are fire-and-forget from the listener's POV; persistence is the source
//! of truth for the dashboard.

use crate::app::AppState;
use crate::agent::types::calculate_cost;
use rusqlite::params;

/// Insert one cost record. Errors are logged and swallowed — cost capture
/// is best-effort and must never fail the agent loop.
pub fn record(state: &AppState, session_id: &str, model: &str, input_tokens: u32, output_tokens: u32) {
    let cost = calculate_cost(model, input_tokens, output_tokens);
    let now = chrono::Utc::now().timestamp_millis();
    let id = uuid::Uuid::new_v4().to_string();
    let conn = match state.db.lock() {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("cost_store: DB lock failed: {}", e);
            return;
        }
    };
    if let Err(e) = conn.execute(
        "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, session_id, model, input_tokens as i64, output_tokens as i64, cost, now],
    ) {
        tracing::warn!("cost_store: INSERT failed: {}", e);
    }
}
