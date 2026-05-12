# Phase 6-C — Cost Dashboard + Monthly Budget Alerts Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a monthly budget anchor (month-to-date total + 80%/100% toast alerts) and a per-workspace cost rollup to the existing Settings → 用量 tab, without breaking the 30-day Recharts views that already live there.

**Architecture:** No DB migration. V13's `cost_records` already has everything. Add three small Tauri commands (`list_workspace_cost_rollup`, `get_month_cost_total`, plus a budget-threshold event emitted from the existing `emit_turn_cost`). Extend `UserSettings` with `monthly_budget_usd`. Frontend adds a `BudgetHeader` row + per-workspace `BarList` above the existing daily/model/session charts. Toast on threshold crossing via a `budget:threshold` event listener in AppShell, deduped per-session.

**Tech Stack:** Rust (rusqlite, Tauri events), TypeScript + React + Jotai, sonner toast, Tailwind theme tokens.

---

## Background: spec drift vs. reality

Spec §1 calls the current UI "rudimentary" — that understates it. [ui/src/components/settings/UsageSettings.tsx](ui/src/components/settings/UsageSettings.tsx) already renders three Recharts views (daily bar chart, per-model donut, per-session table) over a 30-day window with live-update via `agent:turn_cost`. The Rust side has `get_daily_costs`, `get_model_costs`, `get_session_costs` in [src-tauri/src/tauri_commands.rs:603-700](src-tauri/src/tauri_commands.rs#L603-L700) and matching IPC types at [src-tauri/src/ipc.rs:833-866](src-tauri/src/ipc.rs#L833-L866).

So Phase 6-C is **additive**, not a rewrite:

- **Keep** the 30-day Recharts dashboard. Don't regress what works.
- **Add** a month-to-date budget header at the top of the tab (this is the new value: budget anchor + 80%/100% alerts).
- **Add** per-workspace rollup as a new section between the budget header and the existing daily chart.
- **Add** monthly budget input field in Settings persistence.

Spec §5 says "register all three commands in `main.rs`". We need only two new commands (`list_workspace_cost_rollup`, `get_month_cost_total`) — per-model already exists as `get_model_costs` and is reusable as-is.

Spec §6 assumes a richer `Settings` shape with `patch_settings` supporting partial updates of arbitrary fields. The actual `UserSettings` (defined in [src-tauri/src/settings.rs:7-10](src-tauri/src/settings.rs#L7-L10)) only has `language` and `theme`. Adding `monthly_budget_usd` requires updates to four places: `UserSettings`, `GetSettingsResponse`, `PatchSettingsInput`, and `patch_settings`. Mechanical but real.

## File Structure

**New files:**
- `ui/src/atoms/cost.ts` — atoms + helper for month-start and threshold dedup.

**Modified files:**
- `src-tauri/src/settings.rs` — add `monthly_budget_usd: Option<f64>` to `UserSettings`.
- `src-tauri/src/ipc.rs` — add field to `GetSettingsResponse` and `PatchSettingsInput`; add new types `WorkspaceCostRollup`, `BudgetThresholdPayload`.
- `src-tauri/src/tauri_commands.rs` — extend `get_settings` / `patch_settings` to round-trip the field; add `list_workspace_cost_rollup` and `get_month_cost_total` commands.
- `src-tauri/src/main.rs` — register the two new commands in `invoke_handler!`.
- `src-tauri/src/agent/dispatcher.rs` — extend `emit_turn_cost` to compute monthly total and emit `budget:threshold` when crossing 80% / 100%.
- `ui/src/lib/tauri-bridge.ts` — wrappers for the two new commands + `onBudgetThreshold` event listener.
- `ui/src/lib/types.ts` — `WorkspaceCostRollup`, `BudgetThresholdPayload` types + extend `GetSettingsResponse` / `PatchSettingsInput`.
- `ui/src/components/settings/UsageSettings.tsx` — add `BudgetHeader` + per-workspace section above the existing charts. Rename the tab label from "用量" to "用量与预算".
- `ui/src/components/settings/SettingsPanel.tsx` — relabel the tab.
- `ui/src/components/app-shell/AppShell.tsx` — mount the `budget:threshold` toast listener.

**Test files:**
- `src-tauri/src/tauri_commands.rs` — extend `#[cfg(test)] mod cost_tests` (or add a new one) for the new commands.
- `ui/src/atoms/cost.test.ts` — atom helper tests.
- `ui/src/components/settings/UsageSettings.test.tsx` — extend or create for the new sections (existing tests mock Recharts; follow that pattern).

---

## Task 1: Settings — `monthlyBudgetUsd` round-trip

**Files:**
- Modify: `src-tauri/src/settings.rs:5-19`
- Modify: `src-tauri/src/ipc.rs:8-22`
- Modify: `src-tauri/src/tauri_commands.rs:37-60`
- Modify: `ui/src/lib/types.ts` (find existing `GetSettingsResponse` / `PatchSettingsInput`)
- Test: extend an existing tauri_commands test or add a new one

### - [ ] Step 1.1: Add the field to `UserSettings`

In `src-tauri/src/settings.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserSettings {
    pub language: String,
    pub theme: String,
    /// Optional monthly budget in USD. None disables budget alerts.
    /// Persisted in `config.json`. Default: None.
    #[serde(default)]
    pub monthly_budget_usd: Option<f64>,
}

impl Default for UserSettings {
    fn default() -> Self {
        Self {
            language: "en".to_string(),
            theme: "light".to_string(),
            monthly_budget_usd: None,
        }
    }
}
```

The `#[serde(default)]` on the field is critical — without it, loading an old `config.json` (missing the field) would fail deserialization. With it, missing → `None`.

### - [ ] Step 1.2: Extend `GetSettingsResponse` and `PatchSettingsInput`

In `src-tauri/src/ipc.rs`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetSettingsResponse {
    pub language: String,
    pub theme: String,
    pub config_path: String,
    pub data_path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub monthly_budget_usd: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchSettingsInput {
    pub language: Option<String>,
    pub theme: Option<String>,
    /// When provided, sets the monthly budget. To CLEAR the budget, send `Some(None)` —
    /// represented on the wire as `monthlyBudgetUsd: null`. To leave unchanged, omit the
    /// field entirely (which deserializes to `None` here at the outer Option layer).
    #[serde(default, deserialize_with = "deserialize_option_option")]
    pub monthly_budget_usd: Option<Option<f64>>,
}

// Helper at the bottom of the file (or in a util module) to disambiguate
// "field absent" (outer None) vs "field present and null" (Some(None)).
fn deserialize_option_option<'de, D>(d: D) -> Result<Option<Option<f64>>, D::Error>
where D: serde::Deserializer<'de>
{
    Ok(Some(Option::<f64>::deserialize(d)?))
}
```

Why `Option<Option<f64>>`: lets the frontend distinguish "don't touch this field" (absent) from "clear the budget" (null). This is the standard JSON patch idiom.

### - [ ] Step 1.3: Update `get_settings` and `patch_settings`

In `src-tauri/src/tauri_commands.rs`:

```rust
#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<GetSettingsResponse, Error> {
    let settings = state.settings.read().await;
    Ok(GetSettingsResponse {
        language: settings.language.clone(),
        theme: settings.theme.clone(),
        config_path: state.config_path.to_string_lossy().into(),
        data_path: state.data_dir.to_string_lossy().into(),
        monthly_budget_usd: settings.monthly_budget_usd,
    })
}

#[tauri::command]
pub async fn patch_settings(state: State<'_, AppState>, input: PatchSettingsInput) -> Result<GetSettingsResponse, Error> {
    let mut settings = state.settings.write().await;
    if let Some(lang) = input.language {
        settings.language = lang;
    }
    if let Some(theme) = input.theme {
        settings.theme = theme;
    }
    // Outer Some = field was present in the JSON; inner is the new value (or None to clear).
    if let Some(budget) = input.monthly_budget_usd {
        // Clamp negatives to None — the input form should already prevent this but
        // belt-and-suspenders for IPC robustness.
        settings.monthly_budget_usd = budget.filter(|&b| b > 0.0);
    }
    settings.save(&state.config_path)?;
    drop(settings);
    get_settings(state).await
}
```

### - [ ] Step 1.4: Write a Rust unit test

Append to `src-tauri/src/tauri_commands.rs` test module (or create one if missing — there's already `search_workspace_tests` from Phase 6-B which can be your reference):

```rust
#[cfg(test)]
mod settings_budget_tests {
    use crate::settings::UserSettings;

    #[test]
    fn user_settings_default_has_no_budget() {
        let s = UserSettings::default();
        assert_eq!(s.monthly_budget_usd, None);
    }

    #[test]
    fn user_settings_roundtrips_through_json() {
        let s = UserSettings {
            language: "en".into(),
            theme: "light".into(),
            monthly_budget_usd: Some(50.0),
        };
        let json = serde_json::to_string(&s).unwrap();
        let s2: UserSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(s2.monthly_budget_usd, Some(50.0));
    }

    #[test]
    fn user_settings_loads_legacy_config_without_field() {
        // Simulate an old config.json that pre-dates the new field.
        let legacy = r#"{"language":"en","theme":"light"}"#;
        let s: UserSettings = serde_json::from_str(legacy).unwrap();
        assert_eq!(s.monthly_budget_usd, None);
    }
}
```

### - [ ] Step 1.5: Run the test

```bash
cd src-tauri && cargo test --lib settings_budget_tests 2>&1 | tail -10
```
Expected: 3 passing tests.

### - [ ] Step 1.6: Build the full backend

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors. (Adding fields can break callsites that destructure the old shape — fix any that surface.)

### - [ ] Step 1.7: Update frontend types

In `ui/src/lib/types.ts`, find the existing `GetSettingsResponse` interface and add:

```ts
export interface GetSettingsResponse {
  language: string;
  theme: string;
  configPath: string;
  dataPath: string;
  monthlyBudgetUsd?: number | null;  // null = no budget set
}

export interface PatchSettingsInput {
  language?: string;
  theme?: string;
  /** Send `null` to clear; omit to leave unchanged. */
  monthlyBudgetUsd?: number | null;
}
```

(If the existing types are camelCase, match that. The Rust struct uses `#[serde(rename_all = "camelCase")]` so on-the-wire is camelCase already.)

### - [ ] Step 1.8: TypeScript check + frontend tests

```bash
cd ui && npx tsc --noEmit 2>&1 | head -10
cd ui && npm test -- --run 2>&1 | tail -5
```
Expected: 0 errors, all tests pass.

### - [ ] Step 1.9: Commit

```bash
git add src-tauri/src/settings.rs src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs ui/src/lib/types.ts
git commit -m "$(cat <<'EOF'
feat(settings): monthlyBudgetUsd field on UserSettings

Adds an optional monthly budget (USD) to UserSettings, round-tripped
through get_settings / patch_settings. Stored in config.json.

PatchSettingsInput uses Option<Option<f64>> so the frontend can
distinguish "field absent" (no-op) from "field present and null"
(clear the budget). Default None — no budget alerts unless set.

Legacy config.json without the field deserializes fine via
#[serde(default)] (test added).
EOF
)"
```

---

## Task 2: Backend — two new cost commands

**Files:**
- Modify: `src-tauri/src/ipc.rs` — add `WorkspaceCostRollup` type
- Modify: `src-tauri/src/tauri_commands.rs` — add `list_workspace_cost_rollup` and `get_month_cost_total`
- Modify: `src-tauri/src/main.rs` — register both in `invoke_handler!`
- Test: extend cost test module

### - [ ] Step 2.1: Add the IPC type

In `src-tauri/src/ipc.rs`, near the other cost rollup types (around line 866):

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceCostRollup {
    pub workspace_id: String,
    pub workspace_name: String,
    pub workspace_icon: String,
    pub total_cost_usd: f64,
    pub total_tokens: i64,
}
```

### - [ ] Step 2.2: Write the failing tests for both new commands

Append to the test module in `tauri_commands.rs`:

```rust
#[cfg(test)]
mod cost_rollup_tests {
    use rusqlite::Connection;
    use crate::db::migrations::run_migrations;

    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().expect("open in-memory db");
        run_migrations(&mut conn).expect("run migrations");
        conn
    }

    /// Insert helper.
    fn insert_session(conn: &Connection, id: &str, space_id: &str, title: &str) {
        conn.execute(
            "INSERT INTO agent_sessions (id, space_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 0)",
            rusqlite::params![id, space_id, title],
        ).unwrap();
    }
    fn insert_workspace(conn: &Connection, id: &str, name: &str) {
        conn.execute(
            "INSERT INTO spaces (id, name, icon, path, attached_dirs,
                                 sort_order, created_at, updated_at)
             VALUES (?1, ?2, 'Folder', '/x', '[]', 0, '0', '0')",
            rusqlite::params![id, name],
        ).unwrap();
    }
    fn insert_cost(conn: &Connection, session_id: &str, model: &str, cost: f64, ts: i64) {
        let id = uuid::Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO cost_records (id, session_id, model, input_tokens, output_tokens, cost_usd, created_at)
             VALUES (?1, ?2, ?3, 100, 50, ?4, ?5)",
            rusqlite::params![id, session_id, model, cost, ts],
        ).unwrap();
    }

    #[test]
    fn workspace_rollup_groups_costs_by_space() {
        let conn = setup_db();
        insert_workspace(&conn, "ws-a", "Alpha");
        insert_workspace(&conn, "ws-b", "Beta");
        insert_session(&conn, "s1", "ws-a", "");
        insert_session(&conn, "s2", "ws-a", "");
        insert_session(&conn, "s3", "ws-b", "");
        insert_cost(&conn, "s1", "claude-x", 1.0, 1000);
        insert_cost(&conn, "s2", "claude-x", 2.0, 2000);
        insert_cost(&conn, "s3", "claude-x", 0.5, 1500);

        // Run the SAME SQL the production command will run.
        let mut stmt = conn.prepare(
            "SELECT s.space_id, COALESCE(sp.name, ''), COALESCE(sp.icon, 'Folder'),
                    SUM(c.cost_usd), SUM(c.input_tokens + c.output_tokens)
             FROM cost_records c
             JOIN agent_sessions s ON c.session_id = s.id
             LEFT JOIN spaces sp ON sp.id = s.space_id
             WHERE c.created_at >= ?1
             GROUP BY s.space_id
             ORDER BY SUM(c.cost_usd) DESC"
        ).unwrap();
        let rows: Vec<(String, String, String, f64, i64)> = stmt
            .query_map([500i64], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?))
            }).unwrap()
            .filter_map(Result::ok)
            .collect();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].0, "ws-a");
        assert!((rows[0].3 - 3.0).abs() < 0.01);
        assert_eq!(rows[0].4, 450); // (100+50)*3
        assert_eq!(rows[1].0, "ws-b");
        assert!((rows[1].3 - 0.5).abs() < 0.01);
    }

    #[test]
    fn workspace_rollup_filters_by_since_ms() {
        let conn = setup_db();
        insert_workspace(&conn, "ws-a", "Alpha");
        insert_session(&conn, "s1", "ws-a", "");
        insert_cost(&conn, "s1", "claude-x", 1.0, 500);   // before cutoff
        insert_cost(&conn, "s1", "claude-x", 2.0, 1500);  // after cutoff

        let mut stmt = conn.prepare(
            "SELECT SUM(c.cost_usd)
             FROM cost_records c
             JOIN agent_sessions s ON c.session_id = s.id
             WHERE c.created_at >= ?1"
        ).unwrap();
        let total: f64 = stmt.query_row([1000i64], |r| r.get(0)).unwrap();
        assert!((total - 2.0).abs() < 0.01);
    }

    #[test]
    fn workspace_rollup_returns_empty_for_no_records() {
        let conn = setup_db();
        let mut stmt = conn.prepare(
            "SELECT COUNT(*) FROM cost_records c WHERE c.created_at >= ?1"
        ).unwrap();
        let count: i64 = stmt.query_row([0i64], |r| r.get(0)).unwrap();
        assert_eq!(count, 0);
    }

    #[test]
    fn month_total_sums_recent_records() {
        let conn = setup_db();
        insert_workspace(&conn, "ws-a", "Alpha");
        insert_session(&conn, "s1", "ws-a", "");
        insert_cost(&conn, "s1", "x", 1.0, 1000);
        insert_cost(&conn, "s1", "x", 2.0, 2000);
        insert_cost(&conn, "s1", "x", 4.0, 500);  // before cutoff

        let total: f64 = conn.query_row(
            "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
            [800i64], |r| r.get(0),
        ).unwrap();
        assert!((total - 3.0).abs() < 0.01);
    }
}
```

### - [ ] Step 2.3: Run the tests — they should pass (verify schema)

```bash
cd src-tauri && cargo test --lib cost_rollup_tests 2>&1 | tail -10
```
Expected: 4 passing.

### - [ ] Step 2.4: Implement `list_workspace_cost_rollup`

In `src-tauri/src/tauri_commands.rs`, near the other cost commands (after `get_session_costs` around line 720):

```rust
/// Sum cost_records for the current month, grouped by workspace.
/// `since_ms` is the start of the current month in user-local time
/// (computed in the frontend — keeps timezone logic out of Rust).
#[tauri::command]
pub async fn list_workspace_cost_rollup(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<Vec<WorkspaceCostRollup>, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let mut stmt = conn.prepare(
        "SELECT
             s.space_id AS workspace_id,
             COALESCE(sp.name, '默认工作区') AS workspace_name,
             COALESCE(sp.icon, 'Folder') AS workspace_icon,
             COALESCE(SUM(c.cost_usd), 0) AS total_cost_usd,
             COALESCE(SUM(c.input_tokens + c.output_tokens), 0) AS total_tokens
         FROM cost_records c
         JOIN agent_sessions s ON c.session_id = s.id
         LEFT JOIN spaces sp ON sp.id = s.space_id
         WHERE c.created_at >= ?1
         GROUP BY s.space_id
         ORDER BY total_cost_usd DESC"
    ).map_err(|e| Error::Internal(format!("prepare workspace rollup: {}", e)))?;
    let rows = stmt.query_map(rusqlite::params![since_ms], |row| {
        Ok(WorkspaceCostRollup {
            workspace_id: row.get(0)?,
            workspace_name: row.get(1)?,
            workspace_icon: row.get(2)?,
            total_cost_usd: row.get(3)?,
            total_tokens: row.get(4)?,
        })
    }).map_err(|e| Error::Internal(format!("workspace rollup query: {}", e)))?;
    Ok(rows.filter_map(Result::ok).collect())
}

