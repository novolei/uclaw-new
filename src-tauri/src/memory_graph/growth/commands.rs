//! Tauri commands for memory growth (成长 tab + 记忆成长 settings). Errors
//! are plain strings shown as-is.

use tauri::State;

use super::service::{GrowthDeps, GrowthOutcome, GrowthSettings};
use super::store::{self, ReflectionRow, UserModel, UserModelHistoryRow};
use crate::app::AppState;
use crate::memubot_config::MemoryOsConfig;

/// Clamp user input: N in 5..=200 turns, budget in 0..=500_000 tokens.
pub(crate) fn clamp_settings(enabled: bool, every_n_turns: u32, daily_budget: u32) -> GrowthSettings {
    GrowthSettings { enabled, every_n_turns: every_n_turns.clamp(5, 200), daily_budget: daily_budget.min(500_000) }
}

pub(crate) fn apply_settings(cfg: &mut MemoryOsConfig, s: &GrowthSettings) {
    cfg.growth_enabled = s.enabled;
    cfg.growth_every_n_turns = s.every_n_turns;
    cfg.growth_daily_token_budget = s.daily_budget;
}

fn with_conn<T>(state: &AppState, f: impl FnOnce(&rusqlite::Connection) -> anyhow::Result<T>) -> Result<T, String> {
    let conn = state.db.lock().map_err(|e| e.to_string())?;
    f(&conn).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn list_growth_reflections(
    state: State<'_, AppState>,
    limit: Option<usize>,
    include_archived: Option<bool>,
) -> Result<Vec<ReflectionRow>, String> {
    with_conn(&state, |c| store::list_reflections(c, limit.unwrap_or(30).min(200), include_archived.unwrap_or(false)))
}

#[tauri::command]
pub async fn set_growth_reflection_archived(state: State<'_, AppState>, id: String, archived: bool) -> Result<bool, String> {
    with_conn(&state, |c| store::set_reflection_archived(c, &id, archived))
}

#[tauri::command]
pub async fn get_growth_user_model(state: State<'_, AppState>) -> Result<Option<UserModel>, String> {
    with_conn(&state, store::get_user_model)
}

#[tauri::command]
pub async fn list_growth_user_model_history(
    state: State<'_, AppState>,
    limit: Option<usize>,
) -> Result<Vec<UserModelHistoryRow>, String> {
    with_conn(&state, |c| store::list_user_model_history(c, limit.unwrap_or(10).min(100)))
}

/// 立即刷新 — reflect on any new turns, re-promote, consolidate when due.
#[tauri::command]
pub async fn refresh_growth_now(state: State<'_, AppState>) -> Result<GrowthOutcome, String> {
    let settings = GrowthSettings::from_config(&state.memubot_config.read().await.memory_os);
    let deps = GrowthDeps {
        db: std::sync::Arc::clone(&state.db),
        llm: state.learning_llm.clone(),
        facets: std::sync::Arc::clone(&state.facet_cache),
    };
    Ok(state.growth.run_now(&deps, &settings).await)
}

#[tauri::command]
pub async fn get_growth_settings(state: State<'_, AppState>) -> Result<GrowthSettings, String> {
    Ok(GrowthSettings::from_config(&state.memubot_config.read().await.memory_os))
}

/// Saves clamped values; read fresh each turn, so no service restart.
#[tauri::command]
pub async fn set_growth_settings(
    state: State<'_, AppState>,
    enabled: bool,
    every_n_turns: u32,
    daily_budget: u32,
) -> Result<GrowthSettings, String> {
    let s = clamp_settings(enabled, every_n_turns, daily_budget);
    let mut cfg = state.memubot_config.write().await;
    apply_settings(&mut cfg.memory_os, &s);
    cfg.save(&state.data_dir).map_err(|e| e.to_string())?;
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_growth_settings_clamps() {
        assert_eq!(clamp_settings(true, 1, 9_999_999), GrowthSettings { enabled: true, every_n_turns: 5, daily_budget: 500_000 });
        assert_eq!(clamp_settings(false, 999, 0), GrowthSettings { enabled: false, every_n_turns: 200, daily_budget: 0 });
        let mut cfg = MemoryOsConfig::default();
        apply_settings(&mut cfg, &clamp_settings(false, 30, 1000));
        assert_eq!(GrowthSettings::from_config(&cfg), GrowthSettings { enabled: false, every_n_turns: 30, daily_budget: 1000 });
    }

    #[test]
    fn refresh_outcome_serializes() {
        let skipped = serde_json::to_value(GrowthOutcome::Skipped { reason: "no_llm" }).unwrap();
        assert_eq!(skipped, serde_json::json!({ "status": "skipped", "reason": "no_llm" }));
        let ran = serde_json::to_value(GrowthOutcome::Ran { reflection: true, promoted: false, consolidated: false }).unwrap();
        assert_eq!(ran, serde_json::json!({ "status": "ran", "reflection": true, "promoted": false, "consolidated": false }));
        let s = serde_json::to_value(GrowthSettings { enabled: true, every_n_turns: 20, daily_budget: 1 }).unwrap();
        assert_eq!(s, serde_json::json!({ "enabled": true, "everyNTurns": 20, "dailyBudget": 1 }));
    }
}