/// Sum of cost_records.cost_usd where created_at >= since_ms.
/// Returns 0.0 when no records.
#[tauri::command]
pub async fn get_month_cost_total(
    state: State<'_, AppState>,
    since_ms: i64,
) -> Result<f64, Error> {
    let conn = state.db.lock().map_err(|e| Error::Internal(format!("DB lock: {}", e)))?;
    let total: f64 = conn.query_row(
        "SELECT COALESCE(SUM(cost_usd), 0) FROM cost_records WHERE created_at >= ?1",
        rusqlite::params![since_ms],
        |row| row.get(0),
    ).map_err(|e| Error::Internal(format!("month total query: {}", e)))?;
    Ok(total)
}
```

Also add to the imports at the top of `tauri_commands.rs` if `WorkspaceCostRollup` isn't already imported via the existing `use crate::ipc::{...}` line:

```rust
use crate::ipc::{
    DailyCostRollup, ModelCostRollup, SessionCostRollup, WorkspaceCostRollup,
    PermissionRule, PermissionAuditEntry, CreatePermissionRuleInput,
};
```

### - [ ] Step 2.5: Register both commands in `main.rs`

In `src-tauri/src/main.rs`, find the existing `tauri::generate_handler!` (or equivalent) macro listing all commands. Add both new entries alongside the other cost commands:

```rust
uclaw_core::tauri_commands::get_daily_costs,
uclaw_core::tauri_commands::get_model_costs,
uclaw_core::tauri_commands::get_session_costs,
uclaw_core::tauri_commands::list_workspace_cost_rollup,  // NEW
uclaw_core::tauri_commands::get_month_cost_total,        // NEW
```

(Grep `get_daily_costs` in `main.rs` to find the right place; add right after.)

### - [ ] Step 2.6: Build the backend

```bash
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 0 errors.

### - [ ] Step 2.7: Commit

```bash
git add src-tauri/src/ipc.rs src-tauri/src/tauri_commands.rs src-tauri/src/main.rs
git commit -m "$(cat <<'EOF'
feat(cost): list_workspace_cost_rollup + get_month_cost_total commands

Two new Tauri commands powering the Phase 6-C dashboard additions:

- list_workspace_cost_rollup(since_ms): SUM of cost_records grouped by
  agent_sessions.space_id, joined with spaces for name/icon. Used by
  the per-workspace section.
- get_month_cost_total(since_ms): SUM(cost_usd) for the current month.
  Used by the budget header.

since_ms is the month-start in user-local time, computed frontend-side
so Rust stays timezone-agnostic. V13's idx_cost_records_created keeps
both queries fast.
EOF
)"
```

---

## Task 3: Backend — emit `budget:threshold` event on crossings

**Files:**
- Modify: `src-tauri/src/ipc.rs` — add `BudgetThresholdPayload` type
- Modify: `src-tauri/src/agent/dispatcher.rs` — extend `emit_turn_cost`
- Modify: `src-tauri/src/cost_store.rs` — refactor `record` to return the inserted `cost_usd` and provide a `monthly_total` helper, OR fold the SUM into dispatcher (decide below)

### - [ ] Step 3.1: Add `BudgetThresholdPayload`

In `src-tauri/src/ipc.rs`, near the cost types:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetThresholdPayload {
    /// 80 or 100. The threshold percentage that was crossed.
    pub threshold: u8,
    /// Current monthly total in USD.
    pub current: f64,
    /// Configured monthly budget in USD.
    pub budget: f64,
}
```

### - [ ] Step 3.2: Add a `monthly_total` helper in `cost_store.rs`

Append to `src-tauri/src/cost_store.rs`:

```rust
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
/// Matches the frontend's monthStartMsAtom semantics closely enough for
/// the threshold check (within a few hours of timezone drift, which
/// doesn't matter for "did we cross 80%").
pub fn current_month_start_ms() -> i64 {
    use chrono::{Datelike, TimeZone, Utc};
    let now = Utc::now();
    Utc.with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}
```

### - [ ] Step 3.3: Extend `emit_turn_cost` to check threshold crossings

In `src-tauri/src/agent/dispatcher.rs`, find `emit_turn_cost` (around line 231) and add the threshold logic AFTER the existing `cost_store::record` call and BEFORE the existing `app_handle.emit("agent:turn_cost", ...)`:

```rust
fn emit_turn_cost(&self, usage: &TokenUsage) {
    let cost = calculate_cost(&self.model, usage.input_tokens, usage.output_tokens);
    let turn_cost = TurnCostInfo {
        input_tokens: usage.input_tokens,
        output_tokens: usage.output_tokens,
        cost_usd: format_cost(cost),
    };
    tracing::info!(
        input_tokens = usage.input_tokens,
        output_tokens = usage.output_tokens,
        cost_usd = %turn_cost.cost_usd,
        "Emitting agent:turn_cost"
    );

    use tauri::Manager;
    if let Some(state) = self.app_handle.try_state::<crate::app::AppState>() {
        crate::cost_store::record(
            &state,
            &self.conversation_id,
            &self.model,
            usage.input_tokens,
            usage.output_tokens,
        );

        // Phase 6-C: budget threshold check. Skip when no budget set.
        if let Some(budget) = futures::executor::block_on(state.settings.read()).monthly_budget_usd {
            if budget > 0.0 {
                let month_start = crate::cost_store::current_month_start_ms();
                let total_after = crate::cost_store::monthly_total(&state, month_start);
                let total_before = (total_after - cost).max(0.0);

                let crossed_80 = total_before / budget < 0.80 && total_after / budget >= 0.80;
                let crossed_100 = total_before / budget < 1.00 && total_after / budget >= 1.00;

                if crossed_100 {
                    let _ = self.app_handle.emit("budget:threshold", crate::ipc::BudgetThresholdPayload {
                        threshold: 100,
                        current: total_after,
                        budget,
                    });
                } else if crossed_80 {
                    let _ = self.app_handle.emit("budget:threshold", crate::ipc::BudgetThresholdPayload {
                        threshold: 80,
                        current: total_after,
                        budget,
                    });
                }
            }
        }
    }

    let _ = self.app_handle.emit("agent:turn_cost", serde_json::json!({
        "conversationId": self.conversation_id,
        // ... existing payload unchanged ...
    }));
}
```

**Note on the `futures::executor::block_on`**: this is called from a sync context inside the dispatcher, and `state.settings` is `tokio::sync::RwLock`. If the dispatcher already has a different settings-read pattern, use that instead. Grep for existing `state.settings.read()` callsites in `dispatcher.rs` to match the local idiom. If `futures` isn't already a dep, fall back to `tokio::runtime::Handle::current().block_on(state.settings.read())` (the dispatcher runs inside the Tauri async runtime, so this should work).

### - [ ] Step 3.4: Write a unit test for threshold detection

Append to `src-tauri/src/cost_store.rs`:

```rust
#[cfg(test)]
mod tests {
    /// Pure helper that mirrors the threshold-crossing logic in emit_turn_cost.
    /// Extracted for testability — fires exactly once per crossing.
    fn fired_threshold(total_before: f64, total_after: f64, budget: f64) -> Option<u8> {
        if budget <= 0.0 { return None; }
        let crossed_100 = total_before / budget < 1.00 && total_after / budget >= 1.00;
        let crossed_80 = total_before / budget < 0.80 && total_after / budget >= 0.80;
        if crossed_100 { Some(100) }
        else if crossed_80 { Some(80) }
        else { None }
    }

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
        // Lowered budget mid-month: single turn pushes from <80% to >100%.
        assert_eq!(fired_threshold(50.0, 150.0, 100.0), Some(100));
    }

    #[test]
    fn no_fire_when_budget_zero_or_negative() {
        assert_eq!(fired_threshold(50.0, 150.0, 0.0), None);
        assert_eq!(fired_threshold(50.0, 150.0, -10.0), None);
    }
}
```

The test exercises the pure logic; the actual production callsite in `emit_turn_cost` inlines the same comparison. If you want to keep DRY, extract the helper to `pub(crate) fn`-level and call it from `emit_turn_cost`.

### - [ ] Step 3.5: Run tests + build

```bash
cd src-tauri && cargo test --lib cost_store 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: 6 passing tests, 0 build errors.

### - [ ] Step 3.6: Commit

```bash
git add src-tauri/src/ipc.rs src-tauri/src/cost_store.rs src-tauri/src/agent/dispatcher.rs
git commit -m "$(cat <<'EOF'
feat(cost): emit budget:threshold event on 80% and 100% crossings

After each cost_store::record, if a monthly budget is set and the
new monthly total CROSSED 80% or 100% on this turn, emit a
budget:threshold event with the threshold + current + budget values.

Crossings are evaluated as "total_before < threshold AND total_after
>= threshold" — fires exactly once per crossing, never re-fires while
the total stays above the threshold. If a single turn jumps past both
80% and 100% (e.g. budget lowered mid-month), only 100% fires.

Six unit tests in cost_store::tests cover the threshold logic.
Frontend listener + toast come in a later commit.
EOF
)"
```

---

## Task 4: Frontend — cost atoms + bridge wrappers

**Files:**
- Create: `ui/src/atoms/cost.ts`
- Create: `ui/src/atoms/cost.test.ts`
- Modify: `ui/src/lib/tauri-bridge.ts`
- Modify: `ui/src/lib/types.ts`

### - [ ] Step 4.1: Add TS types

In `ui/src/lib/types.ts`, append (near the existing `DailyCostRollup` etc.):

```ts
export interface WorkspaceCostRollup {
  workspaceId: string;
  workspaceName: string;
  workspaceIcon: string;
  totalCostUsd: number;
  totalTokens: number;
}

export interface BudgetThresholdPayload {
  threshold: 80 | 100;
  current: number;
  budget: number;
}
```

### - [ ] Step 4.2: Add bridge wrappers

In `ui/src/lib/tauri-bridge.ts`:

```ts
import type { WorkspaceCostRollup, BudgetThresholdPayload } from './types'
// ... existing imports ...

export const listWorkspaceCostRollup = (sinceMs: number): Promise<WorkspaceCostRollup[]> =>
  invoke('list_workspace_cost_rollup', { sinceMs })

export const getMonthCostTotal = (sinceMs: number): Promise<number> =>
  invoke('get_month_cost_total', { sinceMs })

export const onBudgetThreshold = (cb: (payload: BudgetThresholdPayload) => void): Promise<() => void> =>
  listen('budget:threshold', ({ payload }) => cb(payload as BudgetThresholdPayload))
```

(Find the existing `listen` import or the pattern that `onTurnCost` uses — match that.)

### - [ ] Step 4.3: Write the failing atom tests

Create `ui/src/atoms/cost.test.ts`:

```ts
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { createStore } from 'jotai'
import {
  monthStartMsAtom,
  monthTotalUsdAtom,
  workspaceRollupAtom,
  firedBudgetThresholdsAtom,
  refreshCostsAtom,
} from './cost'

vi.mock('@/lib/tauri-bridge', () => ({
  getMonthCostTotal: vi.fn().mockResolvedValue(42.5),
  listWorkspaceCostRollup: vi.fn().mockResolvedValue([
    { workspaceId: 'ws-a', workspaceName: 'A', workspaceIcon: 'Folder', totalCostUsd: 30, totalTokens: 1000 },
    { workspaceId: 'ws-b', workspaceName: 'B', workspaceIcon: 'Folder', totalCostUsd: 12.5, totalTokens: 500 },
  ]),
}))

describe('cost atoms', () => {
  beforeEach(() => { vi.clearAllMocks() })

  it('monthStartMsAtom returns first-of-month midnight in local time', () => {
    const store = createStore()
    const ms = store.get(monthStartMsAtom)
    const d = new Date(ms)
    expect(d.getDate()).toBe(1)
    expect(d.getHours()).toBe(0)
    expect(d.getMinutes()).toBe(0)
    expect(d.getSeconds()).toBe(0)
    // Sanity check: it's the current month
    const now = new Date()
    expect(d.getMonth()).toBe(now.getMonth())
    expect(d.getFullYear()).toBe(now.getFullYear())
  })

  it('refreshCostsAtom fetches and writes both atoms', async () => {
    const store = createStore()
    expect(store.get(monthTotalUsdAtom)).toBe(null)
    expect(store.get(workspaceRollupAtom)).toEqual([])

    await store.set(refreshCostsAtom)

    expect(store.get(monthTotalUsdAtom)).toBe(42.5)
    expect(store.get(workspaceRollupAtom)).toHaveLength(2)
    expect(store.get(workspaceRollupAtom)[0].workspaceId).toBe('ws-a')
  })

  it('firedBudgetThresholdsAtom defaults to empty Set', () => {
    const store = createStore()
    const fired = store.get(firedBudgetThresholdsAtom)
    expect(fired).toBeInstanceOf(Set)
    expect(fired.size).toBe(0)
  })

  it('firedBudgetThresholdsAtom accepts adding thresholds', () => {
    const store = createStore()
    store.set(firedBudgetThresholdsAtom, new Set([80 as const]))
    expect(store.get(firedBudgetThresholdsAtom).has(80)).toBe(true)
    expect(store.get(firedBudgetThresholdsAtom).has(100)).toBe(false)
  })
})
```

### - [ ] Step 4.4: Run the tests — verify they fail (module doesn't exist yet)

```bash
cd ui && npm test -- --run atoms/cost 2>&1 | tail -10
```
Expected: FAIL with "Cannot find module './cost'".

### - [ ] Step 4.5: Implement the atoms

Create `ui/src/atoms/cost.ts`:

```ts
import { atom } from 'jotai'
import {
  getMonthCostTotal,
  listWorkspaceCostRollup,
} from '@/lib/tauri-bridge'
import type { WorkspaceCostRollup } from '@/lib/types'

/**
 * Start of the current month at local midnight, in epoch ms.
 * Computed once per atom-read (stable through a session). If the user
 * keeps the app open across the 1st of a new month, this stays on the
 * previous month until next read — acceptable for a usage tracker.
 */
export const monthStartMsAtom = atom<number>(() => {
  const now = new Date()
  return new Date(now.getFullYear(), now.getMonth(), 1).getTime()
})

/** Month-to-date total in USD. `null` until first refresh. */
export const monthTotalUsdAtom = atom<number | null>(null)

/** Per-workspace rollup for the current month, sorted by spend desc. */
export const workspaceRollupAtom = atom<WorkspaceCostRollup[]>([])

/**
 * Which budget thresholds have already fired this session. Resets on
 * app restart — intentional, so a fresh boot in a new month can
 * re-alert if still over budget.
 */
export const firedBudgetThresholdsAtom = atom<Set<80 | 100>>(new Set())

/** Refresh both monthly atoms in parallel. */
export const refreshCostsAtom = atom(null, async (get, set) => {
  const since = get(monthStartMsAtom)
  const [total, rollup] = await Promise.all([
    getMonthCostTotal(since),
    listWorkspaceCostRollup(since),
  ])
  set(monthTotalUsdAtom, total)
  set(workspaceRollupAtom, rollup)
})
```

### - [ ] Step 4.6: Run the tests to verify they pass

```bash
cd ui && npm test -- --run atoms/cost 2>&1 | tail -15
```
Expected: 4 passing.

### - [ ] Step 4.7: Commit

```bash
git add ui/src/atoms/cost.ts ui/src/atoms/cost.test.ts ui/src/lib/tauri-bridge.ts ui/src/lib/types.ts
git commit -m "$(cat <<'EOF'
feat(atoms): cost atoms for monthly budget + per-workspace rollup

Five atoms in ui/src/atoms/cost.ts:
- monthStartMsAtom: first-of-month at local midnight (epoch ms)
- monthTotalUsdAtom: month-to-date total (null until first refresh)
- workspaceRollupAtom: per-workspace rollup for current month
- firedBudgetThresholdsAtom: Set<80|100> for client-side dedup
- refreshCostsAtom: parallel fetch + state update

Bridge wrappers added for the two new Tauri commands; onBudgetThreshold
wraps the budget:threshold event listener.

Four unit tests cover the month-start computation, the refresh action,
and the dedup Set's default + mutation.
EOF
)"
```

---

## Task 5: Frontend UI — BudgetHeader + per-workspace section in UsageSettings

**Files:**
- Modify: `ui/src/components/settings/UsageSettings.tsx` — prepend new sections to the existing render
- Modify: `ui/src/components/settings/SettingsPanel.tsx` — relabel tab "用量" → "用量与预算"
- Test: extend `ui/src/components/settings/UsageSettings.test.tsx` if it exists, or add a focused new test file

### - [ ] Step 5.1: Read the existing `UsageSettings.tsx` to understand its structure

```bash
cd /Users/ryanliu/Documents/uclaw
cat ui/src/components/settings/UsageSettings.tsx | head -50
```

You'll see it already has top-level `<div>` wrapping three chart sections. New work goes at the **top** of that wrapper.

### - [ ] Step 5.2: Check whether a settings atom for `monthlyBudgetUsd` exists

```bash
grep -rn "monthlyBudgetUsd\|monthly_budget" ui/src/atoms/ ui/src/components/settings/ | head
```

If the codebase has a `settingsAtom` that already exposes the full Settings shape, use it. If not, add a minimal `monthlyBudgetUsdAtom`:

```ts
// In ui/src/atoms/cost.ts, add:
import { getSettings, patchSettings } from '@/lib/tauri-bridge'

export const monthlyBudgetUsdAtom = atom<number | null>(null)

/** One-shot loader: fetch from backend and seed the atom. */
export const loadBudgetAtom = atom(null, async (_get, set) => {
  const s = await getSettings()
  set(monthlyBudgetUsdAtom, s.monthlyBudgetUsd ?? null)
})

/** Patch the budget on the backend AND update the atom. */
export const setBudgetAtom = atom(null, async (_get, set, value: number | null) => {
  await patchSettings({ monthlyBudgetUsd: value })
  set(monthlyBudgetUsdAtom, value)
})
```

(Verify `getSettings` / `patchSettings` exist in `tauri-bridge.ts`. If they don't, add thin wrappers.)

Update `ui/src/atoms/cost.test.ts` with two more tests:

```ts
import { getSettings, patchSettings } from '@/lib/tauri-bridge'

vi.mock('@/lib/tauri-bridge', () => ({
  getMonthCostTotal: vi.fn().mockResolvedValue(42.5),
  listWorkspaceCostRollup: vi.fn().mockResolvedValue([]),
  getSettings: vi.fn().mockResolvedValue({
    language: 'en', theme: 'light',
    configPath: '/x', dataPath: '/y',
    monthlyBudgetUsd: 100,
  }),
  patchSettings: vi.fn().mockResolvedValue({}),
}))

// Inside the existing describe block:
it('loadBudgetAtom hydrates monthlyBudgetUsdAtom from backend', async () => {
  const store = createStore()
  await store.set(loadBudgetAtom)
  expect(store.get(monthlyBudgetUsdAtom)).toBe(100)
})

it('setBudgetAtom calls patchSettings and updates atom', async () => {
  const store = createStore()
  await store.set(setBudgetAtom, 250)
  expect(patchSettings).toHaveBeenCalledWith({ monthlyBudgetUsd: 250 })
  expect(store.get(monthlyBudgetUsdAtom)).toBe(250)
})
```

Run: `cd ui && npm test -- --run atoms/cost 2>&1 | tail -15`. Expected: 6 passing.

### - [ ] Step 5.3: Build the `BudgetHeader` inline in UsageSettings.tsx

Inline because it's a small, single-use component. If it grows beyond 80 lines, split it out — but start inline.

In `ui/src/components/settings/UsageSettings.tsx`, **add at the top** of the file (after imports):

```tsx
import { useAtomValue, useSetAtom } from 'jotai'
import {
  monthTotalUsdAtom,
  workspaceRollupAtom,
  monthlyBudgetUsdAtom,
  refreshCostsAtom,
  loadBudgetAtom,
  setBudgetAtom,
} from '@/atoms/cost'
import { getWorkspaceIcon } from '@/lib/workspace-icons'
import { Folder } from 'lucide-react'

function formatUsdShort(v: number): string {
  if (v < 0.01) return `$${v.toFixed(4)}`
  if (v < 1) return `$${v.toFixed(3)}`
  return `$${v.toFixed(2)}`
}

function BudgetHeader({
  total, budget, onSave,
}: {
  total: number
  budget: number | null
  onSave: (v: number | null) => void
}): React.ReactElement {
  const [editing, setEditing] = React.useState(false)
  const [draft, setDraft] = React.useState<string>(budget?.toString() ?? '')

  if (budget == null) {
    return (
      <div className="rounded-xl border border-border/60 bg-card p-4">
        <div className="text-[12.5px] font-medium text-foreground/80">本月已使用 {formatUsdShort(total)}</div>
        <div className="mt-1 text-[11px] text-muted-foreground/70">设置月度预算后，达到 80% / 100% 会收到提醒。</div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              const v = parseFloat(draft)
              if (Number.isFinite(v) && v > 0) {
                onSave(v)
                setEditing(false)
              }
            }}
            className="mt-3 flex items-center gap-2"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="1"
              className="w-24 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
            />
            <button type="submit" className="rounded-md bg-primary px-2.5 py-1 text-[11.5px] text-primary-foreground hover:bg-primary/90">
              保存
            </button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">
              取消
            </button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => setEditing(true)}
            className="mt-3 rounded-md border border-dashed border-border/70 bg-transparent px-3 py-1.5 text-[11.5px] text-muted-foreground/85 hover:border-primary/50 hover:text-foreground/90"
          >
            设置月度预算
          </button>
        )}
      </div>
    )
  }

  const pct = Math.min(total / budget, 1.5) // cap visual at 150% so the bar doesn't fly off
  const isOver = total > budget
  const isWarn = !isOver && total / budget >= 0.8

  return (
    <div className="rounded-xl border border-border/60 bg-card p-4">
      <div className="flex items-baseline justify-between">
        <div>
          <div className="text-[14px] font-semibold text-foreground/90">本月用量</div>
          <div className="mt-0.5 text-[11.5px] text-muted-foreground/70">
            {formatUsdShort(total)} / {formatUsdShort(budget)} ·{' '}
            <span className={isOver ? 'text-destructive font-medium' : isWarn ? 'text-amber-500 font-medium' : ''}>
              {Math.round((total / budget) * 100)}%
            </span>
          </div>
        </div>
        {editing ? (
          <form
            onSubmit={(e) => {
              e.preventDefault()
              const v = parseFloat(draft)
              if (draft === '') { onSave(null); setEditing(false); return }
              if (Number.isFinite(v) && v > 0) { onSave(v); setEditing(false) }
            }}
            className="flex items-center gap-1.5"
          >
            <span className="text-[12px] text-muted-foreground/80">$</span>
            <input
              autoFocus
              value={draft}
              onChange={(e) => setDraft(e.target.value)}
              type="number" min="0" step="1"
              className="w-20 rounded-md border border-border/60 bg-background px-2 py-1 text-[12.5px] outline-none focus:border-primary"
              placeholder="预算"
            />
            <button type="submit" className="text-[11.5px] text-primary hover:underline">保存</button>
            <button type="button" onClick={() => setEditing(false)} className="text-[11.5px] text-muted-foreground/70 hover:text-foreground/80">×</button>
          </form>
        ) : (
          <button
            type="button"
            onClick={() => { setDraft(budget.toString()); setEditing(true) }}
            className="text-[11px] text-muted-foreground/70 hover:text-foreground/80 underline-offset-2 hover:underline"
          >
            修改预算
          </button>
        )}
      </div>
      <div className="mt-3 h-2 w-full overflow-hidden rounded-full bg-muted">
        <div
          className={`h-full rounded-full transition-all duration-500 ${
            isOver ? 'bg-destructive' : isWarn ? 'bg-amber-500' : 'bg-primary'
          }`}
          style={{ width: `${(pct / 1.5) * 100}%` }}
        />
      </div>
    </div>
  )
}

function WorkspaceRollupSection({ items }: { items: WorkspaceCostRollup[] }): React.ReactElement | null {
  if (items.length === 0) return null
  const max = Math.max(...items.map((i) => i.totalCostUsd), 0.0001)
  return (
    <section>
      <h3 className="mb-2 text-[12px] font-semibold uppercase tracking-wide text-muted-foreground/80">按工作区（本月）</h3>
      <div className="space-y-1.5">
        {items.map((r) => {
          const Icon = getWorkspaceIcon(r.workspaceIcon)
          return (
            <div key={r.workspaceId} className="flex items-center gap-2.5 rounded-md border border-border/40 bg-card/60 px-3 py-2">
              <span className="inline-flex items-center justify-center size-5 rounded bg-primary/15 text-primary shrink-0">
                <Icon className="size-3.5" />
              </span>
              <span className="flex-1 truncate text-[12.5px] text-foreground/85">{r.workspaceName || '默认工作区'}</span>
              <div className="relative h-1.5 w-24 overflow-hidden rounded-full bg-muted">
                <div className="h-full rounded-full bg-primary/70" style={{ width: `${(r.totalCostUsd / max) * 100}%` }} />
              </div>
              <span className="w-16 shrink-0 text-right text-[12px] tabular-nums text-foreground/80">{formatUsdShort(r.totalCostUsd)}</span>
            </div>
          )
        })}
      </div>
    </section>
  )
}
```

(Import `WorkspaceCostRollup` from `@/lib/types`.)

### - [ ] Step 5.4: Wire them into the `UsageSettings` component body

Find the existing `return (...)` in `UsageSettings`. Right before the existing top-level wrapper's first child, add:

```tsx
const monthTotal = useAtomValue(monthTotalUsdAtom)
const wsRollup = useAtomValue(workspaceRollupAtom)
const budget = useAtomValue(monthlyBudgetUsdAtom)
const refreshCosts = useSetAtom(refreshCostsAtom)
const loadBudget = useSetAtom(loadBudgetAtom)
const saveBudget = useSetAtom(setBudgetAtom)

React.useEffect(() => {
  void refreshCosts()
  void loadBudget()
}, [refreshCosts, loadBudget])

// Re-fetch monthly rollups on each turn_cost event (debounce-piggyback
// on the existing `refetch` listener below — extend it to also call
// refreshCosts).
```

In the existing `onTurnCost` listener (inside the existing `React.useEffect` for live updates), add a call to `refreshCosts` alongside the 30-day `refetch`:

```tsx
const unlistenP = onTurnCost(() => {
  if (timer) clearTimeout(timer)
  timer = setTimeout(() => {
    void refetch()
    void refreshCosts()  // NEW — keep the monthly view live too
  }, 1000)
})
```

In the JSX, **prepend** to the top-level wrapper:

```tsx
return (
  <div className="space-y-6">
    <BudgetHeader total={monthTotal ?? 0} budget={budget} onSave={(v) => void saveBudget(v)} />
    <WorkspaceRollupSection items={wsRollup} />
    {/* ... existing daily / model / session sections unchanged ... */}
  </div>
)
```

### - [ ] Step 5.5: Relabel the settings tab

In `ui/src/components/settings/SettingsPanel.tsx`, find the line:

```tsx
{ id: 'usage', label: '用量', icon: <BarChart3 size={15} /> },
```

Change to:

```tsx
{ id: 'usage', label: '用量与预算', icon: <BarChart3 size={15} /> },
```

### - [ ] Step 5.6: Add a smoke test for the new sections

Check whether `ui/src/components/settings/UsageSettings.test.tsx` exists:

```bash
ls ui/src/components/settings/UsageSettings.test.tsx 2>/dev/null
```

If yes, extend it. If no, **skip writing a test** for this task — UsageSettings uses Recharts which is finicky under jsdom and the existing pattern in this repo is to not test it. The atom tests in Task 4 + manual smoke cover the new behavior.

### - [ ] Step 5.7: Verify build + tests + tsc

```bash
cd /Users/ryanliu/Documents/uclaw/ui
npm test -- --run 2>&1 | tail -5
npx tsc --noEmit 2>&1 | head -10
```
Expected: all tests pass, 0 errors.

### - [ ] Step 5.8: Commit

```bash
git add ui/src/components/settings/UsageSettings.tsx ui/src/components/settings/SettingsPanel.tsx ui/src/atoms/cost.ts ui/src/atoms/cost.test.ts
git commit -m "$(cat <<'EOF'
feat(settings): BudgetHeader + per-workspace section in UsageSettings

Adds two new sections at the top of the 用量 tab (renamed to 用量与预算):

- BudgetHeader: month-to-date spend + progress bar against the
  configured monthly budget. Inline "设置月度预算" form when no
  budget is set; "修改预算" affordance when one is. Bar turns amber
  at >= 80%, destructive at > 100%.
- WorkspaceRollupSection: per-workspace spend for the current month,
  sorted desc, each row shows workspace icon + name + a relative bar
  + the dollar amount.

Existing daily/model/session Recharts views stay unchanged below the
new sections. Live-update piggybacks on the existing onTurnCost
listener (debounced 1s).

monthlyBudgetUsdAtom + loadBudgetAtom + setBudgetAtom added to
ui/src/atoms/cost.ts for the budget round-trip.
EOF
)"
```

---

## Task 6: AppShell — `budget:threshold` toast listener

**Files:**
- Modify: `ui/src/components/app-shell/AppShell.tsx` — add a `React.useEffect` for the listener

### - [ ] Step 6.1: Confirm the existing listener pattern in AppShell

```bash
grep -n "onTurnCost\|onBudget\|listen.*budget" ui/src/components/app-shell/AppShell.tsx | head
```

If AppShell already has effect hooks subscribing to Tauri events (likely yes), follow the same pattern.

### - [ ] Step 6.2: Add imports

In `ui/src/components/app-shell/AppShell.tsx`:

```tsx
import { toast } from 'sonner'
import { onBudgetThreshold } from '@/lib/tauri-bridge'
import { firedBudgetThresholdsAtom } from '@/atoms/cost'
```

If `useAtom`/`useSetAtom` etc. are already imported, reuse those.

### - [ ] Step 6.3: Mount the listener inside the AppShell component

Near the other Tauri-event `useEffect` hooks, add:

```tsx
const [fired, setFired] = useAtom(firedBudgetThresholdsAtom)
const firedRef = React.useRef(fired)
React.useEffect(() => { firedRef.current = fired }, [fired])

React.useEffect(() => {
  let unlisten: (() => void) | undefined
  onBudgetThreshold((payload) => {
    if (firedRef.current.has(payload.threshold)) return
    setFired(new Set([...firedRef.current, payload.threshold]))
    const currentStr = `$${payload.current.toFixed(2)}`
    const budgetStr = `$${payload.budget.toFixed(2)}`
    if (payload.threshold === 80) {
      toast.warning(`本月已使用预算 ${currentStr} / ${budgetStr} (80%)`)
    } else {
      toast.error(`本月已超出预算: ${currentStr} / ${budgetStr}。AI 调用将继续，但请留意。`)
    }
  }).then((fn) => { unlisten = fn })
  return () => { unlisten?.() }
}, [setFired])
```

The `firedRef` indirection is to avoid re-subscribing every time the Set changes — the listener callback reads via the ref, while the useEffect only depends on the stable setter.

### - [ ] Step 6.4: Run the full test suite

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npm test -- --run 2>&1 | tail -5
```
Expected: all pass.

### - [ ] Step 6.5: TypeScript check

```bash
cd /Users/ryanliu/Documents/uclaw/ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: 0 errors.

### - [ ] Step 6.6: Commit

```bash
git add ui/src/components/app-shell/AppShell.tsx
git commit -m "$(cat <<'EOF'
feat(app-shell): toast on budget:threshold events

Mounts a budget:threshold event listener in AppShell. First crossing
of 80% raises a warning toast; first crossing of 100% raises an error
toast. firedBudgetThresholdsAtom dedupes within a session.

Toast text shows current spend / configured budget (rounded to cents).
Sonner already in the bundle — no new dep.

A firedRef indirection keeps the listener subscription stable
through Set mutations: the effect re-mounts only on app start, not
on every threshold-fire.
EOF
)"
```

---

## Task 7: Smoke + ship

### - [ ] Step 7.1: Final backend tests + build

```bash
cd src-tauri && cargo test --lib 2>&1 | tail -10
cd src-tauri && cargo build 2>&1 | grep -E "^error" | head -5
```
Expected: all tests pass, 0 build errors.

### - [ ] Step 7.2: Final frontend tests + type check

```bash
cd ui && npm test -- --run 2>&1 | tail -5
cd ui && npx tsc --noEmit 2>&1 | head -10
```
Expected: all tests pass, 0 errors.

### - [ ] Step 7.3: Push the branch

```bash
git push -u origin <branch-name>
```

### - [ ] Step 7.4: Open the PR

Title: `feat(cost): monthly budget alerts + per-workspace rollup in Usage tab`

Body: include the 6-commit bisectable table and this smoke plan:

- [ ] Open Settings → 用量与预算. Tab label shows "用量与预算" (not just "用量").
- [ ] BudgetHeader renders. With no budget set, shows "设置月度预算" button + "本月已使用 $X" text.
- [ ] Click "设置月度预算", enter $0.50 (low enough to trigger), save. Header now shows current / 0.50 + progress bar.
- [ ] Send one or two agent turns (each costs ~$0.01-0.05). Watch the bar fill up.
- [ ] When monthly total crosses 80% of $0.50 ($0.40), confirm a warning toast appears.
- [ ] Continue until total crosses $0.50, confirm an error toast appears.
- [ ] Keep going — no more toasts fire (dedup working).
- [ ] WorkspaceRollupSection shows your workspaces sorted by spend.
- [ ] Restart the app — the budget persists; if still over budget after restart, the threshold can re-fire (intentional).
- [ ] Set budget to empty / null via the edit form — alerts disable.
- [ ] Existing daily / model / session Recharts views still render correctly below the new sections.

---

## Self-Review

### Spec coverage

- §2 Goal: Settings → "用量与预算" with 3 sections (month total + per-workspace + per-model) → ✓ Tasks 5 adds month + per-workspace; per-model rollup already exists in `UsageSettings` (preserved, not removed).
- §2 Goal: Monthly budget input → ✓ Task 1 (Settings round-trip) + Task 5 (inline form in BudgetHeader).
- §2 Goal: Toast alerts on 80% / 100% → ✓ Tasks 3 (emission) + 6 (listener).
- §3 Non-goals: no per-workspace budgets, no blocking, no historical browsing, USD only, no CSV → ✓ none added.
- §4 Data model, no schema change → ✓ Tasks 2-3 use only V13 schema.
- §5 Backend: spec asks 3 new commands. Actual work is 2 new (`list_workspace_cost_rollup`, `get_month_cost_total`) — `get_model_costs` already exists in the codebase. **Documented in Background section.**
- §5 Threshold check in `emit_turn_cost` → ✓ Task 3 with crossing detection (fires once, not on every turn above threshold).
- §6 Settings shape → ✓ Task 1 with `Option<Option<f64>>` to disambiguate absent-vs-null per JSON Merge Patch semantics.
- §7 Frontend atoms → ✓ Task 4 + Task 5 (loadBudget/setBudget appended).
- §7 CostDashboard component → spec says new component; we **extend UsageSettings** instead because it already has Recharts plumbing. Inline `BudgetHeader` + `WorkspaceRollupSection`. **Documented in Background section.**
- §7 Toast listener in AppShell → ✓ Task 6.
- §7 Settings page wiring → ✓ Task 5 (label change).
- §8 Edge cases — all covered: no records → empty list; no budget → alerts disabled; budget mid-month change → re-evaluates on next turn; month rollover → known acceptable limitation (atom is stable through a session); dedup resets on restart; negative budget clamped at the IPC layer; small spends → 4-decimal formatting under $0.01.
- §9 Vitest coverage → ✓ atom helpers + threshold dedup.
- §9 Rust unit — fired_threshold logic (6 tests) + workspace rollup SQL (3 tests) + month total SQL (1 test) + settings (3 tests) = 13 Rust tests across the PR.
- §10 6-commit shape → ✓ Tasks 1-6 each commit once.

### Placeholder scan

No "TBD" / "implement later". Step 3.3 has a runtime note about `block_on` choice that depends on the dispatcher's existing pattern — that's a real engineering call, not a placeholder; the engineer needs to look at the file and pick the matching idiom.

### Type consistency

- Rust `monthly_budget_usd: Option<f64>` → TS `monthlyBudgetUsd?: number | null`. ✓
- Rust `BudgetThresholdPayload { threshold: u8, current: f64, budget: f64 }` → TS `BudgetThresholdPayload { threshold: 80 | 100, current: number, budget: number }`. The TS narrows `u8` to the two valid values — safe since Rust only emits 80 or 100.
- `WorkspaceCostRollup` shape consistent across both sides (workspaceId, workspaceName, workspaceIcon, totalCostUsd, totalTokens). ✓
- `refreshCostsAtom` writes both `monthTotalUsdAtom` and `workspaceRollupAtom` — used together in Task 5's `useEffect`. ✓
- `firedBudgetThresholdsAtom: Set<80 | 100>` — same literal-type set in both atom and listener. ✓

No issues found.
